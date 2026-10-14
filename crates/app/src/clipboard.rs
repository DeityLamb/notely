//! Copying text out of the app.
//!
//! In the browser egui writes to the clipboard through the asynchronous
//! clipboard API, which only exists on secure pages (https or localhost) and
//! silently does nothing elsewhere. Ctrl+C and Ctrl+X arrive as the page's
//! own `copy` and `cut` events, which can carry text on any page, so there
//! the text is handed to that event instead.

use eframe::egui;

/// Puts `text` on the clipboard in answer to a copy or cut keypress.
pub fn copy(ctx: &egui::Context, text: String) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        web::PENDING.with(|pending| *pending.borrow_mut() = Some(text));
    }
    #[cfg(not(target_arch = "wasm32"))]
    ctx.copy_text(text);
}

#[cfg(target_arch = "wasm32")]
pub use web::install;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast as _;

    thread_local! {
        /// Text the app wants copied by the `copy` or `cut` event being handled.
        pub static PENDING: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// Listens for the page's `copy` and `cut` events. Call it once the app
    /// has started: egui's own listeners on the document run first and have
    /// the app produce the text, then this one attaches it to the event.
    pub fn install() {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
        let attach = Closure::<dyn Fn(web_sys::ClipboardEvent)>::new(|event: web_sys::ClipboardEvent| {
            let text = PENDING.with(|pending| pending.borrow_mut().take());
            if let (Some(text), Some(data)) = (text, event.clipboard_data()) {
                if data.set_data("text/plain", &text).is_ok() {
                    event.prevent_default();
                }
            }
        });
        for kind in ["copy", "cut"] {
            let _ = document.add_event_listener_with_callback(kind, attach.as_ref().unchecked_ref());
        }
        // The listeners live as long as the page.
        attach.forget();
    }
}
