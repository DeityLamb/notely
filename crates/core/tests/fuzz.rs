//! Random editing sessions. After every command the document must still be
//! well formed, and undoing the whole session must give back the start.

use notely_core::{mark, markdown, store, Block, Editor, Kind, Pos, Selection, COLORS};

/// Small deterministic generator, so a failure can be replayed from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.next(items.len())]
    }
}

const KINDS: [Kind; 10] = [
    Kind::Paragraph,
    Kind::H1,
    Kind::H2,
    Kind::H3,
    Kind::Bullet,
    Kind::Numbered,
    Kind::Todo,
    Kind::Quote,
    Kind::Code,
    Kind::Divider,
];

const TEXT: [&str; 24] = [
    "a", "b", " ", " ", "é", "мир", "日本", "👍", "#", "-", "1.", "[]", ">", "*", "**", "`", "~~", "_",
    "[x](y)", "---", "```", "\\", "/", "word ",
];

const PASTE: [&str; 8] = [
    "plain",
    "# Title\n\ntext **bold** and `code`",
    "- one\n  - two\n- [x] done",
    "```rust\nfn x() {}\n```",
    "a\\\nb",
    "> quote *it* [l](https://e.x)\n\n---\n",
    "мир **жир** 日本",
    "",
];

fn random_pos(rng: &mut Rng, blocks: &[Block]) -> Pos {
    let block = rng.next(blocks.len());
    let text = blocks[block].text.as_str();
    let mut offset = rng.next(text.len() + 1);
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    Pos::new(block, offset)
}

fn check(e: &Editor, seed: u64, step: usize, op: usize) {
    let at = format!("seed {seed} step {step} op {op}");
    let blocks = e.blocks();
    assert!(!blocks.is_empty(), "no blocks, {at}");
    for p in [e.selection().anchor, e.selection().focus] {
        let text = blocks.get(p.block).unwrap_or_else(|| panic!("selection past the end, {at}")).text.as_str();
        assert!(text.is_char_boundary(p.offset), "selection inside a character, {at}");
    }
    for b in blocks {
        let mut end = 0;
        for run in b.text.raw_runs() {
            assert!(run.len > 0, "empty run, {at}");
            end += run.len;
            assert!(b.text.as_str().is_char_boundary(end), "run splits a character, {at}");
        }
        assert_eq!(end, b.text.len(), "runs do not cover the text, {at}");
    }
    // Saving must be lossless.
    let (_, loaded) =
        store::load(&store::save(&Default::default(), blocks)).unwrap_or_else(|| panic!("saved document does not load, {at}"));
    assert_eq!(loaded.len(), blocks.len(), "{at}");
    for (a, b) in loaded.iter().zip(blocks) {
        assert!(a.kind == b.kind && a.checked == b.checked && a.indent == b.indent && a.text == b.text, "{at}");
        assert!(a.lang == b.lang && a.nowrap == b.nowrap, "{at}");
    }
    // Markdown out and back in must at least not fail.
    markdown::parse(&markdown::serialize(blocks));
    // On some steps only: it is slow next to the rest.
    for b in blocks.iter().filter(|_| step % 8 == 0) {
        let pieces = notely_core::highlight::highlight(b.text.as_str(), b.lang);
        let mut end = 0;
        for (len, _) in pieces {
            end += len;
            assert!(b.text.as_str().is_char_boundary(end), "highlighting splits a character, {at}");
        }
        assert_eq!(end, b.text.len(), "highlighting does not cover the text, {at}");
    }
    let _ = (e.selection_markdown(), e.active_marks(), e.active_colors(), e.link(), e.link_at_caret(), e.has_rich_selection());
}

