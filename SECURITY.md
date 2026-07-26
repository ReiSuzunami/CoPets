# Security Policy

## Supported versions

CoPets publishes `v0.2.0` as a public prerelease for testing. Security fixes target that prerelease
and the latest revision of `main`; older snapshots and other local development builds are
unsupported. No notarized release has been published.

## Report a vulnerability

Do not disclose a vulnerability, private Codex payload, prompt, log, credential, or stable user
identifier in a public issue.

Use this repository's **Security** page and choose **Report a vulnerability**. GitHub keeps that
report private to maintainers. Do not open a public issue for a vulnerability. If the reporting
form is temporarily unavailable, contact a maintainer through an existing trusted channel instead.

Include:

- the affected revision and macOS version;
- the observed impact and required attacker access;
- minimal reproduction steps;
- sanitized logs with prompts, answers, paths, tokens, and identifiers removed;
- whether the issue affects local files, IPC controls, pet-package import, signing, or updates.

Maintainers should acknowledge a report before discussing disclosure timing. No response-time or
release-time guarantee exists before the first public release.

## Security boundary

CoPets is a local macOS companion for the official Codex App. Normal observation attaches to an
already-running App. The optional experimental bridge can launch, restart, or connect one verified
same-user App; that App may expose a dynamically chosen IPv4 loopback debugging endpoint. CoPets
does not expose a listener or accept remote endpoints. Processes already running under the same
macOS UID remain inside the local trust boundary. The runtime architecture and dated security
research describe the exact source-validation, privacy, and private-interface limits:

- [Runtime privacy boundary](docs/architecture/runtime.md#privacy-boundary)
- [Security and legal boundary](docs/research/security-and-legal-boundary.md)

Private Codex interfaces are unversioned and unsupported. Unknown data must degrade to unavailable
state; it must never invent successful observation or control.
