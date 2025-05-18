# NebulaSys
The manager for package managers. This project aims to provide unified interfaces for various system package managers.

## Modules

### `nebula-dnf`

`nebula-dnf` is a Svelte and Tauri application that provides a graphical user interface for interacting with the DNF package manager on Fedora-based systems.

#### Core Functionality:

*   **List User-Installed Packages:** Displays packages explicitly installed by the user, along with their direct dependencies. Users can toggle the visibility of dependencies for each package.
*   **List All Installed Packages:** Provides a flat list of all packages currently installed on the system.
*   **Efficient Backend:** Utilizes Rust for backend logic, invoking system package management tools (`dnf`, `rpm`) to fetch package information and parsing its output.
*   **User Interface:** Modern and responsive UI built with Svelte.

#### Key Features (Recent Enhancements):

*   **Persistent Backend Caching:** User-installed packages and their dependencies are cached in a local JSON file (`package_cache.json`). This makes subsequent application loads and view switches nearly instantaneous.
*   **Optimized Dependency Resolution:** Uses `rpm -qR <package_name>` for resolving dependencies of installed packages, which is generally faster and lighter than `dnf repoquery --requires` for this purpose.
*   **Faster "All Installed" List:** Employs `rpm -qa --qf '%{NAME}\n'` for a rapid retrieval of all installed package names.
*   **Controlled Concurrency:** Limits the number of concurrent `rpm` processes during dependency fetching to prevent system overload and crashes, ensuring stability even with many packages.
*   **Manual Cache Refresh:** A "Refresh Current View" button allows users to bypass the local cache and fetch fresh package information from the system on demand.
*   **Responsive UI Caching:** The Svelte frontend also maintains a session cache for quickly re-rendering views.

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

### `nebula-dnf` - Performance and Stability Overhaul (November 2023)

This set of updates focused on dramatically improving the performance, stability, and responsiveness of the `nebula-dnf` application, especially when dealing with a large number of installed packages.

*   **Implemented Persistent Backend Caching:**
    *   User-installed packages and their dependencies are now cached in a local JSON file (`package_cache.json`) within the application's local data directory.
    *   Subsequent application starts or views of user-installed packages load data from this cache, resulting in near-instantaneous display after the initial fetch.
    *   A `force_refresh` option allows bypassing the cache when needed (e.g., via the "Refresh Current View" button).
*   **Switched to `rpm` for Faster Queries:**
    *   Dependency resolution for user-installed packages now uses `rpm -qR <package_name>` instead of `dnf repoquery --requires`. This directly queries the local RPM database and is significantly faster for installed packages.
    *   Listing all installed packages (flat list) now uses `rpm -qa --qf '%{NAME}\n'`, which is more performant than the previous `dnf repoquery --installed`.
*   **Introduced Controlled Concurrency for Backend Tasks:**
    *   A semaphore (`tokio::sync::Semaphore`) is used in the Rust backend to limit the number of concurrent `rpm -qR` processes (e.g., to 5). This prevents the application from overwhelming the system with too many simultaneous processes, which previously led to memory exhaustion and crashes.
*   **Improved Package Name Extraction for `rpm` Output:**
    *   The `extract_base_package_name` helper function was enhanced to better parse various output formats from `rpm -qR`, including file paths and complex capability strings, providing more consistent display names.
*   **Frontend Integration for Cache Control:**
    *   The Svelte frontend now passes a `forceRefresh` parameter to the backend, enabling the "Refresh Current View" button to trigger a full data refresh and cache update.
*   **Build System and Dependency Management:**
    *   Added `tokio` as a dependency in `Cargo.toml` to support asynchronous operations and the semaphore.
    *   Resolved various compilation errors related to missing dependencies, type annotations, and function definitions during the refactoring process.

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

## Roadmap

This section outlines the planned development path for NebulaSys, focusing on the `nebula-dnf` module initially.

### `nebula-dnf` Module

#### Completed

*   **Phase 1: Core Functionality & Performance Overhaul** (October - November 2023)
    *   Established basic listing of user-installed and all installed packages.
    *   Implemented dependency viewing for user-installed packages.
    *   Significantly optimized backend performance by:
        *   Switching from `dnf repoquery` to direct `rpm` calls for faster data retrieval.
        *   Implementing backend caching (`package_cache.json`) for near-instantaneous loads after the first fetch.
        *   Introducing controlled concurrency for `rpm` calls to prevent system overload.
        *   Optimizing regex handling.
    *   Refined package name extraction for consistency.
*   **Phase 2: UI/UX Enhancement (Nebula Theme)**
    *   Implemented a modern, space-themed UI ("Nebula") for `nebula-dnf` for a visually engaging experience.
    *   Redesigned layout, buttons, lists, and typography with a dark theme and vibrant accents using Svelte.
    *   Replaced inline styles with a global stylesheet and CSS variables for better maintainability.
    *   Enhanced interactivity with hover effects and clear visual states for UI elements.

#### Next Steps

*   **Phase 3: Interactive Package Management & Advanced Features**
    *   **Search Functionality:** Implement a search bar within `nebula-dnf` to quickly find packages within the displayed lists (both user-installed and all packages).
    *   **Package Management Operations:** 
        *   Introduce functionality to update selected packages.
        *   Introduce functionality to remove (delete) selected packages.
        *   Ensure appropriate user confirmations and robust error handling for these operations.
    *   **Advanced Sorting/Filtering for User Packages:**
        *   Develop a mechanism to differentiate between packages installed as part of the initial desktop environment/OS setup and those installed manually by the user post-setup.
        *   Allow sorting or filtering based on this distinction to give users a clearer view of their own additions.

*   **Phase 4: Broader NebulaSys Vision (Potential)**
    *   Exploration of UI components for other package managers.
    *   Unified dashboard concepts.
    *   Configuration and settings management for NebulaSys.