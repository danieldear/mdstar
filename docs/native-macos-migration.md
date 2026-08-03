# Native macOS migration

MD Star is moving its macOS presentation from the Tauri/WebView desktop frontend to a native SwiftUI application while keeping the Rust parser, semantic model, CLI, terminal TUI, and current Tauri application intact.

## Phase 1 native vertical slice

`macos/MDStarNative` is a macOS 13+ SwiftUI application. It provides a persistent workspace sidebar, recursive file tree, separate document structure outline, native toolbar/breadcrumbs/history, responsive semantic document rendering, and a collapsible inspector shell.

### Delivered (reader)

The reader is styled after the Mud viewer with the system accent color:

- Correct inline formatting from the IR — bold, italic, strikethrough, inline
  code, links — plus block images, task lists, blockquotes, `Grid`-based tables,
  and borderless code blocks with a dependency-free syntax highlighter.
- A **free-flowing, title-less window** (`.hiddenTitleBar`) with a **floating
  Liquid Glass toolbar** (`glassEffect` on macOS 26, material fallback below) that
  the document scrolls beneath.
- Sidebar: search, workspace tree, and a badge-free outline whose active section
  tracks scroll position; section **bookmarks** persisted per document.
- **Split / View** modes (read-only source view with a line-number gutter), Space
  to toggle, in-page anchor links, and a **floating find widget** (bottom-center,
  ⌘F) with match highlighting.

### Deferred (needs a richer text engine)

- **Highlights and comments** anchor to arbitrary text selections, which SwiftUI's
  `Text` cannot report. They require moving the reader to a TextKit/`NSTextView`
  layer; the inspector currently states this instead of exposing dead controls.
- Real editing in Edit/Split (currently source is read-only).

~~~text
Workspace root
  ├─ Rust DocumentIR v1 (semantic JSON, IDs, source ranges)
  ├─ mdstar-ffi C ABI (owned strings and error envelopes)
  └─ SwiftUI reader/workspace shell

Existing Tauri frontend ─ retained as fallback
Terminal renderer/TUI    ─ unchanged
Quick Look parser        ─ retained until contract parity is proven
~~~

## Build and run

Run the native app bundle with:

```bash
./script/build_and_run.sh
```

The script builds `mdstar-ffi`, builds the Swift package, creates `dist/MD Star.app`, embeds the Rust dynamic library, and launches the app as a real macOS application.

## Contract rules

- Native presentation consumes `DocumentIR` JSON from `mdstar-ffi`; it does not parse Markdown itself and does not treat HTML as app state.
- `DocumentIR` is versioned and carries deterministic node IDs plus source ranges to support later bookmarks, highlights, and comments.
- The current Quick Look extension must converge on the same DocumentIR contract before its handwritten Swift parser is removed.

## Parity gate before removing Tauri or the Quick Look parser

All of the following must hold:

1. Rust semantic fixtures cover headings, links, tasks, tables, code, and parser diagnostics.
2. The FFI JSON contract is validated for success, parse failure, invalid arguments, and ownership/free behavior.
3. The native app is manually tested for workspace restore, Finder/drop open, file tree navigation, back/forward, resize, light/dark appearance, tables, and code blocks.
4. The existing Tauri app and terminal TUI remain build/test green.
5. Quick Look receives a separate migration implementation and matches shared fixture expectations.
