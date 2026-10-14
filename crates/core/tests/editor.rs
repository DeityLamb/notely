use notely_core::{mark, markdown, store, Editor, Kind, Page, Pos, Selection, COLORS};

fn typed(e: &mut Editor, s: &str) {
    for c in s.chars() {
        match c {
            '\n' => e.insert_paragraph(),
            _ => e.insert_text(c.encode_utf8(&mut [0; 4])),
        }
    }
}

fn editor(s: &str) -> Editor {
    let mut e = Editor::default();
    typed(&mut e, s);
    e
}

fn texts(e: &Editor) -> Vec<&str> {
    e.blocks().iter().map(|b| b.text.as_str()).collect()
}

fn kinds(e: &Editor) -> Vec<Kind> {
    e.blocks().iter().map(|b| b.kind).collect()
}

fn select(e: &mut Editor, a: (usize, usize), b: (usize, usize)) {
    e.set_selection(Selection { anchor: Pos::new(a.0, a.1), focus: Pos::new(b.0, b.1) });
}

#[test]
fn block_shortcuts() {
    let e = editor("# Title\n## Sub\n- item\nnext\n\n1. one\n\n[] todo\n\n> quote");
    assert_eq!(
        kinds(&e),
        [Kind::H1, Kind::H2, Kind::Bullet, Kind::Bullet, Kind::Numbered, Kind::Todo, Kind::Quote]
    );
    assert_eq!(texts(&e), ["Title", "Sub", "item", "next", "one", "todo", "quote"]);
}

#[test]
fn checked_todo_and_code_fence() {
    let e = editor("[x] done\n\n```\nlet a = 1;\nlet b = 2;");
    assert_eq!(kinds(&e), [Kind::Todo, Kind::Code]);
    assert!(e.blocks()[0].checked);
    assert_eq!(e.blocks()[1].text.as_str(), "let a = 1;\nlet b = 2;");
}

#[test]
fn divider_shortcut_leaves_caret_in_text() {
    let mut e = editor("above\n---");
    assert_eq!(kinds(&e), [Kind::Paragraph, Kind::Divider, Kind::Paragraph]);
    assert_eq!(e.selection().focus, Pos::new(2, 0));
    e.delete_backward();
    assert_eq!(kinds(&e), [Kind::Paragraph, Kind::Paragraph]);
}

#[test]
fn inline_shortcuts() {
    let e = editor("a **bold** b *it* c `code` d ~~gone~~ e _it_ f");
    let text = &e.blocks()[0].text;
    assert_eq!(text.as_str(), "a bold b it c code d gone e it f");
    assert!(text.has_mark(2..6, mark::BOLD));
    assert!(text.has_mark(9..11, mark::ITALIC));
    assert!(text.has_mark(14..18, mark::CODE));
    assert!(text.has_mark(21..25, mark::STRIKE));
    assert!(text.has_mark(28..30, mark::ITALIC));
    // Formatting does not leak into what is typed after it.
    assert!(!text.has_mark(6..7, mark::BOLD));
    assert!(!text.has_mark(30..32, mark::ITALIC));
}

#[test]
fn inline_shortcuts_leave_ordinary_text_alone() {
    let e = editor("snake_case_name and 2 * 3 * 4");
    assert_eq!(e.blocks()[0].text.as_str(), "snake_case_name and 2 * 3 * 4");
    assert_eq!(e.blocks()[0].text.run_count(), 1);
}

#[test]
fn link_shortcut() {
    let e = editor("see [docs](https://example.com) now");
    let text = &e.blocks()[0].text;
    assert_eq!(text.as_str(), "see docs now");
    assert_eq!(text.link_at(4).as_deref(), Some("https://example.com"));
    assert_eq!(text.link_at(9), None);
}

#[test]
fn link_at_caret_spans_mixed_formatting() {
    let mut e = Editor::from_blocks(markdown::parse("a [one **two**](x.com)[b](y.com) c"));
    for offset in [2, 5, 9] {
        select(&mut e, (0, offset), (0, offset));
        let (span, url) = e.link_at_caret().expect("in link");
        assert_eq!((span.start(), span.end(), &*url), (Pos::new(0, 2), Pos::new(0, 9), "x.com"));
    }
    select(&mut e, (0, 10), (0, 10));
    assert_eq!(&*e.link_at_caret().unwrap().1, "y.com");
    for offset in [1, 11] {
        select(&mut e, (0, offset), (0, offset));
        assert!(e.link_at_caret().is_none());
    }
    // Replacing the address of the whole link, then removing it.
    select(&mut e, (0, 4), (0, 4));
    let (span, _) = e.link_at_caret().unwrap();
    e.set_selection(span);
    e.set_link(Some("z.com"));
    assert_eq!(e.blocks()[0].text.link_at(8).as_deref(), Some("z.com"));
    e.set_link(None);
    assert_eq!(e.blocks()[0].text.link_at(8), None);
    assert!(e.blocks()[0].text.has_mark(6..9, mark::BOLD));
}

