//! Emoji, drawn by Twemoji.
//!
//! `emoji/emoji.txt` lists the emoji the picker offers and
//! `emoji/twemoji.br` holds their images as SVG (`tools/twemoji.py` builds
//! both). The images are unpacked the first time an emoji is drawn, and
//! each is rendered at the exact pixel size it is drawn at, so it is sharp
//! at any size and on any screen. Text Twemoji has no image for is drawn by
//! the emoji font instead.

pub mod picker;

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{Align2, Color32, ColorImage, Context, Id, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use eframe::emath::GuiRounding;

use crate::theme;

const LIST: &str = include_str!("../../emoji/emoji.txt");
const IMAGES: &[u8] = include_bytes!("../../emoji/twemoji.br");
/// Images rendered on one frame at most; the rest wait for the next ones,
/// so scrolling through the picker never stalls.
const RENDERS_PER_FRAME: usize = 24;
/// Textures kept beyond those in use, and how many frames an unused one
/// stays.
const KEEP: usize = 400;
const UNUSED_FRAMES: u64 = 120;

/// A picker category: how it is named in `emoji.txt`, its heading, and the
/// symbol of its button.
pub struct Category {
    pub key: &'static str,
    pub title: &'static str,
    pub symbol: &'static str,
}

pub const CATEGORIES: [Category; 8] = [
    Category { key: "people", title: "People", symbol: "☺" },
    Category { key: "nature", title: "Nature", symbol: "🌿" },
    Category { key: "food", title: "Food", symbol: "🍔" },
    Category { key: "activity", title: "Activity", symbol: "⚽" },
    Category { key: "travel", title: "Travel", symbol: "✈" },
    Category { key: "objects", title: "Objects", symbol: "💡" },
    Category { key: "symbols", title: "Symbols", symbol: "♥" },
    Category { key: "flags", title: "Flags", symbol: "🚩" },
];

pub struct Emoji {
    /// Index into [`CATEGORIES`].
    pub category: usize,
    pub text: &'static str,
    pub name: &'static str,
    /// More words it is found by, space separated.
    pub words: &'static str,
    /// The emoji in each skin tone, light to dark, if it has them.
    pub skins: Option<[&'static str; 5]>,
}

impl Emoji {
    /// The emoji in a skin tone: 0 is the default yellow, 1 to 5 light to
    /// dark.
    pub fn in_tone(&self, tone: usize) -> &'static str {
        match (self.skins, tone) {
            (Some(skins), 1..=5) => skins[tone - 1],
            _ => self.text,
        }
    }

    /// Whether every word of `query` starts a word of its name or of its
    /// other words.
    pub fn matches(&self, query: &str) -> bool {
        let words = || self.name.split([' ', ':', '-']).chain(self.words.split(' '));
        query.split_whitespace().all(|q| words().any(|w| w.to_lowercase().starts_with(q)))
    }
}

/// Every emoji the picker offers, in order.
pub fn all() -> &'static [Emoji] {
    static ALL: OnceLock<Vec<Emoji>> = OnceLock::new();
    ALL.get_or_init(|| {
        LIST.lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let category = fields.next()?;
                let category = CATEGORIES.iter().position(|c| c.key == category)?;
                let (text, name, words) = (fields.next()?, fields.next()?, fields.next().unwrap_or(""));
                let skins: Vec<&str> = fields.collect();
                let skins = <[&str; 5]>::try_from(skins).ok();
                Some(Emoji { category, text, name, words, skins })
            })
            .collect()
    })
}

/// Text compared without the emoji variation selector, which only asks
/// for what Twemoji draws anyway.
fn key(text: &str) -> String {
    text.replace('\u{fe0f}', "")
}

