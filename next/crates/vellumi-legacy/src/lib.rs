//! Import from Vellumi 0.5.x (Swift). The old app kept everything in its
//! sandboxed UserDefaults plist (REWRITE_PLAN §1.6). This crate reads that
//! file — never writes it, so going back to 0.5.x stays possible — and maps
//! the keys onto [`Settings`] (decision D7: one-way migration).
//!
//! Security-scoped bookmarks (projects, Drafts folder) are turned into paths
//! by a resolver the caller supplies (macOS reads the path stored inside the
//! bookmark without resolving it, so no sandbox grant is needed).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::path::{Path, PathBuf};

use plist::{Dictionary, Value};
use vellumi_core::settings::{Appearance, Settings, SidebarPanel};

#[derive(Debug, thiserror::Error)]
pub enum LegacyError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: plist::Error,
    },
    #[error("{path} is not a preferences dictionary")]
    NotADictionary { path: PathBuf },
}

/// Where 0.5.x kept its preferences, most likely first.
pub fn candidate_paths(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Library/Containers/com.aaron.vellumi/Data/Library/Preferences/com.aaron.vellumi.plist"),
        home.join("Library/Preferences/com.aaron.vellumi.plist"),
    ]
}

pub fn read_preferences(path: &Path) -> Result<Dictionary, LegacyError> {
    let value = Value::from_file(path).map_err(|source| LegacyError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    value.into_dictionary().ok_or_else(|| LegacyError::NotADictionary {
        path: path.to_path_buf(),
    })
}

/// What was imported, for the one-time notice and the log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Keys whose values were taken over.
    pub imported: Vec<&'static str>,
    pub projects: usize,
    /// Projects whose folder could not be recovered from the bookmark.
    pub projects_unresolved: usize,
    pub drafts_folder: bool,
    /// Folders 0.5.x had been granted access to (for images); listed so the
    /// UI can offer them as projects. Not imported automatically.
    pub authorized_folders: Vec<String>,
}

