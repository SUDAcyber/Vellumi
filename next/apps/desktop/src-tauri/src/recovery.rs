//! Crash recovery for untitled documents (Swift: NSDocument "Autosave
//! Information"). Titled documents autosave in place; untitled ones get a
//! copy in the app data folder that is restored, still dirty, on next launch
//! and deleted once the document is saved or deliberately discarded.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

use crate::state::headless;

fn folder(app: &AppHandle) -> Option<PathBuf> {
    if headless() {
        return None;
    }
    app.path().app_data_dir().ok().map(|dir| dir.join("recovery"))
}

pub fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("untitled-{nanos}-{}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

pub fn write(app: &AppHandle, id: &str, text: &str) {
    let Some(dir) = folder(app) else { return };
    if let Err(error) = vellumi_fs::create_dir_all(&dir) {
        eprintln!("[vellumi] {error}");
        return;
    }
    if let Err(error) = vellumi_fs::write_atomic(&dir.join(format!("{id}.md")), text.as_bytes()) {
        eprintln!("[vellumi] recovery copy failed: {error}");
    }
}

pub fn remove(app: &AppHandle, id: &str) {
    let Some(dir) = folder(app) else { return };
    let _ = std::fs::remove_file(dir.join(format!("{id}.md")));
}

/// Recovery copies left by a previous run, oldest first.
pub fn pending(app: &AppHandle) -> Vec<(String, String)> {
    let Some(dir) = folder(app) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut found: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let id = path.file_stem()?.to_string_lossy().into_owned();
            if path.extension()? != "md" || !id.starts_with("untitled-") {
                return None;
            }
            let (bytes, _) = vellumi_fs::read_limited(&path, vellumi_core::MAX_FILE_SIZE).ok()?;
            let (text, _) = vellumi_core::encoding::decode(&bytes).ok()?;
            Some((id, text))
        })
        .collect();
    found.sort();
    found
}
