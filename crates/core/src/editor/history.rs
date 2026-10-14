//! Undo history, and the primitives every change is made of.
//!
//! A command opens an undo group with `begin`, changes blocks through
//! `touch` and `splice`, and closes the group with `end`.

use std::collections::HashSet;
use std::ops::Range;

use super::Editor;
use crate::doc::{Block, Kind, Selection};
use crate::text::RichText;

const COALESCE_MS: f64 = 1000.0;
const MAX_UNDO: usize = 1000;

/// What an edit is, for deciding whether it joins the previous undo group.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Edit {
    Typing,
    Space,
    Deleting,
    Other,
}

/// Net changes are only looked for in groups that reach across at most
/// this many blocks; a wider group is taken to have changed something.
const MAX_COMPARED: usize = 64;

struct Splice {
    at: usize,
    removed: Vec<Block>,
    inserted: usize,
}

pub(super) struct Group {
    splices: Vec<Splice>,
    /// Selection before the group, restored when it is reverted.
    before: Selection,
    /// Selection after the group, restored when it is done again.
    after: Selection,
    /// Blocks whose pre-group state is already recorded (or that the group
    /// created), so further edits to them need no new record.
    covered: HashSet<u32>,
    /// How many blocks at the start and at the end of the document the
    /// group has left alone.
    head: usize,
    tail: usize,
}

impl Group {
    pub(super) fn new(sel: Selection) -> Self {
        let (splices, covered) = (Vec::new(), HashSet::new());
        Group { splices, before: sel, after: sel, covered, head: usize::MAX, tail: usize::MAX }
    }

    /// Records a splice that has just been made in a document that now has
    /// `len` blocks.
    fn record(&mut self, splice: Splice, len: usize) {
        self.head = self.head.min(splice.at);
        self.tail = self.tail.min(len - splice.at - splice.inserted);
        self.splices.push(splice);
    }

    /// Whether the document is other than it was before the group.
    fn changed(&self, blocks: &[Block]) -> bool {
        let (removed, inserted) =
            self.splices.iter().fold((0, 0), |(r, i), s| (r + s.removed.len(), i + s.inserted));
        let now = &blocks[self.head..blocks.len() - self.tail];
        if removed != inserted || now.len() > MAX_COMPARED {
            return true;
        }
        let mut was = now.to_vec();
        for s in self.splices.iter().rev() {
            let at = s.at - self.head;
            was.splice(at..at + s.inserted, s.removed.iter().cloned());
        }
        was.iter().zip(now).any(|(a, b)| !a.same_content(b))
    }
}

impl Editor {
    pub fn undo(&mut self) -> bool {
        let Some(group) = self.undo.pop() else { return false };
        let inverse = self.revert(group);
        self.redo.push(inverse);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(group) = self.redo.pop() else { return false };
        let inverse = self.revert(group);
        self.undo.push(inverse);
        true
    }

    /// Undoes a group and returns the group that undoes the undo.
    fn revert(&mut self, group: Group) -> Group {
        let mut inverse = Group::new(group.after);
        inverse.after = group.before;
        for s in group.splices.into_iter().rev() {
            let inserted = s.removed.len();
            let removed = self.blocks.splice(s.at..s.at + s.inserted, s.removed).collect();
            inverse.splices.push(Splice { at: s.at, removed, inserted });
        }
        self.clock += 1;
        self.last = None;
        self.pending = None;
        self.sel = self.clamp(group.before);
        inverse
    }

    // ---- primitives ----

    pub(super) fn begin(&mut self, kind: Edit) {
        let joins = match self.last {
            Some((prev, time, caret)) => {
                !self.undo.is_empty()
                    && self.now - time < COALESCE_MS
                    && self.sel == Selection::caret(caret)
                    && matches!(
                        (prev, kind),
                        (Edit::Typing, Edit::Typing)
                            | (Edit::Space, Edit::Space)
                            | (Edit::Space, Edit::Typing)
                            | (Edit::Deleting, Edit::Deleting)
                    )
            }
            None => false,
        };
        if !joins {
            self.undo.push(Group::new(self.sel));
        }
        self.last = Some((kind, self.now, self.sel.focus));
    }

    pub(super) fn end(&mut self) {
        if self.blocks.is_empty() {
            let nb = self.new_block(Kind::Paragraph);
            self.splice(0..0, vec![nb]);
        }
        self.sel = self.clamp(self.sel);
        let group = self.undo.last_mut().expect("edit outside an undo group");
        if group.splices.is_empty() || !group.changed(&self.blocks) {
            // Nothing to undo: the step is not kept, and what was undone
            // before it can still be redone.
            self.undo.pop();
            self.last = None;
            return;
        }
        group.after = self.sel;
        self.redo.clear();
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        if let Some(last) = &mut self.last {
            last.2 = self.sel.focus;
        }
    }

    /// Mutable access to a block, recording its previous state for undo.
    pub(super) fn touch(&mut self, i: usize) -> &mut Block {
        let group = self.undo.last_mut().expect("edit outside an undo group");
        if group.covered.insert(self.blocks[i].id) {
            let removed = vec![self.blocks[i].clone()];
            group.record(Splice { at: i, removed, inserted: 1 }, self.blocks.len());
        }
        self.clock += 1;
        let b = &mut self.blocks[i];
        b.rev = self.clock;
        b
    }

    pub(super) fn splice(&mut self, range: Range<usize>, new: Vec<Block>) {
        if range.is_empty() && new.is_empty() {
            return;
        }
        let group = self.undo.last_mut().expect("edit outside an undo group");
        group.covered.extend(new.iter().map(|b| b.id));
        let (at, inserted) = (range.start, new.len());
        let removed = self.blocks.splice(range, new).collect();
        group.record(Splice { at, removed, inserted }, self.blocks.len());
        self.clock += 1;
    }

    pub(super) fn new_block(&mut self, kind: Kind) -> Block {
        self.next_id += 1;
        self.clock += 1;
        Block { id: self.next_id, rev: self.clock, ..Block::new(kind, RichText::new()) }
    }
}
