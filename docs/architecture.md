# Architecture

## Current product

MD Star's desktop app, terminal viewer/editor, and Quick Look provider are Rust.
There is no active Tauri frontend, Swift app, or Swift/Rust FFI bridge.

```text
Markdown files
     |
     v
mdstar-core: parser + semantic DocumentIR + diagnostics
     |
     +------------------------------+
     |                              |
     v                              v
mdstar-volt                     mdstar-render-html
     |                         HTML export library
     +-- native Volt UI Kit reader/editor
     |     workspace tree, tabs, find, themes, live preview
     +-- md CLI/TUI (ratatui + terminal renderer)
     +-- PDF renderer/export
     `-- mdstar-preview: macOS Quick Look extension
```

## Build boundaries

The root Cargo workspace contains `mdstar-core` and `mdstar-render-html`.
`crates/mdstar-volt` is a separate workspace with its own lockfile because it
consumes `../../../rust/volt-ui-kit` as a path dependency. GitHub workflows
fetch the kit into that relative location at a pinned commit.
Run both workspaces' tests; root `cargo test --workspace` alone does not test
the application.

## Native application

Paths below are relative to `crates/mdstar-volt`:

- `src/main.rs`: startup, command registry, menus, event loop, native window.
- `src/app.rs`: document state, tabs/history, edits, persistence, external changes.
- `src/workspace.rs`: directory scanning and file tree.
- `src/view.rs`, `src/render.rs`: widget composition and document rendering.
- `src/compose.rs`, `src/find.rs`, `src/highlight.rs`: editing, search, syntax styling.
- `src/diagram.rs`: Mermaid diagram rendering.
- `src/export.rs`: paginated PDF rendering shared with Quick Look.
- `src/cli.rs`, `src/terminal.rs`: terminal invocation, viewer/editor.
- `src/macos.rs`: AppKit integration (file opening, native menus and services).
- `src/preview.rs`: Quick Look provider entry point.

The bundle's `Contents/MacOS/md` alias selects terminal mode; `md --app` opens
the native GUI. The Quick Look extension is nested inside the application.

## Distribution

`cargo volt` consumes `[package.metadata.volt.bundle]` in the app manifest.
CI checks formatting, linting, tests and ad-hoc bundle assembly. Release builds
use Developer ID signing, hardened runtime, notarization, stapling and Gatekeeper
assessment for the app and DMG. See [release checklist](release-checklist.md).

macOS Apple silicon is the shipping target. Cross-platform compilation is not
proof of packaged desktop behavior. Installation, Finder integration, Quick Look
and editing acceptance remain separate from automated tests.
