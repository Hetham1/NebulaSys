# NebulaSys (WIP)
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
please note that this is a hobby project and its a work in progress, also note that currently im only planing to develop the same thing for apt and maybe snaps and flatpacks.
