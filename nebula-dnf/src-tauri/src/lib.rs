use regex::Regex;
use serde::{Serialize, Deserialize};
use tauri_plugin_shell::ShellExt;
use std::collections::HashSet;
use once_cell::sync::Lazy;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tauri::Manager; // Required for app.path()

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

const CACHE_FILE_NAME: &str = "package_cache.json";
const MAX_CONCURRENT_RPM_QUERIES: usize = 5; // Limit concurrent rpm processes

// --- Regex Definitions ---
// Regex for extracting base package name: captures name part before potential version string.
// Example: "pkg-name-1.2.3-4.arch" -> Group 1: "pkg-name"
// Example: "lib-example-1.0" -> Group 1: "lib-example"
// Example: "nameonly" -> Group 1: "nameonly"
// Example: "name-devel" (no version like -1.0) -> Group 1: "name-devel"
static NAME_EXTRACTOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^([a-zA-Z0-9][a-zA-Z0-9._+-]*?)(?:-([0-9].*))?$").unwrap()
});

// Regexes for parsing deplist output
static PACKAGE_LINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^package:\s*(.+)").unwrap());
static PROVIDER_LINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s+provider:\s*(.+)").unwrap());

// --- Struct Definitions ---
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct DisplayablePackage {
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)] // Added PartialEq, Eq, Hash for potential future use
pub enum PackageCategory {
    Manual,
    DesktopEnvironment,
    System,
    Library,
    Development,
    Multimedia,
    Office,
    Games,
    Utility,
    Network,
    Security,
    OtherApplication, // For other apps not fitting above
    Unknown,
}

