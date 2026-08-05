import AppKit

/// Where a block landed in the composed text, so the reader can scroll to it and
/// report which section is on screen.
struct BlockRange: Equatable {
    let id: String
    let kind: String
    let level: Int?
    let range: NSRange
}

/// The result of composing a document: one attributed string plus the index
/// needed to navigate it.
struct ComposedDocument {
    let text: NSAttributedString
    let blocks: [BlockRange]

    /// Plain text, used for find and for annotation snapshots.
    var string: String { text.string }

    func range(for blockID: String) -> NSRange? {
        blocks.first { $0.id == blockID }?.range
    }

    func blockID(containing location: Int) -> String? {
        blocks.last { $0.range.location <= location }?.id
    }
}

/// Attribute key marking which block a run belongs to. Lets the view resolve a
/// click or a selection back to a semantic block without a separate lookup.
extension NSAttributedString.Key {
    static let mdstarBlockID = NSAttributedString.Key("mdstarBlockID")
}

/// Renders `DocumentIR` into a single `NSAttributedString`.
///
/// The SwiftUI renderer drew each block as its own `Text`, which made every
/// block an isolated selection island — text could not be selected across
/// blocks and no selected range was observable. Composing one attributed string
/// solves both, and is what highlights and comments anchor to.
@MainActor
struct AttributedDocumentBuilder {
    let document: DocumentIR
    let settings: ReaderSettings

    private var textColor: NSColor { .labelColor }
    private var secondaryColor: NSColor { .secondaryLabelColor }

    func build() -> ComposedDocument {
        let output = NSMutableAttributedString()
        var blocks: [BlockRange] = []

        for block in document.blocks {
            let start = output.length
            append(block: block, to: output, indent: 0)
            let length = output.length - start
            guard length > 0 else { continue }
            let range = NSRange(location: start, length: length)
            blocks.append(
                BlockRange(id: block.id, kind: block.kind, level: block.level, range: range)
            )
            output.addAttribute(.mdstarBlockID, value: block.id, range: range)
        }

        return ComposedDocument(text: output, blocks: blocks)
    }

    // MARK: - Blocks

    private func append(block: BlockIR, to output: NSMutableAttributedString, indent: CGFloat) {
        switch block.kind {
        case "heading":
            appendHeading(block, to: output, indent: indent)
        case "paragraph":
            appendParagraph(block, to: output, indent: indent)
        case "blockquote":
            appendBlockquote(block, to: output, indent: indent)
        case "list":
            appendList(block, to: output, indent: indent)
        case "code":
            appendCode(block.code ?? "", language: block.language, to: output, indent: indent)
        case "table":
            appendTable(block, to: output, indent: indent)
        case "thematic_break":
            appendThematicBreak(to: output)
        case "frontmatter", "math", "html":
            appendCode(block.raw ?? "", language: block.kind, to: output, indent: indent)
        default:
            for child in block.children { append(block: child, to: output, indent: indent) }
        }
    }

    private func appendHeading(_ block: BlockIR, to output: NSMutableAttributedString, indent: CGFloat) {
        let level = block.level ?? 1
        let paragraph = NSMutableParagraphStyle()
        paragraph.paragraphSpacingBefore = level <= 1 ? 12 : 22
        paragraph.paragraphSpacing = 8
        paragraph.firstLineHeadIndent = indent
        paragraph.headIndent = indent

        let inline = inlineText(
            block.inlines,
            base: settings.nsHeadingFont(level: level),
            paragraph: paragraph
        )
        output.append(inline)
        output.append(newline(paragraph))
    }

    private func appendParagraph(_ block: BlockIR, to output: NSMutableAttributedString, indent: CGFloat) {
        // Images are promoted out of the text flow so they render as pictures.
        let images = collectImages(block.inlines)
        let textual = removingImages(block.inlines)

        if !plainText(textual).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            let paragraph = bodyParagraphStyle(indent: indent)
            output.append(inlineText(textual, base: settings.nsBodyFont, paragraph: paragraph))
            output.append(newline(paragraph))
        }

