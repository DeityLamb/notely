//! Document model and editing commands. No DOM, no JS: everything here runs
//! (and is tested) natively, the app crate only maps events to commands.

pub mod doc;
pub mod editor;
pub mod highlight;
pub mod markdown;
pub mod store;
pub mod text;

pub use doc::{Block, Kind, Page, Pos, Selection, MAX_INDENT};
pub use editor::Editor;
pub use text::{mark, Attrs, RichText, COLORS};

/// Markdown of the document a new user starts with.
pub const WELCOME: &str = include_str!("../welcome.md");
