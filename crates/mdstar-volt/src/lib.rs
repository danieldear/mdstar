//! MD Star: the Markdown reader and editor, its renderers and `md`. The app
//! (`src/main.rs`) and the Quick Look preview extension (`src/preview.rs`)
//! are thin binaries over this library.

pub mod app;
pub mod cli;
pub mod compose;
pub mod diagram;
pub mod export;
pub mod find;
pub mod highlight;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod render;
pub mod settings;
pub mod terminal;
pub mod view;
pub mod workspace;

pub use app::App;

/// The app's bundle identifier, from `[package.metadata.volt.bundle]`.
pub const BUNDLE_ID: &str = "dev.mdstar.volt";

/// File extensions opened as Markdown.
pub const MARKDOWN: [&str; 5] = ["md", "markdown", "mdown", "mkd", "txt"];
