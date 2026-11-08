//! Menu bar, command IDs and keyboard shortcuts.

use std::path::PathBuf;
use std::ptr::null;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::file::{Encoding, Format};
use crate::{fonts, themes, wide};

pub const IDM_NEW: u16 = 100;
pub const IDM_OPEN: u16 = 101;
pub const IDM_SAVE: u16 = 102;
pub const IDM_SAVE_AS: u16 = 103;
pub const IDM_EXIT: u16 = 104;
pub const IDM_CLOSE_TAB: u16 = 105;
pub const IDM_SAVE_ALL: u16 = 106;
pub const IDM_NEXT_TAB: u16 = 107;
pub const IDM_PREV_TAB: u16 = 108;
pub const IDM_CLEAR_RECENT: u16 = 109;
pub const IDM_UNDO: u16 = 110;
pub const IDM_REDO: u16 = 111;
pub const IDM_CUT: u16 = 112;
pub const IDM_COPY: u16 = 113;
pub const IDM_PASTE: u16 = 114;
pub const IDM_SELECT_ALL: u16 = 115;
pub const IDM_FIND: u16 = 116;
pub const IDM_FIND_NEXT: u16 = 117;
pub const IDM_FIND_PREV: u16 = 118;
pub const IDM_REPLACE: u16 = 119;
pub const IDM_WRAP: u16 = 120;
pub const IDM_TOGGLE_DARK: u16 = 121;
pub const IDM_TEXT_COLOR: u16 = 122;
pub const IDM_BACK_COLOR: u16 = 123;
pub const IDM_TOOLBAR: u16 = 124;
pub const IDM_GOTO: u16 = 125;
pub const IDM_STATUS_BAR: u16 = 126;
pub const IDM_LINE_NUMBERS: u16 = 127;
pub const IDM_FONT_DIALOG: u16 = 130;
pub const IDM_FONT_BIGGER: u16 = 131;
pub const IDM_FONT_SMALLER: u16 = 132;
pub const IDM_FONT_RESET: u16 = 133;
pub const IDM_LICENSES: u16 = 140;
pub const IDM_ABOUT: u16 = 141;
pub const IDM_ENC_UTF8: u16 = 150;
pub const IDM_ENC_UTF8_BOM: u16 = 151;
pub const IDM_ENC_UTF16LE: u16 = 152;
pub const IDM_ENC_ANSI: u16 = 153;
pub const IDM_EOL_CRLF: u16 = 160;
pub const IDM_EOL_LF: u16 = 161;
/// Theme n is IDM_THEME_BASE + n; the custom theme is n = PRESETS.len().
pub const IDM_THEME_BASE: u16 = 200;
/// Font n of the Font menu list is IDM_FONT_BASE + n.
pub const IDM_FONT_BASE: u16 = 300;
/// Recent file n is IDM_RECENT_BASE + n.
pub const IDM_RECENT_BASE: u16 = 500;
pub const MAX_RECENT: usize = 10;

const VK_TAB: u16 = 0x09;
const VK_PRIOR: u16 = 0x21;
const VK_NEXT: u16 = 0x22;
const VK_F3: u16 = 0x72;
const VK_OEM_PLUS: u16 = 0xBB;
const VK_OEM_MINUS: u16 = 0xBD;
const VK_ADD: u16 = 0x6B;
const VK_SUBTRACT: u16 = 0x6D;
const VK_NUMPAD0: u16 = 0x60;

const SEPARATOR: (u16, &str) = (0, "");

const ENCODINGS: [(u16, Encoding, &str); 4] = [
    (IDM_ENC_UTF8, Encoding::Utf8, "&UTF-8"),
    (IDM_ENC_UTF8_BOM, Encoding::Utf8Bom, "UTF-8 with &BOM"),
    (IDM_ENC_UTF16LE, Encoding::Utf16Le, "UTF-16 &LE"),
    (IDM_ENC_ANSI, Encoding::Ansi, "&ANSI"),
];

/// Submenus the app changes or shows elsewhere after the menu bar is built.
#[derive(Clone, Copy)]
pub struct Menus {
    pub bar: HMENU,
    pub recent: HMENU,
    pub encoding: HMENU,
    pub line_endings: HMENU,
}

/// What the menus show as checked.
pub struct Checks<'a> {
    pub theme: usize,
    pub font_list: &'a [String],
    pub face: &'a str,
    pub is_wrap: bool,
    pub is_toolbar: bool,
    pub is_status_bar: bool,
    pub is_line_numbers: bool,
    pub format: Format,
}

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

pub fn recent_index(id: u16) -> Option<usize> {
    let index = id.checked_sub(IDM_RECENT_BASE)? as usize;
    (index < MAX_RECENT).then_some(index)
}

pub fn encoding_for(id: u16) -> Option<Encoding> {
    ENCODINGS.iter().find(|e| e.0 == id).map(|e| e.1)
}

