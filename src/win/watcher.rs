use std::collections::HashSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use regex::Regex;

use super::enumerate::{enumerate_dofus_windows, DetectedWindow};
use super::process::ExeCache;

pub type WindowsSnapshot = Arc<Mutex<Vec<DetectedWindow>>>;

const POLL_INTERVAL: Duration = Duration::from_millis(1500);

pub fn spawn_watcher(
    snapshot: WindowsSnapshot,
    ctx: egui::Context,
    regex_src: String,
    refresh_rx: Receiver<()>,
) {
    let regex = Regex::new(&regex_src)
        .unwrap_or_else(|e| panic!("RORGANIZER_TITLE_REGEX invalid: {e}"));

    std::thread::Builder::new()
        .name("rorg-watcher".into())
        .spawn(move || run(snapshot, ctx, regex, refresh_rx))
        .expect("failed to spawn watcher thread");
}

fn run(snapshot: WindowsSnapshot, ctx: egui::Context, regex: Regex, refresh_rx: Receiver<()>) {
    let mut buf: Vec<DetectedWindow> = Vec::with_capacity(8);
    let mut exe_cache: ExeCache = ExeCache::with_capacity(256);
    let mut seen_pids: HashSet<u32> = HashSet::with_capacity(256);

    loop {
        enumerate_dofus_windows(&regex, &mut buf, &mut exe_cache, &mut seen_pids);
        // Evict stale entries: any pid whose visible window vanished this tick.
        // Keeps the cache from growing unboundedly and from holding stale
        // mappings if Windows recycles a pid.
        exe_cache.retain(|pid, _| seen_pids.contains(pid));

        {
            let mut guard = snapshot.lock().expect("snapshot mutex poisoned");
            guard.clear();
            guard.extend(buf.iter().cloned());
        }
        ctx.request_repaint();

        match refresh_rx.recv_timeout(POLL_INTERVAL) {
            Ok(_) | Err(RecvTimeoutError::Timeout) => {
                while refresh_rx.try_recv().is_ok() {}
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
