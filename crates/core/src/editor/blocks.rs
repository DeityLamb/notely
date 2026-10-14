//! Whole blocks: indenting, moving, adding and removing them.

use std::ops::Range;

use super::history::Edit;
use super::Editor;
use crate::doc::{Kind, Pos, Selection, MAX_INDENT};

impl Editor {
    /// Tab / Shift+Tab.
    pub fn tab(&mut self, back: bool) {
        let (a, b) = (self.sel.start().block, self.sel.end().block);
        if a == b && self.blocks[a].kind == Kind::Code {
            if !back {
                self.insert_text("  ");
            }
            return;
        }
        self.begin(Edit::Other);
        if back {
            for i in a..=b {
                if self.blocks[i].indent > 0 {
                    self.touch(i).indent -= 1;
                }
            }
        } else {
            let limit = self.indent_limit(a);
            if self.blocks[a].indent < limit {
                for i in a..=b {
                    if self.blocks[i].indent < MAX_INDENT {
                        self.touch(i).indent += 1;
                    }
                }
            }
        }
        self.end();
    }

    /// Moves the selected blocks one position up or down.
    pub fn move_blocks(&mut self, up: bool) {
        let (a, b) = (self.sel.start().block, self.sel.end().block);
        if (up && a == 0) || (!up && b + 1 >= self.blocks.len()) {
            return;
        }
        self.begin(Edit::Other);
        if up {
            let blk = self.blocks[a - 1].clone();
            self.splice(a - 1..a, Vec::new());
            self.splice(b..b, vec![blk]);
            self.sel.anchor.block -= 1;
            self.sel.focus.block -= 1;
        } else {
            let blk = self.blocks[b + 1].clone();
            self.splice(b + 1..b + 2, Vec::new());
            self.splice(a..a, vec![blk]);
            self.sel.anchor.block += 1;
            self.sel.focus.block += 1;
        }
        self.end();
    }

    /// A block together with the more deeply indented blocks that follow it.
    pub fn subtree(&self, i: usize) -> Range<usize> {
        let depth = self.blocks[i].indent;
        let nested = self.blocks[i + 1..].iter().take_while(|b| b.indent > depth).count();
        i..i + 1 + nested
    }

    /// Moves a run of blocks to start where block `to` is now; `to` may be
    /// the block count, for the end of the document.
    pub fn move_range(&mut self, range: Range<usize>, to: usize) {
        let n = self.blocks.len();
        if range.is_empty() || range.end > n || to > n || (range.start..=range.end).contains(&to) {
            return;
        }
        self.begin(Edit::Other);
        let mut moved = self.blocks[range.clone()].to_vec();
        self.splice(range.clone(), Vec::new());
        let count = moved.len();
        let dest = if to > range.start { to - count } else { to };
        let limit = self.indent_limit(dest);
        let excess = moved[0].indent.saturating_sub(limit);
        for b in &mut moved {
            self.clock += 1;
            b.rev = self.clock;
            b.indent = b.indent.saturating_sub(excess);
        }
        self.splice(dest..dest, moved);
        let follow = |p: Pos| {
            let block = if range.contains(&p.block) {
                p.block - range.start + dest
            } else {
                let i = if p.block >= range.end { p.block - count } else { p.block };
                if i >= dest { i + count } else { i }
            };
            Pos::new(block, p.offset)
        };
        self.sel = Selection { anchor: follow(self.sel.anchor), focus: follow(self.sel.focus) };
        self.end();
    }

    /// Removes a run of blocks, leaving the caret where they were.
    pub fn delete_blocks(&mut self, range: Range<usize>) {
        let range = range.start..range.end.min(self.blocks.len());
        if range.is_empty() {
            return;
        }
        self.begin(Edit::Other);
        self.splice(range.clone(), Vec::new());
        self.sel = Selection::caret(Pos::new(range.start, 0));
        self.end();
    }

    /// Puts the caret in a new empty paragraph below a block and the blocks
    /// nested under it. An empty paragraph is used as it is.
    pub fn add_below(&mut self, i: usize) {
        let i = i.min(self.blocks.len() - 1);
        let b = &self.blocks[i];
        if b.kind == Kind::Paragraph && b.text.is_empty() {
            self.set_selection(Selection::caret(Pos::new(i, 0)));
            return;
        }
        let at = self.subtree(i).end;
        self.begin(Edit::Other);
        let mut nb = self.new_block(Kind::Paragraph);
        nb.indent = self.blocks[i].indent;
        self.splice(at..at, vec![nb]);
        self.sel = Selection::caret(Pos::new(at, 0));
        self.end();
    }

    /// Puts the caret in an empty paragraph at the end of the document,
    /// adding one if needed.
    pub fn focus_end(&mut self) {
        let n = self.blocks.len();
        let last = &self.blocks[n - 1];
        if last.kind == Kind::Paragraph && last.text.is_empty() {
            self.set_selection(Selection::caret(Pos::new(n - 1, 0)));
            return;
        }
        self.begin(Edit::Other);
        let nb = self.new_block(Kind::Paragraph);
        self.splice(n..n, vec![nb]);
        self.sel = Selection::caret(Pos::new(n, 0));
        self.end();
    }

    /// The deepest indent a block at `i` can have: a block can sit at most
    /// one level below the one above it.
    fn indent_limit(&self, i: usize) -> u8 {
        match i {
            0 => 0,
            _ => (self.blocks[i - 1].indent + 1).min(MAX_INDENT),
        }
    }
}
