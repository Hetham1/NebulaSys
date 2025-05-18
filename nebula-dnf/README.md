# Tauri + SvelteKit

This template should help get you started developing with Tauri and SvelteKit in Vite.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

## Roadmap

This project is an exploration into building a more user-friendly interface for DNF package management using Tauri and SvelteKit.

### Completed

*   **Phase 1: UI Overhaul (Nebula Theme)**
    *   Implemented a modern, space-themed UI ("Nebula") for a visually engaging experience.
    *   Redesigned layout, buttons, lists, and typography with a dark theme and vibrant accents.
    *   Replaced inline styles with a global stylesheet and CSS variables for better maintainability.
    *   Enhanced interactivity with hover effects and clear visual states for UI elements.

### Next Steps

*   **Phase 2: Core Functionality Enhancements**
    *   **Search Functionality:** Implement a search bar to quickly find packages within the displayed lists (both user-installed and all packages).
    *   **Package Management Options:** 
        *   Introduce functionality to update selected packages.
        *   Introduce functionality to remove (delete) selected packages.
        *   Ensure appropriate confirmations and error handling for these operations.
    *   **Advanced Sorting for User Packages:**
        *   Develop a mechanism to differentiate between packages installed as part of the desktop environment/OS setup and those installed manually by the user.
        *   Allow sorting or filtering based on this distinction.

*   **Phase 3: Further Enhancements (Potential)**
    *   Detailed package information view.
    *   History of package operations.
    *   Configuration options for the user.
