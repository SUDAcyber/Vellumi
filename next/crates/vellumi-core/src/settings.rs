//! App settings (Swift: the `Vellumi.*` UserDefaults keys, REWRITE_PLAN §1.6).
//! Stored as JSON by the shell; unknown/missing keys fall back to defaults so
//! old and new versions can read each other's files.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarPanel {
    #[default]
    Files,
    Outline,
    Search,
}

/// A project: a reference to a folder of Markdown files (never a copy).
/// Swift kept a security-scoped bookmark; without the sandbox (D2) the path
/// is enough, and it survives in plain JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    /// Milliseconds since the Unix epoch.
    pub added_ms: u64,
}

pub const THEMES: &[&str] = &["frame", "nord", "classic"];
pub const FONT_SCHEMES: &[&str] = &["default", "serif", "sans"];
pub const ZOOM_RANGE: (f64, f64) = (0.5, 3.0);
pub const ZOOM_STEP: f64 = 1.1;
pub const LINE_HEIGHT_RANGE: (f64, f64) = (1.2, 2.2);
pub const PARAGRAPH_SPACING_RANGE: (f64, f64) = (0.2, 2.0);
pub const LINE_WIDTH_RANGE: (f64, f64) = (560.0, 1080.0);
pub const MAX_RECENTS: usize = 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub appearance: Appearance,
    pub theme: String,
    pub zoom: f64,
    pub typewriter: bool,
    pub focus_mode: bool,
    pub spellcheck: bool,
    pub smart_punctuation: bool,
    pub font_scheme: String,
    /// 0 = theme default.
    pub line_height: f64,
    /// em; 0 = theme default.
    pub paragraph_spacing: f64,
    /// px; 0 = adaptive (clamp(760px, 75%, 1200px)).
    pub line_width: f64,
    /// 0 = off.
    pub word_goal: u32,
    pub image_folder: String,
    pub sidebar_visible: bool,
    pub sidebar_panel: SidebarPanel,
    /// 0 words + reading time, 1 characters, 2 paragraphs.
    pub stats_mode: u8,
    pub has_shown_welcome: bool,
    pub recents: Vec<String>,
    pub projects: Vec<Project>,
    /// Opt-in: File ▸ New creates a real file here (Swift: Drafts folder).
    pub drafts_folder: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            theme: "frame".to_owned(),
            zoom: 1.0,
            typewriter: false,
            focus_mode: false,
            spellcheck: false,
            smart_punctuation: false,
            font_scheme: "default".to_owned(),
            line_height: 0.0,
            paragraph_spacing: 0.0,
            line_width: 0.0,
            word_goal: 0,
            image_folder: crate::assets::DEFAULT_IMAGE_FOLDER.to_owned(),
            sidebar_visible: true,
            sidebar_panel: SidebarPanel::Files,
            stats_mode: 0,
            has_shown_welcome: false,
            recents: Vec::new(),
            projects: Vec::new(),
            drafts_folder: None,
        }
    }
}

fn clamp_or_zero(value: f64, (low, high): (f64, f64)) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        0.0
    } else {
        value.clamp(low, high)
    }
}

