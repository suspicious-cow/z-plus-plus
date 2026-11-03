//! Applying a look (colors and font) to the window.
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
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32,
};
use windows_sys::Win32::UI::Controls::{
    SetWindowTheme, DRAWITEMSTRUCT, EM_GETMODIFY, EM_SETMODIFY, ODS_DISABLED, ODS_GRAYED,
    ODS_HOTLIGHT, ODS_INACTIVE, ODS_NOACCEL, ODS_SELECTED,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::richedit::*;
use crate::themes::{self, Colors};

pub const WM_UAHDRAWMENU: u32 = 0x0091;
pub const WM_UAHDRAWMENUITEM: u32 = 0x0092;

const HOT_MIX_PERCENT: u32 = 18;
const INACTIVE_MIX_PERCENT: u32 = 50;
const TWIPS_PER_TENTH_POINT: u32 = 2;

const ORD_SET_PREFERRED_APP_MODE: usize = 135;
const ORD_FLUSH_MENU_THEMES: usize = 136;
const APP_MODE_FORCE_DARK: i32 = 2;
const APP_MODE_FORCE_LIGHT: i32 = 3;

pub struct Look<'a> {
    pub colors: Colors,
    pub face: &'a str,
    pub size_tenths: u32,
}

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

/// GDI objects for painting the dark menu bar in the current theme's colors.
pub struct Paint {
    bar: HBRUSH,
    hot: HBRUSH,
    text: u32,
    inactive: u32,
    font: HFONT,
}

impl Paint {
    pub unsafe fn new(colors: Colors) -> Self {
        let mut ncm: NONCLIENTMETRICSW = zeroed();
        ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            &mut ncm as *mut _ as *mut c_void,
            0,
        );
        Paint {
            bar: CreateSolidBrush(colors.bg),
            hot: CreateSolidBrush(themes::mix(colors.bg, colors.fg, HOT_MIX_PERCENT)),
            text: colors.fg,
            inactive: themes::mix(colors.bg, colors.fg, INACTIVE_MIX_PERCENT),
            font: CreateFontIndirectW(&ncm.lfMenuFont),
        }
    }
}

impl Drop for Paint {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.bar as HGDIOBJ);
            DeleteObject(self.hot as HGDIOBJ);
            DeleteObject(self.font as HGDIOBJ);
        }
    }
}

/// Popup menus. Call before the first menu is shown, and again on every change.
pub unsafe fn set_app_mode(is_dark: bool) {
    let ux = LoadLibraryExW(w!("uxtheme.dll"), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
    if ux.is_null() {
        return;
    }
    if let Some(f) = GetProcAddress(ux, ORD_SET_PREFERRED_APP_MODE as *const u8) {
        let set_mode: unsafe extern "system" fn(i32) -> i32 = transmute(f);
        set_mode(if is_dark {
            APP_MODE_FORCE_DARK
        } else {
            APP_MODE_FORCE_LIGHT
        });
    }
    if let Some(f) = GetProcAddress(ux, ORD_FLUSH_MENU_THEMES as *const u8) {
        let flush: unsafe extern "system" fn() = transmute(f);
        flush();
    }
}

pub unsafe fn apply(main: HWND, edit: HWND, look: &Look) {
    let is_dark = themes::is_dark(look.colors.bg);
    set_app_mode(is_dark);
    let flag: i32 = is_dark as i32;
    DwmSetWindowAttribute(
        main,
        DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        &flag as *const i32 as *const c_void,
        4,
    );
    let scroll_theme = if is_dark {
        w!("DarkMode_Explorer")
    } else {
        w!("Explorer")
    };
    SetWindowTheme(edit, scroll_theme, null());
    apply_editor(edit, look);
    DrawMenuBar(main);
    RedrawWindow(
        main,
        null(),
        null_mut(),
        RDW_FRAME | RDW_INVALIDATE | RDW_ALLCHILDREN,
    );
}

unsafe fn apply_editor(edit: HWND, look: &Look) {
    let was_modified = SendMessageW(edit, EM_GETMODIFY, 0, 0);
    SendMessageW(edit, EM_SETBKGNDCOLOR, 0, look.colors.bg as isize);
    let mut cf: CHARFORMATW = zeroed();
    cf.cbSize = size_of::<CHARFORMATW>() as u32;
    cf.dwMask = CFM_COLOR | CFM_FACE | CFM_SIZE;
    cf.crTextColor = look.colors.fg;
    cf.yHeight = (look.size_tenths * TWIPS_PER_TENTH_POINT) as i32;
    let face: Vec<u16> = look
        .face
        .encode_utf16()
        .take(cf.szFaceName.len() - 1)
        .collect();
    cf.szFaceName[..face.len()].copy_from_slice(&face);
    let cf_ptr = &cf as *const CHARFORMATW as isize;
    SendMessageW(edit, EM_SETCHARFORMAT, SCF_DEFAULT as usize, cf_ptr);
    SendMessageW(edit, EM_SETCHARFORMAT, SCF_ALL as usize, cf_ptr);
    SendMessageW(edit, EM_SETMODIFY, was_modified as usize, 0);
}

/// Fills the whole menu bar. Only called for dark themes.
pub unsafe fn draw_menu_bar(hwnd: HWND, lparam: LPARAM, paint: &Paint) {
    let um = &*(lparam as *const UahMenu);
    let Some(mut rc) = menu_bar_rect(hwnd) else {
        return;
    };
    let mut win: RECT = zeroed();
    GetWindowRect(hwnd, &mut win);
    OffsetRect(&mut rc, -win.left, -win.top);
    FillRect(um.hdc, &rc, paint.bar);
}

/// Draws one top-level menu item. Only called for dark themes.
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
    FillRect(
        item.um.hdc,
        &item.dis.rcItem,
        if is_hot { paint.hot } else { paint.bar },
    );

    let mut flags = DT_CENTER | DT_SINGLELINE | DT_VCENTER;
    if state & ODS_NOACCEL != 0 {
        flags |= DT_HIDEPREFIX;
    }
    let is_dim = state & (ODS_INACTIVE | ODS_DISABLED | ODS_GRAYED) != 0;
    let hdc = item.um.hdc;
    let old_font = SelectObject(hdc, paint.font as HGDIOBJ);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, if is_dim { paint.inactive } else { paint.text });
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
    let line = RECT {
        left: client.left,
        top: client.top - 1,
        right: client.right,
        bottom: client.top,
    };
    let hdc = GetWindowDC(hwnd);
    FillRect(hdc, &line, paint.bar);
    ReleaseDC(hwnd, hdc);
}

unsafe fn menu_bar_rect(hwnd: HWND) -> Option<RECT> {
    let mut mbi: MENUBARINFO = zeroed();
    mbi.cbSize = size_of::<MENUBARINFO>() as u32;
    (GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut mbi) != 0).then_some(mbi.rcBar)
}
