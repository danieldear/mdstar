import AppKit
import SwiftUI
import WebKit

/// The reading surface.
///
/// Markup and structural CSS come from the Rust core, matching what the console
/// frontend renders from the same document. Only theming is applied here.
///
/// Two things make the web view safe to point at untrusted documents: the core
/// strips scripting constructs before the HTML ever arrives, and the page runs
/// under a CSP that forbids script entirely. The app's own bridge lives in a
/// private content world, so it is unaffected by that policy and invisible to
/// the page.
struct WebReaderView: NSViewRepresentable {
    let document: DocumentIR
    let fileURL: URL?
    @ObservedObject var settings: ReaderSettings
    @ObservedObject var annotations: AnnotationStore

    /// Live source. Rendering this rather than the file is what makes the split
    /// view update while typing, before anything is saved.
    let source: String
    let focusedBlockID: String?
    var searchQuery: String = ""

    let onOpenLink: (URL) -> Void
    var onActiveHeadingChange: (String?) -> Void = { _ in }
    var onSelectionChange: (WebSelection?) -> Void = { _ in }
    var onFindResults: (Int, Int) -> Void = { _, _ in }
    /// Vertical scroll position, used to fade the toolbar as the page moves.
    var onScrollOffsetChange: (CGFloat) -> Void = { _ in }

    private static let bridgeWorld = WKContentWorld.world(name: "mdstar.bridge")
    private static let resourceScheme = "mdstar-resource"

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(context.coordinator, forURLScheme: Self.resourceScheme)

        let controller = configuration.userContentController
        controller.addUserScript(
            WKUserScript(
                source: ReaderBridgeScript.source,
                injectionTime: .atDocumentEnd,
                forMainFrameOnly: true,
                in: Self.bridgeWorld
            )
        )
        for handler in ["activeHeading", "openLink", "selection", "findResults", "scrollOffset"] {
            controller.add(context.coordinator, contentWorld: Self.bridgeWorld, name: handler)
        }

        let webView = ReaderWebView(frame: .zero, configuration: configuration)
        // The bridge reports whether a selection exists, which gates the
        // annotation commands in the contextual menu.
        webView.hasSelection = { [weak coordinator = context.coordinator] in
            coordinator?.hasSelection ?? false
        }
        webView.navigationDelegate = context.coordinator
        webView.allowsMagnification = false

        context.coordinator.webView = webView
        context.coordinator.load(
            document: document, fileURL: fileURL, source: source, settings: settings
        )
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self

        let signature = ReaderSignature(
            documentID: document.documentID,
            origin: document.origin,
            fontSize: settings.fontSize,
            family: settings.fontFamily,
            lineSpacing: settings.lineSpacing,
            contentWidth: settings.contentWidth,
            isDark: webView.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua,
            sourceHash: source.hashValue
        )
        if coordinator.signature != signature {
            coordinator.signature = signature
            coordinator.load(
                document: document, fileURL: fileURL, source: source, settings: settings
            )
        }

        coordinator.apply(searchQuery: searchQuery)
        coordinator.apply(annotations: annotations.annotations(for: document.documentID))

