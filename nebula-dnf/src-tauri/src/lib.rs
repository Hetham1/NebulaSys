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

#[derive(Debug, Serialize, Deserialize, Clone)] // Added Deserialize and Clone
struct UserPackageWithDependencies {
    name: String,
    dependencies: Vec<DisplayablePackage>,
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

// --- Tauri Commands ---
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn list_installed_packages(app: tauri::AppHandle) -> Result<Vec<DisplayablePackage>, String> {
    let shell = app.shell();
    // Use rpm -qa for speed, as it directly queries the RPM DB.
    // --qf '%{NAME}\n' formats output to be one package name per line.
    let output = shell
        .command("rpm")
        .args(["-qa", "--qf", "%{NAME}\n"])
        .output()
        .await;

    match output {
        Ok(output_val) => {
            if output_val.status.success() {
                let stdout_str = String::from_utf8_lossy(&output_val.stdout);
                let unique_base_names: HashSet<String> = stdout_str
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(|line| extract_base_package_name(line)) // Though rpm -qa --qf %{NAME} should give base names
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
                    "rpm -qa command failed with status {}: {}",
                    output_val.status.code().unwrap_or(-1),
                    stderr_str
                ))
            }
        }
        Err(e) => Err(format!("Failed to execute rpm -qa command: {}", e)),
    }
}

#[tauri::command]
async fn list_user_installed_packages(app: tauri::AppHandle, force_refresh: bool) -> Result<Vec<UserPackageWithDependencies>, String> {
    if !force_refresh {
        if let Ok(Some(cached_data)) = load_cache(&app) {
            if !cached_data.is_empty() { // Ensure cache wasn't just an empty array
                println!("Loaded user packages from cache.");
                return Ok(cached_data);
            }
        }
    }
    println!("Fetching user packages from system (force_refresh: {})...", force_refresh);

    let shell = app.shell();
    let user_pkgs_output_result = shell
        .command("dnf")
        .args(["repoquery", "--userinstalled", "--quiet", "--latest-limit=1"]) // Get NEVRAs
        .output()
        .await;

    let user_installed_neavras_or_names: Vec<String> = match user_pkgs_output_result {
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

    if user_installed_neavras_or_names.is_empty() {
        let empty_list = Vec::new();
        save_cache(&app, &empty_list)?; // Save empty list to cache
        return Ok(empty_list);
    }

    let semaphore: Arc<tokio::sync::Semaphore> = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_RPM_QUERIES));
    let mut tasks = Vec::new();

    for nevra_or_name_str in user_installed_neavras_or_names {
        let app_handle_clone = app.clone();
        let sem_clone = Arc::clone(&semaphore);
        
        let task = tauri::async_runtime::spawn(async move {
            let _permit = sem_clone.acquire().await.expect("Semaphore acquire failed"); // Or handle error
            let shell_for_task = app_handle_clone.shell();
            
            // extract_base_package_name is crucial here as rpm -qR needs a package name, not NEVRA.
            let base_name = extract_base_package_name(&nevra_or_name_str);
            // println!(
            //     "Fetching dependencies for: {} (original: {}) using rpm -qR",
            //     base_name, nevra_or_name_str
            // );

            // Use `rpm -qR <package_name>` or `rpm -q --requires <package_name>`
            // `-qR` is shorthand for `query --requires`
            let dep_output_result = shell_for_task
                .command("rpm")
                .args(["-qR", &base_name]) // Use base_name
                .output()
                .await;

            let mut dependencies: Vec<DisplayablePackage> = Vec::new();
            match dep_output_result {
                Ok(output_val) => {
                    let stdout_str = String::from_utf8_lossy(&output_val.stdout);
                    if output_val.status.success() {
                        dependencies = parse_rpm_requires_output(&stdout_str, &base_name);
                    } else {
                        let stderr_str = String::from_utf8_lossy(&output_val.stderr);
                        if !stderr_str.contains("is not installed") && !stdout_str.is_empty() && stdout_str.trim() != "(none)" {
                             eprintln!(
                                "rpm -qR for {} (original: {}) failed or gave non-success: {:?}. Stderr: {}. Stdout: {}",
                                base_name, nevra_or_name_str, output_val.status, stderr_str, stdout_str
                            );
                        } else if stdout_str.trim() == "(none)" {
                            // println!("Package {} has no explicit dependencies listed by rpm -qR.", base_name);
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Shell command error for rpm -qR for {} (original: {}): {}. Proceeding with empty deps.",
                        base_name, nevra_or_name_str, e
                    );
                }
            }
            // Drop permit explicitly when task scope ends (or if using older Tokio, do it in a block)
            // With modern Tokio, permit is dropped when _permit goes out of scope.
            Ok::<_, String>(UserPackageWithDependencies { name: base_name, dependencies })
        });
        tasks.push(task);
    }

    let mut result_list = Vec::<UserPackageWithDependencies>::new();
    for task_handle in tasks {
        match task_handle.await {
            Ok(Ok(package_with_deps)) => {
                result_list.push(package_with_deps);
            }
            Ok(Err(e)) => {
                eprintln!("A package dependency task resulted in an error: {}", e); // Error from our String Err
            }
            Err(join_err) => {
                eprintln!("A concurrent task failed to complete (JoinError): {}", join_err); // Tokio task panic
            }
        }
    }
    
    result_list.sort_by(|a, b| a.name.cmp(&b.name));
    if let Err(e) = save_cache(&app, &result_list) {
        eprintln!("Failed to save package data to cache: {}", e);
        // Don't return error to client for cache save failure, just log it.
        // The data is still available for the current session.
    } else {
        println!("Successfully saved user packages to cache.");
    }
    Ok(result_list)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|_app| { // Use _app to mark as unused
            // Optionally, you could trigger an initial cache population here if desired,
            // or ensure the directory exists. For now, lazy creation is fine.
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet, 
            list_installed_packages, 
            list_user_installed_packages
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
