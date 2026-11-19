//! Vellumi desktop shell. Rust owns files, windows, menus and settings; the
//! window content is the Svelte shell (apps/desktop/ui) around the same
//! Milkdown Crepe editor the Swift app shipped.

mod assets;
mod bridge;
mod commands;
mod error;
mod export;
mod legacy;
mod menu;
mod projects;
mod recovery;
mod settings_store;
mod state;
mod updater;
mod watcher;
mod windows;

use std::path::PathBuf;

use tauri::{AppHandle, Manager, RunEvent, Window, WindowEvent};
use vellumi_core::document::DocumentSession;

use crate::state::{headless, roundtrip_dir, AppState};

fn on_window_event(window: &Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            let app = window.app_handle();
            let label = window.label();
            if app.state::<AppState>().docs().contains_key(label) {
                // Never let the window vanish before its last keystrokes are saved.
                api.prevent_close();
                bridge::request_close(app, label);
            }
        }
        WindowEvent::Focused(true) => {
            let app = window.app_handle();
            let source = app
                .state::<AppState>()
                .docs()
                .get(window.label())
                .is_some_and(|entry| entry.session.source_mode());
            menu::sync_source_check(app, source);
        }
        _ => {}
    }
}

fn open_launch_documents(app: &AppHandle) {
    let state = app.state::<AppState>();
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    for path in &paths {
        windows::open_path(app, path);
    }
    if headless() {
        if state.docs().is_empty() {
            app.exit(0);
        }
        // A file whose editor never answers must not hang a harness.
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(90));
            eprintln!("[harness] timed out");
            app.exit(2);
        });
        return;
    }
    // Untitled documents left by a crash come back first, still dirty.
    for (id, text) in recovery::pending(app) {
        let mut session = DocumentSession::untitled(&text);
        session.mark_recovered();
        if let Err(error) = windows::open_document(app, session, Some(id)) {
            eprintln!("[vellumi] restoring a recovered document failed: {error}");
        }
    }
    if paths.is_empty() && state.docs().is_empty() {
        // Finder "Open With" arrives as RunEvent::Opened right after launch.
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(400));
            if !app.state::<AppState>().docs().is_empty() {
                return;
            }
            let first_launch = !app.state::<AppState>().settings().has_shown_welcome;
            if first_launch {
                settings_store::update(&app, |s| s.has_shown_welcome = true);
                windows::open_untitled(&app, windows::welcome_text());
            } else {
                windows::open_untitled(&app, "");
            }
        });
    }
}

fn on_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        RunEvent::Ready => open_launch_documents(app),
        #[cfg(target_os = "macos")]
        RunEvent::Opened { urls } => {
            for url in urls {
                match url.to_file_path() {
                    Ok(path) => windows::open_path(app, &path),
                    Err(()) => eprintln!("[vellumi] ignoring non-file URL {url}"),
                }
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } => windows::open_untitled(app, ""),
        RunEvent::ExitRequested { code, api, .. } => {
            let state = app.state::<AppState>();
            if code.is_some() || roundtrip_dir().is_some() {
                return; // our own app.exit(..)
            }
            if state.is_quitting() || !state.docs().is_empty() {
                // System quit (Dock, logout) with documents open: run the
                // same save handshake as ⌘Q.
                api.prevent_exit();
                if !state.is_quitting() {
                    menu::quit(app);
                }
            } else {
                // Last window closed: a Mac document app keeps running.
                api.prevent_exit();
            }
        }
        _ => {}
    }
}

pub fn run() {
    // Read-only rehearsal of the 0.5.x import: prints JSON, writes nothing.
    if std::env::args().any(|arg| arg == "--legacy-report") {
        println!("{}", legacy::dry_run());
        return;
    }
    let built = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(export::ExportState::default())
        .register_asynchronous_uri_scheme_protocol("vellumi-asset", assets::protocol)
        .register_asynchronous_uri_scheme_protocol("vellumi-export", export::protocol)
        .invoke_handler(tauri::generate_handler![
            bridge::bridge_message,
            bridge::doc_flush,
            commands::doc_info,
            commands::doc_set_front_matter,
            commands::doc_source_enter,
            commands::doc_source_changed,
            commands::doc_source_exit,
            commands::doc_revert,
            commands::doc_reload_from_disk,
            commands::doc_keep_mine,
            commands::tree,
            commands::search,
            commands::quick_open,
            commands::open_path,
            commands::reveal_in_finder,
            commands::settings_get,
            commands::settings_patch,
            commands::run_command,
            commands::selftest_done,
            commands::ui_log,
            commands::selftest_external,
            commands::selftest_read,
            legacy::take_migration_notice,
            commands::index_status,
            commands::projects_list,
            commands::projects_add,
            commands::projects_remove,
            commands::projects_rename,
            commands::projects_relocate,
            commands::project_new_note,
            commands::settings_pick_drafts,
            commands::open_projects_window,
            export::export_html,
            export::export_pdf,
            export::print_document,
            export::cards_open,
            export::cards_source,
            export::cards_write,
            export::export_release,
        ])
        .setup(|app| {
            let handle = app.handle();
            let settings = settings_store::load(handle);
            *app.state::<AppState>().settings() = settings.clone();
            menu::sync_checks(handle, &settings);
            watcher::init(handle);
            projects::init(handle);
            #[cfg(feature = "updater")]
            if !state::headless() {
                handle.plugin(tauri_plugin_updater::Builder::new().build())?;
                updater::check(handle, false);
            }
            Ok(())
        })
        .menu(menu::build)
        .on_menu_event(menu::handle)
        .on_window_event(on_window_event)
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(on_run_event),
        Err(error) => eprintln!("[vellumi] failed to start: {error}"),
    }
}
