use markdown::mdast::Node;
use markdown::{Constructs, ParseOptions};
use thiserror::Error;

pub mod document_ir;

// ─── Inline nodes ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    /// Inline code span: `code`
    Code(String),
    /// Inline math: $expr$
    Math(String),
    /// GFM strikethrough: ~~text~~
    Delete(Vec<Inline>),
    Link {
        children: Vec<Inline>,
        url: String,
        title: Option<String>,
    },
    Image {
        alt: String,
        url: String,
        title: Option<String>,
    },
    /// Raw HTML inline
    Html(String),
    /// Soft line break (single newline in source)
    SoftBreak,
    /// Hard line break (trailing spaces or `\` in source)
    HardBreak,
}

// ─── Block nodes ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    pub children: Vec<Block>,
    pub checked: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        children: Vec<Inline>,
    },
    Paragraph {
        children: Vec<Inline>,
    },
    BlockQuote(Vec<Block>),
    List {
        ordered: bool,
        start: Option<u32>,
        items: Vec<ListItem>,
    },
    CodeFence {
        language: Option<String>,
        meta: Option<String>,
        code: String,
    },
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    ThematicBreak,
    /// Raw HTML block
    Html(String),
    /// Block math: $$expr$$
    Math(String),
    /// YAML or TOML frontmatter value (raw string, leading/trailing `---` stripped)
    Frontmatter(String),
}

// ─── Document ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub metadata: DocumentMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentMetadata {
    pub headings: Vec<HeadingMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingMetadata {
    pub level: u8,
    pub text: String,
}

// ─── Diagnostics ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: &'static str,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParseOutput {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
}

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("parser adapter failure: {0}")]
    ParserAdapter(String),
}

pub type Result<T> = std::result::Result<T, MarkdownError>;

// ─── Parser adapter trait ────────────────────────────────────────────────────

pub trait ParserAdapter {
    fn parse(&self, input: &str) -> Result<ParseOutput>;
}

// ─── Public parse functions ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default)]
pub struct MarkdownRsParser;

impl ParserAdapter for MarkdownRsParser {
    fn parse(&self, input: &str) -> Result<ParseOutput> {
        let document = parse_with_markdown_rs(input)?;
        let diagnostics = collect_diagnostics(input, &document);
        Ok(ParseOutput {
            document,
            diagnostics,
        })
    }
}

pub fn parse_with_adapter<A: ParserAdapter>(adapter: &A, input: &str) -> Result<ParseOutput> {
    adapter.parse(input)
}

pub fn parse_markdown_with_diagnostics(input: &str) -> Result<ParseOutput> {
    MarkdownRsParser.parse(input)
}

pub fn parse_markdown(input: &str) -> Result<Document> {
    parse_markdown_with_diagnostics(input).map(|o| o.document)
}

// ─── markdown-rs parsing ─────────────────────────────────────────────────────

fn build_parse_options() -> ParseOptions {
    ParseOptions {
        constructs: Constructs {
            frontmatter: true,
            math_flow: true,
            math_text: true,
            ..Constructs::gfm()
        },
        ..ParseOptions::default()
    }
}

fn parse_with_markdown_rs(input: &str) -> Result<Document> {
    let options = build_parse_options();
    let node = markdown::to_mdast(input, &options)
        .map_err(|msg| MarkdownError::ParserAdapter(msg.reason.clone()))?;

    let mut document = Document::default();

    let Node::Root(root) = node else {
        return Ok(document);
    };

    for child in &root.children {
        if let Some(block) = node_to_block(child) {
            // Collect heading metadata from the parsed block.
            if let Block::Heading { level, children } = &block {
                document.metadata.headings.push(HeadingMetadata {
                    level: *level,
                    text: inlines_to_plain_text(children),
                });
            }
            document.blocks.push(block);
        }
    }

    Ok(document)
}

// ─── Block conversion ────────────────────────────────────────────────────────

fn node_to_block(node: &Node) -> Option<Block> {
    match node {
        Node::Heading(h) => Some(Block::Heading {
            level: h.depth,
            children: collect_inlines(&h.children),
        }),

        Node::Paragraph(p) => Some(Block::Paragraph {
            children: collect_inlines(&p.children),
        }),

        Node::Blockquote(bq) => Some(Block::BlockQuote(nodes_to_blocks(&bq.children))),

        Node::List(list) => {
            let items = list
                .children
                .iter()
                .filter_map(|item_node| {
                    if let Node::ListItem(item) = item_node {
                        Some(ListItem {
                            children: nodes_to_blocks(&item.children),
                            checked: item.checked,
                        })
                    } else {
                        None
                    }
                })
                .collect();
            Some(Block::List {
                ordered: list.ordered,
                start: list.start,
                items,
            })
        }

        Node::Code(code) => {
            let language = code.lang.clone().filter(|s| !s.is_empty());
            let meta = code.meta.clone().filter(|s| !s.is_empty());
            Some(Block::CodeFence {
                language,
                meta,
                code: code.value.trim_end_matches('\n').to_string(),
            })
        }

        Node::Math(math) => Some(Block::Math(math.value.clone())),

        Node::Table(table) => {
            let mut headers: Vec<Vec<Inline>> = Vec::new();
            let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();

            for (row_idx, row_node) in table.children.iter().enumerate() {
                if let Node::TableRow(row) = row_node {
                    let cells: Vec<Vec<Inline>> = row
                        .children
                        .iter()
                        .filter_map(|cell| {
                            if let Node::TableCell(cell) = cell {
                                Some(collect_inlines(&cell.children))
                            } else {
                                None
                            }
                        })
                        .collect();

                    if row_idx == 0 {
                        headers = cells;
                    } else {
                        rows.push(cells);
                    }
                }
            }

            Some(Block::Table { headers, rows })
        }

        Node::ThematicBreak(_) => Some(Block::ThematicBreak),

        Node::Html(html) => Some(Block::Html(html.value.clone())),

        Node::Yaml(yaml) => Some(Block::Frontmatter(yaml.value.clone())),
        Node::Toml(toml) => Some(Block::Frontmatter(toml.value.clone())),

        // Definition nodes (link reference targets) are part of the AST but
        // don't produce rendered output — they are referenced by LinkReference
        // and ImageReference nodes.  We skip them at the block level.
        Node::Definition(_) | Node::FootnoteDefinition(_) => None,

        _ => None,
    }
}