#[test]
fn undo_restores_literal_shortcut() {
    let mut e = editor("# ");
    assert_eq!(kinds(&e), [Kind::H1]);
    e.undo();
    assert_eq!(kinds(&e), [Kind::Paragraph]);
    assert_eq!(texts(&e), ["# "]);
    e.redo();
    assert_eq!(kinds(&e), [Kind::H1]);
    assert_eq!(texts(&e), [""]);
}

#[test]
fn undo_groups_typing_by_word() {
    let mut e = editor("hello world");
    e.undo();
    assert_eq!(texts(&e), ["hello"]);
    e.undo();
    assert_eq!(texts(&e), [""]);
    assert!(!e.undo());
    e.redo();
    e.redo();
    assert_eq!(texts(&e), ["hello world"]);
    assert_eq!(e.selection().focus, Pos::new(0, 11));
}

#[test]
fn undo_and_redo_structure() {
    let mut e = editor("one\ntwo\nthree");
    select(&mut e, (0, 1), (2, 2));
    e.delete_selection();
    assert_eq!(texts(&e), ["oree"]);
    e.undo();
    assert_eq!(texts(&e), ["one", "two", "three"]);
    assert_eq!(e.selection().start(), Pos::new(0, 1));
    e.redo();
    assert_eq!(texts(&e), ["oree"]);
    for _ in 0..20 {
        e.undo();
    }
    assert_eq!(texts(&e), [""]);
    for _ in 0..20 {
        e.redo();
    }
    assert_eq!(texts(&e), ["oree"]);
}

#[test]
fn enter_splits_and_continues_lists() {
    let mut e = editor("- one");
    select(&mut e, (0, 1), (0, 1));
    e.insert_paragraph();
    assert_eq!(texts(&e), ["o", "ne"]);
    assert_eq!(kinds(&e), [Kind::Bullet, Kind::Bullet]);
    // Enter on an empty item leaves the list.
    select(&mut e, (1, 2), (1, 2));
    e.insert_paragraph();
    e.insert_paragraph();
    assert_eq!(kinds(&e), [Kind::Bullet, Kind::Bullet, Kind::Paragraph]);
}

#[test]
fn enter_after_heading_starts_a_paragraph() {
    let mut e = editor("# Title\nbody");
    assert_eq!(kinds(&e), [Kind::H1, Kind::Paragraph]);
    select(&mut e, (0, 0), (0, 0));
    e.insert_paragraph();
    assert_eq!(texts(&e), ["", "Title", "body"]);
    assert_eq!(kinds(&e), [Kind::Paragraph, Kind::H1, Kind::Paragraph]);
    assert_eq!(e.selection().focus, Pos::new(1, 0));
}

#[test]
fn backspace_unwraps_outdents_then_joins() {
    let mut e = editor("first\n- item");
    e.tab(false);
    assert_eq!(e.blocks()[1].indent, 1);
    select(&mut e, (1, 0), (1, 0));
    e.delete_backward();
    assert_eq!(kinds(&e), [Kind::Paragraph, Kind::Paragraph]);
    e.delete_backward();
    assert_eq!(e.blocks()[1].indent, 0);
    e.delete_backward();
    assert_eq!(texts(&e), ["firstitem"]);
    assert_eq!(e.selection().focus, Pos::new(0, 5));
}

#[test]
fn delete_forward_joins_and_handles_multibyte() {
    let mut e = editor("héé\nb");
    select(&mut e, (0, 1), (0, 1));
    e.delete_forward();
    assert_eq!(texts(&e), ["hé", "b"]);
    e.delete_forward();
    e.delete_forward();
    assert_eq!(texts(&e), ["hb"]);
    e.delete_backward();
    assert_eq!(texts(&e), ["b"]);
}

