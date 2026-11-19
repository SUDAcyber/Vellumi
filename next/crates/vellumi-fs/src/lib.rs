//! File IO for documents and assets. Everything returns `CoreError` with the
//! operation and path attached; nothing here panics on user data.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use vellumi_core::document::{DocumentSession, Fingerprint};
use vellumi_core::tree::{self, FileMatches, TreeNode};
use vellumi_core::{CoreError, MAX_FILE_SIZE};

fn io<'a>(op: &'static str, path: &'a Path) -> impl FnOnce(std::io::Error) -> CoreError + 'a {
    move |source| CoreError::Io {
        op,
        path: path.to_path_buf(),
        source,
    }
}

/// Current fingerprint of `path`.
pub fn fingerprint(path: &Path) -> Result<Fingerprint, CoreError> {
    let meta = fs::metadata(path).map_err(io("stat", path))?;
    Ok(fingerprint_of(&meta))
}

fn fingerprint_of(meta: &fs::Metadata) -> Fingerprint {
    let modified_ns = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    Fingerprint {
        len: meta.len(),
        modified_ns,
    }
}

/// Reads at most `limit` bytes; a larger file is refused before it is read.
pub fn read_limited(path: &Path, limit: u64) -> Result<(Vec<u8>, Fingerprint), CoreError> {
    let mut file = File::open(path).map_err(io("open", path))?;
    let meta = file.metadata().map_err(io("stat", path))?;
    if meta.len() > limit {
        return Err(CoreError::TooLarge {
            path: path.to_path_buf(),
            size: meta.len(),
            limit,
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    // `take` guards against the file growing between stat and read.
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io("read", path))?;
    let read_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if read_len > limit {
        return Err(CoreError::TooLarge {
            path: path.to_path_buf(),
            size: read_len,
            limit,
        });
    }
    Ok((bytes, fingerprint_of(&meta)))
}

/// Opens a Markdown document.
pub fn open_document(path: &Path) -> Result<DocumentSession, CoreError> {
    let (bytes, fingerprint) = read_limited(path, MAX_FILE_SIZE)?;
    DocumentSession::from_bytes(path.to_path_buf(), &bytes, fingerprint)
}

/// Writes `bytes` atomically: a temp file in the same folder, fsync, then
/// rename over the target, so a crash never leaves a half-written document.
/// Existing permissions are carried over.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<Fingerprint, CoreError> {
    let parent = path.parent().ok_or_else(|| CoreError::NoParent {
        path: path.to_path_buf(),
    })?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp: PathBuf = parent.join(format!(".{name}.vellumi-{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(io("create temp file", &temp))?;
        file.write_all(bytes).map_err(io("write", &temp))?;
        file.sync_all().map_err(io("sync", &temp))?;
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&temp, meta.permissions()).map_err(io("set permissions", &temp))?;
        }
        fs::rename(&temp, path).map_err(io("replace", path))
    })();
    if result.is_err() {
        // Best effort: never leave our temp file behind in the user's folder.
        let _ = fs::remove_file(&temp);
    }
    result?;
    fingerprint(path)
}

/// Creates `path` exclusively (never overwrites) with `bytes`.
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io("create", path))?;
    file.write_all(bytes).map_err(io("write", path))
}

pub fn create_dir_all(path: &Path) -> Result<(), CoreError> {
    fs::create_dir_all(path).map_err(io("create folder", path))
}

