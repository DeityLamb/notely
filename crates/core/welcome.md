# Welcome to Notely

A small block editor. The document, every edit and the Markdown handling run in **Rust** compiled to *WebAssembly*; the page only draws the result.

## Try it

- Type `/` on a line to pick a block type
- Start a line with `#`, `-`, `1.`, `[]` or `>` and press space
- Wrap words in `**bold**`, `*italic*`, `~~strike~~` or backticks
- Select text for the formatting bar, or paste Markdown anywhere
- Select text and press Ctrl+K to add a [link](https://www.rust-lang.org), then hover it
- [x] Press Tab to nest list items
- [ ] Check this off with a click

> Everything is saved as you type.

---

```
fn main() {
    println!("three backticks start a code block");
}
```

