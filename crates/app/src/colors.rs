//! The colour button of the formatting toolbar and the picker it opens: the
//! colours used last, then a text colour or a background for the selected
//! text, each list starting with the default, which takes the colour away.

use eframe::egui::{
    self, pos2, vec2, Align2, Area, CursorIcon, FontId, Id, Key, Order, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};
use notely_core::{Editor, COLORS};

use crate::storage;
use crate::theme::Palette;

const COLUMNS: usize = 5;
const CELL: f32 = 32.0;
const SWATCH: f32 = 24.0;
const PAD: f32 = 10.0;
const WIDTH: f32 = COLUMNS as f32 * CELL + 2.0 * PAD;
const HEADING: f32 = 30.0;
/// Choices kept under "Recently used": one row.
const RECENT: usize = COLUMNS;
/// Where the recent choices are saved: one a line, most recent first, `t`
/// for a text colour or `b` for a background, then the colour's index.
const SAVED: &str = "colors";

/// A colour in the picker: an index into `COLORS`, for the text or for the
/// background behind it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Text(u8),
    Background(u8),
}

impl Choice {
    fn parse(s: &str) -> Option<Self> {
        let (kind, color) = s.split_at_checked(1)?;
        let color = color.parse().ok().filter(|&c: &u8| usize::from(c) < COLORS.len())?;
        match kind {
            "t" => Some(Choice::Text(color)),
            "b" => Some(Choice::Background(color)),
            _ => None,
        }
    }

    fn save(self) -> String {
        match self {
            Choice::Text(c) => format!("t{c}"),
            Choice::Background(c) => format!("b{c}"),
        }
    }

    /// What its tooltip calls it: "Red text", "Default background".
    fn name(self) -> String {
        let (color, what) = match self {
            Choice::Text(c) => (c, "text"),
            Choice::Background(c) => (c, "background"),
        };
        let name = COLORS[usize::from(color)];
        format!("{}{} {what}", name[..1].to_uppercase(), &name[1..])
    }
}

#[derive(Default)]
pub struct ColorPicker {
    open: bool,
    /// Pass the button was last shown on: the picker closes with the toolbar.
    shown: u64,
    recent: Vec<Choice>,
    loaded: bool,
}

impl ColorPicker {
    fn load(&mut self) {
        if !std::mem::replace(&mut self.loaded, true) {
            let saved = storage::load(SAVED).unwrap_or_default();
            self.recent = saved.lines().filter_map(Choice::parse).take(RECENT).collect();
        }
    }

    fn used(&mut self, choice: Choice) {
        self.recent.retain(|&c| c != choice);
        self.recent.insert(0, choice);
        self.recent.truncate(RECENT);
        let lines: Vec<String> = self.recent.iter().map(|c| c.save()).collect();
        // Losing them costs little; the picker works on without.
        let _ = storage::save(SAVED, &lines.join("\n"));
    }

    /// The toolbar's button: an "A" over a bar, in the colours of the
    /// selection. It opens the picker below it.
    pub fn button(&mut self, ui: &mut Ui, ed: &mut Editor, pal: &Palette) {
        let pass = ui.ctx().cumulative_pass_nr();
        if self.shown + 1 < pass {
            self.open = false;
        }
        self.shown = pass;
        let (color, bg) = ed.active_colors();
        let height = ui.spacing().interact_size.y;
        let (rect, response) = ui.allocate_exact_size(vec2(height, height), Sense::click());
        let response = response.on_hover_cursor(CursorIcon::PointingHand);
        let painter = ui.painter();
        if response.hovered() || self.open {
            painter.rect_filled(rect, 4.0, ui.visuals().widgets.hovered.weak_bg_fill);
        }
        let letters = Rect::from_center_size(rect.center(), vec2(14.0, 16.0));
        if let Some(bg) = bg.filter(|&bg| bg != 0) {
            painter.rect_filled(letters.expand(1.0), 3.0, pal.back[usize::from(bg)]);
        }
        let ink = pal.text[usize::from(color.unwrap_or(0))];
        painter.text(letters.center() - vec2(0.0, 1.5), Align2::CENTER_CENTER, "A", FontId::proportional(14.0), ink);
        let bar = Rect::from_min_max(pos2(letters.left(), letters.bottom() - 2.0), letters.right_bottom());
        painter.rect_filled(bar, 1.0, ink);
        let clicked = response.on_hover_text("Color").clicked();
        if clicked {
            self.open = !self.open;
            self.load();
        }
        if self.open {
            self.popup(ui.ctx(), rect, ed, pal, clicked);
        }
    }