#[test]
fn indent_needs_a_parent() {
    let mut e = editor("- a\nb");
    select(&mut e, (0, 0), (0, 0));
    e.tab(false);
    assert_eq!(e.blocks()[0].indent, 0);
    select(&mut e, (1, 0), (1, 0));
    e.tab(false);
    e.tab(false);
    assert_eq!(e.blocks()[1].indent, 1);
    e.tab(true);
    assert_eq!(e.blocks()[1].indent, 0);
}

#[test]
fn toggle_mark_on_selection_and_caret() {
    let mut e = editor("hello world");
    select(&mut e, (0, 0), (0, 5));
    e.toggle_mark(mark::BOLD);
    assert!(e.blocks()[0].text.has_mark(0..5, mark::BOLD));
    assert_eq!(e.active_marks(), mark::BOLD);
    e.toggle_mark(mark::BOLD);
    assert_eq!(e.blocks()[0].text.run_count(), 1);

    select(&mut e, (0, 11), (0, 11));
    e.toggle_mark(mark::ITALIC);
    typed(&mut e, "!!");
    assert!(e.blocks()[0].text.has_mark(11..13, mark::ITALIC));
    assert!(!e.blocks()[0].text.has_mark(10..11, mark::ITALIC));
}

fn color(name: &str) -> u8 {
    COLORS.iter().position(|c| *c == name).unwrap() as u8
}

#[test]
fn colors_on_selection_and_caret() {
    let mut e = editor("hello world");
    let (red, blue) = (color("red"), color("blue"));
    select(&mut e, (0, 0), (0, 5));
    e.set_color(red);
    e.set_background(blue);
    assert_eq!(e.active_colors(), (Some(red), Some(blue)));
    select(&mut e, (0, 3), (0, 8));
    assert_eq!(e.active_colors(), (None, None), "the selection has more than one colour");
    e.set_background(0);
    assert_eq!(e.active_colors(), (None, Some(0)));
    let runs: Vec<_> = e.blocks()[0].text.runs().map(|(s, a)| (s, a.color, a.bg)).collect();
    assert_eq!(runs, [("hel", red, blue), ("lo", red, 0), (" world", 0, 0)]);
    e.set_color(COLORS.len() as u8);
    assert_eq!(e.active_colors().0, None, "an unknown colour is not applied");

    // Typed text carries on the colour before it, or the one set at the caret.
    select(&mut e, (0, 5), (0, 5));
    typed(&mut e, "!");
    assert_eq!(e.blocks()[0].text.attrs_at(6).color, red);
    select(&mut e, (0, 12), (0, 12));
    e.set_color(color("green"));
    assert_eq!(e.active_colors(), (Some(color("green")), Some(0)));
    typed(&mut e, "ok");
    assert_eq!(e.blocks()[0].text.attrs_at(14).color, color("green"));
    assert_eq!(e.blocks()[0].text.attrs_at(12).color, 0);
}

#[test]
fn move_blocks_keeps_selection_on_them() {
    let mut e = editor("a\nb\nc");
    select(&mut e, (2, 1), (2, 1));
    e.move_blocks(true);
    assert_eq!(texts(&e), ["a", "c", "b"]);
    assert_eq!(e.selection().focus, Pos::new(1, 1));
    e.move_blocks(false);
    assert_eq!(texts(&e), ["a", "b", "c"]);
    e.undo();
    e.undo();
    assert_eq!(texts(&e), ["a", "b", "c"]);
}

#[test]
fn move_range_takes_nested_blocks_and_the_selection_along() {
    let mut e = Editor::from_blocks(markdown::parse("a\n\n- b\n  - c\n\nd"));
    select(&mut e, (2, 1), (2, 1));
    assert_eq!(e.subtree(1), 1..3);
    e.move_range(1..3, 4);
    assert_eq!(texts(&e), ["a", "d", "b", "c"]);
    assert_eq!(e.selection().focus, Pos::new(3, 1));
    e.move_range(2..4, 0);
    assert_eq!(texts(&e), ["b", "c", "a", "d"]);
    assert_eq!(e.selection().focus, Pos::new(1, 1));
    // Dropping a block where it already is changes nothing.
    e.move_range(0..2, 2);
    e.move_range(0..2, 0);
    assert_eq!(texts(&e), ["b", "c", "a", "d"]);
    e.undo();
    e.undo();
    assert_eq!(texts(&e), ["a", "b", "c", "d"]);
}

