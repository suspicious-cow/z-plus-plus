//! What z++ remembers between runs, like Notepad++: every open tab (with caret, scroll and
//! view), and the text of tabs with unsaved edits, kept in backup files so closing z++ never
//! loses them. The tab list is saved whenever it changes, so a crash loses at most a few seconds.

use std::path::{Path, PathBuf};

/// Prefix of every tab record, so a later format can tell old records apart.
const VERSION: &str = "v1";
const SEP: char = '\t';
const BACKUP_DIR: &str = "backup";
const BACKUP_EXT: &str = "txt";

/// One tab as saved in the session.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Tab {
    /// None for an Untitled tab.
    pub path: Option<PathBuf>,
    /// File name in the backup folder holding the tab's unsaved text.
    pub backup: Option<String>,
    /// Caret position in bytes.
    pub caret: usize,
    pub first_line: usize,
    pub is_rendered: bool,
}

/// `v1<TAB>path<TAB>backup<TAB>caret<TAB>first line<TAB>rendered`; Windows paths can't hold tabs.
pub fn encode(tab: &Tab) -> String {
    let path = tab
        .path
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let fields = [
        VERSION.to_string(),
        path,
        tab.backup.clone().unwrap_or_default(),
        tab.caret.to_string(),
        tab.first_line.to_string(),
        (tab.is_rendered as u8).to_string(),
    ];
    fields.join(&SEP.to_string())
}

pub fn decode(record: &str) -> Option<Tab> {
    let mut fields = record.split(SEP);
    if fields.next()? != VERSION {
        return None;
    }
    let path = fields.next()?;
    let backup = fields.next()?;
    let tab = Tab {
        path: (!path.is_empty()).then(|| PathBuf::from(path)),
        backup: (!backup.is_empty() && is_backup_name(backup)).then(|| backup.to_string()),
        caret: fields.next()?.parse().ok()?,
        first_line: fields.next()?.parse().ok()?,
        is_rendered: fields.next()? == "1",
    };
    (tab.path.is_some() || tab.backup.is_some()).then_some(tab)
}

/// Only names z++ makes itself, so a damaged record can't point outside the backup folder.
fn is_backup_name(name: &str) -> bool {
    name.strip_suffix(&format!(".{BACKUP_EXT}"))
        .is_some_and(|stem| {
            !stem.is_empty() && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        })
}

/// A path as z++ shows and stores it: full, with back slashes and long names (no PROGRA~1).
pub fn tidy_path(path: &Path) -> PathBuf {
    let slashed = PathBuf::from(path.to_string_lossy().replace('/', "\\"));
    let full = std::path::absolute(&slashed).unwrap_or(slashed);
    long_name(&full).unwrap_or(full)
}

/// The same file however it was typed: full path, back slashes, long names, any case.
pub fn path_key(path: &Path) -> String {
    tidy_path(path).to_string_lossy().to_lowercase()
}

/// Expands 8.3 short names (PROGRA~1); None if the file does not exist.
fn long_name(path: &Path) -> Option<PathBuf> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Storage::FileSystem::GetLongPathNameW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut buf = vec![0u16; 32 * 1024];
    let n = unsafe { GetLongPathNameW(wide.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(std::ffi::OsString::from_wide(&buf[..n])))
}

/// `%LOCALAPPDATA%\z++\backup`, or `backup-<profile>` under a ZPP_PROFILE.
pub fn backup_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    let name = match crate::profile::name() {
        Some(p) => format!("{BACKUP_DIR}-{p}"),
        None => BACKUP_DIR.to_string(),
    };
    base.join("z++").join(name)
}

/// A new, unused backup file name.
pub fn new_backup_name() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{:x}-{:x}.{BACKUP_EXT}", std::process::id(), nanos)
}

/// Writes a tab's text (UTF-8) to its backup file, replacing it in one step.
pub fn write_backup(name: &str, text: &str) -> std::io::Result<()> {
    let dir = backup_dir();
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, text.as_bytes())?;
    std::fs::rename(&tmp, dir.join(name))
}

pub fn read_backup(name: &str) -> Option<String> {
    let bytes = std::fs::read(backup_dir().join(name)).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn delete_backup(name: &str) {
    let _ = std::fs::remove_file(backup_dir().join(name));
}

/// Deletes backup files no saved tab refers to (left by a crash or an old session).
pub fn delete_unused_backups(keep: &[String]) {
    let Ok(entries) = std::fs::read_dir(backup_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_ours = is_backup_name(&name) || name.ends_with(".tmp");
        if is_ours && !keep.contains(&name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let tabs = [
            Tab {
                path: Some(PathBuf::from(r"C:\notes\a b.md")),
                backup: None,
                caret: 120,
                first_line: 7,
                is_rendered: true,
            },
            Tab {
                path: None,
                backup: Some("1f-2a.txt".into()),
                caret: 0,
                first_line: 0,
                is_rendered: false,
            },
        ];
        for tab in &tabs {
            assert_eq!(decode(&encode(tab)).as_ref(), Some(tab));
        }
    }

    #[test]
    fn rejects_damaged_records() {
        assert_eq!(decode(""), None);
        assert_eq!(decode("v2\tC:\\a\t\t0\t0\t0"), None);
        assert_eq!(decode("v1\tC:\\a\t\tx\t0\t0"), None);
        assert_eq!(decode("v1\t\t\t0\t0\t0"), None, "no file and no backup");
        let escape = decode("v1\tC:\\a\t..\\..\\evil.txt\t0\t0\t0").unwrap();
        assert_eq!(escape.backup, None, "backup names stay inside the folder");
    }

    #[test]
    fn path_keys_match_other_spellings() {
        let dir = std::env::temp_dir();
        let file = dir.join("zpp-key-test.txt");
        std::fs::write(&file, b"x").unwrap();
        let forward = file.to_string_lossy().replace('\\', "/").to_uppercase();
        assert_eq!(path_key(&file), path_key(Path::new(&forward)));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn backup_names_are_unique_and_valid() {
        let a = new_backup_name();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = new_backup_name();
        assert_ne!(a, b);
        assert!(is_backup_name(&a) && is_backup_name(&b));
    }
}
