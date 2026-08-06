import AppKit
import XCTest
@testable import MDStarNative

@MainActor
final class AttributedDocumentBuilderTests: XCTestCase {
    private var settings: ReaderSettings!

    override func setUp() async throws {
        for key in [
            "mdstar.native.reader.fontSize",
            "mdstar.native.reader.fontFamily",
            "mdstar.native.reader.lineSpacing",
            "mdstar.native.reader.contentWidth",
        ] {
            UserDefaults.standard.removeObject(forKey: key)
        }
        settings = ReaderSettings()
    }

    private func compose(_ blocks: [BlockIR]) -> ComposedDocument {
        let document = DocumentIR(
            schemaVersion: 1,
            documentID: "doc-test",
            origin: "/tmp/test.md",
            blocks: blocks,
            outline: [],
            diagnostics: []
        )
        return AttributedDocumentBuilder(document: document, settings: settings).build()
    }

    /// The whole point of the TextKit reader: separate blocks share one string,
    /// so a selection can span them.
    func testBlocksShareASingleContiguousString() {
        let composed = compose([
            IR.heading("Introduction", level: 1),
            IR.paragraph("First paragraph."),
            IR.paragraph("Second paragraph."),
        ])

        XCTAssertTrue(composed.string.contains("Introduction"))
        XCTAssertTrue(composed.string.contains("First paragraph."))
        XCTAssertTrue(composed.string.contains("Second paragraph."))
        XCTAssertEqual(composed.blocks.count, 3)

        // A range spanning the first and last block is valid in one container.
        let first = composed.blocks[0].range
        let last = composed.blocks[2].range
        let span = NSRange(location: first.location, length: last.upperBound - first.location)
        XCTAssertLessThanOrEqual(span.upperBound, composed.text.length)
        let selected = (composed.string as NSString).substring(with: span)
        XCTAssertTrue(selected.contains("Introduction"))
        XCTAssertTrue(selected.contains("Second paragraph."))
    }

    func testBlockRangesAreOrderedAndNonOverlapping() {
        let composed = compose([
            IR.heading("A", level: 1),
            IR.paragraph("body one"),
            IR.heading("B", level: 2),
            IR.paragraph("body two"),
        ])

        for (previous, next) in zip(composed.blocks, composed.blocks.dropFirst()) {
            XCTAssertLessThanOrEqual(previous.range.upperBound, next.range.location)
        }
    }

    func testRangeLookupAndReverseLookupAgree() {
        let composed = compose([
            IR.heading("Title", level: 1),
            IR.paragraph("content here"),
        ])
        let target = composed.blocks[1]
        let range = try? XCTUnwrap(composed.range(for: target.id))
        XCTAssertEqual(range, target.range)
        XCTAssertEqual(composed.blockID(containing: target.range.location), target.id)
    }

    func testHeadingIsLargerThanBody() {
        let composed = compose([IR.heading("Big", level: 1), IR.paragraph("small")])
        let headingRange = composed.blocks[0].range
        let bodyRange = composed.blocks[1].range

        let headingFont = composed.text.attribute(.font, at: headingRange.location, effectiveRange: nil) as? NSFont
        let bodyFont = composed.text.attribute(.font, at: bodyRange.location, effectiveRange: nil) as? NSFont
        XCTAssertNotNil(headingFont)
        XCTAssertNotNil(bodyFont)
        XCTAssertGreaterThan(headingFont!.pointSize, bodyFont!.pointSize)
    }

    func testLinkAttributeIsApplied() {
        let link = InlineIR(
            id: "i1", range: nil, kind: "link",
            text: nil,
            children: [IR.text("docs")],
            url: "https://example.com", title: nil, alt: nil
        )
        let composed = compose([IR.paragraph(inlines: [link])])
        let location = composed.string.range(of: "docs").map {
            composed.string.distance(from: composed.string.startIndex, to: $0.lowerBound)
        }
        let index = try? XCTUnwrap(location)
        let url = composed.text.attribute(.link, at: index!, effectiveRange: nil) as? URL
        XCTAssertEqual(url?.absoluteString, "https://example.com")
    }

    func testEveryRunCarriesItsBlockIdentifier() {
        let composed = compose([IR.heading("H", level: 2), IR.paragraph("text")])
        for block in composed.blocks {
            let id = composed.text.attribute(.mdstarBlockID, at: block.range.location, effectiveRange: nil) as? String
            XCTAssertEqual(id, block.id)
        }
    }

