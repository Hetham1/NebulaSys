# Security Policy

NebulaSys runs local package-manager commands and can trigger privileged system changes. Please report security issues privately rather than through a public issue.

## Supported Versions

The project is pre-1.0. Security fixes target the `main` branch until stable releases begin.

## Reporting

Open a private GitHub security advisory for this repository, or contact the maintainer directly if advisories are unavailable.

Include:

- Affected package manager or operation.
- Reproduction steps.
- Expected and actual command behavior.
- Whether privilege escalation is involved.
- Any relevant command output.

## Security-Sensitive Areas

- Package identifier validation.
- Privileged command construction.
- Dry-run behavior vs actual mutation behavior.
- Cache invalidation after package changes.
- Tauri IPC permissions and CSP.

