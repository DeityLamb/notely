//! The document widget: lays the blocks out, paints them, and maps keyboard
//! and pointer input to editor commands.
//!
//! It is one widget for the whole document rather than one text field per
//! block, so the caret and selection can cross blocks. Each block's text is
//! laid out by egui into a `Galley`, cached until the block's revision or the
//! column width changes.

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{
    self, pos2, vec2, Align2, Area, Color32, CursorIcon, Event, EventFilter, FontId, Galley, Id,
    ImeEvent, Key, KeyboardShortcut, LayerId, Modifiers, Order, Painter, Pos2, Rect, RectAlign, Response,
    Sense, Stroke, StrokeKind, Ui, Vec2,
};
use notely_core::highlight::LANGUAGES;
use notely_core::{mark, Attrs, Block, Editor, Kind, Pos, Selection};

use crate::colors::ColorPicker;
use crate::emoji::{self, picker::{Choice, EmojiPicker}};
use crate::math;
use crate::theme::{self, Palette, BODY, FADE, STEP};

const MAX_WIDTH: f32 = 700.0;
/// Space beside the text column; the left one holds the block handles.
const MARGIN: f32 = 52.0;
/// Widths of the add button and of the drag handle left of a block, their
/// height, and the gap between the handle and the block.
const ADD_WIDTH: f32 = 22.0;
const GRIP_WIDTH: f32 = 16.0;
const HANDLE_HEIGHT: f32 = 22.0;
const HANDLE_GAP: f32 = 6.0;
/// Width of the wrap and copy buttons of a code block, the height of all
/// its options, and their distance from the block's top right corner.
const OPTION_WIDTH: f32 = 26.0;
const OPTION_HEIGHT: f32 = 22.0;
const OPTION_INSET: f32 = 6.0;
/// Space above the page's title, or above its icon, without a cover.
const TOP: f32 = 56.0;
/// Height of a page's cover, the size of its icon and of the square the
/// icon sits in, and the height of the buttons above the title.
const COVER_HEIGHT: f32 = 200.0;
const ICON_SIZE: f32 = 64.0;
const ICON_BOX: f32 = 78.0;
const HEAD_BUTTON_HEIGHT: f32 = 28.0;
const CODE_PAD: Vec2 = vec2(18.0, 14.0);
const QUOTE_PAD: f32 = 16.0;
const CODE_RADIUS: f32 = 6.0;
const INLINE_CODE_RADIUS: f32 = 3.5;
/// Font size of formulas, as KaTeX sets them beside body text, and the
/// space above and below one.
const MATH_SIZE: f32 = BODY * 1.21;
const MATH_PAD: f32 = 10.0;
/// Tallest a formula is drawn; taller ones, and ones wider than the column,
/// are drawn smaller.
const MATH_MAX_HEIGHT: f32 = 800.0;
/// Height of the line below invalid TeX that says what is wrong with it.
const MATH_ERROR: f32 = 26.0;
/// Widest the math editor is.
const MATH_EDITOR_WIDTH: f32 = 560.0;
/// Where lines through and under text are drawn, in font sizes below the
/// middle of the line: through the middle of the small letters, and a
/// little under the baseline.
const STRIKE_DROP: f32 = 0.09;
const UNDERLINE_DROP: f32 = 0.47;
/// Seconds the caret is shown, then hidden, while nothing happens.
const BLINK: f64 = 0.53;

/// Seconds the pointer rests on a link before its address is shown, and
/// seconds the address stays once the pointer has left.
const HOVER_SHOW: f64 = 0.3;
const HOVER_HIDE: f64 = 0.25;
/// Seconds the copy button shows a tick after copying.
const COPIED: f64 = 1.2;

/// Slash menu entries: label, typed shortcut shown as a hint, extra lowercase
/// search words, and the block type they produce.
const ITEMS: [(&str, &str, &str, Kind); 11] = [
    ("Text", "", "paragraph plain", Kind::Paragraph),
    ("Heading 1", "#", "h1 title", Kind::H1),
    ("Heading 2", "##", "h2 subtitle", Kind::H2),
    ("Heading 3", "###", "h3", Kind::H3),
    ("Bulleted list", "-", "ul unordered", Kind::Bullet),
    ("Numbered list", "1.", "ol ordered", Kind::Numbered),
    ("To-do list", "[]", "todo task checkbox", Kind::Todo),
    ("Quote", ">", "blockquote", Kind::Quote),
    ("Code", "```", "snippet pre", Kind::Code),
    ("Block equation", "$$", "math equation formula latex tex katex", Kind::Math),
    ("Divider", "---", "hr rule line separator", Kind::Divider),
];

/// A button of the formatting toolbar or the link popup: its name and
/// shortcut, as its tooltip gives them, and its icon.
struct Tool {
    name: &'static str,
    keys: Option<KeyboardShortcut>,
    icon: fn(&Icon),
}

const fn tool(name: &'static str, modifiers: Modifiers, key: Key, icon: fn(&Icon)) -> Tool {
    Tool { name, keys: Some(KeyboardShortcut::new(modifiers, key)), icon }
}

/// A button with no shortcut.
const fn action(name: &'static str, icon: fn(&Icon)) -> Tool {
    Tool { name, keys: None, icon }
}

/// The formatting toolbar's buttons for marks, with the mark each toggles.
const MARK_TOOLS: [(u8, Tool); 5] = [
    (mark::BOLD, tool("Bold", Modifiers::COMMAND, Key::B, bold_icon)),
    (mark::ITALIC, tool("Italic", Modifiers::COMMAND, Key::I, italic_icon)),
    (mark::UNDERLINE, tool("Underline", Modifiers::COMMAND, Key::U, underline_icon)),
    (mark::STRIKE, tool("Strikethrough", Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S, strike_icon)),
    (mark::CODE, tool("Inline code", Modifiers::COMMAND, Key::E, code_icon)),
];
const LINK_TOOL: Tool = tool("Link", Modifiers::COMMAND, Key::K, link_icon);
/// The link popup's buttons.
const OPEN_TOOL: Tool = action("Open link", open_icon);
const COPY_TOOL: Tool = action("Copy link", sheets_icon);
const COPIED_TOOL: Tool = action("Copied", tick_icon);
const EDIT_TOOL: Tool = action("Edit link", pencil_icon);
const UNLINK_TOOL: Tool = action("Remove link", unlink_icon);
const APPLY_TOOL: Tool = tool("Apply", Modifiers::NONE, Key::Enter, tick_icon);
/// The math editor's button.
const DONE_TOOL: Tool = tool("Done", Modifiers::NONE, Key::Enter, tick_icon);
/// Side of a toolbar button.
pub const TOOL: f32 = 28.0;

struct Cached {
    rev: u32,
    /// Wrap width and theme the galley was laid out for.
    key: (u32, bool),
    galley: Arc<Galley>,
    seen: u64,
}

/// Where a block sits, in document coordinates (origin at the top left of
/// the text column).
struct Row {
    top: f32,
    height: f32,
    /// Top left of the block's lines.
    text: Vec2,
    /// How far below that the text is drawn, to sit in the middle of its
    /// lines: see `theme::lead`.
    lead: f32,
    galley: Arc<Galley>,
    /// Ordinal of a numbered list item.
    number: u32,
    /// A math block's formula: where it is drawn, in document coordinates,
    /// and at what size. None while the block is empty or its TeX invalid.
    formula: Option<(Rect, f32)>,
    /// Whether the block shows only its formula, its TeX hidden: a math
    /// block without the caret.
    typeset: bool,
}

struct Menu {
    /// Position of the `/` that opened the menu.
    at: Pos,
    /// Indices into `ITEMS` matching what was typed after it.
    items: Vec<usize>,
    active: usize,
}

/// The open language list of a code block.
struct LangMenu {
    block: usize,
    /// It opened on this frame.
    fresh: bool,
    /// What was typed into its search field.
    query: String,
    /// Which of the matching languages Enter would choose.
    active: usize,
}

/// Where the parts of the page above its blocks are, in document
/// coordinates.
#[derive(Default)]
struct Head {
    /// Bottom of the cover, which spans the window; zero without a cover.
    cover: f32,
    icon: Option<Rect>,
    /// Top of the row of "Add icon" and "Add cover", when either is missing.
    buttons: Option<f32>,
    /// Where the title's text field is: its top and height.
    title: (f32, f32),
    /// Where the first block starts.
    end: f32,
}

/// The buttons above the title, and those on the cover.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum HeadButton {
    AddIcon,
    AddCover,
    ChangeCover,
    RemoveCover,
}

