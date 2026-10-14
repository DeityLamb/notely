//! Markdown shortcuts: text that turns into formatting as it is typed
//! (`# `, `**bold**`, `[label](url)`).

use std::ops::Range;
use std::rc::Rc;

use super::history::Edit;
use super::Editor;
use crate::doc::{Kind, Pos, Selection};
use crate::markdown;
use crate::text::{mark, RichText};

enum Format {
    Block(Kind, bool),
    Divider,
    Mark { open: Range<usize>, close: Range<usize>, mark: u8 },
    Link { open: usize, label_end: usize, url: Rc<str> },
}

impl Editor {
    /// Runs after a single typed char. The conversion is its own undo step,
    /// so undoing it brings back the literal characters.
    pub(super) fn auto_format(&mut self, c: char) {
        let Some(format) = self.detect(c) else { return };
        let Pos { block: i, offset } = self.sel.focus;
        self.begin(Edit::Other);
        match format {
            Format::Block(kind, checked) => {
                self.convert(i, kind);
                let b = self.touch(i);
                b.checked = checked;
                b.text.delete(0..offset);
                self.sel = Selection::caret(Pos::new(i, 0));
            }
            Format::Divider => {
                self.touch(i).text = RichText::new();
                self.divider_at(i);
            }
            Format::Mark { open, close, mark } => {
                let len = close.start - open.end;
                let text = &mut self.touch(i).text;
                text.delete(close);
                text.delete(open.clone());
                text.set_mark(open.start..open.start + len, mark, true);
                self.caret_after_format(Pos::new(i, open.start + len), mark);
            }
            Format::Link { open, label_end, url } => {
                let text = &mut self.touch(i).text;
                text.delete(label_end..offset);
                text.delete(open..open + 1);
                text.set_link(open..label_end - 1, Some(url));
                self.caret_after_format(Pos::new(i, label_end - 1), 0);
            }
        }
        self.end();
    }

    /// Places the caret after freshly formatted text without carrying the
    /// format into what is typed next.
    fn caret_after_format(&mut self, pos: Pos, mark: u8) {
        let mut attrs = self.blocks[pos.block].text.attrs_at(pos.offset);
        attrs.marks &= !mark;
        attrs.link = None;
        self.sel = Selection::caret(pos);
        self.pending = Some((pos, attrs));
    }

    fn detect(&self, c: char) -> Option<Format> {
        let Pos { block: i, offset } = self.sel.focus;
        let b = &self.blocks[i];
        if !b.kind.is_rich() {
            return None;
        }
        let text = b.text.as_str();
        let before = &text[..offset];
        if c == ' ' {
            let (kind, checked) = block_marker(before)?;
            return (kind != b.kind).then_some(Format::Block(kind, checked));
        }
        if c == '-' {
            return (b.kind == Kind::Paragraph && text == "---").then_some(Format::Divider);
        }
        if b.text.attrs_at(offset).marks & mark::CODE != 0 {
            return None;
        }
        if c == ')' {
            return link(before);
        }
        closed_mark(before)
    }
}

/// The block type that the text before the caret, ending in the space just
/// typed, is the marker of, and whether it is a checked to-do.
fn block_marker(before: &str) -> Option<(Kind, bool)> {
    match before {
        "[] " | "[ ] " => Some((Kind::Todo, false)),
        "[x] " | "[X] " => Some((Kind::Todo, true)),
        "``` " => Some((Kind::Code, false)),
        _ => match markdown::prefix(before) {
            Some((kind, checked, "")) if kind != Kind::Divider => Some((kind, checked)),
            _ => None,
        },
    }
}

/// A `[label](url)` that the `)` at the end of `before` completes.
fn link(before: &str) -> Option<Format> {
    let mid = before.rfind("](")?;
    let open = before[..mid].rfind('[')?;
    let url = &before[mid + 2..before.len() - 1];
    if mid == open + 1 || url.is_empty() || url.contains(char::is_whitespace) {
        return None;
    }
    Some(Format::Link { open, label_end: mid, url: Rc::from(url) })
}

/// A span like `**bold**` that the delimiter at the end of `before` closes.
fn closed_mark(before: &str) -> Option<Format> {
    const RULES: [(&str, u8); 5] = [
        ("**", mark::BOLD),
        ("~~", mark::STRIKE),
        ("`", mark::CODE),
        ("*", mark::ITALIC),
        ("_", mark::ITALIC),
    ];
    for (delim, mark) in RULES {
        if !before.ends_with(delim) {
            continue;
        }
        let close = before.len() - delim.len();
        let Some(open) = before[..close].rfind(delim) else { continue };
        let inner = &before[open + delim.len()..close];
        if inner.is_empty() || inner.starts_with(char::is_whitespace) || inner.ends_with(char::is_whitespace) {
            continue;
        }
        if delim.len() == 1 {
            // `*` must not be half of a `**`, and `_` not inside a word.
            let lead = &before[..open];
            if lead.ends_with(delim) || inner.starts_with(delim) || inner.ends_with(delim) {
                continue;
            }
            if delim == "_" && lead.ends_with(char::is_alphanumeric) {
                continue;
            }
        }
        return Some(Format::Mark { open: open..open + delim.len(), close: close..before.len(), mark });
    }
    None
}
