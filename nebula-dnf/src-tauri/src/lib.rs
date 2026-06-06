use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

const CACHE_SCHEMA_VERSION: u8 = 2;
const CACHE_TTL_SECS: u64 = 300;

static PACKAGE_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._+:@-]{0,199}$").unwrap());
static NEVRA_NAME_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(.+)-[0-9][^-]*-.+$").unwrap());

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PackageManagerId {
    Dnf,
    Apt,
    Snap,
    Flatpak,
}

impl PackageManagerId {
    fn as_str(self) -> &'static str {
        match self {
            PackageManagerId::Dnf => "dnf",
            PackageManagerId::Apt => "apt",
            PackageManagerId::Snap => "snap",
            PackageManagerId::Flatpak => "flatpak",
        }
    }

    fn label(self) -> &'static str {
        match self {
            PackageManagerId::Dnf => "DNF",
            PackageManagerId::Apt => "APT",
            PackageManagerId::Snap => "Snap",
            PackageManagerId::Flatpak => "Flatpak",
        }
    }

    fn executable(self) -> &'static str {
        match self {
            PackageManagerId::Dnf => "dnf",
            PackageManagerId::Apt => "apt-get",
            PackageManagerId::Snap => "snap",
            PackageManagerId::Flatpak => "flatpak",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PackageView {
    User,
    All,
}

impl PackageView {
    fn as_str(self) -> &'static str {
        match self {
            PackageView::User => "user",
            PackageView::All => "all",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PackageOperation {
    Update,
    Uninstall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManagerStatus {
    id: PackageManagerId,
    label: String,
    installed: bool,
    executable: String,
    version: Option<String>,
    notes: String,
    supports_user_installed: bool,
    supports_dependencies: bool,
    supports_update: bool,
    supports_uninstall: bool,
    supports_force_uninstall: bool,
    supports_cleanup_orphans: bool,
    requires_privilege: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageInfo {
    manager: PackageManagerId,
    name: String,
    display_name: String,
    version: Option<String>,
    category: String,
    summary: Option<String>,
    source: String,
    dependencies: Vec<DependencyInfo>,
    dependencies_loaded: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct DependencyInfo {
    name: String,
    kind: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageOperationArgs {
    manager: PackageManagerId,
    package_name: String,
    operation: PackageOperation,
    dry_run: bool,
    force: bool,
    cleanup_orphans: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackageOperationResult {
    success: bool,
    message: String,
    details: Option<String>,
    command: Option<String>,
    dry_run: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PackageCacheEntry {
    schema_version: u8,
    generated_at: u64,
    manager: PackageManagerId,
    view: PackageView,
    packages: Vec<PackageInfo>,
}

#[derive(Debug)]
struct CommandOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

fn current_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn executable_in_path(name: &str) -> Option<String> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

fn validate_package_name(package_name: &str) -> Result<(), String> {
    if PACKAGE_NAME_RE.is_match(package_name) {
        Ok(())
    } else {
        Err(format!(
            "Package name '{}' is not a supported package identifier.",
            package_name
        ))
    }
}

fn command_output(command: &str, args: Vec<String>) -> Result<CommandOutput, String> {
    let output = Command::new(command)
        .args(&args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| format!("Failed to execute '{}': {}", command, e))?;

    Ok(CommandOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

async fn run_command(command: &str, args: Vec<String>) -> Result<CommandOutput, String> {
    let command = command.to_string();
    tauri::async_runtime::spawn_blocking(move || command_output(&command, args))
        .await
        .map_err(|e| format!("Command task failed: {}", e))?
}

fn command_line(command: &str, args: &[String]) -> String {
    if args.is_empty() {
        command.to_string()
    } else {
        format!("{} {}", command, args.join(" "))
    }
}

fn command_details(command: &str, args: &[String], output: &CommandOutput) -> String {
    format!(
        "Command: {}\nExit code: {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
        command_line(command, args),
        output.code,
        output.stdout.trim(),
        output.stderr.trim()
    )
}

fn cache_path(app: &tauri::AppHandle, manager: PackageManagerId, view: PackageView) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|p| {
            p.join(format!(
                "package_cache_v{}_{}_{}.json",
                CACHE_SCHEMA_VERSION,
                manager.as_str(),
                view.as_str()
            ))
        })
        .map_err(|e| format!("Failed to get app local data directory: {}", e))
}

fn load_package_cache(
    app: &tauri::AppHandle,
    manager: PackageManagerId,
    view: PackageView,
) -> Result<Option<Vec<PackageInfo>>, String> {
    let path = cache_path(app, manager, view)?;
    if !path.exists() {
        return Ok(None);
    }

    let mut file = File::open(&path).map_err(|e| format!("Failed to open cache file: {}", e))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Failed to read cache file: {}", e))?;
    if contents.trim().is_empty() {
        return Ok(None);
    }

    let entry: PackageCacheEntry = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse package cache: {}", e))?;

    let fresh = current_unix_secs().saturating_sub(entry.generated_at) <= CACHE_TTL_SECS;
    if entry.schema_version == CACHE_SCHEMA_VERSION
        && entry.manager == manager
        && entry.view == view
        && fresh
    {
        Ok(Some(entry.packages))
    } else {
        Ok(None)
    }
}

fn save_package_cache(
    app: &tauri::AppHandle,
    manager: PackageManagerId,
    view: PackageView,
    packages: &[PackageInfo],
) -> Result<(), String> {
    let path = cache_path(app, manager, view)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;
    }

    let entry = PackageCacheEntry {
        schema_version: CACHE_SCHEMA_VERSION,
        generated_at: current_unix_secs(),
        manager,
        view,
        packages: packages.to_vec(),
    };

    let json = serde_json::to_string_pretty(&entry)
        .map_err(|e| format!("Failed to serialize package cache: {}", e))?;
    let mut file = File::create(path).map_err(|e| format!("Failed to create cache file: {}", e))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write cache file: {}", e))
}

fn clear_manager_cache(app: &tauri::AppHandle, manager: PackageManagerId) {
    let Ok(dir) = app.path().app_local_data_dir() else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let prefix = format!("package_cache_v{}_{}", CACHE_SCHEMA_VERSION, manager.as_str());
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if file_name.starts_with(&prefix) && file_name.ends_with(".json") {
            let _ = fs::remove_file(path);
        }
    }
}

fn manager_note(manager: PackageManagerId) -> &'static str {
    match manager {
        PackageManagerId::Dnf => "Fedora/RHEL packages from DNF and RPM.",
        PackageManagerId::Apt => "Debian/Ubuntu packages from APT and dpkg.",
        PackageManagerId::Snap => "Snap applications. Snap does not expose manual/dependency data like distro package managers.",
        PackageManagerId::Flatpak => "Flatpak applications. User and all views are equivalent for app listings.",
    }
}

async fn manager_version(manager: PackageManagerId) -> Option<String> {
    let command = manager.executable();
    let args = match manager {
        PackageManagerId::Dnf => vec!["--version".to_string()],
        PackageManagerId::Apt => vec!["--version".to_string()],
        PackageManagerId::Snap => vec!["version".to_string()],
        PackageManagerId::Flatpak => vec!["--version".to_string()],
    };

    let Ok(output) = run_command(command, args).await else {
        return None;
    };
    if output.code != 0 {
        return None;
    }
    output
        .stdout
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
}

fn manager_status_template(manager: PackageManagerId, installed: bool, version: Option<String>) -> ManagerStatus {
    ManagerStatus {
        id: manager,
        label: manager.label().to_string(),
        installed,
        executable: manager.executable().to_string(),
        version,
        notes: manager_note(manager).to_string(),
        supports_user_installed: !matches!(manager, PackageManagerId::Snap | PackageManagerId::Flatpak),
        supports_dependencies: !matches!(manager, PackageManagerId::Snap),
        supports_update: true,
        supports_uninstall: true,
        supports_force_uninstall: matches!(manager, PackageManagerId::Dnf | PackageManagerId::Apt),
        supports_cleanup_orphans: matches!(manager, PackageManagerId::Dnf | PackageManagerId::Apt),
        requires_privilege: !matches!(manager, PackageManagerId::Flatpak),
    }
}

fn normalize_category(category: &str) -> String {
    let trimmed = category.trim();
    if trimmed.is_empty() || trimmed == "(none)" {
        return "Uncategorized".to_string();
    }

    let lowered = trimmed.to_lowercase();
    if lowered.contains("desktop") {
        "Desktop Environment".to_string()
    } else if lowered.contains("admin") || lowered.contains("system") || lowered.contains("kernel") {
        "System".to_string()
    } else if lowered.contains("devel") || lowered.contains("programming") {
        "Development".to_string()
    } else if lowered.contains("libs") || lowered.contains("library") {
        "Library".to_string()
    } else if lowered.contains("net") || lowered.contains("web") || lowered.contains("mail") {
        "Network".to_string()
    } else if lowered.contains("sound") || lowered.contains("video") || lowered.contains("graphics") {
        "Multimedia".to_string()
    } else if lowered.contains("game") {
        "Games".to_string()
    } else if lowered.contains("security") {
        "Security".to_string()
    } else if lowered.contains("utility") || lowered.contains("utils") {
        "Utility".to_string()
    } else {
        trimmed
            .split('/')
            .next()
            .unwrap_or(trimmed)
            .replace('-', " ")
            .trim()
            .to_string()
    }
}

fn extract_package_name(candidate: &str) -> String {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    for arch in [".x86_64", ".aarch64", ".noarch", ".i686", ".armv7hl"] {
        if let Some(stripped) = trimmed.strip_suffix(arch) {
            if !stripped.contains('-') {
                return stripped.to_string();
            }
        }
    }

    if let Some(caps) = NEVRA_NAME_RE.captures(trimmed) {
        return caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| trimmed.to_string());
    }

    trimmed.to_string()
}

fn split_tsv(line: &str) -> Vec<&str> {
    line.split('\t').map(str::trim).collect()
}

async fn list_dnf_packages(view: PackageView) -> Result<Vec<PackageInfo>, String> {
    let rpm_args = vec![
        "-qa".to_string(),
        "--queryformat".to_string(),
        "%{NAME}\t%{VERSION}-%{RELEASE}.%{ARCH}\t%{SUMMARY}\t%{GROUP}\n".to_string(),
    ];
    let rpm_output = run_command("rpm", rpm_args).await?;
    if rpm_output.code != 0 {
        return Err(format!("rpm package query failed: {}", rpm_output.stderr.trim()));
    }

    let mut packages = Vec::new();
    for line in rpm_output.stdout.lines().filter(|line| !line.trim().is_empty()) {
        let fields = split_tsv(line);
        let name = fields.get(0).copied().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }

        packages.push(PackageInfo {
            manager: PackageManagerId::Dnf,
            display_name: name.clone(),
            name,
            version: fields
                .get(1)
                .map(|v| v.to_string())
                .filter(|v| !v.is_empty()),
            summary: fields
                .get(2)
                .map(|v| v.to_string())
                .filter(|v| !v.is_empty() && v.as_str() != "(none)"),
            category: normalize_category(fields.get(3).copied().unwrap_or_default()),
            source: "rpmdb".to_string(),
            dependencies: Vec::new(),
            dependencies_loaded: false,
        });
    }

    if view == PackageView::User {
        let manual_args = vec![
            "repoquery".to_string(),
            "--userinstalled".to_string(),
            "--quiet".to_string(),
            "--queryformat".to_string(),
            "%{name}".to_string(),
        ];
        let manual_output = match run_command("dnf", manual_args).await {
            Ok(output) if output.code == 0 => output,
            _ => {
                let fallback_args = vec![
                    "repoquery".to_string(),
                    "--userinstalled".to_string(),
                    "--quiet".to_string(),
                ];
                let output = run_command("dnf", fallback_args).await?;
                if output.code != 0 {
                    return Err(format!(
                        "dnf user-installed query failed: {}",
                        output.stderr.trim()
                    ));
                }
                output
            }
        };

        let manual_set: HashSet<String> = manual_output
            .stdout
            .lines()
            .map(extract_package_name)
            .filter(|name| !name.is_empty())
            .collect();

        packages.retain(|package| manual_set.contains(&package.name));
        for package in &mut packages {
            package.source = "dnf manual".to_string();
        }
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

async fn list_apt_packages(view: PackageView) -> Result<Vec<PackageInfo>, String> {
    let query_args = vec![
        "-W".to_string(),
        "-f=${Package}\t${Version}\t${db:Status-Abbrev}\t${Section}\n".to_string(),
    ];
    let output = run_command("dpkg-query", query_args).await?;
    if output.code != 0 {
        return Err(format!("dpkg-query failed: {}", output.stderr.trim()));
    }

    let mut packages = Vec::new();
    for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
        let fields = split_tsv(line);
        let status = fields.get(2).copied().unwrap_or_default();
        if !status.starts_with("ii") {
            continue;
        }
        let name = fields.get(0).copied().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        packages.push(PackageInfo {
            manager: PackageManagerId::Apt,
            display_name: name.clone(),
            name,
            version: fields
                .get(1)
                .map(|v| v.to_string())
                .filter(|v| !v.is_empty()),
            category: normalize_category(fields.get(3).copied().unwrap_or_default()),
            summary: None,
            source: "dpkg".to_string(),
            dependencies: Vec::new(),
            dependencies_loaded: false,
        });
    }

    if view == PackageView::User {
        let manual_output = run_command("apt-mark", vec!["showmanual".to_string()]).await?;
        if manual_output.code != 0 {
            return Err(format!(
                "apt-mark showmanual failed: {}",
                manual_output.stderr.trim()
            ));
        }
        let manual_set: HashSet<String> = manual_output
            .stdout
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(String::from)
            .collect();
        packages.retain(|package| manual_set.contains(&package.name));
        for package in &mut packages {
            package.source = "apt manual".to_string();
        }
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

async fn list_snap_packages(_view: PackageView) -> Result<Vec<PackageInfo>, String> {
    let output = run_command("snap", vec!["list".to_string()]).await?;
    if output.code != 0 {
        return Err(format!("snap list failed: {}", output.stderr.trim()));
    }

    let mut packages = Vec::new();
    for (index, line) in output.stdout.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let name = fields.get(0).copied().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        packages.push(PackageInfo {
            manager: PackageManagerId::Snap,
            display_name: name.clone(),
            name,
            version: fields.get(1).map(|v| v.to_string()).filter(|v| !v.is_empty()),
            category: "Snap".to_string(),
            summary: None,
            source: "snap".to_string(),
            dependencies: Vec::new(),
            dependencies_loaded: true,
        });
    }

    packages.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packages)
}

async fn list_flatpak_packages(_view: PackageView) -> Result<Vec<PackageInfo>, String> {
    let args = vec![
        "list".to_string(),
        "--app".to_string(),
        "--columns=application,name,version,origin".to_string(),
    ];
    let output = run_command("flatpak", args).await?;
    if output.code != 0 {
        return Err(format!("flatpak list failed: {}", output.stderr.trim()));
    }

    let mut packages = Vec::new();
    for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
        let fields = split_tsv(line);
        let app_id = fields.get(0).copied().unwrap_or_default().to_string();
        if app_id.is_empty() {
            continue;
        }
        let display_name = fields
            .get(1)
            .map(|v| v.to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| app_id.clone());
        packages.push(PackageInfo {
            manager: PackageManagerId::Flatpak,
            name: app_id,
            display_name,
            version: fields
                .get(2)
                .map(|v| v.to_string())
                .filter(|v| !v.is_empty()),
            category: "Flatpak App".to_string(),
            summary: fields
                .get(3)
                .map(|origin| format!("Origin: {}", origin))
                .filter(|v| !v.ends_with(": ")),
            source: "flatpak".to_string(),
            dependencies: Vec::new(),
            dependencies_loaded: false,
        });
    }

    packages.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(packages)
}

fn parse_requirement_name(line: &str) -> String {
    let mut value = line.trim().to_string();
    for marker in [" >= ", " <= ", " = ", " > ", " < "] {
        if let Some((name, _)) = value.split_once(marker) {
            value = name.trim().to_string();
            break;
        }
    }
    if let Some((name, _)) = value.split_once('(') {
        if !value.starts_with("perl(") {
            return name.trim().to_string();
        }
    }
    value
}

fn parse_apt_dependency_line(line: &str) -> Option<DependencyInfo> {
    let trimmed = line.trim();
    let (kind, rest) = trimmed.split_once(':')?;
    let kind = kind.trim();
    if !matches!(kind, "PreDepends" | "Depends" | "Recommends" | "Suggests") {
        return None;
    }
    let mut name = rest.trim();
    if name.starts_with('<') && name.ends_with('>') {
        name = name.trim_matches(['<', '>'].as_ref());
    }
    if let Some((base, _version)) = name.split_once(' ') {
        name = base;
    }
    if name.is_empty() {
        None
    } else {
        Some(DependencyInfo {
            name: name.to_string(),
            kind: kind.to_string(),
        })
    }
}

async fn dependencies_for_package(
    manager: PackageManagerId,
    package_name: &str,
) -> Result<Vec<DependencyInfo>, String> {
    validate_package_name(package_name)?;

    let mut dependencies = HashSet::new();

    match manager {
        PackageManagerId::Dnf => {
            let output = run_command("rpm", vec!["-qR".to_string(), package_name.to_string()]).await?;
            if output.code != 0 {
                return Err(format!("rpm requirement query failed: {}", output.stderr.trim()));
            }
            for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
                let name = parse_requirement_name(line);
                if !name.is_empty() && name != package_name {
                    dependencies.insert(DependencyInfo {
                        name,
                        kind: "Requirement".to_string(),
                    });
                }
            }
        }
        PackageManagerId::Apt => {
            let output = run_command(
                "apt-cache",
                vec![
                    "depends".to_string(),
                    "--installed".to_string(),
                    package_name.to_string(),
                ],
            )
            .await?;
            if output.code != 0 {
                return Err(format!("apt-cache dependency query failed: {}", output.stderr.trim()));
            }
            for line in output.stdout.lines() {
                if let Some(dep) = parse_apt_dependency_line(line) {
                    if dep.name != package_name {
                        dependencies.insert(dep);
                    }
                }
            }
        }
        PackageManagerId::Snap => {}
        PackageManagerId::Flatpak => {
            let output = run_command("flatpak", vec!["info".to_string(), package_name.to_string()]).await?;
            if output.code != 0 {
                return Err(format!("flatpak info failed: {}", output.stderr.trim()));
            }
            for line in output.stdout.lines() {
                let trimmed = line.trim();
                for prefix in ["Runtime:", "Sdk:"] {
                    if let Some(value) = trimmed.strip_prefix(prefix) {
                        let name = value.trim();
                        if !name.is_empty() {
                            dependencies.insert(DependencyInfo {
                                name: name.to_string(),
                                kind: prefix.trim_end_matches(':').to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<DependencyInfo> = dependencies.into_iter().collect();
    result.sort_by(|a, b| a.name.cmp(&b.name).then(a.kind.cmp(&b.kind)));
    Ok(result)
}

async fn package_is_installed(manager: PackageManagerId, package_name: &str) -> Result<bool, String> {
    validate_package_name(package_name)?;
    let (command, args) = match manager {
        PackageManagerId::Dnf => ("rpm", vec!["-q".to_string(), package_name.to_string()]),
        PackageManagerId::Apt => (
            "dpkg-query",
            vec![
                "-W".to_string(),
                "-f=${Status}".to_string(),
                package_name.to_string(),
            ],
        ),
        PackageManagerId::Snap => ("snap", vec!["list".to_string(), package_name.to_string()]),
        PackageManagerId::Flatpak => ("flatpak", vec!["info".to_string(), package_name.to_string()]),
    };

    let output = run_command(command, args).await?;
    if output.code != 0 {
        return Ok(false);
    }
    if manager == PackageManagerId::Apt {
        Ok(output.stdout.contains("install ok installed"))
    } else {
        Ok(true)
    }
}

fn unsupported_preview_result(manager: PackageManagerId, operation: PackageOperation) -> PackageOperationResult {
    let operation_label = match operation {
        PackageOperation::Update => "update",
        PackageOperation::Uninstall => "uninstall",
    };
    PackageOperationResult {
        success: true,
        message: format!(
            "{} does not expose a reliable dry-run command for {}. No changes were made.",
            manager.label(),
            operation_label
        ),
        details: Some("This manager requires an actual command for this operation.".to_string()),
        command: None,
        dry_run: true,
    }
}

fn operation_command(args: &PackageOperationArgs) -> Result<(String, Vec<String>), String> {
    if args.dry_run {
        return match (args.manager, args.operation, args.force) {
            (PackageManagerId::Dnf, PackageOperation::Update, _) => Ok((
                "dnf".to_string(),
                vec![
                    "upgrade".to_string(),
                    args.package_name.clone(),
                    "--assumeno".to_string(),
                ],
            )),
            (PackageManagerId::Dnf, PackageOperation::Uninstall, false) => Ok((
                "dnf".to_string(),
                vec![
                    "remove".to_string(),
                    args.package_name.clone(),
                    "--assumeno".to_string(),
                ],
            )),
            (PackageManagerId::Dnf, PackageOperation::Uninstall, true) => Ok((
                "rpm".to_string(),
                vec![
                    "-e".to_string(),
                    "--nodeps".to_string(),
                    "--test".to_string(),
                    args.package_name.clone(),
                ],
            )),
            (PackageManagerId::Apt, PackageOperation::Update, _) => Ok((
                "apt-get".to_string(),
                vec![
                    "-s".to_string(),
                    "install".to_string(),
                    "--only-upgrade".to_string(),
                    args.package_name.clone(),
                ],
            )),
            (PackageManagerId::Apt, PackageOperation::Uninstall, false) => Ok((
                "apt-get".to_string(),
                vec!["-s".to_string(), "remove".to_string(), args.package_name.clone()],
            )),
            (PackageManagerId::Apt, PackageOperation::Uninstall, true) => Ok((
                "dpkg".to_string(),
                vec![
                    "--remove".to_string(),
                    "--force-depends".to_string(),
                    "--dry-run".to_string(),
                    args.package_name.clone(),
                ],
            )),
            (PackageManagerId::Snap, _, _) => Err("unsupported-dry-run".to_string()),
            (PackageManagerId::Flatpak, PackageOperation::Update, _) => Ok((
                "flatpak".to_string(),
                vec![
                    "update".to_string(),
                    "--dry-run".to_string(),
                    args.package_name.clone(),
                ],
            )),
            (PackageManagerId::Flatpak, PackageOperation::Uninstall, _) => Ok((
                "flatpak".to_string(),
                vec![
                    "uninstall".to_string(),
                    "--dry-run".to_string(),
                    args.package_name.clone(),
                ],
            )),
        };
    }

    if matches!(args.manager, PackageManagerId::Dnf | PackageManagerId::Apt | PackageManagerId::Snap)
        && executable_in_path("pkexec").is_none()
    {
        return Err("pkexec is required for privileged package operations but was not found.".to_string());
    }

    match (args.manager, args.operation, args.force) {
        (PackageManagerId::Dnf, PackageOperation::Update, _) => Ok((
            "pkexec".to_string(),
            vec![
                "dnf".to_string(),
                "upgrade".to_string(),
                args.package_name.clone(),
                "--assumeyes".to_string(),
            ],
        )),
        (PackageManagerId::Dnf, PackageOperation::Uninstall, false) => Ok((
            "pkexec".to_string(),
            vec![
                "dnf".to_string(),
                "remove".to_string(),
                args.package_name.clone(),
                "--assumeyes".to_string(),
            ],
        )),
        (PackageManagerId::Dnf, PackageOperation::Uninstall, true) => Ok((
            "pkexec".to_string(),
            vec![
                "rpm".to_string(),
                "-e".to_string(),
                "--nodeps".to_string(),
                args.package_name.clone(),
            ],
        )),
        (PackageManagerId::Apt, PackageOperation::Update, _) => Ok((
            "pkexec".to_string(),
            vec![
                "apt-get".to_string(),
                "install".to_string(),
                "--only-upgrade".to_string(),
                "-y".to_string(),
                args.package_name.clone(),
            ],
        )),
        (PackageManagerId::Apt, PackageOperation::Uninstall, false) => Ok((
            "pkexec".to_string(),
            vec![
                "apt-get".to_string(),
                "remove".to_string(),
                "-y".to_string(),
                args.package_name.clone(),
            ],
        )),
        (PackageManagerId::Apt, PackageOperation::Uninstall, true) => Ok((
            "pkexec".to_string(),
            vec![
                "dpkg".to_string(),
                "--remove".to_string(),
                "--force-depends".to_string(),
                args.package_name.clone(),
            ],
        )),
        (PackageManagerId::Snap, PackageOperation::Update, _) => Ok((
            "pkexec".to_string(),
            vec!["snap".to_string(), "refresh".to_string(), args.package_name.clone()],
        )),
        (PackageManagerId::Snap, PackageOperation::Uninstall, _) => Ok((
            "pkexec".to_string(),
            vec!["snap".to_string(), "remove".to_string(), args.package_name.clone()],
        )),
        (PackageManagerId::Flatpak, PackageOperation::Update, _) => Ok((
            "flatpak".to_string(),
            vec!["update".to_string(), "-y".to_string(), args.package_name.clone()],
        )),
        (PackageManagerId::Flatpak, PackageOperation::Uninstall, _) => Ok((
            "flatpak".to_string(),
            vec!["uninstall".to_string(), "-y".to_string(), args.package_name.clone()],
        )),
    }
}

async fn cleanup_orphans(manager: PackageManagerId) -> Result<Option<String>, String> {
    let (command, args) = match manager {
        PackageManagerId::Dnf => (
            "pkexec",
            vec!["dnf".to_string(), "autoremove".to_string(), "--assumeyes".to_string()],
        ),
        PackageManagerId::Apt => (
            "pkexec",
            vec!["apt-get".to_string(), "autoremove".to_string(), "-y".to_string()],
        ),
        _ => return Ok(None),
    };

    let output = run_command(command, args.clone()).await?;
    let details = command_details(command, &args, &output);
    if output.code == 0 {
        Ok(Some(details))
    } else {
        Err(format!("Orphan cleanup failed.\n\n{}", details))
    }
}

#[tauri::command]
async fn get_manager_statuses() -> Result<Vec<ManagerStatus>, String> {
    let mut statuses = Vec::new();
    for manager in [
        PackageManagerId::Dnf,
        PackageManagerId::Apt,
        PackageManagerId::Snap,
        PackageManagerId::Flatpak,
    ] {
        let installed = executable_in_path(manager.executable()).is_some()
            && match manager {
                PackageManagerId::Dnf => executable_in_path("rpm").is_some(),
                PackageManagerId::Apt => executable_in_path("dpkg-query").is_some(),
                _ => true,
            };
        let version = if installed {
            manager_version(manager).await
        } else {
            None
        };
        statuses.push(manager_status_template(manager, installed, version));
    }
    Ok(statuses)
}

#[tauri::command]
async fn list_packages(
    app: tauri::AppHandle,
    manager: PackageManagerId,
    view: PackageView,
    force_refresh: bool,
) -> Result<Vec<PackageInfo>, String> {
    if executable_in_path(manager.executable()).is_none() {
        return Err(format!("{} is not installed or not on PATH.", manager.label()));
    }

    if !force_refresh {
        if let Some(cached) = load_package_cache(&app, manager, view)? {
            return Ok(cached);
        }
    }

    let packages = match manager {
        PackageManagerId::Dnf => list_dnf_packages(view).await?,
        PackageManagerId::Apt => list_apt_packages(view).await?,
        PackageManagerId::Snap => list_snap_packages(view).await?,
        PackageManagerId::Flatpak => list_flatpak_packages(view).await?,
    };

    let _ = save_package_cache(&app, manager, view, &packages);
    Ok(packages)
}

#[tauri::command]
async fn get_package_dependencies(
    manager: PackageManagerId,
    package_name: String,
) -> Result<Vec<DependencyInfo>, String> {
    dependencies_for_package(manager, &package_name).await
}

#[tauri::command]
async fn execute_package_operation(
    app: tauri::AppHandle,
    args: PackageOperationArgs,
) -> Result<PackageOperationResult, String> {
    validate_package_name(&args.package_name)?;

    if !package_is_installed(args.manager, &args.package_name).await? {
        return Err(format!(
            "'{}' is not installed according to {}.",
            args.package_name,
            args.manager.label()
        ));
    }

    let (command, command_args) = match operation_command(&args) {
        Ok(command) => command,
        Err(reason) if args.dry_run && reason == "unsupported-dry-run" => {
            return Ok(unsupported_preview_result(args.manager, args.operation));
        }
        Err(reason) => return Err(reason),
    };

    let output = run_command(&command, command_args.clone()).await?;
    let details = command_details(&command, &command_args, &output);
    let operation_label = match args.operation {
        PackageOperation::Update => "update",
        PackageOperation::Uninstall => "uninstall",
    };

    let mut success = output.code == 0;
    let mut message = if success {
        if args.dry_run {
            format!("Preview completed for '{}'. No changes were made.", args.package_name)
        } else {
            format!(
                "{} {} completed for '{}'.",
                args.manager.label(),
                operation_label,
                args.package_name
            )
        }
    } else {
        format!(
            "{} {} failed for '{}'.",
            args.manager.label(),
            operation_label,
            args.package_name
        )
    };

    let mut details_parts = vec![details];
    if success
        && !args.dry_run
        && args.operation == PackageOperation::Uninstall
        && args.cleanup_orphans
        && matches!(args.manager, PackageManagerId::Dnf | PackageManagerId::Apt)
    {
        match cleanup_orphans(args.manager).await {
            Ok(Some(cleanup_details)) => {
                message.push_str(" Orphan cleanup completed.");
                details_parts.push(cleanup_details);
            }
            Ok(None) => {}
            Err(error) => {
                success = false;
                message.push_str(" Orphan cleanup failed.");
                details_parts.push(error);
            }
        }
    }

    if success && !args.dry_run {
        clear_manager_cache(&app, args.manager);
    }

    Ok(PackageOperationResult {
        success,
        message,
        details: Some(details_parts.join("\n\n---\n\n")),
        command: Some(command_line(&command, &command_args)),
        dry_run: args.dry_run,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_manager_statuses,
            list_packages,
            get_package_dependencies,
            execute_package_operation
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_safe_package_names() {
        assert!(validate_package_name("bash").is_ok());
        assert!(validate_package_name("libssl3:amd64").is_ok());
        assert!(validate_package_name("org.mozilla.firefox").is_ok());
        assert!(validate_package_name("-rf").is_err());
        assert!(validate_package_name("name;rm").is_err());
        assert!(validate_package_name("../name").is_err());
    }

    #[test]
    fn extracts_names_from_nevra_fallbacks() {
        assert_eq!(
            extract_package_name("python3-foobar-0.1.1-11.fc39.noarch"),
            "python3-foobar"
        );
        assert_eq!(extract_package_name("package.x86_64"), "package");
        assert_eq!(extract_package_name("package-name"), "package-name");
    }

    #[test]
    fn parses_apt_dependency_lines() {
        assert_eq!(
            parse_apt_dependency_line("Depends: libc6 (>= 2.34)").unwrap(),
            DependencyInfo {
                name: "libc6".to_string(),
                kind: "Depends".to_string()
            }
        );
        assert!(parse_apt_dependency_line("Breaks: old-package").is_none());
    }
}