    func testTaskListMarkersRender() {
        let done = ListItemIR(id: "li1", range: nil, checked: true, children: [IR.paragraph("done item")])
        let todo = ListItemIR(id: "li2", range: nil, checked: false, children: [IR.paragraph("todo item")])
        let composed = compose([IR.list(items: [done, todo])])
        XCTAssertTrue(composed.string.contains("done item"))
        XCTAssertTrue(composed.string.contains("todo item"))
        // Checkboxes are SF Symbol attachments, which appear in the string as
        // the object-replacement character rather than a drawn glyph.
        XCTAssertTrue(
            composed.string.contains("\u{FFFC}"),
            "task items need a checkbox attachment"
        )
    }

    func testTableCellsRemainSelectableText() {
        let composed = compose([
            IR.table(
                headers: [[IR.text("Name")], [IR.text("Value")]],
                rows: [[[IR.text("alpha")], [IR.text("1")]]]
            )
        ])
        // Cells are real text, not an opaque attachment.
        XCTAssertTrue(composed.string.contains("Name"))
        XCTAssertTrue(composed.string.contains("alpha"))
    }

    func testMultiBlockSelectionProducesOrderedAnnotationSegments() {
        let composed = compose([
            IR.heading("Title", level: 1),
            IR.paragraph("First section."),
            IR.paragraph("Second section."),
        ])
        let start = composed.blocks[1].range.location + 6
        let end = composed.blocks[2].range.location + 6
        let segments = composed.annotationSegments(
            for: NSRange(location: start, length: end - start)
        )

        XCTAssertEqual(segments.map(\.blockID), [composed.blocks[1].id, composed.blocks[2].id])
        XCTAssertEqual(segments.map(\.length).reduce(0, +), end - start)
        XCTAssertEqual(composed.range(for: segments[0]), segments[0].range)
    }
}

// MARK: - Annotation anchoring

@MainActor
final class AnnotationAnchoringTests: XCTestCase {
    func testResolvesAtStoredOffsetWhenTextIsUnchanged() {
        let text = "The quick brown fox"
        let range = (text as NSString).range(of: "quick")
        let annotation = Annotation(kind: .highlight, range: range, snippet: "quick")
        XCTAssertEqual(annotation.resolvedRange(in: text), range)
    }

    /// Offsets alone are brittle; the snippet is the fallback anchor.
    func testRecoversWhenTextShiftsEarlier() {
        let original = "The quick brown fox"
        let range = (original as NSString).range(of: "quick")
        let annotation = Annotation(kind: .highlight, range: range, snippet: "quick")

        let shifted = "Prefix added. The quick brown fox"
        let resolved = annotation.resolvedRange(in: shifted)
        XCTAssertNotEqual(resolved.location, NSNotFound)
        XCTAssertEqual((shifted as NSString).substring(with: resolved), "quick")
    }

    func testReturnsNotFoundWhenSnippetIsGone() {
        let annotation = Annotation(
            kind: .comment,
            range: NSRange(location: 4, length: 5),
            snippet: "absent"
        )
        XCTAssertEqual(annotation.resolvedRange(in: "totally different text").location, NSNotFound)
    }

    func testRoundTripsThroughJSON() throws {
        let annotation = Annotation(
            kind: .comment,
            range: NSRange(location: 3, length: 7),
            snippet: "passage",
            note: "worth revisiting",
            blockID: "block-1"
        )
        let data = try JSONEncoder().encode(annotation)
        let decoded = try JSONDecoder().decode(Annotation.self, from: data)
        XCTAssertEqual(decoded.id, annotation.id)
        XCTAssertEqual(decoded.kind, .comment)
        XCTAssertEqual(decoded.snippet, "passage")
        XCTAssertEqual(decoded.note, "worth revisiting")
        XCTAssertEqual(decoded.blockID, "block-1")
        XCTAssertEqual(decoded.range, annotation.range)
    }

    func testLegacyAnnotationWithoutSegmentsStillDecodes() throws {
        let legacy = """
        {"id":"11111111-1111-1111-1111-111111111111","kind":"highlight","location":4,"length":5,"snippet":"quick","note":"","createdAt":0,"blockID":"block-1"}
        """.data(using: .utf8)!
        let decoded = try JSONDecoder().decode(Annotation.self, from: legacy)
        XCTAssertNil(decoded.segments)
        XCTAssertEqual(decoded.resolvedRange(in: "The quick brown fox"), NSRange(location: 4, length: 5))
    }

    func testSegmentedAnnotationRoundTripsThroughJSON() throws {
        let annotation = Annotation(
            kind: .comment,
            range: NSRange(location: 2, length: 12),
            snippet: "spans blocks",
            blockID: "first",
            segments: [
                AnnotationSegment(blockID: "first", location: 2, length: 5),
                AnnotationSegment(blockID: "second", location: 7, length: 7),
            ]
        )
        let decoded = try JSONDecoder().decode(Annotation.self, from: JSONEncoder().encode(annotation))
        XCTAssertEqual(decoded.segments, annotation.segments)
    }
}