impl Settings {
    /// Brings every field back into its valid range (hand-edited files,
    /// older versions). Never fails.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        if !THEMES.contains(&self.theme.as_str()) {
            "frame".clone_into(&mut self.theme);
        }
        if !FONT_SCHEMES.contains(&self.font_scheme.as_str()) {
            "default".clone_into(&mut self.font_scheme);
        }
        self.zoom = if self.zoom.is_finite() && self.zoom > 0.0 {
            self.zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1)
        } else {
            1.0
        };
        self.line_height = clamp_or_zero(self.line_height, LINE_HEIGHT_RANGE);
        self.paragraph_spacing = clamp_or_zero(self.paragraph_spacing, PARAGRAPH_SPACING_RANGE);
        self.line_width = clamp_or_zero(self.line_width, LINE_WIDTH_RANGE);
        self.image_folder = crate::assets::image_folder_name(&self.image_folder);
        self.stats_mode %= 3;
        self.recents.truncate(MAX_RECENTS);
        self
    }

    /// Adds a project for `path` (already canonical). Re-adding the same
    /// folder keeps its id and name instead of duplicating it.
    pub fn add_project(&mut self, path: &str, name: &str, id: &str, added_ms: u64) -> Project {
        if let Some(existing) = self.projects.iter().find(|p| p.path == path) {
            return existing.clone();
        }
        let project = Project {
            id: id.to_owned(),
            name: name.to_owned(),
            path: path.to_owned(),
            added_ms,
        };
        self.projects.push(project.clone());
        project
    }

    pub fn remove_project(&mut self, id: &str) {
        self.projects.retain(|p| p.id != id);
    }

    /// Display name only; the folder is untouched. Blank names are ignored.
    pub fn rename_project(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == id) {
            name.clone_into(&mut project.name);
        }
    }

    /// Points a project whose folder moved at its new location.
    pub fn relocate_project(&mut self, id: &str, path: &str) {
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == id) {
            path.clone_into(&mut project.path);
        }
    }

    /// Moves `path` to the front of the recent-documents list.
    pub fn note_recent(&mut self, path: &str) {
        self.recents.retain(|existing| existing != path);
        self.recents.insert(0, path.to_owned());
        self.recents.truncate(MAX_RECENTS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_use_defaults() {
        let parsed: Settings = serde_json::from_str("{\"theme\":\"nord\",\"futureKey\":1}").unwrap();
        assert_eq!(parsed.theme, "nord");
        assert_eq!(parsed.zoom, 1.0);
        assert!(parsed.sidebar_visible);
    }

    #[test]
    fn sanitize_clamps_ranges() {
        let settings = Settings {
            theme: "neon".into(),
            zoom: 9.0,
            line_height: 5.0,
            paragraph_spacing: -1.0,
            line_width: 100.0,
            image_folder: "../".into(),
            stats_mode: 7,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(settings.theme, "frame");
        assert_eq!(settings.zoom, 3.0);
        assert_eq!(settings.line_height, 2.2);
        assert_eq!(settings.paragraph_spacing, 0.0);
        assert_eq!(settings.line_width, 560.0);
        assert_eq!(settings.image_folder, "assets");
        assert_eq!(settings.stats_mode, 1);
    }

    #[test]
    fn projects_add_dedupe_rename_relocate_remove() {
        let mut settings = Settings::default();
        let a = settings.add_project("/n/a", "a", "p1", 1);
        let again = settings.add_project("/n/a", "other", "p2", 2);
        assert_eq!(a, again, "same folder keeps its project");
        assert_eq!(settings.projects.len(), 1);
        settings.rename_project("p1", "  笔记  ");
        settings.rename_project("p1", "   ");
        assert_eq!(settings.projects[0].name, "笔记");
        settings.relocate_project("p1", "/n/moved");
        assert_eq!(settings.projects[0].path, "/n/moved");
        settings.remove_project("p1");
        assert!(settings.projects.is_empty());
    }

    #[test]
    fn older_settings_files_without_projects_still_load() {
        let parsed: Settings = serde_json::from_str("{\"theme\":\"classic\",\"recents\":[\"/a.md\"]}").unwrap();
        assert!(parsed.projects.is_empty());
        assert_eq!(parsed.drafts_folder, None);
    }

    #[test]
    fn recents_dedupe_and_cap() {
        let mut settings = Settings::default();
        for index in 0..25 {
            settings.note_recent(&format!("/f{index}.md"));
        }
        settings.note_recent("/f3.md");
        assert_eq!(settings.recents.len(), MAX_RECENTS);
        assert_eq!(settings.recents[0], "/f3.md");
        assert_eq!(settings.recents.iter().filter(|p| *p == "/f3.md").count(), 1);
    }
}
