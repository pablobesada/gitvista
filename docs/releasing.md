# macOS release process

Releases are prepared locally on macOS Apple Silicon. There is no automated release workflow. Version 0.1.0 distributes an ad hoc signed ZIP; Developer ID signing and notarization remain future work. Do not describe an ad hoc signature as Apple verification.

## Prepare

1. Start with a clean checkout and confirm the intended commit against `origin/main`. Align versions in `package.json`, `package-lock.json`, workspace `Cargo.toml`/`Cargo.lock` and `src-tauri/tauri.conf.json`. Never replace an existing published tag or release asset silently.
2. Run `npm ci`, `npm run check`, `npm test`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`. Follow the [development guide](development.md) for native QA appropriate to behavior changes.
3. Update installation links, platform limitations, security support wording and release notes. Commit the release preparation before building, so provenance identifies the exact source.
4. Run `npm run bundle`. Check the app's version, bundle identifier and executable architecture. The expected identifier is `io.gitvista.desktop`; the current artifact is ARM64 only. A binary deployment target is not proof of support on older macOS versions.

## Package and verify

- Work under ignored `output/releases/`. Stage a copy of `target/release/bundle/macos/GitVista.app` with `ditto`; preserve the original bundle and record its file hashes.
- Include the project `LICENSE`, dependency inventory/notices and `BUILD-INFO.json` in the staged app's `Contents/Resources/`. Inventory the locked production npm graph and active-target Rust normal/build graph, including available license/notice files. Retrieve omitted notices from the dependency's pinned upstream revision and record their source. Preserve upstream source links for dependencies requiring source availability. Do not include credentials, local configuration, repository contents or build-cache paths in the artifact.
- Record version, full source commit, architecture, toolchain versions and signing/notarization status in build information. This identifies the local build; it does not claim bit-for-bit reproducibility or independent provenance attestation.
- For the current ad hoc distribution, sign **only the staged copy** with `codesign --force --sign - --identifier io.gitvista.desktop --timestamp=none PATH_TO_STAGED_APP`. Validate with `codesign --verify --deep --strict --verbose=2 PATH_TO_STAGED_APP`.
- Create `GitVista-VERSION-macOS-arm64.zip` with `ditto -c -k --keepParent PATH_TO_STAGED_APP PATH_TO_ZIP`, then generate `SHA256SUMS` using `shasum -a 256` from the asset directory.
- Extract the ZIP into a fresh directory and verify archive integrity, executable permissions, version, architecture and the extracted signature. Confirm the source bundle hashes are unchanged. Exercise the extracted executable on isolated fixtures; use separate preferences for GUI write tests and preserve the user's running session.

## Publish

Push the reviewed source commit to GitHub and create a **draft** release with a new version tag targeting that exact commit. Attach the ZIP and `SHA256SUMS`; avoid uploading staged directories or local QA files. The release notes must state Apple Silicon only, Git as a runtime prerequisite, ad hoc signing, no notarization, measured validation and unverified platform coverage, with a link to [installation](installation.md).

Download the uploaded assets into a new directory and verify their checksums and extracted signature. Publish the draft and check the public release, tag target, asset sizes and download links. Do not claim an update system, notarization, Intel compatibility or validation on another Mac unless those were actually provided or tested.
