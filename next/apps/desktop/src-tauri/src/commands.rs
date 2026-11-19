//! Commands the shell UI invokes (types mirrored in ui/src/lib/ipc.ts).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use vellumi_core::settings::Settings;
use vellumi_core::tree::{FileMatches, TreeNode};
use vellumi_core::{fuzzy, CoreError};

use crate::error::{AppError, AppResult};
use crate::state::{headless, selftest_report, AppState};
use crate::windows::{self, DocInfo};

fn label_of(window: &WebviewWindow) -> String {
    window.label().to_owned()
}

fn with_entry<T>(app: &AppHandle, label: &str, f: impl FnOnce(&mut crate::state::DocEntry) -> T) -> AppResult<T> {
    let state = app.state::<AppState>();
    let mut docs = state.docs();
    let entry = docs.get_mut(label).ok_or_else(|| AppError::UnknownWindow(label.to_owned()))?;
    Ok(f(entry))
}

fn current_folder(app: &AppHandle, label: &str) -> Option<PathBuf> {
    let state = app.state::<AppState>();
    let docs = state.docs();
    docs.get(label)?.session.path()?.parent().map(Path::to_path_buf)
}

#[tauri::command]
pub async fn doc_info(app: AppHandle, window: WebviewWindow) -> AppResult<DocInfo> {
    with_entry(&app, &label_of(&window), |entry| windows::doc_info(entry))
}

#[tauri::command]
pub async fn doc_set_front_matter(app: AppHandle, window: WebviewWindow, text: String) -> AppResult<DocInfo> {
    let label = label_of(&window);
    with_entry(&app, &label, |entry| entry.session.set_front_matter(&text))?.map_err(CoreError::from)?;
    windows::publish(&app, &label);
    with_entry(&app, &label, |entry| windows::doc_info(entry))
}

/// ⌘/ in: adopts the editor's last text, returns the full file text.
#[tauri::command]
pub async fn doc_source_enter(app: AppHandle, window: WebviewWindow, markdown: Option<String>) -> AppResult<String> {
    let label = label_of(&window);
    let text = with_entry(&app, &label, |entry| {
        if let Some(markdown) = markdown {
            entry.session.editor_changed(&markdown);
        }
        entry.session.set_source_mode(true);
        entry.session.full_text()
    })?;
    windows::publish(&app, &label);
    Ok(text)
}

#[tauri::command]
pub async fn doc_source_changed(app: AppHandle, window: WebviewWindow, text: String) -> AppResult<()> {
    let label = label_of(&window);
    let changed = with_entry(&app, &label, |entry| entry.session.source_changed(&text).changed)?;
    if changed {
        windows::publish(&app, &label);
    }
    Ok(())
}

/// ⌘/ out: adopts the source text, returns the body for the rich editor.
#[tauri::command]
pub async fn doc_source_exit(app: AppHandle, window: WebviewWindow, text: String) -> AppResult<String> {
    let label = label_of(&window);
    let body = with_entry(&app, &label, |entry| {
        entry.session.source_changed(&text);
        entry.session.set_source_mode(false);
        entry.session.body().to_owned()
    })?;
    windows::publish(&app, &label);
    Ok(body)
}

/// File ▸ Revert to Saved, after confirmation. Returns false when cancelled
/// or impossible (untitled).
#[tauri::command]
pub async fn doc_revert(app: AppHandle, window: WebviewWindow) -> AppResult<bool> {
    let label = label_of(&window);
    let (titled, dirty, name) = with_entry(&app, &label, |entry| {
        (
            entry.session.path().is_some(),
            entry.session.is_dirty(),
            windows::display_name(&entry.session),
        )
    })?;
    if !titled {
        return Ok(false);
    }
    if dirty && !headless() {
        let (message, ok, cancel) = if windows::is_chinese() {
            (format!("要将“{name}”恢复为上次存储的版本吗？当前的更改将丢失。"), "恢复", "取消")
        } else {
            (format!("Revert “{name}” to the last saved version? Current changes will be lost."), "Revert", "Cancel")
        };
        let confirmed = app
            .dialog()
            .message(message)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(ok.to_owned(), cancel.to_owned()))
            .blocking_show();
        if !confirmed {
            return Ok(false);
        }
    }
    Ok(crate::watcher::reload(&app, &label).is_some())
}

