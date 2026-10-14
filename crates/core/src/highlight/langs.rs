//! What the scanner is told about each language, and the guess of which
//! one a text is in.

pub(super) struct Lang {
    /// What a guess of this language is called.
    pub(super) name: &'static str,
    /// Words that give the language away; the language with most of them
    /// in the text is the guess.
    pub(super) hints: &'static [&'static str],
    pub(super) keywords: &'static [&'static str],
    /// What starts a comment that runs to the end of the line.
    pub(super) line: &'static [&'static str],
    /// What opens and closes a comment that can span lines.
    pub(super) block: Option<(&'static str, &'static str)>,
    /// Characters that open and close a string.
    pub(super) quotes: &'static str,
    /// `'a` names a lifetime and only `'a'` is a character.
    pub(super) lifetimes: bool,
    /// Keywords count in any letter case, and hints too.
    pub(super) any_case: bool,
}

pub(super) const RUST: Lang = Lang {
    name: "Rust",
    hints: &["fn ", "let mut ", "impl ", "pub ", "::", "-> ", "&str", "&mut ", "println!", "match ", "use "],
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn", "for",
        "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self",
        "static", "struct", "super", "trait", "type", "unsafe", "use", "where", "while",
    ],
    line: &["//"],
    block: Some(("/*", "*/")),
    quotes: "\"'",
    lifetimes: true,
    any_case: false,
};

pub(super) const PYTHON: Lang = Lang {
    name: "Python",
    hints: &["def ", "elif ", "import ", "self.", "print(", "None", "    return", "__", "lambda ", "except"],
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except",
        "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass",
        "raise", "return", "self", "try", "while", "with", "yield",
    ],
    line: &["#"],
    block: None,
    quotes: "\"'",
    lifetimes: false,
    any_case: false,
};

pub(super) const SCRIPT: Lang = Lang {
    name: "JavaScript",
    hints: &["function ", "const ", "=> ", "console.", "===", "var ", "export ", "require(", "async ", "document."],
    keywords: &[
        "async", "await", "break", "case", "catch", "class", "const", "continue", "default", "delete", "do", "else",
        "enum", "export", "extends", "finally", "for", "from", "function", "if", "implements", "import", "in",
        "instanceof", "interface", "let", "new", "of", "return", "static", "super", "switch", "this", "throw", "try",
        "type", "typeof", "var", "void", "while", "yield",
    ],
    line: &["//"],
    block: Some(("/*", "*/")),
    quotes: "\"'`",
    lifetimes: false,
    any_case: false,
};

pub(super) const SHELL: Lang = Lang {
    name: "Bash",
    hints: &["#!/bin", "echo ", "sudo ", "$(", "apt ", "cd ", " | ", "fi\n", "git ", "npm ", "cargo ", "curl "],
    keywords: &[
        "case", "cd", "do", "done", "echo", "elif", "else", "esac", "exit", "export", "fi", "for", "function", "if",
        "in", "local", "return", "sudo", "then", "while",
    ],
    line: &["#"],
    block: None,
    quotes: "\"'",
    lifetimes: false,
    any_case: false,
};

pub(super) const SQL: Lang = Lang {
    name: "SQL",
    hints: &["select ", " from ", "where ", "insert into", "create table", "update ", "join "],
    keywords: &[
        "alter", "and", "as", "by", "create", "delete", "distinct", "drop", "from", "group", "having", "inner",
        "insert", "into", "join", "key", "left", "limit", "not", "on", "or", "order", "outer", "primary", "right",
        "select", "set", "table", "union", "update", "values", "where",
    ],
    line: &["--"],
    block: Some(("/*", "*/")),
    quotes: "'\"",
    lifetimes: false,
    any_case: true,
};

/// C, C++, Java, C#, Go, Kotlin and Swift look alike enough to share one
/// description. It is also what text of no recognised language gets.
pub(super) const C_LIKE: Lang = Lang {
    name: "C-like",
    hints: &[
        "#include", "int ", "void ", "public ", "class ", "func ", "package ", "std::", "printf(", "static ", "new ",
        "struct ", "();", "};", "return ", "if (", "for (",
    ],
    keywords: &[
        "auto", "bool", "break", "case", "catch", "chan", "char", "class", "const", "continue", "default", "defer",
        "delete", "do", "double", "else", "enum", "extern", "final", "float", "fn", "for", "fun", "func", "go", "goto",
        "if", "import", "inline", "int", "interface", "let", "long", "map", "namespace", "new", "override", "package",
        "private", "protected", "public", "range", "return", "short", "signed", "sizeof", "static", "string",
        "struct", "switch", "template", "this", "throw", "try", "type", "typedef", "union", "unsigned", "using", "val",
        "var", "virtual", "void", "volatile", "while",
    ],
    line: &["//"],
    block: Some(("/*", "*/")),
    quotes: "\"'`",
    lifetimes: false,
    any_case: false,
};

const LANGS: [&Lang; 6] = [&RUST, &PYTHON, &SCRIPT, &SHELL, &SQL, &C_LIKE];

/// The language most of whose telltale words are in the text, if any are.
pub(super) fn guess(src: &str) -> Option<&'static Lang> {
    let lower = src.to_ascii_lowercase();
    let score = |lang: &Lang| {
        let text = if lang.any_case { lower.as_str() } else { src };
        lang.hints.iter().filter(|hint| text.contains(*hint)).count()
    };
    // The first of the best: on a tie the more distinctive language wins.
    let mut best = (None, 0);
    for lang in LANGS {
        if score(lang) > best.1 {
            best = (Some(lang), score(lang));
        }
    }
    best.0
}
