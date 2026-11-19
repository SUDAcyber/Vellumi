//! Auto-update (replaces Sparkle). Compiled only with `--features updater`,
//! which scripts/release.sh enables once an update signing key exists — the
//! plugin refuses to start without a public key in the config, and creating
//! that key is the owner's decision (docs/RELEASE.md).

#[cfg(feature = "updater")]
pub fn check(app: &tauri::AppHandle, interactive: bool) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    use tauri_plugin_updater::UpdaterExt;

    let chinese = crate::windows::is_chinese();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = match app.updater() {
            Ok(updater) => updater.check().await,
            Err(error) => Err(error),
        };
        match result {
            Ok(Some(update)) => {
                let (message, install, later) = if chinese {
                    (format!("Vellumi {} 已发布（当前 {}）。现在下载并安装吗？", update.version, update.current_version), "安装并重启", "以后")
                } else {
                    (format!("Vellumi {} is available (you have {}). Install it now?", update.version, update.current_version), "Install and Relaunch", "Later")
                };
                let confirmed = app
                    .dialog()
                    .message(message)
                    .kind(MessageDialogKind::Info)
                    .buttons(MessageDialogButtons::OkCancelCustom(install.to_owned(), later.to_owned()))
                    .blocking_show();
                if !confirmed {
                    return;
                }
                // Quit runs the normal save handshake for every document first.
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(()) => crate::menu::quit_and_restart(&app),
                    Err(error) => {
                        app.dialog().message(error.to_string()).kind(MessageDialogKind::Error).blocking_show();
                    }
                }
            }
            Ok(None) => {
                if interactive {
                    let text = if chinese { "已经是最新版本。" } else { "You're up to date." };
                    app.dialog().message(text).kind(MessageDialogKind::Info).blocking_show();
                }
            }
            Err(error) => {
                eprintln!("[vellumi] update check failed: {error}");
                if interactive {
                    app.dialog().message(error.to_string()).kind(MessageDialogKind::Error).blocking_show();
                }
            }
        }
    });
}

#[cfg(not(feature = "updater"))]
pub fn check(_app: &tauri::AppHandle, _interactive: bool) {}

pub fn enabled() -> bool {
    cfg!(feature = "updater")
}
