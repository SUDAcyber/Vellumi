//! File-tree and find-in-files types plus the pure line matcher. Port of
//! `MoltenFileTree.swift` (budgets) and `MoltenProjectContentSearch.swift`.
//! The walking itself lives in `vellumi-fs`.

use serde::Serialize;

pub const MAX_DEPTH: usize = 6;
pub const MAX_NODES: usize = 2000;
/// Directory entries examined in one walk, Markdown or not (node_modules…).
pub const MAX_SCANNED_ENTRIES: usize = 20_000;
pub const MAX_SEARCH_FILES: usize = 1000;
pub const MAX_MATCHES_PER_FILE: usize = 50;
const MAX_LINE_PREVIEW: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// Only folders that contain Markdown are kept.
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    /// Every file below this node, depth first.
    pub fn files(&self) -> Vec<&TreeNode> {
        let mut out = Vec::new();
        self.collect_files(&mut out);
        out
    }

    fn collect_files<'a>(&'a self, out: &mut Vec<&'a TreeNode>) {
        if self.is_dir {
            for child in &self.children {
                child.collect_files(out);
            }
        } else {
            out.push(self);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineMatch {
    /// 1-based.
    pub line: usize,
    /// Trimmed line, shortened for display.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatches {
    pub path: String,
    pub name: String,
    pub matches: Vec<LineMatch>,
}

/// Case-folding used by find-in-files: full Unicode lowercase, compared on
/// the folded strings of each line (positions are not needed here).
fn fold(text: &str, case_sensitive: bool) -> String {
    if case_sensitive {
        text.to_owned()
    } else {
        text.to_lowercase()
    }
}

/// First match per line, at most `max` lines. Pure.
pub fn line_matches(content: &str, query: &str, case_sensitive: bool, max: usize) -> Vec<LineMatch> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let needle = fold(query, case_sensitive);
    let mut out = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if fold(line, case_sensitive).contains(&needle) {
            let trimmed = line.trim();
            let text: String = trimmed.chars().take(MAX_LINE_PREVIEW).collect();
            out.push(LineMatch { line: index + 1, text });
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

/// Sort order used everywhere a folder is listed: folders first, then
/// natural (number-aware), case-insensitive name order.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => {
                let take_number = |iter: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(c) = iter.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        iter.next();
                    }
                    digits
                };
                let (ln, rn) = (take_number(&mut left), take_number(&mut right));
                let (lt, rt) = (ln.trim_start_matches('0'), rn.trim_start_matches('0'));
                let order = lt.len().cmp(&rt.len()).then_with(|| lt.cmp(rt));
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
            (Some(l), Some(r)) => {
                let order = l.to_lowercase().cmp(r.to_lowercase());
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
                left.next();
                right.next();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_case_insensitive_and_one_per_line() {
        let content = "Alpha beta\nnothing\n  BETA beta  \n";
        let hits = line_matches(content, "beta", false, 50);
        assert_eq!(
            hits,
            vec![
                LineMatch { line: 1, text: "Alpha beta".into() },
                LineMatch { line: 3, text: "BETA beta".into() },
            ]
        );
        assert_eq!(line_matches(content, "beta", true, 50).len(), 2);
        assert_eq!(line_matches(content, "BETA", true, 50).len(), 1);
    }

    #[test]
    fn empty_query_and_cap() {
        assert!(line_matches("a", "  ", false, 50).is_empty());
        assert_eq!(line_matches(&"x\n".repeat(100), "x", false, 50).len(), 50);
    }

    #[test]
    fn chinese_queries() {
        assert_eq!(line_matches("一天的节奏\n别的", "节奏", false, 50).len(), 1);
    }

    #[test]
    fn natural_sort() {
        let mut names = vec!["note 10.md", "Note 2.md", "note 1.md", "b.md", "A.md"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["A.md", "b.md", "note 1.md", "Note 2.md", "note 10.md"]);
    }
}
