//! Petits fichiers texte persistants (options, sauvegarde) :
//! - natif : `<nom>.ron` dans le dossier de config de l'OS
//!   (`~/.config/giants-flame/`, `%APPDATA%\giants-flame\`, `~/Library/Application Support/giants-flame/`) ;
//! - web : `localStorage` du navigateur, clé `giants-flame.<nom>`.

#[cfg(target_arch = "wasm32")]
fn local() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn read(name: &str) -> Option<String> {
    local()?.get_item(&format!("giants-flame.{name}")).ok()?
}

#[cfg(target_arch = "wasm32")]
pub fn write(name: &str, s: &str) {
    if let Some(l) = local() {
        let _ = l.set_item(&format!("giants-flame.{name}"), s);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn path(name: &str) -> Option<std::path::PathBuf> {
    use std::env::var_os;
    use std::path::PathBuf;
    let base = if cfg!(target_os = "windows") {
        var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    }?;
    Some(base.join("giants-flame").join(format!("{name}.ron")))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read(name: &str) -> Option<String> {
    std::fs::read_to_string(path(name)?).ok()
}

/// Écrit via un fichier temporaire puis renomme : un arrêt brutal ne laisse pas de fichier tronqué.
#[cfg(not(target_arch = "wasm32"))]
pub fn write(name: &str, s: &str) {
    let Some(p) = path(name) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = p.with_extension("ron.tmp");
    if let Err(e) = std::fs::write(&tmp, s).and_then(|_| std::fs::rename(&tmp, &p)) {
        bevy::log::warn!("impossible d'écrire {}: {e}", p.display());
    }
}
