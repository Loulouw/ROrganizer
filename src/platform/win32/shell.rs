use std::path::Path;

pub fn open_folder(path: &Path) {
    // `explorer.exe <path>` is the lightest way to open a folder in
    // Explorer without pulling in `Win32_UI_Shell` just for this.
    // Failure is silent — the user can't act on it anyway.
    let _ = std::process::Command::new("explorer.exe")
        .arg(path)
        .spawn();
}
