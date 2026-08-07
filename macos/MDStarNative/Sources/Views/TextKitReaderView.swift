import AppKit
import SwiftUI

/// The reading surface.
///
/// One `NSTextView` holds the whole document, which is what makes selection work
/// across headings, paragraphs, lists and tables — and what gives annotations a
/// real character range to anchor to. The previous SwiftUI renderer drew each
/// block as its own `Text`, so every block was an isolated selection island.
struct TextKitReaderView: NSViewRepresentable {
    let document: DocumentIR
    /// Document IDs are stable across edits for navigation and annotations, so
    /// they cannot signal that parsed content has changed. This revision does.
    let documentRevision: Int
    @ObservedObject var settings: ReaderSettings
    @ObservedObject var annotations: AnnotationStore

    let focusedBlockID: String?
    var searchQuery: String = ""
    var currentMatchID: String?

    let onOpenLink: (URL) -> Void
    var onActiveHeadingChange: (String?) -> Void = { _ in }
    var onSelectionChange: (SelectedText?) -> Void = { _ in }

    func makeNSView(context: Context) -> NSScrollView {
        let textView = ReaderTextView()
        textView.isEditable = false
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.isRichText = true
        textView.usesFindBar = false
        textView.textContainerInset = NSSize(width: 0, height: 20)
        textView.linkTextAttributes = [
            .foregroundColor: NSColor.linkColor,
            .cursor: NSCursor.pointingHand,
        ]
        textView.delegate = context.coordinator
        textView.coordinator = context.coordinator
        textView.autoresizingMask = [.width]

        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        // Content must run under the titlebar for the toolbar to blur it.
        scrollView.automaticallyAdjustsContentInsets = true
        scrollView.contentInsets = NSEdgeInsets(top: 0, left: 0, bottom: 24, right: 0)
        scrollView.documentView = textView

        context.coordinator.textView = textView
        context.coordinator.scrollView = scrollView

        // Track scrolling to report which section is on screen.
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(
            context.coordinator,
            selector: #selector(Coordinator.boundsDidChange),
            name: NSView.boundsDidChangeNotification,
            object: scrollView.contentView
        )

        textView.readingWidth = CGFloat(settings.contentWidth)
        context.coordinator.rebuild(document: document, settings: settings)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self

        // Recompose only when the document or typography actually changed;
        // rebuilding on every SwiftUI update would fight the user's scrolling.
        let signature = DocumentSignature(
            documentID: document.documentID,
            origin: document.origin,
            documentRevision: documentRevision,
            fontSize: settings.fontSize,
            family: settings.fontFamily,
            lineSpacing: settings.lineSpacing,
            contentWidth: settings.contentWidth
        )
        if coordinator.signature != signature {
            coordinator.signature = signature
            coordinator.rebuild(document: document, settings: settings)
        }

        coordinator.applyTransientHighlights(
            query: searchQuery,
            currentMatchID: currentMatchID,
            annotations: annotations.annotations(for: document.documentID)
        )

        coordinator.textView?.readingWidth = CGFloat(settings.contentWidth)

        if let focusedBlockID, coordinator.lastScrolledBlockID != focusedBlockID {
            coordinator.lastScrolledBlockID = focusedBlockID
            coordinator.scroll(to: focusedBlockID)
        }
    }

    static func dismantleNSView(_ scrollView: NSScrollView, coordinator: Coordinator) {
        NotificationCenter.default.removeObserver(coordinator)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    struct DocumentSignature: Equatable {
        let documentID: String
        let origin: String
        let documentRevision: Int
        let fontSize: Double
        let family: ReaderFontFamily
        let lineSpacing: Double
        let contentWidth: Double
    }

    // MARK: - Coordinator

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: TextKitReaderView
        weak var textView: ReaderTextView?
        weak var scrollView: NSScrollView?

        var composed: ComposedDocument?
        var signature: DocumentSignature?
        var lastScrolledBlockID: String?
        private var lastReportedHeading: String?

        init(_ parent: TextKitReaderView) {
            self.parent = parent
        }

        func rebuild(document: DocumentIR, settings: ReaderSettings) {
            guard let textView, let storage = textView.textStorage else { return }
            let builder = AttributedDocumentBuilder(document: document, settings: settings)
            let result = builder.build()
            composed = result

            storage.beginEditing()
            storage.setAttributedString(result.text)
            storage.endEditing()

            textView.composed = result
            lastScrolledBlockID = nil
            lastReportedHeading = nil
        }

        // MARK: Scrolling

        func scroll(to blockID: String) {
            guard let textView, let range = composed?.range(for: blockID) else { return }
            textView.scrollRangeToVisible(range)
            // Pull the target to the top rather than merely into view.
            if let layoutManager = textView.layoutManager,
               let container = textView.textContainer {
                let glyphRange = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
                let rect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: container)
                let target = max(0, rect.minY - 12)
                textView.enclosingScrollView?.contentView.scroll(to: NSPoint(x: 0, y: target))
                textView.enclosingScrollView?.reflectScrolledClipView(textView.enclosingScrollView!.contentView)
            }
        }

