//! The editor bridge (Swift: `MoltenEditorViewController` + `MoltenDocument`
//! callbacks) and the save / close handshake.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind, MessageDialogResult};

use crate::error::{AppError, AppResult};
use crate::state::{headless, roundtrip_dir, AppState, CloseState};
use crate::windows::{self, eval, is_chinese, js_string};

/// Messages posted by the editor (contract in packages/editor/src/main.js).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum BridgeMessage {
    Ready,
    Change {
        markdown: String,
    },
    Normalized {
        markdown: String,
        changed: bool,
    },
    BootError {
        message: String,
    },
    ImageSave {
        id: u64,
        name: String,
        base64: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlushReason {
    Save,
    SaveAs,
    Close,
}

const AUTOSAVE_DELAY: Duration = Duration::from_secs(2);
const CLOSE_WATCHDOG: Duration = Duration::from_secs(3);

#[tauri::command]
pub async fn bridge_message(app: AppHandle, window: WebviewWindow, message: BridgeMessage) -> AppResult<()> {
    let label = window.label().to_owned();
    let state = app.state::<AppState>();
    match message {
        BridgeMessage::Ready => {
            let body = {
                let mut docs = state.docs();
                let entry = docs.get_mut(&label).ok_or_else(|| AppError::UnknownWindow(label.clone()))?;
                entry.ready = true;
                entry.session.body().to_owned()
            };
            if let Some(markdown) = js_string(&body) {
                eval(&window, &format!("window.moltenAPI.setMarkdown({markdown});"));
            }
            windows::publish(&app, &label);
        }
        BridgeMessage::Change { markdown } => {
            let changed = {
                let mut docs = state.docs();
                let entry = docs.get_mut(&label).ok_or_else(|| AppError::UnknownWindow(label.clone()))?;
                entry.session.editor_changed(&markdown).changed
            };
            if changed {
                windows::publish(&app, &label);
                schedule_autosave(&app, &label);
            }
        }
        BridgeMessage::Normalized { markdown, changed } => {
            {
                let mut docs = state.docs();
                if let Some(entry) = docs.get_mut(&label) {
                    entry.session.editor_normalized(&markdown, changed);
                }
            }
            windows::publish(&app, &label);
            if roundtrip_dir().is_some() {
                finish_roundtrip(&app, &window, Ok(()));
            }
        }
        BridgeMessage::BootError { message } => {
            eprintln!("[vellumi] editor failed to boot in {label}: {message}");
            if roundtrip_dir().is_some() {
                finish_roundtrip(&app, &window, Err(&message));
            }
        }
        BridgeMessage::ImageSave { id, name, base64 } => {
            let path = match crate::assets::save_image(&app, &label, &name, &base64) {
                Ok(path) => Some(path),
                Err(error) => {
                    eprintln!("[vellumi] image save failed: {error}");
                    if matches!(error, AppError::Core(vellumi_core::CoreError::Untitled)) {
                        let text = if is_chinese() {
                            "请先保存文档，图片会存到文档旁边的文件夹里。"
                        } else {
                            "Save the document first — images are stored in a folder next to it."
                        };
                        if !headless() {
                            app.dialog().message(text).kind(MessageDialogKind::Info).blocking_show();
                        }
                    } else {
                        windows::show_error(&app, &error.to_string());
                    }
                    None
                }
            };
            let path_js = path.as_deref().and_then(js_string).unwrap_or_else(|| "null".to_owned());
            eval(&window, &format!("window.moltenAPI.resolveImageSave({id}, {path_js});"));
        }
        BridgeMessage::Unknown => {}
    }
    Ok(())
}

/// Round-trip harness: write what a save would produce, close, exit when done.
fn finish_roundtrip(app: &AppHandle, window: &WebviewWindow, result: Result<(), &str>) {
    let label = window.label();
    let state = app.state::<AppState>();
    // Write BEFORE leaving the document list: another window finishing at the
    // same moment exits the process as soon as the list is empty, which used
    // to kill this write midway (a missing or truncated fixture output).
    let output = {
        let docs = state.docs();
        docs.get(label).map(|entry| {
            let name = windows::display_name(&entry.session);
            // Exactly what Save would write (edge newlines already conformed).
            match result {
                Ok(()) => (name, entry.session.file_bytes()),
                Err(message) => (format!("{name}.error"), message.as_bytes().to_vec()),
            }
        })
    };
    if let (Some(dir), Some((name, bytes))) = (roundtrip_dir(), output) {
        let target = dir.join(name);
        if let Err(error) = std::fs::write(&target, bytes) {
            eprintln!("[roundtrip] cannot write {}: {error}", target.display());
        }
    }
    if let Err(error) = window.destroy() {
        eprintln!("[roundtrip] destroy failed: {error}");
    }
    // Remove and check emptiness under one lock so exactly one window exits.
    let last = {
        let mut docs = state.docs();
        docs.remove(label);
        docs.is_empty()
    };
    if last {
        app.exit(0);
    }
}

/// The shell's answer to `window.__vellumiFlush(reason)`: the editor's
/// freshest serialization, or the source view's text while it is active.
#[tauri::command]
pub async fn doc_flush(
    app: AppHandle,
    window: WebviewWindow,
    reason: FlushReason,
    markdown: Option<String>,
    source: Option<String>,
) -> AppResult<()> {
    let label = window.label().to_owned();
    {
        let state = app.state::<AppState>();
        let mut docs = state.docs();
        if let Some(entry) = docs.get_mut(&label) {
            match (entry.session.source_mode(), source, markdown) {
                (true, Some(text), _) => {
                    entry.session.source_changed(&text);
                }
                (false, _, Some(markdown)) => {
                    entry.session.editor_changed(&markdown);
                }
                _ => {}
            }
        }
    }
    match reason {
        FlushReason::Save => {
            save(&app, &label, false)?;
        }
        FlushReason::SaveAs => {
            save(&app, &label, true)?;
        }
        FlushReason::Close => {
            if take_close_turn(&app, &label) {
                finish_close(&app, &label);
            }
        }
    }
    windows::publish(&app, &label);
    Ok(())
}

/// Writes the document. `ask_for_path` forces the save panel (Save As);
/// untitled documents always get it. Returns false when the user cancelled.
pub fn save(app: &AppHandle, label: &str, ask_for_path: bool) -> AppResult<bool> {
    let state = app.state::<AppState>();
    let (current_path, snapshot, suggested) = {
        let docs = state.docs();
        let entry = docs.get(label).ok_or_else(|| AppError::UnknownWindow(label.to_owned()))?;
        (
            entry.session.path().cloned(),
            entry.session.snapshot(),
            windows::display_name(&entry.session),
        )
    };
    let target: PathBuf = match current_path.clone() {
        Some(path) if !ask_for_path => path,
        _ => {
            if headless() {
                return Ok(false);
            }
            let file_name = if suggested.contains('.') { suggested } else { format!("{suggested}.md") };
            let mut dialog = app
                .dialog()
                .file()
                .set_file_name(file_name)
                .add_filter("Markdown", &["md", "markdown", "txt"]);
            if let Some(folder) = current_path.as_deref().and_then(std::path::Path::parent) {
                dialog = dialog.set_directory(folder);
            }
            let Some(picked) = dialog.blocking_save_file() else { return Ok(false) };
            match picked.into_path() {
                Ok(path) => path,
                Err(error) => return Err(AppError::Tauri(tauri::Error::Anyhow(error.into()))),
            }
        }
    };
    set_writing(app, label, true);
    let written = vellumi_fs::write_atomic(&target, &snapshot.bytes);
    let fingerprint = match written {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            set_writing(app, label, false);
            return Err(error.into());
        }
    };
    let recovery_id = {
        let mut docs = state.docs();
        let Some(entry) = docs.get_mut(label) else { return Ok(true) };
        entry.session.mark_saved(target.clone(), &snapshot, fingerprint);
        entry.writing = false;
        entry.conflict_shown = false;
        entry.recovery_id.take()
    };
    if let Some(id) = recovery_id {
        crate::recovery::remove(app, &id);
    }
    if current_path.as_deref() != Some(target.as_path()) {
        if let Some(old) = &current_path {
            crate::watcher::unwatch(app, old);
        }
        crate::watcher::watch(app, &target);
        let recent = target.to_string_lossy().into_owned();
        crate::settings_store::update(app, |settings| settings.note_recent(&recent));
    }
    windows::publish(app, label);
    Ok(true)
}