impl HeadButton {
    fn label(self) -> &'static str {
        match self {
            HeadButton::AddIcon => "Add icon",
            HeadButton::AddCover => "Add cover",
            HeadButton::ChangeCover => "Change cover",
            HeadButton::RemoveCover => "Remove",
        }
    }

    fn on_cover(self) -> bool {
        matches!(self, HeadButton::ChangeCover | HeadButton::RemoveCover)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Picker {
    Icon,
    Cover,
}

/// The link popup.
#[derive(Default)]
enum LinkUi {
    #[default]
    Closed,
    /// The pointer is resting on a link.
    Hover { span: Selection, since: f64 },
    /// Shows a link's address with Copy and Edit.
    View { span: Selection, url: Rc<str>, away: Option<f64>, copied: Option<f64> },
    /// Edits the address of the text in `span`. It holds the keyboard.
    Edit { span: Selection, input: String, had_link: bool, fresh: bool },
}

/// The editor of a math block's TeX: a popup under the block, which holds
/// the keyboard while it is open. The block stays as it is, its formula
/// following the TeX as it is typed.
struct MathEdit {
    /// Id of the block.
    block: u32,
    input: String,
    /// The TeX the block shows: the input, or while that cannot be read,
    /// the last that could, so the formula does not come and go as it is
    /// typed.
    shown: String,
    /// It opened on this frame.
    fresh: bool,
    /// It closed, and is fading out.
    closed: bool,
}

#[derive(Default)]
pub struct DocView {
    cache: HashMap<u32, Cached>,
    frame: u64,
    /// Width of the text column.
    width: f32,
    rows: Vec<Row>,
    /// Height of all rows.
    total: f32,
    /// What `rows` was computed for: document version, width, theme, and
    /// the math block being edited.
    laid: Option<(u32, u32, bool, Option<u32>)>,
    /// Column Up/Down try to stay in, kept across consecutive presses.
    h_pos: Option<f32>,
    /// At a soft line wrap the same offset is both the end of one row and
    /// the start of the next; this says which one the caret is shown on.
    next_row: bool,
    menu: Option<Menu>,
    /// Scroll the caret into view after this frame's edits.
    reveal: bool,
    /// Text being composed by an input method.
    preedit: String,
    /// Selection and document version the caret was last drawn for, and
    /// when they changed: blinking restarts from there.
    caret_state: Option<(Selection, u32)>,
    caret_since: f64,
    fling: crate::scroll::Fling,
    link: LinkUi,
    /// Whether the pointer is on the link `link` is showing.
    on_link: bool,
    /// Document version `link` was opened for.
    link_version: u32,
    /// Block whose add button and drag handle are shown.
    handle: Option<usize>,
    /// Block the handles were last drawn for and the document version then,
    /// so they can fade out where they were.
    handle_drawn: Option<(usize, u32)>,
    /// Whether the pointer is on the add button, and on the drag handle.
    handle_hot: (bool, bool),
    /// The pointer has not moved since the last typing: no handles meanwhile.
    typing: bool,
    /// Blocks being dragged by their handle, and the block they would be
    /// dropped in front of.
    drag: Option<(Range<usize>, usize)>,
    /// Blocks selected as blocks rather than as text: where the selection
    /// started and where it reaches, both included. While it is set the
    /// keyboard acts on these blocks and the text caret is hidden, parked
    /// at the start of the second one so that it is what gets scrolled to.
    picked: Option<(usize, usize)>,
    /// A press outside the text that may become a selection box: where it
    /// started and where the pointer is, in document coordinates.
    marquee: Option<(Vec2, Vec2)>,
    /// How far code blocks with unwrapped lines are scrolled sideways, by
    /// block id.
    code_scroll: HashMap<u32, f32>,
    /// Code block whose options (language, wrap, copy) are shown, and
    /// which of the three the pointer is on.
    code: Option<usize>,
    code_hot: [bool; 3],
    /// Whether the pointer is on one of those three buttons.
    on_option: bool,
    /// Block the options were last drawn for and the document version
    /// then, so they can fade out where they were.
    code_drawn: Option<(usize, u32)>,
    lang_menu: Option<LangMenu>,
    /// Id of the code block last copied with its button, and when.
    code_copied: Option<(u32, f64)>,
    head: Head,
    /// The title has the keyboard rather than the document.
    editing_title: bool,
    /// Give the title the keyboard, with its caret at the end.
    focus_title: bool,
    /// The title's caret was on its last line.
    title_last_row: bool,
    /// Whether the buttons above the title, and those on the cover, are
    /// shown; and which of them the pointer is on.
    head_shown: (bool, bool),
    head_hot: Option<HeadButton>,
    /// Whether the pointer is on the icon.
    icon_hot: bool,
    /// The open icon or cover picker, and whether it opened on this frame.
    picker: Option<(Picker, bool)>,
    emoji_picker: EmojiPicker,
    colors: ColorPicker,
    math: Option<MathEdit>,
    /// The selection and the type of its block when last seen: the editor
    /// opens when they change to a caret in a math block.
    math_seen: Option<(Selection, Kind)>,
}

/// Left and right space between the text column's edges and a block's text.
fn insets(b: &Block) -> (f32, f32) {
    let indent = f32::from(b.indent) * STEP;
    match b.kind {
        Kind::Bullet | Kind::Numbered | Kind::Todo => (indent + STEP, 0.0),
        Kind::Quote => (indent + QUOTE_PAD, 0.0),
        Kind::Code | Kind::Math => (indent + CODE_PAD.x, CODE_PAD.x),
        _ => (indent, 0.0),
    }
}

/// Space above and below a block's text.
fn spacing(b: &Block, first: bool) -> (f32, f32) {
    // A heading has twice the space above it that it has below. Both are
    // part of the block: its selection highlight covers them.
    let below = match b.kind {
        Kind::H1 => 10.0,
        Kind::H2 => 8.0,
        Kind::H3 => 6.0,
        Kind::Code | Kind::Math => 6.0 + CODE_PAD.y,
        _ => 0.0,
    };
    let above = match b.kind {
        Kind::H1 | Kind::H2 | Kind::H3 if first => 0.0,
        Kind::H1 | Kind::H2 | Kind::H3 => 2.0 * below,
        _ => below,
    };
    (above + 4.0, below + 4.0)
}

fn ccursor(b: &Block, offset: usize) -> CCursor {
    CCursor::new(b.text.as_str()[..offset].chars().count())
}

fn byte_offset(b: &Block, cursor: CCursor) -> usize {
    let text = b.text.as_str();
    text.char_indices().nth(cursor.index.0).map_or(text.len(), |(i, _)| i)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Start of the word before `at`, skipping whitespace first.
fn word_start(text: &str, at: usize) -> usize {
    let head = text[..at].trim_end();
    let word = head.trim_end_matches(is_word);
    match word.len() == head.len() {
        // Not on a word: step over one punctuation character.
        true => head.char_indices().next_back().map_or(0, |(i, _)| i),
        false => word.len(),
    }
}

/// End of the word after `at`, skipping whitespace first.
fn word_end(text: &str, at: usize) -> usize {
    let tail = text[at..].trim_start();
    let rest = tail.trim_start_matches(is_word);
    let rest = match rest.len() == tail.len() {
        true => rest.chars().next().map_or(rest, |c| &rest[c.len_utf8()..]),
        false => rest,
    };
    text.len() - rest.len()
}

/// The word around `at`, for double-click selection.
fn word_at(text: &str, at: usize) -> Range<usize> {
    let start = text[..at].trim_end_matches(is_word).len();
    let end = text.len() - text[at..].trim_start_matches(is_word).len();
    start..end
}

/// Only plain web and mail addresses are opened.
fn openable(url: &str) -> Option<String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if ["http://", "https://", "mailto:"].iter().any(|p| lower.starts_with(p)) {
        Some(url.to_string())
    } else if url.contains(':') || url.is_empty() {
        None
    } else {
        Some(format!("https://{url}"))
    }
}

impl DocView {
    pub fn show(&mut self, ui: &mut Ui, ed: &mut Editor, pal: &Palette) {
        self.frame += 1;
        let avail = ui.available_width();
        self.width = (avail - 2.0 * MARGIN).clamp(120.0, MAX_WIDTH);
        let id = ui.id().with("document");
        ed.set_time(ui.input(|i| i.time) * 1000.0);

        let coast = self.fling.update(ui, ui.clip_rect().height());
        if coast != 0.0 {
            ui.scroll_with_delta_animation(vec2(0.0, coast), egui::style::ScrollAnimation::none());
            ui.ctx().request_repaint();
        }

        // The document takes all typing, so it holds the keyboard focus
        // whenever the title, the link editor, the language list, the
        // emoji picker and the math editor do not.
        let elsewhere = self.editing_title || self.picking_icon() || self.editing_math();
        if !matches!(self.link, LinkUi::Edit { .. }) && self.lang_menu.is_none() && !elsewhere {
            ui.memory_mut(|m| {
                m.request_focus(id);
                let all = EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true };
                m.set_focus_lock_filter(id, all);
            });
            self.keyboard(ui, ed, pal);
        }
        self.layout(ui, ed, pal);

        let bottom = ui.clip_rect().height() * 0.4;
        let (rect, _) = ui.allocate_exact_size(vec2(avail, self.total + bottom), Sense::hover());
        let response = ui.interact(rect, id, Sense::click_and_drag());
        let origin = pos2(rect.left() + ((avail - self.width) / 2.0).max(MARGIN), rect.top());

        let last = ed.blocks().len() - 1;
        self.picked = self.picked.map(|(anchor, focus)| (anchor.min(last), focus.min(last)));
        self.handles(ui, ed, id, rect, origin);
        self.code_options(ui, ed, id, origin);
        self.head(ui, ed, pal, id, rect, origin);
        self.pointer(ui, ed, &response, origin);
        self.follow_math(ed);
        if !ui.input(|i| i.pointer.primary_down()) {
            self.marquee = None;
        }
        if self.link_version != ed.version() {
            // Positions held by the link popup no longer mean the same text.
            self.link_version = ed.version();
            self.link = LinkUi::Closed;
            self.lang_menu = None;
        }
        self.layout(ui, ed, pal);
        self.scroll_code(ui, ed, origin);
        self.hover(ui, ed, &response, origin);
        self.paint_head(ui, ed, pal, id, rect, origin);
        self.paint(ui, ed, pal, origin);
        self.paint_handles(ui, ed, pal, id, origin);
        self.paint_code_options(ui, ed, pal, id, origin);
        self.popups(ui, ed, pal, origin);
    }

    // ---- layout ----

    fn galley(&mut self, ui: &Ui, b: &Block, pal: &Palette) -> Arc<Galley> {
        let (left, right) = insets(b);
        let unwrapped = b.kind == Kind::Code && b.nowrap;
        let width = if unwrapped { f32::INFINITY } else { (self.width - left - right).max(40.0) };
        let key = (width.to_bits(), pal.dark);
        if let Some(c) = self.cache.get_mut(&b.id) {
            if c.rev == b.rev && c.key == key {
                c.seen = self.frame;
                return c.galley.clone();
            }
        }
        let galley = ui.painter().layout_job(theme::job(b, width, pal));
        let cached = Cached { rev: b.rev, key, galley: galley.clone(), seen: self.frame };
        self.cache.insert(b.id, cached);
        galley
    }

    /// Positions every block. Does nothing when neither the document, the
    /// width nor the theme changed since the last call.
    fn layout(&mut self, ui: &Ui, ed: &Editor, pal: &Palette) {
        let editing = self.math.as_ref().filter(|m| !m.closed).map(|m| m.block);
        let state = Some((ed.version(), self.width.to_bits(), pal.dark, editing));
        if self.laid == state {
            return;
        }
        self.laid = state;
        self.rows.clear();
        self.layout_head(ui, ed, pal);
        let mut numbers = [0u32; notely_core::MAX_INDENT as usize + 1];
        let mut y = self.head.end;
        for (i, b) in ed.blocks().iter().enumerate() {
            let depth = usize::from(b.indent.min(notely_core::MAX_INDENT));
            if b.kind == Kind::Numbered {
                numbers[depth] += 1;
                numbers[depth + 1..].fill(0);
            } else {
                numbers[depth..].fill(0);
            }
            let galley = self.galley(ui, b, pal);
            let (above, below) = spacing(b, i == 0);
            let mut height = above + galley.size().y + below;
            let (left, right) = insets(b);
            // Unwrapped code keeps how far it was scrolled sideways.
            let most = (galley.size().x - (self.width - left - right)).max(0.0);
            let across = match b.kind == Kind::Code && b.nowrap {
                true => self.code_scroll.get(&b.id).copied().unwrap_or(0.0).min(most),
                false => 0.0,
            };
            let text = vec2(left - across, y + above);
            let lead = theme::lead(ui, b.kind);
            let (mut formula, mut typeset) = (None, false);
            if b.kind == Kind::Math {
                let indent = f32::from(b.indent) * STEP;
                match self.fit_formula(ui, self.shown_tex(b), self.width - indent) {
                    Some(Ok((extent, size))) => {
                        typeset = true;
                        let at = pos2(indent + (self.width - indent - extent.x) / 2.0, y + 4.0 + MATH_PAD);
                        formula = Some((Rect::from_min_size(at, extent), size));
                        height = 2.0 * (4.0 + MATH_PAD) + extent.y;
                    }
                    // Its TeX, with a line below saying what is wrong with it.
                    Some(Err(_)) => height += MATH_ERROR - 6.0,
                    // A box saying it is empty.
                    None => {}
                }
            }
            self.rows.push(Row { top: y, height, text, lead, galley, number: numbers[depth], formula, typeset });
            y += height;
        }
        self.total = y;
        // Forget galleys of blocks that no longer exist.
        if self.cache.len() > self.rows.len() * 2 + 64 {
            let frame = self.frame;
            self.cache.retain(|_, c| c.seen == frame);
        }
    }

    /// The TeX a math block shows: its own, or while it is edited, the last
    /// that could be read.
    fn shown_tex<'a>(&'a self, b: &'a Block) -> &'a str {
        match &self.math {
            Some(m) if m.block == b.id && !m.closed => &m.shown,
            _ => b.text.as_str(),
        }
    }

    /// The extent and font size of a formula, made smaller to fit `width`;
    /// None while it is empty, and why its TeX is invalid if it is.
    fn fit_formula(&self, ui: &Ui, tex: &str, width: f32) -> Option<Result<(Vec2, f32), String>> {
        if tex.trim().is_empty() {
            return None;
        }
        Some(math::measure(ui.ctx(), tex, MATH_SIZE).map(|extent| {
            let scale = (width / extent.x).min(MATH_MAX_HEIGHT / extent.y).min(1.0);
            (extent * scale, MATH_SIZE * scale)
        }))
    }

    /// Places the cover, the icon, the buttons above the title and the title.
    fn layout_head(&mut self, ui: &Ui, ed: &Editor, pal: &Palette) {
        let page = ed.page();
        let cover = if page.cover.is_empty() { 0.0 } else { COVER_HEIGHT };
        let mut y = if page.cover.is_empty() { TOP } else { cover + 12.0 };
        let icon = (!page.icon.is_empty()).then(|| {
            // Over a cover, the icon hangs half way off its bottom edge.
            let top = if page.cover.is_empty() { y } else { cover - ICON_BOX / 2.0 };
            y = top + ICON_BOX + 8.0;
            Rect::from_min_size(pos2(-6.0, top), Vec2::splat(ICON_BOX))
        });
        let buttons = (page.icon.is_empty() || page.cover.is_empty()).then_some(y);
        if buttons.is_some() {
            y += HEAD_BUTTON_HEIGHT + 6.0;
        }
        // Wrapped as the title's text field wraps it.
        let galley = ui.painter().layout(page.title.clone(), theme::title_font(), pal.fg, self.width);
        let title = (y, galley.size().y);
        self.head = Head { cover, icon, buttons, title, end: y + title.1 + 20.0 };
    }

    /// Caret rectangle of a position, in document coordinates.
    fn caret(&self, ed: &Editor, p: Pos, next_row: bool) -> Rect {
        let row = &self.rows[p.block];
        let mut cursor = ccursor(&ed.blocks()[p.block], p.offset);
        cursor.prefer_next_row = next_row;
        let rect = row.galley.pos_from_cursor(cursor);
        rect.translate(vec2(ink_nudge(&row.galley, cursor), 0.0) + row.text)
    }

    /// The position nearest a point given in document coordinates, and
    /// whether it is the start of a wrapped row.
    fn hit(&self, ed: &Editor, at: Vec2) -> (Pos, bool) {
        let i = self.rows.partition_point(|r| r.top + r.height <= at.y).min(self.rows.len() - 1);
        let row = &self.rows[i];
        if row.typeset {
            // A formula opens with the caret after its TeX.
            return (Pos::new(i, ed.blocks()[i].text.len()), false);
        }
        let cursor = row.galley.cursor_from_pos(at - row.text);
        (Pos::new(i, byte_offset(&ed.blocks()[i], cursor)), cursor.prefer_next_row)
    }

    // ---- keyboard ----

    fn keyboard(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette) {
        let events = ui.input(|i| i.events.clone());
        let before = (ed.version(), ed.selection());
        for event in events {
            if self.picked.is_some() {
                self.block_event(ui, ed, pal, &event);
                continue;
            }
            match event {
                Event::Text(text) => {
                    self.h_pos = None;
                    ed.insert_text(&text);
                    if text == "/" {
                        self.open_menu(ed);
                    }
                }
                Event::Paste(text) => ed.paste(&text.replace('\r', "")),
                Event::Copy => self.copy(ui, ed),
                Event::Cut => {
                    self.copy(ui, ed);
                    ed.delete_selection();
                }
                Event::Ime(ImeEvent::Preedit { text, .. }) => self.preedit = text,
                Event::Ime(ImeEvent::Commit(text)) => {
                    self.preedit.clear();
                    ed.insert_text(&text);
                }
                Event::Key { key, pressed: true, modifiers, .. } => {
                    self.key(ui, ed, pal, key, modifiers);
                }
                _ => {}
            }
        }
        if (ed.version(), ed.selection()) != before {
            self.reveal = true;
            self.typing = true;
            self.update_menu(ed);
        }
    }

    fn copy(&self, ui: &Ui, ed: &Editor) {
        if !ed.selection().is_collapsed() {
            crate::clipboard::copy(ui.ctx(), ed.selection_markdown());
        }
    }

    fn key(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette, key: Key, m: Modifiers) {
        if self.menu_key(ed, key) {
            return;
        }
        let cmd = m.command && !m.alt;
        if !matches!(key, Key::ArrowUp | Key::ArrowDown) || (cmd && m.shift) {
            self.h_pos = None;
        }
        match (cmd, m.shift, key) {
            (false, false, Key::ArrowUp) if self.on_first_line(ui, ed, pal) => {
                self.focus_title = true;
                self.menu = None;
            }
            (false, shift, Key::Tab) => ed.tab(shift),
            (true, _, Key::Enter) => ed.mod_enter(),
            (false, true, Key::Enter) => ed.insert_line_break(),
            (false, false, Key::Enter) => ed.insert_paragraph(),
            (_, _, Key::Backspace) => self.delete(ed, true, cmd),
            (_, _, Key::Delete) => self.delete(ed, false, cmd),
            (true, true, Key::ArrowUp) => ed.move_blocks(true),
            (true, true, Key::ArrowDown) => ed.move_blocks(false),
            (
                _,
                shift,
                Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End,
            ) => {
                self.layout(ui, ed, pal);
                let sel = ed.selection();
                let horizontal = matches!(key, Key::ArrowLeft | Key::ArrowRight);
                let to = if horizontal && !shift && !sel.is_collapsed() {
                    // Without Shift, an arrow first collapses the selection.
                    if key == Key::ArrowLeft { sel.start() } else { sel.end() }
                } else {
                    self.target(ed, key, cmd)
                };
                if shift || to.block == sel.focus.block {
                    ed.set_selection(Selection { anchor: if shift { sel.anchor } else { to }, focus: to });
                } else {
                    self.land(ed, to);
                }
            }
            (true, false, Key::A) => {
                // First the text of the block, then every block.
                let sel = ed.selection();
                let block = sel.focus.block;
                let end = Pos::new(block, ed.blocks()[block].text.len());
                if (sel.start(), sel.end()) == (Pos::new(block, 0), end) || sel.anchor.block != block {
                    self.pick(ed, 0, ed.blocks().len() - 1);
                } else {
                    ed.set_selection(Selection { anchor: Pos::new(block, 0), focus: end });
                }
            }
            (true, false, Key::B) => ed.toggle_mark(mark::BOLD),
            (true, false, Key::I) => ed.toggle_mark(mark::ITALIC),
            (true, false, Key::U) => ed.toggle_mark(mark::UNDERLINE),
            (true, false, Key::E) => ed.toggle_mark(mark::CODE),
            (true, false, Key::K) => self.edit_link(ed),
            (false, false, Key::Escape) => (self.link, self.lang_menu) = (LinkUi::Closed, None),
            (true, true, Key::S | Key::X) => ed.toggle_mark(mark::STRIKE),
            (true, false, Key::Z) => drop(ed.undo()),
            (true, true, Key::Z) | (true, false, Key::Y) => drop(ed.redo()),
            (true, true, Key::Num0) => ed.set_kind(Kind::Paragraph),
            (true, true, Key::Num1) => ed.set_kind(Kind::H1),
            (true, true, Key::Num2) => ed.set_kind(Kind::H2),
            (true, true, Key::Num3) => ed.set_kind(Kind::H3),
            (true, true, Key::Num4) => ed.set_kind(Kind::Todo),
            (true, true, Key::Num5) => ed.set_kind(Kind::Bullet),
            (true, true, Key::Num6) => ed.set_kind(Kind::Numbered),
            (true, true, Key::Num7) => ed.set_kind(Kind::Quote),
            (true, true, Key::Num8) => ed.set_kind(Kind::Code),
            (true, true, Key::Num9) => ed.set_kind(Kind::Math),
            _ => {}
        }
    }

    /// Whether the caret is on the first line of the document, where Up
    /// goes on to the title.
    fn on_first_line(&mut self, ui: &Ui, ed: &Editor, pal: &Palette) -> bool {
        self.layout(ui, ed, pal);
        let p = ed.selection().focus;
        let mut cursor = ccursor(&ed.blocks()[0], p.offset);
        cursor.prefer_next_row = self.next_row;
        p.block == 0 && self.rows[0].galley.layout_from_cursor(cursor).row == 0
    }

    // ---- blocks selected as blocks ----

    /// Selects blocks `anchor` to `focus` as blocks.
    fn pick(&mut self, ed: &mut Editor, anchor: usize, focus: usize) {
        self.picked = Some((anchor, focus));
        self.menu = None;
        self.link = LinkUi::Closed;
        ed.set_selection(Selection::caret(Pos::new(focus, 0)));
    }

    /// Moves the caret to `to` from another block. A math block is picked
    /// rather than entered: Enter opens its editor, arrows go on past it.
    fn land(&mut self, ed: &mut Editor, to: Pos) {
        if ed.blocks()[to.block].kind == Kind::Math {
            self.pick(ed, to.block, to.block);
        } else {
            self.picked = None;
            ed.set_selection(Selection::caret(to));
        }
    }

    /// Keyboard and clipboard input while blocks are selected as blocks.
    fn block_event(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette, event: &Event) {
        let Some((anchor, focus)) = self.picked else { return };
        let last = ed.blocks().len() - 1;
        let (lo, hi) = (anchor.min(focus), anchor.max(focus));
        // The same blocks as a text selection, for commands that take one.
        let span = |ed: &mut Editor| {
            let end = Pos::new(hi, ed.blocks()[hi].text.len());
            ed.set_selection(Selection { anchor: Pos::new(lo, 0), focus: end });
        };
        let copy = |ed: &Editor| {
            let text = notely_core::markdown::serialize(&ed.blocks()[lo..=hi]);
            crate::clipboard::copy(ui.ctx(), text);
        };
        let remove = |view: &mut Self, ed: &mut Editor| {
            view.picked = None;
            ed.delete_blocks(lo..hi + 1);
        };
        match event {
            Event::Copy => copy(ed),
            Event::Cut => {
                copy(ed);
                remove(self, ed);
            }
            Event::Paste(text) => {
                self.picked = None;
                span(ed);
                ed.paste(&text.replace('\r', ""));
            }
            Event::Key { key, pressed: true, modifiers, .. } => {
                let (key, m) = (*key, *modifiers);
                let cmd = m.command && !m.alt;
                match (cmd, m.shift, key) {
                    (_, _, Key::Backspace | Key::Delete) => remove(self, ed),
                    (false, _, Key::Escape) => self.picked = None,
                    (false, false, Key::Enter) if lo == hi && ed.blocks()[hi].kind == Kind::Math => self.open_math(ed, hi),
                    (false, false, Key::Enter) => {
                        self.picked = None;
                        ed.set_selection(Selection::caret(Pos::new(hi, ed.blocks()[hi].text.len())));
                    }
                    // Arrows step over a formula picked by arrowing onto it.
                    (false, false, Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight)
                        if lo == hi && ed.blocks()[hi].kind == Kind::Math =>
                    {
                        let to = match key {
                            Key::ArrowUp | Key::ArrowLeft => hi.checked_sub(1).map(|i| Pos::new(i, ed.blocks()[i].text.len())),
                            _ => (hi < last).then(|| Pos::new(hi + 1, 0)),
                        };
                        if let Some(to) = to {
                            self.land(ed, to);
                        }
                    }
                    (false, shift, Key::ArrowUp | Key::ArrowDown) => {
                        let to = if key == Key::ArrowUp { focus.saturating_sub(1) } else { (focus + 1).min(last) };
                        self.pick(ed, if shift { anchor } else { to }, to);
                    }
                    (true, false, Key::A) => self.pick(ed, 0, last),
                    (true, _, Key::Z | Key::Y) => {
                        self.picked = None;
                        self.key(ui, ed, pal, key, m);
                    }
                    // Commands that work on the blocks of a text selection.
                    (false, _, Key::Tab) | (true, true, Key::ArrowUp | Key::ArrowDown) => {
                        span(ed);
                        self.key(ui, ed, pal, key, m);
                        let (a, b) = (ed.selection().start().block, ed.selection().end().block);
                        let (anchor, focus) = if anchor <= focus { (a, b) } else { (b, a) };
                        self.pick(ed, anchor, focus);
                    }
                    (true, true, k) if (Key::Num0..=Key::Num9).contains(&k) => {
                        span(ed);
                        self.key(ui, ed, pal, key, m);
                        self.pick(ed, anchor, focus);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Selects the blocks a box drawn between two points touches.
    fn box_select(&mut self, ed: &mut Editor, from: Vec2, to: Vec2) {
        let area = Rect::from_two_pos(from.to_pos2(), to.to_pos2());
        let touched = |i: &usize| {
            let (row, left) = (&self.rows[*i], f32::from(ed.blocks()[*i].indent) * STEP);
            let across = area.right() >= left && area.left() <= self.width;
            across && row.top < area.bottom() && row.top + row.height > area.top()
        };
        let first = (0..self.rows.len()).find(touched);
        let last = (0..self.rows.len()).rev().find(touched);
        match first.zip(last) {
            Some((first, last)) if to.y >= from.y => self.pick(ed, first, last),
            Some((first, last)) => self.pick(ed, last, first),
            None => self.picked = None,
        }
    }

    /// Backspace/Delete; with `word` (Ctrl) up to the nearest word boundary.
    fn delete(&mut self, ed: &mut Editor, backward: bool, word: bool) {
        let p = ed.selection().focus;
        if word && ed.selection().is_collapsed() {
            let text = ed.blocks()[p.block].text.as_str();
            let to = if backward { word_start(text, p.offset) } else { word_end(text, p.offset) };
            if to != p.offset {
                return ed.delete_span(Pos::new(p.block, to), p);
            }
        }
        if backward {
            ed.delete_backward();
        } else {
            ed.delete_forward();
        }
    }

    /// Where a movement key takes the caret.
    fn target(&mut self, ed: &Editor, key: Key, cmd: bool) -> Pos {
        let blocks = ed.blocks();
        let p = ed.selection().focus;
        let b = &blocks[p.block];
        let text = b.text.as_str();
        let row = &self.rows[p.block];
        let mut cursor = ccursor(b, p.offset);
        cursor.prefer_next_row = self.next_row;
        let last = blocks.len() - 1;
        match key {
            Key::ArrowLeft | Key::ArrowRight => self.next_row = false,
            Key::Home => self.next_row = true,
            Key::End => self.next_row = false,
            _ => {}
        }
        match key {
            Key::ArrowLeft if p.offset > 0 => {
                let prev = text[..p.offset].char_indices().next_back().map_or(0, |(i, _)| i);
                Pos::new(p.block, if cmd { word_start(text, p.offset) } else { prev })
            }
            Key::ArrowLeft if p.block > 0 => Pos::new(p.block - 1, blocks[p.block - 1].text.len()),
            Key::ArrowRight if p.offset < text.len() => {
                let next = p.offset + text[p.offset..].chars().next().map_or(0, char::len_utf8);
                Pos::new(p.block, if cmd { word_end(text, p.offset) } else { next })
            }
            Key::ArrowRight if p.block < last => Pos::new(p.block + 1, 0),
            Key::Home if cmd => Pos::default(),
            Key::End if cmd => Pos::new(last, blocks[last].text.len()),
            Key::Home => Pos::new(p.block, byte_offset(b, row.galley.cursor_begin_of_row(&cursor))),
            Key::End => Pos::new(p.block, byte_offset(b, row.galley.cursor_end_of_row(&cursor))),
            Key::ArrowUp | Key::ArrowDown => {
                let down = key == Key::ArrowDown;
                let caret_x = row.text.x + row.galley.pos_from_cursor(cursor).center().x;
                let x = *self.h_pos.get_or_insert(caret_x);
                let line = row.galley.layout_from_cursor(cursor).row;
                let inside = if down { line + 1 < row.galley.rows.len() } else { line > 0 };
                if inside {
                    let x = Some(x - row.text.x);
                    let (to, _) = match down {
                        true => row.galley.cursor_down_one_row(&cursor, x),
                        false => row.galley.cursor_up_one_row(&cursor, x),
                    };
                    self.next_row = to.prefer_next_row;
                    return Pos::new(p.block, byte_offset(b, to));
                }
                // Into the neighbouring block, landing in the same column.
                let n = if down { p.block + 1 } else { p.block.wrapping_sub(1) };
                let Some(next) = self.rows.get(n) else {
                    return Pos::new(p.block, if down { text.len() } else { 0 });
                };
                let y = if down { 0.0 } else { next.galley.size().y - 1.0 };
                let to = next.galley.cursor_from_pos(vec2(x - next.text.x, y));
                self.next_row = to.prefer_next_row;
                Pos::new(n, byte_offset(&blocks[n], to))
            }
            _ => p,
        }
    }

    // ---- pointer ----

    fn pointer(&mut self, ui: &Ui, ed: &mut Editor, response: &Response, origin: Pos2) {
        let Some(pos) = response.interact_pointer_pos() else { return };
        // A press on the page's head is not one in the text.
        if ui.input(|i| i.pointer.press_origin()).is_some_and(|p| p.y - origin.y < self.head.end) {
            return;
        }
        let at = pos - origin;
        let (shift, pressed) = ui.input(|i| (i.modifiers.shift, i.pointer.primary_pressed()));
        let before = ed.selection();
        if pressed {
            self.h_pos = None;
            self.picked = None;
            // A press beside, above or below the text can become a box.
            let beside = !(0.0..=self.width).contains(&at.x) || !(self.head.end..self.total).contains(&at.y);
            self.marquee = beside.then_some((at, at));
            if at.y >= self.total {
                // Below the last block: a click continues the document there.
            } else if let Some(i) = self.checkbox_at(ed, at) {
                ed.toggle_todo(i);
            } else {
                let (p, next_row) = self.hit(ed, at);
                self.next_row = next_row;
                ed.set_selection(Selection { anchor: if shift { before.anchor } else { p }, focus: p });
            }
        } else if response.triple_clicked() {
            let (p, _) = self.hit(ed, at);
            let end = Pos::new(p.block, ed.blocks()[p.block].text.len());
            ed.set_selection(Selection { anchor: Pos::new(p.block, 0), focus: end });
        } else if response.double_clicked() {
            let (p, _) = self.hit(ed, at);
            let word = word_at(ed.blocks()[p.block].text.as_str(), p.offset);
            let (anchor, focus) = (Pos::new(p.block, word.start), Pos::new(p.block, word.end));
            ed.set_selection(Selection { anchor, focus });
        } else if response.dragged() {
            if let Some((from, _)) = self.marquee {
                self.marquee = Some((from, at));
                self.box_select(ed, from, at);
            } else {
                let (p, next_row) = self.hit(ed, at);
                self.next_row = next_row;
                ed.set_selection(Selection { anchor: before.anchor, focus: p });
            }
            // Dragging at or past the top or bottom edge scrolls that way.
            let view = ui.clip_rect();
            let push = (view.top() + 24.0 - pos.y).max(0.0) - (pos.y - view.bottom() + 24.0).max(0.0);
            if push != 0.0 {
                ui.scroll_with_delta(vec2(0.0, push.clamp(-40.0, 40.0)));
                ui.ctx().request_repaint();
            }
        }
        if response.clicked() && at.y >= self.total {
            ed.focus_end();
        }
        // A plain click on a link (not a drag across it) opens it.
        if response.clicked() && !shift {
            let link = self.link_under(ed, at);
            if let Some(url) = link.and_then(|(_, url)| openable(&url)) {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
        if ed.selection() != before {
            self.update_menu(ed);
        }
    }

    // ---- code blocks ----

    /// A painter for a block's text. Code is kept inside its block, where
    /// lines that are not wrapped would run out of it.
    fn text_painter(&self, painter: &Painter, b: &Block, row: &Row, origin: Pos2) -> Painter {
        if !theme::is_source(b.kind) {
            return painter.clone();
        }
        let left = origin.x + f32::from(b.indent) * STEP + CODE_PAD.x - 4.0;
        let min = pos2(left, origin.y + row.top);
        let max = pos2(origin.x + self.width - CODE_PAD.x + 4.0, origin.y + row.top + row.height);
        painter.with_clip_rect(Rect::from_min_max(min, max).intersect(painter.clip_rect()))
    }

    /// Scrolls unwrapped code sideways: the block under the pointer by the
    /// wheel or a swipe, the one with the caret to keep the caret in view.
    fn scroll_code(&mut self, ui: &Ui, ed: &Editor, origin: Pos2) {
        let (pointer, wheel) = ui.input(|i| (i.pointer.hover_pos(), i.smooth_scroll_delta.x));
        let under = pointer.map(|p| p - origin).filter(|at| (self.head.end..self.total).contains(&at.y) && wheel != 0.0);
        let under = under.map(|at| self.rows.partition_point(|r| r.top + r.height <= at.y));
        let focus = ed.selection().focus;
        for (i, wheel) in [(under, wheel), (self.reveal.then_some(focus.block), 0.0)] {
            let Some((b, row)) = i.and_then(|i| Some((ed.blocks().get(i)?, self.rows.get_mut(i)?))) else { continue };
            if b.kind != Kind::Code || !b.nowrap {
                continue;
            }
            let (left, right) = insets(b);
            let room = self.width - left - right;
            let mut across = left - row.text.x - wheel;
            if wheel == 0.0 {
                let caret = row.galley.pos_from_cursor(ccursor(b, focus.offset)).left();
                across = across.clamp(caret - room + 8.0, (caret - 8.0).max(caret - room + 8.0));
            }
            let across = across.clamp(0.0, (row.galley.size().x - room).max(0.0));
            row.text.x = left - across;
            self.code_scroll.insert(b.id, across);
        }
    }

    /// The language, wrap and copy buttons in a code block's top right.
    fn option_rects(&self, ui: &Ui, ed: &Editor, origin: Pos2, i: usize) -> [Rect; 3] {
        let (b, row) = (&ed.blocks()[i], &self.rows[i]);
        let name = ui.painter().layout_no_wrap(language_name(b).to_owned(), FontId::proportional(12.0), Color32::WHITE);
        let top = origin.y + row.text.y - CODE_PAD.y + OPTION_INSET;
        let right = origin.x + self.width - OPTION_INSET;
        let cut = |right: f32, width: f32| {
            Rect::from_min_max(pos2(right - width, top), pos2(right, top + OPTION_HEIGHT))
        };
        let copy = cut(right, OPTION_WIDTH);
        let wrap = cut(copy.left(), OPTION_WIDTH);
        [cut(wrap.left(), name.size().x + 16.0), wrap, copy]
    }

    /// Shows the options of the code block under the pointer and acts on
    /// them.
    fn code_options(&mut self, ui: &Ui, ed: &mut Editor, id: Id, origin: Pos2) {
        let open = self.lang_menu.as_ref().map(|menu| menu.block);
        self.code = open.or(self.handle).filter(|&i| {
            i < self.rows.len() && ed.blocks()[i].kind == Kind::Code && self.drag.is_none()
        });
        self.code_hot = [false; 3];
        self.on_option = false;
        let Some(i) = self.code else {
            self.lang_menu = None;
            return;
        };
        let [lang, wrap, copy] = self.option_rects(ui, ed, origin, i);
        let lang = ui.interact(lang, id.with("code-language"), Sense::click());
        let wrap = ui.interact(wrap, id.with("code-wrap"), Sense::click()).on_hover_text("Wrap lines");
        let copy = ui.interact(copy, id.with("code-copy"), Sense::click()).on_hover_text("Copy code");
        self.code_hot = [lang.hovered() || open.is_some(), wrap.hovered(), copy.hovered()];
        self.on_option = lang.hovered() || wrap.hovered() || copy.hovered();
        if self.on_option {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if lang.clicked() {
            let active = usize::from(ed.blocks()[i].lang);
            let menu = LangMenu { block: i, fresh: true, query: String::new(), active };
            self.lang_menu = if open.is_some() { None } else { Some(menu) };
        }
        if wrap.clicked() {
            ed.toggle_wrap(i);
        }
        if copy.clicked() {
            let b = &ed.blocks()[i];
            ui.ctx().copy_text(b.text.as_str().to_owned());
            self.code_copied = Some((b.id, ui.input(|i| i.time)));
            ui.ctx().request_repaint_after_secs(COPIED as f32);
        }
    }

    fn paint_code_options(&mut self, ui: &Ui, ed: &Editor, pal: &Palette, id: Id, origin: Pos2) {
        let opacity = ui.ctx().animate_bool_with_time(id.with("code-options"), self.code.is_some(), FADE);
        if let Some(i) = self.code {
            self.code_drawn = Some((i, ed.version()));
        }
        // Fading out, they stay on the block they were on.
        let still = self.code_drawn.filter(|&(i, version)| {
            version == ed.version() && i < self.rows.len() && ed.blocks()[i].kind == Kind::Code
        });
        let Some((i, _)) = still.filter(|_| opacity > 0.0) else { return };
        let b = &ed.blocks()[i];
        let mut painter = ui.painter().clone();
        painter.set_opacity(opacity);
        let [lang, wrap, copy] = self.option_rects(ui, ed, origin, i);
        // A plate of the block's own colour hides the code underneath.
        painter.rect_filled(lang.union(copy).expand(3.0), 5.0, pal.code_bg);
        for (rect, hot) in [lang, wrap, copy].into_iter().zip(self.code_hot) {
            if hot {
                painter.rect_filled(rect, 4.0, pal.line);
            }
        }
        let color = |hot| if hot { pal.fg } else { pal.muted };
        painter.text(lang.center(), Align2::CENTER_CENTER, language_name(b), FontId::proportional(12.0), color(self.code_hot[0]));
        // Lit while lines are wrapped.
        wrap_icon(&painter, wrap.center(), if b.nowrap { pal.faint } else { color(true) });
        let now = ui.input(|i| i.time);
        let copied = self.code_copied.is_some_and(|(block, at)| block == b.id && now - at < COPIED);
        copy_icon(&painter, copy.center(), copied, color(self.code_hot[2]));
    }

    /// The language list of the code block whose language button was
    /// clicked: a search field and the languages that match it.
    fn language_menu(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette, origin: Pos2) {
        let Some(mut menu) = self.lang_menu.take().filter(|menu| menu.block < self.rows.len()) else { return };
        let ctx = ui.ctx();
        let opened = std::mem::take(&mut menu.fresh);
        let [lang, ..] = self.option_rects(ui, ed, origin, menu.block);
        let current = usize::from(ed.blocks()[menu.block].lang);
        let query = menu.query.trim().to_ascii_lowercase();
        let items: Vec<usize> = (0..LANGUAGES.len()).filter(|&n| LANGUAGES[n].matches(&query)).collect();
        let (up, down, enter, escape) = ctx.input(|i| {
            let pressed = |key| i.key_pressed(key);
            (pressed(Key::ArrowUp), pressed(Key::ArrowDown), pressed(Key::Enter), pressed(Key::Escape))
        });
        if !items.is_empty() {
            let n = items.len();
            menu.active = (menu.active.min(n - 1) + usize::from(down) + if up { n - 1 } else { 0 }) % n;
        }
        let mut chosen = items.get(menu.active).copied().filter(|_| enter);
        let shown = Area::new(Id::new("code-languages"))
            .order(Order::Foreground)
            .pivot(Align2::RIGHT_TOP)
            .fixed_pos(pos2(origin.x + self.width, lang.bottom() + 6.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let field = egui::TextEdit::singleline(&mut menu.query).hint_text("Search").desired_width(158.0);
                    let field = ui.add(field);
                    if opened {
                        field.request_focus();
                    }
                    if field.changed() {
                        menu.active = 0;
                    }
                    ui.add_space(4.0);
                    for (k, &n) in items.iter().enumerate() {
                        let (rect, item) = ui.allocate_exact_size(vec2(166.0, 24.0), Sense::click());
                        let item = item.on_hover_cursor(CursorIcon::PointingHand);
                        let visuals = ui.visuals();
                        if k == menu.active || item.hovered() {
                            ui.painter().rect_filled(rect, 5.0, visuals.widgets.hovered.weak_bg_fill);
                        }
                        let at = rect.left_center() + vec2(10.0, 0.0);
                        let font = FontId::proportional(13.0);
                        ui.painter().text(at, Align2::LEFT_CENTER, LANGUAGES[n].name, font, visuals.text_color());
                        if n == current {
                            let c = rect.right_center() - vec2(14.0, 0.0);
                            let tick = [c + vec2(-4.0, 0.2), c + vec2(-1.3, 3.0), c + vec2(4.2, -3.2)];
                            ui.painter().line(tick.to_vec(), Stroke::new(1.4, pal.muted));
                        }
                        if item.clicked() {
                            chosen = Some(n);
                        }
                    }
                    if items.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(vec2(166.0, 24.0), Sense::hover());
                        let at = rect.left_center() + vec2(10.0, 0.0);
                        ui.painter().text(at, Align2::LEFT_CENTER, "No match", FontId::proportional(13.0), pal.muted);
                    }
                });
            });
        if let Some(language) = chosen {
            ed.set_lang(menu.block, language as u8);
        } else if !escape && (opened || !shown.response.clicked_elsewhere()) {
            self.lang_menu = Some(menu);
        }
    }

    // ---- the page's cover, icon and title ----

    /// The buttons above the title and those on the cover, on the screen.
    fn head_buttons(&self, ui: &Ui, ed: &Editor, origin: Pos2) -> Vec<(HeadButton, Rect)> {
        let page = ed.page();
        let font = FontId::proportional(13.5);
        let width = |b: HeadButton| ui.painter().layout_no_wrap(b.label().to_owned(), font.clone(), Color32::WHITE).size().x + 16.0;
        let mut buttons = Vec::new();
        if let Some(top) = self.head.buttons {
            let mut left = origin.x - 6.0;
            let missing = [(HeadButton::AddIcon, page.icon.is_empty()), (HeadButton::AddCover, page.cover.is_empty())];
            for (b, _) in missing.into_iter().filter(|(_, missing)| *missing) {
                let rect = Rect::from_min_size(pos2(left, origin.y + top), vec2(width(b), HEAD_BUTTON_HEIGHT));
                left = rect.right() + 4.0;
                buttons.push((b, rect));
            }
        }
        if self.head.cover > 0.0 {
            // In the cover's bottom right corner, over the end of the text column.
            let (bottom, mut right) = (origin.y + self.head.cover - 12.0, origin.x + self.width);
            for b in [HeadButton::RemoveCover, HeadButton::ChangeCover] {
                let rect = Rect::from_min_max(pos2(right - width(b), bottom - 26.0), pos2(right, bottom));
                right = rect.left() - 6.0;
                buttons.push((b, rect));
            }
        }
        buttons
    }

    /// Shows the buttons of the part of the page's head under the pointer
    /// and acts on them and on the icon, then shows the title.
    fn head(&mut self, ui: &mut Ui, ed: &mut Editor, pal: &Palette, id: Id, rect: Rect, origin: Pos2) {
        let at = ui.input(|i| i.pointer.hover_pos()).filter(|_| ui.rect_contains_pointer(rect)).map(|p| p - origin);
        // Not while blocks are dragged or text is selected with the mouse.
        let free = self.drag.is_none() && ui.ctx().dragged_id() != Some(id);
        let over_head = at.is_some_and(|at| (-MARGIN..=self.width + MARGIN).contains(&at.x) && at.y < self.head.end);
        let over_cover = at.is_some_and(|at| at.y < self.head.cover);
        let picking_cover = self.picker.is_some_and(|(p, _)| p == Picker::Cover);
        self.head_shown = (free && (over_head || self.editing_title), free && (over_cover || picking_cover));
        self.head_hot = None;
        let mut page = ed.page().clone();
        let toggle = |picker: &mut Option<(Picker, bool)>, which| {
            *picker = match *picker {
                Some((open, _)) if open == which => None,
                _ => Some((which, true)),
            };
        };
        for (button, r) in self.head_buttons(ui, ed, origin) {
            if !(if button.on_cover() { self.head_shown.1 } else { self.head_shown.0 }) {
                continue;
            }
            let response = ui.interact(r, id.with(button), Sense::click());
            if response.hovered() {
                self.head_hot = Some(button);
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if !response.clicked() {
                continue;
            }
            // New icons and covers are picked at random, as in Notion.
            let random = (ui.input(|i| i.time) * 1000.0) as usize;
            match button {
                HeadButton::AddIcon => {
                    page.icon = emoji::random(random).to_owned();
                }
                HeadButton::AddCover => page.cover = theme::COVERS[random % theme::COVERS.len()].0.to_owned(),
                HeadButton::ChangeCover => toggle(&mut self.picker, Picker::Cover),
                HeadButton::RemoveCover => page.cover.clear(),
            }
        }
        self.icon_hot = false;
        if let Some(icon) = self.head.icon {
            let response = ui.interact(icon.translate(origin.to_vec2()), id.with("icon"), Sense::click());
            self.icon_hot = response.hovered();
            if response.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            }
            if response.clicked() {
                toggle(&mut self.picker, Picker::Icon);
            }
        }
        ed.set_page(page);
        self.title(ui, ed, pal, origin);
    }

    /// The page's title: a text field of its own, which has the keyboard
    /// while it is edited. Enter, or Down on its last line, goes on to the
    /// document.
    fn title(&mut self, ui: &mut Ui, ed: &mut Editor, pal: &Palette, origin: Pos2) {
        let ctx = ui.ctx().clone();
        let id = ui.id().with("title");
        let mut page = ed.page().clone();
        if std::mem::take(&mut self.focus_title) {
            let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
            let end = CCursor::new(page.title.chars().count());
            state.cursor.set_char_range(Some(CCursorRange::one(end)));
            state.store(&ctx, id);
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let (enter, down, escape, pressed) = ctx.input(|i| {
            (i.key_pressed(Key::Enter), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Escape), i.pointer.any_pressed())
        });
        let (top, height) = self.head.title;
        let rect = Rect::from_min_size(origin + vec2(0.0, top), vec2(self.width, height));
        let mut field = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let hint = egui::RichText::new("Untitled").font(theme::title_font()).color(pal.faint);
        let output = egui::TextEdit::multiline(&mut page.title)
            .id(id)
            .font(theme::title_font())
            .text_color(pal.fg)
            .hint_text(hint)
            .frame(egui::Frame::NONE)
            .desired_width(self.width)
            .desired_rows(1)
            .return_key(None)
            .show(&mut field);
        let response = &output.response.response;
        let mut focused = response.has_focus();
        if focused && (enter || (down && self.title_last_row)) {
            ed.set_selection(Selection::caret(Pos::default()));
            (self.picked, self.next_row, self.reveal, focused) = (None, false, true, false);
        } else if escape || (pressed && !response.hovered()) {
            focused = false;
        }
        if !focused && response.has_focus() {
            response.surrender_focus();
        }
        self.editing_title = focused;
        self.title_last_row = output.cursor_range.is_some_and(|range| {
            output.galley.layout_from_cursor(range.primary).row + 1 >= output.galley.rows.len()
        });
        // A pasted line break or tab has no place in a title.
        if page.title.contains(['\n', '\r', '\t']) {
            page.title = page.title.replace(['\n', '\r', '\t'], " ");
        }
        ed.set_page(page);
    }

    /// The cover, the icon and the buttons for them.
    fn paint_head(&self, ui: &Ui, ed: &Editor, pal: &Palette, id: Id, rect: Rect, origin: Pos2) {
        let (painter, page) = (ui.painter(), ed.page());
        if self.head.cover > 0.0 {
            let band = Rect::from_min_max(pos2(rect.left(), origin.y), pos2(rect.right(), origin.y + self.head.cover));
            cover(painter, band, &page.cover);
        }
        if let Some(icon) = self.head.icon.map(|icon| icon.translate(origin.to_vec2())) {
            emoji::paint(painter, icon.center(), ICON_SIZE, &page.icon, pal.fg);
        }
        let fade = |name, shown| ui.ctx().animate_bool_with_time(id.with(name), shown, FADE);
        let opacity = [fade("head-buttons", self.head_shown.0), fade("cover-buttons", self.head_shown.1)];
        for (button, r) in self.head_buttons(ui, ed, origin) {
            let mut painter = painter.clone();
            painter.set_opacity(opacity[usize::from(button.on_cover())]);
            let hot = self.head_hot == Some(button);
            // On the cover, buttons sit on a plate of the page's colour.
            if button.on_cover() || hot {
                painter.rect_filled(r, 4.0, if hot { pal.line } else { pal.bg });
            }
            let color = if hot { pal.fg } else { pal.muted };
            painter.text(r.center(), Align2::CENTER_CENTER, button.label(), FontId::proportional(13.5), color);
        }
    }

    /// Whether the emoji picker is open; its filter has the keyboard.
    fn picking_icon(&self) -> bool {
        self.picker.is_some_and(|(p, _)| p == Picker::Icon)
    }

    /// The icon or the cover picker: Remove, and a grid to choose from.
    /// The icon picker closes on a choice, the cover picker stays to try
    /// another.
    fn picker(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette, origin: Pos2) {
        let Some((which, fresh)) = self.picker.take() else { return };
        let (pivot, at) = match (which, self.head.icon) {
            (Picker::Icon, Some(icon)) => (Align2::LEFT_TOP, origin + icon.left_bottom().to_vec2() + vec2(0.0, 6.0)),
            (Picker::Cover, _) if self.head.cover > 0.0 => {
                (Align2::RIGHT_TOP, pos2(origin.x + self.width, origin.y + self.head.cover + 6.0))
            }
            _ => return,
        };
        let ctx = ui.ctx();
        let mut page = ed.page().clone();
        let mut close = false;
        let seed = (ctx.input(|i| i.time) * 1000.0) as usize;
        let shown = Area::new(Id::new(("page-picker", which)))
            .order(Order::Foreground)
            .pivot(pivot)
            .fixed_pos(at)
            .show(ctx, |ui| match which {
                Picker::Icon => {
                    egui::Frame::popup(ui.style()).inner_margin(0.0).show(ui, |ui| {
                        match self.emoji_picker.show(ui, &page.icon, pal, fresh, seed) {
                            Some(Choice::Pick(emoji)) => (page.icon, close) = (emoji, true),
                            Some(Choice::Random(emoji)) => page.icon = emoji,
                            Some(Choice::Remove) => (page.icon, close) = (String::new(), true),
                            None => {}
                        }
                    });
                }
                Picker::Cover => {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
                        if ui.button("Remove").clicked() {
                            (page.cover, close) = (String::new(), true);
                        }
                        for row in theme::COVERS.chunks(4) {
                            ui.horizontal(|ui| {
                                for &(name, ..) in row {
                                    let (rect, cell) = ui.allocate_exact_size(vec2(76.0, 44.0), Sense::click());
                                    let cell = cell.on_hover_cursor(CursorIcon::PointingHand);
                                    cover(ui.painter(), rect, name);
                                    let current = page.cover == name;
                                    if current || cell.hovered() {
                                        let stroke = Stroke::new(2.0, if current { pal.accent } else { pal.muted });
                                        ui.painter().rect_stroke(rect, 0.0, stroke, StrokeKind::Inside);
                                    }
                                    if cell.clicked() {
                                        page.cover = name.to_owned();
                                    }
                                }
                            });
                        }
                    });
                }
            });
        ed.set_page(page);
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        if !close && !escape && (fresh || !shown.response.clicked_elsewhere()) {
            self.picker = Some((which, false));
        }
    }

    // ---- block handles ----

    /// The add button and the drag handle of a block, left of its first line.
    fn handle_rects(&self, ed: &Editor, origin: Pos2, i: usize) -> (Rect, Rect) {
        let (b, row) = (&ed.blocks()[i], &self.rows[i]);
        let line = row.galley.rows.first().map_or(BODY, |r| r.row.size.y);
        let middle = match b.kind == Kind::Divider || row.typeset {
            true => row.top + row.height / 2.0,
            false => row.text.y + line / 2.0,
        };
        let right = origin.x + f32::from(b.indent) * STEP - HANDLE_GAP;
        let (top, bottom) = (origin.y + middle - HANDLE_HEIGHT / 2.0, origin.y + middle + HANDLE_HEIGHT / 2.0);
        let grip = Rect::from_min_max(pos2(right - GRIP_WIDTH, top), pos2(right, bottom));
        let add = Rect::from_min_max(pos2(grip.left() - ADD_WIDTH, top), pos2(grip.left(), bottom));
        (add, grip)
    }

    /// Shows the handles of the block under the pointer and acts on them:
    /// the add button opens a paragraph below, the handle selects the block
    /// on a click and moves it on a drag.
    fn handles(&mut self, ui: &Ui, ed: &mut Editor, id: Id, rect: Rect, origin: Pos2) {
        let (pointer, moved, down) =
            ui.input(|i| (i.pointer.latest_pos(), i.pointer.delta() != Vec2::ZERO, i.pointer.primary_down()));
        if moved {
            self.typing = false;
        }
        // A pressed button keeps the handles where they are.
        if !down {
            let free = !self.typing && self.menu.is_none() && !matches!(self.link, LinkUi::Edit { .. });
            self.handle = pointer.filter(|_| free && ui.rect_contains_pointer(rect)).and_then(|p| {
                let at = p - origin;
                let inside = (-MARGIN..=self.width).contains(&at.x) && (self.head.end..self.total).contains(&at.y);
                inside.then(|| self.rows.partition_point(|r| r.top + r.height <= at.y).min(self.rows.len() - 1))
            });
        }
        self.handle_hot = (false, false);
        let Some(i) = self.handle.filter(|&i| i < self.rows.len()) else {
            self.drag = None;
            return;
        };
        let (add, grip) = self.handle_rects(ed, origin, i);
        let add = ui.interact(add, id.with("add"), Sense::click());
        let grip = ui.interact(grip, id.with("grip"), Sense::click_and_drag());
        self.handle_hot = (add.hovered(), grip.hovered() || grip.dragged());

        if add.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        if add.clicked() {
            self.picked = None;
            ed.add_below(i);
            (self.h_pos, self.next_row, self.reveal) = (None, false, true);
        }

        if grip.drag_started() {
            // A handle inside a selection of several blocks takes them all.
            let (a, b) = match self.picked {
                Some((anchor, focus)) => (anchor.min(focus), anchor.max(focus)),
                None => (ed.selection().start().block, ed.selection().end().block),
            };
            let several = a != b || self.picked.is_some();
            let blocks = if several && (a..=b).contains(&i) { a..ed.subtree(b).end } else { ed.subtree(i) };
            self.drag = Some((blocks, i));
        }
        if grip.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.drag = None;
            }
            if let (Some((_, to)), Some(p)) = (&mut self.drag, pointer) {
                let y = p.y - origin.y;
                *to = self.rows.partition_point(|r| r.top + r.height / 2.0 < y);
                // Dragging at or past the top or bottom edge scrolls that way.
                let view = ui.clip_rect();
                let push = (view.top() + 24.0 - p.y).max(0.0) - (p.y - view.bottom() + 24.0).max(0.0);
                if push != 0.0 {
                    ui.scroll_with_delta(vec2(0.0, push.clamp(-40.0, 40.0)));
                    ui.ctx().request_repaint();
                }
            }
        } else if grip.drag_stopped() {
            if let Some((blocks, to)) = self.drag.take() {
                // The selection follows the blocks: it says where they went.
                let end = Pos::new(blocks.end - 1, 0);
                ed.set_selection(Selection { anchor: Pos::new(blocks.start, 0), focus: end });
                ed.move_range(blocks, to);
                let moved = ed.selection();
                self.pick(ed, moved.anchor.block, moved.focus.block);
                self.handle = None;
            }
        } else {
            self.drag = None;
            if grip.hovered() {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            }
            if grip.clicked() {
                // With Shift, from where the selection started up to here.
                let extend = self.picked.filter(|_| ui.input(|i| i.modifiers.shift));
                match extend {
                    Some((anchor, _)) if anchor > i => self.pick(ed, anchor, i),
                    Some((anchor, _)) => self.pick(ed, anchor, ed.subtree(i).end - 1),
                    None => self.pick(ed, i, ed.subtree(i).end - 1),
                }
            }
        }
    }

    fn paint_handles(&mut self, ui: &Ui, ed: &Editor, pal: &Palette, id: Id, origin: Pos2) {
        let painter = ui.painter();
        if let Some((blocks, to)) = &self.drag {
            // The blocks being moved fade, a bar marks where they would land
            // and the first of them follows the pointer.
            if let (Some(first), Some(last)) = (self.rows.get(blocks.start), self.rows.get(blocks.end - 1)) {
                let min = pos2(origin.x - HANDLE_GAP, origin.y + first.top);
                let max = pos2(origin.x + self.width, origin.y + last.top + last.height);
                painter.rect_filled(Rect::from_min_max(min, max), 0.0, pal.bg.gamma_multiply(0.6));
                let pointer = ui.input(|i| i.pointer.latest_pos());
                if let Some(p) = pointer.filter(|_| ed.blocks()[blocks.start].kind != Kind::Divider) {
                    let mut ghost = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, id.with("ghost")));
                    ghost.set_opacity(0.8);
                    let plate = Rect::from_min_size(p + vec2(16.0, -12.0), first.galley.size());
                    ghost.rect_filled(plate.expand2(vec2(8.0, 4.0)), CODE_RADIUS, pal.code_bg);
                    ghost.galley(plate.min + vec2(0.0, first.lead), first.galley.clone(), pal.fg);
                }
            }
            if !(blocks.start..=blocks.end).contains(to) {
                let y = origin.y + self.rows.get(*to).map_or(self.total, |r| r.top);
                let bar = Rect::from_min_max(pos2(origin.x, y - 1.5), pos2(origin.x + self.width, y + 1.5));
                painter.rect_filled(bar, 1.5, pal.accent.gamma_multiply(0.6));
            }
        }
        // Not while text is being selected with the mouse.
        let shown = self.handle.filter(|&i| i < self.rows.len() && ui.ctx().dragged_id() != Some(id));
        let opacity = ui.ctx().animate_bool_with_time(id.with("handles"), shown.is_some(), FADE);
        if let Some(i) = shown {
            self.handle_drawn = Some((i, ed.version()));
        }
        // Fading out, they stay on the block they were on.
        let still = self.handle_drawn.filter(|&(i, version)| version == ed.version() && i < self.rows.len());
        let Some((i, _)) = still.filter(|_| opacity > 0.0) else { return };
        let mut painter = painter.clone();
        painter.set_opacity(opacity);
        let (add, grip) = self.handle_rects(ed, origin, i);
        for (rect, hot) in [(add, self.handle_hot.0), (grip, self.handle_hot.1)] {
            if hot {
                painter.rect_filled(rect, 4.0, pal.line);
            }
        }
        let color = |hot| if hot { pal.muted } else { pal.faint };
        let (c, stroke) = (add.center(), Stroke::new(1.4, color(self.handle_hot.0)));
        painter.hline(c.x - 5.0..=c.x + 5.0, c.y, stroke);
        painter.vline(c.x, c.y - 5.0..=c.y + 5.0, stroke);
        for (dx, dy) in [(-2.5, -5.0), (2.5, -5.0), (-2.5, 0.0), (2.5, 0.0), (-2.5, 5.0), (2.5, 5.0)] {
            painter.circle_filled(grip.center() + vec2(dx, dy), 1.3, color(self.handle_hot.1));
        }
    }

    /// The link whose text is under this point: its extent and target.
    fn link_under(&self, ed: &Editor, at: Vec2) -> Option<(Selection, Rc<str>)> {
        let i = self.rows.partition_point(|r| r.top + r.height <= at.y);
        let row = self.rows.get(i)?;
        let local = at - row.text;
        if !row.galley.rows.iter().any(|r| r.rect().contains(local.to_pos2())) {
            return None;
        }
        let offset = byte_offset(&ed.blocks()[i], row.galley.cursor_from_pos(local));
        ed.link_span_at(Pos::new(i, offset))
    }

    /// Sets the mouse cursor, and starts or keeps the popup of a link the
    /// pointer rests on.
    fn hover(&mut self, ui: &Ui, ed: &Editor, response: &Response, origin: Pos2) {
        let at = response.hover_pos().map(|p| p - origin);
        let busy = self.menu.is_some() || ui.input(|i| i.pointer.primary_down());
        let over = at.filter(|_| !busy).and_then(|at| self.link_under(ed, at));
        if let Some(at) = at {
            let checkbox = at.y < self.total && self.checkbox_at(ed, at).is_some();
            // The text cursor only over lines that hold text.
            let i = self.rows.partition_point(|r| r.top + r.height <= at.y);
            let on_text = self.rows.get(i).is_some_and(|row| {
                let line = row.text.x..=self.width;
                at.y >= row.top && line.contains(&at.x) && ed.blocks()[i].kind != Kind::Divider
            });
            let on_formula = on_text && self.rows[i].typeset;
            if self.on_option || self.handle_hot != (false, false) {
                // The button under the pointer has set the cursor.
            } else if over.is_some() || checkbox || on_formula {
                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
            } else if on_text {
                ui.ctx().set_cursor_icon(CursorIcon::Text);
            }
        }
        let now = ui.input(|i| i.time);
        let shown = match &self.link {
            LinkUi::Edit { .. } => return,
            LinkUi::Hover { span, since } => Some((*span, *since, false)),
            LinkUi::View { span, .. } => Some((*span, now, true)),
            LinkUi::Closed => None,
        };
        self.on_link = matches!((&over, shown), (Some((span, _)), Some((s, ..))) if *span == s);
        match (over, shown) {
            (Some(_), Some((_, _, true))) if self.on_link => {}
            (Some((span, url)), Some((_, since, false))) if self.on_link => {
                match HOVER_SHOW - (now - since) {
                    left if left > 0.0 => ui.ctx().request_repaint_after_secs(left as f32),
                    _ => self.link = LinkUi::View { span, url, away: None, copied: None },
                }
            }
            (Some((span, _)), _) => {
                self.link = LinkUi::Hover { span, since: now };
                ui.ctx().request_repaint_after_secs(HOVER_SHOW as f32);
            }
            // A shown popup closes itself once the pointer has been away.
            (None, Some((.., true))) => {}
            (None, _) => self.link = LinkUi::Closed,
        }
    }

    /// Opens the link editor for the selection, or for the link at the caret.
    fn edit_link(&mut self, ed: &Editor) {
        let (span, url) = if ed.has_rich_selection() {
            (ed.selection(), ed.link())
        } else if let Some((span, url)) = ed.link_at_caret() {
            (span, Some(url))
        } else {
            return;
        };
        let input = url.as_deref().unwrap_or_default().to_owned();
        self.link = LinkUi::Edit { span, input, had_link: url.is_some(), fresh: true };
    }

    /// Fills the text between two positions, as a selection is drawn.
    fn highlight(&self, painter: &Painter, ed: &Editor, origin: Pos2, range: Selection, color: Color32) {
        let (start, end) = (range.start(), range.end());
        let clip = painter.clip_rect();
        for i in start.block..=end.block {
            let (b, row) = (&ed.blocks()[i], &self.rows[i]);
            let visible = origin.y + row.top <= clip.bottom() && origin.y + row.top + row.height >= clip.top();
            if !visible || b.kind == Kind::Divider {
                continue;
            }
            if let Some((rect, _)) = row.formula.filter(|_| row.typeset) {
                painter.rect_filled(rect.translate(origin.to_vec2()).expand(4.0), 3.0, color);
                continue;
            }
            let painter = &self.text_painter(painter, b, row, origin);
            let text = origin + row.text;
            let from = if i == start.block { start.offset } else { 0 };
            let to = if i == end.block { end.offset } else { b.text.len() };
            let mut first = ccursor(b, from);
            first.prefer_next_row = true;
            let a = row.galley.pos_from_cursor(first);
            let z = row.galley.pos_from_cursor(ccursor(b, to));
            // One piece per line, as tall as the letters rather than the line.
            let height = theme::metrics(b.kind).0 * 1.2;
            for line in &row.galley.rows {
                let line = line.rect();
                if line.bottom() <= a.top() + 1.0 || line.top() >= z.bottom() - 1.0 {
                    continue;
                }
                let last = line.bottom() >= z.bottom() - 1.0;
                let left = if line.top() <= a.top() + 1.0 { a.left() } else { line.left() };
                // A line break inside the selection is shown as a little extra width.
                let right = match last {
                    true => z.right() + if i < end.block { 6.0 } else { 0.0 },
                    false => line.right().max(left + 6.0),
                };
                if right > left {
                    let piece = Rect::from_min_max(pos2(left, line.top()), pos2(right, line.bottom()));
                    let piece = Rect::from_center_size(piece.center(), vec2(piece.width(), height));
                    painter.rect_filled(piece.translate(text.to_vec2()), 0.0, color);
                }
            }
        }
    }

    /// Index of the to-do block whose checkbox is at this point.
    fn checkbox_at(&self, ed: &Editor, at: Vec2) -> Option<usize> {
        let i = self.rows.partition_point(|r| r.top + r.height <= at.y).min(self.rows.len() - 1);
        let b = &ed.blocks()[i];
        let left = f32::from(b.indent) * STEP;
        (b.kind == Kind::Todo && (left..left + STEP).contains(&at.x)).then_some(i)
    }

    // ---- painting ----

    fn paint(&mut self, ui: &Ui, ed: &Editor, pal: &Palette, origin: Pos2) {
        let painter = ui.painter();
        let clip = ui.clip_rect();
        let blocks = ed.blocks();
        let sel = ed.selection();
        let column = Rect::from_min_size(origin, vec2(self.width, self.total));

        let first = self.rows.partition_point(|r| origin.y + r.top + r.height < clip.top());
        for (i, row) in self.rows.iter().enumerate().skip(first) {
            if origin.y + row.top > clip.bottom() {
                break;
            }
            let b = &blocks[i];
            let text = origin + row.text;
            let left = origin.x + f32::from(b.indent) * STEP;
            let line = row.galley.rows.first().map_or(BODY, |r| r.row.size.y);
            let size = row.galley.size();
            match b.kind {
                Kind::Bullet => {
                    // Level with the middle of the small letters.
                    let middle = text.y + line / 2.0 + BODY * STRIKE_DROP;
                    painter.circle_filled(pos2(left + STEP / 2.0, middle), 2.6, pal.fg);
                }
                Kind::Numbered => {
                    let at = pos2(left + STEP - 6.0, text.y + line / 2.0);
                    let label = format!("{}.", row.number);
                    painter.text(at, Align2::RIGHT_CENTER, label, FontId::proportional(BODY), pal.fg);
                }
                Kind::Todo => {
                    let side = 16.0;
                    let corner = pos2(left + 3.0, text.y + (line - side) / 2.0);
                    let rect = Rect::from_min_size(corner, vec2(side, side));
                    if b.checked {
                        painter.rect_filled(rect, 3.0, pal.accent);
                        let c = rect.center();
                        let tick = [c + vec2(-4.0, 0.2), c + vec2(-1.3, 3.0), c + vec2(4.2, -3.2)];
                        painter.line(tick.to_vec(), Stroke::new(1.8, egui::Color32::WHITE));
                    } else {
                        painter.rect_stroke(rect, 3.0, Stroke::new(1.5, pal.muted), StrokeKind::Inside);
                    }
                }
                Kind::Quote => {
                    let bar = Rect::from_min_max(pos2(left, text.y), pos2(left + 3.0, text.y + size.y));
                    painter.rect_filled(bar, 0.0, pal.fg);
                }
                Kind::Math if row.typeset => {
                    // Under the pointer, a formula shows it can be clicked;
                    // being edited, which one is.
                    let edited = self.math.as_ref().is_some_and(|m| m.block == b.id && !m.closed);
                    let lit = edited || (self.handle == Some(i) && self.drag.is_none());
                    let fade = ui.ctx().animate_bool_with_time(ui.id().with(("formula", b.id)), lit, FADE);
                    if fade > 0.0 {
                        let min = pos2(left, origin.y + row.top + 4.0);
                        let max = pos2(column.right(), origin.y + row.top + row.height - 4.0);
                        let color = pal.code_bg.gamma_multiply(0.6 * fade);
                        painter.rect_filled(Rect::from_min_max(min, max), CODE_RADIUS, color);
                    }
                }
                Kind::Code | Kind::Math => {
                    let min = pos2(left, text.y - CODE_PAD.y);
                    let max = pos2(column.right(), text.y + size.y + CODE_PAD.y);
                    painter.rect_filled(Rect::from_min_max(min, max), CODE_RADIUS, pal.code_bg);
                }
                Kind::Divider => {
                    let y = origin.y + row.top + row.height / 2.0;
                    painter.hline(left..=column.right(), y, Stroke::new(1.0, pal.line));
                }
                _ => {}
            }

            if b.kind.is_rich() {
                color_backgrounds(painter, b, &row.galley, text, pal);
                code_backgrounds(painter, b, &row.galley, text, pal);
            }
            if b.kind != Kind::Divider && !row.typeset {
                let painter = self.text_painter(painter, b, row, origin);
                painter.galley(text + vec2(0.0, row.lead), row.galley.clone(), pal.fg);
            }
            if let Some((rect, size)) = row.formula {
                math::paint(painter, origin + rect.min.to_vec2(), self.shown_tex(b), size, pal.fg);
            } else if b.kind == Kind::Math && !b.text.is_empty() {
                if let Err(error) = math::measure(ui.ctx(), b.text.as_str(), MATH_SIZE) {
                    let at = pos2(text.x, text.y + size.y + CODE_PAD.y + MATH_ERROR / 2.0);
                    let painter = self.text_painter(painter, b, row, origin);
                    painter.text(at, Align2::LEFT_CENTER, error, FontId::proportional(12.5), pal.code_fg);
                }
            }
            if b.kind.is_rich() {
                text_lines(painter, b, &row.galley, text, pal);
            }
            let caret_here = sel.is_collapsed() && sel.focus.block == i && self.picked.is_none();
            if b.text.is_empty() && b.kind == Kind::Math {
                let at = pos2(text.x, text.y + line / 2.0);
                painter.text(at, Align2::LEFT_CENTER, "Add a TeX equation", FontId::proportional(line * 0.62), pal.muted);
            } else if b.text.is_empty() && caret_here && self.preedit.is_empty() {
                let hint = match b.kind {
                    Kind::Paragraph => "Type '/' for commands",
                    Kind::H1 => "Heading 1",
                    Kind::H2 => "Heading 2",
                    Kind::H3 => "Heading 3",
                    Kind::Bullet | Kind::Numbered => "List",
                    Kind::Todo => "To-do",
                    Kind::Quote => "Quote",
                    _ => "",
                };
                let at = pos2(text.x, text.y + line / 2.0);
                painter.text(at, Align2::LEFT_CENTER, hint, FontId::proportional(line * 0.62), pal.faint);
            }
        }

        // The highlight of blocks selected as blocks fades in and out, each
        // block on its own.
        let picked = self.picked.map_or(0..0, |(anchor, focus)| anchor.min(focus)..anchor.max(focus) + 1);
        for (i, row) in self.rows.iter().enumerate().skip(first) {
            if origin.y + row.top > clip.bottom() {
                break;
            }
            let b = &blocks[i];
            let fade = ui.ctx().animate_bool_with_time(ui.id().with(("picked", b.id)), picked.contains(&i), FADE);
            if fade == 0.0 {
                continue;
            }
            // The whole block with the space that belongs to it, less a
            // small gap to its neighbours.
            let (top, bottom) = match b.kind {
                Kind::Code => (row.text.y - CODE_PAD.y, row.text.y + row.galley.size().y + CODE_PAD.y),
                _ => (row.top + 2.0, row.top + row.height - 2.0),
            };
            let min = origin + vec2(f32::from(b.indent) * STEP - 4.0, top);
            let max = origin + vec2(self.width + 4.0, bottom);
            painter.rect_filled(Rect::from_min_max(min, max), 3.0, pal.picked.gamma_multiply(fade));
        }
        if let Some((from, to)) = self.marquee.filter(|(from, to)| (*to - *from).length() > 3.0) {
            painter.rect_filled(Rect::from_two_pos(origin + from, origin + to), 2.0, pal.select);
        }

        // Drawn over the text: inline code has a background of its own that
        // would hide a selection painted beneath it.
        match &self.link {
            LinkUi::Edit { span, .. } => self.highlight(painter, ed, origin, *span, pal.select),
            _ if !sel.is_collapsed() => self.highlight(painter, ed, origin, sel, pal.select),
            _ => {}
        }

        let caret = self.caret(ed, sel.focus, self.next_row).translate(origin.to_vec2());
        // As tall as the letters rather than the line, and blinking while
        // the window has focus and nothing changes.
        let (now, focused) = ui.input(|i| (i.time, i.focused));
        let state = Some((sel, ed.version()));
        if self.caret_state != state {
            self.caret_state = state;
            self.caret_since = now;
        }
        let phase = (now - self.caret_since) % (2.0 * BLINK);
        if focused {
            ui.ctx().request_repaint_after_secs((BLINK - phase % BLINK) as f32);
        }
        let elsewhere = matches!(self.link, LinkUi::Edit { .. }) || self.editing_title || self.picking_icon() || self.editing_math();
        if focused && phase < BLINK && self.picked.is_none() && !elsewhere {
            let height = theme::metrics(blocks[sel.focus.block].kind).0 * 1.2;
            let bar = Rect::from_center_size(pos2(caret.left(), caret.center().y), vec2(1.0, height));
            let (b, row) = (&blocks[sel.focus.block], &self.rows[sel.focus.block]);
            self.text_painter(painter, b, row, origin).rect_filled(bar, 0.0, pal.fg);
        }
        if !self.preedit.is_empty() {
            // Shown at the caret over what follows; not laid out into the text.
            let font = FontId::proportional(BODY);
            let shown = painter.text(caret.left_center(), Align2::LEFT_CENTER, &self.preedit, font, pal.accent);
            painter.rect_filled(shown, 0.0, pal.bg);
            painter.text(caret.left_center(), Align2::LEFT_CENTER, &self.preedit, FontId::proportional(BODY), pal.accent);
            painter.hline(shown.x_range(), shown.bottom(), Stroke::new(1.0, pal.accent));
        }
        // Tell the platform where the caret is, so an input method can
        // place its candidate window next to it. The title tells it its own.
        ui.output_mut(|o| {
            if self.editing_title || self.picking_icon() || self.editing_math() {
                return;
            }
            o.ime = Some(egui::output::IMEOutput {
                purpose: egui::IMEPurpose::Normal,
                rect: column,
                cursor_rect: caret,
                should_interrupt_composition: false,
            });
        });
        if std::mem::take(&mut self.reveal) {
            ui.scroll_to_rect(caret.expand2(vec2(0.0, 32.0)), None);
        }
    }

    // ---- slash menu, formatting toolbar and link popup ----

    /// Opens the menu for a `/` just typed at the start of a word.
    fn open_menu(&mut self, ed: &Editor) {
        let p = ed.selection().focus;
        let b = &ed.blocks()[p.block];
        if !b.kind.is_rich() || p.offset == 0 {
            return;
        }
        let at = p.offset - 1;
        if b.text.as_str()[..at].ends_with(|c: char| !c.is_whitespace()) {
            return;
        }
        self.menu = Some(Menu { at: Pos::new(p.block, at), items: Vec::new(), active: 0 });
        self.update_menu(ed);
    }

    /// Refilters the menu from the text after the `/`, closing it once the
    /// caret leaves that text or nothing matches.
    fn update_menu(&mut self, ed: &Editor) {
        let Some(menu) = &mut self.menu else { return };
        let s = ed.selection();
        let query = ed.blocks().get(menu.at.block).and_then(|b| {
            let inside = s.is_collapsed() && s.focus.block == menu.at.block;
            let q = b.text.as_str().get(menu.at.offset..s.focus.offset)?.strip_prefix('/')?;
            (inside && !q.contains(char::is_whitespace)).then(|| q.to_lowercase())
        });
        menu.items = match &query {
            Some(q) => (0..ITEMS.len())
                .filter(|&i| ITEMS[i].0.to_lowercase().contains(q) || ITEMS[i].2.contains(q))
                .collect(),
            None => Vec::new(),
        };
        if menu.items.is_empty() {
            self.menu = None;
        } else {
            menu.active = menu.active.min(menu.items.len() - 1);
        }
    }

    /// Handles a key while the menu is open; true when the key was used.
    fn menu_key(&mut self, ed: &mut Editor, key: Key) -> bool {
        let Some(menu) = &mut self.menu else { return false };
        let n = menu.items.len();
        match key {
            Key::ArrowDown => menu.active = (menu.active + 1) % n,
            Key::ArrowUp => menu.active = (menu.active + n - 1) % n,
            Key::Enter | Key::Tab => {
                let item = menu.items[menu.active];
                self.choose(ed, item);
            }
            Key::Escape => self.menu = None,
            _ => return false,
        }
        true
    }

    fn choose(&mut self, ed: &mut Editor, item: usize) {
        if let Some(menu) = self.menu.take() {
            ed.run_slash(menu.at, ITEMS[item].3);
            self.reveal = true;
        }
    }

    fn popups(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette, origin: Pos2) {
        self.language_menu(ui, ed, pal, origin);
        self.picker(ui, ed, pal, origin);
        self.math_popup(ui, ed, pal, origin);
        if self.editing_title {
            return;
        }
        let ctx = ui.ctx();
        let sel = ed.selection();
        if let Some(menu) = &self.menu {
            let caret = self.caret(ed, sel.focus, self.next_row).translate(origin.to_vec2());
            let row = vec2(240.0, 28.0);
            // Below the caret, or above it when the window ends first.
            let height = row.y * menu.items.len() as f32 + 16.0;
            let below = caret.bottom() + 6.0;
            let top = match below + height > ctx.content_rect().bottom() {
                true => (caret.top() - 6.0 - height).max(0.0),
                false => below,
            };
            let mut chosen = None;
            Area::new(Id::new("slash-menu"))
                .order(Order::Foreground)
                .fixed_pos(pos2(caret.left(), top))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (n, &i) in menu.items.iter().enumerate() {
                            let (label, hint, ..) = ITEMS[i];
                            let (rect, item) = ui.allocate_exact_size(row, Sense::click());
                            let item = item.on_hover_cursor(CursorIcon::PointingHand);
                            let visuals = ui.visuals();
                            if n == menu.active || item.hovered() {
                                ui.painter().rect_filled(rect, 5.0, visuals.widgets.hovered.weak_bg_fill);
                            }
                            let (left, right) = (rect.left_center() + vec2(10.0, 0.0), rect.right_center() - vec2(10.0, 0.0));
                            let painter = ui.painter();
                            painter.text(left, Align2::LEFT_CENTER, label, FontId::proportional(14.0), visuals.text_color());
                            painter.text(right, Align2::RIGHT_CENTER, hint, FontId::monospace(12.0), visuals.weak_text_color());
                            if item.clicked() {
                                chosen = Some(i);
                            }
                        }
                    });
                });
            if let Some(item) = chosen {
                self.choose(ed, item);
            }
            return;
        }

        if !matches!(self.link, LinkUi::Closed | LinkUi::Hover { .. }) {
            return self.link_popup(ui, ed, pal, origin);
        }

        let selecting = ctx.input(|i| i.pointer.primary_down()) && ctx.dragged_id().is_some();
        if selecting || !ed.has_rich_selection() {
            return;
        }
        let anchor = self.caret(ed, sel.start(), true).translate(origin.to_vec2());
        let marks = ed.active_marks();
        let (mut toggled, mut link) = (None, false);
        Area::new(Id::new("format-bar"))
            .order(Order::Foreground)
            .pivot(Align2::LEFT_BOTTOM)
            .fixed_pos(anchor.left_top() - vec2(0.0, 8.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).inner_margin(4.0).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for &(m, ref tool) in &MARK_TOOLS {
                            if tool_button(ui, pal, tool, marks & m != 0) {
                                toggled = Some(m);
                            }
                        }
                        ui.separator();
                        self.colors.button(ui, ed, pal);
                        link = tool_button(ui, pal, &LINK_TOOL, ed.link().is_some());
                    });
                });
            });
        if let Some(m) = toggled {
            ed.toggle_mark(m);
        }
        if link {
            self.edit_link(ed);
        }
    }

    /// Whether the math editor is open; it has the keyboard.
    fn editing_math(&self) -> bool {
        self.math.as_ref().is_some_and(|m| !m.closed)
    }

    /// Opens the editor of math block `i`, with the document's caret
    /// parked after its TeX.
    fn open_math(&mut self, ed: &mut Editor, i: usize) {
        let b = &ed.blocks()[i];
        let input = b.text.as_str().to_owned();
        self.math = Some(MathEdit { block: b.id, shown: input.clone(), input, fresh: true, closed: false });
        (self.picked, self.menu, self.link) = (None, None, LinkUi::Closed);
        ed.set_selection(Selection::caret(Pos::new(i, b.text.len())));
        self.math_seen = Some((ed.selection(), Kind::Math));
    }

    /// Opens the math editor when the caret comes to rest in a math block,
    /// by a click, a shortcut or the slash menu, and closes it when its
    /// block goes away.
    fn follow_math(&mut self, ed: &mut Editor) {
        let sel = ed.selection();
        let b = &ed.blocks()[sel.focus.block];
        let seen = Some((sel, b.kind));
        let moved = std::mem::replace(&mut self.math_seen, seen) != seen;
        let inside = sel.anchor.block == sel.focus.block && self.picked.is_none() && !self.editing_title;
        let open = self.math.as_ref().filter(|m| !m.closed).map(|m| m.block);
        if moved && inside && b.kind == Kind::Math && open != Some(b.id) {
            self.open_math(ed, sel.focus.block);
        }
        if let Some(m) = self.math.as_mut().filter(|m| !m.closed) {
            m.closed = !ed.blocks().iter().any(|b| b.id == m.block && b.kind == Kind::Math);
        }
    }

    /// The math editor: the TeX in a field under its block, Done, and what
    /// is wrong with the TeX when it cannot be read. It fades in and out.
    fn math_popup(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette, origin: Pos2) {
        let ctx = ui.ctx();
        let open = self.editing_math();
        let opacity = ctx.animate_bool_with_time(Id::new("math-editor"), open, FADE);
        let Some(m) = self.math.as_mut() else { return };
        let i = ed.blocks().iter().position(|b| b.id == m.block).filter(|&i| i < self.rows.len());
        let Some(i) = i.filter(|_| open || opacity > 0.0) else {
            self.math = None;
            return;
        };
        let (b, row) = (&ed.blocks()[i], &self.rows[i]);
        let indent = f32::from(b.indent) * STEP;
        let at = origin + vec2(indent, row.top + row.height + 2.0);
        let block = Rect::from_min_size(origin + vec2(indent, row.top), vec2(self.width - indent, row.height));
        let width = (self.width - indent).min(MATH_EDITOR_WIDTH);
        let error = match m.input.trim().is_empty() {
            true => None,
            false => math::measure(ctx, &m.input, MATH_SIZE).err(),
        };
        let opened = std::mem::take(&mut m.fresh);
        let mut done = false;
        let area = Area::new(Id::new("math-editor")).order(Order::Foreground).fixed_pos(at).interactable(open);
        let shown = area.show(ctx, |ui| {
            ui.set_opacity(opacity);
            egui::Frame::popup(ui.style()).inner_margin(4.0).show(ui, |ui| {
                ui.set_width(width);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let field = egui::TextEdit::multiline(&mut m.input)
                        .font(FontId::monospace(13.5))
                        .hint_text(r"TeX, like e^{i\pi} = -1")
                        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6)))
                        .desired_rows(1)
                        .desired_width(width - TOOL - 2.0)
                        .return_key(Some(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter)));
                    ui.visuals_mut().text_cursor.stroke.color = Color32::TRANSPARENT;
                    let field = field.show(ui);
                    field_caret(ui, &field, pal.fg);
                    let field = field.response.response;
                    // It keeps the keyboard while open, even from the click
                    // that opened it.
                    if open && !field.has_focus() {
                        field.request_focus();
                    }
                    if field.changed() {
                        replace_tex(ed, i, &m.input);
                        if m.input.trim().is_empty() || math::measure(ui.ctx(), &m.input, MATH_SIZE).is_ok() {
                            m.shown.clone_from(&m.input);
                        }
                    }
                    // Enter is done, unless it is the one that opened it;
                    // Shift+Enter breaks the line.
                    let enter = !opened && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
                    done = tool_button(ui, pal, &DONE_TOOL, false) || enter;
                });
                if let Some(error) = &error {
                    let text = egui::RichText::new(error).size(12.5).color(pal.code_fg);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.add(egui::Label::new(text).wrap());
                    });
                    ui.add_space(4.0);
                }
            });
        });
        if !open {
            return;
        }
        let escape = ctx.input(|i| i.key_pressed(Key::Escape));
        let on_block = ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| block.contains(p));
        let away = !opened && shown.response.clicked_elsewhere() && !on_block;
        if done || escape || away {
            m.closed = true;
        }
        if done {
            // On to the text after the formula.
            match ed.blocks().get(i + 1).filter(|next| next.kind.is_rich()) {
                Some(_) => ed.set_selection(Selection::caret(Pos::new(i + 1, 0))),
                None => ed.add_below(i),
            }
            self.reveal = true;
        } else if escape {
            self.pick(ed, i, i);
        }
    }

    fn link_popup(&mut self, ui: &Ui, ed: &mut Editor, pal: &Palette, origin: Pos2) {
        let ctx = ui.ctx();
        let now = ctx.input(|i| i.time);
        let span = match &self.link {
            LinkUi::View { span, .. } | LinkUi::Edit { span, .. } => *span,
            _ => return,
        };
        // Under the first line of the linked text.
        let anchor = self.caret(ed, span.start(), true).translate(origin.to_vec2());
        let area = Area::new(Id::new("link")).order(Order::Foreground).fixed_pos(anchor.left_bottom() + vec2(0.0, 6.0));
        // What the popup asks for: the address to apply (`None` removes the
        // link), to start editing, or to close.
        let (mut apply, mut edit, mut close) = (None, false, false);
        match &mut self.link {
            LinkUi::View { url, away, copied, .. } => {
                let shown = area.show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).inner_margin(4.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let (rect, _) = ui.allocate_exact_size(vec2(24.0, TOOL), Sense::hover());
                            globe(ui.painter(), rect.center(), pal.muted);
                            let mut text: String = url.chars().take(48).collect();
                            if text.len() < url.len() {
                                text.push('…');
                            }
                            let text = egui::RichText::new(text).size(12.5).color(pal.muted);
                            ui.add(egui::Label::new(text).selectable(false));
                            ui.add_space(6.0);
                            let open = openable(url);
                            if open.is_some() && tool_button(ui, pal, &OPEN_TOOL, false) {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(open.unwrap_or_default()));
                            }
                            let done = copied.is_some_and(|at| now - at < COPIED);
                            if tool_button(ui, pal, if done { &COPIED_TOOL } else { &COPY_TOOL }, false) {
                                ui.ctx().copy_text(url.to_string());
                                *copied = Some(now);
                                ui.ctx().request_repaint_after_secs(COPIED as f32);
                            }
                            edit = tool_button(ui, pal, &EDIT_TOOL, false);
                            if tool_button(ui, pal, &UNLINK_TOOL, false) {
                                apply = Some(None);
                            }
                        });
                    });
                });
                if self.on_link || shown.response.contains_pointer() {
                    *away = None;
                } else {
                    let left = HOVER_HIDE - (now - *away.get_or_insert(now));
                    close = left <= 0.0;
                    ctx.request_repaint_after_secs(left.max(0.0) as f32);
                }
            }
            LinkUi::Edit { input, had_link, fresh, .. } => {
                let opened = std::mem::take(fresh);
                let shown = area.show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).inner_margin(4.0).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let (rect, _) = ui.allocate_exact_size(vec2(24.0, TOOL), Sense::hover());
                            globe(ui.painter(), rect.center(), pal.muted);
                            let field = egui::TextEdit::singleline(input)
                                .hint_text("Paste or type a link")
                                .frame(egui::Frame::NONE)
                                .desired_width(240.0);
                            ui.visuals_mut().text_cursor.stroke.color = Color32::TRANSPARENT;
                            let field = field.show(ui);
                            field_caret(ui, &field, pal.fg);
                            let field = field.response.response;
                            if opened {
                                field.request_focus();
                            }
                            ui.add_space(6.0);
                            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                            if tool_button(ui, pal, &APPLY_TOOL, false) || enter {
                                apply = Some(Some(input.trim().to_owned()).filter(|url| !url.is_empty()));
                            }
                            if *had_link && tool_button(ui, pal, &UNLINK_TOOL, false) {
                                apply = Some(None);
                            }
                        });
                    });
                });
                let escape = ctx.input(|i| i.key_pressed(Key::Escape));
                close = escape || (!opened && shown.response.clicked_elsewhere());
            }
            _ => {}
        }
        if let Some(url) = apply {
            ed.set_selection(span);
            ed.set_link(url.as_deref());
            let end = span.end();
            ed.set_selection(Selection { anchor: end, focus: end });
            self.link = LinkUi::Closed;
        } else if edit {
            let LinkUi::View { url, .. } = std::mem::take(&mut self.link) else { return };
            self.link = LinkUi::Edit { span, input: url.to_string(), had_link: true, fresh: true };
        } else if close {
            self.link = LinkUi::Closed;
        }
    }
}

