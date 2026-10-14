//! Where the document is kept: a file in the user data directory natively,
//! `localStorage` in the browser.

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::path::PathBuf;

    /// `document.notely` in the platform's per-user data directory.
    fn file() -> Option<PathBuf> {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        let dir = if cfg!(target_os = "windows") {
            var("APPDATA")?
        } else if cfg!(target_os = "macos") {
            var("HOME")?.join("Library/Application Support")
        } else {
            var("XDG_DATA_HOME").or_else(|| Some(var("HOME")?.join(".local/share")))?
        };
        Some(dir.join("notely").join("document.notely"))
    }

    pub fn load() -> Option<String> {
        std::fs::read_to_string(file()?).ok()
    }

    pub fn save(data: &str) -> Result<(), String> {
        let path = file().ok_or("no data directory")?;
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
    const KEY: &str = "notely:doc";

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn load() -> Option<String> {
        storage()?.get_item(KEY).ok()?
    }

    pub fn save(data: &str) -> Result<(), String> {
        let storage = storage().ok_or("localStorage is not available")?;
        storage.set_item(KEY, data).map_err(|_| "localStorage refused the write".to_owned())
    }
}

pub use imp::{load, save};