fn int(prefs: &Dictionary, key: &str) -> Option<i64> {
    match prefs.get(key)? {
        Value::Integer(n) => n.as_signed(),
        Value::Real(r) => Some(r.round() as i64),
        Value::Boolean(b) => Some(i64::from(*b)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn real(prefs: &Dictionary, key: &str) -> Option<f64> {
    match prefs.get(key)? {
        Value::Real(r) => Some(*r),
        #[allow(clippy::cast_precision_loss)]
        Value::Integer(n) => n.as_signed().map(|n| n as f64),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn boolean(prefs: &Dictionary, key: &str) -> Option<bool> {
    match prefs.get(key)? {
        Value::Boolean(b) => Some(*b),
        Value::Integer(n) => n.as_signed().map(|n| n != 0),
        Value::String(s) => match s.as_str() {
            "1" | "YES" | "true" => Some(true),
            "0" | "NO" | "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn string(prefs: &Dictionary, key: &str) -> Option<String> {
    prefs.get(key)?.as_string().map(str::to_owned)
}

/// Maps 0.5.x preferences onto `base` (normally the defaults).
/// `bookmark_path` turns bookmark data into a folder path.
pub fn migrate(prefs: &Dictionary, base: Settings, bookmark_path: &dyn Fn(&[u8]) -> Option<String>) -> (Settings, Report) {
    let mut settings = base;
    let mut report = Report::default();
    let took = |key: &'static str, report: &mut Report| report.imported.push(key);

    if let Some(mode) = int(prefs, "Vellumi.appearanceMode") {
        settings.appearance = match mode {
            1 => Appearance::Light,
            2 => Appearance::Dark,
            _ => Appearance::System,
        };
        took("appearance", &mut report);
    }
    if let Some(theme) = string(prefs, "Vellumi.editorTheme") {
        settings.theme = theme;
        took("theme", &mut report);
    }
    if let Some(zoom) = real(prefs, "Vellumi.editorZoom").filter(|z| *z > 0.0) {
        settings.zoom = zoom;
        took("zoom", &mut report);
    }
    for (key, name, target) in [
        ("Vellumi.spellcheck", "spellcheck", &mut settings.spellcheck),
        ("Vellumi.typewriter", "typewriter", &mut settings.typewriter),
        ("Vellumi.focusMode", "focusMode", &mut settings.focus_mode),
        ("Vellumi.smartPunctuation", "smartPunctuation", &mut settings.smart_punctuation),
        ("Vellumi.hasShownWelcome", "hasShownWelcome", &mut settings.has_shown_welcome),
    ] {
        if let Some(value) = boolean(prefs, key) {
            *target = value;
            report.imported.push(name);
        }
    }
    if let Some(scheme) = string(prefs, "Vellumi.fontScheme") {
        settings.font_scheme = scheme;
        took("fontScheme", &mut report);
    }
    for (key, name, target) in [
        ("Vellumi.lineHeight", "lineHeight", &mut settings.line_height),
        ("Vellumi.paragraphSpacing", "paragraphSpacing", &mut settings.paragraph_spacing),
        ("Vellumi.lineWidth", "lineWidth", &mut settings.line_width),
    ] {
        if let Some(value) = real(prefs, key) {
            *target = value;
            report.imported.push(name);
        }
    }
    if let Some(goal) = int(prefs, "Vellumi.wordGoal") {
        settings.word_goal = u32::try_from(goal.max(0)).unwrap_or(0);
        took("wordGoal", &mut report);
    }
    if let Some(folder) = string(prefs, "Vellumi.imageFolderName") {
        settings.image_folder = folder;
        took("imageFolder", &mut report);
    }
    if let Some(mode) = int(prefs, "Vellumi.statsDisplayMode") {
        settings.stats_mode = u8::try_from(mode.clamp(0, 2)).unwrap_or(0);
        took("statsMode", &mut report);
    }
    // Two independent sidebars became one with panels: outline wins (it was
    // the inner, more specific one), then files.
    let outline = boolean(prefs, "Vellumi.showsOutline");
    let files = boolean(prefs, "Vellumi.showsFileTree");
    if outline.is_some() || files.is_some() {
        settings.sidebar_visible = outline == Some(true) || files == Some(true);
        settings.sidebar_panel = if outline == Some(true) {
            SidebarPanel::Outline
        } else {
            SidebarPanel::Files
        };
        took("sidebar", &mut report);
    }

    // Projects: PropertyListEncoder-encoded [{id, name, folderBookmark, dateAdded}].
    if let Some(Value::Data(encoded)) = prefs.get("Vellumi.projects.list") {
        if let Ok(Value::Array(projects)) = Value::from_reader(std::io::Cursor::new(encoded)) {
            for project in projects {
                let Some(dict) = project.as_dictionary() else { continue };
                let name = dict.get("name").and_then(Value::as_string).unwrap_or_default().to_owned();
                let id = dict
                    .get("id")
                    .and_then(Value::as_string)
                    .map_or_else(|| format!("legacy-{}", report.projects + report.projects_unresolved), |id| format!("legacy-{id}"));
                let added_ms = match dict.get("dateAdded") {
                    Some(Value::Date(date)) => std::time::SystemTime::from(*date)
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0)),
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    Some(Value::Real(seconds)) => ((seconds + 978_307_200.0) * 1000.0).max(0.0) as u64,
                    _ => 0,
                };
                match dict.get("folderBookmark").and_then(Value::as_data).and_then(bookmark_path) {
                    Some(path) => {
                        let display = if name.is_empty() {
                            Path::new(&path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned())
                        } else {
                            name
                        };
                        settings.add_project(&path, &display, &id, added_ms);
                        report.projects += 1;
                    }
                    None => report.projects_unresolved += 1,
                }
            }
        }
    }
    if let Some(path) = prefs
        .get("Vellumi.drafts.folderBookmark")
        .and_then(Value::as_data)
        .and_then(bookmark_path)
    {
        settings.drafts_folder = Some(path);
        report.drafts_folder = true;
    }
    report.authorized_folders = prefs
        .keys()
        .filter_map(|key| key.strip_prefix("Vellumi.folderBookmark."))
        .map(str::to_owned)
        .collect();
    report.authorized_folders.sort();

    (settings.sanitized(), report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(entries: Vec<(&str, Value)>) -> Dictionary {
        entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
    }

    fn no_bookmarks(_: &[u8]) -> Option<String> {
        None
    }

    #[test]
    fn maps_view_settings() {
        let prefs = prefs(vec![
            ("Vellumi.appearanceMode", Value::Integer(2.into())),
            ("Vellumi.editorTheme", Value::String("nord".into())),
            ("Vellumi.editorZoom", Value::String("0.9090909090909091".into())),
            ("Vellumi.typewriter", Value::Boolean(true)),
            ("Vellumi.spellcheck", Value::Integer(1.into())),
            ("Vellumi.fontScheme", Value::String("serif".into())),
            ("Vellumi.lineHeight", Value::Real(1.8)),
            ("Vellumi.lineWidth", Value::Real(0.0)),
            ("Vellumi.wordGoal", Value::Integer(2000.into())),
            ("Vellumi.imageFolderName", Value::String("images".into())),
            ("Vellumi.statsDisplayMode", Value::Integer(2.into())),
            ("Vellumi.hasShownWelcome", Value::Boolean(true)),
            ("Vellumi.showsOutline", Value::Boolean(true)),
            ("Vellumi.showsFileTree", Value::Boolean(false)),
        ]);
        let (s, report) = migrate(&prefs, Settings::default(), &no_bookmarks);
        assert_eq!(s.appearance, Appearance::Dark);
        assert_eq!(s.theme, "nord");
        assert!((s.zoom - 0.909).abs() < 0.001);
        assert!(s.typewriter && s.spellcheck && s.has_shown_welcome);
        assert_eq!(s.font_scheme, "serif");
        assert!((s.line_height - 1.8).abs() < 1e-9);
        assert_eq!(s.line_width, 0.0, "0 stays adaptive");
        assert_eq!(s.word_goal, 2000);
        assert_eq!(s.image_folder, "images");
        assert_eq!(s.stats_mode, 2);
        assert!(s.sidebar_visible);
        assert_eq!(s.sidebar_panel, SidebarPanel::Outline);
        assert!(report.imported.contains(&"zoom"));
    }

    #[test]
    fn invalid_values_are_sanitized_not_trusted() {
        let prefs = prefs(vec![
            ("Vellumi.editorTheme", Value::String("neon".into())),
            ("Vellumi.editorZoom", Value::Real(12.0)),
            ("Vellumi.imageFolderName", Value::String("../x".into())),
        ]);
        let (s, _) = migrate(&prefs, Settings::default(), &no_bookmarks);
        assert_eq!(s.theme, "frame");
        assert_eq!(s.zoom, 3.0);
        assert_eq!(s.image_folder, "..x");
    }

    #[test]
    fn projects_and_drafts_come_from_bookmarks() {
        let project = |name: &str, bookmark: &[u8]| {
            let mut d = Dictionary::new();
            d.insert("id".into(), Value::String("UUID-1".into()));
            d.insert("name".into(), Value::String(name.into()));
            d.insert("folderBookmark".into(), Value::Data(bookmark.to_vec()));
            d.insert("dateAdded".into(), Value::Real(0.0));
            Value::Dictionary(d)
        };
        let mut encoded = Vec::new();
        Value::Array(vec![project("笔记", "ok:/Users/x/笔记".as_bytes()), project("丢失", b"broken")])
            .to_writer_binary(&mut encoded)
            .unwrap();
        let prefs = prefs(vec![
            ("Vellumi.projects.list", Value::Data(encoded)),
            ("Vellumi.drafts.folderBookmark", Value::Data(b"ok:/Users/x/Drafts".to_vec())),
            ("Vellumi.folderBookmark./Users/x/Documents", Value::Data(vec![1])),
        ]);
        let resolver = |data: &[u8]| std::str::from_utf8(data).ok()?.strip_prefix("ok:").map(str::to_owned);
        let (s, report) = migrate(&prefs, Settings::default(), &resolver);
        assert_eq!(s.projects.len(), 1);
        assert_eq!(s.projects[0].name, "笔记");
        assert_eq!(s.projects[0].path, "/Users/x/笔记");
        assert_eq!(s.projects[0].added_ms, 978_307_200_000, "Apple reference date");
        assert_eq!(report.projects_unresolved, 1);
        assert_eq!(s.drafts_folder.as_deref(), Some("/Users/x/Drafts"));
        assert_eq!(report.authorized_folders, vec!["/Users/x/Documents".to_owned()]);
    }

    #[test]
    fn empty_preferences_change_nothing() {
        let (s, report) = migrate(&Dictionary::new(), Settings::default(), &no_bookmarks);
        assert_eq!(s, Settings::default());
        assert!(report.imported.is_empty());
    }

    #[test]
    fn reads_binary_plists_from_disk() {
        let dir = std::env::temp_dir().join(format!("vellumi-legacy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.plist");
        Value::Dictionary(prefs(vec![("Vellumi.editorTheme", Value::String("classic".into()))]))
            .to_file_binary(&path)
            .unwrap();
        let read = read_preferences(&path).unwrap();
        assert_eq!(read.get("Vellumi.editorTheme").and_then(Value::as_string), Some("classic"));
        assert!(read_preferences(&dir.join("missing.plist")).is_err());
    }
}