/// Puts `input` in place of math block `i`'s TeX, replacing only what
/// differs, so that it is undone as the typing it was.
fn replace_tex(ed: &mut Editor, i: usize, input: &str) {
    let old = ed.blocks()[i].text.as_str();
    let mut start = old.bytes().zip(input.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(start) {
        start -= 1;
    }
    let (rest, new) = (&old[start..], &input[start..]);
    let mut end = rest.bytes().rev().zip(new.bytes().rev()).take_while(|(a, b)| a == b).count();
    while !rest.is_char_boundary(rest.len() - end) {
        end -= 1;
    }
    let range = start..old.len() - end;
    ed.replace_text(i, range, &new[..new.len() - end]);
}

/// Calls `draw` with each piece of line that a run of the block's text
/// accepted by `wanted` covers: one rectangle per line the run is on, as
/// tall as the line, in the galley's coordinates.
fn run_pieces(b: &Block, galley: &Galley, wanted: impl Fn(&Attrs) -> bool, mut draw: impl FnMut(Rect, &Attrs)) {
    let mut at = 0;
    for (s, attrs) in b.text.runs() {
        let (from, to) = (at, at + s.len());
        at = to;
        if wanted(attrs) {
            pieces(b, galley, from..to, |piece| draw(piece, attrs));
        }
    }
}

/// Calls `draw` with each piece of line that the text in `range` covers,
/// as `run_pieces` does for a run.
fn pieces(b: &Block, galley: &Galley, range: Range<usize>, mut draw: impl FnMut(Rect)) {
    let mut start = ccursor(b, range.start);
    start.prefer_next_row = true;
    let a = galley.pos_from_cursor(start);
    let z = galley.pos_from_cursor(ccursor(b, range.end));
    for line in &galley.rows {
        let line = line.rect();
        if line.bottom() <= a.top() + 1.0 || line.top() >= z.bottom() - 1.0 {
            continue;
        }
        let left = if line.top() <= a.top() + 1.0 { a.left() } else { line.left() };
        let right = if line.bottom() >= z.bottom() - 1.0 { z.right() } else { line.right() };
        if right > left {
            draw(Rect::from_min_max(pos2(left, line.top()), pos2(right, line.bottom())));
        }
    }
}

/// A piece of line cut down to the height of the letters, and widened by
/// `pad` on each side.
fn letters(b: &Block, piece: Rect, text: Pos2, pad: f32) -> Rect {
    let half = theme::metrics(b.kind).0 * 0.6;
    let min = pos2(piece.left() - pad, piece.center().y - half);
    let max = pos2(piece.right() + pad, piece.center().y + half);
    Rect::from_min_max(min, max).translate(text.to_vec2())
}

/// Backgrounds given to a block's text: a plain fill behind the letters,
/// square and no wider than them so that it never reads as inline code;
/// one for each stretch of text with the same background, however its
/// other formatting changes.
fn color_backgrounds(painter: &Painter, b: &Block, galley: &Galley, text: Pos2, pal: &Palette) {
    let runs: Vec<(usize, u8)> = b.text.raw_runs().iter().map(|r| (r.len, r.attrs.bg)).collect();
    let mut at = 0;
    for stretch in runs.chunk_by(|x, y| x.1 == y.1) {
        let (len, bg) = (stretch.iter().map(|r| r.0).sum::<usize>(), stretch[0].1);
        if bg != 0 {
            let color = pal.back[usize::from(bg)];
            pieces(b, galley, at..at + len, |piece| {
                painter.rect_filled(letters(b, piece, text, 0.0), 0.0, color);
            });
        }
        at += len;
    }
}

/// Rounded backgrounds behind a block's inline code. Code given a
/// background is filled with it and outlined, to stand out as code from
/// the background around it.
fn code_backgrounds(painter: &Painter, b: &Block, galley: &Galley, text: Pos2, pal: &Palette) {
    run_pieces(b, galley, |attrs| attrs.marks & mark::CODE != 0, |piece, attrs| {
        let chip = letters(b, piece, text, 2.5);
        if attrs.bg == 0 {
            painter.rect_filled(chip, INLINE_CODE_RADIUS, pal.code_bg);
        } else {
            painter.rect_filled(chip, INLINE_CODE_RADIUS, pal.back[usize::from(attrs.bg)]);
            painter.rect_stroke(chip, INLINE_CODE_RADIUS, Stroke::new(1.0, pal.line), StrokeKind::Inside);
        }
    });
}

/// Underlines (of links and underlined text) and strike-through (of struck
/// text and done to-dos), placed by the letters.
fn text_lines(painter: &Painter, b: &Block, galley: &Galley, text: Pos2, pal: &Palette) {
    let size = theme::metrics(b.kind).0;
    let done = b.kind == Kind::Todo && b.checked;
    let under = |attrs: &Attrs| attrs.marks & mark::UNDERLINE != 0 || attrs.link.is_some();
    let through = |attrs: &Attrs| done || attrs.marks & mark::STRIKE != 0;
    run_pieces(b, galley, |attrs| under(attrs) || through(attrs), |piece, attrs| {
        let stroke = Stroke::new((size / 16.0).max(1.0), theme::color(b, attrs, pal));
        let (across, middle) = (text.x + piece.left()..=text.x + piece.right(), text.y + piece.center().y);
        if under(attrs) {
            painter.hline(across.clone(), middle + size * UNDERLINE_DROP, stroke);
        }
        if through(attrs) {
            painter.hline(across, middle + size * STRIKE_DROP, stroke);
        }
    });
}

/// A cover: one colour, or a gradient from its top left corner to its
/// bottom right one.
fn cover(painter: &Painter, rect: Rect, name: &str) {
    let (a, b) = theme::cover(name);
    let middle = a.lerp_to_gamma(b, 0.5);
    let mut mesh = egui::Mesh::default();
    for (at, color) in [(rect.left_top(), a), (rect.right_top(), middle), (rect.right_bottom(), b), (rect.left_bottom(), middle)] {
        mesh.colored_vertex(at, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// A small globe: the icon in front of a link's address.
fn globe(painter: &Painter, c: Pos2, color: Color32) {
    let (r, stroke) = (6.0, Stroke::new(1.0, color));
    painter.circle_stroke(c, r, stroke);
    painter.add(egui::Shape::ellipse_stroke(c, vec2(r * 0.42, r), stroke));
    painter.hline(c.x - r..=c.x + r, c.y, stroke);
}

/// What a code block's language button says: the language it is set to,
/// or the one its text is guessed to be in.
fn language_name(b: &Block) -> &'static str {
    match LANGUAGES.get(usize::from(b.lang)) {
        Some(language) if b.lang != 0 => language.name,
        _ => notely_core::highlight::detected(b.text.as_str()),
    }
}

/// Lines of text, the middle one turning back under itself.
fn wrap_icon(painter: &Painter, c: Pos2, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    painter.hline(c.x - 5.5..=c.x + 5.5, c.y - 4.0, stroke);
    painter.hline(c.x - 5.5..=c.x - 2.0, c.y + 4.0, stroke);
    let turn = [(-5.5, 0.0), (4.0, 0.0), (5.5, 1.0), (5.5, 3.0), (4.0, 4.0), (1.0, 4.0)];
    painter.line(turn.iter().map(|&(x, y)| c + vec2(x, y)).collect(), stroke);
    painter.line(vec![c + vec2(2.8, 2.2), c + vec2(1.0, 4.0), c + vec2(2.8, 5.8)], stroke);
}

/// Two overlapping sheets, or a tick once the link has been copied.
fn copy_icon(painter: &Painter, c: Pos2, done: bool, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    if done {
        let tick = [c + vec2(-4.5, 0.3), c + vec2(-1.5, 3.3), c + vec2(4.5, -3.3)];
        painter.line(tick.to_vec(), stroke);
        return;
    }
    let sheet = |at: Pos2| Rect::from_center_size(at, vec2(7.5, 9.0));
    painter.rect_stroke(sheet(c + vec2(-1.5, -1.5)), 1.5, stroke, StrokeKind::Middle);
    painter.rect_stroke(sheet(c + vec2(1.5, 1.5)), 1.5, stroke, StrokeKind::Middle);
}

/// A square button of the formatting toolbar or the link popup, tinted while `on`, with a
/// tooltip above it. Returns whether it was clicked.
fn tool_button(ui: &mut Ui, pal: &Palette, tool: &Tool, on: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(TOOL, TOOL), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 5.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    if on {
        painter.rect_filled(rect, 5.0, pal.accent.gamma_multiply(0.16));
    }
    let c = pos2(painter.round_to_pixel_center(rect.center().x), painter.round_to_pixel_center(rect.center().y));
    let stroke = Stroke::new(1.4, if on { pal.accent } else { pal.fg });
    (tool.icon)(&Icon { painter, c, stroke, turn: 0.0 });
    tool_tip(&response, pal, tool.name, tool.keys.as_ref());
    response.clicked()
}

/// A toolbar button's tooltip: its name and shortcut, above it.
pub fn tool_tip(response: &Response, pal: &Palette, name: &str, keys: Option<&KeyboardShortcut>) {
    let mut tip = egui::Tooltip::for_enabled(response);
    tip.popup = tip.popup.align(RectAlign::TOP).gap(6.0);
    tip.show(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(name).color(pal.fg));
            if let Some(keys) = keys {
                ui.label(egui::RichText::new(ui.ctx().format_shortcut(keys)).color(pal.muted));
            }
        });
    });
}