fn set_writing(app: &AppHandle, label: &str, writing: bool) {
    let state = app.state::<AppState>();
    let mut docs = state.docs();
    if let Some(entry) = docs.get_mut(label) {
        entry.writing = writing;
    }
}

/// Autosave (NSDocument autosave-in-place): 2 s after the last change a
/// titled document is written silently; an untitled one gets a crash
/// recovery copy instead.
fn schedule_autosave(app: &AppHandle, label: &str) {
    let generation = {
        let state = app.state::<AppState>();
        let mut docs = state.docs();
        let Some(entry) = docs.get_mut(label) else { return };
        entry.autosave_generation += 1;
        entry.autosave_generation
    };
    let app = app.clone();
    let label = label.to_owned();
    thread::spawn(move || {
        thread::sleep(AUTOSAVE_DELAY);
        enum Work {
            Save,
            Recover(String, String),
            Nothing,
        }
        let work = {
            let state = app.state::<AppState>();
            let mut docs = state.docs();
            match docs.get_mut(&label) {
                Some(entry)
                    if entry.autosave_generation == generation
                        && entry.close == CloseState::Open
                        && entry.session.is_dirty() =>
                {
                    if entry.session.path().is_some() {
                        if entry.conflict_shown {
                            Work::Nothing // never overwrite an unresolved conflict
                        } else {
                            Work::Save
                        }
                    } else {
                        let id = entry.recovery_id.get_or_insert_with(crate::recovery::new_id).clone();
                        Work::Recover(id, entry.session.full_text())
                    }
                }
                _ => Work::Nothing,
            }
        };
        match work {
            Work::Save => {
                if let Err(error) = save(&app, &label, false) {
                    eprintln!("[vellumi] autosave failed for {label}: {error}");
                }
            }
            Work::Recover(id, text) => crate::recovery::write(&app, &id, &text),
            Work::Nothing => {}
        }
    });
}

