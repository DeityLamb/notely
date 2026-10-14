//! Syntax colouring for code blocks.
//!
//! A code block does not say what language it holds, so the language is
//! guessed from telltale words, and the text is then cut into tokens by one
//! small scanner that the guess adjusts (which words are keywords, what
//! starts a comment, whether `'a` is a lifetime). It knows nothing of
//! grammar: it is fast, small, and right about the things worth colouring.

mod langs;
mod scan;
#[cfg(test)]
mod tests;

use langs::{guess, Lang, C_LIKE, PYTHON, RUST, SCRIPT, SHELL, SQL};
pub use scan::highlight;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    Plain,
    Comment,
    String,
    /// Numbers and named constants (`true`, `None`, `MAX_LEN`).
    Number,
    Keyword,
    Type,
    /// A name that is being called.
    Function,
}

/// A language a code block can be set to.
pub struct Language {
    pub name: &'static str,
    /// Names it goes by after a Markdown fence; the first one is written.
    pub tags: &'static [&'static str],
    lang: Option<&'static Lang>,
}

impl Language {
    /// Whether the name or one of the Markdown names contains `query`,
    /// which is in lower case.
    pub fn matches(&self, query: &str) -> bool {
        self.name.to_ascii_lowercase().contains(query) || self.tags.iter().any(|tag| tag.contains(query))
    }
}

/// The first entry guesses the language from the text, the second leaves
/// the text uncoloured.
pub const LANGUAGES: [Language; 16] = [
    Language { name: "Auto", tags: &[], lang: None },
    Language { name: "Plain text", tags: &["text", "plain", "txt"], lang: None },
    Language { name: "Bash", tags: &["bash", "sh", "shell", "zsh"], lang: Some(&SHELL) },
    Language { name: "C", tags: &["c", "h"], lang: Some(&C_LIKE) },
    Language { name: "C++", tags: &["cpp", "c++", "cc", "hpp"], lang: Some(&C_LIKE) },
    Language { name: "C#", tags: &["csharp", "cs"], lang: Some(&C_LIKE) },
    Language { name: "Go", tags: &["go", "golang"], lang: Some(&C_LIKE) },
    Language { name: "Java", tags: &["java"], lang: Some(&C_LIKE) },
    Language { name: "JavaScript", tags: &["javascript", "js", "jsx"], lang: Some(&SCRIPT) },
    Language { name: "JSON", tags: &["json"], lang: Some(&SCRIPT) },
    Language { name: "Kotlin", tags: &["kotlin", "kt"], lang: Some(&C_LIKE) },
    Language { name: "Python", tags: &["python", "py"], lang: Some(&PYTHON) },
    Language { name: "Rust", tags: &["rust", "rs"], lang: Some(&RUST) },
    Language { name: "SQL", tags: &["sql"], lang: Some(&SQL) },
    Language { name: "Swift", tags: &["swift"], lang: Some(&C_LIKE) },
    Language { name: "TypeScript", tags: &["typescript", "ts", "tsx"], lang: Some(&SCRIPT) },
];

/// The language a Markdown fence names, or the guessing one.
pub fn language(tag: &str) -> u8 {
    let tag = tag.trim().to_ascii_lowercase();
    LANGUAGES.iter().position(|l| l.tags.contains(&tag.as_str())).unwrap_or(0) as u8
}

/// What is written after a Markdown fence for a language.
pub fn tag(language: u8) -> &'static str {
    LANGUAGES.get(usize::from(language)).and_then(|l| l.tags.first()).copied().unwrap_or("")
}

/// The name of the language the text of a code block is guessed to be in.
pub fn detected(src: &str) -> &'static str {
    guess(src).map_or(LANGUAGES[1].name, |lang| lang.name)
}
