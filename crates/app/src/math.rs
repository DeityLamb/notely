//! Formulas: TeX typeset by RaTeX, a Rust port of KaTeX's layout, and drawn
//! from the outlines of the KaTeX fonts it lays them out for.
//!
//! `math/katex.br` holds the fonts (`tools/katex.py` builds it); they are
//! unpacked the first time a formula is drawn. A formula is laid out once
//! and rendered at the exact pixel size and colour it is drawn at, so it is
//! sharp at any size and on any screen.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{Color32, ColorImage, Context, Id, Painter, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use eframe::emath::GuiRounding;
use ratex_font::FontId;
use ratex_layout::{layout, to_display_list, LayoutOptions};
use ratex_types::display_item::{DisplayItem, DisplayList};
use ratex_types::path_command::PathCommand;
use resvg::tiny_skia::{self, FillRule, Paint, PathBuilder, Pixmap, Transform};

const FONTS: &[u8] = include_bytes!("../math/katex.br");
/// Formulas rendered on one frame at most; the rest wait for the next ones.
const RENDERS_PER_FRAME: usize = 8;
/// Layouts and textures kept beyond those in use, and how many frames an
/// unused one stays.
const KEEP: usize = 64;
const UNUSED_FRAMES: u64 = 120;
/// Room around a formula's box for ink that reaches past it (the tail of an
/// italic f, a tall integral), in font sizes.
const BLEED: f32 = 0.2;
/// Longest side of a rendered formula, in pixels.
const MAX_SIDE: f32 = 4096.0;

/// The extent of a formula drawn at `size`, in points, or why its TeX
/// could not be read.
pub fn measure(ctx: &Context, tex: &str, size: f32) -> Result<Vec2, String> {
    let list = cache(ctx).lock().unwrap_or_else(|e| e.into_inner()).list(tex, ctx.cumulative_pass_nr())?;
    Ok(Vec2::new(list.width as f32, (list.height + list.depth) as f32) * size)
}

/// Draws a formula with the top left of its box at `min`. The formula's
/// own colour, black, is drawn in `color`, the text colour of the theme;
/// colours it sets with `\color` stay.
pub fn paint(painter: &Painter, min: Pos2, tex: &str, size: f32, color: Color32) {
    let ctx = painter.ctx();
    let ppp = ctx.pixels_per_point();
    let em = size * ppp;
    let cache = cache(ctx);
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    let cache = &mut *guard;
    let pass = ctx.cumulative_pass_nr();
    if cache.pass != pass {
        cache.pass = pass;
        cache.rendered = 0;
        if cache.textures.len() > KEEP {
            cache.textures.retain(|_, (_, used)| pass - *used < UNUSED_FRAMES);
        }
        if cache.lists.len() > KEEP {
            cache.lists.retain(|_, (_, used)| pass - *used < UNUSED_FRAMES);
        }
    }
    let Ok(list) = cache.list(tex, pass) else { return };
    let key = (hash(tex), em.to_bits(), color);
    let (texture, pad) = match cache.textures.get_mut(&key) {
        Some((texture, used)) => {
            *used = pass;
            texture.clone()
        }
        None if cache.rendered >= RENDERS_PER_FRAME => {
            ctx.request_repaint();
            return;
        }
        None => {
            cache.rendered += 1;
            let (image, pad) = render(&list, em, color);
            let texture = ctx.load_texture(format!("formula-{}", key.0), image, TextureOptions::LINEAR);
            cache.textures.insert(key, ((texture.clone(), pad), pass));
            (texture, pad)
        }
    };
    // Whole pixels, so the image's pixels are the screen's.
    let size = texture.size_vec2() / ppp;
    let min = (min - Vec2::splat(pad / ppp)).round_to_pixels(ppp);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    painter.image(texture.id(), Rect::from_min_size(min, size), uv, Color32::WHITE);
}

type Layout = Result<Arc<DisplayList>, String>;
/// A rendered formula and the pixels left around its box.
type Rendered = (TextureHandle, f32);

#[derive(Default)]
struct Cache {
    /// Formulas laid out, in font sizes, by their TeX.
    lists: HashMap<String, (Layout, u64)>,
    /// Rendered formulas by the hash of their TeX, pixels per font size
    /// and colour.
    textures: HashMap<(u64, u32, Color32), (Rendered, u64)>,
    pass: u64,
    rendered: usize,
}

