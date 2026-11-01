//! Shared native Markdown rendering; app actions and media are explicit adapters.
pub mod find;
pub mod highlight;
pub mod render;
pub use mdstar_core;
use mdstar_core::document_ir::{BlockIr, InlineIr};
pub use render::*;
use volt_ui_kit::ui::prelude::Element;
pub trait Host: Sized + 'static {
    fn follow_link(&mut self, _target: String) {}
    fn copy(&mut self, _source: &str, _id: Option<String>) {}
    fn open_row_menu(&mut self, _row: usize, _at: [f32; 2]) {}
    fn close_row_menu(&mut self) {}
    fn row_action(&mut self, _row: usize, _action: u8) {}
}
pub trait Media<C: Host> {
    fn diagram(&self, block: &BlockIr, ctx: &Ctx<C>) -> Element<C>;
    fn math(&self, _block: &BlockIr, _ctx: &Ctx<C>) -> Option<Element<C>> {
        None
    }
    fn inlines(
        &self,
        _inlines: &[InlineIr],
        _size: f32,
        _key: &str,
        _available: f32,
        _ctx: &Ctx<C>,
    ) -> Option<Element<C>> {
        None
    }
}

#[cfg(feature = "diagrams")]
pub mod diagram;
