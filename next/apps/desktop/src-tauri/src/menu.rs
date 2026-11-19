//! Main menu with the 0.5.x shortcuts (REWRITE_PLAN §1.7). `dispatch` is the
//! single entry point for menu clicks and the command palette.

use std::collections::HashMap;
use std::thread;

use tauri::menu::{
    CheckMenuItem, CheckMenuItemBuilder, Menu, MenuBuilder, MenuEvent, MenuItemBuilder, PredefinedMenuItem,
    Submenu, SubmenuBuilder,
};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_dialog::DialogExt;
use vellumi_core::settings::{Appearance, Settings, ZOOM_RANGE, ZOOM_STEP};

use crate::bridge;
use crate::settings_store;
use crate::state::AppState;
use crate::windows::{self, eval, is_chinese};

fn t(zh: &'static str, en: &'static str) -> &'static str {
    if is_chinese() {
        zh
    } else {
        en
    }
}

/// Check items the app keeps in sync with the settings.
pub struct MenuHandles {
    checks: HashMap<&'static str, CheckMenuItem<Wry>>,
}

/// (id, 中文, English, accelerator, JS run in the focused editor)
pub const FORMAT_COMMANDS: &[(&str, &str, &str, Option<&str>, &str)] = &[
    ("format.body", "正文", "Body Text", Some("CmdOrCtrl+0"), "window.moltenAPI.setHeading(0)"),
    ("format.h1", "标题 1", "Heading 1", Some("CmdOrCtrl+1"), "window.moltenAPI.setHeading(1)"),
    ("format.h2", "标题 2", "Heading 2", Some("CmdOrCtrl+2"), "window.moltenAPI.setHeading(2)"),
    ("format.h3", "标题 3", "Heading 3", Some("CmdOrCtrl+3"), "window.moltenAPI.setHeading(3)"),
    ("format.h4", "标题 4", "Heading 4", Some("CmdOrCtrl+4"), "window.moltenAPI.setHeading(4)"),
    ("format.h5", "标题 5", "Heading 5", Some("CmdOrCtrl+5"), "window.moltenAPI.setHeading(5)"),
    ("format.h6", "标题 6", "Heading 6", Some("CmdOrCtrl+6"), "window.moltenAPI.setHeading(6)"),
    ("-", "", "", None, ""),
    ("format.bold", "粗体", "Bold", Some("CmdOrCtrl+B"), "window.moltenAPI.toggleBold()"),
    ("format.italic", "斜体", "Italic", Some("CmdOrCtrl+I"), "window.moltenAPI.toggleItalic()"),
    ("format.code", "行内代码", "Inline Code", Some("CmdOrCtrl+E"), "window.moltenAPI.toggleInlineCode()"),
    ("format.strike", "删除线", "Strikethrough", Some("Shift+CmdOrCtrl+X"), "window.moltenAPI.toggleStrikethrough()"),
    ("format.link", "链接", "Link", Some("CmdOrCtrl+K"), "window.moltenAPI.toggleLink()"),
    ("-", "", "", None, ""),
    ("format.bullet", "无序列表", "Bullet List", Some("Shift+CmdOrCtrl+U"), "window.moltenAPI.toggleBulletList()"),
    ("format.ordered", "有序列表", "Numbered List", Some("Shift+CmdOrCtrl+O"), "window.moltenAPI.toggleOrderedList()"),
    ("format.quote", "引用", "Quote", Some("Shift+CmdOrCtrl+Q"), "window.moltenAPI.toggleBlockquote()"),
    ("format.hr", "分隔线", "Horizontal Rule", Some("Alt+CmdOrCtrl+-"), "window.moltenAPI.insertHorizontalRule()"),
];

/// Commands the shell UI implements; the menu only forwards them.
const SHELL_ACTIONS: &[&str] = &[
    "view.outline",
    "view.files",
    "view.search",
    "view.source",
    "view.sidebar",
    "edit.find",
    "edit.findNext",
    "edit.findPrev",
    "app.palette",
    "file.frontMatter",
    "file.revert",
    "file.exportHtml",
    "file.exportPdf",
    "file.exportCards",
    "file.print",
];

const THEMES: &[(&str, &str, &str)] = &[("frame", "画框", "Frame"), ("nord", "北欧", "Nord"), ("classic", "经典", "Classic")];
const APPEARANCES: &[(&str, &str, &str)] = &[("system", "跟随系统", "System"), ("light", "浅色", "Light"), ("dark", "深色", "Dark")];

