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
        for handler in ["activeHeading", "openLink", "selection", "findResults"] {
            controller.add(context.coordinator, contentWorld: Self.bridgeWorld, name: handler)
        }

        let webView = ReaderWebView(frame: .zero, configuration: configuration)
        // WKWebView has a strong intrinsic horizontal resistance by default.
        // The document CSS is responsive, so let the native split hierarchy
        // determine its width when sidebars or the source pane are visible.
        webView.setContentHuggingPriority(.defaultLow, for: .horizontal)
        webView.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        // The bridge reports whether a selection exists, which gates the
        // annotation commands in the contextual menu.
        webView.hasSelection = { [weak coordinator = context.coordinator] in
            coordinator?.hasSelection ?? false
        }
        webView.reloadPreview = { [weak coordinator = context.coordinator] in
            coordinator?.reloadPreview()
        }
        webView.navigationDelegate = context.coordinator
        webView.allowsMagnification = false

        context.coordinator.webView = webView
        context.coordinator.signature = readerSignature(for: webView)
        context.coordinator.sourceHash = source.hashValue
        context.coordinator.load(
            document: document, fileURL: fileURL, source: source, settings: settings
        )
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self

        let signature = readerSignature(for: webView)
        let sourceHash = source.hashValue
        let refreshAction = Self.refreshAction(
            previousSignature: coordinator.signature,
            previousSourceHash: coordinator.sourceHash,
            nextSignature: signature,
            nextSourceHash: sourceHash
        )
        coordinator.signature = signature
        coordinator.sourceHash = sourceHash

        switch refreshAction {
        case .reloadPage:
            coordinator.load(
                document: document, fileURL: fileURL, source: source, settings: settings
            )
        case .replaceBody:
            coordinator.replaceBody(document: document, fileURL: fileURL, source: source)
        case .none:
            break
        }

        coordinator.apply(searchQuery: searchQuery)
        coordinator.apply(annotations: annotations.annotations(for: document.documentID))

        if let focusedBlockID,
           refreshAction == .reloadPage || coordinator.lastScrolledBlockID != focusedBlockID {
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
    }

    enum RefreshAction: Equatable {
        case none
        case replaceBody
        case reloadPage
    }

    static func refreshAction(
        previousSignature: ReaderSignature?,
        previousSourceHash: Int?,
        nextSignature: ReaderSignature,
        nextSourceHash: Int
    ) -> RefreshAction {
        guard previousSignature == nextSignature else { return .reloadPage }
        return previousSourceHash == nextSourceHash ? .none : .replaceBody
    }

    private func readerSignature(for webView: WKWebView) -> ReaderSignature {
        ReaderSignature(
            documentID: document.documentID,
            origin: document.origin,
            fontSize: settings.fontSize,
            family: settings.fontFamily,
            lineSpacing: settings.lineSpacing,
            contentWidth: settings.contentWidth,
            isDark: webView.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        )
    }

    // MARK: - Coordinator

    @MainActor
    final class Coordinator: NSObject, WKNavigationDelegate, WKScriptMessageHandler, WKURLSchemeHandler {
        var parent: WebReaderView
        weak var webView: WKWebView?
        var signature: ReaderSignature?
        var sourceHash: Int?
        var lastScrolledBlockID: String?
        /// Mirrors the page's selection so the contextual menu can offer the
        /// annotation commands only when something is selected.
        private(set) var hasSelection = false

        private let htmlService = ReaderHTMLService()
        private var isLoaded = false
        private var pendingSearch: String?
        private var pendingAnnotations: [Annotation]?
        private var pendingScrollBlockID: String?
        private var pendingBodyReplacement: String?
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
            pendingBodyReplacement = nil
            pendingScrollBlockID = nil
            appliedSearch = nil
            // A custom scheme keeps relative image paths working without
            // handing the page blanket file:// access.
            let baseURL = URL(string: "\(WebReaderView.resourceScheme)://document/")
            webView.loadHTMLString(page, baseURL: baseURL)
        }

        /// Replaces only the rendered document body while keeping the current
        /// WKWebView page and viewport alive. A full `loadHTMLString` here would
        /// reset scroll position to zero on every editor keystroke.
        func replaceBody(document: DocumentIR, fileURL: URL?, source: String) {
            let url = fileURL ?? URL(fileURLWithPath: document.origin)
            resourceRoot = url.deletingLastPathComponent()

            let rendered = source.isEmpty
                ? htmlService.html(forFileAt: url)
                : htmlService.html(forSource: source, origin: url.path)
            guard let body = rendered else { return }
            guard isLoaded else {
                pendingBodyReplacement = body
                return
            }
            replaceLoadedBody(with: body)
        }

        private func replaceLoadedBody(with body: String) {
            appliedSearch = nil
            hasSelection = false
            parent.onSelectionChange(nil)
            evaluate(
                "window.__mdstar && window.__mdstar.replaceBody(\(Self.quote(body)));"
            )
        }

        func reloadPreview() {
            load(
                document: parent.document,
                fileURL: parent.fileURL,
                source: parent.source,
                settings: parent.settings
            )
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
            guard isLoaded else {
                pendingScrollBlockID = id
                return
            }
            // The page extends underneath the transparent toolbar. Position
            // the edited block below that native overlap plus a small context
            // margin instead of letting scrollIntoView hide it in the fade.
            let topInset = (webView.map(WindowToolbarGeometry.overlap(for:)) ?? 0) + 24
            evaluate(
                "window.__mdstar && window.__mdstar.scrollToBlock(\(Self.quote(id)), \(Double(topInset)));")
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
            if let pendingBodyReplacement {
                replaceLoadedBody(with: pendingBodyReplacement)
            }
            if let pendingSearch { apply(searchQuery: pendingSearch) }
            if let pendingAnnotations { apply(annotations: pendingAnnotations) }
            if let pendingScrollBlockID { scroll(toBlock: pendingScrollBlockID) }
            pendingBodyReplacement = nil
            pendingSearch = nil
            pendingAnnotations = nil
            pendingScrollBlockID = nil
        }

        /// Only the initial in-memory load is allowed; anything the page tries
        /// to navigate to is handed to the app instead.
        func webView(
            _ webView: WKWebView,
            decidePolicyFor navigationAction: WKNavigationAction,
            decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void
        ) {
            if ReaderWebView.handlesReloadNavigation(navigationAction.navigationType) {
                // `loadHTMLString` uses mdstar-resource://document/ only as a
                // relative-resource base. Letting WebKit reload that URL turns
                // it into a top-level navigation, which Launch Services then
                // mistakes for an external custom URL. Rebuild the in-memory
                // page instead.
                decisionHandler(.cancel)
                Task { @MainActor [weak self] in self?.reloadPreview() }
                return
            }
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
    var reloadPreview: () -> Void = {}

    static func handlesReloadNavigation(_ navigationType: WKNavigationType) -> Bool {
        navigationType == .reload
    }

    static func isReloadMenuItem(_ item: NSMenuItem) -> Bool {
        let action = item.action.map(NSStringFromSelector) ?? ""
        return item.title == "Reload" || action == "reload:" || action == "reloadFromOrigin:"
    }

    static func annotationMenuItems(hasSelection: Bool) -> [NSMenuItem] {
        let highlight = NSMenuItem(
            title: "Highlight",
            action: #selector(addHighlightFromMenu),
            keyEquivalent: ""
        )
        highlight.isEnabled = hasSelection

        let comment = NSMenuItem(
            title: "Add Comment\u{2026}",
            action: #selector(addCommentFromMenu),
            keyEquivalent: ""
        )
        comment.isEnabled = hasSelection

        return [highlight, comment]
    }

    static func insertAnnotationItems(
        into menu: NSMenu,
        hasSelection: Bool,
        target: AnyObject?
    ) {
        let titles = Set(annotationMenuItems(hasSelection: true).map(\.title))
        for item in menu.items where titles.contains(item.title) {
            menu.removeItem(item)
        }

        let annotationItems = annotationMenuItems(hasSelection: hasSelection)
        for item in annotationItems.reversed() {
            item.target = target
            menu.insertItem(item, at: 0)
        }
        menu.insertItem(.separator(), at: annotationItems.count)
    }

    override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) {
        super.willOpenMenu(menu, with: event)
        for item in menu.items {
            if Self.isReloadMenuItem(item) {
                item.target = self
                item.action = #selector(reloadPreviewFromMenu)
            }
        }
        Self.insertAnnotationItems(into: menu, hasSelection: hasSelection(), target: self)
    }

    @objc private func reloadPreviewFromMenu() {
        reloadPreview()
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
