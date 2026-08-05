import XCTest
@testable import MDStarNative

final class LinkResolverTests: XCTestCase {
    private let origin = "/workspace/docs/guide.md"

    func testAbsoluteWebURLPassesThrough() {
        let url = LinkResolver.resolvedURL("https://example.com/a", origin: origin)
        XCTAssertEqual(url?.absoluteString, "https://example.com/a")
    }

    func testRelativePathResolvesAgainstTheDocumentDirectory() {
        let url = LinkResolver.resolvedURL("images/logo.png", origin: origin)
        XCTAssertEqual(url?.path, "/workspace/docs/images/logo.png")
    }

    func testParentRelativePathIsNormalised() {
        let url = LinkResolver.resolvedURL("../other.md", origin: origin)
        XCTAssertEqual(url?.path, "/workspace/other.md")
    }

    /// Markdown commonly percent-encodes spaces in local paths.
    func testPercentEncodedSpacesAreDecoded() {
        let url = LinkResolver.resolvedURL("my%20file.md", origin: origin)
        XCTAssertEqual(url?.path, "/workspace/docs/my file.md")
    }

    func testAbsolutePathIsKept() {
        let url = LinkResolver.resolvedURL("/etc/notes.md", origin: origin)
        XCTAssertEqual(url?.path, "/etc/notes.md")
    }

    func testTildeIsExpanded() {
        let url = LinkResolver.resolvedURL("~/notes.md", origin: origin)
        XCTAssertEqual(url?.path, NSString(string: "~/notes.md").expandingTildeInPath)
    }

    func testAnchorBecomesAnAnchorURL() {
        let url = LinkResolver.resolvedURL("#Core-Objectives", origin: origin)
        XCTAssertEqual(url?.scheme, LinkResolver.anchorScheme)
        XCTAssertEqual(url.flatMap(LinkResolver.anchorSlug(from:)), "core-objectives")
    }

    func testAnchorSlugIgnoresOrdinaryURLs() {
        let url = try? XCTUnwrap(URL(string: "https://example.com"))
        XCTAssertNil(url.flatMap(LinkResolver.anchorSlug(from:)))
    }

    func testEmptyAndNilAreRejected() {
        XCTAssertNil(LinkResolver.resolvedURL(nil, origin: origin))
        XCTAssertNil(LinkResolver.resolvedURL("", origin: origin))
    }
}
