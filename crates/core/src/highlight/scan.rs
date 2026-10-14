//! The scanner that cuts the text of a code block into tokens.

use super::langs::{guess, Lang};
use super::{Token, LANGUAGES};

/// Named values, coloured like numbers in every language.
const CONSTANTS: [&str; 9] = ["true", "false", "null", "nil", "None", "True", "False", "undefined", "NULL"];

/// Built-in types that do not start with a capital letter.
const PRIMITIVES: [&str; 17] = [
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64", "bool",
    "str", "char",
];

/// Cuts `src` into coloured pieces: each is a length in bytes and what the
/// text of that length is. The lengths add up to the length of `src`.
/// `language` is an index into `LANGUAGES`.
pub fn highlight(src: &str, language: u8) -> Vec<(usize, Token)> {
    let set = LANGUAGES.get(usize::from(language)).and_then(|l| l.lang);
    // Text of no language, said or guessed, stays plain.
    let Some(lang) = set.or_else(|| guess(src).filter(|_| language != 1)) else {
        return vec![(src.len(), Token::Plain)];
    };
    let bytes = src.as_bytes();
    let mut out: Vec<(usize, Token)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let (len, token) = if let Some(len) = comment(lang, src, i) {
            (len, Token::Comment)
        } else if lang.quotes.as_bytes().contains(&b) {
            quoted(lang, src, i)
        } else if b.is_ascii_digit() {
            let digits = src[i..].bytes().take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_'));
            (digits.count(), Token::Number)
        } else if is_ident(b) {
            word(lang, &src[i..])
        } else {
            (1, Token::Plain)
        };
        // Neighbouring pieces of one colour are one piece.
        match out.last_mut() {
            Some((n, last)) if *last == token => *n += len,
            _ => out.push((len, token)),
        }
        i += len;
    }
    out
}

/// Length of the comment that starts at `at`, if one does.
fn comment(lang: &Lang, src: &str, at: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let rest = &src[at..];
    // A `#` or `--` comment needs space before it (`$#`, `x--`), and a
    // `//` must not be the one in `https://`.
    let apart = at == 0 || bytes[at - 1].is_ascii_whitespace();
    let line = lang.line.iter().any(|mark| {
        rest.starts_with(mark) && if *mark == "//" { at == 0 || bytes[at - 1] != b':' } else { apart }
    });
    if line {
        return Some(rest.find('\n').unwrap_or(rest.len()));
    }
    let (_, close) = lang.block.filter(|(open, _)| rest.starts_with(open))?;
    Some(rest[2..].find(close).map_or(rest.len(), |end| 2 + end + close.len()))
}

/// The piece that starts with the quote at `at`: a string, or in a
/// language that has them a lifetime.
fn quoted(lang: &Lang, src: &str, at: usize) -> (usize, Token) {
    let rest = &src[at..];
    let quote = rest.as_bytes()[0];
    let mut chars = rest[1..].chars();
    let character = matches!((chars.next(), chars.next()), (Some('\\'), _) | (Some(_), Some('\'')));
    let triple = rest.as_bytes().get(..3).is_some_and(|open| open.iter().all(|q| *q == quote)) && quote != b'`';
    if quote == b'\'' && lang.lifetimes && !character {
        // A lifetime: the quote and the name after it.
        let len = 1 + rest[1..].bytes().take_while(|b| is_ident(*b)).count();
        (len, Token::Type)
    } else if triple {
        // Three quotes open a string that runs to the next three.
        let len = rest[3..].find(&rest[..3]).map_or(rest.len(), |end| end + 6);
        (len, Token::String)
    } else {
        (string_end(src, at) - at, Token::String)
    }
}

/// The word that `rest` starts with: its length and what it is.
fn word(lang: &Lang, rest: &str) -> (usize, Token) {
    let len = rest.bytes().take_while(|b| is_ident(*b)).count();
    let word = &rest[..len];
    let after = rest[len..].trim_start_matches(' ');
    let called = after.starts_with('(') || (lang.lifetimes && after.starts_with("!("));
    let keyword = match lang.any_case {
        true => lang.keywords.contains(&word.to_ascii_lowercase().as_str()),
        false => lang.keywords.contains(&word),
    };
    let upper = word.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
    let token = if keyword {
        Token::Keyword
    } else if CONSTANTS.contains(&word) {
        Token::Number
    } else if called {
        Token::Function
    } else if upper && len > 1 {
        Token::Number
    } else if rest.as_bytes()[0].is_ascii_uppercase() || PRIMITIVES.contains(&word) {
        Token::Type
    } else {
        Token::Plain
    };
    (len, token)
}

fn is_ident(b: u8) -> bool {
    // Bytes of non-ASCII characters count as letters, which also keeps
    // every cut on a character boundary.
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

/// End of the string that opens at `at`: after its closing quote, or the
/// end of the line when it is not closed. Backticks may span lines.
fn string_end(src: &str, at: usize) -> usize {
    let bytes = src.as_bytes();
    let quote = bytes[at];
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 1,
            b if b == quote => return i + 1,
            b'\n' if quote != b'`' => return i,
            _ => {}
        }
        i += 1;
    }
    // An escape at the very end can step past it or into a character.
    let mut end = i.min(src.len());
    while !src.is_char_boundary(end) {
        end += 1;
    }
    end
}
