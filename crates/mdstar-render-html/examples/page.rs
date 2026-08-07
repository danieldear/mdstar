use mdstar_core::document_ir::parse_document_ir_with_diagnostics;
fn main() {
    let p = std::env::args().nth(1).unwrap();
    let dark = std::env::args()
        .nth(2)
        .map(|v| v == "dark")
        .unwrap_or(false);
    let input = std::fs::read_to_string(&p).unwrap();
    let ir = parse_document_ir_with_diagnostics(&input, &p).unwrap();
    let body = mdstar_render_html::render_document_ir(&ir);
    let theme = if dark {
        ":root{--reader-text:#f2f2f5;--reader-secondary:#a2a2a9;--reader-accent:#3f92ff;\
         --reader-hairline:rgba(255,255,255,0.13);--reader-fill:rgba(255,255,255,0.055);\
         --reader-fill-strong:rgba(255,255,255,0.09);--reader-code-bg:rgba(255,255,255,0.07);\
         --reader-syntax-comment:#7fd18c;--reader-syntax-string:#ff8f7a;\
         --reader-syntax-number:#b39bff;--reader-syntax-keyword:#ff7ab6;}\
         body{background:#1d1d1f;}"
    } else {
        "body{background:#fff;}"
    };
    println!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"/><style>{}</style><style>{}</style></head><body><div id=\"reader-root\">{}</div></body></html>",
        mdstar_render_html::base_stylesheet(),
        theme,
        body
    );
}