        if let focusedBlockID, coordinator.lastScrolledBlockID != focusedBlockID {
            coordinator.lastScrolledBlockID = focusedBlockID
            coordinator.scroll(toBlock: focusedBlockID)
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    struct ReaderSignature: Equatable {
        let documentID: String
        let origin: String
        let fontSize: Double
        let family: ReaderFontFamily
        let lineSpacing: Double
        let contentWidth: Double
        let isDark: Bool
        /// Included so editing the buffer re-renders the preview.
        let sourceHash: Int
    }

    // MARK: - Coordinator

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler, WKURLSchemeHandler {
        var parent: WebReaderView
        weak var webView: WKWebView?
        var signature: ReaderSignature?
        var lastScrolledBlockID: String?
        /// Mirrors the page's selection so the contextual menu can offer the
        /// annotation commands only when something is selected.
        private(set) var hasSelection = false

        private let htmlService = ReaderHTMLService()
        private var isLoaded = false
        private var pendingSearch: String?
        private var pendingAnnotations: [Annotation]?
        private var appliedSearch: String?
        /// Directory the document lives in; the only place resources are served
        /// from, so a document cannot pull files from elsewhere on disk.
        private var resourceRoot: URL?

        init(_ parent: WebReaderView) {
            self.parent = parent
        }

        func load(document: DocumentIR, fileURL: URL?, source: String, settings: ReaderSettings) {
            guard let webView else { return }
            let url = fileURL ?? URL(fileURLWithPath: document.origin)
            resourceRoot = url.deletingLastPathComponent()

            // Prefer the buffer so unsaved edits are visible; fall back to disk.
            let rendered = source.isEmpty
                ? htmlService.html(forFileAt: url)
                : htmlService.html(forSource: source, origin: url.path)
            guard let body = rendered else { return }
            let isDark = webView.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            let page = Self.page(
                body: body,
                base: htmlService.baseStylesheet(),
                theme: ReaderTheme.overrides(settings: settings, isDark: isDark)
            )

            isLoaded = false
            appliedSearch = nil
            // A custom scheme keeps relative image paths working without
            // handing the page blanket file:// access.
            let baseURL = URL(string: "\(WebReaderView.resourceScheme)://document/")
            webView.loadHTMLString(page, baseURL: baseURL)
        }

        private static func page(body: String, base: String, theme: String) -> String {
            """
            <!DOCTYPE html>
            <html>
            <head>
            <meta charset="utf-8" />
            <meta http-equiv="Content-Security-Policy"
                  content="default-src 'none'; img-src \(WebReaderView.resourceScheme): data: https:; style-src 'unsafe-inline'; script-src 'none'" />
            <style>\(base)</style>
            <style>\(theme)</style>
            </head>
            <body><div id="reader-root">\(body)</div></body>
            </html>
            """
        }

        // MARK: Commands

        func scroll(toBlock id: String) {
            evaluate("window.__mdstar && window.__mdstar.scrollToBlock(\(Self.quote(id)));")
        }

        func apply(searchQuery: String) {
            guard isLoaded else { pendingSearch = searchQuery; return }
            guard appliedSearch != searchQuery else { return }
            appliedSearch = searchQuery
            evaluate("window.__mdstar && window.__mdstar.find(\(Self.quote(searchQuery)));")
        }

        func findNext() { evaluate("window.__mdstar && window.__mdstar.findNext();") }
        func findPrevious() { evaluate("window.__mdstar && window.__mdstar.findPrevious();") }

        func apply(annotations: [Annotation]) {
            guard isLoaded else { pendingAnnotations = annotations; return }
            let payload = annotations.compactMap { annotation -> [String: Any]? in
                guard let blockID = annotation.blockID else { return nil }
                return [
                    "blockId": blockID,
                    "start": annotation.location,
                    "end": annotation.location + annotation.length,
                    "kind": annotation.kind.rawValue,
                ]
            }
            guard let data = try? JSONSerialization.data(withJSONObject: payload),
                  let json = String(data: data, encoding: .utf8) else { return }
            evaluate("window.__mdstar && window.__mdstar.applyAnnotations(\(json));")
        }

        private func evaluate(_ script: String) {
            webView?.evaluateJavaScript(script, in: nil, in: WebReaderView.bridgeWorld)
        }

        private static func quote(_ value: String) -> String {
            let data = try? JSONSerialization.data(withJSONObject: [value])
            guard let data, let text = String(data: data, encoding: .utf8) else { return "\"\"" }
            return String(text.dropFirst().dropLast())
        }

        // MARK: WKNavigationDelegate

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            isLoaded = true
            if let pendingSearch { apply(searchQuery: pendingSearch) }
            if let pendingAnnotations { apply(annotations: pendingAnnotations) }
            pendingSearch = nil
            pendingAnnotations = nil
        }

        /// Only the initial in-memory load is allowed; anything the page tries
        /// to navigate to is handed to the app instead.
        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
        ) {
            guard navigationAction.navigationType == .other else {
                if let url = navigationAction.request.url { parent.onOpenLink(url) }
                decisionHandler(.cancel)
                return
            }
            decisionHandler(.allow)
        }

        // MARK: WKScriptMessageHandler