/// A text field's caret, drawn like the document's: a pixel wide, just
/// after the boundary between two letters, blinking while nothing changes.
/// egui's own caret, two pixels across the boundary, sits half on the
/// letter before and reads as left of the gap, so the field hides it.
fn field_caret(ui: &Ui, field: &egui::text_edit::TextEditOutput, color: Color32) {
    let Some(range) = field.cursor_range.filter(|_| field.response.response.has_focus()) else { return };
    let now = ui.input(|i| i.time);
    let id = field.response.response.id.with("caret");
    let since = ui.data_mut(|d| {
        let (seen, since) = d.get_temp_mut_or(id, (range, now));
        if *seen != range || field.response.response.changed() {
            (*seen, *since) = (range, now);
        }
        *since
    });
    let phase = (now - since) % (2.0 * BLINK);
    if !ui.input(|i| i.focused) {
        return;
    }
    ui.ctx().request_repaint_after_secs((BLINK - phase % BLINK) as f32);
    if phase < BLINK {
        let nudge = ink_nudge(&field.galley, range.primary);
        let shift = field.galley_pos.to_vec2() + vec2(nudge - field.galley.rect.left(), 0.0);
        let at = field.galley.pos_from_cursor(range.primary).translate(shift);
        let bar = Rect::from_min_size(at.left_top(), vec2(1.0, at.height()));
        ui.painter().with_clip_rect(field.text_clip_rect.expand(1.0)).rect_filled(bar, 0.0, color);
    }
}

