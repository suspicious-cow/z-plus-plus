//! Menu bar, command IDs and keyboard shortcuts.

use std::ptr::null;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::{fonts, themes, wide};

pub const IDM_NEW: u16 = 100;
pub const IDM_OPEN: u16 = 101;
pub const IDM_SAVE: u16 = 102;
pub const IDM_SAVE_AS: u16 = 103;
pub const IDM_EXIT: u16 = 104;
pub const IDM_UNDO: u16 = 110;
pub const IDM_REDO: u16 = 111;
pub const IDM_CUT: u16 = 112;
pub const IDM_COPY: u16 = 113;
pub const IDM_PASTE: u16 = 114;
pub const IDM_SELECT_ALL: u16 = 115;
pub const IDM_WRAP: u16 = 120;
pub const IDM_TOGGLE_DARK: u16 = 121;
pub const IDM_TEXT_COLOR: u16 = 122;
pub const IDM_BACK_COLOR: u16 = 123;
pub const IDM_TOOLBAR: u16 = 124;
pub const IDM_FONT_DIALOG: u16 = 130;
pub const IDM_FONT_BIGGER: u16 = 131;
pub const IDM_FONT_SMALLER: u16 = 132;
pub const IDM_FONT_RESET: u16 = 133;
pub const IDM_LICENSES: u16 = 140;
pub const IDM_ABOUT: u16 = 141;
/// Theme n is IDM_THEME_BASE + n; the custom theme is n = PRESETS.len().
pub const IDM_THEME_BASE: u16 = 200;
/// Font n of the Font menu list is IDM_FONT_BASE + n.
pub const IDM_FONT_BASE: u16 = 300;

const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_MINUS: u16 = 0xBD;
const VK_ADD: u16 = 0x6B;
const VK_SUBTRACT: u16 = 0x6D;
const VK_NUMPAD0: u16 = 0x60;

const SEPARATOR: (u16, &str) = (0, "");

pub fn theme_id(index: usize) -> u16 {
    IDM_THEME_BASE + index as u16
}

/// The theme index for a command ID, if it is a theme command.
pub fn theme_index(id: u16) -> Option<usize> {
    let index = id.checked_sub(IDM_THEME_BASE)? as usize;
    (index <= themes::PRESETS.len()).then_some(index)
}

/// The Font menu list index for a command ID, if it is a font command.
pub fn font_index(id: u16, font_count: usize) -> Option<usize> {
    let index = id.checked_sub(IDM_FONT_BASE)? as usize;
    (index < font_count).then_some(index)
}

/// `windows_fonts` then the bundled fonts make up the Font menu list, in that order.
pub unsafe fn build(windows_fonts: &[&str]) -> HMENU {
    let bar = CreateMenu();
    let file = popup(&[
        (IDM_NEW, "&New\tCtrl+N"),
        (IDM_OPEN, "&Open...\tCtrl+O"),
        (IDM_SAVE, "&Save\tCtrl+S"),
        (IDM_SAVE_AS, "Save &As...\tCtrl+Shift+S"),
        SEPARATOR,
        (IDM_EXIT, "E&xit"),
    ]);
    let edit = popup(&[
        (IDM_UNDO, "&Undo\tCtrl+Z"),
        (IDM_REDO, "&Redo\tCtrl+Y"),
        SEPARATOR,
        (IDM_CUT, "Cu&t\tCtrl+X"),
        (IDM_COPY, "&Copy\tCtrl+C"),
        (IDM_PASTE, "&Paste\tCtrl+V"),
        SEPARATOR,
        (IDM_SELECT_ALL, "Select &All\tCtrl+A"),
    ]);
    let format = popup(&[
        (IDM_FONT_DIALOG, "&Installed Fonts..."),
        SEPARATOR,
        (IDM_FONT_BIGGER, "&Bigger\tCtrl+="),
        (IDM_FONT_SMALLER, "&Smaller\tCtrl+-"),
        (IDM_FONT_RESET, "&Reset Size\tCtrl+0"),
        SEPARATOR,
        (IDM_WRAP, "&Word Wrap\tAlt+Z"),
    ]);
    insert_submenu(format, 0, font_menu(windows_fonts), "&Font");
    let view = popup(&[
        (IDM_TEXT_COLOR, "&Text Color..."),
        (IDM_BACK_COLOR, "&Background Color..."),
        SEPARATOR,
        (IDM_TOGGLE_DARK, "Switch &Light/Dark\tCtrl+Shift+D"),
        SEPARATOR,
        (IDM_TOOLBAR, "Tool&bar"),
    ]);
    insert_submenu(view, 0, theme_menu(), "T&heme");
    let help = popup(&[(IDM_LICENSES, "Font &Licenses"), (IDM_ABOUT, "&About z++")]);
    for (menu, label) in [
        (file, "&File"),
        (edit, "&Edit"),
        (format, "F&ormat"),
        (view, "&View"),
        (help, "&Help"),
    ] {
        let label = wide(label);
        AppendMenuW(bar, MF_POPUP, menu as usize, label.as_ptr());
    }
    bar
}

