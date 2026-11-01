# Shared native Markdown renderer

Extracted from MD Star for use by its reader and Volt Editor; this crate does not
embed either app, a webview, an agent/provider stack or an extra native window.
It depends on `mdstar-core` for the document IR and on Volt UI Kit's `rust-ui`
feature for elements. Native/GPU features remain the consumer's choice.
It is a standalone, unpublished library excluded from MD Star's core/HTML
workspace, so those libraries and their CI do not require the private toolkit.

```text
MD Star parser -> document IR ------+-> shared native units/prose/code/tables
                                   |              |
Volt streaming parser -> IR adapter +              +-> host application
                                                  |
                                         explicit Media adapter
                                 shared native Mermaid / Volt math worker
```

## Contracts

- `Host` supplies link, clipboard and row-menu actions; interactive consumers
  must implement the actions they expose instead of depending on another App.
- `Ctx` supplies typography, images already loaded by the host, optional find
  state and an optional scoped link callback. Rendering does not read files or
  fetch URLs; image/resource trust policy remains in the host.
- `Media` supplies diagrams, display math and optional typeset inline math.
  Volt keeps its worker and PNG-copy controls, uses shared native Mermaid and
  retains its typeset math rather than MD Star's Unicode fallback.
- `row` owns the reader's margins/context-menu shell; `content` renders only a
  unit, allowing Plan/chat to retain their existing session/selection/scroll shell.
- Text is chunked on UTF-8 boundaries below the kit's 8 KiB and styled-span limits
  without dropping the remainder. Long code lines and their original newlines
  are retained, and decoration does not insert invisible inline-code padding.
- Links are length/control validated, syntax colours follow light/dark pairs,
  and deeply nested visual structures fall back to readable text at the renderer
  depth boundary rather than constructing an invalid native tree.
- `units`, `estimate` and `unit_texts_with` never request cold diagrams. MD Star
  now uses a bounded background preview worker; synchronous diagram building is
  reserved for its explicit export/legacy non-UI API.

## Streaming integration

Volt's existing incremental-arrival parsing and transcript pagination remain
in place. Its thin adapter maps that AST to the shared IR, caches unchanged scoped
pages and passes unfinished Mermaid fences as incomplete to the existing media
worker. Keys are scoped by project/session/response/page, and the response still
owns text selection, context menus and scroll anchoring. This extraction unifies
the native renderer, not every application's parser or media engine.

## Verification status

Compile checks were run for the shared crate, the MD Star reader library/binary
and Volt Editor. Regression sources cover byte-preserving long Unicode code lines
and rich text exceeding a single element's span/byte budget, but **no tests were
run**, as requested. No app launches, GPU snapshots, clipboard operations or native
interaction acceptance are claimed.

Before distribution, manually check long/many-turn responses, streaming and
partial fences, UTF-8 selections/context menus, theme changes, math in tables and
paragraphs, large diagrams, PNG copying, resizing/session switches and MD Star
reader find/export. Use existing projects/fixtures rather than creating another
persistent dummy project.

## Shared Mermaid media (follow-up)

The opt-in `diagrams` feature now contains the actual MD Star native diagram
engine, not only a media callback. Both apps use this module: common flow/state/
sequence diagrams use native kit text and a compact layered layout, while other
families retain a bounded offline SVG/raster fallback. The fallback excludes
LastResort placeholder faces, whose all-glyph coverage can replace an entire
label during SVG font fallback. No remote resources or browser runtime are used.

Flowchart nodes/labels/edges use the strict parser, preserving fan-out such as
`A & B --> C` rather than inventing an `A & B` node; the native model retains
nested-group/style metadata. Native canvas paths are split below toolkit budgets.
The native layout still inherits the outer graph direction for nested groups;
this is not a claim of complete Mermaid CSS, interaction or layout compatibility.
The optional media dependencies match MD Star's existing versions; enabling
`diagrams` requires Rust 1.92 because `mermaid-text 0.16.8` does. Base/core/HTML
workspace features remain independent of that optional backend.

Volt keeps its own bounded worker and calls this engine from that worker, not
from scrolling/paint. Its math engine is unchanged, and PNG copying builds the
same native scene as the live diagram. Wide diagrams scroll horizontally rather
than shrinking labels below the reading floor. Visual layout, emoji rendering,
large-graph performance, and pasted image acceptance remain manual checks; no
tests or native app launches were performed for this follow-up.