impl Default for PackageCategory {
    fn default() -> Self {
        PackageCategory::Unknown
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserPackageWithDependencies {
    name: String,
    category: PackageCategory, // New field
    dependencies: Vec<DisplayablePackage>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageOperationResult {
    success: bool,
    message: String,      // User-facing summary. For dry run, this could be a preamble.
    details: Option<String>, // For verbose output like dry run text or full dnf output.
}

// Enum for different uninstall modes
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum UninstallMode {
    Safe,      // Actual removal: dnf remove <pkg> -y
    Force,     // Actual removal: rpm -e --nodeps <pkg>
    DryRunSafe,// dnf remove <pkg> --assumeno
    DryRunForce, // rpm -e --nodeps <pkg> --test
}

// Struct for uninstall arguments
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UninstallArgs {
    package_name: String,
    mode: UninstallMode,
    cleanup_orphans: bool, // Only relevant for Safe/DryRunSafe modes
}

// --- Helper Functions ---
// Helper function to extract base package name from a full NEVRA or similar string
fn extract_base_package_name(full_spec: &str) -> String {
    let trimmed_spec = full_spec.trim();
    // RPM requirements can be file paths or complex strings, try to simplify common ones.
    if trimmed_spec.starts_with('/') { // like /bin/sh
        if let Some(file_name) = std::path::Path::new(trimmed_spec).file_name().and_then(|n| n.to_str()) {
            return file_name.to_string();
        }
    }
    // Handle cases like "rpmlib(VersionedDependencies)" -> "rpmlib"
    if let Some(cap_idx) = trimmed_spec.find('(') {
        if !trimmed_spec.starts_with("perl(") { // perl(Foo::Bar) should be kept as is for uniqueness
             return trimmed_spec[..cap_idx].to_string();
        }
    }
    if let Some(caps) = NAME_EXTRACTOR_RE.captures(trimmed_spec) {
        if let Some(name) = caps.get(1) {
            return name.as_str().to_string();
        }
    }
    trimmed_spec.to_string()
}

// Renamed function from parse_requires_output to parse_rpm_requires_output
fn parse_rpm_requires_output(output: &str, main_pkg_base_name_for_context: &str) -> Vec<DisplayablePackage> {
    println!(
        "--- Parsing `rpm -qR` output for [{}] ---\n{}\n--- End `rpm -qR` output for [{}] ---",
        main_pkg_base_name_for_context, output, main_pkg_base_name_for_context
    );

    let mut deps = HashSet::new(); // Use HashSet to avoid duplicate deps

    for line in output.lines() {
        let dep_spec = line.trim();
        if !dep_spec.is_empty() && !dep_spec.starts_with("Last metadata expiration check:") {
            let dep_base_name = extract_base_package_name(dep_spec);
            println!(
                "  Found requirement spec: '{}', Extracted base name: '{}'",
                dep_spec,
                dep_base_name
            );
            // Avoid adding the package itself as its own dependency
            if dep_base_name != main_pkg_base_name_for_context {
                deps.insert(DisplayablePackage { name: dep_base_name });
            }
        }
    }
    let mut deps_vec: Vec<DisplayablePackage> = deps.into_iter().collect();
    deps_vec.sort_by(|a, b| a.name.cmp(&b.name));
    deps_vec
}

fn get_cache_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_local_data_dir()
        .map(|p| p.join(CACHE_FILE_NAME))
        .map_err(|e| format!("Failed to get app local data directory path: {}", e))
}

fn load_cache(app: &tauri::AppHandle) -> Result<Option<Vec<UserPackageWithDependencies>>, String> {
    let cache_path = get_cache_path(app)?;
    if cache_path.exists() {
        let mut file = File::open(cache_path).map_err(|e| format!("Failed to open cache file: {}", e))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(|e| format!("Failed to read cache file: {}", e))?;
        if contents.is_empty() {
             return Ok(None); // Cache file is empty
        }
        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to deserialize cache: {}. Cache file might be corrupted.", e))
            .map(Some)
    } else {
        Ok(None)
    }
}

fn save_cache(app: &tauri::AppHandle, data: &Vec<UserPackageWithDependencies>) -> Result<(), String> {
    let cache_path = get_cache_path(app)?;
    if let Some(parent_dir) = cache_path.parent() {
        fs::create_dir_all(parent_dir).map_err(|e| format!("Failed to create cache directory: {}", e))?;
    }
    let mut file = File::create(cache_path).map_err(|e| format!("Failed to create cache file: {}", e))?;
    let json_data = serde_json::to_string_pretty(data).map_err(|e| format!("Failed to serialize data: {}", e))?;
    file.write_all(json_data.as_bytes()).map_err(|e| format!("Failed to write to cache file: {}", e))
}

// Helper function to get package category based on RPM group
async fn get_package_category(shell: &tauri_plugin_shell::Shell<tauri::Wry>, package_name: &str) -> PackageCategory {
    let output_result = shell
        .command("rpm")
        .args(["-q", "--qf", "%{GROUP}\n", package_name])
        .output()
        .await;

    match output_result {
        Ok(output_val) => {
            if output_val.status.success() {
                let group_str = String::from_utf8_lossy(&output_val.stdout).trim().to_lowercase();
                // println!("Package: {}, RPM Group: '{}'", package_name, group_str);

                if group_str.is_empty() || group_str.contains("not installed") || group_str.contains("no such file") {
                    return PackageCategory::Unknown; // Package might have been removed or is a virtual package
                }

                // More specific checks first
                if group_str.contains("desktop environment") || group_str.contains("desktops") || group_str.contains("xfce") || group_str.contains("kde") || group_str.contains("gnome") {
                    return PackageCategory::DesktopEnvironment;
                }
                if group_str.starts_with("system environment/base") || group_str.starts_with("system environment/kernel") || group_str == "system environment" {
                    return PackageCategory::System;
                }
                if group_str.contains("games") {
                    return PackageCategory::Games;
                }
                if group_str.contains("multimedia") || group_str.contains("sound") || group_str.contains("video") {
                    return PackageCategory::Multimedia;
                }
                if group_str.contains("office") || group_str.contains("productivity") {
                    return PackageCategory::Office;
                }
                 if group_str.contains("network") || group_str.contains("web") || group_str.contains("mail") {
                    return PackageCategory::Network;
                }
                if group_str.contains("security") || group_str.contains("firewall") {
                    return PackageCategory::Security;
                }
                 // General application categories
                if group_str.starts_with("applications/") {
                    if group_str.contains("development") || group_str.contains("debugging") {
                        return PackageCategory::Development;
                    }
                    if group_str.contains("utilities") {
                        return PackageCategory::Utility;
                    }
                     // Catch-all for other things under "applications/"
                    return PackageCategory::OtherApplication; 
                }
                if group_str.starts_with("development/") {
                    return PackageCategory::Development;
                }
                // Libraries are often harder to distinguish from system components if not explicitly categorized
                if group_str.contains("libraries") || group_str.ends_with("lib") || group_str.contains("shared libraries") {
                    return PackageCategory::Library;
                }
                // If it's user installed but doesn't fit above, lean towards Manual or OtherApplication
                // For now, let's assume if it's in dnf userinstalled and not clearly system/DE/library, it was somewhat manual.
                // This is a heuristic and might need refinement.
                if !group_str.starts_with("system environment/") { // Avoid re-classifying things already potentially System
                    return PackageCategory::Manual; 
                }
                
                PackageCategory::Unknown
            } else {
                // e.g. package not found by rpm, or rpm command error
                // eprintln!("RPM query for group failed for {}: Status {}, Stderr: {}", 
                //     package_name, 
                //     output_val.status.code().unwrap_or(-1),
                //     String::from_utf8_lossy(&output_val.stderr).trim()
                // );
                PackageCategory::Unknown
            }
        }
        Err(_e) => {
            // eprintln!("Failed to execute rpm query for group of {}: {}", package_name, e);
            PackageCategory::Unknown
        }
    }
}

// --- Tauri Commands ---
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn list_installed_packages(app: tauri::AppHandle) -> Result<Vec<DisplayablePackage>, String> {
    println!("Attempting to list all installed packages using 'rpm -qa'.");
    let shell = app.shell();
    let output_result = shell
        .command("rpm")
        .args(["-qa"]) // Changed from dnf list installed
        .output()
        .await;

    match output_result {
        Ok(output_val) => {
            if output_val.status.success() {
                let stdout_str = String::from_utf8_lossy(&output_val.stdout);
                let unique_base_names: HashSet<String> = stdout_str
                    .lines()
                    // .skip(1) // Removed skip(1) as rpm -qa has no header
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(|line| {
                        // rpm -qa output is typically 'name-version-release.arch'
                        // extract_base_package_name can handle this
                        extract_base_package_name(line)
                    })
                    .filter(|name| !name.is_empty())
                    .collect();
                
                let mut packages: Vec<DisplayablePackage> = unique_base_names
                    .into_iter()
                    .map(|name| DisplayablePackage { name })
                    .collect();
                
                packages.sort_by(|a, b| a.name.cmp(&b.name));
                Ok(packages)
            } else {
                let stderr_str = String::from_utf8_lossy(&output_val.stderr);
                Err(format!(
                    "rpm -qa command failed with status {}: {}", // Correctly blames rpm -qa
                    output_val.status.code().unwrap_or(-1),
                    stderr_str
                ))
            }
        }
        Err(e) => Err(format!("Failed to execute rpm -qa command: {}", e)), // Correctly blames rpm -qa
    }
}

#[tauri::command]
async fn list_user_installed_packages(app: tauri::AppHandle, force_refresh: bool) -> Result<Vec<UserPackageWithDependencies>, String> {
    println!(
        "Attempting to list user-installed packages. Force refresh: {}",
        force_refresh
    );
    let cache_path = get_cache_path(&app)?;
    println!("Cache path: {:?}", cache_path);

    if !force_refresh {
        if let Some(cached_data) = load_cache(&app)? {
            println!("Returning cached user package data.");
            return Ok(cached_data);
        }
    }
    println!("Cache not used or refresh forced. Fetching fresh data...");

    let shell = app.shell();

    // Step 1: Get all actually installed packages (our source of truth for "is it installed?")
    let rpm_qa_output_result = shell
        .command("rpm")
        .args(["-qa", "--queryformat", "%{NAME}\n"]) // Get only base names
        .output()
        .await;

    let actually_installed_set: HashSet<String> = match rpm_qa_output_result {
        Ok(output) => {
            if output.status.success() {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            } else {
                return Err(format!(
                    "Failed to get `rpm -qa` list: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
        }
        Err(e) => return Err(format!("Shell command error for `rpm -qa`: {}", e)),
    };
    if actually_installed_set.is_empty() {
        println!("`rpm -qa` returned no packages. Assuming no user packages can be listed.");
         let empty_list = Vec::new();
        if let Err(e) = save_cache(&app, &empty_list) {
            eprintln!("Warning: Failed to save empty cache (rpm -qa was empty): {}", e);
        }
        return Ok(empty_list);
    }


    // Step 2: Get packages marked as user-installed by DNF
    let dnf_history_output_result = shell
        .command("dnf")
        .args([
            "repoquery",         // Changed from "history"
            "--userinstalled",   // Argument for repoquery
            "--quiet",
        ])
        .output()
        .await;

    let dnf_user_packages_list: Vec<String> = match dnf_history_output_result {
        Ok(output_val) => {
            if output_val.status.success() {
                String::from_utf8_lossy(&output_val.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with("Last metadata expiration check:"))
                    .map(String::from)
                    .collect()
            } else {
                return Err(format!(
                    "Failed to get user-installed packages list (dnf): {}",
                    String::from_utf8_lossy(&output_val.stderr)
                ));
            }
        }
        Err(e) => return Err(format!("Shell command error for user-installed packages list (dnf): {}", e)),
    };

    if dnf_user_packages_list.is_empty() {
        println!("`dnf repoquery userinstalled` returned no packages.");
        let empty_list = Vec::new();
        if let Err(e) = save_cache(&app, &empty_list) {
            eprintln!("Warning: Failed to save empty cache (dnf repoquery was empty): {}", e);
        }
        return Ok(empty_list);
    }

    // Step 3: Filter DNF's list against actually installed packages
    let mut packages_to_process = Vec::new();
    for pkg_name_from_dnf in dnf_user_packages_list {
        // dnf history userinstalled might give package.arch or just name.
        // rpm -qa --queryformat %{NAME} gives just the name.
        // We need to compare them. extract_base_package_name can help.
        let base_name_from_dnf = extract_base_package_name(&pkg_name_from_dnf);
        if actually_installed_set.contains(&base_name_from_dnf) {
            packages_to_process.push(base_name_from_dnf); // Add the base name for consistency
        } else {
            println!("Package '{}' (base: '{}') from DNF's userinstalled list is not in 'rpm -qa' output. Skipping.", pkg_name_from_dnf, base_name_from_dnf);
        }
    }
     // Deduplicate after base name extraction, as different arch/versions might resolve to same base name
    let unique_packages_to_process: Vec<String> = packages_to_process.into_iter().collect::<HashSet<_>>().into_iter().collect();


    if unique_packages_to_process.is_empty() {
        println!("No user-installed packages remain after cross-referencing with rpm -qa.");
        let empty_list = Vec::new();
         if let Err(e) = save_cache(&app, &empty_list) {
            eprintln!("Warning: Failed to save empty cache (no packages after filter): {}", e);
        }
        return Ok(empty_list);
    }


    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_RPM_QUERIES));
    let mut tasks = Vec::new();

    // Now process only the filtered and confirmed installed packages
    for package_name_str in unique_packages_to_process { // Iterate over the filtered list
        let app_clone = app.clone();
        let sem_clone = semaphore.clone();
        let task = tokio::spawn(async move {
            let _permit = sem_clone.acquire().await.unwrap();
            let shell_clone = app_clone.shell();

            let deps_output_result = shell_clone
                .command("rpm")
                .args(["-qR", &package_name_str])
                .output()
                .await;

            let dependencies = match deps_output_result {
                Ok(dep_val) => {
                    if dep_val.status.success() {
                        let dep_stdout_str = String::from_utf8_lossy(&dep_val.stdout);
                        parse_rpm_requires_output(&dep_stdout_str, &package_name_str)
                    } else {
                        // Log error but continue, package might not have deps or is virtual
                        // eprintln!(
                        //     "rpm -qR for {} failed: Status {}, Stderr: {}",
                        //     package_name_str, 
                        //     dep_val.status.code().unwrap_or(-1),
                        //     String::from_utf8_lossy(&dep_val.stderr).trim()
                        // );
                        Vec::new()
                    }
                }
                Err(_e) => {
                    // eprintln!("Failed to execute rpm -qR for {}: {}", package_name_str, e);
                    Vec::new()
                }
            };
            
            // Sort dependencies by name for consistent display
            let mut sorted_deps = dependencies;
            sorted_deps.sort_by(|a, b| a.name.cmp(&b.name));

            // Get category
            let category = get_package_category(&shell_clone, &package_name_str).await;

            UserPackageWithDependencies {
                name: package_name_str,
                dependencies: sorted_deps,
                category,
            }
        });
        tasks.push(task);
    }

    let mut user_packages_with_deps = Vec::new();
    for task in tasks {
        match task.await {
            Ok(pkg_with_deps) => user_packages_with_deps.push(pkg_with_deps),
            Err(e) => eprintln!("Task join error: {}", e), // Log error and continue
        }
    }
    
    // Sort the final list of packages by name before caching and returning
    user_packages_with_deps.sort_by(|a, b| a.name.cmp(&b.name));

    if let Err(e) = save_cache(&app, &user_packages_with_deps) {
        eprintln!("Warning: Failed to save updated cache: {}", e);
        // Depending on desired behavior, you might choose to return an error here
        // return Err(format!("Failed to save cache: {}", e));
    }
    Ok(user_packages_with_deps)
}

#[tauri::command]
async fn manage_package_update(app: tauri::AppHandle, package_name: String) -> Result<PackageOperationResult, String> {
    println!("Attempting to update package: {}", package_name);
    let shell = app.shell();

    // Command: pkexec dnf update <package_name> -y
    let output_result = shell
        .command("pkexec") // Use pkexec for privilege escalation
        .args(["dnf", "update", &package_name, "--assumeyes"])
        .output()
        .await;

    match output_result {
        Ok(output) => {
            let stdout_str = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr_str = String::from_utf8_lossy(&output.stderr).into_owned();
            let full_details = format!("STDOUT:\n{}\nSTDERR:\n{}", stdout_str, stderr_str);

            if output.status.success() {
                println!("Package '{}' updated successfully.", package_name);
                Ok(PackageOperationResult {
                    success: true,
                    message: format!("Package '{}' updated successfully.", package_name),
                    details: Some(full_details),
                })
            } else {
                let err_msg = format!(
                    "Failed to update package '{}'. Exit code: {}.\n{}",
                    package_name,
                    output.status.code().unwrap_or(-1),
                    if stderr_str.is_empty() { &stdout_str } else { &stderr_str }
                );
                eprintln!("{}", err_msg);
                Ok(PackageOperationResult {
                    success: false,
                    message: format!("Failed to update package '{}'.", package_name),
                    details: Some(full_details),
                })
            }
        }
        Err(e) => {
            let err_msg = format!("Error executing update command for '{}': {}", package_name, e);
            eprintln!("{}", err_msg);
            Err(err_msg)
        }
    }
}

#[tauri::command]
async fn execute_package_uninstall(app: tauri::AppHandle, args: UninstallArgs) -> Result<PackageOperationResult, String> {
    println!("Executing uninstall for package: {}, Mode: {:?}, Cleanup: {}", args.package_name, args.mode, args.cleanup_orphans);
    let shell = app.shell();
    let mut final_message = String::new();
    let mut final_details = String::new();
    let mut overall_success = true;

    let (cmd_name, cmd_args, _is_privileged) = match args.mode { // _is_privileged was unused
        UninstallMode::Safe => ("pkexec", vec!["dnf".to_string(), "remove".to_string(), args.package_name.clone(), "--assumeyes".to_string()], true),
        UninstallMode::Force => ("pkexec", vec!["rpm".to_string(), "-e".to_string(), "--nodeps".to_string(), args.package_name.clone()], true),
        UninstallMode::DryRunSafe => ("dnf", vec!["remove".to_string(), args.package_name.clone(), "--assumeno".to_string()], false),
        UninstallMode::DryRunForce => ("rpm", vec!["-e".to_string(), "--nodeps".to_string(), args.package_name.clone(), "--test".to_string()], false),
    };

    println!("Executing command: {} with args: {:?}", cmd_name, cmd_args);

    let output_result = shell
        .command(cmd_name)
        .args(&cmd_args)
        .output()
        .await;

    match output_result {
        Ok(output) => {
            let stdout_str = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr_str = String::from_utf8_lossy(&output.stderr).into_owned();
            let details_for_this_step = format!("STDOUT:\n{}\nSTDERR:\n{}", stdout_str, stderr_str);

            if output.status.success() {
                let success_msg = format!(
                    "{} operation for '{}' completed successfully.",
                    match args.mode {
                        UninstallMode::DryRunSafe | UninstallMode::DryRunForce => "Dry run",
                        _ => "Uninstall"
                    },
                    args.package_name
                );
                println!("{}", success_msg);
                final_message.push_str(&success_msg);
                final_details.push_str(&details_for_this_step);
                if matches!(args.mode, UninstallMode::DryRunSafe | UninstallMode::DryRunForce) {
                    final_details = stdout_str; // For dry run, stdout is usually the most relevant detail
                }
            } else {
                overall_success = false;
                let err_msg = format!(
                    "Failed {} for package '{}'. Exit code: {}.\nDetails:\n{}",
                    match args.mode {
                        UninstallMode::DryRunSafe | UninstallMode::DryRunForce => "dry run",
                        _ => "uninstall"
                    },
                    args.package_name,
                    output.status.code().unwrap_or(-1),
                    if stderr_str.is_empty() { &stdout_str } else { &stderr_str }
                );
                eprintln!("{}", err_msg);
                final_message.push_str(&format!(
                    "Failed {} for package '{}'.",
                     match args.mode {
                        UninstallMode::DryRunSafe | UninstallMode::DryRunForce => "dry run",
                        _ => "uninstall"
                    },
                    args.package_name
                ));
                final_details.push_str(&details_for_this_step);
            }
        }
        Err(e) => {
            overall_success = false;
            let err_msg = format!("Error executing command for '{}': {}", args.package_name, e);
            eprintln!("{}", err_msg);
            final_message = err_msg.clone();
            final_details = err_msg;
        }
    }

    // Handle cleanup_orphans for Safe mode after successful uninstall
    if overall_success && matches!(args.mode, UninstallMode::Safe) && args.cleanup_orphans {
        println!("Attempting to cleanup orphans after uninstalling '{}'", args.package_name);
        final_details.push_str("\n\n--- Autoremove (Orphans) ---\n");

        let autoremove_output_result = shell
            .command("pkexec")
            .args(["dnf", "autoremove", "--assumeyes"])
            .output()
            .await;

        match autoremove_output_result {
            Ok(output) => {
                let stdout_str = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr_str = String::from_utf8_lossy(&output.stderr).into_owned();
                let autoremove_details = format!("STDOUT:\n{}\nSTDERR:\n{}", stdout_str, stderr_str);
                final_details.push_str(&autoremove_details);

                if output.status.success() {
                    println!("Orphan cleanup successful.");
                    final_message.push_str("\nOrphan cleanup successful.");
                } else {
                    overall_success = false; // Mark overall as failed if autoremove fails
                    let err_msg = format!(
                        "Orphan cleanup failed after uninstalling '{}'. Exit code: {}.\n{}",
                        args.package_name,
                        output.status.code().unwrap_or(-1),
                        if stderr_str.is_empty() { &stdout_str } else { &stderr_str }
                    );
                    eprintln!("{}", err_msg);
                    final_message.push_str("\nOrphan cleanup failed.");
                }
            }
            Err(e) => {
                overall_success = false;
                let err_msg = format!("Error executing dnf autoremove: {}", e);
                eprintln!("{}", err_msg);
                final_message.push_str(&format!("\nError during orphan cleanup: {}", e));
                final_details.push_str(&format!("\nError during orphan cleanup: {}", e));
            }
        }
    }

    // After all operations, including potential autoremove
    if overall_success && !matches!(args.mode, UninstallMode::DryRunSafe | UninstallMode::DryRunForce) {
        println!("Uninstall successful, attempting to clear package cache.");
        final_message.push_str(&format!("
Uninstall of {} successful.", args.package_name)); // Add confirmation to user message
        match get_cache_path(&app) {
            Ok(cache_path) => {
                if cache_path.exists() {
                    if let Err(e) = fs::remove_file(&cache_path) {
                        let cache_err_msg = format!("
Warning: Failed to delete package cache file at {:?}: {}", cache_path, e);
                        eprintln!("{}", cache_err_msg);
                        final_message.push_str(&cache_err_msg);
                        // Don't make the whole operation fail for this, but log it.
                    } else {
                        println!("Successfully deleted package cache file.");
                        final_message.push_str("
Package cache cleared for next refresh.");
                    }
                } else {
                    println!("Package cache file not found, no deletion needed.");
                     final_message.push_str("
Package cache was not present.");
                }
            }
            Err(e) => {
                let cache_path_err_msg = format!("
Warning: Failed to get cache path for deletion: {}", e);
                eprintln!("{}", cache_path_err_msg);
                final_message.push_str(&cache_path_err_msg);
            }
        }
    }

    Ok(PackageOperationResult {
        success: overall_success,
        message: final_message.trim().to_string(), // Trim leading/trailing newlines
        details: Some(final_details),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|_app| { 
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet, 
            list_installed_packages, 
            list_user_installed_packages,
            manage_package_update,
            execute_package_uninstall
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Test for extract_base_package_name
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_name() {
        assert_eq!(extract_base_package_name("package-name-1.2.3-4.fc36.x86_64"), "package-name");
        assert_eq!(extract_base_package_name("package-name-1.2.3"), "package-name");
        assert_eq!(extract_base_package_name("package-name"), "package-name");
        assert_eq!(extract_base_package_name("package-name.x86_64"), "package-name"); // Added test for arch
        assert_eq!(extract_base_package_name("libX11-1.2.3-4.fc36.x86_64"), "libX11");
        assert_eq!(extract_base_package_name("python3-foobar-0.1.1-11.fc39.noarch"), "python3-foobar");
        assert_eq!(extract_base_package_name("my-package-devel-1.0-1.noarch"), "my-package-devel");
        assert_eq!(extract_base_package_name("package-1:1.0-1"), "package"); // Epoch
        assert_eq!(extract_base_package_name("perl(Some::Module)"), "perl(Some::Module)"); // Should be kept
        assert_eq!(extract_base_package_name("rpmlib(VersionedDependencies)"), "rpmlib");
        assert_eq!(extract_base_package_name("/usr/bin/bash"), "bash");
        assert_eq!(extract_base_package_name("libcrypto.so.1.1()(64bit)"), "libcrypto.so.1.1");
        assert_eq!(extract_base_package_name("A spezielle.package-1.0"), "A spezielle.package");


    }
     #[test]
    fn test_parse_rpm_deps() {
        let rpm_output = "rpmlib(CompressedFileNames) <= 3.0.4-1\n\
        rpmlib(FileDigests) <= 4.6.0-1\n\
        rpmlib(PayloadFilesHavePrefix) <= 4.0-1\n\
        rpmlib(PayloadIsXz) <= 5.2-1\n\
        libc.so.6()(64bit)\n\
        libm.so.6()(64bit)\n\
        libz.so.1()(64bit)\n\
        my-own-package-dep\n\
        /usr/bin/perl\n\
        perl(strict)\n\
        perl(warnings)";
        let deps = parse_rpm_requires_output(rpm_output, "my-main-package");
        assert!(deps.contains(&DisplayablePackage { name: "rpmlib".to_string() }));
        assert!(deps.contains(&DisplayablePackage { name: "libc.so.6".to_string() }));
        assert!(deps.contains(&DisplayablePackage { name: "my-own-package-dep".to_string() }));
        assert!(deps.contains(&DisplayablePackage { name: "perl".to_string() })); // from /usr/bin/perl
        assert!(deps.contains(&DisplayablePackage { name: "perl(strict)".to_string() })); // full perl module name
    }
}