#[test]
fn a_moved_block_is_outdented_to_fit_its_new_place() {
    let mut e = Editor::from_blocks(markdown::parse("a\n\n- b\n  - c"));
    e.move_range(2..3, 0);
    assert_eq!(texts(&e), ["c", "a", "b"]);
    assert_eq!(e.blocks()[0].indent, 0);
}

#[test]
fn add_below_opens_a_paragraph_after_the_nested_blocks() {
    let mut e = Editor::from_blocks(markdown::parse("- a\n  - b\n\nc"));
    e.add_below(0);
    assert_eq!(texts(&e), ["a", "b", "", "c"]);
    assert_eq!(kinds(&e)[2], Kind::Paragraph);
    assert_eq!(e.selection().focus, Pos::new(2, 0));
    // An empty paragraph is the new block already.
    e.add_below(2);
    assert_eq!(texts(&e).len(), 4);
    e.undo();
    assert_eq!(texts(&e), ["a", "b", "c"]);
}

#[test]
fn delete_blocks_removes_them_and_never_leaves_the_document_empty() {
    let mut e = editor("a\nb\nc");
    e.delete_blocks(1..3);
    assert_eq!(texts(&e), ["a"]);
    assert_eq!(e.selection().focus, Pos::new(0, 0));
    e.delete_blocks(0..1);
    assert_eq!(texts(&e), [""]);
    e.undo();
    e.undo();
    assert_eq!(texts(&e), ["a", "b", "c"]);
}

#[test]
fn code_blocks_keep_their_language_and_wrapping() {
    let mut e = editor("```py\nx = 1");
    assert_eq!(kinds(&e), [Kind::Code]);
    assert_eq!(markdown::serialize(e.blocks()), "```python\nx = 1\n```");
    e.toggle_wrap(0);
    e.set_lang(0, notely_core::highlight::language("rust"));
    let (_, loaded) = store::load(&store::save(e.page(), e.blocks())).unwrap();
    assert!(loaded[0].nowrap && loaded[0].lang == e.blocks()[0].lang);
    assert_eq!(markdown::parse("```rust\nx\n```")[0].lang, loaded[0].lang);
    e.undo();
    e.undo();
    assert!(!e.blocks()[0].nowrap && markdown::serialize(e.blocks()).starts_with("```python"));
    // Other kinds of block carry neither.
    e.set_kind(Kind::Paragraph);
    assert_eq!(e.blocks()[0].lang, 0);
}

#[test]
fn slash_command_replaces_query() {
    let mut e = editor("/he");
    e.run_slash(Pos::new(0, 0), Kind::H2);
    assert_eq!(kinds(&e), [Kind::H2]);
    assert_eq!(texts(&e), [""]);
    e.undo();
    assert_eq!(texts(&e), ["/he"]);
}

#[test]
fn paste_blocks_into_the_middle_of_text() {
    let mut e = editor("headtail");
    select(&mut e, (0, 4), (0, 4));
    e.paste("# Title\n\n- a\n  - b\n");
    assert_eq!(texts(&e), ["head", "Title", "a", "b", "tail"]);
    assert_eq!(e.blocks()[3].indent, 1);
    assert_eq!(e.selection().focus, Pos::new(3, 1));
    e.undo();
    assert_eq!(texts(&e), ["headtail"]);
}

#[test]
fn paste_inline_keeps_formatting() {
    let mut e = editor("ab");
    select(&mut e, (0, 1), (0, 1));
    e.paste("**x**");
    assert_eq!(texts(&e), ["axb"]);
    assert!(e.blocks()[0].text.has_mark(1..2, mark::BOLD));
}

#[test]
fn composition_sync_replaces_a_range() {
    let mut e = editor("ab");
    e.replace_text(0, 1..1, "日本");
    assert_eq!(texts(&e), ["a日本b"]);
    assert_eq!(e.selection().focus, Pos::new(0, 7));
}

const SAMPLE: &str = "# Title

