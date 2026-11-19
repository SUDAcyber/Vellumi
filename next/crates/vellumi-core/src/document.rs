//! Document session state. Port of the state machine in `MoltenDocument.swift`
//! (dirty tracking, normalization baseline, source mode), without NSDocument.
//!
//! Ownership split is unchanged: the editor owns the editing session and the
//! serialization (file bytes only ever come from it, see REWRITE_PLAN §2.3);
//! this type owns what is on disk and whether the window is "edited".

use std::path::PathBuf;

use serde::Serialize;

use crate::encoding::{self, SourceEncoding};
use crate::error::{CoreError, FrontMatterError};
use crate::front_matter;

/// Identity of the on-disk file when it was last read or written. Used to
/// tell our own writes apart from external changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    pub len: u64,
    /// Modification time in nanoseconds since the Unix epoch (0 = unknown).
    pub modified_ns: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSession {
    path: Option<PathBuf>,
    encoding: SourceEncoding,
    front_matter: String,
    body: String,
    saved_front_matter: String,
    saved_body: String,
    /// While true the source view is authoritative; text arriving from the
    /// (parked, stale) rich editor is ignored.
    source_mode: bool,
    fingerprint: Option<Fingerprint>,
    /// The editor normalized this file's Markdown on load (show the notice).
    normalized_on_load: bool,
    /// Content was restored from a crash-recovery copy: it is NOT what is on
    /// disk, so it must stay dirty (audit A9).
    recovered: bool,
    /// Newlines the editor cannot represent: the blank lines between the
    /// front matter and the first block, and the file's trailing newlines.
    /// The serializer drops / invents these, so they are carried here.
    edges: Edges,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Edges {
    lead: String,
    trail: String,
    /// Every line break in the body is CRLF: the editor's LF output is
    /// converted back so Windows-authored files keep their line endings.
    crlf: bool,
}

impl Edges {
    fn of(body: &str) -> Self {
        let line_feeds = body.matches('\n').count();
        let crlf = line_feeds > 0 && body.matches("\r\n").count() == line_feeds;
        Self {
            crlf,
            ..Self::edges_of(body)
        }
    }

    fn edges_of(body: &str) -> Self {
        let is_eol = |c: char| c == '\n' || c == '\r';
        let core = body.trim_start_matches(is_eol);
        let lead = body[..body.len() - core.len()].to_owned();
        let trimmed = core.trim_end_matches(is_eol);
        let trail = if core.is_empty() {
            // Empty body: new files end with a single newline.
            "\n".to_owned()
        } else {
            core[trimmed.len()..].to_owned()
        };
        Self { lead, trail, crlf: false }
    }

    /// The editor's serialization with this file's edge newlines restored.
    fn apply(&self, markdown: &str) -> String {
        let is_eol = |c: char| c == '\n' || c == '\r';
        let core = markdown.trim_matches(is_eol);
        if core.is_empty() {
            return String::new();
        }
        if self.crlf {
            let core = core.replace("\r\n", "\n").replace('\n', "\r\n");
            format!("{}{core}{}", self.lead, self.trail)
        } else {
            format!("{}{core}{}", self.lead, self.trail)
        }
    }
}

/// What the shell needs after a mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirtyState {
    pub dirty: bool,
    pub changed: bool,
}

impl DocumentSession {
    /// A new, never-saved document.
    pub fn untitled(initial_text: &str) -> Self {
        let parts = front_matter::split(initial_text);
        Self {
            edges: Edges::of(&parts.body),
            path: None,
            encoding: SourceEncoding::Utf8,
            saved_front_matter: parts.front_matter.clone(),
            saved_body: parts.body.clone(),
            front_matter: parts.front_matter,
            body: parts.body,
            source_mode: false,
            fingerprint: None,
            normalized_on_load: false,
            recovered: false,
        }
    }

    /// A document read from `path`. `bytes` must already be size-checked.
    pub fn from_bytes(path: PathBuf, bytes: &[u8], fingerprint: Fingerprint) -> Result<Self, CoreError> {
        let (text, encoding) = encoding::decode(bytes).map_err(|kind| CoreError::Encoding {
            path: path.clone(),
            kind,
        })?;
        let mut session = Self::untitled(&text);
        session.path = Some(path);
        session.encoding = encoding;
        session.fingerprint = Some(fingerprint);
        Ok(session)
    }