unsafe fn theme_menu() -> HMENU {
    let menu = CreatePopupMenu();
    for (i, preset) in themes::PRESETS.iter().enumerate() {
        append(menu, theme_id(i), preset.name);
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    append(menu, theme_id(themes::PRESETS.len()), themes::CUSTOM);
    menu
}

unsafe fn font_menu(windows_fonts: &[&str]) -> HMENU {
    let menu = CreatePopupMenu();
    for (i, face) in windows_fonts.iter().enumerate() {
        append(menu, IDM_FONT_BASE + i as u16, face);
    }
    if !fonts::BUNDLED.is_empty() {
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        let heading = wide("Bundled with z++");
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, heading.as_ptr());
    }
    for (i, font) in fonts::BUNDLED.iter().enumerate() {
        append(
            menu,
            IDM_FONT_BASE + (windows_fonts.len() + i) as u16,
            font.family,
        );
    }
    menu
}

/// Puts a check on the current theme, the current font (if it is in the list), word wrap and toolbar.
pub unsafe fn update_checks(
    bar: HMENU,
    theme: usize,
    font_list: &[String],
    face: &str,
    is_wrap: bool,
    is_toolbar: bool,
) {
    check(bar, IDM_TOOLBAR, is_toolbar);
    let last_theme = theme_id(themes::PRESETS.len());
    CheckMenuRadioItem(
        bar,
        theme_id(0) as u32,
        last_theme as u32,
        theme_id(theme) as u32,
        MF_BYCOMMAND,
    );
    for (i, name) in font_list.iter().enumerate() {
        check(
            bar,
            IDM_FONT_BASE + i as u16,
            name.eq_ignore_ascii_case(face),
        );
    }
    let is_listed = font_list.iter().any(|n| n.eq_ignore_ascii_case(face));
    check(bar, IDM_FONT_DIALOG, !is_listed);
    check(bar, IDM_WRAP, is_wrap);
}

pub unsafe fn accelerators() -> HACCEL {
    let ctrl = FVIRTKEY | FCONTROL;
    let ctrl_shift = FVIRTKEY | FCONTROL | FSHIFT;
    let alt = FVIRTKEY | FALT;
    let table = [
        ACCEL {
            fVirt: ctrl,
            key: b'N' as u16,
            cmd: IDM_NEW,
        },
        ACCEL {
            fVirt: ctrl,
            key: b'O' as u16,
            cmd: IDM_OPEN,
        },
        ACCEL {
            fVirt: ctrl,
            key: b'S' as u16,
            cmd: IDM_SAVE,
        },
        ACCEL {
            fVirt: ctrl_shift,
            key: b'S' as u16,
            cmd: IDM_SAVE_AS,
        },
        ACCEL {
            fVirt: ctrl_shift,
            key: b'D' as u16,
            cmd: IDM_TOGGLE_DARK,
        },
        ACCEL {
            fVirt: alt,
            key: b'Z' as u16,
            cmd: IDM_WRAP,
        },
        ACCEL {
            fVirt: ctrl,
            key: VK_OEM_PLUS,
            cmd: IDM_FONT_BIGGER,
        },
        ACCEL {
            fVirt: ctrl,
            key: VK_ADD,
            cmd: IDM_FONT_BIGGER,
        },
        ACCEL {
            fVirt: ctrl,
            key: VK_OEM_MINUS,
            cmd: IDM_FONT_SMALLER,
        },
        ACCEL {
            fVirt: ctrl,
            key: VK_SUBTRACT,
            cmd: IDM_FONT_SMALLER,
        },
        ACCEL {
            fVirt: ctrl,
            key: b'0' as u16,
            cmd: IDM_FONT_RESET,
        },
        ACCEL {
            fVirt: ctrl,
            key: VK_NUMPAD0,
            cmd: IDM_FONT_RESET,
        },
    ];
    CreateAcceleratorTableW(table.as_ptr(), table.len() as i32)
}

/// id 0 is a separator.
unsafe fn popup(items: &[(u16, &str)]) -> HMENU {
    let menu = CreatePopupMenu();
    for &(id, label) in items {
        if id == 0 {
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
        } else {
            append(menu, id, label);
        }
    }
    menu
}

unsafe fn append(menu: HMENU, id: u16, label: &str) {
    let label = wide(label);
    AppendMenuW(menu, MF_STRING, id as usize, label.as_ptr());
}

unsafe fn insert_submenu(menu: HMENU, position: u32, submenu: HMENU, label: &str) {
    let label = wide(label);
    InsertMenuW(
        menu,
        position,
        MF_BYPOSITION | MF_POPUP,
        submenu as usize,
        label.as_ptr(),
    );
}

unsafe fn check(bar: HMENU, id: u16, is_on: bool) {
    let state = if is_on { MF_CHECKED } else { MF_UNCHECKED };
    CheckMenuItem(bar, id as u32, MF_BYCOMMAND | state);
}