/// The SVG of every emoji Twemoji draws, by [`key`].
fn svgs() -> &'static HashMap<String, &'static str> {
    static SVGS: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    SVGS.get_or_init(|| {
        let mut text = String::new();
        if brotli_decompressor::Decompressor::new(IMAGES, 1 << 16).read_to_string(&mut text).is_err() {
            return HashMap::new();
        }
        let text: &'static str = text.leak();
        text.lines().filter_map(|line| line.split_once('\t')).map(|(emoji, svg)| (key(emoji), svg)).collect()
    })
}

#[derive(Default)]
struct Textures {
    by_size: HashMap<(String, u32), (TextureHandle, u64)>,
    pass: u64,
    rendered: usize,
}

/// Draws `text` as a square of side `size` centred on `center`; `color` is
/// for the font, should Twemoji not draw it.
pub fn paint(painter: &Painter, center: Pos2, size: f32, text: &str, color: Color32) {
    let ctx = painter.ctx();
    let Some(&svg) = svgs().get(&key(text)) else {
        // The font's glyphs are drawn smaller than their size.
        painter.text(center, Align2::CENTER_CENTER, text, theme::emoji_font(size / 0.9), color);
        return;
    };
    let ppp = ctx.pixels_per_point();
    let pixels = (size * ppp).round().max(1.0) as u32;
    let textures = textures(ctx);
    let mut guard = textures.lock().unwrap_or_else(|e| e.into_inner());
    let textures = &mut *guard;
    let pass = ctx.cumulative_pass_nr();
    if textures.pass != pass {
        textures.pass = pass;
        textures.rendered = 0;
        if textures.by_size.len() > KEEP {
            textures.by_size.retain(|_, (_, used)| pass - *used < UNUSED_FRAMES);
        }
    }
    let name = (key(text), pixels);
    let texture = match textures.by_size.get_mut(&name) {
        Some((texture, used)) => {
            *used = pass;
            texture.id()
        }
        None if textures.rendered >= RENDERS_PER_FRAME => {
            ctx.request_repaint();
            return;
        }
        None => {
            textures.rendered += 1;
            let image = render(svg, pixels).unwrap_or_else(|| ColorImage::filled([1, 1], Color32::TRANSPARENT));
            let texture = ctx.load_texture(format!("twemoji-{}-{pixels}", name.0), image, TextureOptions::LINEAR);
            let id = texture.id();
            textures.by_size.insert(name, (texture, pass));
            id
        }
    };
    // Whole pixels, so the image's pixels are the screen's.
    let side = pixels as f32 / ppp;
    let min = (center - Vec2::splat(side / 2.0)).round_to_pixels(ppp);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    painter.image(texture, Rect::from_min_size(min, Vec2::splat(side)), uv, Color32::WHITE);
}

fn textures(ctx: &Context) -> Arc<Mutex<Textures>> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Arc<Mutex<Textures>>>(Id::new("twemoji")).clone())
}

fn render(svg: &str, pixels: u32) -> Option<ColorImage> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(pixels, pixels)?;
    let size = tree.size();
    let scale = pixels as f32 / size.width().max(size.height());
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let side = pixels as usize;
    Some(ColorImage::from_rgba_premultiplied([side, side], pixmap.data()))
}

/// An emoji at random, for "Add icon".
pub fn random(seed: usize) -> &'static str {
    let all = all();
    all[seed % all.len()].text
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_has_emoji() {
        for (n, category) in CATEGORIES.iter().enumerate() {
            assert!(all().iter().any(|e| e.category == n), "{} is empty", category.title);
        }
    }

    #[test]
    fn every_emoji_has_an_image() {
        for e in all() {
            for text in std::iter::once(e.text).chain(e.skins.into_iter().flatten()) {
                assert!(svgs().contains_key(&key(text)), "{text} ({}) has no image", e.name);
            }
        }
    }

    #[test]
    fn images_render() {
        let image = render(svgs()[&key("👋")], 48).expect("renders");
        assert_eq!(image.size, [48, 48]);
        assert!(image.pixels.iter().any(|p| p.a() > 0));
    }
}
