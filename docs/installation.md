# Install GitVista on Mac

## Download

The [latest release](https://github.com/pablobesada/gitvista/releases/latest) includes a ZIP containing **GitVista.app**, ready to run without compiling. Choose **GitVista-0.1.0-macOS-arm64.zip** for version 0.1.0. GitHub's automatic “Source code” archives contain source, not the app.

The binary is for **Apple Silicon Macs (M1 or newer)**. Native validation used macOS 26.6; a minimum supported macOS version has not been established. This release has no Intel, Linux or Windows binary.

1. Ensure Git is installed: open Terminal and run `git --version`. If macOS offers to install Command Line Tools, finish that installation first. GitVista uses the installed Git executable.
2. Download and unzip the app archive.
3. Drag **GitVista.app** into **Applications**.
4. Open it, then choose a workspace folder with **⌘O**.

Node.js, npm, Rust and the source checkout are not required to run this download.

## First launch and macOS security

Version 0.1.0 has an **ad hoc signature** for bundle integrity. It does **not** have a Developer ID signature, an identified Apple developer or Apple notarization. macOS may therefore block opening an internet download.

After attempting to open the app, if macOS blocks it as an unidentified developer or because Apple cannot check it, open **System Settings → Privacy & Security**, find the message for GitVista, and choose **Open Anyway**. Confirm only if you trust the download from this repository. Follow [Apple's instructions for opening a blocked app](https://support.apple.com/en-us/102445); organization-managed Macs may restrict this option. Do not disable Gatekeeper globally.

If macOS reports malware or a damaged app, stop and verify the download and release source instead of bypassing that warning. The release ZIP is checked for integrity and extracted-signature validity on the build Mac; first launch on a separate quarantined Mac has not been validated.

## Verify the download

Download **SHA256SUMS** alongside the ZIP from the same release. From that download folder:

```sh
shasum -a 256 -c SHA256SUMS
```

The ZIP should report `OK`. A checksum checks transfer integrity; it is not a separate publisher identity or a substitute for notarization.

The app includes the project license, third-party notices and build information under `GitVista.app/Contents/Resources/`. Build information records the release commit and architecture.

## Update or remove

Quit GitVista before replacing the app with a newer downloaded copy. Resolve unsaved drafts before quitting. Replacing or removing the app does not remove your repositories or their Git data. Preferences are stored separately by the desktop application.

For review controls, editing guards and stash recovery, see the [usage guide](usage.md). Report installation problems through [GitHub Issues](https://github.com/pablobesada/gitvista/issues/new/choose), including the release version, macOS version, chip and exact message without private repository contents.
