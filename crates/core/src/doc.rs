use crate::text::RichText;

pub const MAX_INDENT: u8 = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Paragraph,
    H1,
    H2,
    H3,
    Bullet,
    Numbered,
    Todo,
    Quote,
    Code,
    Divider,
}

impl Kind {
    pub fn is_list(self) -> bool {
        matches!(self, Kind::Bullet | Kind::Numbered | Kind::Todo)
    }

    /// Blocks whose text can carry inline formatting.
    pub fn is_rich(self) -> bool {
        !matches!(self, Kind::Code | Kind::Divider)
    }
}

/// What a page has besides its blocks: a title, an icon (an emoji) and a
/// cover (the name of one of the app's covers). Each is empty when not set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub title: String,
    pub icon: String,
    pub cover: String,
}

/// The document is a flat list of blocks; nesting is expressed by `indent`.
#[derive(Clone, Debug)]
pub struct Block {
    /// Stable identity, used by the view to key DOM nodes.
    pub id: u32,
    /// Bumped on every change, so the view can skip untouched blocks.
    pub rev: u32,
    pub kind: Kind,
    pub checked: bool,
    pub indent: u8,
    /// Language of a code block: an index into `highlight::LANGUAGES`.
    pub lang: u8,
    /// A code block whose long lines are cut off rather than wrapped.
    pub nowrap: bool,
    pub text: RichText,
}

impl Block {
    pub fn new(kind: Kind, text: RichText) -> Self {
        Block { id: 0, rev: 0, kind, checked: false, indent: 0, lang: 0, nowrap: false, text }
    }

    /// Whether two blocks hold the same thing, whichever blocks they are.
    pub fn same_content(&self, other: &Block) -> bool {
        (self.kind, self.checked, self.indent, self.lang, self.nowrap)
            == (other.kind, other.checked, other.indent, other.lang, other.nowrap)
            && self.text == other.text
    }
}

/// Block index plus byte offset into its text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Pos {
    pub block: usize,
    pub offset: usize,
}

impl Pos {
    pub fn new(block: usize, offset: usize) -> Self {
        Pos { block, offset }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Selection {
    pub anchor: Pos,
    pub focus: Pos,
}

impl Selection {
    pub fn caret(pos: Pos) -> Self {
        Selection { anchor: pos, focus: pos }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.focus
    }

    pub fn start(&self) -> Pos {
        self.anchor.min(self.focus)
    }

    pub fn end(&self) -> Pos {
        self.anchor.max(self.focus)
    }
}