impl Cache {
    fn list(&mut self, tex: &str, pass: u64) -> Layout {
        if let Some((list, used)) = self.lists.get_mut(tex) {
            *used = pass;
            return list.clone();
        }
        let list = typeset(tex);
        self.lists.insert(tex.to_owned(), (list.clone(), pass));
        list
    }
}

fn cache(ctx: &Context) -> Arc<Mutex<Cache>> {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Arc<Mutex<Cache>>>(Id::new("formulas")).clone())
}

fn hash(tex: &str) -> u64 {
    let mut h = DefaultHasher::new();
    tex.hash(&mut h);
    h.finish()
}

/// Lays a formula out in display style, in font sizes with y downward and
/// the baseline at `height`.
fn typeset(tex: &str) -> Layout {
    let ast = ratex_parser::parse(tex).map_err(|e| e.message)?;
    let list = to_display_list(&layout(&ast, &LayoutOptions::default()));
    Ok(Arc::new(list))
}

/// The KaTeX fonts, by the name RaTeX gives them.
fn fonts() -> &'static HashMap<FontId, ttf_parser::Face<'static>> {
    static FACES: OnceLock<HashMap<FontId, ttf_parser::Face<'static>>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut data = Vec::new();
        if brotli_decompressor::Decompressor::new(FONTS, 1 << 16).read_to_end(&mut data).is_err() {
            return HashMap::new();
        }
        let data: &'static [u8] = data.leak();
        let Some(end) = data.iter().position(|&b| b == b'\n') else { return HashMap::new() };
        let mut at = end + 1;
        let mut faces = HashMap::new();
        for entry in std::str::from_utf8(&data[..end]).unwrap_or_default().split(' ') {
            let Some((name, len)) = entry.split_once(':') else { continue };
            let Ok(len) = len.parse::<usize>() else { break };
            let font = data.get(at..at + len).and_then(|bytes| ttf_parser::Face::parse(bytes, 0).ok());
            if let (Some(id), Some(font)) = (FontId::parse(name), font) {
                faces.insert(id, font);
            }
            at += len;
        }
        faces
    })
}

/// Renders a laid out formula at `em` pixels per font size, with room
/// around it; returns the image and the pixels of room on each side.
fn render(list: &DisplayList, em: f32, color: Color32) -> (ColorImage, f32) {
    let width = list.width as f32 * em;
    let height = (list.height + list.depth) as f32 * em;
    let em = em * (MAX_SIDE / (width.max(height) + 2.0 * BLEED * em)).min(1.0);
    let pad = (BLEED * em).ceil();
    let size = |extent: f64| ((extent as f32 * em).ceil() + 2.0 * pad).max(1.0) as u32;
    let Some(mut pixmap) = Pixmap::new(size(list.width), size(list.height + list.depth)) else {
        return (ColorImage::filled([1, 1], Color32::TRANSPARENT), 0.0);
    };
    let at = |x: f64, y: f64| (pad + x as f32 * em, pad + y as f32 * em);
    let paint = |c: &ratex_types::color::Color| {
        let mut paint = Paint::default();
        if *c == ratex_types::color::Color::BLACK {
            paint.set_color_rgba8(color.r(), color.g(), color.b(), color.a());
        } else if let Some(c) = tiny_skia::Color::from_rgba(c.r, c.g, c.b, c.a) {
            paint.set_color(c);
        }
        paint
    };
    let fonts = fonts();
    for item in &list.items {
        match item {
            DisplayItem::GlyphPath { x, y, scale, font, char_code, color } => {
                let id = FontId::parse(font).unwrap_or(FontId::MainRegular);
                let Some(face) = fonts.get(&id).or_else(|| fonts.get(&FontId::MainRegular)) else { continue };
                let ch = ratex_font::katex_ttf_glyph_char(id, *char_code);
                let Some(glyph) = face.glyph_index(ch) else { continue };
                let (x, y) = at(*x, *y);
                let scale = *scale as f32 * em / f32::from(face.units_per_em());
                let mut outline = Outline { path: PathBuilder::new(), x, y, scale };
                if face.outline_glyph(glyph, &mut outline).is_some() {
                    if let Some(path) = outline.path.finish() {
                        pixmap.fill_path(&path, &paint(color), FillRule::Winding, Transform::identity(), None);
                    }
                }
            }
            DisplayItem::Line { x, y, width, thickness, color, dashed } => {
                let (x, y) = at(*x, *y);
                let (width, thickness) = (*width as f32 * em, (*thickness as f32 * em).max(1.0));
                // Dashes as long as four rules are thick, with gaps as long.
                let dash = if *dashed { (4.0 * thickness).max(2.0) } else { width };
                let mut from = x;
                while from < x + width {
                    let rect = tiny_skia::Rect::from_xywh(from, y - thickness / 2.0, dash.min(x + width - from), thickness);
                    if let Some(rect) = rect {
                        pixmap.fill_path(&PathBuilder::from_rect(rect), &paint(color), FillRule::Winding, Transform::identity(), None);
                    }
                    from += 2.0 * dash;
                }
            }
            DisplayItem::Rect { x, y, width, height, color } => {
                let (x, y) = at(*x, *y);
                if let Some(rect) = tiny_skia::Rect::from_xywh(x, y, *width as f32 * em, *height as f32 * em) {
                    pixmap.fill_path(&PathBuilder::from_rect(rect), &paint(color), FillRule::Winding, Transform::identity(), None);
                }
            }
            DisplayItem::Path { x, y, commands, fill, color } => {
                let (x, y) = at(*x, *y);
                if !*fill {
                    let stroke = tiny_skia::Stroke { width: 0.04 * em, ..Default::default() };
                    if let Some(path) = path(commands, x, y, em) {
                        pixmap.stroke_path(&path, &paint(color), &stroke, Transform::identity(), None);
                    }
                    continue;
                }
                // Each piece on its own and even-odd: the pieces of a
                // stretched arrow can wind either way, and a tall delimiter
                // draws its stem over its outline.
                let mut start = 0;
                for end in (1..=commands.len()).filter(|&i| i == commands.len() || matches!(commands[i], PathCommand::MoveTo { .. })) {
                    if let Some(path) = path(&commands[start..end], x, y, em) {
                        pixmap.fill_path(&path, &paint(color), FillRule::EvenOdd, Transform::identity(), None);
                    }
                    start = end;
                }
            }
        }
    }
    let side = [pixmap.width() as usize, pixmap.height() as usize];
    (ColorImage::from_rgba_premultiplied(side, pixmap.data()), pad)
}

