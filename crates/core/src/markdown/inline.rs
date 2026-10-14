//! Markdown to the formatted text of one block.

use std::rc::Rc;

use crate::text::{mark, Attrs, RichText};

pub fn parse_inline(s: &str) -> RichText {
    let mut out = RichText::new();
    inline(s, &Attrs::default(), &mut out);
    out
}

fn with_mark(attrs: &Attrs, mark: u8) -> Attrs {
    Attrs { marks: attrs.marks | mark, link: attrs.link.clone() }
}

fn inline(s: &str, attrs: &Attrs, out: &mut RichText) {
    let b = s.as_bytes();
    // Start of the literal text not yet written out.
    let mut lit = 0;
    let mut i = 0;
    while i < b.len() {
        // Markers are ASCII, so nothing can start inside a multi-byte char.
        if !s.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &s[i..];
        // (bytes consumed, offset and length of the content, its attributes)
        let mut span: Option<(usize, usize, usize, Attrs)> = None;
        match b[i] {
            b'\\' if b.get(i + 1).is_some_and(u8::is_ascii_punctuation) => {
                out.push(&s[lit..i], attrs);
                lit = i + 1;
                i += 2;
                continue;
            }
            b'`' => {
                if let Some(n) = rest[1..].find('`').filter(|&n| n > 0) {
                    out.push(&s[lit..i], attrs);
                    out.push(&rest[1..1 + n], &with_mark(attrs, mark::CODE));
                    i += n + 2;
                    lit = i;
                    continue;
                }
            }
            c @ (b'*' | b'_' | b'~') => {
                let in_word = c == b'_' && s[..i].ends_with(char::is_alphanumeric);
                if b.get(i + 1) == Some(&c) {
                    if let Some(n) = close_double(&rest[2..], c) {
                        let m = if c == b'~' { mark::STRIKE } else { mark::BOLD };
                        span = Some((n + 4, 2, n, with_mark(attrs, m)));
                    }
                }
                if span.is_none() && c != b'~' && !in_word {
                    if let Some(n) = close_single(&rest[1..], c) {
                        span = Some((n + 2, 1, n, with_mark(attrs, mark::ITALIC)));
                    }
                }
            }
            b'[' => {
                if let Some(mid) = rest.find("](").filter(|&m| m > 1) {
                    if let Some(n) = rest[mid + 2..].find(')').filter(|&n| n > 0) {
                        let url = &rest[mid + 2..mid + 2 + n];
                        if !url.contains(char::is_whitespace) {
                            let link = Attrs { marks: attrs.marks, link: Some(Rc::from(url)) };
                            span = Some((mid + n + 3, 1, mid - 1, link));
                        }
                    }
                }
            }
            b'<' if rest.starts_with("<u>") => {
                if let Some(n) = rest[3..].find("</u>") {
                    span = Some((n + 7, 3, n, with_mark(attrs, mark::UNDERLINE)));
                }
            }
            _ => {}
        }
        match span {
            Some((consumed, start, len, inner)) => {
                out.push(&s[lit..i], attrs);
                inline(&rest[start..start + len], &inner, out);
                i += consumed;
                lit = i;
            }
            None => i += 1,
        }
    }
    out.push(&s[lit..], attrs);
}

/// Offset of the `cc` that closes a span opened by `cc`.
fn close_double(s: &str, c: u8) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = 0;
    while j + 1 < b.len() {
        if b[j] == b'\\' {
            j += 2;
        } else if b[j] == c && b[j + 1] == c && j > 0 {
            // In `***` the last two close, the first belongs to the content.
            return Some(if b.get(j + 2) == Some(&c) { j + 1 } else { j });
        } else {
            j += 1;
        }
    }
    None
}

/// Offset of the single `c` that closes a span opened by `c`, stepping over
/// `cc` pairs nested inside.
fn close_single(s: &str, c: u8) -> Option<usize> {
    let b = s.as_bytes();
    let mut in_double = false;
    let mut j = 0;
    while j < b.len() {
        if b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] != c {
            j += 1;
            continue;
        }
        let run = b[j..].iter().take_while(|&&x| x == c).count();
        if run == 2 {
            in_double = !in_double;
            j += 2;
            continue;
        }
        let at = if run >= 3 && in_double { j + 2 } else { j };
        let in_word = c == b'_' && s[at + 1..].starts_with(char::is_alphanumeric);
        if at == 0 || in_word {
            j += run;
            continue;
        }
        return Some(at);
    }
    None
}
