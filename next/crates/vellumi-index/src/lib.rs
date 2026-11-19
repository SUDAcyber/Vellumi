//! Full-text index over project folders (tantivy).
//!
//! Tokenization is one token per non-whitespace character, lowercased, with
//! consecutive positions. A query becomes a phrase over its characters, which
//! gives substring semantics for Chinese and English alike (no word
//! segmentation needed) — the index narrows candidates, and every hit is
//! confirmed against the file's real text with the same line matcher the
//! linear search uses, so results equal a full scan's.
//!
//! The index lives in a cache folder and can be deleted at any time; it never
//! writes into the user's folders.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use tantivy::collector::{DocSetCollector, TopDocs};
use tantivy::directory::MmapDirectory;
use tantivy::query::{PhraseQuery, Query, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, INDEXED, STORED, STRING,
};
use tantivy::tokenizer::{TextAnalyzer, Token, TokenStream, Tokenizer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use vellumi_core::tree::{self, FileMatches, TreeNode};

/// Bump when the schema or tokenizer changes; the folder name carries it so
/// an old index is simply ignored and rebuilt.
pub const INDEX_VERSION: u32 = 1;
const TOKENIZER: &str = "vellumi_chars";
const WRITER_HEAP: usize = 50_000_000;
/// Candidate documents fetched from the index before confirmation.
const MAX_CANDIDATES: usize = 5000;

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("search index: {0}")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("search index folder: {0}")]
    Directory(#[from] tantivy::directory::error::OpenDirectoryError),
    #[error("search index folder {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

// ---- Tokenizer -------------------------------------------------------------

#[derive(Clone, Default)]
pub struct CharTokenizer;

pub struct CharTokenStream {
    tokens: Vec<Token>,
    /// Number of tokens handed out so far; the current one is `index - 1`.
    index: usize,
    /// Returned if token() is called before advance() (never by tantivy).
    empty: Token,
}

impl TokenStream for CharTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        self.index.checked_sub(1).and_then(|i| self.tokens.get(i)).unwrap_or(&self.empty)
    }

    fn token_mut(&mut self) -> &mut Token {
        match self.index.checked_sub(1) {
            Some(i) if i < self.tokens.len() => &mut self.tokens[i],
            _ => &mut self.empty,
        }
    }
}

/// The token texts of `text`, in order (also used to build queries).
pub fn char_tokens(text: &str) -> Vec<(usize, usize, String)> {
    text.char_indices()
        .filter(|(_, ch)| !ch.is_whitespace())
        .map(|(offset, ch)| (offset, offset + ch.len_utf8(), ch.to_lowercase().collect()))
        .collect()
}

impl Tokenizer for CharTokenizer {
    type TokenStream<'a> = CharTokenStream;

    fn token_stream<'a>(&'a mut self, text: &'a str) -> CharTokenStream {
        let tokens = char_tokens(text)
            .into_iter()
            .enumerate()
            .map(|(position, (offset_from, offset_to, text))| Token {
                offset_from,
                offset_to,
                position,
                text,
                position_length: 1,
            })
            .collect();
        CharTokenStream {
            tokens,
            index: 0,
            empty: Token::default(),
        }
    }
}

// ---- Index -----------------------------------------------------------------

