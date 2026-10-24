//! Small text files that survive restarts: under the data dir on desktop
//! (`$XDG_DATA_HOME/typepper/<name>`), in localStorage on the web.

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::path::PathBuf;

    fn path(name: &str) -> Option<PathBuf> {
        let var = |k| std::env::var_os(k).map(PathBuf::from);
        let dir = var("XDG_DATA_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".local/share")))
            .or_else(|| var("APPDATA"))?;
        Some(dir.join("typepper").join(name))
    }

    pub fn read(name: &str) -> Option<String> {
        std::fs::read_to_string(path(name)?).ok()
    }

    pub fn write(name: &str, text: &str) -> std::io::Result<()> {
        let p = path(name).ok_or(std::io::ErrorKind::NotFound)?;
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(p, text)
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    fn local() -> Option<ratzilla::web_sys::Storage> {
        ratzilla::web_sys::window()?.local_storage().ok()?
    }

    pub fn read(name: &str) -> Option<String> {
        local()?.get_item(&format!("typepper.{name}")).ok()?
    }

    pub fn write(name: &str, text: &str) -> Result<(), ()> {
        local()
            .ok_or(())?
            .set_item(&format!("typepper.{name}"), text)
            .map_err(|_| ())
    }
}

pub use imp::{read, write};