    /// Marks the content as restored from a recovery copy rather than read
    /// from `path`: the saved baseline is left empty so it reads as dirty.
    pub fn mark_recovered(&mut self) {
        self.recovered = true;
        self.saved_body.clear();
        self.saved_front_matter.clear();
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn encoding(&self) -> SourceEncoding {
        self.encoding
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn front_matter(&self) -> &str {
        &self.front_matter
    }

    pub fn fingerprint(&self) -> Option<Fingerprint> {
        self.fingerprint
    }

    /// "Keep mine" on a conflict: accept the current disk version as known so
    /// it is not reported again; the next save overwrites it.
    pub fn acknowledge_disk_version(&mut self, fingerprint: Fingerprint) {
        self.fingerprint = Some(fingerprint);
    }

    pub fn source_mode(&self) -> bool {
        self.source_mode
    }

    pub fn normalized_on_load(&self) -> bool {
        self.normalized_on_load
    }

    /// Clean means BOTH parts equal what was last read from / written to disk.
    pub fn is_dirty(&self) -> bool {
        self.recovered || self.body != self.saved_body || self.front_matter != self.saved_front_matter
    }

    /// Full on-disk text (front matter + body) for the source view.
    pub fn full_text(&self) -> String {
        front_matter::join(&self.front_matter, &self.body)
    }

    /// Bytes to write, in the file's original encoding (D6).
    pub fn file_bytes(&self) -> Vec<u8> {
        encoding::encode(&self.full_text(), self.encoding)
    }

    /// Debounced change from the rich editor. Ignored in source mode.
    pub fn editor_changed(&mut self, markdown: &str) -> DirtyState {
        if self.source_mode {
            return self.state(false);
        }
        let markdown = self.edges.apply(markdown);
        self.apply_body(&markdown)
    }

    /// The editor serialized freshly loaded content. Its normalized form
    /// becomes the clean baseline so opening-then-closing never rewrites a
    /// file. Never applied to a dirty document or in source mode.
    pub fn editor_normalized(&mut self, markdown: &str, changed: bool) {
        if self.is_dirty() || self.source_mode {
            return;
        }
        let markdown = self.edges.apply(markdown);
        self.body.clone_from(&markdown);
        self.saved_body = markdown;
        if changed {
            self.normalized_on_load = true;
        }
    }

    pub fn set_source_mode(&mut self, on: bool) {
        self.source_mode = on;
    }

    /// Adopts the full text from the source view (front matter re-split).
    pub fn source_changed(&mut self, full_text: &str) -> DirtyState {
        let parts = front_matter::split(full_text);
        let front_matter_changed = parts.front_matter != self.front_matter;
        self.front_matter = parts.front_matter;
        // The source view is authoritative: its edge newlines become the
        // ones the rich editor's output is conformed to afterwards.
        self.edges = Edges::of(&parts.body);
        let mut state = self.apply_body(&parts.body);
        state.changed |= front_matter_changed;
        state
    }

    /// Adopts an edited block from the front-matter editor.
    pub fn set_front_matter(&mut self, edited: &str) -> Result<DirtyState, FrontMatterError> {
        let validated = front_matter::validate_edited(edited)?;
        let changed = validated != self.front_matter;
        self.front_matter = validated;
        Ok(self.state(changed))
    }

    /// Records a completed write of `written` (the snapshot passed to the
    /// writer, NOT the live text: keystrokes during the write stay dirty).
    pub fn mark_saved(&mut self, path: PathBuf, written: &Snapshot, fingerprint: Fingerprint) {
        self.path = Some(path);
        self.saved_front_matter = written.front_matter.clone();
        self.saved_body = written.body.clone();
        self.fingerprint = Some(fingerprint);
        self.recovered = false;
    }

    /// Takes a snapshot to write.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            front_matter: self.front_matter.clone(),
            body: self.body.clone(),
            bytes: self.file_bytes(),
        }
    }

