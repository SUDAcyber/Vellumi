//! YAML front-matter protection. Port of `MoltenFrontMatter.swift` and the
//! validation in `MoltenFrontMatterSheet.swift`.
//!
//! The editor has no front-matter node, so the block is split off before the
//! editor sees the text and spliced back on save. The split is byte-faithful:
//! `join(split(x)) == x` for every input.

use crate::error::FrontMatterError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    pub front_matter: String,
    pub body: String,
}

/// Splits leading front matter from the body. The opening `---` must be the
/// very first line; the block ends at the next `---` / `...` line (CRLF
/// tolerated). An unterminated fence is body.
pub fn split(text: &str) -> Split {
    let not_front_matter = || Split {
        front_matter: String::new(),
        body: text.to_owned(),
    };
    if !(text.starts_with("---\n") || text.starts_with("---\r\n")) {
        return not_front_matter();
    }
    // Split on \n only: a CR stays attached to its line and survives the join.
    let lines: Vec<&str> = text.split('\n').collect();
    let close = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| matches!(line.trim(), "---" | "..."))
        .map(|(index, _)| index);
    let Some(close) = close else {
        return not_front_matter();
    };
    let has_trailing = close + 1 < lines.len();
    let mut front_matter = lines[..=close].join("\n");
    if has_trailing {
        front_matter.push('\n');
    }
    let body = if has_trailing {
        lines[close + 1..].join("\n")
    } else {
        String::new()
    };
    Split { front_matter, body }
}

/// Pure concatenation: `split` never removes anything between the parts.
pub fn join(front_matter: &str, body: &str) -> String {
    let mut out = String::with_capacity(front_matter.len() + body.len());
    out.push_str(front_matter);
    out.push_str(body);
    out
}

/// Validates a block typed into the front-matter editor. Empty (after
/// trimming) means "remove the block". Otherwise it must be exactly one fenced
/// block with nothing after the closing fence; one trailing newline is
/// ensured so the body splice stays clean.
pub fn validate_edited(text: &str) -> Result<String, FrontMatterError> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    let mut candidate = text.to_owned();
    if !candidate.ends_with('\n') {
        candidate.push('\n');
    }
    let parts = split(&candidate);
    if parts.front_matter.is_empty() {
        return Err(FrontMatterError::NotFenced);
    }
    if !parts.body.trim().is_empty() {
        return Err(FrontMatterError::TrailingContent);
    }
    Ok(join(&parts.front_matter, &parts.body))
}

/// Template shown when the user opens the editor on a file without a block.
pub const NEW_FRONT_MATTER_TEMPLATE: &str = "---\ntitle: \n---\n";

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_round_trip(source: &str) {
        let parts = split(source);
        assert_eq!(join(&parts.front_matter, &parts.body), source, "split/join must be byte-faithful");
    }

    #[test]
    fn separates_front_matter_and_body() {
        let source = "---\ntitle: 测试\ntags: [a, b]\n---\n\n# 正文\n\n段落。\n";
        let parts = split(source);
        assert_eq!(parts.front_matter, "---\ntitle: 测试\ntags: [a, b]\n---\n");
        assert_eq!(parts.body, "\n# 正文\n\n段落。\n");
        assert_round_trip(source);
    }

    #[test]
    fn crlf_front_matter_is_recognized_and_preserved() {
        let source = "---\r\ntitle: windows\r\n---\r\n\r\n正文\r\n";
        let parts = split(source);
        assert_eq!(parts.front_matter, "---\r\ntitle: windows\r\n---\r\n");
        assert!(parts.body.contains("正文"));
        assert_round_trip(source);
    }

    #[test]
    fn no_blank_line_after_fence() {
        let source = "---\ntitle: x\n---\nBody";
        let parts = split(source);
        assert_eq!(parts.front_matter, "---\ntitle: x\n---\n");
        assert_eq!(parts.body, "Body");
        assert_round_trip(source);
    }

    #[test]
    fn fence_at_eof_without_trailing_newline() {
        let source = "---\ntitle: x\n---";
        let parts = split(source);
        assert_eq!(parts.front_matter, "---\ntitle: x\n---");
        assert_eq!(parts.body, "");
        assert_round_trip(source);
    }

    #[test]
    fn no_front_matter_passes_through() {
        let source = "# 普通文档\n\n--- 不是开头的围栏\n";
        let parts = split(source);
        assert_eq!(parts.front_matter, "");
        assert_eq!(parts.body, source);
        assert_round_trip(source);
    }

    #[test]
    fn unterminated_fence_is_body() {
        let source = "---\n没有闭合围栏的正文\n继续\n";
        let parts = split(source);
        assert_eq!(parts.front_matter, "");
        assert_eq!(parts.body, source);
        assert_round_trip(source);
    }

    #[test]
    fn dot_dot_dot_closer_accepted() {
        let source = "---\nkey: value\n...\n正文\n";
        let parts = split(source);
        assert_eq!(parts.front_matter, "---\nkey: value\n...\n");
        assert_eq!(parts.body, "正文\n");
        assert_round_trip(source);
    }

    #[test]
    fn bare_fence_alone_is_body() {
        assert_round_trip("---");
        assert_eq!(split("---").front_matter, "");
    }

    #[test]
    fn round_trip_holds_for_awkward_inputs() {
        for source in ["", "\n", "---\n---\n", "---\n\n---", "---\r\n...\r\n", "---\nx\n---\n---\ny"] {
            assert_round_trip(source);
        }
    }

    #[test]
    fn edited_block_validation() {
        assert_eq!(validate_edited("  \n "), Ok(String::new()));
        assert_eq!(validate_edited("---\ntitle: a\n---"), Ok("---\ntitle: a\n---\n".to_owned()));
        assert_eq!(validate_edited("title: a"), Err(FrontMatterError::NotFenced));
        assert_eq!(validate_edited("---\na: 1\n---\nbody"), Err(FrontMatterError::TrailingContent));
    }
}
