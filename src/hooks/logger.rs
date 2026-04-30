use std::fs::OpenOptions;
use std::io::Write;
use std::sync::mpsc::{channel, Sender};

pub fn spawn() -> Sender<String> {
    let (tx, rx) = channel::<String>();
    std::thread::Builder::new()
        .name("rorg-hook-log".into())
        .spawn(move || {
            let path = std::env::temp_dir().join("rorganizer_hooks.log");
            let mut file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(path)
                .ok();
            while let Ok(line) = rx.recv() {
                if let Some(f) = file.as_mut() {
                    let _ = writeln!(f, "{line}");
                }
            }
        })
        .expect("hook log thread");
    tx
}
