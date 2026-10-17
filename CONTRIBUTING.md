# Contributing to GitVista

Bug reports, documentation fixes, focused code changes and reproducible performance measurements are welcome.

## Before starting

- Search [existing issues](https://github.com/pablobesada/gitvista/issues) and pull requests for related work.
- Open a feature proposal before making a substantial UX, architecture or platform change. Explain the review workflow it improves and its performance implications.
- For vulnerabilities, use the private channel in [SECURITY.md](SECURITY.md), not a public issue.
- Be respectful and constructive. Focus feedback on behavior, evidence and code. Do not publish other people's private code, paths, credentials or repository data.

The current focus is fast review across nested repositories, with explicit working-file editing and confirmed stash cleanup. Staging, commits, history and a plugin framework are outside the current feature set.

## Set up a contribution

1. Fork this repository on GitHub.
2. Clone your fork and create a descriptive branch, for example `docs/improve-setup` or `fix/nested-repo-refresh`.
3. Follow [development setup](docs/development.md) to install dependencies and run the application.
4. Read [AGENTS.md](AGENTS.md) and the [architecture](docs/architecture.md) before changing native commands, Git behavior or the renderer. These constraints apply to human and agent-assisted contributions alike.

There is no required branch-name or commit-message format for outside contributors. Prefer short, descriptive commits and one coherent change per PR.

## Keep changes reviewable

- Preserve separate staged and unstaged states, including net-empty Total comparisons.
- Keep Git arguments based on native paths and opaque IDs; frontend display strings are labels, not filesystem identities.
- Keep normal queries read-only. New write operations require a separate explicit contract and prior scope discussion.
- Preserve unsaved drafts through refreshes and retain navigation/exit guards.
- Keep one active workspace, one mounted diff, explicit worker/cache budgets and bounded background work.
- Add meaningful regression coverage for behavior changes. Tests that write files or run Git mutations must use isolated temporary fixtures, never a contributor's checkout.
- Update relevant user or developer documentation when behavior changes. Use synthetic data for screenshots and reproductions.
- Include only relevant dependency/lockfile changes. Do not commit generated bundles, personal preferences or raw local QA output.

## Verify and submit

Run the [checks appropriate to your change](docs/development.md#verification). Frontend preview does not validate Tauri IPC, native filesystem events or WebKit behavior. Describe any native validation you could not run; untested platforms must not be presented as supported.

For documentation-only changes, check relative links, headings, screenshots and the accuracy of commands against `package.json` and `rust-toolchain.toml`. A new native app build is not required unless the change affects build/runtime behavior.

Submit a PR against `main` with:

- The problem and resulting behavior, with a linked issue when one exists.
- Relevant checks and their results, including OS/architecture for native work.
- Screenshots or a short recording for UI changes, using sample data.
- Limitations or remaining validation gaps. Distinguish measurements from performance goals.

If you used an AI coding tool, review its output and verification results before submitting; the same requirements apply. The [PR template](.github/pull_request_template.md) provides a checklist.

Contributions are licensed under the project's [MIT license](LICENSE). Dependencies and copied material must have compatible licensing and retain required notices.
