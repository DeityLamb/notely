//! Deleting text, and joining the blocks a deletion brings together.

use super::history::Edit;
use super::Editor;
use crate::doc::{Kind, Pos, Selection};
use crate::text::{Attrs, RichText};

impl Editor {
    pub fn delete_selection(&mut self) {
        self.begin(Edit::Other);
        let pos = self.delete_range(self.sel.start(), self.sel.end());
        self.sel = Selection::caret(pos);
        self.end();
    }

    /// Deletes an explicit range, e.g. the one the browser reports for
    /// word or line deletion.
    pub fn delete_span(&mut self, a: Pos, b: Pos) {
        let (a, b) = (self.clamp_pos(a.min(b)), self.clamp_pos(a.max(b)));
        self.begin(if a.block == b.block { Edit::Deleting } else { Edit::Other });
        let pos = self.delete_range(a, b);
        self.sel = Selection::caret(pos);
        self.end();
    }

    pub fn delete_backward(&mut self) {
        if !self.sel.is_collapsed() {
            return self.delete_selection();
        }
        let Pos { block: i, offset } = self.sel.focus;
        if offset > 0 {
            let text = self.blocks[i].text.as_str();
            let n = text[..offset].chars().next_back().map_or(0, char::len_utf8);
            return self.delete_span(Pos::new(i, offset - n), self.sel.focus);
        }
        // At the start of a block: unwrap its type, then outdent, then join
        // it to the block above.
        self.begin(Edit::Other);
        let (kind, indent) = (self.blocks[i].kind, self.blocks[i].indent);
        if kind != Kind::Paragraph {
            let b = self.touch(i);
            b.kind = Kind::Paragraph;
            (b.checked, b.lang, b.nowrap) = (false, 0, false);
        } else if indent > 0 {
            self.touch(i).indent -= 1;
        } else if i > 0 {
            let prev = &self.blocks[i - 1];
            if prev.kind == Kind::Divider || prev.text.is_empty() {
                self.splice(i - 1..i, Vec::new());
                self.sel = Selection::caret(Pos::new(i - 1, 0));
            } else {
                let at = prev.text.len();
                let cur = self.blocks[i].text.clone();
                self.join(i - 1, cur);
                self.splice(i..i + 1, Vec::new());
                self.sel = Selection::caret(Pos::new(i - 1, at));
            }
        }
        self.end();
    }

    pub fn delete_forward(&mut self) {
        if !self.sel.is_collapsed() {
            return self.delete_selection();
        }
        let Pos { block: i, offset } = self.sel.focus;
        let (kind, len) = (self.blocks[i].kind, self.blocks[i].text.len());
        if offset < len {
            let text = self.blocks[i].text.as_str();
            let n = text[offset..].chars().next().map_or(0, char::len_utf8);
            return self.delete_span(self.sel.focus, Pos::new(i, offset + n));
        }
        self.begin(Edit::Other);
        if kind == Kind::Divider {
            self.splice(i..i + 1, Vec::new());
        } else if i + 1 < self.blocks.len() {
            if self.blocks[i + 1].kind == Kind::Divider {
                self.splice(i + 1..i + 2, Vec::new());
            } else if len == 0 {
                self.splice(i..i + 1, Vec::new());
            } else {
                let next = self.blocks[i + 1].text.clone();
                self.join(i, next);
                self.splice(i + 1..i + 2, Vec::new());
            }
        }
        self.end();
    }

    /// Deletes `a..b` (`a <= b`), joining the first and last block, and
    /// returns where the caret ends up.
    pub(super) fn delete_range(&mut self, a: Pos, b: Pos) -> Pos {
        if a == b {
            return a;
        }
        if a.block == b.block {
            self.touch(a.block).text.delete(a.offset..b.offset);
            return a;
        }
        if self.blocks[a.block].kind == Kind::Divider {
            // A selection starting on a divider takes the divider with it.
            self.touch(b.block).text.delete(0..b.offset);
            self.splice(a.block..b.block, Vec::new());
            return Pos::new(a.block, 0);
        }
        let len = self.blocks[a.block].text.len();
        self.touch(a.block).text.delete(a.offset..len);
        if self.blocks[b.block].kind == Kind::Divider {
            self.splice(a.block + 1..b.block, Vec::new());
        } else {
            let last = &self.blocks[b.block].text;
            let tail = last.slice(b.offset..last.len());
            self.join(a.block, tail);
            self.splice(a.block + 1..b.block + 1, Vec::new());
        }
        a
    }

    /// Appends text to a block; code and math blocks take it without
    /// formatting.
    pub(super) fn join(&mut self, i: usize, text: RichText) {
        let b = self.touch(i);
        if !b.kind.is_rich() {
            b.text.push(text.as_str(), &Attrs::default());
        } else {
            b.text.append(text);
        }
    }
}
