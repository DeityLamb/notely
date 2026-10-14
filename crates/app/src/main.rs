//! The app, drawn with egui: a native desktop window, and the same code on a
//! canvas in the browser. It only turns input into `notely_core::Editor`
//! commands and paints the result; the document, editing rules, undo and
//! Markdown all live in the core crate.

mod clipboard;
mod colors;
mod emoji;
mod math;
mod meter;
mod scroll;
mod storage;
mod theme;
mod view;

use eframe::egui;
use notely_core::{markdown, store, Block, Editor, Kind, Page};

/// Seconds the document must be left alone before it is saved, and the
/// longest unsaved changes wait while editing never pauses.
const SAVE_DELAY: f64 = 0.4;
const SAVE_AT_MOST: f64 = 3.0;

struct App {
    ed: Editor,
    view: view::DocView,
    /// Document version last written to storage.
    saved: u32,
    /// Latest unsaved version and the time it was first seen.
    pending: Option<(u32, f64)>,
    /// When the oldest unsaved change was made.
    dirty_since: Option<f64>,
    /// Version whose save failed; not retried until the document changes.
    failed: Option<u32>,
    /// The scroll meter, when the app was started to measure scrolling.
    meter: Option<meter::Meter>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        // Popups and tooltips are flat: an outline, no shadow.
        cc.egui_ctx.all_styles_mut(|style| {
            style.visuals.popup_shadow = egui::Shadow::NONE;
            style.animation_time = theme::FADE;
            style.visuals.window_shadow = egui::Shadow::NONE;
            // A pointing hand over anything that can be clicked.
            style.visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
        });
        scroll::configure(&cc.egui_ctx);
        #[cfg(target_arch = "wasm32")]
        scroll::page::follow(&cc.egui_ctx);
        let meter = meter::start();
        let sample = meter.as_ref().map(|_| (Page::default(), markdown::parse(&meter::sample_document())));
        let (page, blocks) =
            sample.unwrap_or_else(|| storage::load(storage::DOCUMENT).and_then(|s| store::load(&s)).unwrap_or_else(welcome));
        let ed = Editor::from_page(page, blocks);
        App {
            saved: ed.version(),
            ed,
            view: view::DocView::default(),
            pending: None,
            dirty_since: None,
            failed: None,
            meter,
        }
    }

    /// Saves once the document has been left alone for `SAVE_DELAY`.
    fn autosave(&mut self, ctx: &egui::Context) {
        let version = self.ed.version();
        if version == self.saved || self.failed == Some(version) {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now - *self.dirty_since.get_or_insert(now) >= SAVE_AT_MOST {
            return self.save();
        }
        let wait = |left: f64| ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
        match self.pending {
            Some((v, since)) if v == version => {
                let left = SAVE_DELAY - (now - since);
                if left > 0.0 {
                    wait(left);
                } else {
                    self.save();
                }
            }
            _ => {
                self.pending = Some((version, now));
                wait(SAVE_DELAY);
            }
        }
    }

    fn save(&mut self) {
        // The meter's sample document must not replace the real one.
        if self.meter.is_some() {
            self.saved = self.ed.version();
            return;
        }
        match storage::save(storage::DOCUMENT, &store::save(self.ed.page(), self.ed.blocks())) {
            Ok(()) => self.saved = self.ed.version(),
            Err(err) => {
                self.failed = Some(self.ed.version());
                log(&format!("notely: could not save: {err}"));
            }
        }
        self.pending = None;
        self.dirty_since = None;
    }
}

/// The document a new user starts with: its heading becomes the page's
/// title, under an icon and a cover.
fn welcome() -> (Page, Vec<Block>) {
    let mut blocks = markdown::parse(notely_core::WELCOME);
    let mut page = Page { icon: "👋".into(), cover: "lavender".into(), ..Page::default() };
    if blocks.first().is_some_and(|b| b.kind == Kind::H1) {
        page.title = blocks.remove(0).text.as_str().to_owned();
    }
    (page, blocks)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let palette = theme::Palette::new(ui.visuals().dark_mode);
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(palette.bg)).show(ui, |ui| {
            let area = egui::ScrollArea::vertical().auto_shrink(false);
            // In the browser the page scrolls and the document follows it.
            #[cfg(target_arch = "wasm32")]
            let page_at = scroll::page::offset();
            #[cfg(target_arch = "wasm32")]
            let area = area
                .vertical_scroll_offset(page_at)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
            let scrolled = area.show(ui, |ui| self.view.show(ui, &mut self.ed, &palette));
            #[cfg(target_arch = "wasm32")]
            scroll::page::sync(scrolled.content_size.y, page_at, scrolled.state.offset.y);
            #[cfg(target_arch = "wasm32")]
            scroll::page::track_popups(ui.ctx());
            if let Some(meter) = &mut self.meter {
                meter.show(ui.ctx(), scrolled.state.offset.y);
            }
        });
        self.autosave(ui.ctx());
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(meter) = &mut self.meter {
            meter.record_input(raw_input);
        }
        #[cfg(target_arch = "wasm32")]
        scroll::page::leave_wheel_to_page(raw_input);
        scroll::speed_up(raw_input);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.ed.version() != self.saved {
            self.save();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn log(msg: &str) {
    eprintln!("{msg}");
}

#[cfg(target_arch = "wasm32")]
fn log(msg: &str) {
    web_sys::console::error_1(&msg.into());
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Notely")
            .with_app_id("notely")
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([360.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native("Notely", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

/// Starts the app on the page's `#canvas`.
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;
    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("no #canvas");
        // Scrolling is the page's own (see `scroll::page`), so plain
        // vertical wheel events are left to the browser. Zooming (Ctrl) and
        // sideways swipes, which the browser may take as back and forward,
        // are still cancelled, and so is all wheel movement over a popup,
        // which scrolls the popup.
        let options = eframe::WebOptions {
            should_prevent_default: Box::new(|event| match event {
                egui::Event::MouseWheel { delta, modifiers, .. } => {
                    modifiers.ctrl || modifiers.command || delta.x.abs() > delta.y.abs() || scroll::page::wheel_is_apps()
                }
                _ => true,
            }),
            ..Default::default()
        };
        let started = eframe::WebRunner::new().start(canvas, options, Box::new(|cc| Ok(Box::new(App::new(cc))))).await;
        match started {
            Ok(()) => clipboard::install(),
            Err(_) => log("notely: could not start (is WebGL available?)"),
        }
    });
}
