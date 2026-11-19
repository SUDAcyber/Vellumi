//! Image attachments and the asset URL scheme. Port of the naming helpers in
//! `MoltenDocument.swift` and the resolver in `MoltenAssetSchemeHandler.swift`.

use std::path::{Component, Path, PathBuf};

use crate::error::CoreError;

pub const DEFAULT_IMAGE_FOLDER: &str = "assets";

/// Sanitizes the Preferences ▸ image-folder value to a single path component
/// (separators or dots would escape the document folder).
pub fn image_folder_name(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| !matches!(c, '/' | '\\' | ':')).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        DEFAULT_IMAGE_FOLDER.to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// Splits a pasted file name into a safe `(stem, extension)`: path parts
/// dropped, runs of `\ / : * ? " < > | ( ) [ ]` and whitespace collapsed to
/// `-`, defaults `image` / `png`.
pub fn sanitized_image_name(preferred: &str) -> (String, String) {
    let last = preferred.rsplit(['/', '\\']).next().unwrap_or(preferred);
    let mut base = String::with_capacity(last.len());
    let mut in_bad_run = false;
    for ch in last.chars() {
        let bad = ch.is_whitespace()
            || matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '(' | ')' | '[' | ']');
        if bad {
            if !in_bad_run {
                base.push('-');
            }
            in_bad_run = true;
        } else {
            base.push(ch);
            in_bad_run = false;
        }
    }
    let (stem, ext) = match base.rsplit_once('.') {
        Some((stem, ext)) if !ext.is_empty() => (stem.to_owned(), ext.to_owned()),
        _ => (base.clone(), String::new()),
    };
    // A name made only of dots ("..") would otherwise survive as a stem.
    let stem = if stem.trim_matches('.').is_empty() { "image".to_owned() } else { stem };
    let ext = if ext.is_empty() { "png".to_owned() } else { ext };
    (stem, ext)
}

/// First free `stem.ext`, then `stem-1.ext`, `stem-2.ext`… in `directory`.
pub fn unique_asset_path(directory: &Path, preferred: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let (stem, ext) = sanitized_image_name(preferred);
    let mut candidate = directory.join(format!("{stem}.{ext}"));
    let mut counter = 1u32;
    while exists(&candidate) {
        candidate = directory.join(format!("{stem}-{counter}.{ext}"));
        counter += 1;
    }
    candidate
}

/// First free `base.md`, then `base 2.md`, `base 3.md`… (New Note / Drafts).
pub fn unique_markdown_path(directory: &Path, base: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let mut candidate = directory.join(format!("{base}.md"));
    let mut counter = 2u32;
    while exists(&candidate) {
        candidate = directory.join(format!("{base} {counter}.md"));
        counter += 1;
    }
    candidate
}

/// The path written into the Markdown for a saved image: both components
/// percent-encoded with the URL path-allowed set, as the editor's
/// display-time rewrite (`encodeURI`) expects.
pub fn document_relative_asset_path(folder: &str, file: &str) -> String {
    format!("{}/{}", percent_encode_path(folder), percent_encode_path(file))
}

fn percent_encode_path(component: &str) -> String {
    let mut out = String::with_capacity(component.len());
    for byte in component.bytes() {
        let allowed = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.' | b'_' | b'~' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';'
                    | b'=' | b':' | b'@' | b'/'
            );
        if allowed {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let hex = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Maps the percent-encoded path of an asset request to a file inside
/// `root`, refusing anything that would leave it (`../`, absolute paths).
pub fn resolve_asset_request(encoded_relative: &str, root: &Path) -> Result<PathBuf, CoreError> {
    let escapes = || CoreError::PathEscapesRoot {
        requested: encoded_relative.to_owned(),
        root: root.to_path_buf(),
    };
    let decoded = percent_decode(encoded_relative).ok_or_else(escapes)?;
    let trimmed = decoded.trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(escapes());
    }
    let mut resolved = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !resolved.pop() {
                    return Err(escapes());
                }
            }
            Component::RootDir | Component::Prefix(_) => return Err(escapes()),
        }
    }
    if resolved.as_os_str().is_empty() {
        return Err(escapes());
    }
    Ok(root.join(resolved))
}

/// MIME type for the asset scheme response.
pub fn mime_type(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "tif" | "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn folder_name_is_sanitized() {
        assert_eq!(image_folder_name("images"), "images");
        assert_eq!(image_folder_name("a/b"), "ab");
        assert_eq!(image_folder_name(".."), "assets");
        assert_eq!(image_folder_name("  "), "assets");
        assert_eq!(image_folder_name("../"), "assets");
    }

    #[test]
    fn colliding_names_get_suffixes() {
        let dir = Path::new("/doc/assets");
        let taken: HashSet<PathBuf> = [dir.join("pic.png")].into_iter().collect();
        assert_eq!(unique_asset_path(dir, "pic.png", |_| false), dir.join("pic.png"));
        assert_eq!(unique_asset_path(dir, "pic.png", |p| taken.contains(p)), dir.join("pic-1.png"));
    }

    #[test]
    fn hostile_name_is_sanitized() {
        let (stem, ext) = sanitized_image_name("../..//evil name?.png");
        assert_eq!(ext, "png");
        assert!(!stem.contains(' ') && !stem.contains('/') && !stem.contains(".."));
        let path = unique_asset_path(Path::new("/d/assets"), "../..//evil name?.png", |_| false);
        assert!(path.starts_with("/d/assets"));
    }

    #[test]
    fn defaults_for_missing_parts() {
        assert_eq!(sanitized_image_name(""), ("image".to_owned(), "png".to_owned()));
        assert_eq!(sanitized_image_name("shot"), ("shot".to_owned(), "png".to_owned()));
    }

    #[test]
    fn markdown_path_percent_encodes_both_components() {
        assert_eq!(document_relative_asset_path("my images", "a b.png"), "my%20images/a%20b.png");
        assert_eq!(document_relative_asset_path("图片", "x.png"), "%E5%9B%BE%E7%89%87/x.png");
    }

    #[test]
    fn new_note_names_count_from_two() {
        let dir = Path::new("/n");
        let taken: HashSet<PathBuf> = [dir.join("未命名.md"), dir.join("未命名 2.md")].into_iter().collect();
        assert_eq!(unique_markdown_path(dir, "未命名", |p| taken.contains(p)), dir.join("未命名 3.md"));
    }

    #[test]
    fn resolves_relative_and_decodes() {
        let root = Path::new("/Users/someone/Notes");
        assert_eq!(resolve_asset_request("assets/pic.png", root).unwrap(), root.join("assets/pic.png"));
        assert_eq!(resolve_asset_request("my%20images/a%20b.png", root).unwrap(), root.join("my images/a b.png"));
        assert_eq!(resolve_asset_request("a/../b.png", root).unwrap(), root.join("b.png"));
    }

    #[test]
    fn refuses_traversal() {
        let root = Path::new("/Users/someone/Notes");
        for bad in ["../../etc/passwd", "..%2F..%2Fetc", "", "/", "%ZZ"] {
            assert!(resolve_asset_request(bad, root).is_err(), "{bad} must be refused");
        }
    }
}
