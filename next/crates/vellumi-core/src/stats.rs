//! Status-bar statistics. Port of `MoltenWordCount.swift` and the character /
//! paragraph loop in `MoltenWorkspaceViewController.noteContentChanged`.
//!
//! Counted over the Markdown source (not rendered text), exactly as 0.5.x:
//! link URLs and code count too. Two deliberate fixes (D10):
//! - any Unicode whitespace (e.g. U+3000 ideographic space used for Chinese
//!   paragraph indents) is no longer counted as a character (audit B20);
//! - `\r` no longer breaks blank-line detection, so CRLF files count
//!   paragraphs correctly.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub words: usize,
    pub characters: usize,
    pub paragraphs: usize,
    /// 0 when there are no words; otherwise ⌈words / 400⌉, at least 1.
    pub reading_minutes: usize,
}

const READING_WORDS_PER_MINUTE: usize = 400;

/// Mixed CJK/Latin word count: each CJK character is one word; each run of
/// Latin letters/digits/underscore is one word.
pub fn word_count(text: &str) -> usize {
    let mut cjk = 0;
    let mut latin_words = 0;
    let mut in_latin_word = false;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
            in_latin_word = false;
        } else if is_word_char(ch) {
            if !in_latin_word {
                latin_words += 1;
                in_latin_word = true;
            }
        } else {
            in_latin_word = false;
        }
    }
    cjk + latin_words
}

pub fn stats(text: &str) -> Stats {
    let words = word_count(text);
    let mut characters = 0;
    let mut paragraphs = 0;
    let mut newline_run = 0;
    let mut paragraph_has_content = false;
    for ch in text.chars() {
        match ch {
            '\n' => {
                newline_run += 1;
                if newline_run >= 2 && paragraph_has_content {
                    paragraphs += 1;
                    paragraph_has_content = false;
                }
            }
            '\r' => {}
            c if c.is_whitespace() => newline_run = 0,
            _ => {
                newline_run = 0;
                characters += 1;
                paragraph_has_content = true;
            }
        }
    }
    if paragraph_has_content {
        paragraphs += 1;
    }
    let reading_minutes = if words == 0 {
        0
    } else {
        words.div_ceil(READING_WORDS_PER_MINUTE).max(1)
    };
    Stats {
        words,
        characters,
        paragraphs,
        reading_minutes,
    }
}

/// Goal completion in whole percent (rounded), `None` when no goal is set.
pub fn goal_percent(words: usize, goal: usize) -> Option<usize> {
    if goal == 0 {
        return None;
    }
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = ((words as f64 / goal as f64) * 100.0).round() as usize;
    Some(percent)
}

fn is_cjk(ch: char) -> bool {
    matches!(
        u32::from(ch),
        0x3400..=0x9FFF       // CJK unified + extension A
            | 0xF900..=0xFAFF // compatibility ideographs
            | 0x3040..=0x30FF // hiragana + katakana
            | 0xAC00..=0xD7A3 // hangul syllables
            | 0x20000..=0x2FA1F // extensions B+
    )
}

fn is_word_char(ch: char) -> bool {
    matches!(u32::from(ch), 0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A | 0x5F | 0xC0..=0x24F)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_chinese_english() {
        assert_eq!(word_count("熔字编辑器好 hello world"), 8);
    }

    #[test]
    fn pure_english_counts_words() {
        assert_eq!(word_count("one two  three\nfour-five"), 5);
    }

    #[test]
    fn pure_cjk_counts_characters() {
        assert_eq!(word_count("这是五个字"), 5);
    }

    #[test]
    fn punctuation_and_markup_ignored() {
        assert_eq!(word_count("# *bold*!"), 1);
        assert_eq!(word_count(""), 0);
        assert_eq!(word_count("——…!?。"), 0);
    }

    #[test]
    fn accented_latin_stays_one_word() {
        assert_eq!(word_count("café naïve"), 2);
    }

    #[test]
    fn characters_skip_all_whitespace_including_ideographic_space() {
        assert_eq!(stats("\u{3000}\u{3000}正文").characters, 2);
        assert_eq!(stats("a b\tc\u{A0}d").characters, 4);
    }

    #[test]
    fn paragraphs_split_on_blank_lines_lf_and_crlf() {
        assert_eq!(stats("a\n\nb").paragraphs, 2);
        assert_eq!(stats("a\r\n\r\nb").paragraphs, 2);
        assert_eq!(stats("a\nb").paragraphs, 1);
        assert_eq!(stats("").paragraphs, 0);
        assert_eq!(stats("\n\n\n").paragraphs, 0);
    }

    #[test]
    fn reading_time_rounds_up_with_floor_of_one() {
        assert_eq!(stats("").reading_minutes, 0);
        assert_eq!(stats("一").reading_minutes, 1);
        assert_eq!(stats(&"字".repeat(400)).reading_minutes, 1);
        assert_eq!(stats(&"字".repeat(401)).reading_minutes, 2);
    }

    #[test]
    fn goal_percentage() {
        assert_eq!(goal_percent(500, 0), None);
        assert_eq!(goal_percent(1284, 2000), Some(64));
        assert_eq!(goal_percent(3000, 2000), Some(150));
    }
}
