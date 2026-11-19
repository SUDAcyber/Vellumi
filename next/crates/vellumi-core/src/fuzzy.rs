//! Quick Open subsequence matcher. Port of `MoltenFuzzyMatch.swift`; the
//! scoring constants are unchanged so results rank the same as 0.5.x.

/// `None` = no match; higher scores sort first.
pub fn score(query: &str, candidate: &str) -> Option<i64> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let c: Vec<char> = candidate.to_lowercase().chars().collect();
    if q.len() > c.len() {
        return None;
    }

    let mut score: i64 = 0;
    let mut qi = 0;
    let mut previous_matched = false;
    for (index, &ch) in c.iter().enumerate() {
        if qi >= q.len() || ch != q[qi] {
            previous_matched = false;
            continue;
        }
        qi += 1;
        score += 1;
        if previous_matched {
            score += 4; // consecutive run
        }
        if index == 0 {
            score += 6; // very first character
        } else if matches!(c[index - 1], ' ' | '-' | '_' | '.' | '/') {
            score += 5; // word boundary
        }
        previous_matched = true;
    }
    if qi != q.len() {
        return None;
    }
    let length = i64::try_from(c.len()).unwrap_or(i64::MAX);
    // Prefer tighter candidates: same hits in a shorter name rank higher.
    Some(score * 100 - length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_hits_beat_scattered_hits() {
        let boundary = score("rn", "release-notes.md").unwrap();
        let scattered = score("rn", "warning.md").unwrap();
        assert!(boundary > scattered);
    }

    #[test]
    fn non_match_is_none() {
        assert_eq!(score("xyz", "notes.md"), None);
        assert_eq!(score("toolong", "no.md"), None);
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("", "anything.md"), Some(0));
    }

    #[test]
    fn chinese_filenames() {
        assert!(score("第一", "第一章草稿.md").is_some());
        assert_eq!(score("第二", "第一章草稿.md"), None);
    }

    #[test]
    fn case_insensitive() {
        assert!(score("README", "readme.md").is_some());
    }
}
