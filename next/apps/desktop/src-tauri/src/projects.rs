//! Projects (Swift: MoltenProjectStore / Projects window) and the full-text
//! index over them. Indexing runs on a background thread; until it is ready,
//! and for folders outside every project, search falls back to a linear scan
//! that returns the same results.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use vellumi_core::settings::Project;
use vellumi_core::tree::FileMatches;
use vellumi_index::{SearchIndex, INDEX_VERSION};

use crate::state::{headless, AppState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IndexState {
    Starting,
    Indexing,
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub state: IndexState,
    pub files: u64,
}

pub struct ProjectsState {
    index: Mutex<Option<Arc<SearchIndex>>>,
    status: Mutex<IndexStatus>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    watched: Mutex<Vec<PathBuf>>,
    pending: Mutex<HashSet<PathBuf>>,
    /// Bumped on every resync so a stale background pass never wins.
    generation: Mutex<u64>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn index_folder(app: &AppHandle) -> Option<PathBuf> {
    if headless() {
        // Tests get a private, throwaway index.
        return Some(std::env::temp_dir().join(format!("vellumi-test-index-{}", std::process::id())));
    }
    app.path()
        .app_cache_dir()
        .ok()
        .map(|dir| dir.join(format!("search-index-v{INDEX_VERSION}")))
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Folders of all projects that currently exist.
pub fn roots(app: &AppHandle) -> Vec<PathBuf> {
    app.state::<AppState>()
        .settings()
        .projects
        .iter()
        .map(|project| PathBuf::from(&project.path))
        .filter(|path| path.is_dir())
        .collect()
}

pub fn status(app: &AppHandle) -> IndexStatus {
    app.try_state::<ProjectsState>()
        .map_or(IndexStatus { state: IndexState::Unavailable, files: 0 }, |s| lock(&s.status).clone())
}

fn set_status(app: &AppHandle, state: IndexState) {
    let Some(projects) = app.try_state::<ProjectsState>() else { return };
    let files = lock(&projects.index).as_ref().map_or(0, |index| index.document_count());
    let status = IndexStatus { state, files };
    *lock(&projects.status) = status.clone();
    if let Err(error) = app.emit("index:status", &status) {
        eprintln!("[vellumi] emit index:status failed: {error}");
    }
}

pub fn init(app: &AppHandle) {
    app.manage(ProjectsState {
        index: Mutex::new(None),
        status: Mutex::new(IndexStatus {
            state: IndexState::Starting,
            files: 0,
        }),
        watcher: Mutex::new(None),
        watched: Mutex::new(Vec::new()),
        pending: Mutex::new(HashSet::new()),
        generation: Mutex::new(0),
    });
    let handle = app.clone();
    let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            if !matches!(event.kind, EventKind::Access(_)) {
                for path in event.paths {
                    schedule_update(&handle, path);
                }
            }
        }
    });
    if let Some(state) = app.try_state::<ProjectsState>() {
        *lock(&state.watcher) = watcher.ok();
    }
    resync(app);
}

/// Re-watches the project folders and brings the index up to date in the
/// background (startup, and whenever the project list changes).
pub fn resync(app: &AppHandle) {
    let Some(state) = app.try_state::<ProjectsState>() else { return };
    let generation = {
        let mut generation = lock(&state.generation);
        *generation += 1;
        *generation
    };
    let roots = roots(app);
    if let Some(watcher) = lock(&state.watcher).as_mut() {
        // Rewatch from scratch: cheap, and the project list is short.
        let mut watched = lock(&state.watched);
        for old in watched.drain(..) {
            let _ = watcher.unwatch(&old);
        }
        for root in &roots {
            match watcher.watch(root, RecursiveMode::Recursive) {
                Ok(()) => watched.push(root.clone()),
                Err(error) => eprintln!("[vellumi] cannot watch project {}: {error}", root.display()),
            }
        }
    }
    let app = app.clone();
    thread::spawn(move || {
        let Some(state) = app.try_state::<ProjectsState>() else { return };
        if lock(&state.index).is_none() {
            let opened = index_folder(&app).map(|folder| SearchIndex::open(&folder));
            match opened {
                Some(Ok(index)) => *lock(&state.index) = Some(Arc::new(index)),
                Some(Err(error)) => {
                    eprintln!("[vellumi] search index unavailable, using linear search: {error}");
                    set_status(&app, IndexState::Unavailable);
                    return;
                }
                None => {
                    set_status(&app, IndexState::Unavailable);
                    return;
                }
            }
        }
        set_status(&app, IndexState::Indexing);
        let index = lock(&state.index).clone();
        if let Some(index) = index {
            match index.sync(&roots) {
                Ok(stats) => {
                    if *lock(&state.generation) == generation {
                        eprintln!(
                            "[vellumi] index synced: {} indexed, {} removed, {} unchanged",
                            stats.indexed, stats.removed, stats.unchanged
                        );
                        set_status(&app, IndexState::Ready);
                    }
                }
                Err(error) => {
                    eprintln!("[vellumi] index sync failed: {error}");
                    set_status(&app, IndexState::Unavailable);
                }
            }
        }
    });
}

