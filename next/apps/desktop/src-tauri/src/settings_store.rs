//! settings.json in the app config folder. A corrupt file is moved aside
//! (never silently overwritten) and defaults are used.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager};
use vellumi_core::settings::Settings;

use crate::state::{headless, AppState};

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    if headless() {
        // Tests never read or write the user's settings.
        return Settings {
            has_shown_welcome: true,
            ..Settings::default()
        };
    }
    let Some(path) = settings_path(app) else {
        return Settings::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        // First launch of this app: take over 0.5.x's preferences once.
        if !path.exists() {
            if let Some((settings, report, source)) = crate::legacy::import(Settings::default()) {
                eprintln!("[vellumi] imported 0.5.x preferences from {}: {:?}", source.display(), report);
                persist(app, &settings);
                app.manage(crate::legacy::MigrationNotice(std::sync::Mutex::new(Some(crate::legacy::notice_text(&report)))));
                return settings;
            }
        }
        return Settings::default();
    };
    match serde_json::from_slice::<Settings>(&bytes) {
        Ok(settings) => settings.sanitized(),
        Err(error) => {
            eprintln!("[vellumi] settings.json is unreadable ({error}); keeping a copy and using defaults");
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            Settings::default()
        }
    }
}

pub fn persist(app: &AppHandle, settings: &Settings) {
    if headless() {
        return;
    }
    let Some(path) = settings_path(app) else { return };
    if let Some(dir) = path.parent() {
        if let Err(error) = vellumi_fs::create_dir_all(dir) {
            eprintln!("[vellumi] {error}");
            return;
        }
    }
    match serde_json::to_vec_pretty(settings) {
        Ok(bytes) => {
            if let Err(error) = vellumi_fs::write_atomic(&path, &bytes) {
                eprintln!("[vellumi] saving settings failed: {error}");
            }
        }
        Err(error) => eprintln!("[vellumi] serializing settings failed: {error}"),
    }
}

/// Applies `change`, sanitizes, persists, and tells every window.
pub fn update(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Settings {
    let updated = {
        let state = app.state::<AppState>();
        let mut settings = state.settings();
        change(&mut settings);
        *settings = settings.clone().sanitized();
        settings.clone()
    };
    persist(app, &updated);
    crate::windows::apply_settings_everywhere(app, &updated);
    if let Err(error) = app.emit("settings:changed", &updated) {
        eprintln!("[vellumi] emit settings:changed failed: {error}");
    }
    crate::menu::sync_checks(app, &updated);
    updated
}
