//! Typing, line breaks and Enter.

use std::ops::Range;

use super::history::Edit;
use super::Editor;
use crate::doc::{Kind, Pos, Selection};
use crate::text::{Attrs, RichText};

impl Editor {
    pub fn insert_text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let mut chars = s.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        let kind = match single {
            Some(c) if self.sel.is_collapsed() => {
                if c.is_whitespace() {
                    Edit::Space
                } else {
                    Edit::Typing
                }
            }
            _ => Edit::Other,
        };
        self.begin(kind);
        let pos = self.delete_range(self.sel.start(), self.sel.end());
        let pos = self.text_target(pos);
        let attrs = self.attrs_here(pos);
        self.pending = None;
        self.touch(pos.block).text.insert(pos.offset, s, &attrs);
        self.sel = Selection::caret(Pos::new(pos.block, pos.offset + s.len()));
        self.end();
        if let Some(c) = single {
            self.auto_format(c);
        }
    }

    pub fn insert_line_break(&mut self) {
        self.insert_text("\n");
    }

    /// Replaces part of a block's text with what the browser put there
    /// (IME composition, spellcheck), keeping surrounding formatting.
    pub fn replace_text(&mut self, block: usize, range: Range<usize>, s: &str) {
        if block >= self.blocks.len() || self.blocks[block].kind == Kind::Divider {
            return;
        }
        self.begin(Edit::Typing);
        let pos = Pos::new(block, range.start);
        let attrs = self.attrs_here(pos);
        self.pending = None;
        let text = &mut self.touch(block).text;
        text.delete(range);
        text.insert(pos.offset, s, &attrs);
        self.sel = Selection::caret(Pos::new(block, pos.offset + s.len()));
        self.end();
    }

    /// Enter.
    pub fn insert_paragraph(&mut self) {
        self.begin(Edit::Other);
        let pos = self.delete_range(self.sel.start(), self.sel.end());
        let i = pos.block;
        let (kind, indent, len) = {
            let b = &self.blocks[i];
            (b.kind, b.indent, b.text.len())
        };
        let text = self.blocks[i].text.as_str();
        let at_end = kind == Kind::Paragraph && pos.offset == len;
        let fence = at_end && text.starts_with("```");
        if kind == Kind::Divider || kind == Kind::Math {
            // A formula is one block however long: Enter goes on below it.
            let nb = self.new_block(Kind::Paragraph);
            self.splice(i + 1..i + 1, vec![nb]);
            self.sel = Selection::caret(Pos::new(i + 1, 0));
        } else if at_end && text == "$$" {
            let b = self.touch(i);
            b.kind = Kind::Math;
            b.text = RichText::new();
            self.sel = Selection::caret(Pos::new(i, 0));
        } else if kind == Kind::Code {
            self.touch(i).text.insert(pos.offset, "\n", &Attrs::default());
            self.sel = Selection::caret(Pos::new(i, pos.offset + 1));
        } else if fence {
            // What follows the fence names the language.
            let lang = crate::highlight::language(&self.blocks[i].text.as_str()[3..]);
            let b = self.touch(i);
            (b.kind, b.lang) = (Kind::Code, lang);
            b.text = RichText::new();
            self.sel = Selection::caret(Pos::new(i, 0));
        } else if len == 0 && (kind.is_list() || kind == Kind::Quote) {
            // Enter on an empty item steps out of the list.
            let b = self.touch(i);
            if b.indent > 0 {
                b.indent -= 1;
            } else {
                b.kind = Kind::Paragraph;
                b.checked = false;
            }
        } else {
            let mut nb = self.new_block(if kind.is_list() { kind } else { Kind::Paragraph });
            nb.indent = indent;
            if pos.offset == 0 && len > 0 {
                self.splice(i..i, vec![nb]);
            } else {
                nb.text = self.touch(i).text.split_off(pos.offset);
                self.splice(i + 1..i + 1, vec![nb]);
            }
            self.sel = Selection::caret(Pos::new(i + 1, 0));
        }
        self.end();
    }

    /// Mod+Enter: checks a to-do, otherwise opens a paragraph below.
    pub fn mod_enter(&mut self) {
        let i = self.sel.focus.block;
        self.begin(Edit::Other);
        if self.blocks[i].kind == Kind::Todo {
            self.touch(i).checked ^= true;
        } else {
            let nb = self.new_block(Kind::Paragraph);
            self.splice(i + 1..i + 1, vec![nb]);
            self.sel = Selection::caret(Pos::new(i + 1, 0));
        }
        self.end();
    }

    /// Dividers hold no text: typing on one goes to a new paragraph below.
    pub(super) fn text_target(&mut self, pos: Pos) -> Pos {
        if self.blocks[pos.block].kind != Kind::Divider {
            return pos;
        }
        let nb = self.new_block(Kind::Paragraph);
        self.splice(pos.block + 1..pos.block + 1, vec![nb]);
        Pos::new(pos.block + 1, 0)
    }
}
