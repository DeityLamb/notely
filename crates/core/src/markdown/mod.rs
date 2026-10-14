//! Markdown in and out: paste, copy and export.
//!
//! One line is one block. A trailing backslash continues the block on the
//! next line (a soft break inside the block).

mod inline;
mod parse;
mod serialize;

pub use inline::parse_inline;
pub use parse::parse;
pub(crate) use parse::prefix;
pub use serialize::{inline_to_string, serialize};