/// How far right of the boundary at `cursor` a caret goes. Kerning can
/// tuck a glyph under the one before it ("r,"), and some letters reach
/// past their width ("t."), which puts the boundary inside the first
/// one's shape: the caret moves halfway out, to read as between the two.
fn ink_nudge(galley: &Galley, cursor: CCursor) -> f32 {
    let at = galley.layout_from_cursor(cursor);
    let glyphs = galley.rows.get(at.row).map_or(&[][..], |r| &r.row.glyphs[..]);
    match (at.column.0.checked_sub(1).and_then(|c| glyphs.get(c)), glyphs.get(at.column.0)) {
        (Some(prev), Some(next)) => {
            let ink_right = prev.pos.x + prev.uv_rect.offset.x + prev.uv_rect.size.x;
            ((ink_right - next.pos.x) / 2.0).max(0.0)
        }
        _ => 0.0,
    }
}

/// Lines of a toolbar icon, given on a 24 unit grid (as Lucide's icons are)
/// centred on `c` and turned clockwise by `turn` around it.
#[derive(Clone, Copy)]
struct Icon<'a> {
    painter: &'a Painter,
    c: Pos2,
    stroke: Stroke,
    turn: f32,
}

impl Icon<'_> {
    /// A line through points of the grid.
    fn line(&self, points: impl IntoIterator<Item = (f32, f32)>) {
        let (sin, cos) = (self.turn * std::f32::consts::TAU).sin_cos();
        let at = |(x, y): (f32, f32)| {
            let (x, y) = (x - 12.0, y - 12.0);
            self.c + vec2(x * cos - y * sin, x * sin + y * cos) * 0.68
        };
        self.painter.line(points.into_iter().map(at).collect(), self.stroke);
    }
}

