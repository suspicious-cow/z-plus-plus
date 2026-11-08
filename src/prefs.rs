//! User preferences in HKCU\Software\z++.

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::null_mut;
use windows_sys::w;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, REG_DWORD, REG_MULTI_SZ, REG_SZ,
    RRF_RT_REG_DWORD, RRF_RT_REG_MULTI_SZ, RRF_RT_REG_SZ,
};

use crate::fonts;
use crate::themes::{self, Colors};

const PREFS_KEY: *const u16 = w!("Software\\z++");
const PERSONALIZE_KEY: *const u16 =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
const MAX_STRING_UNITS: u32 = 256;
/// Enough for the recent-files list and a session of long paths.
const MAX_MULTI_STRING_UNITS: usize = 64 * 1024;

/// The open files and window position, saved on exit and restored on the next start.
#[derive(Default)]
pub struct Session {
    pub files: Vec<PathBuf>,
    pub active: usize,
    pub recent: Vec<PathBuf>,
    pub window: Option<RECT>,
    pub is_maximized: bool,
}

pub struct Prefs {
    pub theme: String,
    pub last_light: String,
    pub last_dark: String,
    pub custom: Colors,
    pub face: String,
    pub size_tenths: u32,
    pub is_wrap: bool,
    pub is_toolbar: bool,
    pub is_status_bar: bool,
    pub is_line_numbers: bool,
}

pub fn load() -> Prefs {
    let light = themes::PRESETS[0].colors;
    Prefs {
        theme: read_string(w!("Theme")).unwrap_or_else(legacy_theme),
        last_light: read_string(w!("LastLightTheme")).unwrap_or_else(|| themes::LIGHT.into()),
        last_dark: read_string(w!("LastDarkTheme")).unwrap_or_else(|| themes::DARK.into()),
        custom: Colors {
            fg: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("CustomText")).unwrap_or(light.fg),
            bg: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("CustomBackground"))
                .unwrap_or(light.bg),
        },
        face: read_string(w!("FontFace")).unwrap_or_else(|| fonts::DEFAULT_FACE.into()),
        size_tenths: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("FontSize"))
            .unwrap_or(fonts::DEFAULT_SIZE_TENTHS),
        is_wrap: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("WordWrap")).is_some_and(|v| v != 0),
        is_toolbar: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("Toolbar")).is_none_or(|v| v != 0),
        is_status_bar: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("StatusBar"))
            .is_none_or(|v| v != 0),
        is_line_numbers: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("LineNumbers"))
            .is_none_or(|v| v != 0),
    }
}

pub fn load_session() -> Session {
    let dword = |name| read_dword(HKEY_CURRENT_USER, PREFS_KEY, name);
    let window = match (
        dword(w!("WindowLeft")),
        dword(w!("WindowTop")),
        dword(w!("WindowRight")),
        dword(w!("WindowBottom")),
    ) {
        (Some(left), Some(top), Some(right), Some(bottom)) => Some(RECT {
            left: left as i32,
            top: top as i32,
            right: right as i32,
            bottom: bottom as i32,
        }),
        _ => None,
    };
    Session {
        files: read_paths(w!("SessionFiles")),
        active: dword(w!("SessionActive")).unwrap_or(0) as usize,
        recent: read_paths(w!("RecentFiles")),
        window,
        is_maximized: dword(w!("WindowMaximized")).is_some_and(|v| v != 0),
    }
}

pub fn save_session(session: &Session) {
    write_paths(w!("SessionFiles"), &session.files);
    write_dword(w!("SessionActive"), session.active as u32);
    write_paths(w!("RecentFiles"), &session.recent);
    if let Some(rc) = session.window {
        write_dword(w!("WindowLeft"), rc.left as u32);
        write_dword(w!("WindowTop"), rc.top as u32);
        write_dword(w!("WindowRight"), rc.right as u32);
        write_dword(w!("WindowBottom"), rc.bottom as u32);
    }
    write_dword(w!("WindowMaximized"), session.is_maximized as u32);
}

/// Recent files are saved as soon as they change, so a crash does not lose the list.
pub fn save_recent(recent: &[PathBuf]) {
    write_paths(w!("RecentFiles"), recent);
}

fn read_paths(name: *const u16) -> Vec<PathBuf> {
    let mut buf = vec![0u16; MAX_MULTI_STRING_UNITS];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PREFS_KEY,
            name,
            RRF_RT_REG_MULTI_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return Vec::new();
    }
    buf.truncate(size as usize / 2);
    buf.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(String::from_utf16_lossy(s)))
        .collect()
}

fn write_paths(name: *const u16, paths: &[PathBuf]) {
    let mut units: Vec<u16> = Vec::new();
    for p in paths {
        units.extend(p.to_string_lossy().encode_utf16());
        units.push(0);
    }
    units.push(0);
    if units.len() == 1 {
        units.push(0);
    }
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PREFS_KEY,
            name,
            REG_MULTI_SZ,
            units.as_ptr() as *const c_void,
            (units.len() * 2) as u32,
        );
    }
}

pub fn save(prefs: &Prefs) {
    write_string(w!("Theme"), &prefs.theme);
    write_string(w!("LastLightTheme"), &prefs.last_light);
    write_string(w!("LastDarkTheme"), &prefs.last_dark);
    write_dword(w!("CustomText"), prefs.custom.fg);
    write_dword(w!("CustomBackground"), prefs.custom.bg);
    write_string(w!("FontFace"), &prefs.face);
    write_dword(w!("FontSize"), prefs.size_tenths);
    write_dword(w!("WordWrap"), prefs.is_wrap as u32);
    write_dword(w!("Toolbar"), prefs.is_toolbar as u32);
    write_dword(w!("StatusBar"), prefs.is_status_bar as u32);
    write_dword(w!("LineNumbers"), prefs.is_line_numbers as u32);
}

/// v0.1 stored only DarkMode. Without either, follow the Windows app theme.
fn legacy_theme() -> String {
    let system_dark =
        read_dword(HKEY_CURRENT_USER, PERSONALIZE_KEY, w!("AppsUseLightTheme")) == Some(0);
    let is_dark =
        read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("DarkMode")).map_or(system_dark, |v| v != 0);
    if is_dark { themes::DARK } else { themes::LIGHT }.into()
}

fn read_dword(root: HKEY, key: *const u16, name: *const u16) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            root,
            key,
            name,
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut value as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    (status == ERROR_SUCCESS).then_some(value)
}

fn read_string(name: *const u16) -> Option<String> {
    let mut buf = [0u16; MAX_STRING_UNITS as usize];
    let mut size = MAX_STRING_UNITS * 2;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PREFS_KEY,
            name,
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len])).filter(|s| !s.is_empty())
}

fn write_dword(name: *const u16, value: u32) {
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PREFS_KEY,
            name,
            REG_DWORD,
            &value as *const u32 as *const c_void,
            4,
        );
    }
}

fn write_string(name: *const u16, value: &str) {
    let units = crate::wide(value);
    let bytes = (units.len() * 2) as u32;
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PREFS_KEY,
            name,
            REG_SZ,
            units.as_ptr() as *const c_void,
            bytes,
        );
    }
}