/// Recursive Markdown tree of `folder` (Swift: `MoltenFileTree.build`).
/// Hidden entries skipped, folders without Markdown pruned, folders first in
/// natural order, and the walk bounded by depth, kept nodes and entries seen.
pub fn build_tree(folder: &Path) -> Vec<TreeNode> {
    struct Budget {
        nodes: usize,
        scanned: usize,
    }
    fn level(dir: &Path, depth: usize, budget: &mut Budget) -> Vec<TreeNode> {
        if depth > tree::MAX_DEPTH || budget.nodes >= tree::MAX_NODES || budget.scanned >= tree::MAX_SCANNED_ENTRIES {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
        let mut folders = Vec::new();
        let mut files = Vec::new();
        for entry in entries.flatten() {
            budget.scanned += 1;
            if budget.nodes >= tree::MAX_NODES || budget.scanned >= tree::MAX_SCANNED_ENTRIES {
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            // Follow symlinks for the kind, but never recurse into a link
            // (cycles); linked files are listed.
            let is_dir = kind.is_dir();
            let is_file = kind.is_file() || (kind.is_symlink() && path.is_file());
            if is_dir {
                let children = level(&path, depth + 1, budget);
                if children.is_empty() {
                    continue;
                }
                budget.nodes += 1;
                folders.push(TreeNode {
                    name,
                    path: path.to_string_lossy().into_owned(),
                    is_dir: true,
                    children,
                });
            } else if is_file && vellumi_core::is_supported_file_name(&name) {
                budget.nodes += 1;
                files.push(TreeNode {
                    name,
                    path: path.to_string_lossy().into_owned(),
                    is_dir: false,
                    children: Vec::new(),
                });
            }
        }
        folders.sort_by(|a, b| tree::natural_cmp(&a.name, &b.name));
        files.sort_by(|a, b| tree::natural_cmp(&a.name, &b.name));
        folders.extend(files);
        folders
    }
    level(folder, 1, &mut Budget { nodes: 0, scanned: 0 })
}

/// Find in files under `folder` (recursive, same budgets as the tree).
/// Unreadable, oversized or non-text files are skipped silently.
pub fn search_folder(folder: &Path, query: &str, case_sensitive: bool) -> Vec<FileMatches> {
    let tree = TreeNode {
        name: String::new(),
        path: folder.to_string_lossy().into_owned(),
        is_dir: true,
        children: build_tree(folder),
    };
    let mut results = Vec::new();
    for file in tree.files().into_iter().take(tree::MAX_SEARCH_FILES) {
        let path = Path::new(&file.path);
        let Ok((bytes, _)) = read_limited(path, MAX_FILE_SIZE) else { continue };
        let Ok((text, _)) = vellumi_core::encoding::decode(&bytes) else { continue };
        let matches = tree::line_matches(&text, query, case_sensitive, tree::MAX_MATCHES_PER_FILE);
        if !matches.is_empty() {
            results.push(FileMatches {
                path: file.path.clone(),
                name: file.name.clone(),
                matches,
            });
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vellumi-fs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let dir = temp_dir("atomic");
        let path = dir.join("a.md");
        fs::write(&path, "old").unwrap();
        let fp = write_atomic(&path, "新内容".as_bytes()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "新内容");
        assert_eq!(fp.len, "新内容".len() as u64);
        let leftovers: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(Result::ok).collect();
        assert_eq!(leftovers.len(), 1);
    }

    #[test]
    fn oversized_files_are_refused() {
        let dir = temp_dir("large");
        let path = dir.join("big.md");
        fs::write(&path, vec![b'a'; 11]).unwrap();
        assert!(matches!(read_limited(&path, 10), Err(CoreError::TooLarge { size: 11, .. })));
    }

    #[test]
    fn open_document_round_trips_bytes() {
        let dir = temp_dir("open");
        let path = dir.join("doc.md");
        let source = "---\ntitle: x\n---\n# 标题\r\n";
        fs::write(&path, source).unwrap();
        let doc = open_document(&path).unwrap();
        assert_eq!(doc.file_bytes(), source.as_bytes());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn tree_prunes_sorts_and_skips_hidden() {
        let dir = temp_dir("tree");
        fs::create_dir_all(dir.join("notes/deep")).unwrap();
        fs::create_dir_all(dir.join("empty/sub")).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::write(dir.join("b.md"), "").unwrap();
        fs::write(dir.join("a 10.md"), "").unwrap();
        fs::write(dir.join("a 2.md"), "").unwrap();
        fs::write(dir.join("pic.png"), "").unwrap();
        fs::write(dir.join(".hidden.md"), "").unwrap();
        fs::write(dir.join(".git/x.md"), "").unwrap();
        fs::write(dir.join("empty/sub/x.png"), "").unwrap();
        fs::write(dir.join("notes/deep/c.txt"), "节奏").unwrap();
        let tree = build_tree(&dir);
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["notes", "a 2.md", "a 10.md", "b.md"]);
        assert_eq!(tree[0].children[0].children[0].name, "c.txt");

        let hits = search_folder(&dir, "节奏", false);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matches[0].line, 1);
    }

    #[test]
    fn write_new_never_overwrites() {
        let dir = temp_dir("new");
        let path = dir.join("n.md");
        write_new(&path, b"").unwrap();
        assert!(write_new(&path, b"x").is_err());
    }
}