/// Points of the grid on a circle around `c`, from turn `from` to turn
/// `to`, counted clockwise from the right.
fn arc(c: (f32, f32), r: f32, from: f32, to: f32) -> impl Iterator<Item = (f32, f32)> {
    let steps = (r * (to - from).abs() * 12.0).ceil().max(2.0) as usize;
    (0..=steps).map(move |i| {
        let (sin, cos) = ((from + (to - from) * i as f32 / steps as f32) * std::f32::consts::TAU).sin_cos();
        (c.0 + r * cos, c.1 + r * sin)
    })
}

/// Rounded corners of a box, each a quarter circle of radius 2 around the
/// given point, starting at the given turn.
fn corners<const N: usize>(at: [(f32, f32, f32); N]) -> impl Iterator<Item = (f32, f32)> {
    at.into_iter().flat_map(|(x, y, from)| arc((x, y), 2.0, from, from + 0.25))
}

/// A B, in heavier lines than the other icons: an upright stroke with two
/// bowls.
fn bold_icon(icon: &Icon) {
    let icon = Icon { stroke: Stroke::new(icon.stroke.width * 1.4, icon.stroke.color), ..*icon };
    let lower = [(6.0, 12.0)].into_iter().chain(arc((15.0, 16.0), 4.0, -0.25, 0.25));
    let upper = [(6.0, 20.0), (6.0, 4.0)].into_iter().chain(arc((14.0, 8.0), 4.0, -0.25, 0.25));
    icon.line(lower.chain(upper));
}

