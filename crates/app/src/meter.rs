//! Scroll meter, a tuning aid: shows how fast the page is scrolling and
//! graphs the speed of the last gesture, to compare with the same panel in
//! `tools/scroll-meter.html` running in a browser.
//!
//! Started with `NOTELY_SCROLL_METER` set on desktop, or with `?meter` in the
//! address in the browser, which also opens a long sample document and turns
//! saving off. Every scroll event the app receives (`w time distance unit
//! phase`) and every frame's scroll offset during a gesture (`f time
//! offset`) are logged, so a gesture can be replayed through the scroll code
//! in a test: on desktop to the file the variable names, if it names one,
//! in the browser to the page's `meterLog` array.

use eframe::egui::{self, pos2, vec2, Align2, Color32, Event, FontId, Rect, Stroke};

/// Seconds without movement that end a gesture.
const GAP: f64 = 0.3;
/// Seconds of a gesture the graph has room for.
const SPAN: f32 = 3.0;

#[derive(Default)]
pub struct Meter {
    /// Scroll offset and time of the previous frame.
    last: Option<(f32, f64)>,
    /// Whether a gesture is in progress, when it began and last moved.
    active: bool,
    start: f64,
    last_move: f64,
    /// The current or last gesture: speed over time, and its totals.
    samples: Vec<(f32, f32)>,
    distance: f32,
    peak: f32,
    duration: f32,
    /// Speed in this frame, points per second.
    speed: f32,
    log: Log,
}

/// Where events and offsets are written.
#[derive(Default)]
enum Log {
    #[default]
    Nowhere,
    #[cfg(not(target_arch = "wasm32"))]
    File(std::fs::File),
    #[cfg(target_arch = "wasm32")]
    Page(js_sys::Array),
}

impl Log {
    fn line(&mut self, line: String) {
        match self {
            Log::Nowhere => drop(line),
            #[cfg(not(target_arch = "wasm32"))]
            Log::File(file) => {
                use std::io::Write as _;
                let _ = writeln!(file, "{line}");
            }
            #[cfg(target_arch = "wasm32")]
            Log::Page(lines) => drop(lines.push(&line.into())),
        }
    }
}

/// The meter, if the app was started to measure scrolling.
#[cfg(not(target_arch = "wasm32"))]
pub fn start() -> Option<Meter> {
    let setting = std::env::var_os("NOTELY_SCROLL_METER")?;
    let file = (setting != "1").then(|| std::fs::File::create(&setting).ok()).flatten();
    Some(Meter { log: file.map_or(Log::Nowhere, Log::File), ..Default::default() })
}

/// The meter, if the page's address asks for it.
#[cfg(target_arch = "wasm32")]
pub fn start() -> Option<Meter> {
    let window = web_sys::window()?;
    if !window.location().search().ok()?.contains("meter") {
        return None;
    }
    let lines = js_sys::Array::new();
    let _ = js_sys::Reflect::set(&window, &"meterLog".into(), &lines);
    Some(Meter { log: Log::Page(lines), ..Default::default() })
}

/// A document long enough to scroll through.
pub fn sample_document() -> String {
    (1..=600).map(|n| format!("Line {n} of a long document for measuring scrolling\n\n")).collect()
}

impl Meter {
    /// Notes the scroll events as the system sent them, before the app
    /// changes them.
    pub fn record_input(&mut self, input: &egui::RawInput) {
        for event in &input.events {
            if let Event::MouseWheel { unit, delta, phase, .. } = event {
                let time = input.time.unwrap_or_default();
                self.log.line(format!("w {time:.4} {:.4} {unit:?} {phase:?}", delta.y));
            }
        }
    }