fn item(app: &AppHandle, id: &str, title: &str, accelerator: Option<&str>) -> tauri::Result<tauri::menu::MenuItem<Wry>> {
    let mut builder = MenuItemBuilder::with_id(id, title);
    if let Some(accelerator) = accelerator {
        builder = builder.accelerator(accelerator);
    }
    builder.build(app)
}

fn check(
    app: &AppHandle,
    handles: &mut HashMap<&'static str, CheckMenuItem<Wry>>,
    id: &'static str,
    title: &str,
    accelerator: Option<&str>,
) -> tauri::Result<CheckMenuItem<Wry>> {
    let mut builder = CheckMenuItemBuilder::with_id(id, title);
    if let Some(accelerator) = accelerator {
        builder = builder.accelerator(accelerator);
    }
    let built = builder.build(app)?;
    handles.insert(id, built.clone());
    Ok(built)
}

/// "Check for Updates…" exists only in builds with the updater.
fn updates_item(app: &AppHandle) -> tauri::Result<Vec<tauri::menu::MenuItem<Wry>>> {
    if crate::updater::enabled() {
        Ok(vec![item(app, "app.checkUpdates", t("检查更新…", "Check for Updates…"), None)?])
    } else {
        Ok(Vec::new())
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let mut checks = HashMap::new();

    let mut app_menu = SubmenuBuilder::new(app, "Vellumi")
        .item(&PredefinedMenuItem::about(app, None, None)?)
        .separator()
        .item(&item(app, "app.settings", t("设置…", "Settings…"), Some("CmdOrCtrl+,"))?);
    for update in updates_item(app)? {
        app_menu = app_menu.item(&update);
    }
    let app_menu = app_menu
        .separator()
        .item(&PredefinedMenuItem::services(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .item(&PredefinedMenuItem::show_all(app, None)?)
        .separator()
        .item(&item(app, "app.quit", t("退出 Vellumi", "Quit Vellumi"), Some("CmdOrCtrl+Q"))?)
        .build()?;

    let file = SubmenuBuilder::new(app, t("文件", "File"))
        .item(&item(app, "file.new", t("新建", "New"), Some("CmdOrCtrl+N"))?)
        .item(&item(app, "file.open", t("打开…", "Open…"), Some("CmdOrCtrl+O"))?)
        .item(&item(app, "app.palette", t("快速打开…", "Quick Open…"), Some("Shift+CmdOrCtrl+P"))?)
        .separator()
        .item(&item(app, "file.close", t("关闭", "Close"), Some("CmdOrCtrl+W"))?)
        .item(&item(app, "file.save", t("存储", "Save"), Some("CmdOrCtrl+S"))?)
        .item(&item(app, "file.saveAs", t("存储为…", "Save As…"), Some("Shift+CmdOrCtrl+S"))?)
        .item(&item(app, "file.revert", t("恢复为已存储的版本…", "Revert to Saved…"), None)?)
        .separator()
        .item(&item(app, "file.frontMatter", t("编辑 Front Matter…", "Edit Front Matter…"), None)?)
        .separator()
        .item(&item(app, "file.exportHtml", t("导出 HTML…", "Export HTML…"), Some("Shift+CmdOrCtrl+E"))?)
        .item(&item(app, "file.exportPdf", t("导出 PDF…", "Export PDF…"), None)?)
        .item(&item(app, "file.exportCards", t("导出为图片卡…", "Export Image Cards…"), None)?)
        .separator()
        .item(&item(app, "file.print", t("打印…", "Print…"), Some("CmdOrCtrl+P"))?)
        .build()?;

    let edit = SubmenuBuilder::new(app, t("编辑", "Edit"))
        .item(&item(app, "edit.undo", t("撤销", "Undo"), Some("CmdOrCtrl+Z"))?)
        .item(&item(app, "edit.redo", t("重做", "Redo"), Some("Shift+CmdOrCtrl+Z"))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .separator()
        .item(&item(app, "edit.find", t("查找与替换…", "Find and Replace…"), Some("CmdOrCtrl+F"))?)
        .item(&item(app, "edit.findNext", t("查找下一个", "Find Next"), Some("CmdOrCtrl+G"))?)
        .item(&item(app, "edit.findPrev", t("查找上一个", "Find Previous"), Some("Shift+CmdOrCtrl+G"))?)
        .item(&item(app, "view.search", t("在文件夹中搜索…", "Search in Folder…"), Some("Shift+CmdOrCtrl+F"))?)
        .build()?;

    let mut format = SubmenuBuilder::new(app, t("格式", "Format"));
    for (id, zh, en, accelerator, _) in FORMAT_COMMANDS {
        format = if *id == "-" {
            format.separator()
        } else {
            format.item(&item(app, id, t(zh, en), *accelerator)?)
        };
    }
    let format = format.build()?;

    let mut appearance = SubmenuBuilder::new(app, t("外观", "Appearance"));
    for (value, zh, en) in APPEARANCES {
        let id: &'static str = match *value {
            "light" => "view.appearance.light",
            "dark" => "view.appearance.dark",
            _ => "view.appearance.system",
        };
        appearance = appearance.item(&check(app, &mut checks, id, t(zh, en), None)?);
    }
    let appearance = appearance.build()?;

    let mut themes = SubmenuBuilder::new(app, t("主题", "Theme"));
    for (value, zh, en) in THEMES {
        let id: &'static str = match *value {
            "nord" => "view.theme.nord",
            "classic" => "view.theme.classic",
            _ => "view.theme.frame",
        };
        themes = themes.item(&check(app, &mut checks, id, t(zh, en), None)?);
    }
    let themes = themes.build()?;

    let view = SubmenuBuilder::new(app, t("显示", "View"))
        .item(&item(app, "view.projects", t("项目…", "Projects…"), Some("Shift+CmdOrCtrl+0"))?)
        .item(&item(app, "view.sidebar", t("显示/隐藏侧栏", "Toggle Sidebar"), Some("Ctrl+CmdOrCtrl+S"))?)
        .item(&item(app, "view.outline", t("大纲", "Outline"), Some("Alt+CmdOrCtrl+1"))?)
        .item(&item(app, "view.files", t("文件树", "Files"), Some("Alt+CmdOrCtrl+2"))?)
        .separator()
        .item(&check(app, &mut checks, "view.source", t("源码模式", "Source Mode"), Some("CmdOrCtrl+/"))?)
        .item(&check(app, &mut checks, "view.typewriter", t("打字机模式", "Typewriter Mode"), Some("Alt+CmdOrCtrl+T"))?)
        .item(&check(app, &mut checks, "view.focus", t("专注模式", "Focus Mode"), Some("Alt+CmdOrCtrl+F"))?)
        .item(&check(app, &mut checks, "view.spellcheck", t("拼写检查", "Check Spelling"), None)?)
        .separator()
        .item(&item(app, "view.zoomIn", t("放大", "Zoom In"), Some("CmdOrCtrl+="))?)
        .item(&item(app, "view.zoomOut", t("缩小", "Zoom Out"), Some("CmdOrCtrl+-"))?)
        .item(&item(app, "view.zoomReset", t("实际大小", "Actual Size"), Some("Ctrl+CmdOrCtrl+0"))?)
        .separator()
        .item(&appearance)
        .item(&themes)
        .separator()
        .item(&PredefinedMenuItem::fullscreen(app, None)?)
        .build()?;

    let window: Submenu<Wry> = SubmenuBuilder::new(app, t("窗口", "Window"))
        .item(&PredefinedMenuItem::minimize(app, None)?)
        .item(&PredefinedMenuItem::maximize(app, None)?)
        .build()?;
    // Lets macOS add Show Tab Bar / Merge All Windows / the window list.
    #[cfg(target_os = "macos")]
    window.set_as_windows_menu_for_nsapp()?;

    app.manage(MenuHandles { checks });
    let settings = app.state::<AppState>().settings().clone();
    sync_checks(app, &settings);

    MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &format, &view, &window])
        .build()
}