/// Conflict banner ▸ Reload: take the disk version, dropping unsaved edits.
#[tauri::command]
pub async fn doc_reload_from_disk(app: AppHandle, window: WebviewWindow) -> AppResult<bool> {
    Ok(crate::watcher::reload(&app, &label_of(&window)).is_some())
}

/// Conflict banner ▸ Keep Mine: the next save overwrites the disk version.
#[tauri::command]
pub async fn doc_keep_mine(app: AppHandle, window: WebviewWindow) -> AppResult<()> {
    let label = label_of(&window);
    let path = with_entry(&app, &label, |entry| entry.session.path().cloned())?;
    let fingerprint = match path {
        Some(path) => Some(vellumi_fs::fingerprint(&path)?),
        None => None,
    };
    with_entry(&app, &label, |entry| {
        if let Some(fingerprint) = fingerprint {
            entry.session.acknowledge_disk_version(fingerprint);
        }
        entry.conflict_shown = false;
    })?;
    Ok(())
}

/// File tree of `folder`, or of the document's folder.
#[tauri::command]
pub async fn tree(app: AppHandle, window: WebviewWindow, folder: Option<String>) -> AppResult<Vec<TreeNode>> {
    let folder = folder.map(PathBuf::from).or_else(|| current_folder(&app, &label_of(&window)));
    Ok(folder.map(|folder| vellumi_fs::build_tree(&folder)).unwrap_or_default())
}

/// Find in files across every project plus the document's folder.
#[tauri::command]
pub async fn search(app: AppHandle, window: WebviewWindow, query: String, case_sensitive: bool) -> AppResult<Vec<FileMatches>> {
    let folder = current_folder(&app, &label_of(&window));
    Ok(crate::projects::search(&app, &query, case_sensitive, folder.as_deref()))
}

#[tauri::command]
pub async fn index_status(app: AppHandle) -> AppResult<crate::projects::IndexStatus> {
    Ok(crate::projects::status(&app))
}

#[tauri::command]
pub async fn projects_list(app: AppHandle) -> AppResult<Vec<crate::projects::ProjectInfo>> {
    let projects = app.state::<AppState>().settings().projects.clone();
    Ok(projects.iter().map(crate::projects::describe).collect())
}

fn pick_folder(app: &AppHandle, title: &str) -> Option<PathBuf> {
    if headless() {
        return None;
    }
    app.dialog()
        .file()
        .set_title(title)
        .blocking_pick_folder()
        .and_then(|picked| picked.into_path().ok())
}

