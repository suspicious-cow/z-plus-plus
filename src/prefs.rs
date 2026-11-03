//! User preferences in HKCU\Software\z++.

use std::ffi::c_void;
use std::ptr::null_mut;
use windows_sys::w;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD,
};

const PREFS_KEY: *const u16 = w!("Software\\z++");
const PERSONALIZE_KEY: *const u16 = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

pub struct Prefs {
    pub is_dark: bool,
    pub is_wrap: bool,
}

pub fn load() -> Prefs {
    let system_dark = read_dword(HKEY_CURRENT_USER, PERSONALIZE_KEY, w!("AppsUseLightTheme")) == Some(0);
    Prefs {
        is_dark: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("DarkMode")).map_or(system_dark, |v| v != 0),
        is_wrap: read_dword(HKEY_CURRENT_USER, PREFS_KEY, w!("WordWrap")).is_some_and(|v| v != 0),
    }
}

pub fn save(prefs: &Prefs) {
    write_dword(w!("DarkMode"), prefs.is_dark as u32);
    write_dword(w!("WordWrap"), prefs.is_wrap as u32);
}

fn read_dword(root: HKEY, key: *const u16, name: *const u16) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(root, key, name, RRF_RT_REG_DWORD, null_mut(), &mut value as *mut u32 as *mut c_void, &mut size)
    };
    (status == ERROR_SUCCESS).then_some(value)
}

fn write_dword(name: *const u16, value: u32) {
    unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, PREFS_KEY, name, REG_DWORD, &value as *const u32 as *const c_void, 4);
    }
}
