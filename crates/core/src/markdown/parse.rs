//! Markdown to blocks.

use super::inline::parse_inline;
use crate::doc::{Block, Kind, MAX_INDENT};
use crate::text::RichText;

pub fn parse(src: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut lines = src.lines();
    while let Some(line) = lines.next() {
        let body = line.trim_start();
        if body.is_empty() {
            continue;
        }
        if let Some(tag) = body.strip_prefix("```") {
            out.push(code_block(tag, &mut lines));
            continue;
        }
        let (kind, checked, rest) = prefix(body).unwrap_or((Kind::Paragraph, false, body));
        let mut text = rest.trim_end().to_string();
        while ends_with_break(&text) {
            let Some(next) = lines.next() else { break };
            text.pop();
            text.push('\n');
            text.push_str(next.trim());
        }
        let mut block = Block::new(kind, parse_inline(&text));
        block.checked = checked;
        if kind.is_list() {
            let lead = &line[..line.len() - body.len()];
            let cols: usize = lead.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();
            block.indent = (cols / 2).min(MAX_INDENT as usize) as u8;
        }
        out.push(block);
    }
    out
}

/// Reads a code block up to its closing fence. `tag` is what followed the
/// opening one.
fn code_block(tag: &str, lines: &mut std::str::Lines) -> Block {
    let mut code = String::new();
    for (n, line) in lines.take_while(|l| l.trim() != "```").enumerate() {
        if n > 0 {
            code.push('\n');
        }
        code.push_str(line);
    }
    let mut block = Block::new(Kind::Code, RichText::plain(&code));
    block.lang = crate::highlight::language(tag);
    block
}

/// Splits a block marker off the start of a line.
pub(crate) fn prefix(s: &str) -> Option<(Kind, bool, &str)> {
    let line = s.trim_end();
    if line.len() >= 3 && matches!(line.as_bytes()[0], b'-' | b'*' | b'_') {
        let c = line.as_bytes()[0];
        if line.bytes().all(|b| b == c) {
            return Some((Kind::Divider, false, ""));
        }
    }
    let hashes = s.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes) {
        if let Some(rest) = s[hashes..].strip_prefix(' ') {
            let kind = match hashes {
                1 => Kind::H1,
                2 => Kind::H2,
                _ => Kind::H3,
            };
            return Some((kind, false, rest.trim_start()));
        }
    }
    if let Some(rest) = s.strip_prefix('>') {
        return Some((Kind::Quote, false, rest.strip_prefix(' ').unwrap_or(rest)));
    }
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = s.strip_prefix(bullet) {
            for (tag, checked) in [("[ ] ", false), ("[] ", false), ("[x] ", true), ("[X] ", true)] {
                if let Some(rest) = rest.strip_prefix(tag) {
                    return Some((Kind::Todo, checked, rest));
                }
            }
            return Some((Kind::Bullet, false, rest));
        }
    }
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let rest = &s[digits..];
        if let Some(rest) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((Kind::Numbered, false, rest));
        }
    }
    None
}

fn ends_with_break(s: &str) -> bool {
    s.bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 1
}