/// A slanted I with serifs.
fn italic_icon(icon: &Icon) {
    icon.line([(10.0, 4.0), (19.0, 4.0)]);
    icon.line([(5.0, 20.0), (14.0, 20.0)]);
    icon.line([(15.0, 4.0), (9.0, 20.0)]);
}

/// A U over a line.
fn underline_icon(icon: &Icon) {
    icon.line([(6.0, 4.0)].into_iter().chain(arc((12.0, 10.0), 6.0, 0.5, 0.0)).chain([(18.0, 4.0)]));
    icon.line([(4.0, 20.0), (20.0, 20.0)]);
}

/// An S struck through.
fn strike_icon(icon: &Icon) {
    icon.line([(16.0, 4.0)].into_iter().chain(arc((9.0, 7.0), 3.0, -0.25, -0.554)));
    icon.line(arc((14.0, 16.0), 4.0, -0.25, 0.25).chain([(6.0, 20.0)]));
    icon.line([(4.0, 12.0), (20.0, 12.0)]);
}

/// Angle brackets around a slash: `</>`.
fn code_icon(icon: &Icon) {
    icon.line([(6.0, 8.0), (2.0, 12.0), (6.0, 16.0)]);
    icon.line([(18.0, 8.0), (22.0, 12.0), (18.0, 16.0)]);
    icon.line([(14.5, 4.0), (9.5, 20.0)]);
}

