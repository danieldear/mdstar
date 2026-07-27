import Foundation

struct FFIEnvelope<Value: Decodable>: Decodable {
    let ok: Bool
    let value: Value?
    let error: FFIError?
}

struct FFIError: Decodable, Error, LocalizedError, Sendable {
    let code: String
    let message: String
    var errorDescription: String? { message }
}

struct DocumentIR: Codable, Identifiable, Sendable {
    let schemaVersion: Int
    let documentID: String
    let origin: String
    let blocks: [BlockIR]
    let outline: [OutlineItem]
    let diagnostics: [DiagnosticIR]

    var id: String { documentID }

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case documentID = "document_id"
        case origin, blocks, outline, diagnostics
    }
}

struct SourceRange: Codable, Hashable, Sendable {
    let start: SourceLocation
    let end: SourceLocation
}

struct SourceLocation: Codable, Hashable, Sendable {
    let byteOffset: Int
    let line: Int
    let column: Int

    enum CodingKeys: String, CodingKey {
        case byteOffset = "byte_offset"
        case line, column
    }
}

struct BlockIR: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let range: SourceRange?
    let kind: String
    let level: Int?
    let inlines: [InlineIR]
    let children: [BlockIR]
    let ordered: Bool?
    let start: Int?
    let items: [ListItemIR]
    let language: String?
    let meta: String?
    let code: String?
    let headers: [[InlineIR]]
    let rows: [[[InlineIR]]]
    let raw: String?
}

struct ListItemIR: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let range: SourceRange?
    let checked: Bool?
    let children: [BlockIR]
}

struct InlineIR: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let range: SourceRange?
    let kind: String
    let text: String?
    let children: [InlineIR]
    let url: String?
    let title: String?
    let alt: String?
}

struct OutlineItem: Codable, Identifiable, Hashable, Sendable {
    let id: String
    let level: Int
    let text: String
    let anchor: String
}

struct DiagnosticIR: Codable, Identifiable, Hashable, Sendable {
    let severity: String
    let code: String
    let message: String
    let line: Int?
    let column: Int?

    var id: String { "\(code)-\(line ?? 0)-\(column ?? 0)" }
}

extension InlineIR {
    var plainText: String {
        let ownText = text ?? alt ?? ""
        let childText = children.map(\.plainText).joined()
        return ownText + childText + (kind == "hard_break" ? " " : "")
    }
}

extension BlockIR {
    /// Flattened text used for in-document search matching.
    var searchableText: String {
        var parts: [String] = [inlines.map(\.plainText).joined()]
        if let code { parts.append(code) }
        if let raw { parts.append(raw) }
        parts.append(headers.map { $0.map(\.plainText).joined() }.joined(separator: " "))
        parts.append(rows.map { $0.map { $0.map(\.plainText).joined() }.joined(separator: " ") }.joined(separator: " "))
        parts.append(items.flatMap(\.children).map(\.searchableText).joined(separator: " "))
        parts.append(children.map(\.searchableText).joined(separator: " "))
        return parts.filter { !$0.isEmpty }.joined(separator: " ")
    }
}

/// A saved position within a document (section-level bookmark).
struct Bookmark: Codable, Identifiable, Hashable, Sendable {
    let id: String      // block id
    let title: String
    let level: Int
}
