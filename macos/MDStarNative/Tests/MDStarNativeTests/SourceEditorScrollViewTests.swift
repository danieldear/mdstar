import AppKit
import XCTest
@testable import MDStarNative

@MainActor
final class SourceEditorScrollViewTests: XCTestCase {
    func testSourceEditorOwnsItsTopEdgeAndCanReturnToDocumentStart() {
        let source = (1...80).map { "line \($0)" }.joined(separator: "\n")
        let scrollView = SourceEditorScrollView(source: source)
        scrollView.frame = NSRect(x: 0, y: 0, width: 520, height: 320)
        scrollView.layoutSubtreeIfNeeded()

        XCTAssertFalse(scrollView.automaticallyAdjustsContentInsets)
        XCTAssertEqual(scrollView.contentInsets.top, 0)
        XCTAssertEqual(scrollView.editorTextView.frame.minY, 0)

        scrollView.contentView.scroll(to: NSPoint(x: 0, y: 180))
        XCTAssertGreaterThan(scrollView.contentView.bounds.origin.y, 0)

        scrollView.scrollToDocumentStart()

        XCTAssertEqual(scrollView.contentView.bounds.origin.x, 0, accuracy: 0.5)
        XCTAssertEqual(scrollView.contentView.bounds.origin.y, 0, accuracy: 0.5)
        XCTAssertTrue(
            scrollView.documentVisibleRect.intersects(
                scrollView.editorTextView.firstRectForCharacterRange(NSRange(location: 0, length: 1))
            )
        )
    }

    func testSourceEditorPreservesLiteralMarkdownPunctuation() {
        let scrollView = SourceEditorScrollView(source: "---")
        let textView = scrollView.editorTextView

        XCTAssertFalse(textView.isAutomaticDashSubstitutionEnabled)
        XCTAssertFalse(textView.isAutomaticQuoteSubstitutionEnabled)
        XCTAssertFalse(textView.isAutomaticTextReplacementEnabled)
        XCTAssertFalse(textView.isAutomaticSpellingCorrectionEnabled)
    }

    func testSourceEditorReservesTransparentToolbarAtDocumentStart() {
        let source = (1...80).map { "line \($0)" }.joined(separator: "\n")
        let scrollView = SourceEditorScrollView(source: source)
        scrollView.frame = NSRect(x: 0, y: 0, width: 520, height: 320)
        scrollView.layoutSubtreeIfNeeded()

        scrollView.applyToolbarContentInset(64)

        XCTAssertFalse(scrollView.automaticallyAdjustsContentInsets)
        XCTAssertEqual(scrollView.toolbarContentInset, 64, accuracy: 0.5)
        XCTAssertEqual(scrollView.contentInsets.top, 64, accuracy: 0.5)
        XCTAssertEqual(scrollView.scrollerInsets.top, 64, accuracy: 0.5)

        scrollView.contentView.scroll(to: NSPoint(x: 0, y: 180))
        XCTAssertGreaterThan(scrollView.contentView.bounds.origin.y, 0)

        scrollView.scrollToDocumentStart()

        XCTAssertEqual(scrollView.contentView.bounds.origin.x, 0, accuracy: 0.5)
        XCTAssertEqual(scrollView.contentView.bounds.origin.y, -64, accuracy: 0.5)

        let firstLine = scrollView.editorTextView.firstRectForCharacterRange(
            NSRange(location: 0, length: 1)
        )
        let firstLineDistanceBelowViewportTop =
            firstLine.minY - scrollView.contentView.bounds.origin.y
        XCTAssertGreaterThanOrEqual(firstLine.minY, scrollView.documentVisibleRect.minY)
        XCTAssertGreaterThanOrEqual(firstLineDistanceBelowViewportTop, 64)
    }

    func testToolbarOverlapUsesWindowContentLayoutWhenSafeAreaIsIgnored() {
        let fullSizeEditor = NSRect(x: 0, y: 0, width: 900, height: 600)
        let unobscuredContent = NSRect(x: 0, y: 0, width: 900, height: 534)

        XCTAssertEqual(
            WindowToolbarGeometry.overlap(
                viewFrameInWindow: fullSizeEditor,
                contentLayoutRect: unobscuredContent
            ),
            66,
            accuracy: 0.5
        )

        XCTAssertEqual(
            WindowToolbarGeometry.overlap(
                viewFrameInWindow: unobscuredContent,
                contentLayoutRect: unobscuredContent
            ),
            0,
            accuracy: 0.5
        )
    }
}

private extension NSTextView {
    func firstRectForCharacterRange(_ range: NSRange) -> NSRect {
        guard let layoutManager, let textContainer else { return .zero }
        let glyphRange = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        return layoutManager.boundingRect(forGlyphRange: glyphRange, in: textContainer)
            .offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
    }
}
