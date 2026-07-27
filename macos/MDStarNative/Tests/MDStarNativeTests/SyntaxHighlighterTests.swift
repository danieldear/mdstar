import SwiftUI
import XCTest
@testable import MDStarNative

final class SyntaxHighlighterTests: XCTestCase {
    func testRustSnippetColorsSomething() {
        let code = "// c\nfn f() { let x = 1; }"
        let attributed = SyntaxHighlighter.highlight(code, language: "rust")
        let colored = attributed.runInfos.contains { $0.foreground != nil }
        XCTAssertTrue(colored, "expected at least one colored run")
    }

    func testRustCommentRunIsColored() {
        let code = "// c\nfn f() { let x = 1; }"
        let attributed = SyntaxHighlighter.highlight(code, language: "rust")
        let commentRun = try! XCTUnwrap(
            attributed.runInfos.first { $0.text == "// c" },
            "expected a run for the comment text"
        )
        XCTAssertNotNil(commentRun.foreground)
    }

    func testEmptyCodeProducesEmptyAttributedString() {
        let attributed = SyntaxHighlighter.highlight("", language: "rust")
        XCTAssertTrue(String(attributed.characters).isEmpty)
        XCTAssertTrue(attributed.runInfos.allSatisfy { $0.foreground == nil })
    }

    func testHashCommentLanguageColorsHashComment() {
        // Python uses `#` line comments rather than `//`.
        let attributed = SyntaxHighlighter.highlight("# note\nx = 1", language: "python")
        let commentRun = try! XCTUnwrap(attributed.runInfos.first { $0.text == "# note" })
        XCTAssertNotNil(commentRun.foreground)
    }
}
