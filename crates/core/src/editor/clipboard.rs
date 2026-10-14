//! Copy and paste, as Markdown.

use super::history::Edit;
use super::Editor;
use crate::doc::{Kind, Pos, Selection};
use crate::markdown;

impl Editor {
    pub fn selection_markdown(&self) -> String {
        let (a, b) = (self.sel.start(), self.sel.end());
        if a.block == b.block {
            let blk = &self.blocks[a.block];
            let text = blk.text.slice(a.offset..b.offset);
            return match blk.kind {
                Kind::Code | Kind::Math => text.as_str().to_string(),
                _ => markdown::inline_to_string(&text),
            };
        }
        let mut part = self.blocks[a.block..=b.block].to_vec();
        let last = part.len() - 1;
        part[last].text = part[last].text.slice(0..b.offset);
        part[0].text = part[0].text.slice(a.offset..part[0].text.len());
        markdown::serialize(&part)
    }

    /// Inserts Markdown at the selection: inline when it is a single line of
    /// text, as blocks otherwise.
    pub fn paste(&mut self, src: &str) {
        let (a, b) = (self.sel.start(), self.sel.end());
        if a.block == b.block && matches!(self.blocks[a.block].kind, Kind::Code | Kind::Math) {
            return self.insert_text(src);
        }
        let mut blocks = markdown::parse(src);
        if blocks.is_empty() {
            return;
        }
        self.begin(Edit::Other);
        let pos = self.delete_range(a, b);
        if blocks.len() == 1 && blocks[0].kind == Kind::Paragraph {
            let pos = self.text_target(pos);
            let text = &blocks[0].text;
            self.touch(pos.block).text.insert_rich(pos.offset, text);
            self.sel = Selection::caret(Pos::new(pos.block, pos.offset + text.len()));
            self.end();
            return;
        }
        for blk in &mut blocks {
            let fresh = self.new_block(blk.kind);
            blk.id = fresh.id;
            blk.rev = fresh.rev;
        }
        let n = blocks.len();
        let caret = blocks[n - 1].text.len();
        let i = pos.block;
        let (kind, len) = (self.blocks[i].kind, self.blocks[i].text.len());
        let at = if len == 0 && kind != Kind::Divider {
            self.splice(i..i + 1, blocks);
            i
        } else if pos.offset == 0 {
            self.splice(i..i, blocks);
            i
        } else {
            if pos.offset < len {
                let mut tail = self.new_block(Kind::Paragraph);
                tail.text = self.touch(i).text.split_off(pos.offset);
                blocks.push(tail);
            }
            self.splice(i + 1..i + 1, blocks);
            i + 1
        };
        self.sel = Selection::caret(Pos::new(at + n - 1, caret));
        self.end();
    }
}
