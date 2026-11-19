//! Strict text decoding. Port of `MoltenDocument.decodeText(from:)`, plus the
//! D6 decision: remember the source encoding so saving writes it back
//! unchanged (0.5.x silently converted UTF-16 / UTF-8-BOM files to plain
//! UTF-8 on the first save).

use serde::{Deserialize, Serialize};

use crate::error::EncodingError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceEncoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

impl SourceEncoding {
    /// Short label for the status bar.
    pub fn label(self) -> &'static str {
        match self {
            SourceEncoding::Utf8 => "UTF-8",
            SourceEncoding::Utf8Bom => "UTF-8 BOM",
            SourceEncoding::Utf16Le => "UTF-16 LE",
            SourceEncoding::Utf16Be => "UTF-16 BE",
        }
    }
}

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Decodes file bytes. Order matches 0.5.x exactly:
/// empty → ""; UTF-16 BOM → UTF-16; any NUL → refuse; invalid UTF-8 → refuse;
/// UTF-8 BOM → stripped.
pub fn decode(bytes: &[u8]) -> Result<(String, SourceEncoding), EncodingError> {
    if bytes.is_empty() {
        return Ok((String::new(), SourceEncoding::Utf8));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, u16::from_le_bytes).map(|text| (text, SourceEncoding::Utf16Le));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, u16::from_be_bytes).map(|text| (text, SourceEncoding::Utf16Be));
    }
    if bytes.contains(&0) {
        return Err(EncodingError::ContainsNul);
    }
    let (payload, encoding) = match bytes.strip_prefix(&UTF8_BOM) {
        Some(rest) => (rest, SourceEncoding::Utf8Bom),
        None => (bytes, SourceEncoding::Utf8),
    };
    match std::str::from_utf8(payload) {
        Ok(text) => Ok((text.to_owned(), encoding)),
        Err(error) => Err(EncodingError::InvalidUtf8 {
            valid_up_to: error.valid_up_to() + (bytes.len() - payload.len()),
        }),
    }
}

fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Result<String, EncodingError> {
    let (pairs, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(EncodingError::InvalidUtf16);
    }
    let units = pairs.iter().map(|pair| unit(*pair));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .map_err(|_| EncodingError::InvalidUtf16)
}

/// Encodes text back into the file's original encoding (BOM included).
pub fn encode(text: &str, encoding: SourceEncoding) -> Vec<u8> {
    match encoding {
        SourceEncoding::Utf8 => text.as_bytes().to_vec(),
        SourceEncoding::Utf8Bom => {
            let mut out = Vec::with_capacity(text.len() + 3);
            out.extend_from_slice(&UTF8_BOM);
            out.extend_from_slice(text.as_bytes());
            out
        }
        SourceEncoding::Utf16Le => {
            let mut out = vec![0xFF, 0xFE];
            text.encode_utf16().for_each(|unit| out.extend_from_slice(&unit.to_le_bytes()));
            out
        }
        SourceEncoding::Utf16Be => {
            let mut out = vec![0xFE, 0xFF];
            text.encode_utf16().for_each(|unit| out.extend_from_slice(&unit.to_be_bytes()));
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_round_trips() {
        let source = "# 标题\n\n熔字 **bold** 表情 🔥 and English.\n";
        let (text, encoding) = decode(source.as_bytes()).unwrap();
        assert_eq!(text, source);
        assert_eq!(encoding, SourceEncoding::Utf8);
        assert_eq!(encode(&text, encoding), source.as_bytes());
    }

    #[test]
    fn empty_file_is_empty_text() {
        assert_eq!(decode(b"").unwrap(), (String::new(), SourceEncoding::Utf8));
    }

    #[test]
    fn bom_identified_utf16_opens_and_round_trips() {
        for encoding in [SourceEncoding::Utf16Le, SourceEncoding::Utf16Be] {
            let bytes = encode("# UTF-16 文档\n", encoding);
            let (text, detected) = decode(&bytes).unwrap();
            assert_eq!(text, "# UTF-16 文档\n");
            assert_eq!(detected, encoding);
            assert_eq!(encode(&text, detected), bytes, "D6: original encoding is preserved");
        }
    }

    #[test]
    fn utf8_bom_is_stripped_but_remembered() {
        let mut bytes = UTF8_BOM.to_vec();
        bytes.extend_from_slice(b"# BOM'd\n");
        let (text, encoding) = decode(&bytes).unwrap();
        assert_eq!(text, "# BOM'd\n");
        assert_eq!(encoding, SourceEncoding::Utf8Bom);
        assert_eq!(encode(&text, encoding), bytes);
    }

    #[test]
    fn invalid_utf8_is_refused_not_guessed() {
        let latin1 = [0x63, 0x61, 0x66, 0xE9]; // "café" in Latin-1
        assert_eq!(decode(&latin1), Err(EncodingError::InvalidUtf8 { valid_up_to: 3 }));
    }

    #[test]
    fn nul_bytes_are_refused() {
        let utf16_no_bom = [0x68, 0x00, 0x69, 0x00];
        assert_eq!(decode(&utf16_no_bom), Err(EncodingError::ContainsNul));
    }

    #[test]
    fn odd_length_utf16_is_refused() {
        assert_eq!(decode(&[0xFF, 0xFE, 0x41]), Err(EncodingError::InvalidUtf16));
    }

    #[test]
    fn lone_surrogate_utf16_is_refused() {
        assert_eq!(decode(&[0xFF, 0xFE, 0x00, 0xD8]), Err(EncodingError::InvalidUtf16));
    }
}
