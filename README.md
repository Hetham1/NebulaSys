# NebulaSys
The manager for package managers. This project aims to provide unified interfaces for various system package managers.

## Modules

### `nebula-dnf`

`nebula-dnf` is a Svelte and Tauri application that provides a graphical user interface for interacting with the DNF package manager on Fedora-based systems.

#### Core Functionality:

*   **List User-Installed Packages:** Displays packages explicitly installed by the user, along with their direct dependencies. Users can toggle the visibility of dependencies for each package.
*   **List All Installed Packages:** Provides a flat list of all packages currently installed on the system.
*   **Efficient Backend:** Utilizes Rust for backend logic, invoking the `dnf` command-line tool to fetch package information and parsing its output.

#### Technical Details:

*   **Frontend:**
    *   Built with Svelte and Vite.
    *   Located in the `nebula-dnf/src` directory.
    *   The main UI component is `nebula-dnf/src/routes/+page.svelte`.
    *   Communicates with the Rust backend using Tauri's `invoke` API.
*   **Backend (Tauri):**
    *   Built with Rust.
    *   Located in the `nebula-dnf/src-tauri` directory.
    *   Core logic is in `nebula-dnf/src-tauri/src/lib.rs`.
    *   Exposes Tauri commands (`list_installed_packages`, `list_user_installed_packages`) to the frontend.
    *   Optimized regular expression handling using `once_cell::sync::Lazy` for performance.
    *   Features robust parsing of `dnf` command output.

---

## Changelog

### `nebula-dnf` - Recent Major Updates (October 2023)

These changes significantly improved performance and data consistency for the `nebula-dnf` application.

*   **Performance Overhaul for User-Installed Packages:**
    *   The `list_user_installed_packages` command was refactored to use a single `dnf deplist <pkg1> <pkg2> ...` call for all user-installed packages. Previously, it made a separate call for each package. This dramatically reduces the loading time for the "User Installed (with Dependencies)" view.
*   **Optimized Regular Expression Handling:**
    *   Introduced `once_cell::sync::Lazy` to compile all necessary regular expressions (for package name extraction, deplist parsing) only once when the application starts. This speeds up all operations that involve parsing `dnf` output.
*   **Robust and Consistent Package Name Extraction:**
    *   A new helper function, `extract_base_package_name`, was implemented. It uses the regex `^([a-zA-Z0-9][a-zA-Z0-9._+-]*?)(?:-([0-9].*))?$` to accurately extract the base package name from a full NEVRA string (e.g., `package-name` from `package-name-1.0.0-1.fc39.x86_64`). This ensures consistent naming across different views and for dependencies.
*   **Efficient Dependency Parsing:**
    *   A new function, `parse_multiple_deplist_output`, was created to parse the output of the bulk `dnf deplist` command. It correctly associates dependencies (using their base names) with each main user-installed package.
*   **Enhanced "All Installed Packages" View:**
    *   The `list_installed_packages` command now also utilizes `extract_base_package_name` to display consistent base package names.
    *   It uses a `HashSet` internally to ensure the list of packages is unique before being sorted and displayed.
*   **Build and Syntax Fixes:**
    *   Corrected Rust's "turbofish" syntax (e.g., `Vec::<Type>::new()`).
    *   Added the `once_cell` crate to `Cargo.toml` dependencies.
    *   Resolved various build issues, including port conflicts (requiring process termination) and corrupted Rust build artifacts (requiring `cargo clean`).
*   **Frontend Adaptations:**
    *   The Svelte frontend (`+page.svelte`) was updated to correctly work with the data structures returned by the refactored backend commands, ensuring the dependency toggling feature remains functional and accurate.