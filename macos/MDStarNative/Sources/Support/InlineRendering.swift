import SwiftUI

/// Styling state that accumulates as we descend through nested inline nodes
/// (e.g. bold inside a link inside emphasis).
private struct InlineStyle {
    var bold = false
    var italic = false
    var strikethrough = false
    var code = false
    var link: URL?
}

/// Builds a fully-styled `AttributedString` from the semantic inline IR.
///
/// This is the piece the original port was missing: `strong`, `emphasis`,
/// `delete`, inline `code`, and `link` are all applied here instead of being
/// flattened to plain text.
enum InlineRenderer {
    static func attributedString(
        _ inlines: [InlineIR],
        base: Font = .body,
        origin: String,
        highlight: String? = nil,
        highlightIsActive: Bool = false
    ) -> AttributedString {
        var result = render(inlines, base: base, style: InlineStyle(), origin: origin)
        if let highlight, !highlight.isEmpty {
            applyHighlight(&result, query: highlight, active: highlightIsActive)
        }
        return result
    }

    /// Paints a background over every case-insensitive occurrence of `query`.
    private static func applyHighlight(_ attributed: inout AttributedString, query: String, active: Bool) {
        let haystack = String(attributed.characters)
        guard !haystack.isEmpty else { return }
        let passive = Color.yellow.opacity(0.55)
        let strong = Color.orange.opacity(0.85)

        var searchStart = haystack.startIndex
        while let range = haystack.range(of: query, options: .caseInsensitive, range: searchStart..<haystack.endIndex) {
            let startOffset = haystack.distance(from: haystack.startIndex, to: range.lowerBound)
            let length = haystack.distance(from: range.lowerBound, to: range.upperBound)
            let aStart = attributed.index(attributed.startIndex, offsetByCharacters: startOffset)
            let aEnd = attributed.index(aStart, offsetByCharacters: length)
            attributed[aStart..<aEnd].backgroundColor = active ? strong : passive
            if active { attributed[aStart..<aEnd].foregroundColor = .black }
            searchStart = range.upperBound
        }
    }

    private static func render(
        _ inlines: [InlineIR],
        base: Font,
        style: InlineStyle,
        origin: String
    ) -> AttributedString {
        var output = AttributedString()
        for inline in inlines {
            switch inline.kind {
            case "text", "html":
                output += styledRun(inline.text ?? "", base: base, style: style)

            case "strong":
                var next = style; next.bold = true
                output += render(inline.children, base: base, style: next, origin: origin)

            case "emphasis":
                var next = style; next.italic = true
                output += render(inline.children, base: base, style: next, origin: origin)

            case "delete":
                var next = style; next.strikethrough = true
                output += render(inline.children, base: base, style: next, origin: origin)

            case "code", "math":
                var next = style; next.code = true
                output += styledRun(inline.text ?? "", base: base, style: next)

            case "link":
                var next = style
                next.link = resolvedURL(inline.url, origin: origin)
                output += render(inline.children, base: base, style: next, origin: origin)

            case "image":
                // Inline images fall back to their alt text; block-level images
                // are rendered as real pictures by DocumentRenderer.
                if let alt = inline.alt, !alt.isEmpty {
                    output += styledRun(alt, base: base, style: style)
                }

            case "hard_break":
                output += AttributedString("\n")

            default:
                output += render(inline.children, base: base, style: style, origin: origin)
            }
        }
        return output
    }

    private static func styledRun(_ text: String, base: Font, style: InlineStyle) -> AttributedString {
        guard !text.isEmpty else { return AttributedString() }
        var run = AttributedString(text)
        run.font = composedFont(base: base, style: style)
        if style.strikethrough {
            run.strikethroughStyle = .single
            run.foregroundColor = .secondary
        }
        if style.code {
            run.backgroundColor = .readerStrongFill
        }
        if let link = style.link {
            run.link = link
            run.foregroundColor = .accentColor
        }
        return run
    }

    private static func composedFont(base: Font, style: InlineStyle) -> Font {
        var font = style.code ? base.monospaced() : base
        if style.bold { font = font.bold() }
        if style.italic { font = font.italic() }
        return font
    }

    /// URL scheme used to mark in-page anchor links (`[x](#heading)`) so the
    /// renderer can scroll instead of navigating.
    static let anchorScheme = "mdstaranchor"

    /// Extracts the heading slug from an anchor URL, if it is one.
    static func anchorSlug(from url: URL) -> String? {
        guard url.scheme == anchorScheme else { return nil }
        let raw = url.absoluteString.dropFirst("\(anchorScheme):".count)
        return raw.removingPercentEncoding ?? String(raw)
    }

    /// Resolves link targets, keeping absolute URLs intact and rooting relative
    /// links against the document's origin directory.
    static func resolvedURL(_ raw: String?, origin: String) -> URL? {
        guard let raw, !raw.isEmpty else { return nil }
        if raw.hasPrefix("#") {
            let slug = String(raw.dropFirst()).lowercased()
            let encoded = slug.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? slug
            return URL(string: "\(anchorScheme):\(encoded)")
        }
        if let absolute = URL(string: raw), absolute.scheme != nil { return absolute }
        guard origin.hasPrefix("/") || origin.hasPrefix("file://") else { return URL(string: raw) }
        let originURL = origin.hasPrefix("file://")
            ? URL(string: origin)
            : URL(fileURLWithPath: origin)
        return URL(fileURLWithPath: raw, relativeTo: originURL?.deletingLastPathComponent())
    }
}
