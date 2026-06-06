# Changelog

All notable changes to NebulaSys will be documented here.

This project follows Conventional Commits and uses tag-based releases.

## [Unreleased]

### Added

- Multi-manager backend for DNF/RPM, APT/dpkg, Snap, and Flatpak.
- Manager detection and capability reporting.
- Lazy requirement loading for better startup performance.
- Previewable update and uninstall operations where supported.
- Typed uninstall confirmation and force-uninstall warnings.
- Repository docs, contribution rules, CI, and release workflow.

### Changed

- Reworked the UI into a production-style package-management workspace.
- Replaced frontend shell permissions with typed Rust Tauri commands.
- Enabled a CSP in Tauri config.
- Aligned package metadata with the GPL-3.0-only repository license.

### Security

- Added backend package-name validation before all command execution.
- Cleared manager caches after successful package mutations.

