use std::path::Path;

/// Opens the folder in the Finder. Failure is silent: the user can't act on
/// it anyway.
pub fn open_folder(path: &Path) {
    let _ = std::process::Command::new("open").arg(path).spawn();
}
