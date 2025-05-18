use regex::Regex;
use serde::Serialize;
use tauri_plugin_shell::ShellExt;
use std::collections::{HashMap, HashSet};
use once_cell::sync::Lazy;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

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
#[derive(Debug, Serialize, Clone, PartialEq, Eq, Hash)] // Added PartialEq, Eq, Hash for HashSet
struct DisplayablePackage {
    name: String,
}

#[derive(Debug, Serialize)]
struct UserPackageWithDependencies {
    name: String,
    dependencies: Vec<DisplayablePackage>,
}

// --- Helper Functions ---
// Helper function to extract base package name from a full NEVRA or similar string
fn extract_base_package_name(full_spec: &str) -> String {
    let trimmed_spec = full_spec.trim();
    if let Some(caps) = NAME_EXTRACTOR_RE.captures(trimmed_spec) {
        if let Some(name) = caps.get(1) {
            return name.as_str().to_string();
        }
    }
    // Fallback: if regex doesn't match, return the trimmed input.
    trimmed_spec.to_string()
}

// Parses the output of `dnf deplist pkg1 pkg2 ...`
fn parse_multiple_deplist_output(output: &str) -> HashMap<String, Vec<DisplayablePackage>> {
    let mut current_main_pkg_base_name = String::new();
    let mut dependencies_map_internal: HashMap<String, HashSet<DisplayablePackage>> = HashMap::new();

    for line in output.lines() {
        if let Some(caps) = PACKAGE_LINE_RE.captures(line) {
            if let Some(main_pkg_full_spec_match) = caps.get(1) {
                let main_pkg_full_spec = main_pkg_full_spec_match.as_str();
                current_main_pkg_base_name = extract_base_package_name(main_pkg_full_spec);
                dependencies_map_internal.entry(current_main_pkg_base_name.clone()).or_default();
            }
        } else if let Some(caps) = PROVIDER_LINE_RE.captures(line) {
            if !current_main_pkg_base_name.is_empty() {
                if let Some(provider_full_spec_match) = caps.get(1) {
                    let provider_full_spec = provider_full_spec_match.as_str();
                    let dep_base_name = extract_base_package_name(provider_full_spec);
                    
                    if !dep_base_name.is_empty() && dep_base_name != current_main_pkg_base_name {
                        dependencies_map_internal
                            .entry(current_main_pkg_base_name.clone())
                            .or_default()
                            .insert(DisplayablePackage { name: dep_base_name });
                    }
                }
            }
        }
    }

    let mut final_dependencies_map: HashMap<String, Vec<DisplayablePackage>> = HashMap::new();
    for (main_pkg_name, dep_set) in dependencies_map_internal {
        let mut deps_vec: Vec<DisplayablePackage> = dep_set.into_iter().collect();
        deps_vec.sort_by(|a, b| a.name.cmp(&b.name));
        final_dependencies_map.insert(main_pkg_name, deps_vec);
    }
    final_dependencies_map
}

// --- Tauri Commands ---
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
async fn list_installed_packages(app: tauri::AppHandle) -> Result<Vec<DisplayablePackage>, String> {
    let shell = app.shell();
    let output = shell
        .command("dnf")
        .args(["repoquery", "--installed", "--quiet", "--latest-limit=1"]) // --latest-limit=1 for robustness
        .output()
        .await;

    match output {
        Ok(output_val) => {
            if output_val.status.success() {
                let stdout_str = String::from_utf8_lossy(&output_val.stdout);
                // Use HashSet to ensure unique base names, then convert to Vec<DisplayablePackage>
                let unique_base_names: HashSet<String> = stdout_str
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.contains("Last metadata expiration check"))
                    .map(|line| extract_base_package_name(line))
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
                    "dnf repoquery --installed command failed with status {}: {}",
                    output_val.status.code().unwrap_or(-1),
                    stderr_str
                ))
            }
        }
        Err(e) => Err(format!("Failed to execute dnf repoquery --installed command: {}", e)),
    }
}

#[tauri::command]
async fn list_user_installed_packages(app: tauri::AppHandle) -> Result<Vec<UserPackageWithDependencies>, String> {
    let shell = app.shell();
    
    // 1. Get user-installed packages (NEVRAs)
    let user_pkgs_output_result = shell
        .command("dnf")
        .args(["repoquery", "--userinstalled", "--quiet", "--latest-limit=1"])
        .output()
        .await;

    let user_installed_neavras: Vec<String> = match user_pkgs_output_result {
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
                    "Failed to get user-installed packages list: {}",
                    String::from_utf8_lossy(&output_val.stderr)
                ));
            }
        }
        Err(e) => return Err(format!("Shell command error for user-installed packages list: {}", e)),
    };

    if user_installed_neavras.is_empty() {
        return Ok(Vec::new());
    }

    // 2. For all user-installed packages, get their dependencies in one go
    let deplist_args: Vec<&str> = user_installed_neavras.iter().map(AsRef::as_ref).collect();
    
    // Construct arguments for dnf deplist: ["deplist", pkg1, pkg2, ...]
    let mut command_args = vec!["deplist"];
    command_args.extend(deplist_args);

    let dep_output_result = shell
        .command("dnf")
        .args(command_args) // Pass combined args
        .output()
        .await;
    
    let mut parsed_deps_map: HashMap<String, Vec<DisplayablePackage>> = HashMap::new();

    match dep_output_result {
        Ok(output_val) => {
            if output_val.status.success() {
                let stdout_str = String::from_utf8_lossy(&output_val.stdout);
                parsed_deps_map = parse_multiple_deplist_output(&stdout_str);
            } else {
                eprintln!(
                    "Global dnf deplist command failed: {}",
                    String::from_utf8_lossy(&output_val.stderr)
                );
                // Proceed with empty dependencies map
            }
        }
        Err(e) => {
            eprintln!("Shell command error for global deplist: {}", e);
            // Proceed with empty dependencies map
        }
    }

    // 3. Construct the final list
    let mut result_list = Vec::<UserPackageWithDependencies>::new();
    for nevra_str in user_installed_neavras {
        let base_name = extract_base_package_name(&nevra_str);
        let dependencies = parsed_deps_map.get(&base_name).cloned().unwrap_or_else(Vec::new);
        
        result_list.push(UserPackageWithDependencies {
            name: base_name,
            dependencies,
        });
    }
    
    result_list.sort_by(|a, b| a.name.cmp(&b.name)); // Sort final list by package name
    Ok(result_list)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![greet, list_installed_packages, list_user_installed_packages])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