struct Fields {
    path: Field,
    body: Field,
    mtime: Field,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: Fields,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncStats {
    pub indexed: usize,
    pub removed: usize,
    pub unchanged: usize,
}

fn schema() -> (Schema, Fields) {
    let mut builder = Schema::builder();
    let path = builder.add_text_field("path", STRING | STORED);
    let body_options = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer(TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let body = builder.add_text_field("body", body_options);
    let mtime = builder.add_u64_field("mtime", STORED | INDEXED);
    (builder.build(), Fields { path, body, mtime })
}

fn modified_ns(path: &Path) -> u64 {
    vellumi_fs::fingerprint(path)
        .map(|fp| u64::try_from(fp.modified_ns).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn read_text(path: &Path) -> Option<String> {
    let (bytes, _) = vellumi_fs::read_limited(path, vellumi_core::MAX_FILE_SIZE).ok()?;
    vellumi_core::encoding::decode(&bytes).ok().map(|(text, _)| text)
}

/// Every supported file below `root`, within the tree budgets.
pub fn files_under(root: &Path) -> Vec<String> {
    let node = TreeNode {
        name: String::new(),
        path: root.to_string_lossy().into_owned(),
        is_dir: true,
        children: vellumi_fs::build_tree(root),
    };
    node.files().into_iter().map(|file| file.path.clone()).collect()
}

impl SearchIndex {
    /// Opens the index in `folder`, rebuilding it if it is unreadable.
    pub fn open(folder: &Path) -> Result<Self, IndexError> {
        std::fs::create_dir_all(folder).map_err(|source| IndexError::Io {
            path: folder.to_path_buf(),
            source,
        })?;
        match Self::open_existing(folder) {
            Ok(index) => Ok(index),
            Err(_) => {
                // A corrupt cache is just a cache: start over.
                let _ = std::fs::remove_dir_all(folder);
                std::fs::create_dir_all(folder).map_err(|source| IndexError::Io {
                    path: folder.to_path_buf(),
                    source,
                })?;
                Self::open_existing(folder)
            }
        }
    }

    fn open_existing(folder: &Path) -> Result<Self, IndexError> {
        let (schema, fields) = schema();
        let index = Index::open_or_create(MmapDirectory::open(folder)?, schema)?;
        index
            .tokenizers()
            .register(TOKENIZER, TextAnalyzer::builder(CharTokenizer).build());
        let reader = index.reader_builder().reload_policy(ReloadPolicy::Manual).try_into()?;
        let writer = index.writer(WRITER_HEAP)?;
        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    fn writer(&self) -> std::sync::MutexGuard<'_, IndexWriter> {
        self.writer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// path → mtime of everything currently indexed.
    fn indexed(&self) -> Result<HashMap<String, u64>, IndexError> {
        let searcher = self.reader.searcher();
        let addresses = searcher.search(&tantivy::query::AllQuery, &DocSetCollector)?;
        let mut out = HashMap::with_capacity(addresses.len());
        for address in addresses {
            let doc: TantivyDocument = searcher.doc(address)?;
            let path = doc.get_first(self.fields.path).and_then(|v| v.as_str()).map(str::to_owned);
            let mtime = doc.get_first(self.fields.mtime).and_then(|v| v.as_u64()).unwrap_or(0);
            if let Some(path) = path {
                out.insert(path, mtime);
            }
        }
        Ok(out)
    }

    fn put(&self, writer: &mut IndexWriter, path: &str) -> Result<bool, IndexError> {
        writer.delete_term(Term::from_field_text(self.fields.path, path));
        let Some(text) = read_text(Path::new(path)) else {
            return Ok(false);
        };
        let mut doc = TantivyDocument::default();
        doc.add_text(self.fields.path, path);
        doc.add_text(self.fields.body, &text);
        doc.add_u64(self.fields.mtime, modified_ns(Path::new(path)));
        writer.add_document(doc)?;
        Ok(true)
    }

    fn commit(&self, writer: &mut IndexWriter) -> Result<(), IndexError> {
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Brings the index in line with `roots`: new and modified files are
    /// (re)indexed, files that vanished or left every root are dropped.
    pub fn sync(&self, roots: &[PathBuf]) -> Result<SyncStats, IndexError> {
        let known = self.indexed()?;
        let mut stats = SyncStats::default();
        let mut wanted = HashSet::new();
        let mut writer = self.writer();
        for root in roots {
            for path in files_under(root) {
                let mtime = modified_ns(Path::new(&path));
                if known.get(&path) == Some(&mtime) {
                    stats.unchanged += 1;
                } else if self.put(&mut writer, &path)? {
                    stats.indexed += 1;
                }
                wanted.insert(path);
            }
        }
        for path in known.keys().filter(|path| !wanted.contains(*path)) {
            writer.delete_term(Term::from_field_text(self.fields.path, path));
            stats.removed += 1;
        }
        self.commit(&mut writer)?;
        Ok(stats)
    }

    /// Re-indexes one file after it changed (or drops it if it is gone).
    pub fn update_file(&self, path: &Path) -> Result<(), IndexError> {
        let path = path.to_string_lossy().into_owned();
        let mut writer = self.writer();
        if Path::new(&path).is_file() && vellumi_core::is_supported_file_name(&path) {
            self.put(&mut writer, &path)?;
        } else {
            writer.delete_term(Term::from_field_text(self.fields.path, &path));
        }
        self.commit(&mut writer)
    }

    pub fn document_count(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// Files containing `query` (substring), confirmed against their text,
    /// restricted to paths under `roots`.
    pub fn search(&self, query: &str, case_sensitive: bool, roots: &[PathBuf]) -> Result<Vec<FileMatches>, IndexError> {
        let query = query.trim();
        let tokens = char_tokens(query);
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let terms: Vec<Term> = tokens
            .iter()
            .map(|(_, _, text)| Term::from_field_text(self.fields.body, text))
            .collect();
        let tantivy_query: Box<dyn Query> = if terms.len() == 1 {
            Box::new(TermQuery::new(terms[0].clone(), IndexRecordOption::WithFreqs))
        } else {
            Box::new(PhraseQuery::new(terms))
        };
        let searcher = self.reader.searcher();
        let hits = searcher.search(&tantivy_query, &TopDocs::with_limit(MAX_CANDIDATES).order_by_score())?;
        let mut results = Vec::new();
        for (_, address) in hits {
            let doc: TantivyDocument = searcher.doc(address)?;
            let Some(path) = doc.get_first(self.fields.path).and_then(|v| v.as_str()) else { continue };
            if !roots.is_empty() && !roots.iter().any(|root| Path::new(path).starts_with(root)) {
                continue;
            }
            let Some(text) = read_text(Path::new(path)) else { continue };
            let matches = tree::line_matches(&text, query, case_sensitive, tree::MAX_MATCHES_PER_FILE);
            if matches.is_empty() {
                continue;
            }
            let name = Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            results.push(FileMatches {
                path: path.to_owned(),
                name,
                matches,
            });
            if results.len() >= tree::MAX_SEARCH_FILES {
                break;
            }
        }
        results.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(results)
    }

    /// Exposed for diagnostics.
    pub fn tantivy(&self) -> &Index {
        &self.index
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vellumi-index-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn tokens_are_lowercased_characters_without_whitespace() {
        let tokens: Vec<String> = char_tokens("Ab 节奏").into_iter().map(|t| t.2).collect();
        assert_eq!(tokens, vec!["a", "b", "节", "奏"]);
    }

    #[test]
    fn indexes_searches_and_syncs() {
        let notes = temp("notes");
        let cache = temp("cache");
        fs::create_dir_all(notes.join("sub")).unwrap();
        fs::write(notes.join("a.md"), "一天的节奏\nRhythm matters\n").unwrap();
        fs::write(notes.join("sub/b.md"), "别的内容\n节奏感\n").unwrap();
        fs::write(notes.join("c.md"), "节 奏 分开写\n").unwrap();
        fs::write(notes.join("pic.png"), "节奏").unwrap();

        let index = SearchIndex::open(&cache).unwrap();
        let stats = index.sync(std::slice::from_ref(&notes)).unwrap();
        assert_eq!(stats.indexed, 3);

        let hits = index.search("节奏", false, &[]).unwrap();
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert_eq!(names, vec!["a.md", "b.md"], "c.md matches the phrase index but not the text: confirmed away");
        assert_eq!(hits[1].matches[0].line, 2);

        assert_eq!(index.search("rhythm", false, &[]).unwrap().len(), 1);
        assert_eq!(index.search("rhythm", true, &[]).unwrap().len(), 0, "case-sensitive confirm");
        assert_eq!(index.search("hyth", false, &[]).unwrap().len(), 1, "substring, not word, semantics");

        // Unchanged files are skipped; edits and deletions are picked up.
        assert_eq!(index.sync(std::slice::from_ref(&notes)).unwrap().unchanged, 3);
        fs::write(notes.join("a.md"), "改掉了\n").unwrap();
        index.update_file(&notes.join("a.md")).unwrap();
        fs::remove_file(notes.join("sub/b.md")).unwrap();
        let stats = index.sync(std::slice::from_ref(&notes)).unwrap();
        assert_eq!(stats.removed, 1);
        assert!(index.search("节奏", false, &[]).unwrap().is_empty());
        assert_eq!(index.document_count(), 2);

        // Roots filter.
        assert!(index.search("改掉", false, &[PathBuf::from("/elsewhere")]).unwrap().is_empty());
    }

    /// cargo test -p vellumi-index --release -- --ignored --nocapture
    #[test]
    #[ignore = "benchmark; prints timings for docs/PERF.md"]
    fn benchmark_ten_thousand_notes() {
        use std::time::Instant;
        let notes = temp("bench-notes");
        let cache = temp("bench-cache");
        let paragraph = "一天的节奏比一天的产量更重要。Slow writing is about rhythm, not speed. ".repeat(20);
        for folder in 0..100 {
            let dir = notes.join(format!("f{folder:03}"));
            fs::create_dir_all(&dir).unwrap();
            for file in 0..100 {
                let marker = if folder == 42 && file == 7 { "独一无二的标记" } else { "" };
                fs::write(dir.join(format!("n{file:03}.md")), format!("# 第 {file} 篇\n\n{paragraph}\n{marker}\n")).unwrap();
            }
        }
        // build_tree caps a walk at 2000 nodes, so index per folder like
        // projects of realistic size.
        let roots: Vec<PathBuf> = (0..100).map(|f| notes.join(format!("f{f:03}"))).collect();
        let index = SearchIndex::open(&cache).unwrap();
        let start = Instant::now();
        let stats = index.sync(&roots).unwrap();
        let first = start.elapsed();
        let start = Instant::now();
        index.sync(&roots).unwrap();
        let resync = start.elapsed();
        let mut times = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            let hits = index.search("独一无二的标记", false, &[]).unwrap();
            times.push(start.elapsed());
            assert_eq!(hits.len(), 1);
        }
        times.sort();
        let start = Instant::now();
        let scan: usize = roots.iter().map(|r| vellumi_fs::search_folder(r, "独一无二的标记", false).len()).sum();
        let linear = start.elapsed();
        assert_eq!(scan, 1);
        println!(
            "PERF | files {} | first index {:?} | resync (unchanged) {:?} | rare query p50 {:?} p95 {:?} | linear scan {:?}",
            stats.indexed,
            first,
            resync,
            times[10],
            times[18],
            linear
        );
    }

    #[test]
    fn corrupt_cache_is_rebuilt() {
        let cache = temp("corrupt");
        fs::write(cache.join("meta.json"), "not json").unwrap();
        let index = SearchIndex::open(&cache).unwrap();
        assert_eq!(index.document_count(), 0);
    }
}
