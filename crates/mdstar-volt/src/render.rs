//! Thin MD Star adapter over the shared native renderer.
use crate::{App, diagram};
use mdstar_core::document_ir::BlockIr;
pub use mdstar_render_volt::render::*;
use volt_ui_kit::ui::prelude::{Axis, ColorToken, Element, text, vstack};
pub type Ctx<'a> = mdstar_render_volt::Ctx<'a, App>;

impl mdstar_render_volt::Host for App {
    fn follow_link(&mut self, target: String) {
        App::follow_link(self, target);
    }
    fn copy(&mut self, source: &str, id: Option<String>) {
        App::copy(self, source, id);
    }
    fn open_row_menu(&mut self, row: usize, at: [f32; 2]) {
        self.open_menu(crate::app::MenuTarget::Row(row), at);
    }
    fn close_row_menu(&mut self) {
        self.menu = None;
    }
    fn row_action(&mut self, row: usize, action: u8) {
        App::row_action(self, row, action);
    }
}

pub struct ReaderMedia;
pub static MEDIA: ReaderMedia = ReaderMedia;
impl mdstar_render_volt::Media<App> for ReaderMedia {
    fn diagram(&self, block: &BlockIr, ctx: &Ctx) -> Element<App> {
        let source = block.code.as_deref().unwrap_or("");
        // Explicit export is synchronous; live layout never parses or rasterizes here.
        if ctx.controls {
            let drawing = diagram::request(source);
            present(drawing.as_ref(), block, ctx)
        } else {
            let drawing = diagram::diagram(source);
            present(drawing.as_ref(), block, ctx)
        }
    }
}
fn present(drawing: &diagram::Diagram, block: &BlockIr, ctx: &Ctx) -> Element<App> {
    match drawing {
        diagram::Diagram::Error(error) => vstack([
            mdstar_render_volt::code_whole(block, ctx),
            mdstar_render_volt::literal(&format!(
                "This Mermaid diagram could not be drawn: {error}"
            ))
            .font_size(12.)
            .color(ColorToken::Muted)
            .selectable(false),
        ])
        .key(format!("{}-diagram", key_of(&block.id)))
        .gap(6., Axis::Vertical)
        .into(),
        diagram::Diagram::Pending => mdstar_render_volt::diagram_frame(
            block,
            ctx,
            text("Rendering diagram…")
                .color(ColorToken::Muted)
                .height(160.)
                .into(),
            0.,
        ),
        d => {
            let scale = diagram::scale(d, ctx.typo.size, ctx.typo.room - 38.);
            let width = diagram::size(d).map_or(0., |s| s[0] * scale);
            mdstar_render_volt::diagram_frame(
                block,
                ctx,
                diagram::view(d, scale, &ctx.typo.code_family, ctx.typo.code_size()),
                width,
            )
        }
    }
}

pub fn unit_texts(unit: &Unit) -> Vec<String> {
    mdstar_render_volt::unit_texts_with(unit, |source| {
        diagram::peek(source).is_some_and(|d| matches!(d.as_ref(), diagram::Diagram::Error(_)))
    })
}