    /// Replaces everything after an external change or Revert. Returns false
    /// (and changes nothing) when the content is identical.
    pub fn reload(&mut self, bytes: &[u8], fingerprint: Fingerprint) -> Result<bool, CoreError> {
        let path = self.path.clone().unwrap_or_default();
        let (text, encoding) = encoding::decode(bytes).map_err(|kind| CoreError::Encoding { path, kind })?;
        self.fingerprint = Some(fingerprint);
        let parts = front_matter::split(&text);
        if parts.body == self.body && parts.front_matter == self.front_matter && !self.recovered {
            self.saved_body = parts.body;
            self.saved_front_matter = parts.front_matter;
            return Ok(false);
        }
        self.encoding = encoding;
        self.edges = Edges::of(&parts.body);
        self.saved_body = parts.body.clone();
        self.saved_front_matter = parts.front_matter.clone();
        self.body = parts.body;
        self.front_matter = parts.front_matter;
        self.recovered = false;
        Ok(true)
    }

    fn apply_body(&mut self, markdown: &str) -> DirtyState {
        if markdown == self.body {
            return self.state(false);
        }
        markdown.clone_into(&mut self.body);
        self.state(true)
    }

    fn state(&self, changed: bool) -> DirtyState {
        DirtyState {
            dirty: self.is_dirty(),
            changed,
        }
    }
}