fn nodes_to_blocks(nodes: &[Node]) -> Vec<Block> {
    nodes.iter().filter_map(node_to_block).collect()
}

// ─── Inline conversion ───────────────────────────────────────────────────────

fn collect_inlines(nodes: &[Node]) -> Vec<Inline> {
    nodes.iter().filter_map(node_to_inline).collect()
}

fn node_to_inline(node: &Node) -> Option<Inline> {
    match node {
        Node::Text(t) => Some(Inline::Text(t.value.clone())),

        Node::Strong(s) => Some(Inline::Strong(collect_inlines(&s.children))),
        Node::Emphasis(e) => Some(Inline::Emphasis(collect_inlines(&e.children))),
        Node::Delete(d) => Some(Inline::Delete(collect_inlines(&d.children))),

        Node::InlineCode(c) => Some(Inline::Code(c.value.clone())),
        Node::InlineMath(m) => Some(Inline::Math(m.value.clone())),

        Node::Link(link) => Some(Inline::Link {
            children: collect_inlines(&link.children),
            url: link.url.clone(),
            title: link.title.clone(),
        }),

        Node::Image(img) => Some(Inline::Image {
            alt: img.alt.clone(),
            url: img.url.clone(),
            title: img.title.clone(),
        }),

        // Resolved reference variants — treat like their counterparts.
        Node::LinkReference(lr) => Some(Inline::Link {
            children: collect_inlines(&lr.children),
            url: String::new(), // URL only known via Definition lookup
            title: None,
        }),
        Node::ImageReference(ir) => Some(Inline::Image {
            alt: ir.alt.clone(),
            url: String::new(),
            title: None,
        }),

        Node::Html(html) => Some(Inline::Html(html.value.clone())),
        Node::Break(_) => Some(Inline::HardBreak),

        // Footnote references render as a superscript marker; skip for now.
        Node::FootnoteReference(_) => None,

        // Paragraphs can appear nested inside list-item tight content.
        // Flatten them into their inline children.
        Node::Paragraph(p) => {
            let children = collect_inlines(&p.children);
            // Return as a sequence — callers collect via filter_map so we can't
            // return Vec<Inline> here directly.  Wrap in a single Text if empty,
            // or emit as-is by returning None and letting callers use
            // `flat_map`.  For now inline all children as SoftBreak-separated.
            if children.is_empty() {
                None
            } else {
                // Flatten: return first child and push rest… we can't do this
                // with Option<Inline>.  Use a synthetic concat node instead.
                Some(children.into_iter().reduce(|mut acc, next| {
                    // We have no "concat" variant so just push SoftBreak-joined text.
                    // This only happens for nested paragraphs (tight lists), which
                    // is rare.  A full fix requires Vec<Inline> return type.
                    if let Inline::Text(ref mut t) = acc
                        && let Inline::Text(s) = next
                    {
                        t.push_str(&s);
                    }
                    acc
                })?)
            }
        }

        _ => None,
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract a plain-text string from a slice of `Inline` nodes (for metadata).
pub fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        inline_plain_text(inline, &mut out);
    }
    out
}

fn inline_plain_text(inline: &Inline, out: &mut String) {
    match inline {
        Inline::Text(t) => out.push_str(t),
        Inline::Code(c) => out.push_str(c),
        Inline::Math(m) => out.push_str(m),
        Inline::Html(h) => out.push_str(h),
        Inline::Strong(children)
        | Inline::Emphasis(children)
        | Inline::Delete(children)
        | Inline::Link { children, .. } => {
            for child in children {
                inline_plain_text(child, out);
            }
        }
        Inline::Image { alt, .. } => out.push_str(alt),
        Inline::SoftBreak | Inline::HardBreak => out.push(' '),
    }
}

// ─── Diagnostics ─────────────────────────────────────────────────────────────

fn collect_diagnostics(input: &str, document: &Document) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if input.trim().is_empty() {
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "MD000",
            message: "document is empty".to_string(),
            line: None,
            column: None,
        });
    }

    for (index, line) in input.lines().enumerate() {
        let line_no = index + 1;
        if line.contains('\t') {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "MD001",
                message: "tab character detected; prefer spaces for stable rendering".to_string(),
                line: Some(line_no),
                column: line.find('\t').map(|v| v + 1),
            });
        }
        if line.ends_with(' ') {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "MD002",
                message: "line has trailing whitespace".to_string(),
                line: Some(line_no),
                column: Some(line.len()),
            });
        }
    }

    let mut previous_level: Option<u8> = None;
    for heading in &document.metadata.headings {
        if let Some(prev) = previous_level
            && heading.level > prev + 1
        {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "MD003",
                message: format!("heading level jump from H{} to H{}", prev, heading.level),
                line: None,
                column: None,
            });
        }
        previous_level = Some(heading.level);
    }

    diagnostics
}
