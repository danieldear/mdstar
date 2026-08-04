import AppKit
import Foundation

/// A highlight or comment attached to a range of the rendered document.
struct Annotation: Codable, Identifiable, Hashable, Sendable {
    enum Kind: String, Codable, Sendable {
        case highlight
        case comment

        var title: String {
            switch self {
            case .highlight: "Highlight"
            case .comment: "Comment"
            }
        }

        var systemImage: String {
            switch self {
            case .highlight: "highlighter"
            case .comment: "text.bubble"
            }
        }

        var highlightColor: NSColor {
            switch self {
            case .highlight: NSColor.systemYellow.withAlphaComponent(0.35)
            case .comment: NSColor.systemBlue.withAlphaComponent(0.22)
            }
        }
    }

    let id: UUID
    let kind: Kind
    /// Character offset when the annotation was made.
    var location: Int
    var length: Int
    /// The text that was selected. Kept so an annotation can be re-found if the
    /// document shifts underneath it, and so the inspector can show a preview
    /// without re-reading the document.
    let snippet: String
    var note: String
    let createdAt: Date
    /// Block the passage belongs to, so the reader can scroll to it with the
    /// same mechanism bookmarks and search use.
    let blockID: String?

    init(
        id: UUID = UUID(),
        kind: Kind,
        range: NSRange,
        snippet: String,
        note: String = "",
        blockID: String? = nil,
        createdAt: Date = Date()
    ) {
        self.id = id
        self.kind = kind
        self.location = range.location
        self.length = range.length
        self.snippet = snippet
        self.note = note
        self.blockID = blockID
        self.createdAt = createdAt
    }

    var range: NSRange { NSRange(location: location, length: length) }

    /// Resolves the annotation against the current text.
    ///
    /// Offsets alone are brittle: editing earlier in the document shifts every
    /// later annotation. When the stored offset no longer contains the original
    /// snippet, fall back to searching for that snippet so the anchor survives.
    func resolvedRange(in text: String) -> NSRange {
        let nsText = text as NSString
        if range.location >= 0,
           range.upperBound <= nsText.length,
           nsText.substring(with: range) == snippet {
            return range
        }
        guard !snippet.isEmpty else { return NSRange(location: NSNotFound, length: 0) }
        let found = nsText.range(of: snippet)
        return found
    }
}
