//! Export helpers (Swift: `MoltenExporter.swift`, `MoltenImageCardTypes.swift`).
//! Pure string/byte functions: the shell gathers the rendered HTML and CSS,
//! these assemble files.

use std::sync::LazyLock;

use regex::Regex;

// Compiled once; the patterns are constants, covered by the tests below.
#[allow(clippy::expect_used)]
static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<h([1-6])([^>]*)>(.*?)</h([1-6])>").expect("valid heading pattern"));
#[allow(clippy::expect_used)]
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("valid tag pattern"));
#[allow(clippy::expect_used)]
static NON_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}]+").expect("valid slug pattern"));
#[allow(clippy::expect_used)]
static TOC_MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<p[^>]*>\s*\[toc\]\s*</p>").expect("valid toc pattern"));
#[allow(clippy::expect_used)]
static EXISTING_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"\sid="[^"]*""#).expect("valid id pattern"));

/// Typora-compatible `[toc]`: every h1–h6 gets a unique slug id, and each
/// paragraph whose whole text is `[toc]` becomes a nested list of links.
pub fn expand_toc(body_html: &str) -> String {
    let mut used = std::collections::HashSet::new();
    let mut headings: Vec<(usize, String, String)> = Vec::new();
    let mut rebuilt = String::with_capacity(body_html.len() + 256);
    let mut cursor = 0;
    for captures in HEADING.captures_iter(body_html) {
        let (Some(whole), Some(open), Some(attrs), Some(inner), Some(close)) =
            (captures.get(0), captures.get(1), captures.get(2), captures.get(3), captures.get(4))
        else {
            continue;
        };
        if open.as_str() != close.as_str() {
            continue;
        }
        let level: usize = open.as_str().parse().unwrap_or(1);
        let text = TAG.replace_all(inner.as_str(), "").into_owned();
        let mut slug = NON_WORD
            .replace_all(&text.to_lowercase(), "-")
            .trim_matches('-')
            .to_owned();
        if slug.is_empty() {
            "heading".clone_into(&mut slug);
        }
        let mut unique = slug.clone();
        let mut counter = 2;
        while !used.insert(unique.clone()) {
            unique = format!("{slug}-{counter}");
            counter += 1;
        }
        rebuilt.push_str(&body_html[cursor..whole.start()]);
        let attrs = EXISTING_ID.replace_all(attrs.as_str(), "");
        rebuilt.push_str(&format!("<h{level} id=\"{unique}\"{attrs}>{}</h{level}>", inner.as_str()));
        cursor = whole.end();
        headings.push((level, text, unique));
    }
    rebuilt.push_str(&body_html[cursor..]);

    if headings.is_empty() {
        return rebuilt;
    }
    let min_level = headings.iter().map(|(level, ..)| *level).min().unwrap_or(1);
    let items: String = headings
        .iter()
        .map(|(level, text, slug)| {
            // The text came from already-valid HTML: entities are intact
            // ("Q&amp;A"); only a stray "<" from tag stripping is escaped.
            let link = text.replace('<', "&lt;");
            format!(
                "<li style=\"margin-left: {}em\"><a href=\"#{slug}\">{link}</a></li>",
                level - min_level
            )
        })
        .collect();
    let nav = format!("<nav class=\"vellumi-toc\"><ul style=\"list-style: none; padding-left: 0\">{items}</ul></nav>");
    TOC_MARKER.replace_all(&rebuilt, regex::NoExpand(&nav)).into_owned()
}

/// Removes `[toc]` marker paragraphs (image cards have no use for a table
/// of contents; HTML/PDF expand it instead).
pub fn strip_toc_marker(body_html: &str) -> String {
    TOC_MARKER.replace_all(body_html, "").into_owned()
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A standalone HTML page around the editor's rendered body. `css` is every
/// stylesheet the page needs, already inlined (fonts as data: URIs).
pub fn html_page(title: &str, css: &str, body_html: &str, lang: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"generator\" content=\"Vellumi\">\n<title>{}</title>\n<style>\n{css}\n\
         html {{ background: #fff; color-scheme: light; }}\n\
         body {{ margin: 0 auto; max-width: 760px; padding: 48px 32px; background: #fff; }}\n\
         .vellumi-toc a {{ text-decoration: none; }}\n\
         @media print {{ body {{ padding: 0; max-width: none; }} }}\n</style>\n</head>\n\
         <body class=\"milkdown\">\n<div class=\"ProseMirror editor\">{body_html}</div>\n</body>\n</html>\n",
        escape_html(title),
        lang = escape_html(lang)
    )
}

// ---- Image cards -------------------------------------------------------

/// File-name-safe base name for exported cards (fallback "Vellumi-Card").
pub fn card_base_name(name: &str) -> String {
    let trimmed = name.trim();
    let fallback = if trimmed.is_empty() { "Vellumi-Card" } else { trimmed };
    let replaced: String = fallback
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | '?' | '%' | '*' | '|' | '"' | '<' | '>' | ':') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let mut collapsed = String::with_capacity(replaced.len());
    let mut in_run = false;
    for c in replaced.chars() {
        if c.is_whitespace() || c == '-' {
            if !in_run {
                collapsed.push('-');
            }
            in_run = true;
        } else {
            collapsed.push(c);
            in_run = false;
        }
    }
    let cleaned = collapsed.trim_matches(|c| c == '.' || c == '-' || c == ' ').to_owned();
    if cleaned.is_empty() {
        "Vellumi-Card".to_owned()
    } else {
        cleaned
    }
}

