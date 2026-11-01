use mdstar_core::expand_emoji_text;

// Regression source only: not executed in the no-tests renderer migration.
#[test]
fn shortcode_byte_budget_does_not_slice_unicode() {
    for padding in 0..68 {
        let source = format!(":{}🦀🦀🦀🦀", "a".repeat(padding));
        assert_eq!(expand_emoji_text(&source), source);
    }
}

#[test]
fn unicode_near_valid_shortcode_keeps_surrounding_text() {
    assert_eq!(expand_emoji_text("🦀 :smile: 世界"), "🦀 😄 世界");
}