/// Adds a folder as a project (`path`, or a folder picker when absent).
#[tauri::command]
pub async fn projects_add(app: AppHandle, path: Option<String>) -> AppResult<Option<vellumi_core::settings::Project>> {
    let title = if windows::is_chinese() { "选择要作为项目的文件夹" } else { "Choose a folder to add as a project" };
    let Some(folder) = path.map(PathBuf::from).or_else(|| pick_folder(&app, title)) else {
        return Ok(None);
    };
    let canonical = std::fs::canonicalize(&folder).map_err(|source| CoreError::Io {
        op: "add project",
        path: folder.clone(),
        source,
    })?;
    let name = canonical
        .file_name()
        .map_or_else(|| canonical.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
    let path = canonical.to_string_lossy().into_owned();
    let id = format!("p{}", crate::projects::now_ms());
    let mut added = None;
    crate::settings_store::update(&app, |settings| {
        added = Some(settings.add_project(&path, &name, &id, crate::projects::now_ms()));
    });
    crate::projects::resync(&app);
    Ok(added)
}

#[tauri::command]
pub async fn projects_remove(app: AppHandle, id: String) -> AppResult<()> {
    crate::settings_store::update(&app, |settings| settings.remove_project(&id));
    crate::projects::resync(&app);
    Ok(())
}

#[tauri::command]
pub async fn projects_rename(app: AppHandle, id: String, name: String) -> AppResult<()> {
    crate::settings_store::update(&app, |settings| settings.rename_project(&id, &name));
    Ok(())
}

/// A project whose folder moved: pick its new location.
#[tauri::command]
pub async fn projects_relocate(app: AppHandle, id: String) -> AppResult<bool> {
    let title = if windows::is_chinese() { "找到这个项目的新位置" } else { "Locate the project's folder" };
    let Some(folder) = pick_folder(&app, title) else { return Ok(false) };
    let path = std::fs::canonicalize(&folder).unwrap_or(folder).to_string_lossy().into_owned();
    crate::settings_store::update(&app, |settings| settings.relocate_project(&id, &path));
    crate::projects::resync(&app);
    Ok(true)
}

/// New Note in a project: a real, empty file up front (so autosave owns it
/// from the first keystroke), deleted again if it is closed untouched.
#[tauri::command]
pub async fn project_new_note(app: AppHandle, id: String) -> AppResult<()> {
    let folder = app
        .state::<AppState>()
        .settings()
        .projects
        .iter()
        .find(|p| p.id == id)
        .map(|p| PathBuf::from(&p.path))
        .ok_or_else(|| AppError::Ipc(format!("no project {id}")))?;
    windows::new_note_in(&app, &folder)?;
    Ok(())
}

/// Settings ▸ Files ▸ Drafts folder: choose (`clear` = false) or turn off.
#[tauri::command]
pub async fn settings_pick_drafts(app: AppHandle, clear: bool) -> AppResult<Settings> {
    if clear {
        return Ok(crate::settings_store::update(&app, |s| s.drafts_folder = None));
    }
    let title = if windows::is_chinese() { "选择草稿文件夹" } else { "Choose a Drafts folder" };
    match pick_folder(&app, title) {
        Some(folder) => {
            let path = folder.to_string_lossy().into_owned();
            Ok(crate::settings_store::update(&app, |s| s.drafts_folder = Some(path)))
        }
        None => Ok(app.state::<AppState>().settings().clone()),
    }
}

#[tauri::command]
pub async fn open_projects_window(app: AppHandle) -> AppResult<()> {
    windows::open_projects(&app);
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub recent: bool,
}

/// Quick open: the document's folder tree plus recent documents, fuzzy
/// ranked by file name (empty query: recents first).
#[tauri::command]
pub async fn quick_open(app: AppHandle, window: WebviewWindow, query: String) -> AppResult<Vec<FileHit>> {
    let recents = app.state::<AppState>().settings().recents.clone();
    let mut candidates: Vec<(String, bool)> = recents
        .into_iter()
        .filter(|path| Path::new(path).is_file())
        .map(|path| (path, true))
        .collect();
    candidates.extend(crate::projects::all_files(&app).into_iter().map(|path| (path, false)));
    if let Some(folder) = current_folder(&app, &label_of(&window)) {
        let root = TreeNode {
            name: String::new(),
            path: folder.to_string_lossy().into_owned(),
            is_dir: true,
            children: vellumi_fs::build_tree(&folder),
        };
        candidates.extend(root.files().into_iter().map(|node| (node.path.clone(), false)));
    }
    let mut seen = std::collections::HashSet::new();
    let mut scored: Vec<(i64, FileHit)> = candidates
        .into_iter()
        .filter(|(path, _)| seen.insert(crate::projects::canonical(Path::new(path))))
        .filter_map(|(path, recent)| {
            let as_path = Path::new(&path);
            let name = as_path.file_name()?.to_string_lossy().into_owned();
            let score = fuzzy::score(&query, &name)?;
            let bonus = if recent && query.is_empty() { 1_000_000 } else { 0 };
            let folder = as_path
                .parent()
                .and_then(Path::file_name)
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            Some((score + bonus, FileHit { name, path, folder, recent }))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    Ok(scored.into_iter().take(50).map(|(_, hit)| hit).collect())
}

#[tauri::command]
pub async fn open_path(app: AppHandle, path: String) -> AppResult<()> {
    windows::open_path(&app, Path::new(&path));
    Ok(())
}

#[tauri::command]
pub async fn reveal_in_finder(path: String) -> AppResult<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|source| CoreError::Io {
            op: "reveal in Finder",
            path: PathBuf::from(&path),
            source,
        })?;
    Ok(())
}

#[tauri::command]
pub async fn settings_get(app: AppHandle) -> AppResult<Settings> {
    Ok(app.state::<AppState>().settings().clone())
}

/// Merges a partial settings object (camelCase keys) and applies it.
#[tauri::command]
pub async fn settings_patch(app: AppHandle, patch: serde_json::Value) -> AppResult<Settings> {
    let current = app.state::<AppState>().settings().clone();
    let mut merged = serde_json::to_value(&current).map_err(|error| AppError::Ipc(error.to_string()))?;
    if let (Some(target), Some(source)) = (merged.as_object_mut(), patch.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
    }
    let next: Settings = serde_json::from_value(merged).map_err(|error| AppError::Ipc(error.to_string()))?;
    Ok(crate::settings_store::update(&app, move |settings| *settings = next))
}

/// Runs a menu command by id (the command palette).
#[tauri::command]
pub async fn run_command(app: AppHandle, window: WebviewWindow, id: String) -> AppResult<()> {
    crate::menu::dispatch(&app, &id, Some(window.label()));
    Ok(())
}

/// Self-test only: plays "another app" writing the open file on disk.
/// `snapshot` remembers the current bytes, `write` replaces the file,
/// `restore` puts the snapshot back. Refused outside the self-test.
#[tauri::command]
pub async fn selftest_external(app: AppHandle, window: WebviewWindow, action: String, text: Option<String>) -> AppResult<()> {
    static SNAPSHOT: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
    if selftest_report().is_none() {
        return Err(AppError::Ipc("only available in the self-test".into()));
    }
    let path = with_entry(&app, &label_of(&window), |entry| entry.session.path().cloned())?
        .ok_or_else(|| AppError::Ipc("document has no file".into()))?;
    let io = |source| CoreError::Io {
        op: "self-test external write",
        path: path.clone(),
        source,
    };
    let mut snapshot = SNAPSHOT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match action.as_str() {
        "snapshot" => *snapshot = Some(std::fs::read(&path).map_err(io)?),
        "write" => std::fs::write(&path, text.unwrap_or_default()).map_err(io)?,
        "restore" => {
            if let Some(bytes) = snapshot.as_ref() {
                std::fs::write(&path, bytes).map_err(io)?;
            }
        }
        other => return Err(AppError::Ipc(format!("unknown action {other}"))),
    }
    Ok(())
}

/// Self-test only: reads back a file the test exported next to the document
/// (`text` = lossy UTF-8 of at most 2 MB, `size` = bytes on disk).
#[tauri::command]
pub async fn selftest_read(app: AppHandle, window: WebviewWindow, name: String) -> AppResult<serde_json::Value> {
    if selftest_report().is_none() || name.contains('/') || name.contains("..") {
        return Err(AppError::Ipc("only available in the self-test".into()));
    }
    let folder = current_folder(&app, &label_of(&window)).ok_or_else(|| AppError::Ipc("no folder".into()))?;
    let path = folder.join(&name);
    let Ok((bytes, _)) = vellumi_fs::read_limited(&path, 64 * 1024 * 1024) else {
        return Ok(serde_json::json!({ "exists": false }));
    };
    let head: Vec<u8> = bytes.iter().take(2 * 1024 * 1024).copied().collect();
    Ok(serde_json::json!({
        "exists": true,
        "size": bytes.len(),
        "text": String::from_utf8_lossy(&head),
        "magic": bytes.iter().take(4).map(|b| format!("{b:02x}")).collect::<String>(),
    }))
}

/// Shell diagnostics (uncaught errors, self-test progress) into the app log.
#[tauri::command]
pub async fn ui_log(window: WebviewWindow, level: String, message: String) -> AppResult<()> {
    eprintln!("[ui:{}:{level}] {message}", window.label());
    Ok(())
}

/// Self-test harness: the shell's report, written for scripts/selftest.sh.
#[tauri::command]
pub async fn selftest_done(app: AppHandle, window: WebviewWindow, report: serde_json::Value) -> AppResult<()> {
    let Some(target) = selftest_report() else { return Ok(()) };
    let path = with_entry(&app, &label_of(&window), |entry| entry.session.path().cloned()).ok().flatten();
    let on_disk = path
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    let document = serde_json::json!({ "report": report, "fileOnDisk": on_disk });
    let bytes = serde_json::to_vec_pretty(&document).map_err(|error| AppError::Ipc(error.to_string()))?;
    std::fs::write(target, bytes).map_err(|source| CoreError::Io {
        op: "write self-test report",
        path: target.clone(),
        source,
    })?;
    app.exit(0);
    Ok(())
}
