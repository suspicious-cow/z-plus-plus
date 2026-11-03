//! Decoding and encoding file bytes. Text is kept as UTF-16 for the edit control.

use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Globalization::{MultiByteToWideChar, WideCharToMultiByte, CP_ACP};
use windows_sys::Win32::Storage::FileSystem::{
    ReplaceFileW, REPLACEFILE_IGNORE_ACL_ERRORS, REPLACEFILE_IGNORE_MERGE_ERRORS,
};

const TEMP_SUFFIX: &str = ".zpp-saving";

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
    /// The file mixed LF, CRLF or CR line breaks; saving uses one style (the most common).
    pub is_mixed_eol: bool,
}

/// Returns the text (no trailing NUL) and the format to write it back with.
/// Refuses input that would not survive a save unchanged, so z++ never silently damages a file.
/// Mixed line endings are the one change a save makes: they are flagged in `is_mixed_eol` so
/// the user is told when the file opens.
pub fn decode(bytes: &[u8]) -> Result<(Vec<u16>, Format), &'static str> {
    let (text, encoding): (Vec<u16>, Encoding) = if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
        let s =
            std::str::from_utf8(rest).map_err(|_| "it has a UTF-8 BOM but is not valid UTF-8")?;
        (s.encode_utf16().collect(), Encoding::Utf8Bom)
    } else if let Some(rest) = bytes.strip_prefix(&UTF16LE_BOM) {
        if rest.len() % 2 != 0 {
            return Err("it has a UTF-16 BOM but an odd number of bytes");
        }
        (
            rest.as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect(),
            Encoding::Utf16Le,
        )
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        (s.encode_utf16().collect(), Encoding::Utf8)
    } else {
        (ansi_to_wide(bytes), Encoding::Ansi)
    };
    if text.contains(&0) {
        return Err("it contains NUL characters (a binary file, or UTF-16 without a BOM)");
    }
    let (crlf, lf, cr) = count_line_breaks(&text);
    let kinds = [crlf, lf, cr].iter().filter(|&&n| n > 0).count();
    Ok((
        text,
        Format {
            encoding,
            is_lf_only: lf > crlf + cr,
            is_mixed_eol: kinds > 1,
        },
    ))
}

/// Counts of CRLF, bare LF and bare CR line breaks.
fn count_line_breaks(text: &[u16]) -> (usize, usize, usize) {
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    let mut i = 0;
    while i < text.len() {
        match (text[i], text.get(i + 1)) {
            (CR, Some(&LF)) => {
                crlf += 1;
                i += 1;
            }
            (CR, _) => cr += 1,
            (LF, _) => lf += 1,
            _ => {}
        }
        i += 1;
    }
    (crlf, lf, cr)
}

/// Encodes text for saving. Returns the bytes and the format actually used:
/// ANSI text with characters the code page cannot hold is saved as UTF-8 instead.
pub fn encode(text: &[u16], format: Format) -> (Vec<u8>, Format) {
    let text = if format.is_lf_only {
        crlf_to_lf(text)
    } else {
        text.to_vec()
    };
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
                let utf8 = Format {
                    encoding: Encoding::Utf8,
                    ..format
                };
                (String::from_utf16_lossy(&text).into_bytes(), utf8)
            }
        },
    }
}

/// Saves without a window where the file is half written: the bytes go to a temp file in the
/// same folder, which is flushed to disk and then swapped in. ReplaceFileW keeps the original
/// file's attributes and permissions. If anything fails, the original file is untouched.
pub fn write_safely(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = temp_path(path);
    let written = (|| {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    let result = written.and_then(|()| {
        if path.exists() {
            replace(path, &temp)
        } else {
            std::fs::rename(&temp, path)
        }
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(TEMP_SUFFIX);
    path.with_file_name(name)
}

fn replace(target: &Path, replacement: &Path) -> io::Result<()> {
    let wide = |p: &Path| p.as_os_str().encode_wide().chain([0]).collect::<Vec<u16>>();
    let (target, replacement) = (wide(target), wide(replacement));
    let flags = REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_IGNORE_ACL_ERRORS;
    let ok = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            null(),
            flags,
            null(),
            null(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// What a file looked like on disk when z++ last read or wrote it, to notice outside changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stamp {
    modified: std::time::SystemTime,
    len: u64,
}

/// None when the file does not exist (or cannot be read).
pub fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp {
        modified: meta.modified().ok()?,
        len: meta.len(),
    })
}

pub fn line_ending_label(format: Format) -> &'static str {
    if format.is_lf_only {
        "LF"
    } else {
        "CRLF"
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
        let n = WideCharToMultiByte(
            CP_ACP,
            0,
            text.as_ptr(),
            len,
            null_mut(),
            0,
            null(),
            null_mut(),
        );
        if n <= 0 {
            return None;
        }
        let mut out = vec![0u8; n as usize];
        let mut used_default = 0;
        let written = WideCharToMultiByte(
            CP_ACP,
            0,
            text.as_ptr(),
            len,
            out.as_mut_ptr(),
            n,
            null(),
            &mut used_default,
        );
        (written == n && used_default == 0).then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("zpp-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_safely_creates_and_replaces() {
        let dir = scratch_dir("save");
        let path = dir.join("note.txt");
        write_safely(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");
        write_safely(&path, b"second, longer").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second, longer");
        assert!(!temp_path(&path).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_safely_leaves_read_only_file_alone() {
        let dir = scratch_dir("readonly");
        let path = dir.join("locked.txt");
        std::fs::write(&path, b"keep me").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms.clone()).unwrap();
        assert!(write_safely(&path, b"overwrite").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
        assert!(!temp_path(&path).exists());
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn refuses_nul_and_bad_boms() {
        assert!(decode(b"h\0i\0").is_err());
        assert!(decode(b"\xEF\xBB\xBF\xFF").is_err());
        assert!(decode(&[0xFF, 0xFE, b'h']).is_err());
    }

    #[test]
    fn line_endings_follow_the_majority_and_flag_mixes() {
        let (_, fmt) = decode(b"a\nb\nc\r\n").unwrap();
        assert!(fmt.is_lf_only && fmt.is_mixed_eol);
        let (_, fmt) = decode(b"a\r\nb\r\nc\n").unwrap();
        assert!(!fmt.is_lf_only && fmt.is_mixed_eol);
        let (_, fmt) = decode(b"a\rb\r").unwrap();
        assert!(!fmt.is_lf_only && !fmt.is_mixed_eol);
        let (_, fmt) = decode(b"a\r\nb\r\n").unwrap();
        assert!(!fmt.is_mixed_eol);
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
