# MD Star

MD Star is an open-source Markdown reader and editor. The app is native Rust,
drawn with [Volt UI Kit](https://github.com/danieldear/volt-ui-kit): a
workspace file tree, tabs, find, an editor with live preview, Mermaid
diagrams, themes and a native macOS shell. The same app bundle carries `md`, a
terminal viewer and live-preview editor.

## Status

Pre-1.0. The app runs on macOS (Apple silicon); Windows and Linux builds
compile and need packaging and testing before a release.

## Workspace Layout

```text
.
+-- Cargo.toml
+-- crates/
|   +-- mdstar-core/         parser, semantic document IR, diagnostics, LaTeX to Unicode
|   +-- mdstar-render-html/  Markdown to HTML (for export)
|   +-- mdstar-render-volt/  shared native rendering for MD Star and Volt Editor
|   `-- mdstar-volt/         the app and `md`: reader, editor, diagrams, terminal viewer
+-- docs/                    project docs and static web page
+-- tests/fixtures/          shared regression fixtures
`-- install.sh               links `md` out of an installed MD Star.app
```

`crates/mdstar-volt` builds on its own, outside the workspace, because it
depends on a local Volt UI Kit checkout until the kit is published. See its
[README](crates/mdstar-volt/README.md) for building, packaging and releasing.

## Quick Start

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cd crates/mdstar-volt && cargo run --release -- ../../tests/fixtures/sample.md
```

## The `md` command

```text
md FILE              interactive viewer (q quit, e edit with live preview, r reload)
md --plain FILE      print the rendered document
md --app FILE        open it in the app
```

Link it out of an installed app with `./install.sh --link-app`.

## Architecture

```text
Markdown input
   |
   v
mdstar-core (parser, document IR)
   |
   +--> mdstar-render-volt: native units, rich text, code, tables and find mapping
   |       `--> mdstar-volt / Volt Editor: app adapters and media backends
   |
   `--> mdstar-render-html: HTML output
```

## Documentation

- [Documentation index](docs/README.md)
- [Architecture](docs/architecture.md)
- [Project plan](docs/project-plan.md)
- [Static project page](docs/index.html)

Planning documents written before the move to Volt UI Kit describe the earlier
Swift and Tauri apps.

## Open Source Baseline

- dual licensed under MIT or Apache-2.0,
- issue templates and pull request template included,
- CI checks format, clippy, and tests.

## Shared native rendering

The reader now uses `mdstar-render-volt` through a thin application adapter. Volt
Editor consumes that same crate while retaining its streaming/session shell and
background math/diagram renderer; see [the shared-crate contract](crates/mdstar-render-volt/README.md).
This source extraction is not a new packaged release or a claim of native UI acceptance.
