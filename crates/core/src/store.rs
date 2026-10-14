//! Lossless storage format: a header line, a line with the page's title,
//! icon and cover (tab separated, escaped like text), then one line per
//! block:
//!
//! `<kind><indent><flag>[language] TAB <text> TAB <runs> [TAB <link>]...`
//!
//! The flag is a checked to-do, or a code block that does not wrap its
//! lines; the language is the Markdown name of a code block's language.
//!
//! Runs are space separated `len:marks[.color.background][:link index]`,
//! the colours as indices into `text::COLORS` and written only when either
//! is set; an empty runs field means unformatted text. Text and links escape
//! `\`, tab and newlines.

use std::rc::Rc;

use crate::doc::{Block, Kind, Page, MAX_INDENT};
use crate::highlight;
use crate::text::{Attrs, RichText, Run, COLORS};

const HEADER: &str = "notely 2";
/// Documents saved before pages had a title, icon and cover: no page line.
const HEADER_V1: &str = "notely 1";

const KINDS: [(char, Kind); 11] = [
    ('p', Kind::Paragraph),
    ('1', Kind::H1),
    ('2', Kind::H2),
    ('3', Kind::H3),
    ('b', Kind::Bullet),
    ('n', Kind::Numbered),
    ('t', Kind::Todo),
    ('q', Kind::Quote),
    ('c', Kind::Code),
    ('m', Kind::Math),
    ('d', Kind::Divider),
];

pub fn save(page: &Page, blocks: &[Block]) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    escape(&page.title, &mut out);
    out.push('\t');
    escape(&page.icon, &mut out);
    out.push('\t');
    escape(&page.cover, &mut out);
    for b in blocks {
        let kind = KINDS.iter().find(|(_, k)| *k == b.kind).map_or('p', |(c, _)| *c);
        out.push('\n');
        out.push(kind);
        out.push((b'0' + b.indent.min(MAX_INDENT)) as char);
        out.push(if b.checked || b.nowrap { '1' } else { '0' });
        out.push_str(highlight::tag(b.lang));
        out.push('\t');
        escape(b.text.as_str(), &mut out);
        out.push('\t');
        let mut links: Vec<&Rc<str>> = Vec::new();
        let runs = b.text.raw_runs();
        if runs.iter().any(|r| !r.attrs.is_plain()) {
            for (i, r) in runs.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&format!("{}:{}", r.len, r.attrs.marks));
                if r.attrs.color != 0 || r.attrs.bg != 0 {
                    out.push_str(&format!(".{}.{}", r.attrs.color, r.attrs.bg));
                }
                if let Some(link) = &r.attrs.link {
                    let at = links.iter().position(|l| *l == link).unwrap_or_else(|| {
                        links.push(link);
                        links.len() - 1
                    });
                    out.push_str(&format!(":{at}"));
                }
            }
        }
        for link in links {
            out.push('\t');
            escape(link, &mut out);
        }
    }
    out
}

pub fn load(src: &str) -> Option<(Page, Vec<Block>)> {
    let mut lines = src.split('\n');
    let page = match lines.next()? {
        HEADER => {
            let mut fields = lines.next()?.split('\t').map(unescape);
            let mut next = || fields.next().unwrap_or_default();
            Page { title: next(), icon: next(), cover: next() }
        }
        HEADER_V1 => Page::default(),
        _ => return None,
    };
    Some((page, lines.map(load_block).collect::<Option<_>>()?))
}

fn load_block(line: &str) -> Option<Block> {
    let mut fields = line.split('\t');
    let mut head = fields.next()?.chars();
    let kind_char = head.next()?;
    let kind = KINDS.iter().find(|(c, _)| *c == kind_char)?.1;
    let indent = (head.next()?.to_digit(10)? as u8).min(MAX_INDENT);
    let flag = head.next()? == '1';
    let lang = highlight::language(head.as_str());
    let text = unescape(fields.next()?);
    let runs_field = fields.next().unwrap_or("");
    let links: Vec<Rc<str>> = fields.map(|l| Rc::from(unescape(l))).collect();

    let rich = if runs_field.is_empty() {
        RichText::plain(&text)
    } else {
        let runs: Option<Vec<Run>> = runs_field
            .split(' ')
            .map(|run| {
                let mut parts = run.split(':');
                let len = parts.next()?.parse().ok()?;
                let mut style = parts.next()?.split('.');
                let marks = style.next()?.parse().ok()?;
                // A colour this version does not know is left out.
                let known = |c: &u8| usize::from(*c) < COLORS.len();
                let mut color = || style.next().and_then(|c| c.parse().ok()).filter(known);
                let (color, bg) = (color().unwrap_or(0), color().unwrap_or(0));
                let link = match parts.next() {
                    Some(i) => Some(links.get(i.parse::<usize>().ok()?)?.clone()),
                    None => None,
                };
                Some(Run { len, attrs: Attrs { marks, color, bg, link } })
            })
            .collect();
        // Formatting that does not fit the text is dropped, the text kept.
        runs.and_then(|runs| RichText::from_parts(text.clone(), runs))
            .unwrap_or_else(|| RichText::plain(&text))
    };
    let mut block = Block::new(kind, if kind == Kind::Divider { RichText::new() } else { rich });
    block.indent = indent;
    if kind == Kind::Code {
        (block.nowrap, block.lang) = (flag, lang);
    } else {
        block.checked = flag;
    }
    Some(block)
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => {}
            _ => out.push(c),
        }
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
