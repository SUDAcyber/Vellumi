//! Document windows: one WebviewWindow per document, grouped into native
//! macOS tabs (decision D3). The shell UI inside each window gets its state
//! through `doc:state` events and the commands in `commands.rs`.

use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Theme, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use vellumi_core::document::DocumentSession;
use vellumi_core::settings::{Appearance, Settings};
use vellumi_core::stats::{self, Stats};

use crate::error::AppResult;
use crate::state::{headless, roundtrip_dir, selftest_report, AppState, DocEntry};

const SHIM: &str = include_str!("../shim.js");
const WELCOME_ZH: &str = include_str!("../../../../../Resources/zh-Hans.lproj/Welcome.md");
const WELCOME_EN: &str = include_str!("../../../../../Resources/en.lproj/Welcome.md");

/// "zh-Hans" when the system prefers Chinese, else "en".
pub fn app_language() -> &'static str {
    match sys_locale::get_locale() {
        Some(locale) if locale.to_ascii_lowercase().starts_with("zh") => "zh-Hans",
        _ => "en",
    }
}

pub fn is_chinese() -> bool {
    app_language() == "zh-Hans"
}

pub fn welcome_text() -> &'static str {
    if is_chinese() {
        WELCOME_ZH
    } else {
        WELCOME_EN
    }
}

/// JSON string literal, which is also a valid JS string literal.
pub fn js_string(value: &str) -> Option<String> {
    serde_json::to_string(value).ok()
}

pub fn init_script() -> String {
    let language = js_string(app_language()).unwrap_or_else(|| "\"en\"".to_owned());
    let selftest = selftest_report().is_some();
    format!("window.__vellumiLanguage = {language};\nwindow.__vellumiSelftest = {selftest};\n{SHIM}")
}

pub fn eval(window: &WebviewWindow, script: &str) {
    if let Err(error) = window.eval(script) {
        eprintln!("[vellumi] eval failed in {}: {error}", window.label());
    }
}

pub fn untitled_name() -> &'static str {
    if is_chinese() {
        "未命名"
    } else {
        "Untitled"
    }
}

pub fn display_name(session: &DocumentSession) -> String {
    session
        .path()
        .and_then(|path| path.file_name())
        .map_or_else(|| untitled_name().to_owned(), |name| name.to_string_lossy().into_owned())
}

/// Everything the shell shows about its document.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    pub name: String,
    pub path: Option<String>,
    pub folder: Option<String>,
    pub encoding: &'static str,
    pub front_matter: String,
    pub dirty: bool,
    pub source_mode: bool,
    pub normalized_on_load: bool,
    pub stats: Stats,
}

pub fn doc_info(entry: &DocEntry) -> DocInfo {
    let session = &entry.session;
    DocInfo {
        name: display_name(session),
        path: session.path().map(|p| p.to_string_lossy().into_owned()),
        folder: session
            .path()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().into_owned()),
        encoding: session.encoding().label(),
        front_matter: session.front_matter().to_owned(),
        dirty: session.is_dirty(),
        source_mode: session.source_mode(),
        normalized_on_load: session.normalized_on_load(),
        stats: stats::stats(session.body()),
    }
}

/// Pushes the document state to its window's shell, and mirrors the dirty
/// flag into the native title bar.
pub fn publish(app: &AppHandle, label: &str) {
    let info = {
        let state = app.state::<AppState>();
        let docs = state.docs();
        let Some(entry) = docs.get(label) else { return };
        doc_info(entry)
    };
    let Some(window) = app.get_webview_window(label) else { return };
    if let Err(error) = window.set_title(&info.name) {
        eprintln!("[vellumi] set_title failed: {error}");
    }
    set_document_edited(&window, info.dirty);
    if let Err(error) = window.emit_to(label, "doc:state", &info) {
        eprintln!("[vellumi] emit doc:state failed: {error}");
    }
}

