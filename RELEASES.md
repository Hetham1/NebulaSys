# Release Process

NebulaSys releases are created from Git tags.

## Versioning

Use semantic versioning once the project reaches stable distribution:

```text
vMAJOR.MINOR.PATCH
```

Before 1.0, minor versions may still include breaking behavior while package-manager support settles.

## Checklist

1. Ensure `CHANGELOG.md` has a section for the release.
2. Run local checks:

   ```bash
   cd nebula-dnf
   npm ci
   npm run check
   npm run build
   cd src-tauri
   cargo fmt --check
   cargo test
   ```

3. Update versions in:
   - `nebula-dnf/package.json`
   - `nebula-dnf/src-tauri/Cargo.toml`
   - `nebula-dnf/src-tauri/tauri.conf.json`
4. Commit with `chore(release): prepare vX.Y.Z`.
5. Tag and push:

   ```bash
   git tag vX.Y.Z
   git push origin main --tags
   ```

6. The release workflow builds Tauri artifacts and creates a draft GitHub release.
7. Review artifacts and release notes, then publish the release.