/// Starts closing a window: ask the shell for its last text, with a
/// watchdog so a dead editor can never block the close (audit C3).
pub fn request_close(app: &AppHandle, label: &str) {
    let ready = {
        let state = app.state::<AppState>();
        let mut docs = state.docs();
        let Some(entry) = docs.get_mut(label) else { return };
        if entry.close != CloseState::Open {
            return;
        }
        entry.close = CloseState::Flushing;
        entry.ready
    };
    match app.get_webview_window(label) {
        Some(window) if ready => eval(&window, "window.__vellumiFlush('close');"),
        _ => {
            if take_close_turn(app, label) {
                finish_close(app, label);
            }
            return;
        }
    }
    let app = app.clone();
    let label = label.to_owned();
    thread::spawn(move || {
        thread::sleep(CLOSE_WATCHDOG);
        if take_close_turn(&app, &label) {
            eprintln!("[vellumi] editor did not answer the close flush in time; closing with the last bridged text");
            finish_close(&app, &label);
        }
    });
}

/// Exactly one of doc_flush / the watchdog gets to finish the close.
fn take_close_turn(app: &AppHandle, label: &str) -> bool {
    let state = app.state::<AppState>();
    let mut docs = state.docs();
    match docs.get_mut(label) {
        Some(entry) if entry.close == CloseState::Flushing => {
            entry.close = CloseState::Finishing;
            true
        }
        _ => false,
    }
}

