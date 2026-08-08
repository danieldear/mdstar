import XCTest
@testable import MDStarNative

final class ReaderBridgeScriptTests: XCTestCase {
    func testEditingScrollReservesNativeTopChrome() {
        XCTAssertTrue(
            ReaderBridgeScript.source.contains(
                "bridge.scrollToBlock = function (id, topInset)"
            )
        )
        XCTAssertTrue(
            ReaderBridgeScript.source.contains(
                "targetTop - reservedTop"
            )
        )
    }

    func testLivePreviewReplacementPreservesTheCurrentViewport() {
        XCTAssertTrue(
            ReaderBridgeScript.source.contains(
                "bridge.replaceBody = function (html)"
            )
        )
        XCTAssertTrue(ReaderBridgeScript.source.contains("const scrollY = window.scrollY"))
        XCTAssertTrue(ReaderBridgeScript.source.contains("window.scrollTo(scrollX, scrollY)"))
    }

    func testCaretSyncDoesNotMoveAnAlreadyVisibleBlock() {
        XCTAssertTrue(
            ReaderBridgeScript.source.contains(
                "rect.bottom > reservedTop && rect.top < visibleBottom"
            )
        )
    }
}
