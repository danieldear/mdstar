# MD Star on Volt

The native MD Star app, drawn with [Volt UI Kit](../../../rust/volt-ui-kit):
a Markdown reader and editor with a workspace file tree, tabs, find, a
split editor with live preview and a native macOS shell.

It depends on a local checkout of the kit, so it builds outside the main
workspace:

```sh
cargo run --release -- README.md      # a file, or a folder to browse
```

## Command line

The app bundle also carries `md`, the terminal viewer and live-preview
editor: `Contents/MacOS/md` links to the app's executable, which runs the
terminal viewer instead of opening a window when started under that name.

```sh
ln -sf "/Applications/MD Star.app/Contents/MacOS/md" ~/.local/bin/md   # or ../../install.sh --link-app
md README.md              # view; e edits with live preview, q quits
md --plain README.md      # print the rendered document
md --app README.md        # open it in the app
```

## Mermaid diagrams

Flowcharts, state, sequence and pie diagrams retain MD Star's native drawing.
Gantt charts, timelines, mindmaps, user journeys, class/ER diagrams and other
families supported by `mermaid-rs-renderer` 0.3.1 use the same strict parser as
Volt Editor. They render offline through SVG into cached high-resolution images;
no JavaScript, browser, or external image requests are used. These extended
charts currently use a light canvas even in dark mode.

Source/SVG size, graph size, raster allocation and cache memory are bounded.
Invalid or unsupported syntax keeps the original code visible with an error.
This is not a claim of complete Mermaid.js syntax compatibility.
See `tests/fixtures/mermaid/extended.md` for examples.

## Quick Look

The app carries **MD Star Preview**, a Quick Look extension: Finder's
Space-bar preview of Markdown files is drawn by the app's own renderer
(diagrams, math, emoji, footnotes) and shown as PDF pages. It is built from
`src/preview.rs` and declared under `[[package.metadata.volt.bundle.extensions]]`.
When another installed app also previews Markdown, choose MD Star Preview in
System Settings → General → Login Items & Extensions → Quick Look.

The same pages can be written to a file:

```sh
mdstar-volt notes.md --pdf notes.pdf      # add --theme dark for dark pages
```

## Packaging

The app bundle, its document types and its signing are described in
`[package.metadata.volt.bundle]` in `Cargo.toml` and built with the kit's
`cargo volt` tool:

```sh
cargo volt bundle                         # target/release/bundle/MD Star.app
cargo volt release --dry-run              # show the release steps
cargo volt release                        # signed, notarized MD-Star-<version>.dmg
```

Releases are for Apple silicon only: built on an Apple silicon Mac, the app
holds just the arm64 binary (`--universal` would add Intel and double it).

`cargo volt release` signs with the Developer ID certificate in the keychain
and notarizes with the `mdstar` keychain profile. Create that profile once:

```sh
xcrun notarytool store-credentials mdstar --apple-id YOU@EXAMPLE.COM --team-id A822K85XUW
```

### Releasing from GitHub

`.github/workflows/ci.yml` builds, lints and tests the app on macOS for every
push; `.github/workflows/release.yml` runs `cargo volt release` on an Apple
silicon runner when a `v*` tag matching this crate's version is pushed, and
attaches the notarized disk image and its checksum to a draft GitHub release.
Both fetch Volt UI Kit at the commit in `VOLT_UI_KIT_REF`.

They need these repository secrets (Settings → Secrets and variables →
Actions). `gh secret set NAME --repo danieldear/mdstar` asks for each value
without echoing it:

| Secret | Value |
|---|---|
| `VOLT_UI_KIT_TOKEN` | A fine-grained token with read-only *Contents* access to `danieldear/volt-ui-kit` |
| `MACOS_CERTIFICATE_P12` | The Developer ID Application certificate and key, exported from Keychain Access as `.p12`, base64-encoded: `base64 -i DeveloperID.p12 \| gh secret set MACOS_CERTIFICATE_P12 --repo danieldear/mdstar` |
| `MACOS_CERTIFICATE_PASSWORD` | The password chosen when exporting the `.p12` |
| `APPLE_ID` | The Apple ID that notarizes |
| `APPLE_APP_PASSWORD` | An app-specific password for that Apple ID (one just for CI) |

To release: set `version` in `Cargo.toml`, commit, then

```sh
git tag v0.1.0 && git push origin v0.1.0
```

Install `cargo volt` from the kit with
`cargo install --path ../../../rust/volt-ui-kit --features dev-tools --bin cargo-volt`.