/// Content captured for one write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub front_matter: String,
    pub body: String,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(text: &str) -> DocumentSession {
        DocumentSession::from_bytes(PathBuf::from("/t/a.md"), text.as_bytes(), Fingerprint::default()).unwrap()
    }

    #[test]
    fn editor_change_dirties() {
        let mut doc = open("old");
        assert!(!doc.is_dirty());
        let state = doc.editor_changed("new content");
        assert_eq!(doc.body(), "new content");
        assert!(state.dirty && state.changed);
    }

    #[test]
    fn undo_back_to_saved_clears_dirty() {
        let mut doc = open("saved state");
        doc.editor_changed("saved state edited");
        assert!(doc.is_dirty());
        assert!(!doc.editor_changed("saved state").dirty);
    }

    #[test]
    fn identical_change_does_not_dirty() {
        let mut doc = open("same");
        assert_eq!(doc.editor_changed("same"), DirtyState { dirty: false, changed: false });
    }

    #[test]
    fn front_matter_survives_edit_cycle() {
        let mut doc = open("---\ntitle: 保护测试\n---\n\n原始正文\n");
        assert_eq!(doc.body(), "\n原始正文\n");
        doc.editor_changed("\n编辑后的正文\n");
        assert_eq!(doc.file_bytes(), "---\ntitle: 保护测试\n---\n\n编辑后的正文\n".as_bytes());
    }

    #[test]
    fn normalization_baseline_keeps_untouched_doc_clean() {
        let mut doc = open("* a\n");
        doc.editor_normalized("- a\n", true);
        assert!(!doc.is_dirty());
        assert!(doc.normalized_on_load());
        assert_eq!(doc.editor_changed("- a\n"), DirtyState { dirty: false, changed: false });
    }

    #[test]
    fn normalization_does_not_clear_pending_edits() {
        let mut doc = open("x");
        doc.editor_changed("y");
        doc.editor_normalized("x", false);
        assert!(doc.is_dirty());
        assert_eq!(doc.body(), "y");
    }

    #[test]
    fn source_mode_ignores_bridge_and_resplits() {
        let mut doc = open("---\ntitle: a\n---\n正文\n");
        doc.set_source_mode(true);
        assert!(!doc.editor_changed("stale").changed);
        let state = doc.source_changed("---\ntitle: b\n---\n新正文\n");
        assert!(state.dirty && state.changed);
        assert_eq!(doc.front_matter(), "---\ntitle: b\n---\n");
        assert_eq!(doc.body(), "新正文\n");
    }

    #[test]
    fn front_matter_only_edit_dirties() {
        let mut doc = open("---\ntitle: a\n---\n正文\n");
        let state = doc.source_changed("---\ntitle: b\n---\n正文\n");
        assert!(state.dirty && state.changed);
    }

    #[test]
    fn undo_body_keeps_dirty_when_front_matter_differs() {
        let mut doc = open("---\ntitle: a\n---\n正文\n");
        doc.set_front_matter("---\ntitle: b\n---\n").unwrap();
        doc.editor_changed("改");
        assert!(doc.editor_changed("正文\n").dirty);
    }

    #[test]
    fn removing_front_matter() {
        let mut doc = open("---\ntitle: a\n---\n正文\n");
        doc.set_front_matter("").unwrap();
        assert_eq!(doc.full_text(), "正文\n");
        assert!(doc.is_dirty());
    }

    #[test]
    fn save_records_snapshot_not_live_text() {
        let mut doc = open("a");
        doc.editor_changed("b");
        let snapshot = doc.snapshot();
        doc.editor_changed("c"); // typed during the write
        doc.mark_saved(PathBuf::from("/t/a.md"), &snapshot, Fingerprint::default());
        assert!(doc.is_dirty());
        assert!(!doc.editor_changed("b").dirty);
    }

    #[test]
    fn recovered_content_stays_dirty() {
        let mut doc = DocumentSession::untitled("恢复的 500 字");
        doc.mark_recovered();
        doc.editor_changed("恢复的 500 字x");
        assert!(doc.editor_changed("恢复的 500 字").dirty, "audit A9: recovery is not the saved baseline");
    }

    #[test]
    fn reload_replaces_content_and_baseline() {
        let mut doc = open("a");
        assert!(doc.reload(b"---\nk: v\n---\nb", Fingerprint { len: 1, modified_ns: 2 }).unwrap());
        assert_eq!(doc.body(), "b");
        assert!(!doc.is_dirty());
        assert!(!doc.reload(b"---\nk: v\n---\nb", Fingerprint::default()).unwrap());
    }

    #[test]
    fn blank_line_after_front_matter_survives_the_editor() {
        let mut doc = open("---\ntitle: x\n---\n\n# 标题\n");
        // The serializer never emits leading newlines and adds a trailing blank line.
        doc.editor_normalized("# 标题\n\n", false);
        assert_eq!(doc.full_text(), "---\ntitle: x\n---\n\n# 标题\n");
        assert!(!doc.is_dirty());
        doc.editor_changed("# 新标题\n\n");
        assert_eq!(doc.full_text(), "---\ntitle: x\n---\n\n# 新标题\n");
    }

    #[test]
    fn missing_final_newline_is_kept_missing() {
        let mut doc = open("末尾没有换行");
        doc.editor_changed("末尾没有换行，改过\n");
        assert_eq!(doc.full_text(), "末尾没有换行，改过");
    }

    #[test]
    fn new_documents_end_with_one_newline() {
        let mut doc = DocumentSession::untitled("");
        doc.editor_changed("第一行\n\n");
        assert_eq!(doc.full_text(), "第一行\n");
        doc.editor_changed("\n");
        assert_eq!(doc.full_text(), "");
    }

    #[test]
    fn source_edits_redefine_the_edges() {
        let mut doc = open("正文\n");
        doc.set_source_mode(true);
        doc.source_changed("\n\n正文\n\n\n");
        doc.set_source_mode(false);
        doc.editor_changed("正文改\n");
        assert_eq!(doc.full_text(), "\n\n正文改\n\n\n");
    }

    #[test]
    fn crlf_bodies_keep_crlf() {
        let mut doc = open("---\r\nk: v\r\n---\r\n\r\n# 标题\r\n\r\n正文\r\n");
        doc.editor_normalized("# 标题\n\n正文\n", false);
        assert_eq!(doc.full_text(), "---\r\nk: v\r\n---\r\n\r\n# 标题\r\n\r\n正文\r\n");
        assert!(!doc.is_dirty());
    }

    #[test]
    fn mixed_line_endings_are_left_to_the_editor() {
        let mut doc = open("a\r\nb\nc\n");
        doc.editor_changed("a\nb\nc\n");
        assert_eq!(doc.full_text(), "a\nb\nc\n");
    }

    #[test]
    fn utf16_file_is_written_back_as_utf16() {
        let bytes = encoding::encode("正文", SourceEncoding::Utf16Le);
        let mut doc = DocumentSession::from_bytes(PathBuf::from("/t/u.md"), &bytes, Fingerprint::default()).unwrap();
        doc.editor_changed("改过的正文");
        assert_eq!(doc.file_bytes(), encoding::encode("改过的正文", SourceEncoding::Utf16Le));
    }
}