Some **bold *both* text**, `code`, ~~gone~~, <u>under</u> and [a link](https://example.com).

- one
  - nested
- [ ] open
- [x] done
1. first
2. second

> quoted\\
second line

---

```
fn main() {}

// **raw**
```

\\# not a heading

1\\. not a list

snake\\_case and 2 \\* 3";

#[test]
fn markdown_round_trip() {
    let blocks = markdown::parse(SAMPLE);
    let kinds: Vec<Kind> = blocks.iter().map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        [
            Kind::H1,
            Kind::Paragraph,
            Kind::Bullet,
            Kind::Bullet,
            Kind::Todo,
            Kind::Todo,
            Kind::Numbered,
            Kind::Numbered,
            Kind::Quote,
            Kind::Divider,
            Kind::Code,
            Kind::Paragraph,
            Kind::Paragraph,
            Kind::Paragraph,
        ]
    );
    let para = &blocks[1].text;
    assert_eq!(para.as_str(), "Some bold both text, code, gone, under and a link.");
    assert!(para.has_mark(5..19, mark::BOLD));
    assert!(para.has_mark(10..14, mark::ITALIC));
    assert!(!para.has_mark(5..10, mark::ITALIC));
    assert!(para.has_mark(33..38, mark::UNDERLINE));
    assert_eq!(para.link_at(43).as_deref(), Some("https://example.com"));
    assert_eq!(blocks[8].text.as_str(), "quoted\nsecond line");
    assert_eq!(blocks[10].text.as_str(), "fn main() {}\n\n// **raw**");
    assert_eq!(blocks[11].text.as_str(), "# not a heading");
    assert_eq!(blocks[13].text.as_str(), "snake_case and 2 * 3");
    assert_eq!(markdown::serialize(&blocks), SAMPLE);
}

#[test]
fn markdown_handles_non_ascii_text() {
    let src = "# Café ☕\n\n- naïve **жирный** *斜体* `код` and [日本語](https://example.com/é) 🎉";
    let blocks = markdown::parse(src);
    assert_eq!(blocks[0].text.as_str(), "Café ☕");
    let text = &blocks[1].text;
    assert_eq!(text.as_str(), "naïve жирный 斜体 код and 日本語 🎉");
    assert!(text.has_mark(7..19, mark::BOLD));
    assert_eq!(markdown::serialize(&blocks), src);

    let mut e = Editor::default();
    e.paste("héllo wörld");
    typed(&mut e, " **gras é** fin");
    assert_eq!(texts(&e), ["héllo wörld gras é fin"]);
}

#[test]
fn store_round_trip() {
    let mut blocks = markdown::parse(SAMPLE);
    blocks[8].text.push("\ttab \\ slash", &Default::default());
    let page = Page { title: "A\ttitle \\ with 🎉".into(), icon: "📝".into(), cover: "ocean".into() };
    let saved = store::save(&page, &blocks);
    let (loaded_page, loaded) = store::load(&saved).expect("loads");
    assert_eq!(loaded_page, page);
    assert_eq!(loaded.len(), blocks.len());
    for (a, b) in blocks.iter().zip(&loaded) {
        assert_eq!((a.kind, a.checked, a.indent), (b.kind, b.checked, b.indent));
        assert_eq!(a.text, b.text);
    }
    assert!(store::load("garbage").is_none());
}

#[test]
fn colors_are_saved_and_copied() {
    let mut e = editor("plain red both");
    select(&mut e, (0, 6), (0, 14));
    e.set_color(color("red"));
    select(&mut e, (0, 10), (0, 14));
    e.set_background(color("yellow"));
    e.toggle_mark(mark::BOLD);
    let (_, loaded) = store::load(&store::save(e.page(), e.blocks())).unwrap();
    assert_eq!(loaded[0].text, e.blocks()[0].text);

    let md = markdown::serialize(e.blocks());
    assert_eq!(
        md,
        "plain <span style=\"color:red\">red </span><span style=\"color:red;background:yellow\">**both**</span>"
    );
    assert_eq!(markdown::parse(&md)[0].text, e.blocks()[0].text);
    let link = markdown::parse("[a <span style=\"background:blue\">b</span>](x.com)");
    assert_eq!(markdown::serialize(&link), "[a <span style=\"background:blue\">b</span>](x.com)");
    let other = "<span style=\"color:teal\">x</span> <span>y</span> <span style=\"color:red\">z";
    assert_eq!(markdown::parse(other)[0].text.as_str(), other, "other spans are left as they are");
}

#[test]
fn store_loads_documents_saved_before_colors() {
    let (_, blocks) = store::load("notely 2\n\t\t\np00\thi there\t3:1 5:0").expect("loads");
    assert!(blocks[0].text.has_mark(0..3, mark::BOLD));
    assert!(blocks[0].text.runs().all(|(_, a)| a.color == 0 && a.bg == 0));
    let (_, blocks) = store::load("notely 2\n\t\t\np00\thi\t2:0.99.6").expect("loads");
    let attrs = blocks[0].text.attrs_at(1);
    assert_eq!((attrs.color, attrs.bg), (0, 6), "a colour this version does not know is left out");
}

