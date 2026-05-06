import CoreGraphics
import Foundation
import QuickLookUI
import UniformTypeIdentifiers

// QLPreviewProvider is the principal class for data-based Quick Look extensions.
// QLPreviewingController is the protocol that supplies the preview data.
final class PreviewProvider: QLPreviewProvider, QLPreviewingController {
    func providePreview(
        for request: QLFilePreviewRequest,
        completionHandler handler: @escaping (QLPreviewReply?, Error?) -> Void
    ) {
        do {
            let source = try String(contentsOf: request.fileURL, encoding: .utf8)
            let html = htmlDocument(
                from: source,
                fileName: request.fileURL.lastPathComponent,
                fileExtension: request.fileURL.pathExtension.lowercased()
            )
            let htmlData = Data(html.utf8)
            let size = CGSize(width: 1100, height: 1600)
            let reply = QLPreviewReply(
                dataOfContentType: .html,
                contentSize: size
            ) { _ in htmlData }
            handler(reply, nil)
        } catch {
            handler(nil, error)
        }
    }
}

private func htmlDocument(from source: String, fileName: String, fileExtension ext: String) -> String {
    let body = renderDocumentBody(source: source, ext: ext)
    return """
    <!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>\(escapeHTML(fileName))</title>
        <style>
          :root { color-scheme: light dark; }
          *, *::before, *::after { box-sizing: border-box; }
          body {
            margin: 0;
            padding: 32px 40px;
            font: 15px/1.65 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
            background: Canvas;
            color: CanvasText;
            max-width: none;
          }
          h1, h2, h3, h4, h5, h6 {
            margin: 1.4em 0 0.4em;
            font-weight: 600;
            line-height: 1.25;
          }
          h1 { font-size: 2em;   border-bottom: 1px solid color-mix(in oklab, CanvasText 15%, Canvas); padding-bottom: 0.3em; }
          h2 { font-size: 1.5em; border-bottom: 1px solid color-mix(in oklab, CanvasText 10%, Canvas); padding-bottom: 0.2em; }
          h3 { font-size: 1.25em; }
          h4 { font-size: 1em; }
          p  { margin: 0.75em 0; }
          a  { color: LinkText; text-decoration: underline; }
          code {
            font: 13px/1.4 ui-monospace, SFMono-Regular, Menlo, monospace;
            background: color-mix(in oklab, CanvasText 8%, Canvas);
            padding: 0.15em 0.35em;
            border-radius: 4px;
          }
          pre {
            margin: 1em 0;
            padding: 16px;
            border: 1px solid color-mix(in oklab, CanvasText 15%, Canvas);
            border-radius: 8px;
            overflow: auto;
            background: color-mix(in oklab, CanvasText 4%, Canvas);
          }
          pre code { background: none; padding: 0; font-size: 13px; }
          pre.mermaid {
            white-space: pre;
            overflow: auto;
          }
          blockquote {
            margin: 1em 0;
            padding: 0.5em 1em;
            border-left: 4px solid color-mix(in oklab, CanvasText 25%, Canvas);
            color: color-mix(in oklab, CanvasText 70%, Canvas);
          }
          blockquote p { margin: 0.25em 0; }
          ul, ol { margin: 0.75em 0; padding-left: 2em; }
          li { margin: 0.3em 0; }
          hr {
            border: none;
            border-top: 1px solid color-mix(in oklab, CanvasText 15%, Canvas);
            margin: 1.5em 0;
          }
          img { max-width: 100%; height: auto; }
          table {
            width: 100%;
            border-collapse: collapse;
            margin: 1em 0;
            table-layout: auto;
          }
          th, td {
            border: 1px solid color-mix(in oklab, CanvasText 18%, Canvas);
            padding: 8px 12px;
            text-align: left;
            vertical-align: top;
            white-space: normal;
            overflow-wrap: anywhere;
          }
          th {
            background: color-mix(in oklab, CanvasText 5%, Canvas);
            font-weight: 600;
          }
          .mermaid-diagram {
            margin: 1em 0;
            padding: 12px;
            border: 1px solid color-mix(in oklab, CanvasText 15%, Canvas);
            border-radius: 8px;
            overflow: auto;
            background: color-mix(in oklab, CanvasText 4%, Canvas);
          }
          .mermaid-diagram svg { max-width: 100%; height: auto; }
        </style>
        <script defer src="https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js"></script>
      </head>
      <body>
        \(body)
        <script>
          (function renderMermaid() {
            function start() {
              if (!window.mermaid) return;
              window.mermaid.initialize({
                startOnLoad: false,
                theme: window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'neutral',
                fontFamily: '-apple-system, "SF Pro Text", "Helvetica Neue", sans-serif',
                fontSize: 14,
                securityLevel: 'loose'
              });
              const blocks = Array.from(document.querySelectorAll('pre.mermaid'));
              blocks.forEach(async (node, index) => {
                try {
                  const { svg } = await window.mermaid.render('qlm-' + index, node.textContent || '');
                  const wrap = document.createElement('div');
                  wrap.className = 'mermaid-diagram';
                  wrap.innerHTML = svg;
                  node.replaceWith(wrap);
                } catch (_) {
                  // Keep source block visible on rendering failure.
                }
              });
            }
            if (document.readyState === 'loading') {
              document.addEventListener('DOMContentLoaded', start, { once: true });
            } else {
              start();
            }
          })();
        </script>
      </body>
    </html>
    """
}

