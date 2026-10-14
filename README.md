# Notely

A block editor in the spirit of Notion, written in Rust. One core crate holds
the document model, editing, undo and Markdown handling. One app crate draws
it with egui, as a native desktop app and, compiled to WebAssembly, on a
canvas in the browser.

## Run

```
cargo install wasm-bindgen-cli --version 0.2.129

make serve      # browser: builds, then serves http://localhost:8080
make desktop    # native desktop app
make test       # all tests, run natively (includes random editing sessions)
```

The Rust version and the wasm target are pinned in `rust-toolchain.toml`;
rustup installs them on first use.

## Layout

- `crates/core` — pure Rust, no UI dependencies.
  - `text.rs` rich text: a string plus run-length encoded formatting
  - `doc.rs` blocks, positions, selection, and the page's title, icon and cover
  - `editor/` editing commands, Markdown shortcuts, undo history
  - `highlight/` syntax colouring of code blocks, language guessed from the text
  - `markdown/` Markdown parse and serialize (paste, copy)
  - `store.rs` lossless line format used for saving
- `crates/app` — the egui front end, for desktop and browser.
  - `main.rs` start-up on each platform, autosave
  - `view.rs` the document widget: layout, painting, keyboard and pointer input
  - `theme.rs` colours, fonts and text styles
  - `storage.rs` a file on desktop, `localStorage` in the browser
  - `clipboard.rs` copying, including on pages without the browser's clipboard API
  - `scroll.rs` in the browser, scrolling by the page itself; on desktop the
    mouse wheel step, and on Linux touchpad speed and gliding matched to
    Chromium's (`testdata/` holds the recorded gestures it is tested on)
  - `meter.rs` the scroll meter panel (`make meter` on desktop, `?meter` in the
    browser)
- `www` — the page hosting the canvas, and the built `pkg/`. The page is what
  scrolls: the canvas is fixed and draws the document at the page's offset.
- `tools/scroll-meter.html` — measures a browser's scrolling; `make meter` shows
  the same panel in the desktop app, to compare the two.
- `spikes` — throwaway demos used to compare UI toolkits, and `web-dom`, the
  earlier DOM front end; not part of the build.

## How it stays fast

- The document is a flat list of blocks; nesting is an indent level.
- Each block has a revision number. Text layout is cached per block and
  redone only for blocks whose revision changed.
- Undo records just the blocks an edit replaced, so history costs as much as
  the edits, not the document.

## Editing

| | |
|---|---|
| `/` | block menu |
| `#`, `##`, `###`, `-`, `1.`, `[]`, `>` then space | block type |
| ` ``` ` then space or Enter, `---` | code block, divider |
| `**b**`, `*i*`, `~~s~~`, `` `c` ``, `[text](url)` | inline formatting |
| Ctrl+B / I / U / E, Ctrl+Shift+S, Ctrl+K | bold, italic, underline, code, strike, link |
| Ctrl+Shift+0…8 | text, H1–H3, to-do, bullet, numbered, quote, code |
| Tab / Shift+Tab | indent / outdent |
| Ctrl+Shift+↑ / ↓ | move blocks |
| Ctrl+Enter | check a to-do, or leave a code block |
| Shift+Enter | line break inside a block |
| Ctrl+Z / Ctrl+Shift+Z | undo / redo |

Hovering a link shows its address with Copy and Edit; clicking opens it.
Ctrl+K or the toolbar's Link button on a selection, or Ctrl+K with the caret in
a link, edits the address.

Hovering a block shows two handles left of it: `+` opens a paragraph below, the
grip moves the block when dragged (with the blocks nested under it, or the
whole selection when it spans several) and selects it when clicked. Escape
cancels a drag.

Blocks can be selected as blocks: click a grip (Shift+click extends), press
Ctrl+A twice, or drag a box starting beside the text. Then ↑ / ↓ move the
selection (Shift extends), Backspace deletes, Ctrl+C / X / V copy, cut and
replace, Tab and the block shortcuts apply to all of them, Enter or Escape
returns to the text.

The page has a title above its blocks, and can have an icon (an emoji) and a
cover (a colour or gradient). Hovering above the title shows Add icon and Add
cover, which pick one at random; clicking the icon, or Change cover on the
cover, opens a picker with Remove. Enter or ↓ on the title's last line goes on
into the document, and ↑ on the document's first line comes back to it.

Hovering a code block shows its options in the top right corner: the
language (guessed from the text until one is chosen; ` ```rust ` in Markdown),
line wrapping, and a copy button. Unwrapped code scrolls sideways with
Shift+wheel or a sideways swipe.

The document is saved as you type: to `localStorage` in the browser, to
`notely/document.notely` in the user data directory on desktop. Copy puts
Markdown on the clipboard, and pasting Markdown creates blocks.
