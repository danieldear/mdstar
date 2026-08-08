import Foundation

/// A source-editor shortcut that expands a slash query into Markdown.
///
/// Commands deliberately insert plain Markdown rather than introducing a
/// second block-document model. The Rust parser and the source file remain the
/// single source of truth.
struct SlashCommand: Identifiable, Equatable, Sendable {
    let id: String
    let title: String
    let subtitle: String
    let systemImage: String
    let keywords: [String]
    let insertion: String
    let selection: NSRange

    init(
        id: String,
        title: String,
        subtitle: String,
        systemImage: String,
        keywords: [String] = [],
        insertion: String,
        selection: NSRange? = nil
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.systemImage = systemImage
        self.keywords = keywords
        self.insertion = insertion
        self.selection = selection ?? NSRange(location: insertion.utf16.count, length: 0)
    }
}

extension SlashCommand {
    static let markdownCommands: [SlashCommand] = [
        SlashCommand(
            id: "heading-1",
            title: "Heading 1",
            subtitle: "Large section heading",
            systemImage: "textformat.size.larger",
            keywords: ["h1", "title"],
            insertion: "# "
        ),
        SlashCommand(
            id: "heading-2",
            title: "Heading 2",
            subtitle: "Medium section heading",
            systemImage: "textformat.size",
            keywords: ["h2", "subtitle"],
            insertion: "## "
        ),
        SlashCommand(
            id: "heading-3",
            title: "Heading 3",
            subtitle: "Small section heading",
            systemImage: "textformat",
            keywords: ["h3"],
            insertion: "### "
        ),
        SlashCommand(
            id: "bulleted-list",
            title: "Bulleted List",
            subtitle: "Create a simple list",
            systemImage: "list.bullet",
            keywords: ["bullet", "unordered"],
            insertion: "- "
        ),
        SlashCommand(
            id: "numbered-list",
            title: "Numbered List",
            subtitle: "Create an ordered list",
            systemImage: "list.number",
            keywords: ["ordered", "list"],
            insertion: "1. "
        ),
        SlashCommand(
            id: "task",
            title: "Task",
            subtitle: "Create a Markdown checkbox",
            systemImage: "checklist",
            keywords: ["todo", "checkbox"],
            insertion: "- [ ] "
        ),
        SlashCommand(
            id: "quote",
            title: "Quote",
            subtitle: "Capture a quotation or callout",
            systemImage: "text.quote",
            keywords: ["blockquote", "callout"],
            insertion: "> "
        ),
        SlashCommand(
            id: "code-block",
            title: "Code Block",
            subtitle: "Insert a fenced code block",
            systemImage: "chevron.left.forwardslash.chevron.right",
            keywords: ["fence", "snippet"],
            insertion: "```text\n\n```",
            selection: NSRange(location: 3, length: 4)
        ),
        SlashCommand(
            id: "table",
            title: "Table",
            subtitle: "Insert a two-column table",
            systemImage: "tablecells",
            keywords: ["grid", "columns"],
            insertion: "| Column 1 | Column 2 |\n|----------|----------|\n| Value    | Value    |",
            selection: NSRange(location: 2, length: 8)
        ),
        SlashCommand(
            id: "link",
            title: "Link",
            subtitle: "Insert a Markdown link",
            systemImage: "link",
            keywords: ["url", "website"],
            insertion: "[title](https://)",
            selection: NSRange(location: 1, length: 5)
        ),
        SlashCommand(
            id: "image",
            title: "Image",
            subtitle: "Insert an image reference",
            systemImage: "photo",
            keywords: ["picture", "asset"],
            insertion: "![alt text](path/to/image.png)",
            selection: NSRange(location: 2, length: 8)
        ),
        SlashCommand(
            id: "mermaid",
            title: "Mermaid Diagram",
            subtitle: "Insert a Mermaid diagram block",
            systemImage: "point.3.connected.trianglepath.dotted",
            keywords: ["diagram", "flowchart", "graph"],
            insertion: "```mermaid\ngraph TD\n    A --> B\n```",
            selection: NSRange(location: 11, length: 8)
        ),
        SlashCommand(
            id: "divider",
            title: "Divider",
            subtitle: "Insert a horizontal rule",
            systemImage: "minus",
            keywords: ["rule", "separator", "horizontal"],
            insertion: "---\n"
        ),
        SlashCommand(
            id: "front-matter",
            title: "Front Matter",
            subtitle: "Insert YAML document metadata",
            systemImage: "doc.badge.gearshape",
            keywords: ["yaml", "metadata"],
            insertion: "---\ntitle: Untitled\n---\n\n",
            selection: NSRange(location: 11, length: 8)
        ),
    ]
}