/// Two links of a chain joined by a bar, rising to the right.
fn link_icon(icon: &Icon) {
    let icon = Icon { turn: -0.125, ..*icon };
    icon.line([(9.0, 17.0)].into_iter().chain(arc((7.0, 12.0), 5.0, 0.25, 0.75)).chain([(9.0, 7.0)]));
    icon.line([(15.0, 7.0)].into_iter().chain(arc((17.0, 12.0), 5.0, -0.25, 0.25)).chain([(15.0, 17.0)]));
    icon.line([(8.0, 12.0), (16.0, 12.0)]);
}

/// An arrow leaving a box.
fn open_icon(icon: &Icon) {
    icon.line([(15.0, 3.0), (21.0, 3.0), (21.0, 9.0)]);
    icon.line([(10.0, 14.0), (21.0, 3.0)]);
    let corners = corners([(16.0, 19.0, 0.0), (5.0, 19.0, 0.25), (5.0, 8.0, 0.5)]);
    icon.line([(18.0, 13.0)].into_iter().chain(corners).chain([(11.0, 6.0)]));
}

/// Two sheets, one in front of the other.
fn sheets_icon(icon: &Icon) {
    icon.line(corners([(4.0, 14.0, 0.25), (4.0, 4.0, 0.5), (14.0, 4.0, 0.75)]));
    let front = corners([(10.0, 10.0, 0.5), (20.0, 10.0, 0.75), (20.0, 20.0, 0.0), (10.0, 20.0, 0.25)]);
    icon.line(front.chain([(8.0, 10.0)]));
}

/// A tick.
fn tick_icon(icon: &Icon) {
    icon.line([(4.0, 12.0), (9.0, 17.0), (20.0, 6.0)]);
}

/// A pencil, writing down to the left.
fn pencil_icon(icon: &Icon) {
    let icon = Icon { turn: -0.125, ..*icon };
    let body = arc((19.0, 12.0), 4.0, -0.25, 0.25).chain([(8.0, 16.0), (1.5, 12.0), (8.0, 8.0), (19.0, 8.0)]);
    icon.line(body);
    icon.line([(15.0, 8.0), (15.0, 16.0)]);
}

/// The two links of a chain apart, cut between.
fn unlink_icon(icon: &Icon) {
    let icon = Icon { turn: -0.125, ..*icon };
    icon.line([(9.0, 17.0)].into_iter().chain(arc((7.0, 12.0), 5.0, 0.25, 0.75)).chain([(9.0, 7.0)]));
    icon.line([(15.0, 7.0)].into_iter().chain(arc((17.0, 12.0), 5.0, -0.25, 0.25)).chain([(15.0, 17.0)]));
    icon.line([(12.0, 4.0), (12.0, 20.0)]);
}
