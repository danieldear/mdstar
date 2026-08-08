import AppKit
import WebKit
import XCTest
@testable import MDStarNative

@MainActor
final class WebReaderViewTests: XCTestCase {
    func testSourceOnlyChangesReplaceTheBodyInsteadOfReloadingThePage() {
        let signature = WebReaderView.ReaderSignature(
            documentID: "doc-1",
            origin: "/tmp/document.md",
            fontSize: 16,
            family: .system,
            lineSpacing: 1.5,
            contentWidth: 900,
            isDark: true
        )

        XCTAssertEqual(
            WebReaderView.refreshAction(
                previousSignature: signature,
                previousSourceHash: 100,
                nextSignature: signature,
                nextSourceHash: 101
            ),
            .replaceBody
        )
    }

    func testDocumentOrThemeChangesStillReloadTheWholePage() {
        let previous = WebReaderView.ReaderSignature(
            documentID: "doc-1",
            origin: "/tmp/document.md",
            fontSize: 16,
            family: .system,
            lineSpacing: 1.5,
            contentWidth: 900,
            isDark: true
        )
        let next = WebReaderView.ReaderSignature(
            documentID: "doc-2",
            origin: "/tmp/other.md",
            fontSize: 16,
            family: .system,
            lineSpacing: 1.5,
            contentWidth: 900,
            isDark: true
        )

        XCTAssertEqual(
            WebReaderView.refreshAction(
                previousSignature: previous,
                previousSourceHash: 100,
                nextSignature: next,
                nextSourceHash: 100
            ),
            .reloadPage
        )
    }

    func testAnnotationCommandsRemainVisibleWithoutASelection() {
        let items = ReaderWebView.annotationMenuItems(hasSelection: false)

        XCTAssertEqual(items.map(\.title), ["Highlight", "Add Comment\u{2026}"])
        XCTAssertTrue(items.allSatisfy { !$0.isEnabled })
    }

    func testAnnotationCommandsEnableWhenTheBridgeReportsASelection() {
        let items = ReaderWebView.annotationMenuItems(hasSelection: true)

        XCTAssertTrue(items.allSatisfy(\.isEnabled))
    }

    func testAddingAnnotationsPreservesWebKitAndMacOSContextMenuCommands() {
        let menu = NSMenu()
        for title in ["Copy", "Look Up", "Search with Google", "Speech", "Services", "Reload"] {
            menu.addItem(NSMenuItem(title: title, action: nil, keyEquivalent: ""))
        }

        ReaderWebView.insertAnnotationItems(into: menu, hasSelection: true, target: nil)

        let titles = menu.items.map(\.title)
        XCTAssertEqual(Array(titles.prefix(2)), ["Highlight", "Add Comment\u{2026}"])
        for title in ["Copy", "Look Up", "Search with Google", "Speech", "Services", "Reload"] {
            XCTAssertTrue(titles.contains(title), "Expected the native \(title) command to remain")
        }
    }

    func testWebKitReloadIsHandledAsAnInMemoryPreviewReload() {
        XCTAssertTrue(ReaderWebView.handlesReloadNavigation(.reload))
        XCTAssertFalse(ReaderWebView.handlesReloadNavigation(.linkActivated))
        XCTAssertFalse(ReaderWebView.handlesReloadNavigation(.other))
    }

    func testSystemContextMenuReloadIsRecognizedForRetargeting() {
        let titledReload = NSMenuItem(title: "Reload", action: nil, keyEquivalent: "")
        let selectorReload = NSMenuItem(
            title: "Refresh Page",
            action: NSSelectorFromString("reloadFromOrigin:"),
            keyEquivalent: ""
        )
        let copy = NSMenuItem(
            title: "Copy",
            action: NSSelectorFromString("copy:"),
            keyEquivalent: ""
        )

        XCTAssertTrue(ReaderWebView.isReloadMenuItem(titledReload))
        XCTAssertTrue(ReaderWebView.isReloadMenuItem(selectorReload))
        XCTAssertFalse(ReaderWebView.isReloadMenuItem(copy))
    }
}
