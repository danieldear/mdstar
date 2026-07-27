//! Serializable semantic document contract for native clients.
//!
//! This deliberately sits beside the existing `Document` model.  The terminal
//! and HTML renderers can keep using their current APIs while platform clients
//! receive stable, versioned data instead of presentation HTML.

use markdown::mdast::Node;
use markdown::{Constructs, ParseOptions};
use serde::{Deserialize, Serialize};

use crate::{Diagnostic, DiagnosticSeverity, MarkdownError, Result};

pub const DOCUMENT_IR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub byte_offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start: SourceLocation,
    pub end: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentIr {
    pub schema_version: u32,
    pub document_id: String,
    pub origin: String,
    pub blocks: Vec<BlockIr>,
    pub outline: Vec<OutlineItemIr>,
    pub diagnostics: Vec<DiagnosticIr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineItemIr {
    pub id: String,
    pub level: u8,
    pub text: String,
    pub anchor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticIr {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

/// A tagged but intentionally flat block shape.  It is straightforward for
/// Swift Codable to decode, and unknown optional fields are ignored safely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockIr {
    pub id: String,
    pub range: Option<SourceRange>,
    pub kind: String,
    pub level: Option<u8>,
    pub inlines: Vec<InlineIr>,
    pub children: Vec<BlockIr>,
    pub ordered: Option<bool>,
    pub start: Option<u32>,
    pub items: Vec<ListItemIr>,
    pub language: Option<String>,
    pub meta: Option<String>,
    pub code: Option<String>,
    pub headers: Vec<Vec<InlineIr>>,
    pub rows: Vec<Vec<Vec<InlineIr>>>,
    pub raw: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItemIr {
    pub id: String,
    pub range: Option<SourceRange>,
    pub checked: Option<bool>,
    pub children: Vec<BlockIr>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineIr {
    pub id: String,
    pub range: Option<SourceRange>,
    pub kind: String,
    pub text: Option<String>,
    pub children: Vec<InlineIr>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub alt: Option<String>,
}

impl BlockIr {
    fn new(id: String, range: Option<SourceRange>, kind: impl Into<String>) -> Self {
        Self {
            id,
            range,
            kind: kind.into(),
            level: None,
            inlines: Vec::new(),
            children: Vec::new(),
            ordered: None,
            start: None,
            items: Vec::new(),
            language: None,
            meta: None,
            code: None,
            headers: Vec::new(),
            rows: Vec::new(),
            raw: None,
        }
    }
}

impl InlineIr {
    fn new(id: String, range: Option<SourceRange>, kind: impl Into<String>) -> Self {
        Self {
            id,
            range,
            kind: kind.into(),
            text: None,
            children: Vec::new(),
            url: None,
            title: None,
            alt: None,
        }
    }
}

/// Parse Markdown directly to a versioned semantic DTO. `origin` should be a
/// stable file path/URL when one exists; it scopes deterministic node IDs.
pub fn parse_document_ir_with_diagnostics(input: &str, origin: &str) -> Result<DocumentIr> {
    let options = ParseOptions {
        constructs: Constructs {
            frontmatter: true,
            math_flow: true,
            math_text: true,
            ..Constructs::gfm()
        },
        ..ParseOptions::default()
    };
    let root = markdown::to_mdast(input, &options)
        .map_err(|message| MarkdownError::ParserAdapter(message.reason.clone()))?;

    let document_id = format!("doc-{:016x}", stable_hash(origin));
    let mut ids = IdAllocator::default();
    let mut blocks = Vec::new();
    if let Node::Root(root) = root {
        for child in &root.children {
            if let Some(block) = node_to_block_ir(child, &document_id, &mut ids) {
                blocks.push(block);
            }
        }
    }

    let outline = blocks
        .iter()
        .filter(|block| block.kind == "heading")
        .map(|block| OutlineItemIr {
            id: block.id.clone(),
            level: block.level.unwrap_or(1),
            text: inline_plain_text(&block.inlines),
            anchor: heading_anchor(&inline_plain_text(&block.inlines)),
        })
        .collect();

    let diagnostics = crate::parse_markdown_with_diagnostics(input)?
        .diagnostics
        .iter()
        .map(diagnostic_to_ir)
        .collect();

    Ok(DocumentIr {
        schema_version: DOCUMENT_IR_SCHEMA_VERSION,
        document_id,
        origin: origin.to_string(),
        blocks,
        outline,
        diagnostics,
    })
}

fn node_to_block_ir(node: &Node, document_id: &str, ids: &mut IdAllocator) -> Option<BlockIr> {
    let range = source_range(node);
    let make_id = |kind: &str, ids: &mut IdAllocator| ids.next(document_id, kind, range.as_ref());
    match node {
        Node::Heading(heading) => {
            let mut block = BlockIr::new(make_id("heading", ids), range, "heading");
            block.level = Some(heading.depth);
            block.inlines = nodes_to_inlines_ir(&heading.children, document_id, ids);
            Some(block)
        }
        Node::Paragraph(paragraph) => {
            let mut block = BlockIr::new(make_id("paragraph", ids), range, "paragraph");
            block.inlines = nodes_to_inlines_ir(&paragraph.children, document_id, ids);
            Some(block)
        }
        Node::Blockquote(quote) => {
            let mut block = BlockIr::new(make_id("blockquote", ids), range, "blockquote");
            block.children = nodes_to_blocks_ir(&quote.children, document_id, ids);
            Some(block)
        }
        Node::List(list) => {
            let mut block = BlockIr::new(make_id("list", ids), range, "list");
            block.ordered = Some(list.ordered);
            block.start = list.start;
            block.items = list
                .children
                .iter()
                .filter_map(|item| match item {
                    Node::ListItem(item) => {
                        let item_range = source_range_from_position(item.position.as_ref());
                        Some(ListItemIr {
                            id: ids.next(document_id, "list_item", item_range.as_ref()),
                            range: item_range,
                            checked: item.checked,
                            children: nodes_to_blocks_ir(&item.children, document_id, ids),
                        })
                    }
                    _ => None,
                })
                .collect();
            Some(block)
        }
        Node::Code(code) => {
            let mut block = BlockIr::new(make_id("code", ids), range, "code");
            block.language = code.lang.clone().filter(|value| !value.is_empty());
            block.meta = code.meta.clone().filter(|value| !value.is_empty());
            block.code = Some(code.value.trim_end_matches('\n').to_string());
            Some(block)
        }
        Node::Math(math) => {
            let mut block = BlockIr::new(make_id("math", ids), range, "math");
            block.raw = Some(math.value.clone());
            Some(block)
        }
        Node::Table(table) => {
            let mut block = BlockIr::new(make_id("table", ids), range, "table");
            for (row_index, row) in table.children.iter().enumerate() {
                let Node::TableRow(row) = row else { continue };
                let cells = row
                    .children
                    .iter()
                    .filter_map(|cell| match cell {
                        Node::TableCell(cell) => {
                            Some(nodes_to_inlines_ir(&cell.children, document_id, ids))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if row_index == 0 {
                    block.headers = cells;
                } else {
                    block.rows.push(cells);
                }
            }
            Some(block)
        }
        Node::ThematicBreak(_) => Some(BlockIr::new(
            make_id("thematic_break", ids),
            range,
            "thematic_break",
        )),
        Node::Html(html) => {
            let mut block = BlockIr::new(make_id("html", ids), range, "html");
            block.raw = Some(html.value.clone());
            Some(block)
        }
        Node::Yaml(yaml) => {
            let mut block = BlockIr::new(make_id("frontmatter", ids), range, "frontmatter");
            block.raw = Some(yaml.value.clone());
            Some(block)
        }
        Node::Toml(toml) => {
            let mut block = BlockIr::new(make_id("frontmatter", ids), range, "frontmatter");
            block.raw = Some(toml.value.clone());
            Some(block)
        }
        Node::Definition(_) | Node::FootnoteDefinition(_) => None,
        _ => None,
    }
}

fn nodes_to_blocks_ir(nodes: &[Node], document_id: &str, ids: &mut IdAllocator) -> Vec<BlockIr> {
    nodes
        .iter()
        .filter_map(|node| node_to_block_ir(node, document_id, ids))
        .collect()
}

fn nodes_to_inlines_ir(nodes: &[Node], document_id: &str, ids: &mut IdAllocator) -> Vec<InlineIr> {
    nodes
        .iter()
        .filter_map(|node| node_to_inline_ir(node, document_id, ids))
        .collect()
}

fn node_to_inline_ir(node: &Node, document_id: &str, ids: &mut IdAllocator) -> Option<InlineIr> {
    let range = source_range(node);
    let make_id = |kind: &str, ids: &mut IdAllocator| ids.next(document_id, kind, range.as_ref());
    match node {
        Node::Text(text) => {
            let mut inline = InlineIr::new(make_id("text", ids), range, "text");
            inline.text = Some(text.value.clone());
            Some(inline)
        }
        Node::Strong(value) => inline_children(
            "strong",
            &value.children,
            make_id("strong", ids),
            range,
            document_id,
            ids,
        ),
        Node::Emphasis(value) => inline_children(
            "emphasis",
            &value.children,
            make_id("emphasis", ids),
            range,
            document_id,
            ids,
        ),
        Node::Delete(value) => inline_children(
            "delete",
            &value.children,
            make_id("delete", ids),
            range,
            document_id,
            ids,
        ),
        Node::InlineCode(value) => {
            let mut inline = InlineIr::new(make_id("code", ids), range, "code");
            inline.text = Some(value.value.clone());
            Some(inline)
        }
        Node::InlineMath(value) => {
            let mut inline = InlineIr::new(make_id("math", ids), range, "math");
            inline.text = Some(value.value.clone());
            Some(inline)
        }
        Node::Link(value) => {
            let mut inline = InlineIr::new(make_id("link", ids), range, "link");
            inline.children = nodes_to_inlines_ir(&value.children, document_id, ids);
            inline.url = Some(value.url.clone());
            inline.title = value.title.clone();
            Some(inline)
        }
        Node::Image(value) => {
            let mut inline = InlineIr::new(make_id("image", ids), range, "image");
            inline.alt = Some(value.alt.clone());
            inline.url = Some(value.url.clone());
            inline.title = value.title.clone();
            Some(inline)
        }
        Node::LinkReference(value) => inline_children(
            "link",
            &value.children,
            make_id("link", ids),
            range,
            document_id,
            ids,
        ),
        Node::ImageReference(value) => {
            let mut inline = InlineIr::new(make_id("image", ids), range, "image");
            inline.alt = Some(value.alt.clone());
            Some(inline)
        }
        Node::Html(value) => {
            let mut inline = InlineIr::new(make_id("html", ids), range, "html");
            inline.text = Some(value.value.clone());
            Some(inline)
        }
        Node::Break(_) => Some(InlineIr::new(
            make_id("hard_break", ids),
            range,
            "hard_break",
        )),
        Node::Paragraph(value) => {
            let mut inline = InlineIr::new(make_id("paragraph", ids), range, "paragraph");
            inline.children = nodes_to_inlines_ir(&value.children, document_id, ids);
            Some(inline)
        }
        _ => None,
    }
}

fn inline_children(
    kind: &str,
    children: &[Node],
    id: String,
    range: Option<SourceRange>,
    document_id: &str,
    ids: &mut IdAllocator,
) -> Option<InlineIr> {
    let mut inline = InlineIr::new(id, range, kind);
    inline.children = nodes_to_inlines_ir(children, document_id, ids);
    Some(inline)
}

fn source_range(node: &Node) -> Option<SourceRange> {
    source_range_from_position(node.position())
}

fn source_range_from_position(position: Option<&markdown::unist::Position>) -> Option<SourceRange> {
    position.map(|position| SourceRange {
        start: SourceLocation {
            byte_offset: position.start.offset,
            line: position.start.line,
            column: position.start.column,
        },
        end: SourceLocation {
            byte_offset: position.end.offset,
            line: position.end.line,
            column: position.end.column,
        },
    })
}

#[derive(Default)]
struct IdAllocator {
    ordinal: u64,
}

impl IdAllocator {
    fn next(&mut self, document_id: &str, kind: &str, range: Option<&SourceRange>) -> String {
        self.ordinal += 1;
        let range_key = range
            .map(|range| format!("{}:{}", range.start.byte_offset, range.end.byte_offset))
            .unwrap_or_else(|| "none".to_string());
        format!(
            "{}-{}-{:016x}",
            kind,
            self.ordinal,
            stable_hash(&format!("{document_id}:{kind}:{range_key}"))
        )
    }
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}

fn inline_plain_text(inlines: &[InlineIr]) -> String {
    let mut output = String::new();
    for inline in inlines {
        if let Some(text) = &inline.text {
            output.push_str(text);
        }
        if inline.kind == "hard_break" {
            output.push(' ');
        }
        output.push_str(&inline_plain_text(&inline.children));
    }
    output
}

fn heading_anchor(text: &str) -> String {
    let anchor = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = anchor.trim_matches('-').replace("--", "-");
    if compact.is_empty() {
        "section".to_string()
    } else {
        compact
    }
}

fn diagnostic_to_ir(diagnostic: &Diagnostic) -> DiagnosticIr {
    DiagnosticIr {
        severity: match diagnostic.severity {
            DiagnosticSeverity::Warning => "warning".to_string(),
            DiagnosticSeverity::Error => "error".to_string(),
        },
        code: diagnostic.code.to_string(),
        message: diagnostic.message.clone(),
        line: diagnostic.line,
        column: diagnostic.column,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_semantic_document_with_anchor_ranges() {
        let input = "# Plan\n\n- [x] Ship\n\n| A | B |\n| - | - |\n| one | two |\n\n```rust\nfn main() {}\n```\n";
        let first = parse_document_ir_with_diagnostics(input, "file:///workspace/plan.md").unwrap();
        let second =
            parse_document_ir_with_diagnostics(input, "file:///workspace/plan.md").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.schema_version, DOCUMENT_IR_SCHEMA_VERSION);
        assert_eq!(first.outline[0].anchor, "plan");
        assert!(first.blocks.iter().all(|block| block.range.is_some()));
        assert!(first.blocks.iter().any(|block| block.kind == "table"));
        assert!(first.blocks.iter().any(|block| block.kind == "code"));
        assert!(
            serde_json::to_string(&first)
                .unwrap()
                .contains("schema_version")
        );
    }
}
