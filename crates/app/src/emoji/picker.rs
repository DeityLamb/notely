//! The emoji picker of a page's icon: a filter, a random emoji, the skin
//! tone, the recent emoji and then every emoji by category, with a button
//! below for each category.

use eframe::egui::{
    self, pos2, vec2, Align, Align2, CursorIcon, FontId, Frame, Id, Key, Margin, Rect, Response, ScrollArea, Sense, Stroke,
    TextEdit, Ui, Vec2,
};

use super::{all, key, paint, CATEGORIES};
use crate::storage;
use crate::theme::{self, Palette};

const COLUMNS: usize = 12;
const CELL: f32 = 32.0;
const EMOJI: f32 = 24.0;
const PAD: f32 = 12.0;
const WIDTH: f32 = COLUMNS as f32 * CELL + 2.0 * PAD;
const TOOLS: f32 = 32.0;
const LIST: f32 = 300.0;
const HEADING: f32 = 30.0;
const BAR: f32 = 44.0;
/// Recent emoji kept: two rows.
const RECENT: usize = 2 * COLUMNS;
/// Where the skin tone and the recent emoji are saved: the tone on the
/// first line, then an emoji on each line, most recent first.
const SAVED: &str = "emoji";
/// The emoji the skin tone button shows, in the tone chosen.
const HAND: &str = "✋";

/// What was chosen in the picker.
pub enum Choice {
    /// An emoji; the picker closes.
    Pick(String),
    /// An emoji at random; the picker stays to try another.
    Random(String),
    Remove,
}

#[derive(Default)]
pub struct EmojiPicker {
    filter: String,
    /// 0 is the default yellow, 1 to 5 light to dark.
    tone: usize,
    /// The tones are shown in place of the filter, to choose one.
    choosing_tone: bool,
    recent: Vec<String>,
    loaded: bool,
    /// The section to scroll to: 0 is the recent emoji, then the categories.
    jump: Option<usize>,
    /// The section at the top of the list.
    shown: usize,
}

impl EmojiPicker {
    /// Starts afresh, as the picker opens.
    fn open(&mut self) {
        (self.filter, self.choosing_tone, self.jump, self.shown) = (String::new(), false, None, 0);
        if !std::mem::replace(&mut self.loaded, true) {
            let saved = storage::load(SAVED).unwrap_or_default();
            let mut lines = saved.lines();
            self.tone = lines.next().and_then(|l| l.parse().ok()).filter(|&t| t <= 5).unwrap_or(0);
            self.recent = lines.filter(|l| !l.is_empty()).take(RECENT).map(str::to_owned).collect();
        }
    }

    fn save(&self) {
        let lines: Vec<String> = std::iter::once(self.tone.to_string()).chain(self.recent.iter().cloned()).collect();
        // Losing them costs little; the picker works on without.
        let _ = storage::save(SAVED, &lines.join("\n"));
    }

    fn used(&mut self, emoji: &str) {
        self.recent.retain(|r| key(r) != key(emoji));
        self.recent.insert(0, emoji.to_owned());
        self.recent.truncate(RECENT);
        self.save();
    }

