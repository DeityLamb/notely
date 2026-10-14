//! Page icons, drawn by Twemoji.
//!
//! `emoji/twemoji.png` holds the icons offered, in the order of [`ICONS`],
//! in a grid of square cells (`tools/twemoji.py` builds it). The first time
//! an icon is drawn, each cell becomes a texture of its own, so sampling
//! never bleeds into a neighbour. An icon outside the set is drawn by the emoji
//! font instead.

use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, ColorImage, Id, Painter, Pos2, Rect, TextureFilter, TextureHandle,
    TextureOptions, Vec2,
};

use crate::theme;

/// Icons offered by the icon picker, and at random by "Add icon".
pub const ICONS: &str = "📝📄📚📌💡🚀🎯✅⭐🔥🌱🌊🌸🍀🌙☀⚡🎨🎵🎮📷🎉🏆💬💼🏠✈🌍🧠🔑📅🧪☕🍕🐱🐶👋😀😎❤";
const ATLAS: &[u8] = include_bytes!("../emoji/twemoji.png");
const CELL: usize = 128;

/// Draws `icon` as a square of side `size` centred on `center`; `color`
/// is for the font, should the icon not be one of [`ICONS`].
pub fn paint(painter: &Painter, center: Pos2, size: f32, icon: &str, color: Color32) {
    let rect = Rect::from_center_size(center, Vec2::splat(size));
    match texture(painter.ctx(), icon) {
        Some(texture) => {
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(texture.id(), rect, uv, Color32::WHITE);
        }
        None => {
            // The font's glyphs are drawn smaller than their size.
            let font = theme::emoji_font(size / 0.9);
            painter.text(center, Align2::CENTER_CENTER, icon, font, color);
        }
    }
}

/// The texture of `icon`, if it is one of [`ICONS`].
fn texture(ctx: &egui::Context, icon: &str) -> Option<TextureHandle> {
    // A trailing variation selector asks for the emoji form, which every
    // icon here has.
    let mut chars = icon.trim_end_matches('\u{fe0f}').chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    let index = ICONS.chars().position(|i| i == c)?;
    let id = Id::new("twemoji");
    let textures = match ctx.data(|d| d.get_temp::<Arc<Vec<TextureHandle>>>(id)) {
        Some(textures) => textures,
        None => {
            let textures = Arc::new(load(ctx));
            ctx.data_mut(|d| d.insert_temp(id, textures.clone()));
            textures
        }
    };
    textures.get(index).cloned()
}

fn load(ctx: &egui::Context) -> Vec<TextureHandle> {
    let Ok(atlas) = image::load_from_memory_with_format(ATLAS, image::ImageFormat::Png) else {
        return Vec::new();
    };
    let atlas = atlas.into_rgba8();
    let columns = atlas.width() as usize / CELL;
    let options = TextureOptions::LINEAR.with_mipmap_mode(Some(TextureFilter::Linear));
    ICONS
        .chars()
        .enumerate()
        .map(|(i, c)| {
            let (x, y) = (i % columns * CELL, i / columns * CELL);
            let cell = image::imageops::crop_imm(&atlas, x as u32, y as u32, CELL as u32, CELL as u32).to_image();
            let image = ColorImage::from_rgba_unmultiplied([CELL, CELL], cell.as_raw());
            ctx.load_texture(format!("twemoji-{c}"), image, options)
        })
        .collect()
}
