use mdstar_core::parse_markdown;
use mdstar_render_terminal::{
    RenderOptions, preprocess_mermaid_blocks, render_markdown, render_terminal,
};

#[test]
fn renders_basic_blocks_for_terminal() {
    let source = include_str!("../../../tests/fixtures/sample.md");
    let doc = parse_markdown(source).expect("expected parse success");

    let output = render_terminal(
        &doc,
        RenderOptions {
            color: false,
            ..RenderOptions::default()
        },
    );

    assert!(output.contains("Sample Document"));
    assert!(output.contains("first item"));
    assert!(output.contains("fn demo()"));
}

#[test]
fn renders_mermaid_blocks_in_preprocessing() {
    let input = r#"# Diagram

```mermaid
graph LR
    A[Build] --> B[Test]
```
"#;

    let output = preprocess_mermaid_blocks(input, RenderOptions::default());
    assert!(output.contains("```mermaid"));
    assert!(output.contains("Build"));
    assert!(output.contains("Test"));
}

#[test]
fn renders_markdown_tables_with_terminal_borders() {
    let input = r#"| Name | Value |
| ---- | ----- |
| A    | 1     |
"#;

    let output = render_markdown(
        input,
        RenderOptions {
            color: false,
            ..RenderOptions::default()
        },
    );

    assert!(output.contains("Name"));
    assert!(output.contains("Value"));
    assert!(output.contains("A"));
}

#[test]
fn preserves_long_table_cell_content_without_truncation() {
    let marker = "LONG_TABLE_TOKEN_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";
    let input = format!("| Name | Value |\n| ---- | ----- |\n| A | {marker} |\n");

    let output = render_markdown(
        &input,
        RenderOptions {
            color: false,
            width: 60,
            ..RenderOptions::default()
        },
    );

    assert!(output.contains(marker));
}

#[test]
fn can_render_mermaid_in_ascii_mode() {
    let input = r#"```mermaid
graph LR
    A[Build] --> B[Deploy]
```
"#;

    let preprocessed = preprocess_mermaid_blocks(
        input,
        RenderOptions {
            ascii_mermaid: true,
            ..RenderOptions::default()
        },
    );

    assert!(preprocessed.is_ascii());
    assert!(preprocessed.contains("Build"));
    assert!(preprocessed.contains("Deploy"));
}

#[test]
fn expands_emoji_and_renders_common_latex_as_readable_terminal_text() {
    let input = r#"Classic: :wink: :cry: :laughing: :yum:

Shortcuts: :-) :-( 8-) ;)

Quadratic: \(x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}\)

Physics: $E = mc^2$ and $F = ma$
"#;

    let output = render_markdown(
        input,
        RenderOptions {
            color: false,
            ..RenderOptions::default()
        },
    );

    for emoji in ["😉", "😢", "😆", "😋", "😄", "😞", "😎"] {
        assert!(output.contains(emoji), "missing {emoji:?} in: {output}");
    }
    assert!(!output.contains(":wink:"), "got: {output}");
    assert!(!output.contains(r"\frac"), "got: {output}");
    assert!(output.contains('±'), "got: {output}");
    assert!(output.contains('√'), "got: {output}");
    assert!(output.contains("b²"), "got: {output}");
    assert!(output.contains("mc²"), "got: {output}");
}

#[test]
fn rich_text_expansion_leaves_code_examples_literal() {
    let input = r#"Text :rocket: and $E = mc^2$.

Inline code: `:wink: \(x = \frac{1}{2}\)`

```text
:cry: $F = ma$ \(x = \sqrt{4}\)
```
"#;

    let output = render_markdown(
        input,
        RenderOptions {
            color: false,
            ..RenderOptions::default()
        },
    );

    assert!(output.contains('🚀'), "got: {output}");
    assert!(output.contains("mc²"), "got: {output}");
    assert!(output.contains(":wink:"), "got: {output}");
    assert!(output.contains(r"\frac{1}{2}"), "got: {output}");
    assert!(output.contains(":cry: $F = ma$"), "got: {output}");
    assert!(output.contains(r"\sqrt{4}"), "got: {output}");
}
