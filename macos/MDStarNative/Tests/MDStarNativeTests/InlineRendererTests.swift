import SwiftUI
import XCTest
@testable import MDStarNative

final class InlineRendererTests: XCTestCase {
    private let origin = "/docs/example.md"

    private func render(_ inlines: [InlineIR], highlight: String? = nil, active: Bool = false) -> AttributedString {
        InlineRenderer.attributedString(
            inlines,
            origin: origin,
            highlight: highlight,
            highlightIsActive: active
        )
    }

    // MARK: - Styled runs

    func testPlainTextRunUsesBaseFontAndNoDecoration() {
        let attributed = render([IR.text("hello")])
        let runs = attributed.runInfos
        XCTAssertEqual(runs.count, 1)
        let run = try! XCTUnwrap(runs.first)
        XCTAssertEqual(run.text, "hello")
        XCTAssertEqual(run.font, .body)
        XCTAssertNil(run.link)
        XCTAssertNil(run.strikethrough)
    }

    func testStrongMakesTextBold() {
        let plain = render([IR.text("word")]).runInfos.first?.font
        let strong = render([IR.inline("strong", children: [IR.text("word")])])
        let run = try! XCTUnwrap(strong.run(containing: "word"))
        XCTAssertNotNil(run.font)
        // Bold changes the font away from the plain base font.
        XCTAssertNotEqual(run.font, plain)
        XCTAssertEqual(run.font, Font.body.bold())
    }

    func testEmphasisMakesTextItalicAndDiffersFromBold() {
        let plainFont = render([IR.text("word")]).runInfos.first?.font
        let emphasis = render([IR.inline("emphasis", children: [IR.text("word")])])
        let boldFont = render([IR.inline("strong", children: [IR.text("word")])]).run(containing: "word")?.font

        let run = try! XCTUnwrap(emphasis.run(containing: "word"))
        XCTAssertNotNil(run.font)
        XCTAssertNotEqual(run.font, plainFont)
        // Italic and bold are distinct transforms of the base font.
        XCTAssertNotEqual(run.font, boldFont)
        XCTAssertEqual(run.font, Font.body.italic())
    }

    func testInlineCodeHasBackgroundFill() {
        let attributed = render([IR.inline("code", text: "let x")])
        let run = try! XCTUnwrap(attributed.run(containing: "let x"))
        XCTAssertNotNil(run.background)
    }

    func testDeleteSetsStrikethrough() {
        let attributed = render([IR.inline("delete", children: [IR.text("gone")])])
        let run = try! XCTUnwrap(attributed.run(containing: "gone"))
        XCTAssertNotNil(run.strikethrough)
    }

    func testLinkSetsResolvedURLAndAccentColor() {
        let attributed = render([IR.inline("link", children: [IR.text("click")], url: "https://example.com")])
        let run = try! XCTUnwrap(attributed.run(containing: "click"))
        let expected = InlineRenderer.resolvedURL("https://example.com", origin: origin)
        XCTAssertNotNil(expected)
        XCTAssertEqual(run.link, expected)
        // Links are painted with the accent color.
        XCTAssertNotNil(run.foreground)
    }

    // MARK: - Highlight

    func testPassiveHighlightPaintsMatchingRun() {
        let attributed = render([IR.text("look at the beacon here")], highlight: "beacon", active: false)
        let run = try! XCTUnwrap(attributed.runInfos.first { $0.text == "beacon" })
        XCTAssertNotNil(run.background)
    }

    func testActiveHighlightUsesStrongerBackgroundThanPassive() {
        let passive = render([IR.text("the beacon")], highlight: "beacon", active: false)
        let active = render([IR.text("the beacon")], highlight: "beacon", active: true)

        let passiveRun = try! XCTUnwrap(passive.runInfos.first { $0.text == "beacon" })
        let activeRun = try! XCTUnwrap(active.runInfos.first { $0.text == "beacon" })

        XCTAssertNotNil(passiveRun.background)
        XCTAssertNotNil(activeRun.background)
        // The active match uses a distinct, stronger background than a passive one.
        XCTAssertNotEqual(passiveRun.background, activeRun.background)
        // The active match also flips the foreground for contrast.
        XCTAssertNotNil(activeRun.foreground)
    }

    func testHighlightIsCaseInsensitive() {
        let attributed = render([IR.text("A BEACON shines")], highlight: "beacon", active: false)
        let run = try! XCTUnwrap(attributed.runInfos.first { $0.text == "BEACON" })
        XCTAssertNotNil(run.background)
    }

    func testEmptyHighlightLeavesTextUnhighlighted() {
        let attributed = render([IR.text("nothing here")], highlight: "", active: false)
        for run in attributed.runInfos {
            XCTAssertNil(run.background)
        }
    }

    // MARK: - URL resolution & anchors

    func testAnchorURLUsesAnchorSchemeAndSlugRoundTrips() {
        let url = try! XCTUnwrap(InlineRenderer.resolvedURL("#Core-Objectives", origin: origin))
        XCTAssertEqual(url.scheme, InlineRenderer.anchorScheme)
        // Implementation lowercases the slug.
        XCTAssertEqual(InlineRenderer.anchorSlug(from: url), "core-objectives")
    }

    func testAnchorSlugReturnsNilForNonAnchorURL() {
        let url = try! XCTUnwrap(URL(string: "https://example.com"))
        XCTAssertNil(InlineRenderer.anchorSlug(from: url))
    }

    func testAbsoluteURLIsReturnedUnchanged() {
        let url = try! XCTUnwrap(InlineRenderer.resolvedURL("https://example.com", origin: origin))
        XCTAssertEqual(url.scheme, "https")
        XCTAssertEqual(url.absoluteString, "https://example.com")
    }

    func testRelativeLinkResolvesAgainstFileOrigin() {
        let url = try! XCTUnwrap(InlineRenderer.resolvedURL("other.md", origin: origin))
        XCTAssertTrue(url.isFileURL)
        XCTAssertTrue(url.absoluteString.hasSuffix("other.md"))
        // Rooted against the origin's directory.
        XCTAssertEqual(url.standardizedFileURL.path, "/docs/other.md")
    }

    func testResolvedURLForEmptyOrNilIsNil() {
        XCTAssertNil(InlineRenderer.resolvedURL(nil, origin: origin))
        XCTAssertNil(InlineRenderer.resolvedURL("", origin: origin))
    }
}
