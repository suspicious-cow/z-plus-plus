//! Decoding and encoding file bytes. Text is kept as UTF-16 for the edit control.

use std::ptr::{null, null_mut};
use windows_sys::Win32::Globalization::{MultiByteToWideChar, WideCharToMultiByte, CP_ACP};

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: [u8; 2] = [0xFF, 0xFE];
const CR: u16 = b'\r' as u16;
const LF: u16 = b'\n' as u16;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf8Bom,
    Utf16Le,
    Ansi,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Format {
    pub encoding: Encoding,
    pub is_lf_only: bool,
}

/// Returns the text (no trailing NUL) and the format to write it back with.
/// Refuses input that would not survive a save unchanged, so z++ never silently damages a file.
pub fn decode(bytes: &[u8]) -> Result<(Vec<u16>, Format), &'static str> {
    let (text, encoding): (Vec<u16>, Encoding) = if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
        let s = std::str::from_utf8(rest).map_err(|_| "it has a UTF-8 BOM but is not valid UTF-8")?;
        (s.encode_utf16().collect(), Encoding::Utf8Bom)
    } else if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
        if rest.len() % 2 != 0 {
            return Err("it has a UTF-16 BOM but an odd number of bytes");
        }
        (rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(), Encoding::Utf16Le)
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        (s.encode_utf16().collect(), Encoding::Utf8)
    } else {
        (ansi_to_wide(bytes), Encoding::Ansi)
    };
    if text.contains(&0) {
        return Err("it contains NUL characters (a binary file, or UTF-16 without a BOM)");
    }
    let is_lf_only = text.contains(&LF) && !text.windows(2).any(|w| w == [CR, LF]);
    Ok((text, Format { encoding, is_lf_only }))
}

/// Encodes text for saving. Returns the bytes and the format actually used:
/// ANSI text with characters the code page cannot hold is saved as UTF-8 instead.
pub fn encode(text: &[u16], format: Format) -> (Vec<u8>, Format) {
    let text = if format.is_lf_only { crlf_to_lf(text) } else { text.to_vec() };
    match format.encoding {
        Encoding::Utf8 => (String::from_utf16_lossy(&text).into_bytes(), format),
        Encoding::Utf8Bom => {
            let mut out = UTF8_BOM.to_vec();
            out.extend_from_slice(String::from_utf16_lossy(&text).as_bytes());
            (out, format)
        }
        Encoding::Utf16Le => {
            let mut out = UTF16LE_BOM.to_vec();
            out.extend(text.iter().flat_map(|u| u.to_le_bytes()));
            (out, format)
        }
        Encoding::Ansi => match wide_to_ansi(&text) {
            Some(bytes) => (bytes, format),
            None => {
                let utf8 = Format { encoding: Encoding::Utf8, ..format };
                (String::from_utf16_lossy(&text).into_bytes(), utf8)
            }
        },
    }
}

pub fn encoding_label(encoding: Encoding) -> &'static str {
    match encoding {
        Encoding::Utf8 => "UTF-8",
        Encoding::Utf8Bom => "UTF-8 BOM",
        Encoding::Utf16Le => "UTF-16 LE",
        Encoding::Ansi => "ANSI",
    }
}

fn crlf_to_lf(text: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut iter = text.iter().peekable();
    while let Some(&u) = iter.next() {
        if u == CR && iter.peek() == Some(&&LF) {
            continue;
        }
        out.push(u);
    }
    out
}

fn ansi_to_wide(bytes: &[u8]) -> Vec<u16> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let len = bytes.len() as i32;
    unsafe {
        let n = MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, null_mut(), 0);
        let mut out = vec![0u16; n.max(0) as usize];
        MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, out.as_mut_ptr(), n);
        out
    }
}

/// None when some character has no mapping in the ANSI code page.
fn wide_to_ansi(text: &[u16]) -> Option<Vec<u8>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    let len = text.len() as i32;
    unsafe {
        let n = WideCharToMultiByte(CP_ACP, 0, text.as_ptr(), len, null_mut(), 0, null(), null_mut());
        if n <= 0 {
            return None;
        }
        let mut out = vec![0u8; n as usize];
        let mut used_default = 0;
        let written = WideCharToMultiByte(CP_ACP, 0, text.as_ptr(), len, out.as_mut_ptr(), n, null(), &mut used_default);
        (written == n && used_default == 0).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn refuses_nul_and_bad_boms() {
        assert!(decode(b"h\0i\0").is_err());
        assert!(decode(b"\xEF\xBB\xBF\xFF").is_err());
        assert!(decode(&[0xFF, 0xFE, b'h']).is_err());
    }

    #[test]
    fn utf8_lf_round_trip() {
        let (text, fmt) = decode(b"a\nb\n").unwrap();
        assert_eq!(fmt.encoding, Encoding::Utf8);
        assert!(fmt.is_lf_only);
        let (bytes, _) = encode(&w("a\r\nb\r\n"), fmt);
        assert_eq!(bytes, b"a\nb\n");
        assert_eq!(text, w("a\nb\n"));
    }

    #[test]
    fn utf8_bom_crlf_round_trip() {
        let input = b"\xEF\xBB\xBFx\r\ny";
        let (text, fmt) = decode(input).unwrap();
        assert_eq!(fmt.encoding, Encoding::Utf8Bom);
        assert!(!fmt.is_lf_only);
        assert_eq!(encode(&text, fmt).0, input);
    }

    #[test]
    fn utf16_round_trip() {
        let input = [0xFF, 0xFE, b'h', 0, b'i', 0];
        let (text, fmt) = decode(&input).unwrap();
        assert_eq!(text, w("hi"));
        assert_eq!(encode(&text, fmt).0, input);
    }

    #[test]
    fn invalid_utf8_is_ansi() {
        let (_, fmt) = decode(b"caf\xE9").unwrap();
        assert_eq!(fmt.encoding, Encoding::Ansi);
    }
}