private func renderDocumentBody(source: String, ext: String) -> String {
    switch ext {
    case "md", "markdown", "mdown", "mkd", "mdtxt":
        return renderMarkdown(source)
    case "csv":
        if let table = renderCSVTable(source) {
            return table
        }
        return renderCodeBlock(source: source, language: ext)
    default:
        return renderCodeBlock(source: formatSource(source: source, ext: ext), language: ext)
    }
}

private func renderCodeBlock(source: String, language: String) -> String {
    "<pre><code class=\"language-\(escapeHTML(language))\">\(escapeHTML(source))</code></pre>"
}

private func formatSource(source: String, ext: String) -> String {
    switch ext {
    case "json":
        guard
            let data = source.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data),
            let pretty = try? JSONSerialization.data(withJSONObject: obj, options: [.prettyPrinted, .sortedKeys]),
            let text = String(data: pretty, encoding: .utf8)
        else { return source }
        return text
    case "xml":
        guard
            let doc = try? XMLDocument(xmlString: source, options: [])
        else { return source }
        return doc.xmlString(options: [.nodePrettyPrint])
    case "yml", "yaml", "toml":
        return source
    default:
        return source
    }
}

private func renderCSVTable(_ source: String) -> String? {
    let rows = source
        .split(whereSeparator: \.isNewline)
        .map { parseCSVRow(String($0)) }
        .filter { !$0.isEmpty }

    guard let header = rows.first else { return nil }
    let bodyRows = rows.dropFirst()

    var html = "<table><thead><tr>"
    for cell in header {
        html += "<th>\(escapeHTML(cell))</th>"
    }
    html += "</tr></thead><tbody>"

    for row in bodyRows {
        html += "<tr>"
        for index in header.indices {
            let cell = index < row.count ? row[index] : ""
            html += "<td>\(escapeHTML(cell))</td>"
        }
        html += "</tr>"
    }

    html += "</tbody></table>"
    return html
}

private func parseCSVRow(_ line: String) -> [String] {
    var cells: [String] = []
    var current = ""
    var inQuotes = false
    var i = line.startIndex

    while i < line.endIndex {
        let ch = line[i]
        if ch == "\"" {
            let next = line.index(after: i)
            if inQuotes, next < line.endIndex, line[next] == "\"" {
                current.append("\"")
                i = line.index(after: next)
                continue
            }
            inQuotes.toggle()
            i = next
            continue
        }

        if ch == ",", !inQuotes {
            cells.append(current)
            current.removeAll(keepingCapacity: true)
            i = line.index(after: i)
            continue
        }

        current.append(ch)
        i = line.index(after: i)
    }

    cells.append(current)
    return cells
}
