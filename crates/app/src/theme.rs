//! Colours, fonts and text styles.

use std::sync::Arc;

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId};
use notely_core::highlight::{highlight, Token};
use notely_core::{mark, Attrs, Block, Kind};

/// Width of one indent level and of the list marker gutter.
pub const STEP: f32 = 26.0;
pub const BODY: f32 = 16.0;

const BOLD: &str = "bold";
const ITALIC: &str = "italic";
const BOLD_ITALIC: &str = "bold-italic";
/// Only egui's emoji fonts, for page icons: Inter and Ubuntu draw some of
/// the symbols differently.
const EMOJI: &str = "emoji";

/// Size of a page's title.
pub const TITLE: f32 = 40.0;

/// The covers a page can have, by the name they are saved under: the
/// colour of their top left corner and of their bottom right one.
pub const COVERS: [(&str, Color32, Color32); 12] = [
    ("red", hex(0xe0_6c_5f), hex(0xe0_6c_5f)),
    ("yellow", hex(0xf2_c4_5a), hex(0xf2_c4_5a)),
    ("green", hex(0x6f_b3_8a), hex(0x6f_b3_8a)),
    ("blue", hex(0x5b_8f_d9), hex(0x5b_8f_d9)),
    ("purple", hex(0x9a_7a_d1), hex(0x9a_7a_d1)),
    ("gray", hex(0x9b_9a_97), hex(0x9b_9a_97)),
    ("sunset", hex(0xff_9a_8b), hex(0xff_6a_88)),
    ("peach", hex(0xff_ec_d2), hex(0xfc_b6_9f)),
    ("mint", hex(0xd4_fc_79), hex(0x96_e6_a1)),
    ("lavender", hex(0xe0_c3_fc), hex(0x8e_c5_fc)),
    ("ocean", hex(0x2e_31_92), hex(0x1b_ff_ff)),
    ("dusk", hex(0x4b_6c_b7), hex(0x18_28_48)),
];

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// The corner colours of a cover; a cover this version does not know is grey.
pub fn cover(name: &str) -> (Color32, Color32) {
    let grey = COVERS[5];
    let (_, a, b) = COVERS.iter().find(|(n, ..)| *n == name).unwrap_or(&grey);
    (*a, *b)
}

pub fn title_font() -> FontId {
    FontId::new(TITLE, FontFamily::Name(BOLD.into()))
}

pub fn emoji_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(EMOJI.into()))
}

pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub fg: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    /// Background of code blocks and of inline code.
    pub code_bg: Color32,
    pub code_fg: Color32,
    pub accent: Color32,
    pub select: Color32,
    /// Laid over blocks that are selected as blocks.
    pub picked: Color32,
    /// Colours of code: comments, strings, numbers, keywords, types and
    /// called names.
    pub syntax: [Color32; 6],
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        let rgb = Color32::from_rgb;
        if dark {
            Palette {
                dark,
                bg: rgb(0x19, 0x19, 0x19),
                fg: rgb(0xe3, 0xe2, 0xde),
                muted: rgb(0x8a, 0x88, 0x82),
                faint: rgb(0x5c, 0x5a, 0x55),
                line: rgb(0x2e, 0x2e, 0x2c),
                code_bg: rgb(0x26, 0x26, 0x26),
                code_fg: rgb(0xcf, 0x51, 0x48),
                accent: rgb(0x5a, 0x9c, 0xf0),
                select: Color32::from_rgba_unmultiplied(35, 131, 226, 51),
                // Gives #1a2735 on the background.
                picked: Color32::from_rgba_unmultiplied(35, 131, 226, 35),
                syntax: [
                    rgb(0x7a, 0x7e, 0x85),
                    rgb(0x6a, 0xab, 0x73),
                    rgb(0x2a, 0xac, 0xb8),
                    rgb(0xcf, 0x8e, 0x6d),
                    rgb(0xc7, 0x7d, 0xbb),
                    rgb(0x56, 0xa8, 0xf5),
                ],
            }
        } else {
            Palette {
                dark,
                bg: rgb(0xff, 0xff, 0xff),
                fg: rgb(0x2b, 0x2a, 0x27),
                muted: rgb(0x8f, 0x8d, 0x86),
                faint: rgb(0xb9, 0xb7, 0xb0),
                line: rgb(0xe7, 0xe5, 0xdf),
                code_bg: rgb(0xf2, 0xf1, 0xee),
                code_fg: rgb(0xcf, 0x51, 0x48),
                accent: rgb(0x2f, 0x7d, 0xe1),
                select: Color32::from_rgba_unmultiplied(35, 131, 226, 51),
                picked: Color32::from_rgba_unmultiplied(35, 131, 226, 35),
                syntax: [
                    rgb(0x8c, 0x8c, 0x8c),
                    rgb(0x06, 0x7d, 0x17),
                    rgb(0x17, 0x50, 0xeb),
                    rgb(0x00, 0x33, 0xb3),
                    rgb(0x87, 0x10, 0x94),
                    rgb(0x00, 0x62, 0x7a),
                ],
            }
        }
    }
}

