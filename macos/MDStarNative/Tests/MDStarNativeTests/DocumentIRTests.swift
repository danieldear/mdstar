import XCTest
@testable import MDStarNative

final class DocumentIRTests: XCTestCase {
    // MARK: - InlineIR.plainText

    func testInlinePlainTextConcatenatesChildren() {
        let node = IR.inline("strong", children: [IR.text("bold "), IR.text("text")])
        XCTAssertEqual(node.plainText, "bold text")
    }

    func testInlinePlainTextFallsBackToAltForImages() {
        let image = IR.inline("image", alt: "a cat")
        XCTAssertEqual(image.plainText, "a cat")
    }

    // MARK: - BlockIR.searchableText

    func testParagraphSearchableTextIncludesInlineText() {
        let block = IR.block("paragraph", inlines: [IR.text("hello "), IR.text("world")])
        XCTAssertTrue(block.searchableText.contains("hello world"))
    }

    func testCodeBlockSearchableTextIncludesCode() {
        let block = IR.block("code", code: "let answer = 42")
        XCTAssertTrue(block.searchableText.contains("let answer = 42"))
    }

    func testTableSearchableTextIncludesHeadersAndRowCells() {
        let block = IR.block(
            "table",
            headers: [[IR.text("Name")], [IR.text("Age")]],
            rows: [
                [[IR.text("Alice")], [IR.text("30")]],
                [[IR.text("Bob")], [IR.text("25")]],
            ]
        )
        let text = block.searchableText
        for token in ["Name", "Age", "Alice", "30", "Bob", "25"] {
            XCTAssertTrue(text.contains(token), "searchableText should contain \(token): \(text)")
        }
    }

    func testSearchableTextIncludesNestedChildBlocks() {
        let child = IR.block("paragraph", inlines: [IR.text("nested content")])
        let parent = IR.block("blockquote", children: [child])
        XCTAssertTrue(parent.searchableText.contains("nested content"))
    }

    // MARK: - Bookmark round-trip

    func testBookmarkRoundTripsThroughJSON() throws {
        let bookmark = Bookmark(id: "block-7", title: "Core Objectives", level: 2)
        let data = try JSONEncoder().encode(bookmark)
        let decoded = try JSONDecoder().decode(Bookmark.self, from: data)
        XCTAssertEqual(decoded.id, bookmark.id)
        XCTAssertEqual(decoded.title, bookmark.title)
        XCTAssertEqual(decoded.level, bookmark.level)
        XCTAssertEqual(decoded, bookmark)
    }

    // MARK: - IR decoding from JSON (snake_case coding keys)

    func testBlockIRDecodesFromSnakeCaseJSON() throws {
        let json = """
        {
            "id": "b1",
            "kind": "code",
            "inlines": [],
            "children": [],
            "items": [],
            "headers": [],
            "rows": [],
            "language": "rust",
            "code": "fn main() {}"
        }
        """.data(using: .utf8)!
        let block = try JSONDecoder().decode(BlockIR.self, from: json)
        XCTAssertEqual(block.id, "b1")
        XCTAssertEqual(block.kind, "code")
        XCTAssertEqual(block.language, "rust")
        XCTAssertTrue(block.searchableText.contains("fn main() {}"))
    }
}
