//! Where the document and the app's settings are kept: files in the user
//! data directory natively, `localStorage` in the browser. Each has a name:
//! [`DOCUMENT`] is the document.

pub const DOCUMENT: &str = "document.notely";

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::path::PathBuf;

    /// `name` in the platform's per-user data directory.
    fn file(name: &str) -> Option<PathBuf> {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let dir = if cfg!(target_os = "windows") {
            var("APPDATA")?
        } else if cfg!(target_os = "macos") {
            var("HOME")?.join("Library/Application Support")
        } else {
            var("XDG_DATA_HOME").or_else(|| Some(var("HOME")?.join(".local/share")))?
        };
        Some(dir.join("notely").join(name))
    }

    pub fn load(name: &str) -> Option<String> {
        std::fs::read_to_string(file(name)?).ok()
    }

    pub fn save(name: &str, data: &str) -> Result<(), String> {
        let path = file(name).ok_or("no data directory")?;
        // Write beside the file and rename, so a crash cannot leave half a document.
        let tmp = path.with_extension("tmp");
        path.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&tmp, data))
            .and_then(|()| std::fs::rename(&tmp, &path))
            .map_err(|err| format!("{}: {err}", path.display()))
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::DOCUMENT;

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    /// The key `name` is kept under. A pull request's preview shares the
    /// site's origin, so it keeps copies of its own, by its path, rather
    /// than read and overwrite the site's.
    ///
    /// A convenience for developers trying previews, not a promise: no
    /// isolation, migration or cleanup of preview data is owed, and
    /// changes need not preserve them. Drop or rework it freely.
    fn key(name: &str) -> String {
        let key = if name == DOCUMENT { "notely:doc".to_owned() } else { format!("notely:{name}") };
        let path = web_sys::window().and_then(|w| w.location().pathname().ok()).unwrap_or_default();
        if path.contains("/pr-preview/") {
            format!("{key}:{path}")
        } else {
            key
        }
    }

    pub fn load(name: &str) -> Option<String> {
        storage()?.get_item(&key(name)).ok()?
    }

    pub fn save(name: &str, data: &str) -> Result<(), String> {
        let storage = storage().ok_or("localStorage is not available")?;
        storage.set_item(&key(name), data).map_err(|_| "localStorage refused the write".to_owned())
    }
}

pub use imp::{load, save};
