import AppKit
import SwiftUI

/// Small, dependency-free syntax highlighter. It is intentionally approximate:
/// a single scan tags comments, strings, numbers, and a shared keyword set, which
/// is enough to give code blocks Mud-like color without bundling a grammar engine.
enum SyntaxHighlighter {
    private enum Kind { case comment, string, number, keyword }

    static func highlight(_ code: String, language: String?) -> AttributedString {
        var attributed = AttributedString(code)
        let chars = Array(code)
        guard !chars.isEmpty else { return attributed }

        let lineTokens = lineCommentTokens(for: language)
        let hasBlockComments = usesCBlockComments(language)
        var spans: [(Int, Int, Kind)] = []

        var i = 0
        while i < chars.count {
            let c = chars[i]

            // Line comments
            if let token = lineTokens.first(where: { matches($0, chars, i) }) {
                let start = i
                i += token.count
                while i < chars.count, chars[i] != "\n" { i += 1 }
                spans.append((start, i, .comment))
                continue
            }
            // Block comments
            if hasBlockComments, matches("/*", chars, i) {
                let start = i
                i += 2
                while i < chars.count, !matches("*/", chars, i) { i += 1 }
                i = min(i + 2, chars.count)
                spans.append((start, i, .comment))
                continue
            }
            // Strings
            if c == "\"" || c == "'" || c == "`" {
                let quote = c
                let start = i
                i += 1
                while i < chars.count {
                    if chars[i] == "\\" { i += 2; continue }
                    if chars[i] == quote { i += 1; break }
                    if chars[i] == "\n" { break }
                    i += 1
                }
                spans.append((start, min(i, chars.count), .string))
                continue
            }
            // Numbers
            if c.isNumber || (c == "." && i + 1 < chars.count && chars[i + 1].isNumber) {
                let start = i
                while i < chars.count, isNumberChar(chars[i]) { i += 1 }
                spans.append((start, i, .number))
                continue
            }
            // Identifiers / keywords
            if c.isLetter || c == "_" {
                let start = i
                while i < chars.count, chars[i].isLetter || chars[i].isNumber || chars[i] == "_" { i += 1 }
                let word = String(chars[start..<i])
                if Self.keywords.contains(word) { spans.append((start, i, .keyword)) }
                continue
            }
            i += 1
        }

        for (start, end, kind) in spans {
            guard start < end, end <= chars.count else { continue }
            let s = attributed.index(attributed.startIndex, offsetByCharacters: start)
            let e = attributed.index(s, offsetByCharacters: end - start)
            attributed[s..<e].foregroundColor = color(kind)
        }
        return attributed
    }

    // MARK: - Helpers

    private static func matches(_ token: String, _ chars: [Character], _ index: Int) -> Bool {
        let t = Array(token)
        guard index + t.count <= chars.count else { return false }
        for k in 0..<t.count where chars[index + k] != t[k] { return false }
        return true
    }

    private static func isNumberChar(_ c: Character) -> Bool {
        c.isNumber || c == "." || c == "_" || c == "x" || c == "b" || c == "e"
            || ("a"..."f").contains(c) || ("A"..."F").contains(c)
    }

    private static func lineCommentTokens(for language: String?) -> [String] {
        switch (language ?? "").lowercased() {
        case "python", "py", "ruby", "rb", "sh", "bash", "shell", "zsh", "yaml", "yml", "toml", "r", "perl", "makefile", "dockerfile":
            return ["#"]
        case "sql", "lua", "haskell", "hs", "elm":
            return ["--"]
        case "lisp", "clojure", "clj", "scheme", "ini":
            return [";"]
        default:
            return ["//"]
        }
    }

    private static func usesCBlockComments(_ language: String?) -> Bool {
        switch (language ?? "").lowercased() {
        case "python", "py", "ruby", "rb", "sh", "bash", "shell", "yaml", "yml", "toml", "lisp", "clojure":
            return false
        default:
            return true
        }
    }

    private static func color(_ kind: Kind) -> Color {
        switch kind {
        case .comment: dynamic(light: NSColor(srgbRed: 0.24, green: 0.51, blue: 0.25, alpha: 1),
                               dark: NSColor(srgbRed: 0.50, green: 0.78, blue: 0.52, alpha: 1))
        case .string:  dynamic(light: NSColor(srgbRed: 0.77, green: 0.20, blue: 0.15, alpha: 1),
                               dark: NSColor(srgbRed: 1.00, green: 0.55, blue: 0.46, alpha: 1))
        case .number:  dynamic(light: NSColor(srgbRed: 0.11, green: 0.20, blue: 0.80, alpha: 1),
                               dark: NSColor(srgbRed: 0.70, green: 0.63, blue: 0.98, alpha: 1))
        case .keyword: dynamic(light: NSColor(srgbRed: 0.66, green: 0.15, blue: 0.55, alpha: 1),
                               dark: NSColor(srgbRed: 1.00, green: 0.48, blue: 0.73, alpha: 1))
        }
    }

    private static func dynamic(light: NSColor, dark: NSColor) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
        })
    }

    private static let keywords: Set<String> = [
        // control / declarations shared across common languages
        "fn", "func", "function", "def", "let", "var", "const", "val", "mut", "static",
        "class", "struct", "enum", "protocol", "interface", "trait", "impl", "extension",
        "public", "private", "protected", "internal", "pub", "package", "module", "namespace",
        "import", "from", "use", "using", "include", "require", "extends", "implements",
        "return", "if", "else", "elif", "for", "while", "loop", "match", "switch", "case",
        "default", "break", "continue", "do", "try", "catch", "throw", "throws", "finally",
        "async", "await", "yield", "defer", "guard", "in", "is", "as", "where",
        "true", "false", "nil", "null", "none", "self", "this", "super", "new", "delete",
        "type", "typedef", "typealias", "void", "int", "float", "double", "bool", "string",
        "and", "or", "not", "with", "lambda", "then", "end", "begin", "override", "final",
    ]
}
