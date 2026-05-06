import Foundation

func renderMarkdown(_ source: String) -> String {
    let lines = source.components(separatedBy: "\n")
    var blocks: [String] = []
    var i = 0

    while i < lines.count {
        let line = lines[i]
        let trimmed = line.trimmingCharacters(in: .whitespaces)

        // Blank line — separator only
        if trimmed.isEmpty {
            i += 1
            continue
        }

        // ATX heading
        if let h = parseHeading(line) {
            blocks.append(h)
            i += 1
            continue
        }

        // GFM table
        if i + 1 < lines.count,
           let headers = parseTableRow(line),
           isTableSeparator(lines[i + 1], expectedColumns: headers.count) {
            i += 2
            var rows: [[String]] = []
            while i < lines.count {
                let rowLine = lines[i]
                if rowLine.trimmingCharacters(in: .whitespaces).isEmpty {
                    break
                }
                guard let row = parseTableRow(rowLine) else {
                    break
                }
                rows.append(normalizeTableRow(row, to: headers.count))
                i += 1
            }
            blocks.append(renderTable(headers: headers, rows: rows))
            continue
        }

        // Fenced code block
        let fencePrefix = line.hasPrefix("```") ? "```" : line.hasPrefix("~~~") ? "~~~" : nil
        if let fence = fencePrefix {
            let lang = String(line.dropFirst(3)).trimmingCharacters(in: .whitespaces)
            var codeLines: [String] = []
            i += 1
            while i < lines.count && !lines[i].hasPrefix(fence) {
                codeLines.append(lines[i])
                i += 1
            }
            i += 1
            let code = codeLines.joined(separator: "\n")
            if lang.caseInsensitiveCompare("mermaid") == .orderedSame {
                blocks.append("<pre class=\"mermaid\">\(escapeHTML(code))</pre>")
            } else {
                let langAttr = lang.isEmpty ? "" : " class=\"language-\(escapeHTML(lang))\""
                blocks.append("<pre><code\(langAttr)>\(escapeHTML(code))</code></pre>")
            }
            continue
        }

        // Horizontal rule
        if isHorizontalRule(trimmed) {
            blocks.append("<hr>")
            i += 1
            continue
        }

        // Blockquote
        if line.hasPrefix(">") {
            var quoteLines: [String] = []
            while i < lines.count && lines[i].hasPrefix(">") {
                let stripped = lines[i].hasPrefix("> ")
                    ? String(lines[i].dropFirst(2))
                    : String(lines[i].dropFirst(1))
                quoteLines.append(stripped)
                i += 1
            }
            let inner = renderMarkdown(quoteLines.joined(separator: "\n"))
            blocks.append("<blockquote>\(inner)</blockquote>")
            continue
        }

        // Unordered list
        if isUnorderedListItem(line) {
            var items: [String] = []
            while i < lines.count && isUnorderedListItem(lines[i]) {
                let content = lines[i].replacingFirst(of: #"^[\-\*\+]\s+"#, with: "")
                if let (checked, label) = parseTaskItem(content) {
                    let checkedAttr = checked ? " checked" : ""
                    items.append("<li><input type=\"checkbox\" disabled\(checkedAttr)> \(renderInline(label))</li>")
                } else {
                    items.append("<li>\(renderInline(content))</li>")
                }
                i += 1
            }
            blocks.append("<ul>\n\(items.joined(separator: "\n"))\n</ul>")
            continue
        }

        // Ordered list
        if isOrderedListItem(line) {
            var items: [String] = []
            while i < lines.count && isOrderedListItem(lines[i]) {
                let content = lines[i].replacingFirst(of: #"^\d+\.\s+"#, with: "")
                if let (checked, label) = parseTaskItem(content) {
                    let checkedAttr = checked ? " checked" : ""
                    items.append("<li><input type=\"checkbox\" disabled\(checkedAttr)> \(renderInline(label))</li>")
                } else {
                    items.append("<li>\(renderInline(content))</li>")
                }
                i += 1
            }
            blocks.append("<ol>\n\(items.joined(separator: "\n"))\n</ol>")
            continue
        }

        // Paragraph — consume until a blank line or a block-level element starts
        var paraLines: [String] = []
        while i < lines.count {
            let l = lines[i]
            let t = l.trimmingCharacters(in: .whitespaces)
            if t.isEmpty { break }
            if parseHeading(l) != nil { break }
            if l.hasPrefix("```") || l.hasPrefix("~~~") { break }
            if isHorizontalRule(t) { break }
            if l.hasPrefix(">") { break }
            if isUnorderedListItem(l) || isOrderedListItem(l) { break }
            if i + 1 < lines.count,
               let headers = parseTableRow(l),
               isTableSeparator(lines[i + 1], expectedColumns: headers.count) {
                break
            }
            paraLines.append(l)
            i += 1
        }
        if !paraLines.isEmpty {
            blocks.append("<p>\(renderInline(paraLines.joined(separator: " ")))</p>")
        }
    }

    return blocks.joined(separator: "\n")
}

// MARK: - Block helpers

private func parseHeading(_ line: String) -> String? {
    guard line.hasPrefix("#") else { return nil }
    var level = 0
    var rest = line[line.startIndex...]
    while rest.hasPrefix("#") {
        level += 1
        rest = rest.dropFirst()
    }
    guard level <= 6, rest.hasPrefix(" ") else { return nil }
    let text = String(rest.dropFirst()).trimmingCharacters(in: .whitespaces)
    let id = text.lowercased()
        .components(separatedBy: .whitespaces).joined(separator: "-")
        .filter { $0.isLetter || $0.isNumber || $0 == "-" }
    return "<h\(level) id=\"\(escapeHTML(id))\">\(renderInline(text))</h\(level)>"
}

private func isHorizontalRule(_ trimmed: String) -> Bool {
    let chars = trimmed.filter { !$0.isWhitespace }
    guard chars.count >= 3 else { return false }
    let set = Set(chars)
    return set.count == 1 && (set.contains("-") || set.contains("*") || set.contains("_"))
}

private func isUnorderedListItem(_ line: String) -> Bool {
    line.range(of: #"^[\-\*\+]\s+"#, options: .regularExpression) != nil
}

private func isOrderedListItem(_ line: String) -> Bool {
    line.range(of: #"^\d+\.\s+"#, options: .regularExpression) != nil
}

private func parseTaskItem(_ content: String) -> (Bool, String)? {
    guard content.count >= 4 else { return nil }
    guard content.hasPrefix("[") else { return nil }

    let chars = Array(content)
    guard chars.count >= 4, chars[2] == "]", chars[3] == " " else { return nil }

    switch chars[1] {
    case " ":
        return (false, String(chars.dropFirst(4)))
    case "x", "X":
        return (true, String(chars.dropFirst(4)))
    default:
        return nil
    }
}

private func parseTableRow(_ line: String) -> [String]? {
    guard line.contains("|") else { return nil }
    var trimmed = line.trimmingCharacters(in: .whitespaces)
    guard !trimmed.isEmpty else { return nil }
    if trimmed.hasPrefix("|") { trimmed.removeFirst() }
    if trimmed.hasSuffix("|") { trimmed.removeLast() }
    let cells = trimmed
        .split(separator: "|", omittingEmptySubsequences: false)
        .map { String($0).trimmingCharacters(in: .whitespaces) }
    guard cells.count >= 2 else { return nil }
    return cells
}

private func isTableSeparator(_ line: String, expectedColumns: Int) -> Bool {
    guard let cells = parseTableRow(line), cells.count == expectedColumns else {
        return false
    }
    return cells.allSatisfy { $0.range(of: #"^:?-{3,}:?$"#, options: .regularExpression) != nil }
}

private func normalizeTableRow(_ cells: [String], to count: Int) -> [String] {
    if cells.count == count {
        return cells
    }
    if cells.count > count {
        return Array(cells.prefix(count))
    }
    return cells + Array(repeating: "", count: count - cells.count)
}

private func renderTable(headers: [String], rows: [[String]]) -> String {
    var html = "<table><thead><tr>"
    for header in headers {
        html += "<th>\(renderInline(header))</th>"
    }
    html += "</tr></thead><tbody>"
    for row in rows {
        html += "<tr>"
        for cell in row {
            html += "<td>\(renderInline(cell))</td>"
        }
        html += "</tr>"
    }
    html += "</tbody></table>"
    return html
}

// MARK: - Inline rendering

func renderInline(_ text: String) -> String {
    var result = ""
    var i = text.startIndex

    while i < text.endIndex {
        let c = text[i]

        // Inline code
        if c == "`" {
            let (code, next) = parseInlineCode(text, from: i)
            result += code
            i = next
            continue
        }

        // Bold + italic (*** or ___)
        if (c == "*" || c == "_") && text.distance(from: i, to: text.endIndex) >= 3 {
            let tri = String(text[i...].prefix(3))
            if tri == "***" || tri == "___" {
                let del = tri.first!
                let after = text.index(i, offsetBy: 3)
                if let end = findDelimiter(String(repeating: del, count: 3), in: text, after: after) {
                    let inner = renderInline(String(text[after..<end]))
                    result += "<strong><em>\(inner)</em></strong>"
                    i = text.index(end, offsetBy: 3)
                    continue
                }
            }
        }

        // Bold (** or __)
        if (c == "*" || c == "_") && text.distance(from: i, to: text.endIndex) >= 2 {
            let dbl = String(text[i...].prefix(2))
            if dbl == "**" || dbl == "__" {
                let del = dbl.first!
                let after = text.index(i, offsetBy: 2)
                if let end = findDelimiter(String(repeating: del, count: 2), in: text, after: after) {
                    let inner = renderInline(String(text[after..<end]))
                    result += "<strong>\(inner)</strong>"
                    i = text.index(end, offsetBy: 2)
                    continue
                }
            }
        }

        // Italic (* or _)
        if c == "*" || c == "_" {
            let after = text.index(after: i)
            if let end = findDelimiter(String(c), in: text, after: after) {
                let inner = renderInline(String(text[after..<end]))
                result += "<em>\(inner)</em>"
                i = text.index(after: end)
                continue
            }
        }

        // Link [text](url) or image ![alt](url)
        if c == "!" && text.index(after: i) < text.endIndex && text[text.index(after: i)] == "[" {
            if let (tag, next) = parseLinkOrImage(text, from: i, isImage: true) {
                result += tag
                i = next
                continue
            }
        }
        if c == "[" {
            if let (tag, next) = parseLinkOrImage(text, from: i, isImage: false) {
                result += tag
                i = next
                continue
            }
        }

        // Autolink <url>
        if c == "<" {
            if let end = text[text.index(after: i)...].firstIndex(of: ">") {
                let inner = String(text[text.index(after: i)..<end])
                if inner.hasPrefix("http://") || inner.hasPrefix("https://") || inner.hasPrefix("mailto:") {
                    result += "<a href=\"\(escapeHTML(inner))\">\(escapeHTML(inner))</a>"
                    i = text.index(after: end)
                    continue
                }
            }
        }

        // Escape
        if c == "\\" && text.index(after: i) < text.endIndex {
            let next = text.index(after: i)
            let nc = text[next]
            if "\\`*_{}[]()#+-.!".contains(nc) {
                result += escapeHTML(String(nc))
                i = text.index(after: next)
                continue
            }
        }

        result += escapeHTML(String(c))
        i = text.index(after: i)
    }

    return result
}

// MARK: - Inline helpers

private func parseInlineCode(_ text: String, from start: String.Index) -> (String, String.Index) {
    var ticks = 0
    var i = start
    while i < text.endIndex && text[i] == "`" {
        ticks += 1
        i = text.index(after: i)
    }
    let delimiter = String(repeating: "`", count: ticks)
    if let end = text[i...].range(of: delimiter) {
        let code = escapeHTML(String(text[i..<end.lowerBound]))
        return ("<code>\(code)</code>", text.index(end.upperBound, offsetBy: 0))
    }
    // No closing — treat opening ticks literally
    return (escapeHTML(delimiter), i)
}

private func findDelimiter(_ delimiter: String, in text: String, after start: String.Index) -> String.Index? {
    var i = start
    while i < text.endIndex {
        if text[i...].hasPrefix(delimiter) {
            // ensure it doesn't have a space immediately before it
            if i > start {
                let prev = text.index(before: i)
                if text[prev] == " " { break }
            }
            return i
        }
        i = text.index(after: i)
    }
    return nil
}

private func parseLinkOrImage(_ text: String, from start: String.Index, isImage: Bool) -> (String, String.Index)? {
    let i = isImage ? text.index(start, offsetBy: 2) : text.index(after: start)
    guard i < text.endIndex else { return nil }

    // Find closing ]
    var depth = 1
    var labelEnd = i
    while labelEnd < text.endIndex && depth > 0 {
        if text[labelEnd] == "[" { depth += 1 }
        else if text[labelEnd] == "]" { depth -= 1 }
        if depth > 0 { labelEnd = text.index(after: labelEnd) }
    }
    guard depth == 0 else { return nil }
    let labelText = String(text[i..<labelEnd])

    let afterBracket = text.index(after: labelEnd)
    guard afterBracket < text.endIndex && text[afterBracket] == "(" else { return nil }

    var parenEnd = text.index(after: afterBracket)
    while parenEnd < text.endIndex && text[parenEnd] != ")" {
        parenEnd = text.index(after: parenEnd)
    }
    guard parenEnd < text.endIndex else { return nil }
    let url = String(text[text.index(after: afterBracket)..<parenEnd])

    let next = text.index(after: parenEnd)
    if isImage {
        let tag = "<img src=\"\(escapeHTML(url))\" alt=\"\(escapeHTML(labelText))\">"
        return (tag, next)
    } else {
        let inner = renderInline(labelText)
        let tag = "<a href=\"\(escapeHTML(url))\">\(inner)</a>"
        return (tag, next)
    }
}

// MARK: - HTML escaping

func escapeHTML(_ s: String) -> String {
    s
        .replacingOccurrences(of: "&", with: "&amp;")
        .replacingOccurrences(of: "<", with: "&lt;")
        .replacingOccurrences(of: ">", with: "&gt;")
        .replacingOccurrences(of: "\"", with: "&quot;")
}

// MARK: - String helpers

extension String {
    func replacingFirst(of pattern: String, with replacement: String) -> String {
        guard let range = self.range(of: pattern, options: .regularExpression) else { return self }
        return replacingCharacters(in: range, with: replacement)
    }
}
