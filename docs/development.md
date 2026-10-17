# Development

This guide covers source builds and verification. For everyday review controls, see the [usage guide](usage.md); for contribution scope and review expectations, see [CONTRIBUTING.md](../CONTRIBUTING.md).

## Validated environment

The desktop MVP is validated on macOS Apple Silicon. Published reference measurements used macOS 26.6, 32 GiB RAM and a local SSD. Those measurements are historical baselines, not hardware requirements or performance guarantees. No minimum supported macOS version has been established.

Intel Macs, Linux and Windows require native build, watcher and WebView verification before support can be claimed. Tauri being cross-platform does not establish GitVista compatibility. See the upstream [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for platform-specific build dependencies when investigating a port. `npm run bundle` currently targets a macOS `.app`.

## Prerequisites

- Node.js **22.12+** and npm.
- Git available on `PATH`.
- Rust installed with [rustup](https://rustup.rs/). `rust-toolchain.toml` pins **1.99.0**, rustfmt and Clippy.
- Xcode Command Line Tools on macOS. Install them with `xcode-select --install` if missing, then complete the installer.
- Python 3 only for the optional benchmark fixture/profiling scripts and Unix process-wrapper integration tests.

Check your environment:

```sh
node --version
npm --version
git --version
rustup --version
xcode-select -p
```

From the repository directory, `cargo --version` should use the pinned toolchain. Rustup may download it on first use. To install it explicitly:

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
```

## Get the source

```sh
git clone https://github.com/pablobesada/gitvista.git
cd gitvista
npm ci
```

For contributions, clone your fork instead. npm dependencies have exact versions, `package-lock.json` locks their graph, and `Cargo.lock` locks Rust dependencies. Keep intentional dependency updates separate from unrelated fixes.

## Run and build

| Command | Purpose |
| --- | --- |
| `npm run desktop` | Run the Tauri app in development mode, backed by real local Git repositories. Starts Vite on port 1420. |
| `npm run dev` | Browser preview with bundled sample data; it cannot inspect local repos or run cleanup. |
| `npm run build` | Type-check and build frontend assets; does not create a desktop application. |
| `npm run bundle` | Build the optimized macOS `.app`, including frontend and Rust. |

Open the release app:

```sh
open target/release/bundle/macos/GitVista.app
```

The compiled executable also accepts a workspace path:

```sh
target/release/gitvista --workspace /path/to/project
```

This uses normal application preferences and may open an additional root tab. Use disposable fixtures when checking saving or cleanup. Local builds do not require an Apple signing account. Downloadable Apple Silicon releases use ad hoc signing; Developer ID signing and notarization are not provided yet. See [installation](installation.md) and the [release process](releasing.md).

## Verification

Run these checks for code changes affecting the corresponding subsystem:

```sh
npm run check
npm test
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm run bundle
git diff --check
```

Frontend-only behavior changes need frontend checks and relevant UI verification; native Git/runtime changes need Rust tests and Clippy. Changes crossing the IPC boundary or renderer/native behavior also need the desktop bundle and native QA. Tests use isolated fixtures; never run destructive checks on personal or user repositories.

For documentation-only PRs, verify links, images, code examples and consistency with the actual command/configuration files. Do not rebuild the desktop app solely for prose changes.

Browser preview validates layout and sample-data interactions. It does **not** validate native Git operations, filesystem watchers, disk-version checks, clipboard integration or WebKit. Describe what was tested and what remains unverified.

## Performance work

Build release mode first, then benchmark the shipped adapter:

```sh
target/release/gitvista --benchmark /path/to/project 30
```

The adapter reports discovery, status and a sample diff without opening a window. It performs read queries. A sample diff time is not an end-to-end p95.

`scripts/create-fixture.py` creates isolated 10K/100K-file workloads and rejects existing destinations. `scripts/profile-idle.py` samples explicitly identified app/WebView processes. See [validation](validation.md#reproduce) for reproducible commands, measured baselines and remaining startup/scroll characterization. Count WebContent, GPU, Networking and transient Git processes when reporting total app memory.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| `cargo` or `rustup` not found | Complete rustup installation and reopen the terminal, or load its shell environment (`source "$HOME/.cargo/env"` for the default Unix installation). The npm Tauri wrapper also checks `~/.cargo/bin`. |
| Native build cannot find macOS SDK/tools | Check `xcode-select -p` and complete Xcode Command Line Tools installation. |
| Port 1420 is occupied | Close the dev server you started before launching `npm run desktop`; do not terminate unrelated processes. |
| Browser shows only sample repositories | This is expected for `npm run dev`. Use `npm run desktop` for local repositories. |
| Some repository state is unavailable | Read the surfaced error and verify the repo with Git. An error/stale result is not a clean state. |

The [architecture guide](architecture.md) describes module boundaries, native path identities, cancellation, caches and write guards.
