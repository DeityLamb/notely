use super::*;

/// The pieces as text, leaving out plain ones.
fn coloured(src: &str) -> Vec<(&str, Token)> {
    let mut at = 0;
    let mut out = Vec::new();
    for (len, token) in highlight(src, 0) {
        if token != Token::Plain {
            out.push((&src[at..at + len], token));
        }
        at += len;
    }
    assert_eq!(at, src.len(), "pieces do not cover the text");
    out
}

#[test]
fn rust() {
    let got = coloured("fn main() {\n    let x: Vec<&'a str> = vec![1, 0xff]; // done\n    println!(\"hi {}\", 'c');\n}");
    let want = [
        ("fn", Token::Keyword),
        ("main", Token::Function),
        ("let", Token::Keyword),
        ("Vec", Token::Type),
        ("'a", Token::Type),
        ("str", Token::Type),
        ("1", Token::Number),
        ("0xff", Token::Number),
        ("// done", Token::Comment),
        ("println", Token::Function),
        ("\"hi {}\"", Token::String),
        ("'c'", Token::String),
    ];
    assert_eq!(got, want);
}

#[test]
fn python() {
    let got = coloured("def area(r):  # circle\n    \"\"\"Doc\nstring\"\"\"\n    return PI * r if r else None");
    let want = [
        ("def", Token::Keyword),
        ("area", Token::Function),
        ("# circle", Token::Comment),
        ("\"\"\"Doc\nstring\"\"\"", Token::String),
        ("return", Token::Keyword),
        ("PI", Token::Number),
        ("if", Token::Keyword),
        ("else", Token::Keyword),
        ("None", Token::Number),
    ];
    assert_eq!(got, want);
}

#[test]
fn script_and_urls() {
    let got = coloured("const url = 'https://e.x/a'; /* two\nlines */ fetch(`${url}`)");
    let want = [
        ("const", Token::Keyword),
        ("'https://e.x/a'", Token::String),
        ("/* two\nlines */", Token::Comment),
        ("fetch", Token::Function),
        ("`${url}`", Token::String),
    ];
    assert_eq!(got, want);
}

#[test]
fn shell_and_sql() {
    assert_eq!(coloured("echo $# # count")[1], ("# count", Token::Comment));
    let got = coloured("SELECT name FROM users -- all\nWHERE id = 3");
    assert_eq!(got[0], ("SELECT", Token::Keyword));
    assert_eq!(got[2], ("-- all", Token::Comment));
}

#[test]
fn a_set_language_overrides_the_guess() {
    let src = "fn x() {} # note";
    assert!(!coloured(src).contains(&("# note", Token::Comment)));
    let python = highlight(src, language("PY"));
    assert_eq!(python.last(), Some(&(6, Token::Comment)));
    assert_eq!(highlight(src, language("text")), [(src.len(), Token::Plain)]);
    assert_eq!((language("nonsense"), tag(language("js")), tag(0)), (0, "javascript", ""));
    assert_eq!((detected(src), detected("just words"), detected("x = None")), ("Rust", "Plain text", "Python"));
    assert_eq!(highlight("just 2 words", 0), [(12, Token::Plain)]);
}

#[test]
fn unfinished_and_odd_text_is_covered() {
    for src in ["", "\"open", "'", "/* open", "x = \"a\\", "'\\", "мир = 'жир' # да", "日本\"語", "a\\\n\"\"\"", "`"] {
        coloured(src);
    }
}
