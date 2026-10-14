//! Formatting: marks, colours and links on text, and the type of a block.

use std::rc::Rc;

use super::history::Edit;
use super::Editor;
use crate::doc::{Kind, Pos, Selection};
use crate::text::{Attrs, RichText, COLORS};

impl Editor {
    pub fn toggle_mark(&mut self, m: u8) {
        if self.sel.is_collapsed() {
            let pos = self.sel.focus;
            if self.blocks[pos.block].kind.is_rich() {
                let mut attrs = self.attrs_here(pos);
                attrs.marks ^= m;
                self.pending = Some((pos, attrs));
            }
            return;
        }
        let ranges = self.sel_ranges();
        if ranges.is_empty() {
            return;
        }
        let on = !ranges.iter().all(|(i, r)| self.blocks[*i].text.has_mark(r.clone(), m));
        self.begin(Edit::Other);
        for (i, r) in ranges {
            self.touch(i).text.set_mark(r, m, on);
        }
        self.end();
    }

    /// Colours the selected text, or text typed next at the caret: an index
    /// into `COLORS`, 0 taking the colour away.
    pub fn set_color(&mut self, color: u8) {
        self.restyle(color, |a| &mut a.color);
    }

    /// Sets the background of the selected text, or of text typed next at
    /// the caret, like `set_color`.
    pub fn set_background(&mut self, bg: u8) {
        self.restyle(bg, |a| &mut a.bg);
    }

    fn restyle(&mut self, value: u8, field: impl Fn(&mut Attrs) -> &mut u8) {
        if usize::from(value) >= COLORS.len() {
            return;
        }
        if self.sel.is_collapsed() {
            let pos = self.sel.focus;
            if self.blocks[pos.block].kind.is_rich() {
                let mut attrs = self.attrs_here(pos);
                *field(&mut attrs) = value;
                self.pending = Some((pos, attrs));
            }
            return;
        }
        let ranges = self.sel_ranges();
        if ranges.is_empty() {
            return;
        }
        self.begin(Edit::Other);
        for (i, r) in ranges {
            self.touch(i).text.restyle(r, |a| *field(a) = value);
        }
        self.end();
    }

    pub fn set_link(&mut self, url: Option<&str>) {
        let ranges = self.sel_ranges();
        if ranges.is_empty() {
            return;
        }
        let link: Option<Rc<str>> = url.map(Rc::from);
        self.begin(Edit::Other);
        for (i, r) in ranges {
            self.touch(i).text.set_link(r, link.clone());
        }
        self.end();
    }

    /// Sets the type of every selected block; selecting the type they all
    /// already have turns them back into paragraphs.
    pub fn set_kind(&mut self, kind: Kind) {
        self.begin(Edit::Other);
        if kind == Kind::Divider {
            self.divider_at(self.sel.focus.block);
        } else {
            let (a, b) = (self.sel.start().block, self.sel.end().block);
            let all = (a..=b).all(|i| self.blocks[i].kind == kind);
            let kind = if all { Kind::Paragraph } else { kind };
            for i in a..=b {
                self.convert(i, kind);
            }
        }
        self.end();
    }

    /// Applies a slash-menu choice: removes the typed `/query` (from `start`
    /// to the caret) and converts the block.
    pub fn run_slash(&mut self, start: Pos, kind: Kind) {
        self.begin(Edit::Other);
        let end = self.sel.focus;
        let start = self.clamp_pos(start);
        let pos = if start.block == end.block && start <= end {
            self.delete_range(start, end)
        } else {
            end
        };
        self.sel = Selection::caret(pos);
        if kind == Kind::Divider {
            self.divider_at(pos.block);
        } else {
            self.convert(pos.block, kind);
        }
        self.end();
    }

    /// Sets the language of a code block: an index into
    /// `highlight::LANGUAGES`.
    pub fn set_lang(&mut self, i: usize, lang: u8) {
        let known = usize::from(lang) < crate::highlight::LANGUAGES.len();
        if known && i < self.blocks.len() && self.blocks[i].kind == Kind::Code && self.blocks[i].lang != lang {
            self.begin(Edit::Other);
            self.touch(i).lang = lang;
            self.end();
        }
    }

    /// Switches a code block between wrapping long lines and cutting them.
    pub fn toggle_wrap(&mut self, i: usize) {
        if i < self.blocks.len() && self.blocks[i].kind == Kind::Code {
            self.begin(Edit::Other);
            self.touch(i).nowrap ^= true;
            self.end();
        }
    }

    pub fn toggle_todo(&mut self, i: usize) {
        if i < self.blocks.len() && self.blocks[i].kind == Kind::Todo {
            self.begin(Edit::Other);
            self.touch(i).checked ^= true;
            self.end();
        }
    }

    pub(super) fn convert(&mut self, i: usize, kind: Kind) {
        let from = self.blocks[i].kind;
        if from == kind || from == Kind::Divider {
            return;
        }
        let b = self.touch(i);
        b.kind = kind;
        (b.checked, b.lang, b.nowrap) = (false, 0, false);
        if !kind.is_rich() {
            b.text = RichText::plain(b.text.as_str());
        }
    }

    /// Turns block `i` into a divider if it is empty, otherwise adds one
    /// below it, and leaves the caret in a text block after the divider.
    pub(super) fn divider_at(&mut self, i: usize) {
        let divider = self.new_block(Kind::Divider);
        let cur = &self.blocks[i];
        let at = if cur.text.is_empty() && cur.kind != Kind::Divider {
            self.splice(i..i + 1, vec![divider]);
            i
        } else {
            self.splice(i + 1..i + 1, vec![divider]);
            i + 1
        };
        if self.blocks.get(at + 1).is_none_or(|b| b.kind == Kind::Divider) {
            let nb = self.new_block(Kind::Paragraph);
            self.splice(at + 1..at + 1, vec![nb]);
        }
        self.sel = Selection::caret(Pos::new(at + 1, 0));
    }
}