/// A command that changed the document is undone by one undo, which also
/// puts the selection back, and redone by one redo. A command that changed
/// nothing leaves no step behind.
fn check_undo(e: &mut Editor, before: (String, Selection), seed: u64, step: usize, op: usize) {
    let at = format!("seed {seed} step {step} op {op}");
    let after = (store::save(e.page(), e.blocks()), e.selection());
    if after.0 == before.0 {
        if e.undo() {
            assert_ne!(store::save(e.page(), e.blocks()), before.0, "a command that changed nothing can be undone, {at}");
            assert!(e.redo());
        }
        return;
    }
    assert!(e.undo(), "nothing to undo, {at}");
    assert_eq!(store::save(e.page(), e.blocks()), before.0, "undo did not restore the document, {at}");
    assert_eq!(e.selection(), before.1, "undo did not restore the selection, {at}");
    assert!(e.redo(), "nothing to redo, {at}");
    assert_eq!(store::save(e.page(), e.blocks()), after.0, "redo did not restore the document, {at}");
    assert_eq!(e.selection(), after.1, "redo did not restore the selection, {at}");
}

fn session(seed: u64, steps: usize) {
    let mut rng = Rng(seed);
    let mut e = Editor::from_blocks(markdown::parse(notely_core::WELCOME));
    let start = store::save(e.page(), e.blocks());
    let mut now = 0.0;
    for step in 0..steps {
        let gap = [5.0, 50.0, 2000.0][rng.next(3)];
        now += gap;
        e.set_time(now);
        let op = rng.next(37);
        let before = (store::save(e.page(), e.blocks()), e.selection());
        match op {
            0..=7 => e.insert_text(rng.pick(&TEXT)),
            8 => e.insert_paragraph(),
            9 => e.insert_line_break(),
            10 | 11 => e.delete_backward(),
            12 => e.delete_forward(),
            13 => e.delete_selection(),
            14 => e.toggle_mark([mark::BOLD, mark::ITALIC, mark::UNDERLINE, mark::STRIKE, mark::CODE][rng.next(5)]),
            15 => e.set_link([Some("https://e.x"), Some("other"), None][rng.next(3)]),
            16 => e.set_kind(KINDS[rng.next(KINDS.len())]),
            17 => e.tab(rng.next(2) == 0),
            18 => e.move_blocks(rng.next(2) == 0),
            19 => e.toggle_todo(rng.next(e.blocks().len())),
            20 => e.paste(rng.pick(&PASTE)),
            21 => drop(e.undo()),
            22 => drop(e.redo()),
            23 => e.mod_enter(),
            24 => {
                let (a, b) = (random_pos(&mut rng, e.blocks()), random_pos(&mut rng, e.blocks()));
                e.delete_span(a.min(b), a.max(b));
            }
            25 => {
                let at = random_pos(&mut rng, e.blocks());
                e.run_slash(at, KINDS[rng.next(KINDS.len())]);
            }
            26 => e.focus_end(),
            31 => e.set_lang(rng.next(e.blocks().len()), rng.next(20) as u8),
            32 => e.toggle_wrap(rng.next(e.blocks().len())),
            35 => e.set_color(rng.next(COLORS.len() + 1) as u8),
            36 => e.set_background(rng.next(COLORS.len() + 1) as u8),
            30 => {
                let n = e.blocks().len();
                let at = rng.next(n);
                e.delete_blocks(at..at + 1 + rng.next(3));
            }
            28 => e.add_below(rng.next(e.blocks().len())),
            29 => {
                let n = e.blocks().len();
                let from = e.subtree(rng.next(n));
                e.move_range(from, rng.next(n + 1));
            }
            27 => e.set_selection(Selection { anchor: Pos::new(rng.next(99), rng.next(999)), focus: Pos::new(rng.next(99), rng.next(999)) }),
            _ => {
                let focus = random_pos(&mut rng, e.blocks());
                let anchor = if rng.next(2) == 0 { focus } else { random_pos(&mut rng, e.blocks()) };
                e.set_selection(Selection { anchor, focus });
            }
        }
        check(&e, seed, step, op);
        // Typing can be two steps (the text, then what it turns into) or
        // part of one; every other command after a pause is one step.
        let one_step = gap >= 2000.0 && !matches!(op, 0..=7 | 9 | 17 | 20..=22 | 27 | 33 | 34);
        if one_step && rng.next(3) == 0 {
            check_undo(&mut e, before, seed, step, op);
        }
    }
    while e.undo() {}
    assert_eq!(store::save(e.page(), e.blocks()), start, "undoing everything did not restore the document, seed {seed}");
}

#[test]
fn random_sessions_keep_the_document_well_formed() {
    for seed in 0..300 {
        session(seed, 400);
    }
}
