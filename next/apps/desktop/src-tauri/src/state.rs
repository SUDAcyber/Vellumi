use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use vellumi_core::document::DocumentSession;
use vellumi_core::settings::Settings;

/// Where a window is in the close handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseState {
    Open,
    /// Asked the editor for its last serialization; waiting for doc_flush
    /// (or the watchdog).
    Flushing,
    /// Saving / prompting; the window will be destroyed or reopened.
    Finishing,
}

pub struct DocEntry {
    pub session: DocumentSession,
    pub ready: bool,
    pub close: CloseState,
    pub autosave_generation: u64,
    /// Crash-recovery copy for an untitled document (file stem in the
    /// recovery folder).
    pub recovery_id: Option<String>,
    /// A write of ours is in flight: the watcher must not mistake it for an
    /// external change.
    pub writing: bool,
    /// The user chose "keep mine" for the current on-disk version.
    pub conflict_shown: bool,
    /// Created empty by New Note / Drafts: deleted on close if still empty.
    pub empty_draft: bool,
}

impl DocEntry {
    pub fn new(session: DocumentSession) -> Self {
        Self {
            session,
            ready: false,
            close: CloseState::Open,
            autosave_generation: 0,
            recovery_id: None,
            writing: false,
            conflict_shown: false,
            empty_draft: false,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    docs: Mutex<HashMap<String, DocEntry>>,
    settings: Mutex<Settings>,
    next_id: AtomicU64,
    quitting: AtomicBool,
    /// Quit was requested by an installed update: relaunch afterwards.
    restart: AtomicBool,
}

impl AppState {
    /// A poisoned lock only means another thread panicked mid-update; the
    /// data itself is still the best we have, so keep using it.
    pub fn docs(&self) -> MutexGuard<'_, HashMap<String, DocEntry>> {
        self.docs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn next_label(&self) -> String {
        format!("doc-{}", self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    pub fn set_quitting(&self, value: bool) {
        self.quitting.store(value, Ordering::SeqCst);
        if !value {
            self.restart.store(false, Ordering::SeqCst);
        }
    }

    #[cfg_attr(not(feature = "updater"), allow(dead_code))]
    pub fn set_restart(&self) {
        self.restart.store(true, Ordering::SeqCst);
    }

    pub fn restart_requested(&self) -> bool {
        self.restart.load(Ordering::SeqCst)
    }

    /// Label of the window already showing `path`, if any.
    pub fn label_for_path(&self, path: &Path) -> Option<String> {
        let wanted = std::fs::canonicalize(path).ok()?;
        self.docs()
            .iter()
            .find(|(_, entry)| {
                entry
                    .session
                    .path()
                    .and_then(|open| std::fs::canonicalize(open).ok())
                    .is_some_and(|open| open == wanted)
            })
            .map(|(label, _)| label.clone())
    }
}

/// Test harness: with `VELLUMI_ROUNDTRIP_OUT=<dir>` the app opens every path
/// given on the command line in a hidden window, writes what the editor would
/// save into `<dir>/<file name>`, and exits (scripts/fidelity.sh).
pub fn roundtrip_dir() -> Option<&'static PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| std::env::var_os("VELLUMI_ROUNDTRIP_OUT").map(PathBuf::from))
        .as_ref()
}

/// Test harness: with `VELLUMI_SELFTEST=<report file>` the shell drives its
/// own UI in a hidden window and reports (scripts/selftest.sh).
pub fn selftest_report() -> Option<&'static PathBuf> {
    static FILE: OnceLock<Option<PathBuf>> = OnceLock::new();
    FILE.get_or_init(|| std::env::var_os("VELLUMI_SELFTEST").map(PathBuf::from))
        .as_ref()
}

pub fn headless() -> bool {
    roundtrip_dir().is_some() || selftest_report().is_some()
}