        func userContentController(
            _ controller: WKUserContentController,
            didReceive message: WKScriptMessage
        ) {
            switch message.name {
            case "activeHeading":
                parent.onActiveHeadingChange(message.body as? String)

            case "openLink":
                guard let href = message.body as? String else { return }
                if let url = InlineRenderer.resolvedURL(href, origin: parent.document.origin) {
                    parent.onOpenLink(url)
                }

            case "selection":
                guard let payload = message.body as? [String: Any],
                      let text = payload["text"] as? String,
                      let blockID = payload["blockId"] as? String,
                      let start = payload["start"] as? Int,
                      let end = payload["end"] as? Int else {
                    hasSelection = false
                    parent.onSelectionChange(nil)
                    return
                }
                hasSelection = !text.isEmpty
                parent.onSelectionChange(
                    WebSelection(text: text, blockID: blockID, start: start, end: end)
                )

            case "scrollOffset":
                guard let offset = message.body as? Double else { return }
                parent.onScrollOffsetChange(CGFloat(offset))

            case "findResults":
                guard let payload = message.body as? [String: Any],
                      let count = payload["count"] as? Int,
                      let index = payload["index"] as? Int else { return }
                parent.onFindResults(count, index)

            default:
                break
            }
        }

        // MARK: WKURLSchemeHandler

        /// Serves images referenced by the document, restricted to the folder
        /// the document itself lives in.
        func webView(_ webView: WKWebView, start urlSchemeTask: WKURLSchemeTask) {
            guard let requested = urlSchemeTask.request.url,
                  let root = resourceRoot else {
                urlSchemeTask.didFailWithError(URLError(.badURL))
                return
            }

            let relative = requested.path.removingPercentEncoding ?? requested.path
            let candidate = URL(fileURLWithPath: relative, relativeTo: root).standardizedFileURL

            guard candidate.path.hasPrefix(root.standardizedFileURL.path),
                  let data = try? Data(contentsOf: candidate) else {
                urlSchemeTask.didFailWithError(URLError(.fileDoesNotExist))
                return
            }

            let response = URLResponse(
                url: requested,
                mimeType: Self.mimeType(for: candidate.pathExtension),
                expectedContentLength: data.count,
                textEncodingName: nil
            )
            urlSchemeTask.didReceive(response)
            urlSchemeTask.didReceive(data)
            urlSchemeTask.didFinish()
        }

        func webView(_ webView: WKWebView, stop urlSchemeTask: WKURLSchemeTask) {}

        private static func mimeType(for pathExtension: String) -> String {
            switch pathExtension.lowercased() {
            case "png": "image/png"
            case "jpg", "jpeg": "image/jpeg"
            case "gif": "image/gif"
            case "svg": "image/svg+xml"
            case "webp": "image/webp"
            case "heic": "image/heic"
            default: "application/octet-stream"
            }
        }
    }
}

/// Web view that contributes the annotation commands to its contextual menu.
///
/// The TextKit surface offered these from the start; the web surface did not,
/// so switching engines silently lost the ability to highlight or comment by
/// right-clicking.
final class ReaderWebView: WKWebView {
    var hasSelection: () -> Bool = { false }

    override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) {
        super.willOpenMenu(menu, with: event)
        guard hasSelection() else { return }

        menu.insertItem(.separator(), at: 0)

        let comment = NSMenuItem(
            title: "Add Comment\u{2026}",
            action: #selector(addCommentFromMenu),
            keyEquivalent: ""
        )
        comment.target = self
        menu.insertItem(comment, at: 0)

        let highlight = NSMenuItem(
            title: "Highlight",
            action: #selector(addHighlightFromMenu),
            keyEquivalent: ""
        )
        highlight.target = self
        menu.insertItem(highlight, at: 0)
    }

    @objc private func addHighlightFromMenu() {
        NotificationCenter.default.post(name: .mdstarAddHighlight, object: nil)
    }

    @objc private func addCommentFromMenu() {
        NotificationCenter.default.post(name: .mdstarAddComment, object: nil)
    }
}

/// A selection reported from the page, anchored to a block and offsets within it.
struct WebSelection: Equatable, Identifiable {
    var id: String { "\(blockID)-\(start)-\(end)" }
    let text: String
    let blockID: String
    let start: Int
    let end: Int
}