fn path(commands: &[PathCommand], x: f32, y: f32, em: f32) -> Option<tiny_skia::Path> {
    let at = |px: f64, py: f64| (x + px as f32 * em, y + py as f32 * em);
    let mut path = PathBuilder::new();
    for c in commands {
        match *c {
            PathCommand::MoveTo { x, y } => {
                let (x, y) = at(x, y);
                path.move_to(x, y);
            }
            PathCommand::LineTo { x, y } => {
                let (x, y) = at(x, y);
                path.line_to(x, y);
            }
            PathCommand::QuadTo { x1, y1, x, y } => {
                let ((x1, y1), (x, y)) = (at(x1, y1), at(x, y));
                path.quad_to(x1, y1, x, y);
            }
            PathCommand::CubicTo { x1, y1, x2, y2, x, y } => {
                let ((x1, y1), (x2, y2), (x, y)) = (at(x1, y1), at(x2, y2), at(x, y));
                path.cubic_to(x1, y1, x2, y2, x, y);
            }
            PathCommand::Close => path.close(),
        }
    }
    path.finish()
}

/// A glyph's outline, placed with its origin at `x`, `y` and scaled from
/// font units to pixels; font units go up, pixels down.
struct Outline {
    path: PathBuilder,
    x: f32,
    y: f32,
    scale: f32,
}

impl Outline {
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x * self.scale, self.y - y * self.scale)
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.path.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.path.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let ((x1, y1), (x, y)) = (self.at(x1, y1), self.at(x, y));
        self.path.quad_to(x1, y1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let ((x1, y1), (x2, y2), (x, y)) = (self.at(x1, y1), self.at(x2, y2), self.at(x, y));
        self.path.cubic_to(x1, y1, x2, y2, x, y);
    }

    fn close(&mut self) {
        self.path.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_loads() {
        assert_eq!(fonts().len(), 19);
    }

    #[test]
    fn formulas_render() {
        let list = typeset(r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}").expect("typesets");
        assert!(list.height > 1.0 && list.depth > 0.5, "a display fraction is tall");
        let (image, pad) = render(&list, 20.0, Color32::BLACK);
        assert_eq!(image.size[1] as f32, ((list.height + list.depth) as f32 * 20.0).ceil() + 2.0 * pad);
        assert!(image.pixels.iter().filter(|p| p.a() > 0).count() > 200);
        assert!(typeset(r"\frac{1}{").is_err());
        assert!(typeset(r"\nosuchcommand").is_err());
    }
}