        for image in images {
            appendImage(image, to: output, indent: indent)
        }
    }

    private func appendBlockquote(_ block: BlockIR, to output: NSMutableAttributedString, indent: CGFloat) {
        let start = output.length
        for child in block.children {
            append(block: child, to: output, indent: indent + 18)
        }
        guard output.length > start else { return }
        // Tint the quoted run so the indent reads as a quotation.
        output.addAttribute(
            .foregroundColor,
            value: secondaryColor,
            range: NSRange(location: start, length: output.length - start)
        )
    }

    private func appendList(_ block: BlockIR, to output: NSMutableAttributedString, indent: CGFloat) {
        let ordered = block.ordered ?? false
        let start = block.start ?? 1

        for (offset, item) in block.items.enumerated() {
            let marker: String?
            if item.checked != nil {
                // Task boxes are drawn with SF Symbols rather than box-drawing
                // characters, which rendered as mismatched glyphs.
                marker = nil
            } else if ordered {
                marker = "\(start + offset). "
            } else {
                marker = "\u{2022} "
            }

            let paragraph = bodyParagraphStyle(indent: indent + 20)
            paragraph.firstLineHeadIndent = indent
            paragraph.paragraphSpacing = 4

            if let marker {
                output.append(NSAttributedString(
                    string: marker,
                    attributes: [
                        .font: settings.nsBodyFont,
                        .foregroundColor: secondaryColor,
                        .paragraphStyle: paragraph,
                    ]
                ))
            } else if let checked = item.checked {
                output.append(checkboxAttachment(checked: checked, paragraph: paragraph))
            }

            // The first child paragraph continues the marker's line; anything
            // further (nested lists, extra paragraphs) starts below it.
            var isFirst = true
            for child in item.children {
                if isFirst, child.kind == "paragraph" {
                    let textual = removingImages(child.inlines)
                    output.append(inlineText(textual, base: settings.nsBodyFont, paragraph: paragraph))
                    output.append(newline(paragraph))
                    isFirst = false
                } else {
                    append(block: child, to: output, indent: indent + 20)
                }
            }
            if isFirst { output.append(newline(paragraph)) }
        }
    }

    /// A task checkbox drawn as an SF Symbol image so it matches the rest of
    /// the system UI at any reading size.
    private func checkboxAttachment(checked: Bool, paragraph: NSParagraphStyle) -> NSAttributedString {
        let name = checked ? "checkmark.circle.fill" : "circle"
        let size = settings.fontSize
        let configuration = NSImage.SymbolConfiguration(pointSize: size, weight: .regular)

        let output = NSMutableAttributedString()
        if let symbol = NSImage(systemSymbolName: name, accessibilityDescription: checked ? "Completed" : "Not completed")?
            .withSymbolConfiguration(configuration) {
            if checked {
                symbol.isTemplate = true
            }
            let attachment = NSTextAttachment()
            attachment.image = symbol
            // Nudge down so the box sits on the text baseline.
            attachment.bounds = CGRect(
                x: 0,
                y: -size * 0.14,
                width: symbol.size.width,
                height: symbol.size.height
            )
            let attachmentString = NSMutableAttributedString(attachment: attachment)
            attachmentString.addAttributes(
                [
                    .paragraphStyle: paragraph,
                    .foregroundColor: checked ? NSColor.controlAccentColor : NSColor.tertiaryLabelColor,
                ],
                range: NSRange(location: 0, length: attachmentString.length)
            )
            output.append(attachmentString)
        }
        output.append(NSAttributedString(
            string: "  ",
            attributes: [.font: settings.nsBodyFont, .paragraphStyle: paragraph]
        ))
        return output
    }

    private func appendCode(_ code: String, language: String?, to output: NSMutableAttributedString, indent: CGFloat) {
        let paragraph = NSMutableParagraphStyle()
        paragraph.paragraphSpacingBefore = 8
        paragraph.paragraphSpacing = 12
        paragraph.firstLineHeadIndent = indent + 12
        paragraph.headIndent = indent + 12
        paragraph.lineSpacing = 2

        let highlighted = NSMutableAttributedString(
            attributedString: SyntaxHighlighter.highlightForAppKit(code, language: language, font: settings.nsCodeFont)
        )
        let full = NSRange(location: 0, length: highlighted.length)
        // No run background here: a flat fill behind the glyphs looked like a
        // highlight rather than a code card. ReaderTextView draws a rounded
        // card behind the whole block instead.
        highlighted.addAttribute(.paragraphStyle, value: paragraph, range: full)
        output.append(highlighted)
        output.append(newline(paragraph))
    }

    /// Tables use `NSTextTable`, so cells remain real selectable text rather
    /// than an opaque attachment.
    private func appendTable(_ block: BlockIR, to output: NSMutableAttributedString, indent: CGFloat) {
        let columnCount = max(block.headers.count, block.rows.map(\.count).max() ?? 0)
        guard columnCount > 0 else { return }

        let table = NSTextTable()
        table.numberOfColumns = columnCount
        table.layoutAlgorithm = .automaticLayoutAlgorithm
        table.collapsesBorders = true
        table.hidesEmptyCells = false

        var rowIndex = 0
        if !block.headers.isEmpty {
            appendTableRow(block.headers, table: table, row: rowIndex, columnCount: columnCount,
                           isHeader: true, to: output, indent: indent)
            rowIndex += 1
        }
        for row in block.rows {
            appendTableRow(row, table: table, row: rowIndex, columnCount: columnCount,
                           isHeader: false, to: output, indent: indent)
            rowIndex += 1
        }

        let spacer = NSMutableParagraphStyle()
        spacer.paragraphSpacing = 12
        output.append(newline(spacer))
    }

    private func appendTableRow(
        _ cells: [[InlineIR]],
        table: NSTextTable,
        row: Int,
        columnCount: Int,
        isHeader: Bool,
        to output: NSMutableAttributedString,
        indent: CGFloat
    ) {
        for column in 0..<columnCount {
            let cellBlock = NSTextTableBlock(
                table: table,
                startingRow: row,
                rowSpan: 1,
                startingColumn: column,
                columnSpan: 1
            )
            cellBlock.setBorderColor(NSColor.separatorColor)
            cellBlock.setWidth(1, type: .absoluteValueType, for: .border)
            cellBlock.setWidth(6, type: .absoluteValueType, for: .padding)
            if isHeader {
                cellBlock.backgroundColor = NSColor.labelColor.withAlphaComponent(0.07)
            }

            let paragraph = NSMutableParagraphStyle()
            paragraph.textBlocks = [cellBlock]
            paragraph.headIndent = indent
            paragraph.firstLineHeadIndent = indent

            let inlines = cells.indices.contains(column) ? cells[column] : []
            let base = isHeader
                ? settings.nsFont(settings.nsSecondaryFont, bold: true, italic: false)
                : settings.nsSecondaryFont
            let cellText = NSMutableAttributedString(
                attributedString: inlineText(inlines, base: base, paragraph: paragraph)
            )
            if cellText.length == 0 {
                cellText.append(NSAttributedString(
                    string: " ",
                    attributes: [.font: base, .paragraphStyle: paragraph]
                ))
            }
            cellText.append(newline(paragraph))
            output.append(cellText)
        }
    }

    private func appendThematicBreak(to output: NSMutableAttributedString) {
        let paragraph = NSMutableParagraphStyle()
        paragraph.paragraphSpacingBefore = 10
        paragraph.paragraphSpacing = 10
        // A run of box-drawing characters reads as a rule and stays selectable.
        let rule = NSAttributedString(
            string: String(repeating: "\u{2500}", count: 40),
            attributes: [
                .font: settings.nsSecondaryFont,
                .foregroundColor: NSColor.separatorColor,
                .paragraphStyle: paragraph,
            ]
        )
        output.append(rule)
        output.append(newline(paragraph))
    }

    private func appendImage(_ inline: InlineIR, to output: NSMutableAttributedString, indent: CGFloat) {
        let paragraph = bodyParagraphStyle(indent: indent)

        guard let url = InlineRenderer.resolvedURL(inline.url, origin: document.origin),
              url.isFileURL,
              let image = NSImage(contentsOf: url) else {
            let alt = inline.alt?.isEmpty == false ? inline.alt! : "Image unavailable"
            output.append(NSAttributedString(
                string: "\u{1F5BC} \(alt)",
                attributes: [
                    .font: settings.nsSecondaryFont,
                    .foregroundColor: secondaryColor,
                    .paragraphStyle: paragraph,
                ]
            ))
            output.append(newline(paragraph))
            return
        }

        // Scale to the reading measure while preserving aspect ratio.
        let maxWidth = CGFloat(settings.contentWidth)
        let size = image.size
        let scale = size.width > maxWidth ? maxWidth / size.width : 1
        let attachment = NSTextAttachment()
        attachment.image = image
        attachment.bounds = CGRect(
            x: 0,
            y: 0,
            width: (size.width * scale).rounded(),
            height: (size.height * scale).rounded()
        )

        let attachmentString = NSMutableAttributedString(attachment: attachment)
        attachmentString.addAttribute(
            .paragraphStyle,
            value: paragraph,
            range: NSRange(location: 0, length: attachmentString.length)
        )
        output.append(attachmentString)
        output.append(newline(paragraph))
    }

    // MARK: - Inlines

    private func inlineText(
        _ inlines: [InlineIR],
        base: NSFont,
        paragraph: NSParagraphStyle
    ) -> NSAttributedString {
        let output = NSMutableAttributedString()
        render(inlines, base: base, paragraph: paragraph, style: InlineStyle(), into: output)
        return output
    }

    private struct InlineStyle {
        var bold = false
        var italic = false
        var strikethrough = false
        var code = false
        var link: URL?
    }

    private func render(
        _ inlines: [InlineIR],
        base: NSFont,
        paragraph: NSParagraphStyle,
        style: InlineStyle,
        into output: NSMutableAttributedString
    ) {
        for inline in inlines {
            switch inline.kind {
            case "text", "html":
                append(text: inline.text ?? "", base: base, paragraph: paragraph, style: style, into: output)
            case "strong":
                var next = style; next.bold = true
                render(inline.children, base: base, paragraph: paragraph, style: next, into: output)
            case "emphasis":
                var next = style; next.italic = true
                render(inline.children, base: base, paragraph: paragraph, style: next, into: output)
            case "delete":
                var next = style; next.strikethrough = true
                render(inline.children, base: base, paragraph: paragraph, style: next, into: output)
            case "code", "math":
                var next = style; next.code = true
                append(text: inline.text ?? "", base: base, paragraph: paragraph, style: next, into: output)
            case "link":
                var next = style
                next.link = InlineRenderer.resolvedURL(inline.url, origin: document.origin)
                render(inline.children, base: base, paragraph: paragraph, style: next, into: output)
            case "image":
                if let alt = inline.alt, !alt.isEmpty {
                    append(text: alt, base: base, paragraph: paragraph, style: style, into: output)
                }
            case "hard_break":
                output.append(NSAttributedString(
                    string: "\n",
                    attributes: [.font: base, .paragraphStyle: paragraph]
                ))
            default:
                render(inline.children, base: base, paragraph: paragraph, style: style, into: output)
            }
        }
    }

    private func append(
        text: String,
        base: NSFont,
        paragraph: NSParagraphStyle,
        style: InlineStyle,
        into output: NSMutableAttributedString
    ) {
        guard !text.isEmpty else { return }

        let font = style.code
            ? NSFont.monospacedSystemFont(ofSize: base.pointSize - 1, weight: .regular)
            : settings.nsFont(base, bold: style.bold, italic: style.italic)

        var attributes: [NSAttributedString.Key: Any] = [
            .font: font,
            .paragraphStyle: paragraph,
            .foregroundColor: textColor,
        ]
        if style.strikethrough {
            attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            attributes[.foregroundColor] = secondaryColor
        }
        if style.code {
            attributes[.backgroundColor] = NSColor.labelColor.withAlphaComponent(0.08)
        }
        if let link = style.link {
            attributes[.link] = link
            attributes[.foregroundColor] = NSColor.linkColor
            attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
        }

        output.append(NSAttributedString(string: text, attributes: attributes))
    }

    // MARK: - Helpers

    private func bodyParagraphStyle(indent: CGFloat) -> NSMutableParagraphStyle {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = CGFloat(settings.lineSpacing)
        paragraph.paragraphSpacing = 10
        paragraph.firstLineHeadIndent = indent
        paragraph.headIndent = indent
        return paragraph
    }

    private func newline(_ paragraph: NSParagraphStyle) -> NSAttributedString {
        NSAttributedString(
            string: "\n",
            attributes: [.font: settings.nsBodyFont, .paragraphStyle: paragraph]
        )
    }

    private func plainText(_ inlines: [InlineIR]) -> String {
        inlines.map(\.plainText).joined()
    }

    private func collectImages(_ inlines: [InlineIR]) -> [InlineIR] {
        inlines.flatMap { inline -> [InlineIR] in
            inline.kind == "image" ? [inline] : collectImages(inline.children)
        }
    }

    private func removingImages(_ inlines: [InlineIR]) -> [InlineIR] {
        inlines.compactMap { inline in
            if inline.kind == "image" { return nil }
            return InlineIR(
                id: inline.id,
                range: inline.range,
                kind: inline.kind,
                text: inline.text,
                children: removingImages(inline.children),
                url: inline.url,
                title: inline.title,
                alt: inline.alt
            )
        }
    }
}
