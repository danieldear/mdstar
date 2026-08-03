# Release Checklist

Releases are macOS-only for now:

| Artifact | Contents |
| --- | --- |
| `MD-Star-<version>-universal.dmg` | Native SwiftUI reader with the `md` CLI embedded in the bundle |
| `md-<tag>-universal-apple-darwin.tar.gz` | Standalone `md` CLI |

The Tauri desktop app is no longer part of the release; macOS ships the native
reader in `macos/MDStarNative`. Linux and Windows builds are deferred — `md`
still links Tauri, so those targets need the GTK/WebKit toolchain and have not
been validated in CI.

## Repository

- `install.sh` defaults to `REPO=danieldear/mdstar`; override with `REPO=` for forks.
- Confirm `docs/index.html` is enabled as the GitHub Pages entry point if Pages is used.

## Local Verification

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

Native app:

```bash
cargo build -p mdstar-ffi
cd macos/MDStarNative && swift build && swift test
```

Release artifact:

```bash
./script/package_macos.sh --arch universal --dmg
lipo -archs "dist/MD Star.app/Contents/MacOS/MDStarNative"   # expect: x86_64 arm64
```

## Code Signing and Notarization

An unsigned build runs on the machine that produced it, but any copy that is
downloaded or transferred to another Mac carries a quarantine flag and **will
not launch**. This is not a warning the user can click through:

- **macOS 15 removed the Control-click > Open bypass.** Advice to "right-click
  and choose Open" no longer works. The recipient must open **System Settings >
  Privacy & Security**, find the blocked app near the bottom, and choose **Open
  Anyway**, or run:
  ```bash
  xattr -dr com.apple.quarantine "/Applications/MD Star.app"
  ```
- Verify what Gatekeeper actually thinks with `spctl -a -t open \
  --context context:primary-signature -v <file>.dmg`. An unsigned artifact
  reports `rejected — source=no usable signature`.

Treat publishing an unsigned build as shipping a broken first-run experience.
To avoid it you need an Apple Developer Program membership and a
*Developer ID Application* certificate.

Locally:

```bash
CODESIGN_IDENTITY="Developer ID Application: NAME (TEAMID)" \
NOTARY_PROFILE=mdstar \
./script/package_macos.sh --arch universal --dmg
```

In CI, add these repository secrets — the release workflow skips signing when
they are absent:

| Secret | Purpose |
| --- | --- |
| `MACOS_CERTIFICATE` | base64 of the `.p12` certificate |
| `MACOS_CERTIFICATE_PWD` | password for the `.p12` |
| `MACOS_SIGN_IDENTITY` | e.g. `Developer ID Application: NAME (TEAMID)` |
| `NOTARY_APPLE_ID` | Apple ID for notarization |
| `NOTARY_TEAM_ID` | Developer team ID |
| `NOTARY_PASSWORD` | app-specific password |

## macOS Verification

Verify on a **second Mac**, not the build machine. Locally built apps are never
quarantined, so the build machine cannot reproduce the experience a downloaded
copy gives — an unsigned release passes every local check and still fails for
everyone who downloads it.

- Install the DMG by dragging **MD Star** to Applications.
- Only one `MD Star.app` should exist. An older Tauri build installed at the same
  path makes LaunchServices resolve the name to the wrong bundle; verify with:
  ```bash
  /usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
    "/Applications/MD Star.app/Contents/Info.plist"   # expect dev.mdstar.native
  ```
- Right-click a `.md` file and choose Open With -> MD Star; confirm it opens.
- Confirm the file tree, outline, tabs, find (Command-F), and Split/View all work.
- Run `./install.sh --link-app` and confirm `md --help` resolves.

## Known Limitations

- The `md` binary still links Tauri, so `md --app` opens the legacy WebView GUI,
  and building it on Linux needs the GTK/WebKit toolchain. Removing that
  dependency is the prerequisite for restoring Linux and Windows releases.
- DMG window styling needs Automation access to Finder. Without it the installer
  is still valid, just unstyled; grant access in System Settings > Privacy &
  Security > Automation and rebuild for the laid-out window.
- Terminal output renders images as `![alt](url)` markup; no sixel/kitty image
  protocol support.
- Highlights and comments are not implemented; they need a TextKit-based reader.
- Quick Look extension embedding/signing from `macos/MarkwellQuickLook` is deferred.
- Linux MIME registration installer is deferred.

## GitHub Release

```bash
git tag v0.1.0
git push origin main
git push origin v0.1.0
```

The workflow publishes a **draft** release. Review the artifacts, then publish.
