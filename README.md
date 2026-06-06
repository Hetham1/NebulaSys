# NebulaSys

NebulaSys is a Tauri desktop application for inspecting and managing Linux package managers from one interface. It currently supports DNF/RPM, APT/dpkg, Snap, and Flatpak with manager detection, package search, lazy requirement lookup, previewable operations, and guarded update/uninstall flows.

This is still early software. Treat destructive package operations with the same care you would use in a terminal.

## Features

- Detects supported package managers available on the host system.
- Lists user-installed and all installed packages where the manager exposes that distinction.
- Searches and filters packages by name, version, category, source, summary, and loaded requirements.
- Loads requirements lazily so large package databases open quickly.
- Previews update and uninstall operations when the underlying manager supports dry runs.
- Requires typed confirmation for actual uninstalls.
- Validates package identifiers in the Rust backend before running package commands.
- Uses short-lived local package caches and clears them after successful mutations.

## Support Matrix

| Manager | Package list | User-installed view | Requirements | Update | Uninstall | Force uninstall |
| --- | --- | --- | --- | --- | --- | --- |
| DNF/RPM | Yes | Yes | RPM requirements | Yes | Yes | Yes |
| APT/dpkg | Yes | Yes | APT dependencies | Yes | Yes | Yes |
| Snap | Yes | Same as all | Not exposed | Yes | Yes | No |
| Flatpak | Apps | Same as all | Runtime details | Yes | Yes | No |

## Repository Layout

```text
.
├── nebula-dnf/              # SvelteKit + Tauri desktop app
│   ├── src/                 # Frontend UI
│   └── src-tauri/           # Rust backend and Tauri config
├── docs/                    # Architecture and release process notes
└── .github/                 # CI, releases, templates
```

The app directory is still named `nebula-dnf` for continuity, but the implementation is now multi-manager.

## Development

Prerequisites:

- Node.js 20+
- Rust stable and Cargo
- Linux package-manager CLIs for the managers you want to test
- Tauri Linux system dependencies

Install frontend dependencies:

```bash
cd nebula-dnf
npm ci
```

Run the frontend checker and build:

```bash
npm run check
npm run build
```

Run Rust tests:

```bash
cd src-tauri
cargo test
```

Run the desktop app:

```bash
cd nebula-dnf
npm run tauri dev
```

## Safety Model

NebulaSys does not expose arbitrary shell execution to the frontend. The Svelte UI calls typed Tauri commands, and the Rust backend chooses the command, validates package identifiers, checks that the target package is installed, and then runs the package-manager command.

Privileged DNF, APT, and Snap mutations use `pkexec`. Flatpak operations run through `flatpak` because user-session installs are common.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for commit conventions, local checks, and pull request expectations.

## Security

Report package-operation safety issues privately. See [SECURITY.md](SECURITY.md).

## License

GPL-3.0-only. See [LICENSE](LICENSE).
