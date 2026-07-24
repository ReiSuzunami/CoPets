# Security Policy

## Supported versions

CoPets has published a private `v0.1.0` prerelease for trusted testing. Security fixes target that
prerelease and the latest revision of `main`; older snapshots and other local development builds
are unsupported. No notarized public release has been published.

## Report a vulnerability

Do not disclose a vulnerability, private Codex payload, prompt, log, credential, or stable user
identifier in a public issue.

Before this repository is published, maintainers must enable a private vulnerability-reporting
channel. Once the repository exposes private vulnerability reporting, use its **Security** page and
choose **Report a vulnerability**. If that option is unavailable, contact a maintainer privately
through an existing trusted channel instead of opening a public report.

Include:

- the affected revision and macOS version;
- the observed impact and required attacker access;
- minimal reproduction steps;
- sanitized logs with prompts, answers, paths, tokens, and identifiers removed;
- whether the issue affects local files, IPC controls, pet-package import, signing, or updates.

Maintainers should acknowledge a report before discussing disclosure timing. No response-time or
release-time guarantee exists before the first public release.

## Security boundary

CoPets is a local macOS sidecar for an already-running Codex App. It has no network listener and
treats processes already running under the same macOS UID as inside its local trust boundary. The
runtime architecture and dated security research describe the exact source-validation, privacy,
and private-interface limits:

- [Runtime privacy boundary](docs/architecture/runtime.md#privacy-boundary)
- [Security and legal boundary](docs/research/security-and-legal-boundary.md)

Private Codex interfaces are unversioned and unsupported. Unknown data must degrade to unavailable
state; it must never invent successful observation or control.
