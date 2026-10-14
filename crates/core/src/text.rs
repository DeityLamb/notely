//! Rich text: a UTF-8 string plus run-length encoded formatting.
//! All offsets are byte offsets on char boundaries.

use std::ops::Range;
use std::rc::Rc;

pub mod mark {
    pub const BOLD: u8 = 1;
    pub const ITALIC: u8 = 2;
    pub const UNDERLINE: u8 = 4;
    pub const STRIKE: u8 = 8;
    pub const CODE: u8 = 16;
    pub const ALL: [u8; 5] = [BOLD, ITALIC, UNDERLINE, STRIKE, CODE];
}

/// The colours text and its background can have, by name; `Attrs` holds an
/// index into this list, 0 meaning none. The order is part of the saved
/// format: new colours go at the end.
pub const COLORS: [&str; 10] =
    ["default", "gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red"];

#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct Attrs {
    pub marks: u8,
    /// Colour of the letters and of the background behind them: indices
    /// into `COLORS`.
    pub color: u8,
    pub bg: u8,
    pub link: Option<Rc<str>>,
}

impl Attrs {
    pub fn is_plain(&self) -> bool {
        self.marks == 0 && self.color == 0 && self.bg == 0 && self.link.is_none()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Run {
    pub len: usize,
    pub attrs: Attrs,
}

/// Invariants: run lengths sum to `text.len()`, no run is empty and adjacent
/// runs have different attributes.
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct RichText {
    text: String,
    runs: Vec<Run>,
}

impl RichText {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn plain(s: &str) -> Self {
        let mut t = Self::new();
        t.push(s, &Attrs::default());
        t
    }

    /// Rebuilds a text from stored parts, rejecting runs that do not tile it.
    pub fn from_parts(text: String, runs: Vec<Run>) -> Option<Self> {
        let mut end = 0;
        for r in &runs {
            end += r.len;
            if r.len == 0 || !text.is_char_boundary(end) {
                return None;
            }
        }
        if end != text.len() {
            return None;
        }
        let mut t = Self { text, runs };
        t.merge();
        Some(t)
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn run_count(&self) -> usize {
        self.runs.len()
    }

    pub fn raw_runs(&self) -> &[Run] {
        &self.runs
    }

    pub fn runs(&self) -> impl Iterator<Item = (&str, &Attrs)> {
        let mut start = 0;
        self.runs.iter().map(move |r| {
            let s = &self.text[start..start + r.len];
            start += r.len;
            (s, &r.attrs)
        })
    }

    /// Index and start offset of the run holding the char that ends at `at`
    /// (the first run for `at == 0`).
    pub fn locate(&self, at: usize) -> (usize, usize) {
        let mut start = 0;
        for (i, r) in self.runs.iter().enumerate() {
            if at <= start + r.len {
                return (i, start);
            }
            start += r.len;
        }
        (self.runs.len().saturating_sub(1), start)
    }

    /// Attributes newly typed text takes at `at`: those of the char before
    /// it. A link is not extended past either of its ends.
    pub fn attrs_at(&self, at: usize) -> Attrs {
        let mut start = 0;
        for r in &self.runs {
            let end = start + r.len;
            if at <= end {
                let mut a = r.attrs.clone();
                if at == end || at == start {
                    a.link = None;
                }
                return a;
            }
            start = end;
        }
        Attrs::default()
    }

    /// Link of the char starting at `at`.
    pub fn link_at(&self, at: usize) -> Option<Rc<str>> {
        let mut start = 0;
        for r in &self.runs {
            if at < start + r.len {
                return r.attrs.link.clone();
            }
            start += r.len;
        }
        None
    }

    /// Extent and target of the link that covers or touches offset `at`.
    pub fn link_span(&self, at: usize) -> Option<(Range<usize>, Rc<str>)> {
        let hit = |span: &(Range<usize>, Rc<str>)| span.0.start <= at && at <= span.0.end;
        let mut span: Option<(Range<usize>, Rc<str>)> = None;
        let mut start = 0;
        for r in &self.runs {
            let end = start + r.len;
            match (&mut span, &r.attrs.link) {
                (Some((range, url)), Some(link)) if url == link => range.end = end,
                _ => {
                    if span.as_ref().is_some_and(hit) {
                        return span;
                    }
                    span = r.attrs.link.clone().map(|link| (start..end, link));
                }
            }
            start = end;
        }
        span.filter(hit)
    }

    /// True when every char in `r` carries `mark`.
    pub fn has_mark(&self, r: Range<usize>, mark: u8) -> bool {
        let mut start = 0;
        for run in &self.runs {
            let end = start + run.len;
            if start < r.end && end > r.start && run.attrs.marks & mark == 0 {
                return false;
            }
            start = end;
        }
        !r.is_empty()
    }

    /// The value `get` gives every char in `r`, if they all agree.
    pub fn shared<T: PartialEq>(&self, r: Range<usize>, get: impl Fn(&Attrs) -> T) -> Option<T> {
        let mut shared = None;
        let mut start = 0;
        for run in &self.runs {
            let end = start + run.len;
            if start < r.end && end > r.start {
                let value = get(&run.attrs);
                match &shared {
                    Some(s) if *s != value => return None,
                    Some(_) => {}
                    None => shared = Some(value),
                }
            }
            start = end;
        }
        shared
    }

    pub fn push(&mut self, s: &str, attrs: &Attrs) {
        self.insert(self.text.len(), s, attrs);
    }

    pub fn insert(&mut self, at: usize, s: &str, attrs: &Attrs) {
        if s.is_empty() {
            return;
        }
        let i = self.cut(at);
        self.text.insert_str(at, s);
        self.runs.insert(i, Run { len: s.len(), attrs: attrs.clone() });
        self.merge();
    }

    pub fn insert_rich(&mut self, at: usize, other: &RichText) {
        let i = self.cut(at);
        self.text.insert_str(at, &other.text);
        self.runs.splice(i..i, other.runs.iter().cloned());
        self.merge();
    }

    pub fn append(&mut self, other: RichText) {
        self.text.push_str(&other.text);
        self.runs.extend(other.runs);
        self.merge();
    }

    pub fn delete(&mut self, r: Range<usize>) {
        if r.is_empty() {
            return;
        }
        let a = self.cut(r.start);
        let b = self.cut(r.end);
        self.runs.drain(a..b);
        self.text.replace_range(r, "");
        self.merge();
    }

    pub fn slice(&self, r: Range<usize>) -> RichText {
        let mut out = RichText::new();
        let mut start = 0;
        for run in &self.runs {
            let end = start + run.len;
            let (s, e) = (start.max(r.start), end.min(r.end));
            if s < e {
                out.runs.push(Run { len: e - s, attrs: run.attrs.clone() });
            }
            start = end;
        }
        out.text.push_str(&self.text[r]);
        out
    }

    /// Keeps `..at` and returns `at..`.
    pub fn split_off(&mut self, at: usize) -> RichText {
        let i = self.cut(at);
        RichText { runs: self.runs.split_off(i), text: self.text.split_off(at) }
    }

    pub fn set_mark(&mut self, r: Range<usize>, mark: u8, on: bool) {
        self.restyle(r, |a| {
            if on {
                a.marks |= mark;
            } else {
                a.marks &= !mark;
            }
        });
    }

    pub fn set_link(&mut self, r: Range<usize>, link: Option<Rc<str>>) {
        self.restyle(r, |a| a.link = link.clone());
    }

    /// Changes the attributes of the text in `r` with `f`.
    pub fn restyle(&mut self, r: Range<usize>, mut f: impl FnMut(&mut Attrs)) {
        if r.is_empty() {
            return;
        }
        let a = self.cut(r.start);
        let b = self.cut(r.end);
        for run in &mut self.runs[a..b] {
            f(&mut run.attrs);
        }
        self.merge();
    }

    /// Ensures a run boundary at `at` and returns the index of the run that
    /// starts there.
    fn cut(&mut self, at: usize) -> usize {
        let mut start = 0;
        for i in 0..self.runs.len() {
            if start == at {
                return i;
            }
            let len = self.runs[i].len;
            if at < start + len {
                let attrs = self.runs[i].attrs.clone();
                self.runs[i].len = at - start;
                self.runs.insert(i + 1, Run { len: start + len - at, attrs });
                return i + 1;
            }
            start += len;
        }
        self.runs.len()
    }

    fn merge(&mut self) {
        self.runs.retain(|r| r.len > 0);
        self.runs.dedup_by(|cur, prev| {
            if cur.attrs == prev.attrs {
                prev.len += cur.len;
                true
            } else {
                false
            }
        });
    }
}
