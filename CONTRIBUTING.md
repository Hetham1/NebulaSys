# Contributing

Thanks for improving NebulaSys. This project touches system package managers, so correctness and safety matter more than speed.

## Development Checks

Run these before opening a pull request:

```bash
cd nebula-dnf
npm ci
npm run check
npm run build
cd src-tauri
cargo fmt --check
cargo test
```

If a check cannot be run on your machine, say why in the PR.

## Commit Guidelines

Use Conventional Commits:

```text
type(scope): short summary
```

Accepted types:

- `feat`: user-facing capability
- `fix`: bug fix
- `perf`: performance improvement
- `refactor`: internal code improvement without behavior change
- `docs`: documentation only
- `test`: tests only
- `ci`: CI or release automation
- `build`: dependency, packaging, or build-system work
- `chore`: maintenance
- `security`: safety or vulnerability fix

Examples:

```text
feat(apt): add manual package listing
fix(dnf): reject unsafe package identifiers
perf(ui): lazy-load package requirements
docs(repo): add release process
```

## Pull Requests

Keep pull requests focused. A package-manager adapter change should include parsing tests where practical and should describe what commands are run for list, preview, update, and uninstall.

PRs that touch package mutations must explain:

- Which backend validation prevents unsafe identifiers.
- Whether the operation supports dry-run previews.
- Whether privileges are required.
- What happens to caches after success.

## Coding Standards

- Prefer typed Rust command contracts over frontend shell access.
- Keep destructive behavior behind previews and explicit confirmation.
- Keep dependency resolution lazy unless there is a proven reason to preload it.
- Avoid manager-specific assumptions leaking into the UI.
- Use clear user-facing errors with command output available in details.

