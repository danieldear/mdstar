import SwiftUI
import XCTest
@testable import MDStarNative

/// Small builders that keep IR construction in tests readable. Every stored
/// field of the memberwise initializers is filled explicitly (using `nil`/`[]`
/// where a node does not use it) so the fixtures track the real struct shapes.
enum IR {
    /// A plain inline text node.
    static func text(_ value: String, id: String = "text") -> InlineIR {
        InlineIR(id: id, range: nil, kind: "text", text: value, children: [], url: nil, title: nil, alt: nil)
    }

    /// A generic inline node (strong/emphasis/code/link/delete/image/...).
    static func inline(
        _ kind: String,
        id: String = "inline",
        text: String? = nil,
        children: [InlineIR] = [],
        url: String? = nil,
        alt: String? = nil
    ) -> InlineIR {
        InlineIR(id: id, range: nil, kind: kind, text: text, children: children, url: url, title: nil, alt: alt)
    }

    /// A generic block node with all list/table/code fields defaulted empty.
    static func block(
        _ kind: String,
        id: String = "block",
        inlines: [InlineIR] = [],
        children: [BlockIR] = [],
        code: String? = nil,
        headers: [[InlineIR]] = [],
        rows: [[[InlineIR]]] = [],
        raw: String? = nil,
        items: [ListItemIR] = []
    ) -> BlockIR {
        BlockIR(
            id: id,
            range: nil,
            kind: kind,
            level: nil,
            inlines: inlines,
            children: children,
            ordered: nil,
            start: nil,
            items: items,
            language: nil,
            meta: nil,
            code: code,
            headers: headers,
            rows: rows,
            raw: raw
        )
    }
}

/// Flattened, easy-to-assert view of an `AttributedString`'s runs. Only SwiftUI
/// and Foundation attribute scopes are imported in this test module, so these
/// dynamic-member accessors resolve unambiguously (SwiftUI `Color`/`Font`,
/// Foundation `URL`).
struct RunInfo {
    let text: String
    let font: Font?
    let foreground: Color?
    let background: Color?
    let link: URL?
    let strikethrough: Text.LineStyle?
}

extension AttributedString {
    var runInfos: [RunInfo] {
        runs.map { run in
            RunInfo(
                text: String(self[run.range].characters),
                font: run.font,
                foreground: run.foregroundColor,
                background: run.backgroundColor,
                link: run.link,
                strikethrough: run.strikethroughStyle
            )
        }
    }

    /// The first run whose visible text contains `needle`.
    func run(containing needle: String) -> RunInfo? {
        runInfos.first { $0.text.contains(needle) }
    }
}
