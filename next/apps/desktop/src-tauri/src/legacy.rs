//! One-time import of Vellumi 0.5.x preferences (P7, decision D7).
//! Runs only when the new app has no settings.json yet; the old plist is read,
//! never modified.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use vellumi_core::settings::Settings;
use vellumi_legacy::Report;

/// The folder path stored inside a (security-scoped) bookmark, read without
/// resolving it — no sandbox grant needed, works for folders that moved.
#[cfg(target_os = "macos")]
pub fn bookmark_path(data: &[u8]) -> Option<String> {
    use objc2_foundation::{NSArray, NSData, NSString, NSURLPathKey, NSURL};
    let data = NSData::with_bytes(data);
    // SAFETY: NSURLPathKey is an immutable framework constant.
    let key = unsafe { NSURLPathKey };
    let keys = NSArray::from_slice(&[key]);
    let values = NSURL::resourceValuesForKeys_fromBookmarkData(&keys, &data)?;
    let value = values.objectForKey(key)?;
    let path = value.downcast_ref::<NSString>()?;
    Some(path.to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn bookmark_path(_data: &[u8]) -> Option<String> {
    None
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Reads the first 0.5.x preferences file found and maps it onto `base`.
pub fn import(base: Settings) -> Option<(Settings, Report, PathBuf)> {
    let home = home()?;
    vellumi_legacy::candidate_paths(&home).into_iter().find_map(|path| {
        let prefs = vellumi_legacy::read_preferences(&path).ok()?;
        let (settings, report) = vellumi_legacy::migrate(&prefs, base.clone(), &bookmark_path);
        Some((settings, report, path))
    })
}

/// A notice for the first document window ("imported your 0.5.x settings").
#[derive(Default)]
pub struct MigrationNotice(pub Mutex<Option<String>>);

pub fn notice_text(report: &Report) -> String {
    let chinese = crate::windows::is_chinese();
    let mut parts = Vec::new();
    if !report.imported.is_empty() {
        parts.push(if chinese { "外观与编辑设置".to_owned() } else { "appearance and editing settings".to_owned() });
    }
    if report.projects > 0 {
        parts.push(if chinese { format!("{} 个项目", report.projects) } else { format!("{} projects", report.projects) });
    }
    if report.drafts_folder {
        parts.push(if chinese { "草稿文件夹".to_owned() } else { "the Drafts folder".to_owned() });
    }
    if chinese {
        format!("已从旧版 Vellumi 导入：{}", parts.join("、"))
    } else {
        format!("Imported from Vellumi 0.5: {}", parts.join(", "))
    }
}

/// Shell asks once at boot; the first window to ask gets the notice.
#[tauri::command]
pub async fn take_migration_notice(app: AppHandle) -> crate::error::AppResult<Option<String>> {
    Ok(app
        .try_state::<MigrationNotice>()
        .and_then(|notice| notice.0.lock().ok().and_then(|mut slot| slot.take())))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DryRun<'a> {
    source: String,
    imported_keys: &'a [&'static str],
    projects: Vec<(String, String)>,
    projects_unresolved: usize,
    drafts_folder: Option<String>,
    authorized_folders: &'a [String],
    resulting_settings: &'a Settings,
}

/// `vellumi-desktop --legacy-report`: what an import would do, printed as
/// JSON. Writes nothing.
pub fn dry_run() -> String {
    match import(Settings::default()) {
        Some((settings, report, path)) => serde_json::to_string_pretty(&DryRun {
            source: path.to_string_lossy().into_owned(),
            imported_keys: &report.imported,
            projects: settings.projects.iter().map(|p| (p.name.clone(), p.path.clone())).collect(),
            projects_unresolved: report.projects_unresolved,
            drafts_folder: settings.drafts_folder.clone(),
            authorized_folders: &report.authorized_folders,
            resulting_settings: &settings,
        })
        .unwrap_or_else(|error| format!("{{\"error\": \"{error}\"}}")),
        None => "{\"source\": null}".to_owned(),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// VELLUMI_TEST_BOOKMARK=<file with bookmark data> VELLUMI_TEST_BOOKMARK_PATH=<expected path>
    #[test]
    #[ignore = "needs a bookmark made by Foundation (see scripts in the P7 notes)"]
    #[allow(clippy::unwrap_used)]
    fn reads_the_path_out_of_a_real_bookmark() {
        let data = std::fs::read(std::env::var("VELLUMI_TEST_BOOKMARK").unwrap()).unwrap();
        let expected = std::env::var("VELLUMI_TEST_BOOKMARK_PATH").unwrap();
        assert_eq!(super::bookmark_path(&data).as_deref(), Some(expected.as_str()));
        assert_eq!(super::bookmark_path(b"not a bookmark"), None);
    }
}