/// `windows_fonts` then the bundled fonts make up the Font menu list, in that order.
pub unsafe fn build(windows_fonts: &[&str]) -> Menus {
    let bar = CreateMenu();
    let recent = CreatePopupMenu();
    let file = popup(&[
        (IDM_NEW, "&New\tCtrl+N"),
        (IDM_OPEN, "&Open...\tCtrl+O"),
        (IDM_SAVE, "&Save\tCtrl+S"),
        (IDM_SAVE_AS, "Save &As...\tCtrl+Shift+S"),
        (IDM_SAVE_ALL, "Sa&ve All"),
        SEPARATOR,
        (IDM_CLOSE_TAB, "&Close Tab\tCtrl+W"),
        SEPARATOR,
        SEPARATOR,
        (IDM_EXIT, "E&xit"),
    ]);
    insert_submenu(file, 8, recent, "&Recent Files");
    let edit = popup(&[
        (IDM_UNDO, "&Undo\tCtrl+Z"),
        (IDM_REDO, "&Redo\tCtrl+Y"),
        SEPARATOR,
        (IDM_CUT, "Cu&t\tCtrl+X"),
        (IDM_COPY, "&Copy\tCtrl+C"),
        (IDM_PASTE, "&Paste\tCtrl+V"),
        SEPARATOR,
        (IDM_FIND, "&Find...\tCtrl+F"),
        (IDM_FIND_NEXT, "Find &Next\tF3"),
        (IDM_FIND_PREV, "Find Pre&vious\tShift+F3"),
        (IDM_REPLACE, "R&eplace...\tCtrl+H"),
        (IDM_GOTO, "&Go to Line...\tCtrl+G"),
        SEPARATOR,
        (IDM_SELECT_ALL, "Select &All\tCtrl+A"),
    ]);
    let encoding = popup(&ENCODINGS.map(|e| (e.0, e.2)));
    let line_endings = popup(&[
        (IDM_EOL_CRLF, "&Windows (CRLF)"),
        (IDM_EOL_LF, "&Unix (LF)"),
    ]);
    let format = popup(&[
        (IDM_FONT_DIALOG, "&Installed Fonts..."),
        SEPARATOR,
        (IDM_FONT_BIGGER, "&Bigger\tCtrl+="),
        (IDM_FONT_SMALLER, "&Smaller\tCtrl+-"),
        (IDM_FONT_RESET, "&Reset Size\tCtrl+0"),
        SEPARATOR,
        (IDM_WRAP, "&Word Wrap\tAlt+Z"),
        SEPARATOR,
    ]);
    insert_submenu(format, 0, font_menu(windows_fonts), "&Font");
    insert_submenu(format, 9, encoding, "&Encoding");
    insert_submenu(format, 10, line_endings, "&Line Endings");
    let view = popup(&[
        (IDM_TEXT_COLOR, "&Text Color..."),
        (IDM_BACK_COLOR, "&Background Color..."),
        SEPARATOR,
        (IDM_TOGGLE_DARK, "Switch &Light/Dark\tCtrl+Shift+D"),
        SEPARATOR,
        (IDM_TOOLBAR, "Tool&bar"),
        (IDM_STATUS_BAR, "&Status Bar"),
        (IDM_LINE_NUMBERS, "Line &Numbers"),
        SEPARATOR,
        (IDM_NEXT_TAB, "&Next Tab\tCtrl+Tab"),
        (IDM_PREV_TAB, "&Previous Tab\tCtrl+Shift+Tab"),
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
    Menus {
        bar,
        recent,
        encoding,
        line_endings,
    }
}

/// Fills File > Recent Files, most recent first.
pub unsafe fn set_recent(menu: HMENU, recent: &[PathBuf]) {
    while GetMenuItemCount(menu) > 0 {
        DeleteMenu(menu, 0, MF_BYPOSITION);
    }
    for (i, path) in recent.iter().take(MAX_RECENT).enumerate() {
        let label = format!("&{} {}", (i + 1) % 10, path.display());
        append(
            menu,
            IDM_RECENT_BASE + i as u16,
            &label.replace('&', "&&").replacen("&&", "&", 1),
        );
    }
    if recent.is_empty() {
        let label = wide("(none)");
        AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, label.as_ptr());
        return;
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    append(menu, IDM_CLEAR_RECENT, "&Clear List");
}

/// Three columns: light themes and Custom, then the dark themes split in two.
/// Hovering an item previews it (see window.rs, WM_MENUSELECT).
unsafe fn theme_menu() -> HMENU {
    let menu = CreatePopupMenu();
    let presets = themes::PRESETS;
    let split = themes::first_dark();
    let dark_half = split + (presets.len() - split).div_ceil(2);
    heading(menu, "Light themes", 0);
    for (i, preset) in presets.iter().enumerate().take(split) {
        append(menu, theme_id(i), preset.name);
    }
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    append(menu, theme_id(presets.len()), themes::CUSTOM);
    heading(menu, "Dark themes", MF_MENUBARBREAK);
    for (i, preset) in presets.iter().enumerate().skip(split) {
        let column_break = if i == dark_half { MF_MENUBARBREAK } else { 0 };
        let label = wide(preset.name);
        AppendMenuW(
            menu,
            MF_STRING | column_break,
            theme_id(i) as usize,
            label.as_ptr(),
        );
    }
    menu
}

unsafe fn heading(menu: HMENU, label: &str, extra_flags: MENU_ITEM_FLAGS) {
    let label = wide(label);
    AppendMenuW(menu, MF_STRING | MF_GRAYED | extra_flags, 0, label.as_ptr());
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

/// Checks the current theme, font, encoding, line endings and the on/off items.
pub unsafe fn update_checks(bar: HMENU, c: &Checks) {
    check(bar, IDM_TOOLBAR, c.is_toolbar);
    check(bar, IDM_STATUS_BAR, c.is_status_bar);
    check(bar, IDM_LINE_NUMBERS, c.is_line_numbers);
    check(bar, IDM_WRAP, c.is_wrap);
    let last_theme = theme_id(themes::PRESETS.len());
    CheckMenuRadioItem(
        bar,
        theme_id(0) as u32,
        last_theme as u32,
        theme_id(c.theme) as u32,
        MF_BYCOMMAND,
    );
    for (i, name) in c.font_list.iter().enumerate() {
        check(
            bar,
            IDM_FONT_BASE + i as u16,
            name.eq_ignore_ascii_case(c.face),
        );
    }
    let is_listed = c.font_list.iter().any(|n| n.eq_ignore_ascii_case(c.face));
    check(bar, IDM_FONT_DIALOG, !is_listed);
    let encoding = ENCODINGS
        .iter()
        .find(|e| e.1 == c.format.encoding)
        .map_or(IDM_ENC_UTF8, |e| e.0);
    CheckMenuRadioItem(
        bar,
        IDM_ENC_UTF8 as u32,
        IDM_ENC_ANSI as u32,
        encoding as u32,
        MF_BYCOMMAND,
    );
    let eol = if c.format.is_lf_only {
        IDM_EOL_LF
    } else {
        IDM_EOL_CRLF
    };
    CheckMenuRadioItem(
        bar,
        IDM_EOL_CRLF as u32,
        IDM_EOL_LF as u32,
        eol as u32,
        MF_BYCOMMAND,
    );
}

const fn acc(flags: ACCEL_VIRT_FLAGS, key: u16, cmd: u16) -> ACCEL {
    ACCEL {
        fVirt: flags,
        key,
        cmd,
    }
}

pub unsafe fn accelerators() -> HACCEL {
    let ctrl = FVIRTKEY | FCONTROL;
    let ctrl_shift = FVIRTKEY | FCONTROL | FSHIFT;
    let alt = FVIRTKEY | FALT;
    let shift = FVIRTKEY | FSHIFT;
    let table = [
        acc(ctrl, b'N' as u16, IDM_NEW),
        acc(ctrl, b'O' as u16, IDM_OPEN),
        acc(ctrl, b'S' as u16, IDM_SAVE),
        acc(ctrl_shift, b'S' as u16, IDM_SAVE_AS),
        acc(ctrl, b'W' as u16, IDM_CLOSE_TAB),
        acc(ctrl, VK_TAB, IDM_NEXT_TAB),
        acc(ctrl_shift, VK_TAB, IDM_PREV_TAB),
        acc(ctrl, VK_NEXT, IDM_NEXT_TAB),
        acc(ctrl, VK_PRIOR, IDM_PREV_TAB),
        acc(ctrl, b'F' as u16, IDM_FIND),
        acc(ctrl, b'H' as u16, IDM_REPLACE),
        acc(FVIRTKEY, VK_F3, IDM_FIND_NEXT),
        acc(shift, VK_F3, IDM_FIND_PREV),
        acc(ctrl, b'G' as u16, IDM_GOTO),
        acc(ctrl_shift, b'D' as u16, IDM_TOGGLE_DARK),
        acc(alt, b'Z' as u16, IDM_WRAP),
        acc(ctrl, VK_OEM_PLUS, IDM_FONT_BIGGER),
        acc(ctrl, VK_ADD, IDM_FONT_BIGGER),
        acc(ctrl, VK_OEM_MINUS, IDM_FONT_SMALLER),
        acc(ctrl, VK_SUBTRACT, IDM_FONT_SMALLER),
        acc(ctrl, b'0' as u16, IDM_FONT_RESET),
        acc(ctrl, VK_NUMPAD0, IDM_FONT_RESET),
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
