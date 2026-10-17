# Security policy

GitVista handles local repository paths, source files and Git metadata. Vulnerabilities involving unintended file writes, unsafe Git arguments, path traversal, helper execution or disclosure of repository data should be reported privately.

## Report a vulnerability

Use GitHub's [private vulnerability reporting form](https://github.com/pablobesada/gitvista/security/advisories/new). This repository has private reporting enabled. A GitHub account is required; the report is shared privately with the repository maintainers.

Please include:

- The affected commit/version, OS/architecture and Git version.
- Expected and observed behavior, with minimal reproduction steps.
- A small synthetic repository or sanitized example when possible.
- Impact and any mitigation you have identified.

Do not open a public issue or PR containing exploit details, private source code, real repository contents, credentials or personal information. If the private form is unavailable, open an issue asking for a private reporting channel **without technical details or sensitive attachments**. Do not send a full report through public comments.

## Supported code

This project is an early MVP, with binary releases starting at v0.1.0 and no LTS/backport policy. Security fixes target the current `main` branch. Reports about older commits are welcome, but a separate patched build for an older commit is not guaranteed. Include the exact commit so maintainers can identify the affected code.

There is no guaranteed response or remediation deadline. Maintainers will use the private report to clarify reproduction, discuss fixes and coordinate disclosure.

## Write boundaries and limitations

- Review queries are read-only with respect to Git's index/configuration.
- Explicit saving changes the current regular working file, guarded by native identity and disk-version checks.
- Confirmed stash cleanup changes the selected repository's working tree and index and creates a stash. It can include untracked files; ignored files and nested working trees are preserved.
- Disk-version checks are optimistic concurrency control, not an atomic lock against other programs writing the same file.

These contracts are described in the [usage guide](docs/usage.md) and [architecture](docs/architecture.md). Suspected violations belong in a private vulnerability report. Ordinary UX bugs and feature requests can use [public issues](https://github.com/pablobesada/gitvista/issues/new/choose).
