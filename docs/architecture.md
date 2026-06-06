# Architecture

NebulaSys is split into a Svelte frontend and a Rust Tauri backend.

## Frontend

The frontend renders package-manager state and calls typed Tauri commands:

- `get_manager_statuses`
- `list_packages`
- `get_package_dependencies`
- `execute_package_operation`

It does not receive shell execution permissions. Package requirements are loaded only when a package row is expanded.

## Backend

The backend owns package-manager behavior:

- Detect manager availability.
- Parse package lists.
- Validate package identifiers.
- Build command arguments.
- Execute dry runs and actual operations.
- Cache package lists briefly.
- Clear caches after successful mutations.

The current adapters are implemented in `nebula-dnf/src-tauri/src/lib.rs`. As the project grows, adapters should move behind a trait such as:

```rust
trait PackageManager {
    async fn status(&self) -> ManagerStatus;
    async fn list(&self, view: PackageView) -> Result<Vec<PackageInfo>, String>;
    async fn dependencies(&self, package: &str) -> Result<Vec<DependencyInfo>, String>;
    async fn operation(&self, args: PackageOperationArgs) -> Result<PackageOperationResult, String>;
}
```

## Safety Rules

- Never pass arbitrary frontend command strings to the backend.
- Validate package names before command construction.
- Prefer dry-run previews for operations that mutate the system.
- Require explicit confirmation for uninstall operations.
- Keep manager-specific behavior in adapters.
- Surface command output in operation details.