/// The native "edited" dot in the close button (NSWindow.documentEdited).
#[cfg(target_os = "macos")]
fn set_document_edited(window: &WebviewWindow, dirty: bool) {
    let Ok(pointer) = window.ns_window() else { return };
    let address = pointer as usize;
    let result = window.run_on_main_thread(move || {
        let object = address as *mut objc2::runtime::AnyObject;
        // SAFETY: the NSWindow is alive (owned by the webview window that
        // scheduled this on the main thread) and setDocumentEdited: takes a
        // BOOL and returns void.
        unsafe {
            if let Some(window) = object.as_ref() {
                let _: () = objc2::msg_send![window, setDocumentEdited: dirty];
            }
        }
    });
    if let Err(error) = result {
        eprintln!("[vellumi] setDocumentEdited failed: {error}");
    }
}

#[cfg(not(target_os = "macos"))]
fn set_document_edited(_window: &WebviewWindow, _dirty: bool) {}

/// Opens `session` in a new window (or focuses the window already showing
/// that file). Returns the window label.
pub fn open_document(app: &AppHandle, session: DocumentSession, recovery_id: Option<String>) -> AppResult<String> {
    let state = app.state::<AppState>();
    if let Some(existing) = session.path().and_then(|path| state.label_for_path(path)) {
        if let Some(window) = app.get_webview_window(&existing) {
            window.set_focus()?;
            return Ok(existing);
        }
    }
    if let Some(path) = session.path() {
        let path = path.to_string_lossy().into_owned();
        crate::settings_store::update(app, |settings| settings.note_recent(&path));
    }
    let label = state.next_label();
    let title = display_name(&session);
    let watched = session.path().cloned();
    let mut entry = DocEntry::new(session);
    entry.recovery_id = recovery_id;
    state.docs().insert(label.clone(), entry);

    let builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(&title)
        .inner_size(1100.0, 760.0)
        .min_inner_size(560.0, 360.0)
        .initialization_script(init_script())
        .visible(!headless());
    // Hidden harness windows: WebKit would otherwise throttle (then suspend)
    // their timers, stalling the self-test.
    let builder = if headless() {
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
    } else {
        builder
    };
    #[cfg(target_os = "macos")]
    let builder = builder.tabbing_identifier("VellumiDocument");

    match builder.build() {
        Ok(window) => {
            let settings = state.settings().clone();
            apply_window_settings(&window, &settings);
        }
        Err(error) => {
            state.docs().remove(&label);
            return Err(error.into());
        }
    }
    if let Some(path) = watched {
        crate::watcher::watch(app, &path);
    }
    Ok(label)
}

pub fn open_untitled(app: &AppHandle, text: &str) {
    if let Err(error) = open_document(app, DocumentSession::untitled(text), None) {
        show_error(app, &error.to_string());
    }
}

/// Opens a file from disk, reporting failures in a dialog.
pub fn open_path(app: &AppHandle, path: &Path) {
    let state = app.state::<AppState>();
    if let Some(label) = state.label_for_path(path) {
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.set_focus();
            return;
        }
    }
    let result = vellumi_fs::open_document(path).and_then(|session| {
        open_document(app, session, None).map_err(|error| vellumi_core::CoreError::Io {
            op: "open window",
            path: path.to_path_buf(),
            source: std::io::Error::other(error.to_string()),
        })
    });
    if let Err(error) = result {
        if roundtrip_dir().is_some() {
            eprintln!("[roundtrip] {error}");
        } else {
            show_error(app, &error.to_string());
        }
    }
}