fn schedule_update(app: &AppHandle, path: PathBuf) {
    let Some(state) = app.try_state::<ProjectsState>() else { return };
    let interesting = path.is_dir()
        || path
            .file_name()
            .is_some_and(|name| vellumi_core::is_supported_file_name(&name.to_string_lossy()));
    if !interesting || !lock(&state.pending).insert(path.clone()) {
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        let Some(state) = app.try_state::<ProjectsState>() else { return };
        lock(&state.pending).remove(&path);
        let index = lock(&state.index).clone();
        let Some(index) = index else { return };
        let result = if path.is_dir() {
            // A folder appeared, vanished or was renamed: resync everything.
            index.sync(&roots(&app)).map(|_| ())
        } else {
            index.update_file(&path)
        };
        if let Err(error) = result {
            eprintln!("[vellumi] index update failed: {error}");
        } else if status(&app).state == IndexState::Ready {
            set_status(&app, IndexState::Ready);
        }
    });
}

/// Find in files across every project plus `extra` (the document's folder).
pub fn search(app: &AppHandle, query: &str, case_sensitive: bool, extra: Option<&Path>) -> Vec<FileMatches> {
    let roots = roots(app);
    let index = app
        .try_state::<ProjectsState>()
        .filter(|state| lock(&state.status).state == IndexState::Ready)
        .and_then(|state| lock(&state.index).clone());
    let mut results = match index.as_ref().map(|index| index.search(query, case_sensitive, &roots)) {
        Some(Ok(found)) => found,
        Some(Err(error)) => {
            eprintln!("[vellumi] index search failed, scanning instead: {error}");
            roots.iter().flat_map(|root| vellumi_fs::search_folder(root, query, case_sensitive)).collect()
        }
        None => roots.iter().flat_map(|root| vellumi_fs::search_folder(root, query, case_sensitive)).collect(),
    };
    // Compare real paths: /var vs /private/var, symlinked folders.
    let extra = extra.map(canonical);
    if let Some(extra) = extra.filter(|folder| !roots.iter().any(|root| folder.starts_with(root))) {
        results.extend(vellumi_fs::search_folder(&extra, query, case_sensitive));
    }
    let mut seen = HashSet::new();
    results.retain(|file| seen.insert(canonical(Path::new(&file.path))));
    results
}

/// The real path when it can be resolved, otherwise the path as given.
pub fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    pub name: String,
    pub path: String,
    pub modified_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    #[serde(flatten)]
    pub project: Project,
    pub available: bool,
    pub file_count: usize,
    pub recent: Vec<RecentFile>,
}

pub fn describe(project: &Project) -> ProjectInfo {
    let root = Path::new(&project.path);
    if !root.is_dir() {
        return ProjectInfo {
            project: project.clone(),
            available: false,
            file_count: 0,
            recent: Vec::new(),
        };
    }
    let files = vellumi_index::files_under(root);
    let mut recent: Vec<RecentFile> = files
        .iter()
        .map(|path| {
            let modified_ms = vellumi_fs::fingerprint(Path::new(path))
                .map(|fp| u64::try_from(fp.modified_ns / 1_000_000).unwrap_or(u64::MAX))
                .unwrap_or(0);
            RecentFile {
                name: Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: path.clone(),
                modified_ms,
            }
        })
        .collect();
    recent.sort_by_key(|file| std::cmp::Reverse(file.modified_ms));
    recent.truncate(3);
    ProjectInfo {
        project: project.clone(),
        available: true,
        file_count: files.len(),
        recent,
    }
}

/// Every file of every project (Quick Open corpus).
pub fn all_files(app: &AppHandle) -> Vec<String> {
    roots(app).iter().flat_map(|root| vellumi_index::files_under(root)).collect()
}
