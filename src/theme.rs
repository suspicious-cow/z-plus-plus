//! Light and dark mode.
//!
//! Documented: the title bar (DWM), the editor colors (RichEdit), the scroll bars (SetWindowTheme).
//! Undocumented, same approach as Notepad++ and the win32-darkmode samples: popup menus follow
//! uxtheme ordinal 135 (SetPreferredAppMode), and the menu bar is painted by handling the
//! WM_UAHDRAWMENU / WM_UAHDRAWMENUITEM messages. If the ordinals ever disappear, menus stay light.

use std::ffi::c_void;
use std::mem::{size_of, transmute, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use crate::richedit::*;
use windows_sys::Win32::UI::Controls::{
    SetWindowTheme, DRAWITEMSTRUCT, EM_GETMODIFY, EM_SETMODIFY, ODS_DISABLED, ODS_GRAYED, ODS_HOTLIGHT,
    ODS_INACTIVE, ODS_NOACCEL, ODS_SELECTED,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WM_UAHDRAWMENU: u32 = 0x0091;
pub const WM_UAHDRAWMENUITEM: u32 = 0x0092;

const DARK_BG: u32 = 0x001E_1E1E;
const DARK_FG: u32 = 0x00D4_D4D4;
const LIGHT_BG: u32 = 0x00FF_FFFF;
const LIGHT_FG: u32 = 0x0000_0000;
const MENU_BAR_DARK: u32 = 0x002B_2B2B;
const MENU_HOT_DARK: u32 = 0x0041_4141;
const MENU_TEXT_DARK: u32 = 0x00E8_E8E8;
const MENU_TEXT_INACTIVE_DARK: u32 = 0x0080_8080;

const EDITOR_FONT: &str = "Consolas";
const EDITOR_FONT_TWIPS: i32 = 220; // 11 pt

const ORD_SET_PREFERRED_APP_MODE: usize = 135;
const ORD_FLUSH_MENU_THEMES: usize = 136;
const APP_MODE_FORCE_DARK: i32 = 2;
const APP_MODE_FORCE_LIGHT: i32 = 3;

#[repr(C)]
struct UahMenu {
    hmenu: HMENU,
    hdc: HDC,
    flags: u32,
}

#[repr(C)]
struct UahMenuItem {
    position: i32,
    item_metrics: [u32; 8],
    popup_metrics: [u32; 5],
}

#[repr(C)]
struct UahDrawMenuItem {
    dis: DRAWITEMSTRUCT,
    um: UahMenu,
    umi: UahMenuItem,
}

pub struct Paint {
    bar: HBRUSH,
    hot: HBRUSH,
    font: HFONT,
}

impl Paint {
    pub unsafe fn new() -> Self {
        let mut ncm: NONCLIENTMETRICSW = zeroed();
        ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut c_void, 0);
        Paint {
            bar: CreateSolidBrush(MENU_BAR_DARK),
            hot: CreateSolidBrush(MENU_HOT_DARK),
            font: CreateFontIndirectW(&ncm.lfMenuFont),
        }
    }
}

/// Popup menus. Call before the first menu is shown, and again on every toggle.
pub unsafe fn set_app_mode(is_dark: bool) {
    let ux = LoadLibraryExW(w!("uxtheme.dll"), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
    if ux.is_null() {
        return;
    }
    if let Some(f) = GetProcAddress(ux, ORD_SET_PREFERRED_APP_MODE as *const u8) {
        let set_mode: unsafe extern "system" fn(i32) -> i32 = transmute(f);
        set_mode(if is_dark { APP_MODE_FORCE_DARK } else { APP_MODE_FORCE_LIGHT });
    }
    if let Some(f) = GetProcAddress(ux, ORD_FLUSH_MENU_THEMES as *const u8) {
        let flush: unsafe extern "system" fn() = transmute(f);
        flush();
    }
}

pub unsafe fn apply(main: HWND, edit: HWND, is_dark: bool) {
    set_app_mode(is_dark);
    let flag: i32 = is_dark as i32;
    DwmSetWindowAttribute(main, DWMWA_USE_IMMERSIVE_DARK_MODE as u32, &flag as *const i32 as *const c_void, 4);
    let scroll_theme = if is_dark { w!("DarkMode_Explorer") } else { w!("Explorer") };
    SetWindowTheme(edit, scroll_theme, null());
    apply_editor(edit, is_dark);
    DrawMenuBar(main);
    RedrawWindow(main, null(), null_mut(), RDW_FRAME | RDW_INVALIDATE | RDW_ALLCHILDREN);
}

unsafe fn apply_editor(edit: HWND, is_dark: bool) {
    let (bg, fg) = if is_dark { (DARK_BG, DARK_FG) } else { (LIGHT_BG, LIGHT_FG) };
    let was_modified = SendMessageW(edit, EM_GETMODIFY, 0, 0);
    SendMessageW(edit, EM_SETBKGNDCOLOR, 0, bg as isize);
    let mut cf: CHARFORMATW = zeroed();
    cf.cbSize = size_of::<CHARFORMATW>() as u32;
    cf.dwMask = CFM_COLOR | CFM_FACE | CFM_SIZE;
    cf.crTextColor = fg;
    cf.yHeight = EDITOR_FONT_TWIPS;
    for (dst, src) in cf.szFaceName.iter_mut().zip(EDITOR_FONT.encode_utf16()) {
        *dst = src;
    }
    let cf_ptr = &cf as *const CHARFORMATW as isize;
    SendMessageW(edit, EM_SETCHARFORMAT, SCF_DEFAULT as usize, cf_ptr);
    SendMessageW(edit, EM_SETCHARFORMAT, SCF_ALL as usize, cf_ptr);
    SendMessageW(edit, EM_SETMODIFY, was_modified as usize, 0);
}

/// Fills the whole menu bar. Only called in dark mode.
pub unsafe fn draw_menu_bar(hwnd: HWND, lparam: LPARAM, paint: &Paint) {
    let um = &*(lparam as *const UahMenu);
    let Some(mut rc) = menu_bar_rect(hwnd) else { return };
    let mut win: RECT = zeroed();
    GetWindowRect(hwnd, &mut win);
    OffsetRect(&mut rc, -win.left, -win.top);
    FillRect(um.hdc, &rc, paint.bar);
}

/// Draws one top-level menu item. Only called in dark mode.
pub unsafe fn draw_menu_item(lparam: LPARAM, paint: &Paint) {
    let item = &*(lparam as *const UahDrawMenuItem);
    let mut buf = [0u16; 256];
    let mut mii: MENUITEMINFOW = zeroed();
    mii.cbSize = size_of::<MENUITEMINFOW>() as u32;
    mii.fMask = MIIM_STRING;
    mii.dwTypeData = buf.as_mut_ptr();
    mii.cch = (buf.len() - 1) as u32;
    GetMenuItemInfoW(item.um.hmenu, item.umi.position as u32, 1, &mut mii);

    let state = item.dis.itemState;
    let is_hot = state & (ODS_HOTLIGHT | ODS_SELECTED) != 0;
    FillRect(item.um.hdc, &item.dis.rcItem, if is_hot { paint.hot } else { paint.bar });

    let mut flags = DT_CENTER | DT_SINGLELINE | DT_VCENTER;
    if state & ODS_NOACCEL != 0 {
        flags |= DT_HIDEPREFIX;
    }
    let color = if state & (ODS_INACTIVE | ODS_DISABLED | ODS_GRAYED) != 0 {
        MENU_TEXT_INACTIVE_DARK
    } else {
        MENU_TEXT_DARK
    };
    let hdc = item.um.hdc;
    let old_font = SelectObject(hdc, paint.font as HGDIOBJ);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, color);
    let mut rc = item.dis.rcItem;
    DrawTextW(hdc, buf.as_ptr(), mii.cch as i32, &mut rc, flags);
    SelectObject(hdc, old_font);
}

/// Covers the one-pixel light line Windows draws between the menu bar and the client area.
pub unsafe fn paint_menu_underline(hwnd: HWND, paint: &Paint) {
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    MapWindowPoints(hwnd, null_mut(), &mut client as *mut RECT as *mut _, 2);
    let mut win: RECT = zeroed();
    GetWindowRect(hwnd, &mut win);
    OffsetRect(&mut client, -win.left, -win.top);
    let line = RECT { left: client.left, top: client.top - 1, right: client.right, bottom: client.top };
    let hdc = GetWindowDC(hwnd);
    FillRect(hdc, &line, paint.bar);
    ReleaseDC(hwnd, hdc);
}

unsafe fn menu_bar_rect(hwnd: HWND) -> Option<RECT> {
    let mut mbi: MENUBARINFO = zeroed();
    mbi.cbSize = size_of::<MENUBARINFO>() as u32;
    (GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut mbi) != 0).then_some(mbi.rcBar)
}
