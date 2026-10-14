//! Editing commands over the block list, with undo history.
//!
//! Every change goes through `touch` (edit one block) or `splice` (replace a
//! range of blocks). Both record what they overwrite into the open undo
//! group, so undo costs as much as the change itself, not the document.
//!
//! This file holds the editor's state and what can be asked of it. The
//! commands are grouped by what they do: `input` (typing and Enter),
//! `delete`, `format` (marks, links, block types), `blocks` (indenting,
//! moving, adding and removing whole blocks), `clipboard`, and `shortcuts`
//! (Markdown typed into the text). `history` has undo and the primitives.

mod blocks;
mod clipboard;
mod delete;
mod format;
mod history;
mod input;
mod shortcuts;

use std::ops::Range;
use std::rc::Rc;

use crate::doc::{Block, Kind, Page, Pos, Selection};
use crate::text::{mark, Attrs, RichText};
use history::{Edit, Group};

pub struct Editor {
    page: Page,
    blocks: Vec<Block>,
    sel: Selection,
    /// Formatting toggled at a caret, applied to the next typed text there.
    pending: Option<(Pos, Attrs)>,
    next_id: u32,
    clock: u32,
    undo: Vec<Group>,
    redo: Vec<Group>,
    /// Kind, time and resulting caret of the last edit.
    last: Option<(Edit, f64, Pos)>,
    now: f64,
}

impl Default for Editor {
    fn default() -> Self {
        Self::from_blocks(Vec::new())
    }
}

impl Editor {
    pub fn from_blocks(blocks: Vec<Block>) -> Self {
        Self::from_page(Page::default(), blocks)
    }

    pub fn from_page(page: Page, mut blocks: Vec<Block>) -> Self {
        if blocks.is_empty() {
            blocks.push(Block::new(Kind::Paragraph, RichText::new()));
        }
        let mut id = 0;
        for b in &mut blocks {
            id += 1;
            b.id = id;
            b.rev = id;
        }
        Editor {
            page,
            blocks,
            sel: Selection::default(),
            pending: None,
            next_id: id,
            clock: id,
            undo: Vec::new(),
            redo: Vec::new(),
            last: None,
            now: 0.0,
        }
    }

    pub fn page(&self) -> &Page {
        &self.page
    }

    /// Changes the page's title, icon or cover. These are not part of the
    /// undo history.
    pub fn set_page(&mut self, page: Page) {
        if page != self.page {
            self.page = page;
            self.clock += 1;
        }
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn selection(&self) -> Selection {
        self.sel
    }

    /// Changes whenever the document does.
    pub fn version(&self) -> u32 {
        self.clock
    }

    /// Current time in milliseconds, used to group typing into undo steps.
    pub fn set_time(&mut self, ms: f64) {
        self.now = ms;
    }

    pub fn set_selection(&mut self, sel: Selection) {
        self.sel = self.clamp(sel);
        if self.pending.as_ref().is_some_and(|(p, _)| Selection::caret(*p) != self.sel) {
            self.pending = None;
        }
    }

    pub fn has_rich_selection(&self) -> bool {
        !self.sel_ranges().is_empty()
    }

    /// Marks active at the caret, or shared by the whole selection.
    pub fn active_marks(&self) -> u8 {
        if self.sel.is_collapsed() {
            return self.attrs_here(self.sel.focus).marks;
        }
        let ranges = self.sel_ranges();
        mark::ALL
            .into_iter()
            .filter(|&m| {
                !ranges.is_empty()
                    && ranges.iter().all(|(i, r)| self.blocks[*i].text.has_mark(r.clone(), m))
            })
            .sum()
    }

    /// Link at the start of the selection.
    pub fn link(&self) -> Option<Rc<str>> {
        let p = self.sel.start();
        self.blocks[p.block].text.link_at(p.offset)
    }

    /// The link the caret is in or next to: its extent and target.
    pub fn link_at_caret(&self) -> Option<(Selection, Rc<str>)> {
        self.link_span_at(self.sel.focus).filter(|_| self.sel.is_collapsed())
    }

    /// The link covering or touching `pos`: its extent and target.
    pub fn link_span_at(&self, pos: Pos) -> Option<(Selection, Rc<str>)> {
        let (range, url) = self.blocks.get(pos.block)?.text.link_span(pos.offset)?;
        let span = Selection {
            anchor: Pos::new(pos.block, range.start),
            focus: Pos::new(pos.block, range.end),
        };
        Some((span, url))
    }

    // ---- helpers for the commands ----

    fn attrs_here(&self, pos: Pos) -> Attrs {
        let b = &self.blocks[pos.block];
        if !b.kind.is_rich() {
            return Attrs::default();
        }
        match &self.pending {
            Some((p, attrs)) if *p == pos => attrs.clone(),
            _ => b.text.attrs_at(pos.offset),
        }
    }

    /// Selected byte range of each formattable block in the selection.
    fn sel_ranges(&self) -> Vec<(usize, Range<usize>)> {
        let (a, b) = (self.sel.start(), self.sel.end());
        (a.block..=b.block)
            .filter_map(|i| {
                let blk = &self.blocks[i];
                let start = if i == a.block { a.offset } else { 0 };
                let end = if i == b.block { b.offset } else { blk.text.len() };
                (blk.kind.is_rich() && start < end).then_some((i, start..end))
            })
            .collect()
    }

    fn clamp_pos(&self, p: Pos) -> Pos {
        let block = p.block.min(self.blocks.len() - 1);
        let text = self.blocks[block].text.as_str();
        let mut offset = p.offset.min(text.len());
        while !text.is_char_boundary(offset) {
            offset -= 1;
        }
        Pos { block, offset }
    }

    fn clamp(&self, s: Selection) -> Selection {
        Selection { anchor: self.clamp_pos(s.anchor), focus: self.clamp_pos(s.focus) }
    }
}
