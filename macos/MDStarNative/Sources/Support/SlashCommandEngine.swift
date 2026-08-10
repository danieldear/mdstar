import Foundation

struct SlashCommandContext: Equatable, Sendable {
    let query: String
    let replacementRange: NSRange
}

enum SlashCommandEngine {
    /// Returns a command query only when the caret follows a slash at the
    /// beginning of a line (allowing indentation). This prevents ordinary
    /// paths, URLs, and prose containing `/` from opening the palette.
    static func context(in source: String, selectedRange: NSRange) -> SlashCommandContext? {
        guard selectedRange.length == 0 else { return nil }

        let text = source as NSString
        let caret = selectedRange.location
        guard caret <= text.length else { return nil }

        let lineRange = text.lineRange(for: NSRange(location: caret, length: 0))
        let prefixRange = NSRange(location: lineRange.location, length: caret - lineRange.location)
        let prefix = text.substring(with: prefixRange) as NSString
        guard let slashRange = prefix.range(of: "/").nonNotFound else { return nil }

        let beforeSlash = prefix.substring(to: slashRange.location)
        guard beforeSlash.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }

        let queryStart = slashRange.location + slashRange.length
        let query = prefix.substring(from: queryStart)
        guard !query.contains(where: { $0.isWhitespace }) else { return nil }

        return SlashCommandContext(
            query: query,
            replacementRange: NSRange(
                location: lineRange.location + slashRange.location,
                length: prefix.length - slashRange.location
            )
        )
    }

    static func filteredCommands(
        matching query: String,
        commands: [SlashCommand] = SlashCommand.markdownCommands
    ) -> [SlashCommand] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return commands }

        return commands.filter { command in
            command.title.lowercased().contains(needle)
                || command.id.lowercased().contains(needle)
                || command.keywords.contains { $0.lowercased().contains(needle) }
        }
    }
}

private extension NSRange {
    var nonNotFound: NSRange? { location == NSNotFound ? nil : self }
}