        @objc func boundsDidChange() {
            reportActiveHeading()
        }

        private func reportActiveHeading() {
            guard let textView,
                  let composed,
                  let layoutManager = textView.layoutManager,
                  let container = textView.textContainer,
                  let clip = scrollView?.contentView else { return }

            let visibleTop = clip.bounds.origin.y
            var active: String?
            for block in composed.blocks where block.kind == "heading" {
                let glyphRange = layoutManager.glyphRange(forCharacterRange: block.range, actualCharacterRange: nil)
                let rect = layoutManager.boundingRect(forGlyphRange: glyphRange, in: container)
                if rect.minY <= visibleTop + 40 {
                    active = block.id
                } else {
                    break
                }
            }
            let resolved = active ?? composed.blocks.first { $0.kind == "heading" }?.id
            guard resolved != lastReportedHeading else { return }
            lastReportedHeading = resolved
            parent.onActiveHeadingChange(resolved)
        }

        // MARK: Transient styling

        /// Search hits and annotation highlights are layout-time decorations, so
        /// they use temporary attributes and never mutate the text storage.
        func applyTransientHighlights(query: String, currentMatchID: String?, annotations: [Annotation]) {
            guard let textView,
                  let layoutManager = textView.layoutManager,
                  let composed else { return }

            let full = NSRange(location: 0, length: composed.text.length)
            layoutManager.removeTemporaryAttribute(.backgroundColor, forCharacterRange: full)

            for annotation in annotations {
                let range = annotation.resolvedRange(in: composed.string)
                guard range.location != NSNotFound, range.upperBound <= composed.text.length else { continue }
                layoutManager.addTemporaryAttributes(
                    [.backgroundColor: annotation.kind.highlightColor],
                    forCharacterRange: range
                )
            }

            let needle = query.trimmingCharacters(in: .whitespaces)
            guard !needle.isEmpty else { return }

            let haystack = composed.string as NSString
            var searchRange = NSRange(location: 0, length: haystack.length)
            let activeRange = currentMatchID.flatMap { composed.range(for: $0) }

            while searchRange.length > 0 {
                let found = haystack.range(of: needle, options: .caseInsensitive, range: searchRange)
                guard found.location != NSNotFound else { break }
                let isCurrent = activeRange.map { NSLocationInRange(found.location, $0) } ?? false
                layoutManager.addTemporaryAttributes(
                    [
                        .backgroundColor: isCurrent
                            ? NSColor.systemOrange.withAlphaComponent(0.75)
                            : NSColor.systemYellow.withAlphaComponent(0.45),
                    ],
                    forCharacterRange: found
                )
                let next = found.upperBound
                searchRange = NSRange(location: next, length: max(0, haystack.length - next))
            }
        }

        // MARK: NSTextViewDelegate

        func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
            guard let url = link as? URL else { return false }
            if let slug = InlineRenderer.anchorSlug(from: url) {
                guard let composed,
                      let target = parent.document.outline.first(where: { $0.anchor == slug })
                        ?? parent.document.outline.first(where: {
                            $0.anchor.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
                                == slug.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
                        }),
                      composed.range(for: target.id) != nil else { return true }
                scroll(to: target.id)
                parent.onActiveHeadingChange(target.id)
                return true
            }
            parent.onOpenLink(url)
            return true
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            guard let textView, let composed else { return }
            let range = textView.selectedRange()
            guard range.length > 0,
                  range.upperBound <= composed.text.length else {
                parent.onSelectionChange(nil)
                return
            }
            let text = (composed.string as NSString).substring(with: range)
            parent.onSelectionChange(
                SelectedText(
                    range: range,
                    text: text,
                    blockID: composed.blockID(containing: range.location)
                )
            )
        }
    }
}