pub fn show_error(app: &AppHandle, message: &str) {
    if headless() {
        eprintln!("[vellumi] error: {message}");
        return;
    }
    app.dialog()
        .message(message)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

/// Window-level settings Rust owns: zoom and light/dark appearance. The
/// shell applies the editor-level ones (theme, typography, modes).
pub fn apply_window_settings(window: &WebviewWindow, settings: &Settings) {
    if let Err(error) = window.set_zoom(settings.zoom) {
        eprintln!("[vellumi] set_zoom failed: {error}");
    }
    let theme = match settings.appearance {
        Appearance::System => None,
        Appearance::Light => Some(Theme::Light),
        Appearance::Dark => Some(Theme::Dark),
    };
    if let Err(error) = window.set_theme(theme) {
        eprintln!("[vellumi] set_theme failed: {error}");
    }
}

pub fn apply_settings_everywhere(app: &AppHandle, settings: &Settings) {
    for window in app.webview_windows().into_values() {
        if !window.label().starts_with("doc-") {
            // Settings / Projects never zoom; they follow the appearance.
            let _ = window.set_theme(match settings.appearance {
                Appearance::System => None,
                Appearance::Light => Some(Theme::Light),
                Appearance::Dark => Some(Theme::Dark),
            });
        } else {
            apply_window_settings(&window, settings);
        }
    }
}

pub fn doc_windows(app: &AppHandle) -> Vec<WebviewWindow<Wry>> {
    app.webview_windows()
        .into_values()
        .filter(|window| window.label().starts_with("doc-"))
        .collect()
}

pub fn focused_doc_window(app: &AppHandle) -> Option<WebviewWindow<Wry>> {
    doc_windows(app)
        .into_iter()
        .find(|window| window.is_focused().unwrap_or(false))
}

/// New Note in `folder` (project or Drafts): an empty `未命名.md` /
/// `未命名 2.md`… created up front so autosave owns it from the first
/// keystroke. Closed untouched, the empty file is removed again (audit B18).
pub fn new_note_in(app: &AppHandle, folder: &Path) -> AppResult<()> {
    let target = vellumi_core::assets::unique_markdown_path(folder, untitled_name(), Path::exists);
    vellumi_fs::write_new(&target, b"")?;
    match vellumi_fs::open_document(&target) {
        Ok(session) => match open_document(app, session, None) {
            Ok(label) => {
                if let Some(entry) = app.state::<AppState>().docs().get_mut(&label) {
                    entry.empty_draft = true;
                }
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::remove_file(&target);
                Err(error)
            }
        },
        Err(error) => {
            let _ = std::fs::remove_file(&target);
            Err(error.into())
        }
    }
}

/// File ▸ New: into the Drafts folder when one is configured and present,
/// otherwise an untitled scratch document (Swift behavior).
pub fn new_document(app: &AppHandle) {
    let drafts = app
        .state::<AppState>()
        .settings()
        .drafts_folder
        .clone()
        .map(std::path::PathBuf::from)
        .filter(|folder| folder.is_dir());
    match drafts {
        Some(folder) => {
            if let Err(error) = new_note_in(app, &folder) {
                eprintln!("[vellumi] drafts folder unusable ({error}); opening an untitled document");
                open_untitled(app, "");
            }
        }
        None => open_untitled(app, ""),
    }
}

/// Projects (⇧⌘0): a single window, reused.
pub fn open_projects(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("projects") {
        let _ = window.set_focus();
        return;
    }
    let title = if is_chinese() { "项目" } else { "Projects" };
    let built = WebviewWindowBuilder::new(app, "projects", WebviewUrl::App("projects.html".into()))
        .title(title)
        .inner_size(1080.0, 720.0)
        .min_inner_size(560.0, 420.0)
        .initialization_script(init_script())
        .build();
    match built {
        Ok(window) => {
            let settings = app.state::<AppState>().settings().clone();
            apply_settings_everywhere(app, &settings);
            let _ = window.set_focus();
        }
        Err(error) => show_error(app, &error.to_string()),
    }
}

/// Settings (⌘,): a single window, reused.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_focus();
        return;
    }
    let title = if is_chinese() { "设置" } else { "Settings" };
    let built = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title(title)
        .inner_size(860.0, 600.0)
        .min_inner_size(640.0, 460.0)
        .initialization_script(init_script())
        .build();
    match built {
        Ok(window) => {
            let settings = app.state::<AppState>().settings().clone();
            apply_settings_everywhere(app, &settings);
            let _ = window.set_focus();
        }
        Err(error) => show_error(app, &error.to_string()),
    }
}