    /// The picker, below the toolbar's `button`, or above the toolbar when
    /// the window ends first; `fresh` that it opened on this pass.
    fn popup(&mut self, ctx: &egui::Context, button: Rect, ed: &mut Editor, pal: &Palette, fresh: bool) {
        let current = ed.active_colors();
        let texts: Vec<Choice> = (0..COLORS.len() as u8).map(Choice::Text).collect();
        let backgrounds: Vec<Choice> = (0..COLORS.len() as u8).map(Choice::Background).collect();
        let sections = [("Recently used", &self.recent), ("Text color", &texts), ("Background color", &backgrounds)];
        let sections: Vec<_> = sections.into_iter().filter(|(_, choices)| !choices.is_empty()).collect();
        let rows = |choices: &[Choice]| choices.len().div_ceil(COLUMNS) as f32;
        let height = 4.0 + sections.iter().map(|(_, c)| HEADING + rows(c) * CELL).sum::<f32>() + PAD;
        let below = button.left_bottom() + vec2(0.0, 12.0);
        let (pivot, at) = match below.y + height > ctx.content_rect().bottom() {
            true => (Align2::LEFT_BOTTOM, button.left_top() - vec2(0.0, 12.0)),
            false => (Align2::LEFT_TOP, below),
        };
        let mut chosen = None;
        let id = Id::new("color-picker");
        let shown = Area::new(id).order(Order::Foreground).pivot(pivot).fixed_pos(at).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(0.0).show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.set_width(WIDTH);
                ui.add_space(4.0);
                for (n, (title, choices)) in sections.iter().enumerate() {
                    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, HEADING), Sense::hover());
                    let at = rect.left_center() + vec2(PAD + 4.0, 2.0);
                    ui.painter().text(at, Align2::LEFT_CENTER, *title, FontId::proportional(12.5), pal.muted);
                    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, rows(choices) * CELL), Sense::hover());
                    for (i, &choice) in choices.iter().enumerate() {
                        let at = vec2(PAD + (i % COLUMNS) as f32 * CELL, (i / COLUMNS) as f32 * CELL);
                        let cell = Rect::from_min_size(rect.min + at, Vec2::splat(CELL));
                        let on = match choice {
                            Choice::Text(c) => current.0 == Some(c),
                            Choice::Background(c) => current.1 == Some(c),
                        };
                        if swatch(ui, id.with((n, i)), cell, choice, on, pal).clicked() {
                            chosen = Some(choice);
                        }
                    }
                }
                ui.add_space(PAD - 2.0);
            });
        });
        match chosen {
            Some(choice) => {
                match choice {
                    Choice::Text(c) => ed.set_color(c),
                    Choice::Background(c) => ed.set_background(c),
                }
                if !matches!(choice, Choice::Text(0) | Choice::Background(0)) {
                    self.used(choice);
                }
                self.open = false;
            }
            None => {
                let escape = ctx.input(|i| i.key_pressed(Key::Escape));
                self.open = !escape && (fresh || !shown.response.clicked_elsewhere());
            }
        }
    }
}

/// One colour of the picker in `cell`: an "A" in a text colour, or a square
/// filled with a background; `on` marks the selection's own.
fn swatch(ui: &Ui, id: Id, cell: Rect, choice: Choice, on: bool, pal: &Palette) -> Response {
    let response = ui.interact(cell, id, Sense::click()).on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(cell, 5.0, ui.visuals().widgets.hovered.weak_bg_fill);
    }
    let square = Rect::from_center_size(cell.center(), Vec2::splat(SWATCH));
    match choice {
        Choice::Text(c) => {
            painter.rect_filled(square, 5.0, pal.bg);
            let ink = pal.text[usize::from(c)];
            painter.text(square.center(), Align2::CENTER_CENTER, "A", FontId::proportional(15.0), ink);
        }
        Choice::Background(c) => {
            painter.rect_filled(square, 5.0, if c == 0 { pal.bg } else { pal.back[usize::from(c)] });
        }
    }
    let stroke = if on { Stroke::new(1.5, pal.accent) } else { Stroke::new(1.0, pal.line) };
    painter.rect_stroke(square, 5.0, stroke, StrokeKind::Inside);
    response.on_hover_text(choice.name())
}