    /// The picker, in a popup; `current` is the page's icon, `fresh` that
    /// it opened on this frame and `seed` a number to pick at random by.
    pub fn show(&mut self, ui: &mut Ui, current: &str, pal: &Palette, fresh: bool, seed: usize) -> Option<Choice> {
        if fresh {
            self.open();
        }
        let mut choice = None;
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.set_width(WIDTH);
        let id = Id::new("emoji-picker");
        let hover = ui.visuals().widgets.hovered.weak_bg_fill;

        // The filter, or the tones to choose from; then the random emoji,
        // the skin tone and Remove.
        ui.add_space(PAD);
        let (tools, _) = ui.allocate_exact_size(vec2(WIDTH, TOOLS), Sense::hover());
        let font = FontId::proportional(13.5);
        let width = ui.fonts_mut(|f| f.layout_no_wrap("Remove".into(), font.clone(), pal.fg).size().x);
        let remove = Rect::from_min_max(pos2(tools.right() - PAD - width - 20.0, tools.top()), pos2(tools.right() - PAD, tools.bottom()));
        let tone_button = Rect::from_min_size(pos2(remove.left() - 6.0 - TOOLS, tools.top()), Vec2::splat(TOOLS));
        let shuffle = tone_button.translate(vec2(-TOOLS - 6.0, 0.0));
        let field = Rect::from_min_max(pos2(tools.left() + PAD, tools.top()), pos2(shuffle.left() - 6.0, tools.bottom()));
        let response = button(ui, id.with("remove"), remove);
        framed(ui, &response, pal, hover);
        let color = if response.hovered() { pal.fg } else { pal.muted };
        ui.painter().text(remove.center(), Align2::CENTER_CENTER, "Remove", font, color);
        if response.clicked() {
            choice = Some(Choice::Remove);
        }
        let hand = all().iter().find(|e| key(e.text) == HAND).unwrap_or(&all()[0]);
        if self.choosing_tone {
            for tone in 0..=5 {
                let cell = Rect::from_min_size(field.min + vec2(tone as f32 * TOOLS, 0.0), Vec2::splat(TOOLS));
                let response = button(ui, id.with(("tone", tone)), cell);
                if response.hovered() || tone == self.tone {
                    ui.painter().rect_filled(cell, 5.0, hover);
                }
                paint(ui.painter(), cell.center(), 22.0, hand.in_tone(tone), pal.fg);
                if response.clicked() {
                    (self.tone, self.choosing_tone) = (tone, false);
                    self.save();
                }
            }
        } else {
            let edit = TextEdit::singleline(&mut self.filter)
                .id(id.with("filter"))
                .hint_text("Filter…")
                .font(FontId::proportional(14.0))
                .frame(
                    Frame::new()
                        .fill(pal.code_bg)
                        .stroke(Stroke::new(1.0, pal.line))
                        .corner_radius(6.0)
                        .inner_margin(Margin { left: 30, right: 8, top: 7, bottom: 7 }),
                );
            let response = ui.put(field, edit);
            if fresh {
                response.request_focus();
            }
            if response.changed() {
                self.jump = None;
            }
            let at = field.left_center() + vec2(15.0, 0.0);
            ui.painter().text(at, Align2::CENTER_CENTER, "🔍", theme::emoji_font(14.0), pal.muted);
        }
        let response = button(ui, id.with("random"), shuffle);
        framed(ui, &response, pal, hover);
        let color = if response.hovered() { pal.fg } else { pal.muted };
        ui.painter().text(shuffle.center(), Align2::CENTER_CENTER, "🔀", theme::emoji_font(16.0), color);
        if response.clicked() {
            let all = all();
            choice = Some(Choice::Random(all[seed % all.len()].in_tone(self.tone).to_owned()));
        }
        let response = button(ui, id.with("tone"), tone_button);
        if response.hovered() || self.choosing_tone {
            ui.painter().rect_filled(tone_button, 5.0, hover);
        }
        paint(ui.painter(), tone_button.center(), 22.0, hand.in_tone(self.tone), pal.fg);
        if response.clicked() {
            self.choosing_tone = !self.choosing_tone;
        }
        ui.add_space(8.0);

        // The emoji: those matching the filter, or the recent ones and
        // every category.
        let query = self.filter.trim().to_lowercase();
        let tone = self.tone;
        let recent = self.recent.clone();
        let mut sections: Vec<(Option<usize>, &str, Vec<&str>)> = Vec::new();
        if !query.is_empty() {
            let found = all().iter().filter(|e| e.matches(&query)).map(|e| e.in_tone(tone)).collect();
            sections.push((None, "", found));
        } else {
            if !recent.is_empty() {
                sections.push((Some(0), "Recent", recent.iter().map(String::as_str).collect()));
            }
            for (n, category) in CATEGORIES.iter().enumerate() {
                let emoji = all().iter().filter(|e| e.category == n).map(|e| e.in_tone(tone)).collect();
                sections.push((Some(n + 1), category.title, emoji));
            }
        }
        let first_found = sections.first().and_then(|(_, _, e)| e.first().map(|e| e.to_string()));
        let enter = ui.input(|i| i.key_pressed(Key::Enter));
        if enter && !query.is_empty() {
            if let Some(emoji) = first_found {
                choice = Some(Choice::Pick(emoji));
            }
        }
        let mut tops = Vec::new();
        ui.allocate_ui(vec2(WIDTH, LIST), |ui| {
            ScrollArea::vertical().id_salt(id.with("list")).auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let top = ui.clip_rect().top();
                for (section, title, emoji) in &sections {
                    if !title.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, HEADING), Sense::hover());
                        let at = rect.left_center() + vec2(PAD + 4.0, 2.0);
                        ui.painter().text(at, Align2::LEFT_CENTER, *title, FontId::proportional(13.0), pal.muted);
                        if let Some(section) = *section {
                            // Recent may not be there; then its button goes to the top.
                            if self.jump.is_some_and(|j| j == section || (j == 0 && tops.is_empty())) {
                                ui.scroll_to_rect(rect, Some(Align::TOP));
                                (self.jump, self.shown) = (None, section);
                            }
                            tops.push((section, rect.top() - top));
                        }
                    }
                    let rows = emoji.len().div_ceil(COLUMNS);
                    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, rows as f32 * CELL), Sense::hover());
                    for (i, &emoji) in emoji.iter().enumerate() {
                        let at = vec2(PAD + (i % COLUMNS) as f32 * CELL, (i / COLUMNS) as f32 * CELL);
                        let cell = Rect::from_min_size(rect.min + at, Vec2::splat(CELL));
                        if !ui.is_rect_visible(cell) {
                            continue;
                        }
                        let response = button(ui, id.with(("emoji", section, i)), cell);
                        if response.hovered() || key(emoji) == key(current) {
                            ui.painter().rect_filled(cell, 5.0, hover);
                        }
                        paint(ui.painter(), cell.center(), EMOJI, emoji, pal.fg);
                        if response.clicked() {
                            choice = Some(Choice::Pick(emoji.to_owned()));
                        }
                    }
                    if emoji.is_empty() && section.is_none() {
                        let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, HEADING), Sense::hover());
                        let at = rect.left_center() + vec2(PAD + 4.0, 0.0);
                        ui.painter().text(at, Align2::LEFT_CENTER, "No results", FontId::proportional(13.0), pal.muted);
                    }
                }
            });
        });
        if self.jump.is_none() && query.is_empty() {
            self.shown = tops.iter().rev().find(|&&(_, y)| y <= HEADING).map_or(0, |&(s, _)| s);
        }

        // A button for each section.
        let (bar, _) = ui.allocate_exact_size(vec2(WIDTH, BAR), Sense::hover());
        ui.painter().hline(bar.x_range(), bar.top(), Stroke::new(1.0, pal.line));
        let step = (WIDTH - 2.0 * PAD) / (CATEGORIES.len() + 1) as f32;
        let symbols = std::iter::once("🕒").chain(CATEGORIES.iter().map(|c| c.symbol));
        for (n, symbol) in symbols.enumerate() {
            let center = pos2(bar.left() + PAD + step * (n as f32 + 0.5), bar.center().y);
            let rect = Rect::from_center_size(center, Vec2::splat(TOOLS));
            let response = button(ui, id.with(("section", n)), rect);
            let on = query.is_empty() && self.shown == n;
            if on || response.hovered() {
                ui.painter().rect_filled(rect, 5.0, if on { pal.line } else { hover });
            }
            let color = if on || response.hovered() { pal.fg } else { pal.muted };
            ui.painter().text(center, Align2::CENTER_CENTER, symbol, theme::emoji_font(17.0), color);
            if response.clicked() {
                self.filter.clear();
                self.jump = Some(n);
            }
        }

        if let Some(Choice::Pick(emoji)) = &choice {
            self.used(&emoji.clone());
        }
        choice
    }
}

fn button(ui: &Ui, id: Id, rect: Rect) -> Response {
    ui.interact(rect, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand)
}

/// The outline of a button, filled while hovered.
fn framed(ui: &Ui, response: &Response, pal: &Palette, hover: egui::Color32) {
    if response.hovered() {
        ui.painter().rect_filled(response.rect, 5.0, hover);
    }
    ui.painter().rect_stroke(response.rect, 5.0, Stroke::new(1.0, pal.line), egui::StrokeKind::Inside);
}