    /// Records this frame's scroll offset and draws the panel.
    pub fn show(&mut self, ctx: &egui::Context, offset: f32) {
        let now = ctx.input(|i| i.time);
        let (before, at) = self.last.unwrap_or((offset, now));
        self.last = Some((offset, now));
        let moved = offset - before;
        // The app does not redraw while idle, so the frame that starts a
        // gesture follows a long pause; count it as one ordinary frame.
        let dt = if self.active { (now - at) as f32 } else { 1.0 / 60.0 };
        self.speed = if dt > 0.0 { moved.abs() / dt } else { 0.0 };
        if moved != 0.0 {
            if !self.active {
                self.active = true;
                self.start = now;
                self.samples.clear();
                (self.distance, self.peak) = (0.0, 0.0);
            }
            self.last_move = now;
            self.distance += moved;
            self.peak = self.peak.max(self.speed);
            self.duration = (now - self.start) as f32;
        }
        if self.active {
            self.log.line(format!("f {now:.4} {offset:.3}"));
            self.samples.push(((now - self.start) as f32, self.speed));
            self.active = now - self.last_move < GAP;
            ctx.request_repaint();
        }
        self.paint(ctx);
    }

    fn paint(&self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let rect = Rect::from_min_size(pos2(screen.right() - 316.0, screen.top() + 16.0), vec2(300.0, 190.0));
        let layer = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("scroll-meter"));
        let painter = ctx.layer_painter(layer);
        let (fg, dim, line) = (Color32::from_gray(230), Color32::from_gray(140), Color32::from_rgb(90, 156, 240));
        painter.rect_filled(rect, 8.0, Color32::from_rgba_unmultiplied(20, 20, 20, 235));
        let text = |row: f32, label: &str, value: String| {
            let y = rect.top() + 12.0 + row * 20.0;
            painter.text(pos2(rect.left() + 12.0, y), Align2::LEFT_TOP, label, FontId::proportional(13.0), dim);
            painter.text(pos2(rect.right() - 12.0, y), Align2::RIGHT_TOP, value, FontId::monospace(13.0), fg);
        };
        text(0.0, "speed now", format!("{:.0} pt/s", self.speed));
        text(1.0, "peak", format!("{:.0} pt/s", self.peak));
        text(2.0, "distance", format!("{:.0} pt", self.distance.abs()));
        text(3.0, "time", format!("{:.2} s", self.duration));

        // Speed over the gesture: full height is the peak, full width `SPAN`.
        let graph = Rect::from_min_max(pos2(rect.left() + 12.0, rect.top() + 98.0), rect.max - vec2(12.0, 12.0));
        painter.rect_stroke(graph, 0.0, Stroke::new(1.0, Color32::from_gray(60)), egui::StrokeKind::Inside);
        let points: Vec<_> = self
            .samples
            .iter()
            .filter(|(t, _)| *t <= SPAN)
            .map(|(t, v)| pos2(graph.left() + t / SPAN * graph.width(), graph.bottom() - v / self.peak.max(1.0) * graph.height()))
            .collect();
        painter.line(points, Stroke::new(1.5, line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the meter over offsets sampled every `dt` seconds.
    fn measure(offsets: &[f32], dt: f64) -> Meter {
        let ctx = egui::Context::default();
        let mut meter = Meter::default();
        for (n, &offset) in offsets.iter().enumerate() {
            let input = egui::RawInput { time: Some(n as f64 * dt), ..Default::default() };
            ctx.run_ui(input, |ui| meter.show(ui.ctx(), offset)).textures_delta.clear();
        }
        meter
    }

    #[test]
    fn measures_a_gesture() {
        // 10 points a frame at 100 frames a second, then still.
        let mut offsets: Vec<f32> = (0..=20).map(|n| n as f32 * 10.0).collect();
        offsets.extend([200.0; 50]);
        let meter = measure(&offsets, 0.01);
        assert_eq!(meter.distance, 200.0);
        assert!((meter.peak - 1000.0).abs() < 1.0, "{}", meter.peak);
        assert!((meter.duration - 0.19).abs() < 0.011, "{}", meter.duration);
        assert!(!meter.active && meter.speed == 0.0);
    }

    #[test]
    fn a_new_gesture_starts_from_zero() {
        let mut offsets = vec![0.0, 50.0];
        offsets.extend([50.0; 60]);
        offsets.extend([50.0, 40.0]);
        let meter = measure(&offsets, 0.01);
        assert_eq!(meter.distance, -10.0);
    }
}