/// A text selection reported back to SwiftUI.
struct SelectedText: Equatable, Identifiable {
    var id: String { "\(range.location)-\(range.length)" }
    let range: NSRange
    let text: String
    let blockID: String?
}

/// Text view that centres its content to the reading measure and exposes
/// annotation commands in the contextual menu.
final class ReaderTextView: NSTextView {
    weak var coordinator: TextKitReaderView.Coordinator?
    var composed: ComposedDocument?

    /// Preferred measure for prose. The container is narrowed to this and
    /// centred, so long lines stay readable in a wide window.
    var readingWidth: CGFloat = 740 {
        didSet { needsLayout = true }
    }

    override func layout() {
        super.layout()
        applyReadingMeasure()
    }

    /// Block decoration is drawn here rather than as text attributes: a run
    /// background paints only behind glyphs, which reads as a highlight, not as
    /// a code card or a quotation rule.
    override func drawBackground(in rect: NSRect) {
        super.drawBackground(in: rect)
        guard let composed, let layoutManager, let container = textContainer else { return }

        let origin = textContainerOrigin
        let cardFill = NSColor.labelColor.withAlphaComponent(0.055)
        let cardStroke = NSColor.labelColor.withAlphaComponent(0.10)
        let quoteBar = NSColor.controlAccentColor.withAlphaComponent(0.55)

        for block in composed.blocks {
            guard block.kind == "code" || block.kind == "blockquote"
                    || block.kind == "frontmatter" || block.kind == "math" || block.kind == "html"
            else { continue }

            let glyphRange = layoutManager.glyphRange(forCharacterRange: block.range, actualCharacterRange: nil)
            var bounds = layoutManager.boundingRect(forGlyphRange: glyphRange, in: container)
            bounds.origin.x += origin.x
            bounds.origin.y += origin.y
            guard bounds.intersects(rect) else { continue }

            if block.kind == "blockquote" {
                let bar = NSRect(x: bounds.minX - 14, y: bounds.minY - 2, width: 3, height: bounds.height + 4)
                quoteBar.setFill()
                NSBezierPath(roundedRect: bar, xRadius: 1.5, yRadius: 1.5).fill()
            } else {
                let card = NSRect(
                    x: bounds.minX - 14,
                    y: bounds.minY - 8,
                    width: max(bounds.width + 28, container.size.width - 4),
                    height: bounds.height + 16
                )
                let path = NSBezierPath(roundedRect: card, xRadius: 8, yRadius: 8)
                cardFill.setFill()
                path.fill()
                cardStroke.setStroke()
                path.lineWidth = 1
                path.stroke()
            }
        }
    }

    private func applyReadingMeasure() {
        guard let container = textContainer, let scrollView = enclosingScrollView else { return }
        let available = scrollView.contentSize.width
        guard available > 0 else { return }

        let target = min(readingWidth, max(320, available - 64))
        let horizontalInset = max(24, ((available - target) / 2).rounded())

        if abs(textContainerInset.width - horizontalInset) > 0.5 {
            textContainerInset = NSSize(width: horizontalInset, height: textContainerInset.height)
        }
        container.widthTracksTextView = true
        if abs(frame.width - available) > 0.5 {
            setFrameSize(NSSize(width: available, height: frame.height))
        }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = super.menu(for: event) ?? NSMenu()
        guard selectedRange().length > 0, let coordinator else { return menu }

        menu.insertItem(.separator(), at: 0)

        let comment = NSMenuItem(
            title: "Add Comment\u{2026}",
            action: #selector(addCommentFromMenu(_:)),
            keyEquivalent: ""
        )
        comment.target = self
        menu.insertItem(comment, at: 0)

        let highlight = NSMenuItem(
            title: "Highlight",
            action: #selector(addHighlightFromMenu(_:)),
            keyEquivalent: ""
        )
        highlight.target = self
        menu.insertItem(highlight, at: 0)

        _ = coordinator
        return menu
    }

    @objc private func addHighlightFromMenu(_ sender: Any?) {
        NotificationCenter.default.post(name: .mdstarAddHighlight, object: nil)
    }

    @objc private func addCommentFromMenu(_ sender: Any?) {
        NotificationCenter.default.post(name: .mdstarAddComment, object: nil)
    }
}
