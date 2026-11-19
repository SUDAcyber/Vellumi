//! External changes (Swift: NSFilePresenter + `reloadIfCleanAndChangedOnDisk`).
//!
//! The parent folder of every open file is watched (atomic saves by other
//! editors replace the file, so watching the file itself misses them). On a
//! change the fingerprint decides: our own writes match it and are ignored;
//! a clean document reloads silently; a dirty one gets a conflict banner.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::windows::{self, eval, js_string};

/// Coalesces the burst of events one save produces.
const SETTLE: Duration = Duration::from_millis(300);

pub struct WatchState {
    watcher: Mutex<Option<RecommendedWatcher>>,
    folders: Mutex<HashMap<PathBuf, usize>>,
    pending: Mutex<HashSet<PathBuf>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExternalChange {
    /// A clean document was replaced with the disk version.
    Reloaded { body: String, full_text: String },
    /// The file changed on disk while there are unsaved edits.
    Conflict,
    /// The file was deleted or moved away.
    Deleted,
}

pub fn init(app: &AppHandle) {
    let handle = app.clone();
    let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| match result {
        Ok(event) if !matches!(event.kind, EventKind::Access(_)) => {
            for path in event.paths {
                schedule(&handle, path);
            }
        }
        Ok(_) => {}
        Err(error) => eprintln!("[vellumi] watcher error: {error}"),
    });
    let watcher = match watcher {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            eprintln!("[vellumi] file watching unavailable: {error}");
            None
        }
    };
    app.manage(WatchState {
        watcher: Mutex::new(watcher),
        folders: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashSet::new()),
    });
}

fn folder_of(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    Some(std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf()))
}

/// Starts watching the folder holding `path` (reference counted).
pub fn watch(app: &AppHandle, path: &Path) {
    let Some(state) = app.try_state::<WatchState>() else { return };
    let Some(folder) = folder_of(path) else { return };
    let mut folders = state.folders.lock().unwrap_or_else(PoisonError::into_inner);
    let count = folders.entry(folder.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        let mut watcher = state.watcher.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(watcher) = watcher.as_mut() {
            if let Err(error) = watcher.watch(&folder, RecursiveMode::NonRecursive) {
                eprintln!("[vellumi] cannot watch {}: {error}", folder.display());
            }
        }
    }
}

pub fn unwatch(app: &AppHandle, path: &Path) {
    let Some(state) = app.try_state::<WatchState>() else { return };
    let Some(folder) = folder_of(path) else { return };
    let mut folders = state.folders.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(count) = folders.get_mut(&folder) else { return };
    *count -= 1;
    if *count == 0 {
        folders.remove(&folder);
        let mut watcher = state.watcher.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(watcher) = watcher.as_mut() {
            let _ = watcher.unwatch(&folder);
        }
    }
}

fn schedule(app: &AppHandle, path: PathBuf) {
    let Some(state) = app.try_state::<WatchState>() else { return };
    let fresh = state.pending.lock().unwrap_or_else(PoisonError::into_inner).insert(path.clone());
    if !fresh {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(SETTLE);
        if let Some(state) = app.try_state::<WatchState>() {
            state.pending.lock().unwrap_or_else(PoisonError::into_inner).remove(&path);
        }
        check(&app, &path);
    });
}

/// Same file even when one side no longer exists (compare canonical parent
/// plus file name).
fn same_file(a: &Path, b: &Path) -> bool {
    a.file_name() == b.file_name() && folder_of(a) == folder_of(b)
}

fn check(app: &AppHandle, changed: &Path) {
    let labels: Vec<String> = {
        let state = app.state::<AppState>();
        let docs = state.docs();
        docs.iter()
            .filter(|(_, entry)| entry.session.path().is_some_and(|path| same_file(path, changed)))
            .map(|(label, _)| label.clone())
            .collect()
    };
    for label in labels {
        handle(app, &label);
    }
}

fn emit(app: &AppHandle, label: &str, change: &ExternalChange) {
    if let Err(error) = app.emit_to(label, "doc:external", change) {
        eprintln!("[vellumi] emit doc:external failed: {error}");
    }
}

pub fn handle(app: &AppHandle, label: &str) {
    let state = app.state::<AppState>();
    let (path, known, dirty, writing, conflict_shown) = {
        let docs = state.docs();
        let Some(entry) = docs.get(label) else { return };
        let Some(path) = entry.session.path().cloned() else { return };
        (
            path,
            entry.session.fingerprint(),
            entry.session.is_dirty(),
            entry.writing,
            entry.conflict_shown,
        )
    };
    if writing {
        return;
    }
    let Ok(on_disk) = vellumi_fs::fingerprint(&path) else {
        if !path.exists() {
            emit(app, label, &ExternalChange::Deleted);
        }
        return;
    };
    if Some(on_disk) == known {
        return;
    }
    if dirty {
        if !conflict_shown {
            if let Some(entry) = state.docs().get_mut(label) {
                entry.conflict_shown = true;
            }
            emit(app, label, &ExternalChange::Conflict);
        }
        return;
    }
    if let Some(change) = reload(app, label) {
        emit(app, label, &change);
    }
}

/// Replaces the document with the disk version and pushes it into the
/// editor. Used for silent reloads, "Reload" on a conflict, and Revert.
pub fn reload(app: &AppHandle, label: &str) -> Option<ExternalChange> {
    let state = app.state::<AppState>();
    let path = state.docs().get(label)?.session.path().cloned()?;
    let (bytes, fingerprint) = match vellumi_fs::read_limited(&path, vellumi_core::MAX_FILE_SIZE) {
        Ok(read) => read,
        Err(error) => {
            eprintln!("[vellumi] reload failed: {error}");
            return None;
        }
    };
    let (body, full_text, source_mode) = {
        let mut docs = state.docs();
        let entry = docs.get_mut(label)?;
        match entry.session.reload(&bytes, fingerprint) {
            Ok(_) => {}
            Err(error) => {
                eprintln!("[vellumi] reload failed: {error}");
                return None;
            }
        }
        entry.conflict_shown = false;
        (
            entry.session.body().to_owned(),
            entry.session.full_text(),
            entry.session.source_mode(),
        )
    };
    if !source_mode {
        if let (Some(window), Some(markdown)) = (app.get_webview_window(label), js_string(&body)) {
            // Rebuilding the editor also resets its undo history, so stale
            // entries can never replay onto the reloaded text.
            eval(&window, &format!("window.moltenAPI.setMarkdown({markdown});"));
        }
    }
    windows::publish(app, label);
    Some(ExternalChange::Reloaded { body, full_text })
}