/// Adds the bundled Inter faces in front of egui's own fonts, which stay
/// as fallback for monospace and emoji. egui cannot embolden or slant a
/// face well, so bold and italic are separate families.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let faces: [(&str, &'static [u8], FontFamily); 4] = [
        ("regular", include_bytes!("../fonts/Inter-Regular.ttf"), FontFamily::Proportional),
        (BOLD, include_bytes!("../fonts/Inter-Bold.ttf"), FontFamily::Name(BOLD.into())),
        (ITALIC, include_bytes!("../fonts/Inter-Italic.ttf"), FontFamily::Name(ITALIC.into())),
        (
            BOLD_ITALIC,
            include_bytes!("../fonts/Inter-BoldItalic.ttf"),
            FontFamily::Name(BOLD_ITALIC.into()),
        ),
    ];
    // egui's monospace face sits high next to Inter; nudge it down so
    // inline code shares the baseline of the text around it.
    if let Some(mono) = fonts.font_data.get_mut("Hack") {
        Arc::make_mut(mono).tweak.y_offset_factor = 0.16;
    }
    for (name, bytes, family) in faces {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        let mut list = vec![name.to_owned()];
        list.extend(fallback.iter().cloned());
        fonts.families.insert(family, list);
    }
    let emoji = ["NotoEmoji-Regular", "emoji-icon-font"].map(str::to_owned);
    fonts.families.insert(FontFamily::Name(EMOJI.into()), emoji.to_vec());
    ctx.set_fonts(fonts);
}

/// Font size and line height of a block's text.
pub fn metrics(kind: Kind) -> (f32, f32) {
    match kind {
        Kind::H1 => (30.0, 38.0),
        Kind::H2 => (24.0, 32.0),
        Kind::H3 => (20.0, 28.0),
        Kind::Code => (13.5, 21.5),
        _ => (BODY, 25.5),
    }
}

/// Colour of a run of a block's text.
pub fn color(b: &Block, attrs: &Attrs, pal: &Palette) -> Color32 {
    if attrs.marks & mark::CODE != 0 && b.kind != Kind::Code {
        pal.code_fg
    } else if (b.kind == Kind::Todo && b.checked) || attrs.link.is_some() {
        pal.muted
    } else {
        pal.fg
    }
}

/// How far the view draws a block's text below where egui lays it out.
///
/// A line is taller than its letters, and egui puts all of the extra height
/// below them. Shifted down by half of it, the text sits in the middle of
/// its line, so whatever is placed by a line's middle (list markers, the
/// caret, the selection, the handles) is level with the text.
pub fn lead(ui: &egui::Ui, kind: Kind) -> f32 {
    let (size, line_height) = metrics(kind);
    let family = if kind == Kind::Code { FontFamily::Monospace } else { FontFamily::Proportional };
    let letters = ui.fonts_mut(|f| f.row_height(&FontId::new(size, family)));
    let pixels = ui.pixels_per_point();
    ((line_height - letters) / 2.0 * pixels).round() / pixels
}

/// The text format of a run. Underlines and strike-through are left out:
/// egui draws them by the line's height rather than by the letters, so the
/// view draws its own.
fn format(b: &Block, attrs: &Attrs, pal: &Palette) -> TextFormat {
    let (size, line_height) = metrics(b.kind);
    let heading = matches!(b.kind, Kind::H1 | Kind::H2 | Kind::H3);
    let has = |m: u8| attrs.marks & m != 0;
    let inline_code = has(mark::CODE) && b.kind != Kind::Code;
    let family = if inline_code || b.kind == Kind::Code {
        FontFamily::Monospace
    } else {
        match (heading || has(mark::BOLD), has(mark::ITALIC)) {
            (true, true) => FontFamily::Name(BOLD_ITALIC.into()),
            (true, false) => FontFamily::Name(BOLD.into()),
            (false, true) => FontFamily::Name(ITALIC.into()),
            (false, false) => FontFamily::Proportional,
        }
    };
    let color = color(b, attrs, pal);
    TextFormat {
        font_id: FontId::new(if inline_code { size * 0.86 } else { size }, family),
        line_height: Some(line_height),
        color,
        ..Default::default()
    }
}

/// The text of a block as an egui layout job: one section per formatting
/// run, or per piece of syntax in a code block.
pub fn job(b: &Block, width: f32, pal: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    if b.kind == Kind::Code && !b.text.is_empty() {
        let (text, mut at) = (b.text.as_str(), 0);
        for (len, token) in highlight(text, b.lang) {
            let mut format = format(b, &Attrs::default(), pal);
            format.color = match token {
                Token::Plain => pal.fg,
                Token::Comment => pal.syntax[0],
                Token::String => pal.syntax[1],
                Token::Number => pal.syntax[2],
                Token::Keyword => pal.syntax[3],
                Token::Type => pal.syntax[4],
                Token::Function => pal.syntax[5],
            };
            job.append(&text[at..at + len], 0.0, format);
            at += len;
        }
        return job;
    }
    if b.text.is_empty() {
        // An empty section still gives the line its height.
        job.append("", 0.0, format(b, &Attrs::default(), pal));
    }
    for (s, attrs) in b.text.runs() {
        job.append(s, 0.0, format(b, attrs, pal));
    }
    job
}