/// `name-01.png`, zero-padded to at least two digits.
pub fn card_page_name(base: &str, index: usize, count: usize, extension: &str) -> String {
    let width = count.max(1).to_string().len().max(2);
    format!("{}-{:0width$}.{extension}", card_base_name(base), index + 1)
}

// ---- ZIP (stored, UTF-8 names) -------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    static TABLE: LazyLock<[u32; 256]> = LazyLock::new(|| {
        let mut table = [0u32; 256];
        for (n, slot) in table.iter_mut().enumerate() {
            let mut c = u32::try_from(n).unwrap_or(0);
            for _ in 0..8 {
                c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
            *slot = c;
        }
        table
    });
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc = TABLE[usize::from((crc as u8) ^ byte)] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ZipError {
    #[error("archive too large for the stored ZIP format")]
    TooLarge,
}

/// A stored (uncompressed) ZIP — PNG/JPEG are already compressed. File names
/// are UTF-8 and flagged as such (general-purpose bit 11), so Chinese names
/// unpack correctly on Windows too (audit B2).
pub fn zip_stored(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, ZipError> {
    const UTF8: u16 = 0x0800;
    let u16_of = |n: usize| u16::try_from(n).map_err(|_| ZipError::TooLarge);
    let u32_of = |n: usize| u32::try_from(n).map_err(|_| ZipError::TooLarge);
    let mut archive = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let name = name.as_bytes();
        let crc = crc32(data);
        let size = u32_of(data.len())?;
        let offset = u32_of(archive.len())?;
        let name_len = u16_of(name.len())?;
        archive.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for value in [20u16, UTF8, 0, 0, 0] {
            archive.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            archive.extend_from_slice(&value.to_le_bytes());
        }
        archive.extend_from_slice(&name_len.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(name);
        archive.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for value in [20u16, 20, UTF8, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, size, size] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name_len, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let central_offset = u32_of(archive.len())?;
    let central_size = u32_of(central.len())?;
    let count = u16_of(files.len())?;
    archive.extend_from_slice(&central);
    archive.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    for value in [0u16, 0, count, count] {
        archive.extend_from_slice(&value.to_le_bytes());
    }
    archive.extend_from_slice(&central_size.to_le_bytes());
    archive.extend_from_slice(&central_offset.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes());
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toc_paragraph_expands_to_nav_with_anchors() {
        let html = "<p>[toc]</p><h1>Intro</h1><h2>Details &amp; More</h2>";
        let out = expand_toc(html);
        assert!(out.contains("<nav class=\"vellumi-toc\">"));
        assert!(out.contains("<h1 id=\"intro\">Intro</h1>"));
        assert!(out.contains("<a href=\"#intro\">Intro</a>"));
        assert!(out.contains("margin-left: 1em\"><a href=\"#details-amp-more\">Details &amp; More</a>"), "{out}");
        assert!(!out.contains("[toc]"));
    }

    #[test]
    fn duplicate_headings_get_unique_slugs() {
        let out = expand_toc("<h2>Same</h2><h2>Same</h2><h2>中文 标题</h2>");
        assert!(out.contains("id=\"same\""));
        assert!(out.contains("id=\"same-2\""));
        assert!(out.contains("id=\"中文-标题\""));
    }

    #[test]
    fn no_marker_leaves_body_untouched_except_ids() {
        let out = expand_toc("<p>Body</p><h3>Title</h3>");
        assert_eq!(out, "<p>Body</p><h3 id=\"title\">Title</h3>");
    }

    #[test]
    fn toc_text_with_dollar_signs_is_literal() {
        let out = expand_toc("<p>[TOC]</p><h1>Cost $5 \\ more</h1>");
        assert!(out.contains(">Cost $5 \\ more</a>"), "{out}");
    }

    #[test]
    fn toc_marker_is_stripped_for_cards() {
        assert_eq!(strip_toc_marker("<h1>A</h1><p> [TOC] </p><p>b</p>"), "<h1>A</h1><p>b</p>");
    }

    #[test]
    fn page_escapes_title_and_inlines_css() {
        let page = html_page("A <b> & \"c\"", ".x{}", "<p>hi</p>", "zh-Hans");
        assert!(page.contains("<title>A &lt;b&gt; &amp; &quot;c&quot;</title>"));
        assert!(page.contains(".x{}"));
        assert!(page.contains("<html lang=\"zh-Hans\">"));
        assert!(page.contains("<p>hi</p>"));
    }

    #[test]
    fn card_names() {
        assert_eq!(card_base_name("  慢写作 / 笔记?.md "), "慢写作-笔记-.md");
        assert_eq!(card_base_name("   "), "Vellumi-Card");
        assert_eq!(card_base_name("..."), "Vellumi-Card");
        assert_eq!(card_page_name("笔记", 0, 5, "png"), "笔记-01.png");
        assert_eq!(card_page_name("笔记", 99, 120, "jpg"), "笔记-100.jpg");
    }

    #[test]
    fn zip_is_well_formed_with_utf8_names() {
        let zip = zip_stored(&[("卡片-01.png".to_owned(), b"abc".to_vec())]).unwrap();
        assert_eq!(&zip[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        assert_eq!(u16::from_le_bytes([zip[6], zip[7]]), 0x0800, "UTF-8 flag");
        assert_eq!(crc32(b"abc"), 0x3524_41C2);
        let end = zip.len() - 22;
        assert_eq!(&zip[end..end + 4], &[0x50, 0x4b, 0x05, 0x06]);
    }
}