/// Mirrors the settings into the menu's check marks.
pub fn sync_checks(app: &AppHandle, settings: &Settings) {
    let Some(handles) = app.try_state::<MenuHandles>() else { return };
    let set = |id: &str, on: bool| {
        if let Some(item) = handles.checks.get(id) {
            let _ = item.set_checked(on);
        }
    };
    set("view.typewriter", settings.typewriter);
    set("view.focus", settings.focus_mode);
    set("view.spellcheck", settings.spellcheck);
    for (value, _, _) in THEMES {
        set(&format!("view.theme.{value}"), settings.theme == *value);
    }
    set("view.appearance.system", settings.appearance == Appearance::System);
    set("view.appearance.light", settings.appearance == Appearance::Light);
    set("view.appearance.dark", settings.appearance == Appearance::Dark);
}

/// Source-mode check mark follows the focused document.
pub fn sync_source_check(app: &AppHandle, on: bool) {
    if let Some(handles) = app.try_state::<MenuHandles>() {
        if let Some(item) = handles.checks.get("view.source") {
            let _ = item.set_checked(on);
        }
    }
}

pub fn handle(app: &AppHandle, event: MenuEvent) {
    dispatch(app, event.id().as_ref(), None);
}

/// Runs command `id` against `target` (a window label) or the focused
/// document window.
pub fn dispatch(app: &AppHandle, id: &str, target: Option<&str>) {
    let target_window = target
        .and_then(|label| app.get_webview_window(label))
        .filter(|window| window.label().starts_with("doc-"))
        .or_else(|| windows::focused_doc_window(app));
    let in_source_mode = |label: &str| {
        app.state::<AppState>()
            .docs()
            .get(label)
            .is_some_and(|entry| entry.session.source_mode())
    };

    if let Some((_, _, _, _, script)) = FORMAT_COMMANDS.iter().find(|(command, ..)| *command == id) {
        if let Some(window) = target_window.filter(|w| !in_source_mode(w.label())) {
            eval(&window, &format!("window.moltenAPI?.focus(); {script};"));
        }
        return;
    }
    if SHELL_ACTIONS.contains(&id) {
        if let Some(window) = target_window {
            if let Err(error) = window.emit_to(window.label(), "shell:action", id) {
                eprintln!("[vellumi] emit shell:action failed: {error}");
            }
        }
        return;
    }
    match id {
        "app.quit" => quit(app),
        "app.settings" => windows::open_settings(app),
        "app.checkUpdates" => crate::updater::check(app, true),
        "file.new" => windows::new_document(app),
        "view.projects" => windows::open_projects(app),
        "file.open" => {
            let app = app.clone();
            thread::spawn(move || {
                let picked = app
                    .dialog()
                    .file()
                    .add_filter("Markdown", &["md", "markdown", "txt"])
                    .blocking_pick_files();
                for file in picked.unwrap_or_default() {
                    match file.into_path() {
                        Ok(path) => windows::open_path(&app, &path),
                        Err(error) => windows::show_error(&app, &error.to_string()),
                    }
                }
            });
        }
        "file.close" => {
            if let Some(window) = target_window {
                bridge::request_close(app, window.label());
            } else if let Some(settings) = app.get_webview_window("settings").filter(|w| w.is_focused().unwrap_or(false)) {
                let _ = settings.close();
            }
        }
        "file.save" | "file.saveAs" => {
            if let Some(window) = target_window {
                let reason = if id == "file.save" { "save" } else { "save-as" };
                eval(&window, &format!("window.__vellumiFlush('{reason}');"));
            }
        }
        "edit.undo" | "edit.redo" => {
            let direction = if id == "edit.undo" { "undo" } else { "redo" };
            let window = target_window.or_else(|| app.get_webview_window("settings"));
            if let Some(window) = window {
                eval(&window, &format!("window.__vellumiHistory?.('{direction}');"));
            }
        }
        "view.typewriter" => {
            settings_store::update(app, |s| s.typewriter = !s.typewriter);
        }
        "view.focus" => {
            settings_store::update(app, |s| s.focus_mode = !s.focus_mode);
        }
        "view.spellcheck" => {
            settings_store::update(app, |s| s.spellcheck = !s.spellcheck);
        }
        "view.zoomIn" => {
            settings_store::update(app, |s| s.zoom = (s.zoom * ZOOM_STEP).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1));
        }
        "view.zoomOut" => {
            settings_store::update(app, |s| s.zoom = (s.zoom / ZOOM_STEP).clamp(ZOOM_RANGE.0, ZOOM_RANGE.1));
        }
        "view.zoomReset" => {
            settings_store::update(app, |s| s.zoom = 1.0);
        }
        theme if theme.starts_with("view.theme.") => {
            let theme = theme.trim_start_matches("view.theme.").to_owned();
            settings_store::update(app, |s| s.theme = theme);
        }
        appearance if appearance.starts_with("view.appearance.") => {
            let value = match appearance.trim_start_matches("view.appearance.") {
                "light" => Appearance::Light,
                "dark" => Appearance::Dark,
                _ => Appearance::System,
            };
            settings_store::update(app, |s| s.appearance = value);
        }
        _ => {}
    }
}

/// ⌘Q: close every document through the normal handshake (each saves or
/// asks); the app exits when the last one is gone. Cancelling any prompt
/// cancels the quit.
/// After an update is installed: close every document through the save
/// handshake, then relaunch (cancelling a prompt cancels the restart).
#[cfg_attr(not(feature = "updater"), allow(dead_code))]
pub fn quit_and_restart(app: &AppHandle) {
    app.state::<AppState>().set_restart();
    if app.state::<AppState>().docs().is_empty() {
        app.restart();
    }
    quit(app);
    app.state::<AppState>().set_restart();
}

pub fn quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    let labels: Vec<String> = state.docs().keys().cloned().collect();
    if labels.is_empty() {
        app.exit(0);
        return;
    }
    state.set_quitting(true);
    for label in labels {
        bridge::request_close(app, &label);
    }
}