fn finish_close(app: &AppHandle, label: &str) {
    let state = app.state::<AppState>();
    let (dirty, titled, name, conflict) = {
        let docs = state.docs();
        let Some(entry) = docs.get(label) else { return };
        (
            entry.session.is_dirty(),
            entry.session.path().is_some(),
            windows::display_name(&entry.session),
            entry.conflict_shown,
        )
    };
    let proceed = if !dirty {
        true
    } else if titled && !conflict {
        // Autosave-in-place semantics: a titled document saves on close.
        match save(app, label, false) {
            Ok(saved) => saved,
            Err(error) => {
                windows::show_error(app, &error.to_string());
                false
            }
        }
    } else {
        ask_to_save(app, label, &name, titled)
    };

    if !proceed {
        let mut docs = state.docs();
        if let Some(entry) = docs.get_mut(label) {
            entry.close = CloseState::Open;
        }
        state.set_quitting(false);
        return;
    }

    let removed = state.docs().remove(label);
    if let Some(entry) = removed {
        if let Some(id) = entry.recovery_id {
            crate::recovery::remove(app, &id);
        }
        if let Some(path) = entry.session.path() {
            crate::watcher::unwatch(app, path);
            // A New Note closed without a single keystroke leaves no file.
            if entry.empty_draft && std::fs::metadata(path).is_ok_and(|meta| meta.len() == 0) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    if let Some(window) = app.get_webview_window(label) {
        if let Err(error) = window.destroy() {
            eprintln!("[vellumi] destroy failed for {label}: {error}");
        }
    }
    if state.is_quitting() && state.docs().is_empty() {
        if state.restart_requested() {
            app.restart();
        }
        app.exit(0);
    }
}

/// Save / Don't Save / Cancel: untitled documents with content, and titled
/// ones whose file changed on disk under unsaved edits.
fn ask_to_save(app: &AppHandle, label: &str, name: &str, titled: bool) -> bool {
    if headless() {
        return true;
    }
    let (message, save_label, discard, cancel) = match (is_chinese(), titled) {
        (true, false) => (format!("要存储对“{name}”所做的更改吗？"), "存储…", "不存储", "取消"),
        (true, true) => (
            format!("“{name}”已在其他地方被修改。要用你的版本覆盖它吗？"),
            "覆盖",
            "放弃我的更改",
            "取消",
        ),
        (false, false) => (format!("Do you want to save the changes made to “{name}”?"), "Save…", "Don't Save", "Cancel"),
        (false, true) => (
            format!("“{name}” was changed on disk. Overwrite it with your version?"),
            "Overwrite",
            "Discard My Changes",
            "Cancel",
        ),
    };
    let answer = app
        .dialog()
        .message(message)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::YesNoCancelCustom(
            save_label.to_owned(),
            discard.to_owned(),
            cancel.to_owned(),
        ))
        .blocking_show_with_result();
    let wants_save = match &answer {
        MessageDialogResult::Yes => true,
        MessageDialogResult::Custom(text) => text == save_label,
        _ => false,
    };
    let wants_discard = match &answer {
        MessageDialogResult::No => true,
        MessageDialogResult::Custom(text) => text == discard,
        _ => false,
    };
    if wants_save {
        if titled {
            // Overwrite: accept the disk version as known, then save over it.
            let state = app.state::<AppState>();
            let path = state.docs().get(label).and_then(|e| e.session.path().cloned());
            if let Some(fingerprint) = path.and_then(|p| vellumi_fs::fingerprint(&p).ok()) {
                let mut docs = state.docs();
                if let Some(entry) = docs.get_mut(label) {
                    entry.session.acknowledge_disk_version(fingerprint);
                    entry.conflict_shown = false;
                }
            }
        }
        return save(app, label, !titled).unwrap_or_else(|error| {
            windows::show_error(app, &error.to_string());
            false
        });
    }
    wants_discard
}