#[test]
fn store_loads_documents_saved_before_pages() {
    let (page, blocks) = store::load("notely 1\n100\thello\t").expect("loads");
    assert_eq!(page, Page::default());
    assert_eq!(texts(&Editor::from_blocks(blocks)), ["hello"]);
}

#[test]
fn page_changes_are_saved_but_not_undone() {
    let mut e = editor("text");
    typed(&mut e, "!");
    let version = e.version();
    e.set_page(Page { title: "Notes".into(), ..Default::default() });
    assert_ne!(e.version(), version, "a new title must be saved");
    let version = e.version();
    e.set_page(e.page().clone());
    assert_eq!(e.version(), version, "the same page is no change");
    e.undo();
    assert_eq!(e.page().title, "Notes");
}

#[test]
fn copy_selection_as_markdown() {
    let mut e = Editor::from_blocks(markdown::parse("# Title\n\nsome **bold** text"));
    select(&mut e, (1, 5), (1, 9));
    assert_eq!(e.selection_markdown(), "**bold**");
    select(&mut e, (0, 2), (1, 4));
    assert_eq!(e.selection_markdown(), "# tle\n\nsome");
}

/// Runs a command that must change the document, then checks that one undo
/// gives back the document and selection from before it, and one redo the
/// ones from after it.
fn undoable(e: &mut Editor, what: &str, command: impl FnOnce(&mut Editor)) {
    // Long enough since the last edit that this one is a step of its own.
    e.set_time(e.version() as f64 * 10_000.0);
    let before = (store::save(e.page(), e.blocks()), e.selection());
    command(e);
    let after = (store::save(e.page(), e.blocks()), e.selection());
    assert_ne!(before.0, after.0, "{what} changed nothing");
    assert!(e.undo(), "{what}: nothing to undo");
    assert_eq!((store::save(e.page(), e.blocks()), e.selection()), before, "undo of {what}");
    assert!(e.redo(), "{what}: nothing to redo");
    assert_eq!((store::save(e.page(), e.blocks()), e.selection()), after, "redo of {what}");
}

