//! Blocks to Markdown.

use std::rc::Rc;

use super::parse::prefix;
use crate::doc::{Block, Kind, MAX_INDENT};
use crate::text::{mark, RichText, COLORS};

pub fn serialize(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut numbers = [0u32; MAX_INDENT as usize + 1];
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            out.push('\n');
            if !(b.kind.is_list() && blocks[i - 1].kind.is_list()) {
                out.push('\n');
            }
        }
        let depth = (b.indent.min(MAX_INDENT)) as usize;
        if b.kind == Kind::Numbered {
            numbers[depth] += 1;
            numbers[depth + 1..].fill(0);
        } else {
            numbers[depth..].fill(0);
        }
        if b.kind.is_list() {
            out.push_str(&"  ".repeat(depth));
        }
        match b.kind {
            Kind::Paragraph => {}
            Kind::H1 => out.push_str("# "),
            Kind::H2 => out.push_str("## "),
            Kind::H3 => out.push_str("### "),
            Kind::Bullet => out.push_str("- "),
            Kind::Numbered => out.push_str(&format!("{}. ", numbers[depth])),
            Kind::Todo => out.push_str(if b.checked { "- [x] " } else { "- [ ] " }),
            Kind::Quote => out.push_str("> "),
            Kind::Code => {
                out.push_str("```");
                out.push_str(crate::highlight::tag(b.lang));
                out.push('\n');
                out.push_str(b.text.as_str());
                out.push_str("\n```");
                continue;
            }
            Kind::Math => {
                out.push_str("$$\n");
                out.push_str(b.text.as_str());
                out.push_str("\n$$");
                continue;
            }
            Kind::Divider => {
                out.push_str("---");
                continue;
            }
        }
        let start = out.len();
        write_inline(&b.text, &mut out);
        if b.kind == Kind::Paragraph {
            // Keep text that merely looks like a block marker a paragraph.
            if let Some((kind, ..)) = prefix(&out[start..]) {
                let at = match kind {
                    Kind::Numbered => out[start..].find(['.', ')']).unwrap_or(0),
                    _ => 0,
                };
                out.insert(start + at, '\\');
            } else if out[start..].starts_with("$$") {
                out.insert(start, '\\');
            }
        }
    }
    out
}

pub fn inline_to_string(text: &RichText) -> String {
    let mut out = String::new();
    write_inline(text, &mut out);
    out
}

fn write_inline(text: &RichText, out: &mut String) {
    const MARKS: [(u8, &str, &str); 4] = [
        (mark::BOLD, "**", "**"),
        (mark::ITALIC, "*", "*"),
        (mark::STRIKE, "~~", "~~"),
        (mark::UNDERLINE, "<u>", "</u>"),
    ];
    // Indices into MARKS of the currently open marks, outermost first.
    let mut open: Vec<usize> = Vec::new();
    let mut link: Option<Rc<str>> = None;
    // Text colour and background of the open span, inside the link.
    let mut colors = (0, 0);
    let close_to = |open: &mut Vec<usize>, depth: usize, out: &mut String| {
        while open.len() > depth {
            out.push_str(MARKS[open.pop().unwrap()].2);
        }
    };
    for (s, attrs) in text.runs() {
        if attrs.link != link || (attrs.color, attrs.bg) != colors {
            close_to(&mut open, 0, out);
            if colors != (0, 0) {
                out.push_str("</span>");
            }
            if attrs.link != link {
                if let Some(url) = link.take() {
                    out.push_str(&format!("]({url})"));
                }
                if attrs.link.is_some() {
                    out.push('[');
                    link = attrs.link.clone();
                }
            }
            colors = (attrs.color, attrs.bg);
            if colors != (0, 0) {
                out.push_str(&color_span(colors));
            }
        }
        if let Some(depth) = open.iter().position(|&k| attrs.marks & MARKS[k].0 == 0) {
            close_to(&mut open, depth, out);
        }
        for (k, m) in MARKS.iter().enumerate() {
            if attrs.marks & m.0 != 0 && !open.contains(&k) {
                out.push_str(m.1);
                open.push(k);
            }
        }
        if attrs.marks & mark::CODE != 0 {
            out.push('`');
            out.push_str(s);
            out.push('`');
        } else {
            for c in s.chars() {
                match c {
                    '\\' | '*' | '_' | '`' | '[' | ']' | '~' => {
                        out.push('\\');
                        out.push(c);
                    }
                    '\n' => out.push_str("\\\n"),
                    _ => out.push(c),
                }
            }
        }
    }
    close_to(&mut open, 0, out);
    if colors != (0, 0) {
        out.push_str("</span>");
    }
    if let Some(url) = link {
        out.push_str(&format!("]({url})"));
    }
}

/// Opening tag of a span of coloured text, naming its colours as CSS does.
fn color_span((color, bg): (u8, u8)) -> String {
    let mut style = Vec::new();
    if color != 0 {
        style.push(format!("color:{}", COLORS[usize::from(color)]));
    }
    if bg != 0 {
        style.push(format!("background:{}", COLORS[usize::from(bg)]));
    }
    format!("<span style=\"{}\">", style.join(";"))
}