#[test]
fn every_command_is_one_undo_step() {
    let mut e = Editor::from_blocks(markdown::parse("# Title\n\none two\n\n- a\n  - b\n- [ ] todo\n\n```\nlet x;\n```\n\nlast"));
    let caret = |e: &mut Editor, block, offset| select(e, (block, offset), (block, offset));

    // Typing, Enter and deleting.
    caret(&mut e, 1, 3);
    undoable(&mut e, "typing", |e| e.insert_text("!"));
    undoable(&mut e, "a line break", |e| e.insert_line_break());
    undoable(&mut e, "Enter", |e| e.insert_paragraph());
    undoable(&mut e, "Backspace joining blocks", |e| e.delete_backward());
    undoable(&mut e, "Backspace", |e| e.delete_backward());
    undoable(&mut e, "Delete", |e| e.delete_forward());
    undoable(&mut e, "deleting a word", |e| e.delete_span(Pos::new(1, 0), Pos::new(1, 3)));
    undoable(&mut e, "a composed character", |e| e.replace_text(1, 0..1, "é"));
    select(&mut e, (0, 2), (2, 1));
    undoable(&mut e, "typing over blocks", |e| e.insert_text("x"));
    e.undo();

    // Formatting.
    select(&mut e, (0, 1), (1, 2));
    undoable(&mut e, "bold", |e| e.toggle_mark(mark::BOLD));
    undoable(&mut e, "a link", |e| e.set_link(Some("https://e.x")));
    undoable(&mut e, "removing a link", |e| e.set_link(None));
    undoable(&mut e, "a text colour", |e| e.set_color(3));
    undoable(&mut e, "a background", |e| e.set_background(4));
    undoable(&mut e, "removing a text colour", |e| e.set_color(0));
    undoable(&mut e, "a block type", |e| e.set_kind(Kind::Quote));
    undoable(&mut e, "a divider", |e| e.set_kind(Kind::Divider));
    e.undo();

    // The slash menu, to-dos and code blocks.
    caret(&mut e, 1, 0);
    e.insert_text("/h");
    undoable(&mut e, "a slash command", |e| e.run_slash(Pos::new(1, 0), Kind::H2));
    undoable(&mut e, "a slash divider", |e| e.run_slash(Pos::new(1, 0), Kind::Divider));
    e.undo();
    let todo = e.blocks().iter().position(|b| b.kind == Kind::Todo).unwrap();
    let code = e.blocks().iter().position(|b| b.kind == Kind::Code).unwrap();
    undoable(&mut e, "a checkbox", |e| e.toggle_todo(todo));
    caret(&mut e, todo, 0);
    undoable(&mut e, "Mod+Enter on a to-do", |e| e.mod_enter());
    undoable(&mut e, "a code language", |e| e.set_lang(code, 12));
    undoable(&mut e, "code wrapping", |e| e.toggle_wrap(code));
    caret(&mut e, code, 0);
    undoable(&mut e, "Tab in code", |e| e.tab(false));
    undoable(&mut e, "Enter in code", |e| e.insert_paragraph());
    undoable(&mut e, "Mod+Enter", |e| e.mod_enter());

    // Whole blocks.
    caret(&mut e, 4, 0);
    undoable(&mut e, "Tab", |e| e.tab(false));
    undoable(&mut e, "Shift+Tab", |e| e.tab(true));
    undoable(&mut e, "moving a block down", |e| e.move_blocks(false));
    undoable(&mut e, "moving a block up", |e| e.move_blocks(true));
    undoable(&mut e, "dragging blocks", |e| e.move_range(e.subtree(2), 0));
    undoable(&mut e, "a block added below", |e| e.add_below(0));
    undoable(&mut e, "deleting blocks", |e| e.delete_blocks(1..3));
    undoable(&mut e, "a paragraph at the end", |e| e.focus_end());

    // Paste.
    caret(&mut e, 1, 2);
    undoable(&mut e, "pasting text", |e| e.paste("in **bold**"));
    undoable(&mut e, "pasting blocks", |e| e.paste("# One\n\n- two\n- three"));
    select(&mut e, (0, 1), (3, 1));
    undoable(&mut e, "pasting over blocks", |e| e.paste("a\n\nb"));
    select(&mut e, (1, 0), (3, 1));
    undoable(&mut e, "cutting", |e| e.delete_selection());
}

#[test]
fn redo_puts_the_caret_back_where_the_change_was() {
    let mut e = editor("one\ntwo\nthree");
    select(&mut e, (0, 3), (0, 3));
    e.set_time(5000.0);
    e.insert_text("!");
    select(&mut e, (2, 1), (2, 1));
    e.undo();
    assert_eq!(e.selection(), Selection::caret(Pos::new(0, 3)));
    select(&mut e, (1, 0), (1, 0));
    e.redo();
    assert_eq!(texts(&e), ["one!", "two", "three"]);
    assert_eq!(e.selection(), Selection::caret(Pos::new(0, 4)));
}

#[test]
fn a_command_that_changes_nothing_is_not_an_undo_step() {
    let mut e = editor("one\ntwo");
    e.set_time(5000.0);
    select(&mut e, (0, 0), (0, 3));
    e.toggle_mark(mark::BOLD);
    e.undo();
    // None of these has anything to do here.
    e.set_link(None);
    e.tab(true);
    e.move_blocks(true);
    e.toggle_todo(0);
    e.delete_span(Pos::new(0, 1), Pos::new(0, 1));
    select(&mut e, (0, 0), (0, 0));
    e.delete_backward();
    // So the bold can still be redone, and undo skips straight to the typing.
    assert!(e.redo());
    assert!(e.blocks()[0].text.has_mark(0..3, mark::BOLD));
    e.undo();
    e.undo();
    assert_eq!(texts(&e), ["one", ""]);

    // Deleting the only, empty block leaves an empty block: no step either.
    let mut e = Editor::default();
    e.delete_blocks(0..1);
    assert!(!e.undo());
}

#[test]
fn undo_steps_survive_the_history_limit_being_reached_by_idle_keys() {
    let mut e = editor("one");
    e.set_time(5000.0);
    select(&mut e, (0, 0), (0, 0));
    // Backspace at the very start does nothing, however often it is pressed.
    for _ in 0..5000 {
        e.delete_backward();
    }
    assert!(e.undo());
    assert_eq!(texts(&e), [""]);
}
