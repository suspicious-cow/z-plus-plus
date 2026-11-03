//! Office-style toolbar under the menu bar: groups of buttons with icons and labels, plus font,
//! size and theme dropdowns. It draws itself in the current theme's colors. Buttons post the same
//! WM_COMMAND IDs as the menus, so every toolbar action is also a menu action.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::menu::*;
use crate::themes::{self, Colors};
use crate::{fonts, wide};

/// Posted to the main window with the new font size in tenths of a point as wparam.
pub const WM_APP_SET_SIZE: u32 = WM_APP + 1;
/// Posted to the main window with a theme index to preview, or END_PREVIEW to go back.
pub const WM_APP_PREVIEW_THEME: u32 = WM_APP + 2;
pub const END_PREVIEW: usize = usize::MAX;

const THEME_ROW_H: i32 = 24;
const THEME_FIELD_H: i32 = 17;
const THEME_TEXT_PAD: i32 = 8;
const HOVER_FRAME: i32 = 2;
/// Rows shown when a dropdown opens. Windows' own default is 30, taller than the editor window.
const COMBO_VISIBLE_ROWS: usize = 16;

const CLASS: *const u16 = w!("ZppToolbar");
const HEIGHT: i32 = 90;
const CAPTION_H: i32 = 18;
const PAD: i32 = 4;
const GAP: i32 = 2;
const GROUP_GAP: i32 = 12;
const LARGE_MIN_W: i32 = 46;
const LARGE_H: i32 = 64;
const SMALL_H: i32 = 28;
const SMALL_ICON_W: i32 = 30;
const ROW1_Y: i32 = 8;
const ROW2_Y: i32 = 38;
const ICON_PX: i32 = 20;
const SMALL_ICON_PX: i32 = 16;
const CORNER: i32 = 6;
const SWATCH_H: i32 = 3;
const COMBO_LIST_H: i32 = 360;
const BASE_DPI: i32 = 96;

const BG_MIX: u32 = 6;
const HOT_MIX: u32 = 16;
const DOWN_MIX: u32 = 28;
const DIM_MIX: u32 = 55;
const LINE_MIX: u32 = 18;

const SIZES_PT: &[u32] = &[8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 28, 32, 36, 48, 72];
const ICON_FONTS: &[&str] = &["Segoe Fluent Icons", "Segoe MDL2 Assets"];
const MORE_FONTS: &str = "More fonts...";

const ID_FONT_COMBO: usize = 10;
const ID_SIZE_COMBO: usize = 11;
const ID_THEME_COMBO: usize = 12;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Large,
    Small,
}

#[derive(Clone, Copy, PartialEq)]
enum Combo {
    Font,
    FontSize,
    Theme,
}

#[derive(Clone, Copy)]
enum Item {
    Button {
        id: u16,
        glyph: u16,
        label: &'static str,
        tip: &'static str,
        kind: Kind,
    },
    Dropdown {
        which: Combo,
        width: i32,
    },
}

struct Group {
    caption: &'static str,
    rows: &'static [&'static [Item]],
}

const fn large(id: u16, glyph: u16, label: &'static str, tip: &'static str) -> Item {
    Item::Button {
        id,
        glyph,
        label,
        tip,
        kind: Kind::Large,
    }
}

const fn small(id: u16, glyph: u16, label: &'static str, tip: &'static str) -> Item {
    Item::Button {
        id,
        glyph,
        label,
        tip,
        kind: Kind::Small,
    }
}

// Glyphs are Segoe Fluent Icons / Segoe MDL2 Assets code points.
const GROUPS: &[Group] = &[
    Group {
        caption: "File",
        rows: &[&[
            large(IDM_NEW, 0xE8A5, "New", "New (Ctrl+N)"),
            large(IDM_OPEN, 0xE8E5, "Open", "Open (Ctrl+O)"),
            large(IDM_SAVE, 0xE74E, "Save", "Save (Ctrl+S)"),
            large(IDM_SAVE_AS, 0xE792, "Save As", "Save As (Ctrl+Shift+S)"),
        ]],
    },
    Group {
        caption: "Edit",
        rows: &[&[
            large(IDM_UNDO, 0xE7A7, "Undo", "Undo (Ctrl+Z)"),
            large(IDM_REDO, 0xE7A6, "Redo", "Redo (Ctrl+Y)"),
            large(IDM_CUT, 0xE8C6, "Cut", "Cut (Ctrl+X)"),
            large(IDM_COPY, 0xE8C8, "Copy", "Copy (Ctrl+C)"),
            large(IDM_PASTE, 0xE77F, "Paste", "Paste (Ctrl+V)"),
            large(IDM_SELECT_ALL, 0xE8B3, "Select All", "Select All (Ctrl+A)"),
        ]],
    },
    Group {
        caption: "Font",
        rows: &[
            &[
                Item::Dropdown {
                    which: Combo::Font,
                    width: 172,
                },
                Item::Dropdown {
                    which: Combo::FontSize,
                    width: 58,
                },
            ],
            &[
                small(IDM_FONT_BIGGER, 0xE8E8, "", "Bigger (Ctrl+=)"),
                small(IDM_FONT_SMALLER, 0xE8E7, "", "Smaller (Ctrl+-)"),
                small(IDM_TEXT_COLOR, 0xE8D3, "", "Text color"),
                small(IDM_BACK_COLOR, 0xE790, "", "Background color"),
                small(IDM_FONT_DIALOG, 0xE8D2, "More", "Installed fonts"),
            ],
        ],
    },
    Group {
        caption: "View",
        rows: &[
            &[Item::Dropdown {
                which: Combo::Theme,
                width: 150,
            }],
            &[
                small(IDM_WRAP, 0xE751, "Wrap", "Word wrap (Alt+Z)"),
                small(
                    IDM_TOGGLE_DARK,
                    0xE708,
                    "Light",
                    "Switch light/dark (Ctrl+Shift+D)",
                ),
            ],
        ],
    },
];

const GLYPH_MOON: u16 = 0xE708;
const GLYPH_SUN: u16 = 0xE706;

struct Placed {
    id: u16,
    glyph: u16,
    label: &'static str,
    kind: Kind,
    rect: RECT,
}

struct GroupBox {
    caption: &'static str,
    left: i32,
    right: i32,
}

pub struct State<'a> {
    pub colors: Colors,
    pub custom: Colors,
    pub theme: usize,
    pub font_list: &'a [String],
    pub face: &'a str,
    pub size_tenths: u32,
    pub is_wrap: bool,
}

struct Toolbar {
    hwnd: Cell<HWND>,
    dpi: Cell<i32>,
    buttons: RefCell<Vec<Placed>>,
    groups: RefCell<Vec<GroupBox>>,
    hot: Cell<Option<usize>>,
    pressed: Cell<Option<usize>>,
    is_tracking: Cell<bool>,
    colors: Cell<Colors>,
    is_wrap: Cell<bool>,
    combos: Cell<[HWND; 3]>,
    ui_font: Cell<HFONT>,
    icon_font: Cell<HFONT>,
    small_icon_font: Cell<HFONT>,
    list_brush: Cell<HBRUSH>,
    font_cmds: RefCell<Vec<u16>>,
    sizes: RefCell<Vec<u32>>,
    /// Custom theme colors, for drawing the Custom row of the theme dropdown.
    custom: Cell<Colors>,
    /// The theme row last previewed while the dropdown is open.
    hover_theme: Cell<Option<usize>>,
}

thread_local! {
    static TB: Toolbar = const { Toolbar {
        hwnd: Cell::new(null_mut()),
        dpi: Cell::new(BASE_DPI),
        buttons: RefCell::new(Vec::new()),
        groups: RefCell::new(Vec::new()),
        hot: Cell::new(None),
        pressed: Cell::new(None),
        is_tracking: Cell::new(false),
        colors: Cell::new(themes::PRESETS[0].colors),
        is_wrap: Cell::new(false),
        combos: Cell::new([null_mut(); 3]),
        ui_font: Cell::new(null_mut()),
        icon_font: Cell::new(null_mut()),
        small_icon_font: Cell::new(null_mut()),
        list_brush: Cell::new(null_mut()),
        font_cmds: RefCell::new(Vec::new()),
        sizes: RefCell::new(Vec::new()),
        custom: Cell::new(themes::PRESETS[0].colors),
        hover_theme: Cell::new(None),
    } };
}

fn tb<R>(f: impl FnOnce(&Toolbar) -> R) -> R {
    TB.with(f)
}

fn s(v: i32) -> i32 {
    tb(|t| v * t.dpi.get() / BASE_DPI)
}

pub fn height() -> i32 {
    s(HEIGHT)
}

pub unsafe fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: CLASS,
        ..zeroed()
    };
    RegisterClassExW(&wc);
    let hwnd = CreateWindowExW(
        0,
        CLASS,
        null(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        parent,
        null_mut(),
        hinst,
        null(),
    );
    let screen = GetDC(null_mut());
    let dpi = GetDeviceCaps(screen, LOGPIXELSY as _);
    ReleaseDC(null_mut(), screen);
    tb(|t| {
        t.hwnd.set(hwnd);
        t.dpi.set(dpi);
    });
    create_fonts();
    layout(hwnd, hinst);
    hwnd
}

unsafe fn create_fonts() {
    let mut ncm: NONCLIENTMETRICSW = zeroed();
    ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
    SystemParametersInfoW(
        SPI_GETNONCLIENTMETRICS,
        ncm.cbSize,
        &mut ncm as *mut _ as *mut _,
        0,
    );
    let icon_face = ICON_FONTS
        .iter()
        .copied()
        .find(|f| fonts::is_installed(f))
        .unwrap_or(ICON_FONTS[0]);
    tb(|t| {
        t.ui_font.set(CreateFontIndirectW(&ncm.lfMessageFont));
        t.icon_font.set(icon_font(icon_face, s(ICON_PX)));
        t.small_icon_font
            .set(icon_font(icon_face, s(SMALL_ICON_PX)));
    });
}

unsafe fn icon_font(face: &str, px: i32) -> HFONT {
    let face = wide(face);
    CreateFontW(
        -px,
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        face.as_ptr(),
    )
}

/// Places buttons and creates the dropdowns and tooltips. Positions are fixed, so this runs once.
unsafe fn layout(hwnd: HWND, hinst: HINSTANCE) {
    let hdc = GetDC(hwnd);
    let old = SelectObject(hdc, tb(|t| t.ui_font.get()) as HGDIOBJ);
    let mut buttons = Vec::new();
    let mut groups = Vec::new();
    let mut combos = [null_mut(); 3];
    let mut x = s(PAD);
    for group in GROUPS {
        let left = x;
        let mut right = left;
        for (r, row) in group.rows.iter().enumerate() {
            let y = if group.rows.len() == 1 {
                s(PAD)
            } else if r == 0 {
                s(ROW1_Y)
            } else {
                s(ROW2_Y)
            };
            let mut rx = left;
            for item in row.iter() {
                let w = item_width(hdc, item);
                match *item {
                    Item::Button {
                        id,
                        glyph,
                        label,
                        kind,
                        ..
                    } => {
                        let h = if kind == Kind::Large {
                            s(LARGE_H)
                        } else {
                            s(SMALL_H)
                        };
                        let rect = RECT {
                            left: rx,
                            top: y,
                            right: rx + w,
                            bottom: y + h,
                        };
                        buttons.push(Placed {
                            id,
                            glyph,
                            label,
                            kind,
                            rect,
                        });
                    }
                    Item::Dropdown { which, .. } => {
                        combos[which as usize] = create_combo(hwnd, hinst, which, rx, y, w);
                    }
                }
                rx += w + s(GAP);
            }
            right = right.max(rx - s(GAP));
        }
        groups.push(GroupBox {
            caption: group.caption,
            left,
            right,
        });
        x = right + s(GROUP_GAP);
    }
    SelectObject(hdc, old);
    ReleaseDC(hwnd, hdc);
    add_tooltips(hwnd, hinst, &buttons);
    tb(|t| {
        *t.buttons.borrow_mut() = buttons;
        *t.groups.borrow_mut() = groups;
        t.combos.set(combos);
    });
}

unsafe fn item_width(hdc: HDC, item: &Item) -> i32 {
    match *item {
        Item::Dropdown { width, .. } => s(width),
        Item::Button {
            id, label, kind, ..
        } => {
            let text_w = if id == IDM_TOGGLE_DARK {
                text_width(hdc, "Light").max(text_width(hdc, "Dark"))
            } else {
                text_width(hdc, label)
            };
            match kind {
                Kind::Large => s(LARGE_MIN_W).max(text_w + s(12)),
                Kind::Small if label.is_empty() => s(SMALL_ICON_W),
                Kind::Small => s(SMALL_ICON_W) + text_w + s(8),
            }
        }
    }
}

unsafe fn text_width(hdc: HDC, text: &str) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut size: SIZE = zeroed();
    GetTextExtentPoint32W(hdc, units.as_ptr(), units.len() as i32, &mut size);
    size.cx
}

unsafe fn create_combo(
    parent: HWND,
    hinst: HINSTANCE,
    which: Combo,
    x: i32,
    y: i32,
    w: i32,
) -> HWND {
    let id = match which {
        Combo::Font => ID_FONT_COMBO,
        Combo::FontSize => ID_SIZE_COMBO,
        Combo::Theme => ID_THEME_COMBO,
    };
    let mut style = WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_TABSTOP | CBS_DROPDOWNLIST as u32;
    if which == Combo::Theme {
        // Each row is drawn in its own theme's colors (see draw_theme_item).
        style |= (CBS_OWNERDRAWFIXED | CBS_HASSTRINGS) as u32;
    }
    let combo = CreateWindowExW(
        0,
        w!("COMBOBOX"),
        null(),
        style,
        x,
        y,
        w,
        s(COMBO_LIST_H),
        parent,
        id as _,
        hinst,
        null(),
    );
    SendMessageW(combo, WM_SETFONT, tb(|t| t.ui_font.get()) as usize, 0);
    SendMessageW(combo, CB_SETMINVISIBLE, COMBO_VISIBLE_ROWS, 0);
    combo
}

unsafe fn add_tooltips(hwnd: HWND, hinst: HINSTANCE, buttons: &[Placed]) {
    let style = WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX;
    let tip = CreateWindowExW(
        WS_EX_TOPMOST,
        w!("tooltips_class32"),
        null(),
        style,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        hwnd,
        null_mut(),
        hinst,
        null(),
    );
    let tips = GROUPS
        .iter()
        .flat_map(|g| g.rows.iter())
        .flat_map(|r| r.iter())
        .filter_map(|item| match *item {
            Item::Button { tip, .. } => Some(tip),
            Item::Dropdown { .. } => None,
        });
    for (i, (button, text)) in buttons.iter().zip(tips).enumerate() {
        let mut text = wide(text);
        let mut ti: TTTOOLINFOW = zeroed();
        ti.cbSize = size_of::<TTTOOLINFOW>() as u32;
        ti.uFlags = TTF_SUBCLASS;
        ti.hwnd = hwnd;
        ti.uId = i;
        ti.rect = button.rect;
        ti.hinst = hinst;
        ti.lpszText = text.as_mut_ptr();
        SendMessageW(tip, TTM_ADDTOOLW, 0, &ti as *const _ as isize);
    }
}

/// Repaints the toolbar in new colors. Leaves the dropdowns alone, so it is safe to call while
/// one is open (theme previews do).
pub unsafe fn set_colors(colors: Colors) {
    tb(|t| {
        t.colors.set(colors);
        let old = t.list_brush.replace(CreateSolidBrush(colors.bg));
        if !old.is_null() {
            DeleteObject(old as HGDIOBJ);
        }
    });
    InvalidateRect(tb(|t| t.hwnd.get()), null(), 0);
}

/// Brings the dropdowns, toggles and colors up to date with the app.
pub unsafe fn sync(state: &State) {
    let is_dark = themes::is_dark(state.colors.bg);
    set_colors(state.colors);
    let [font_combo, size_combo, theme_combo] = tb(|t| {
        t.is_wrap.set(state.is_wrap);
        t.custom.set(state.custom);
        t.combos.get()
    });
    fill_font_combo(font_combo, state);
    fill_size_combo(size_combo, state.size_tenths);
    fill_theme_combo(theme_combo, state.theme);
    let combo_theme = if is_dark {
        w!("DarkMode_CFD")
    } else {
        w!("CFD")
    };
    for combo in [font_combo, size_combo, theme_combo] {
        SetWindowTheme(combo, combo_theme, null());
        InvalidateRect(combo, null(), 1);
    }
}

unsafe fn fill_font_combo(combo: HWND, state: &State) {
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    let mut cmds = Vec::new();
    for (i, name) in state.font_list.iter().enumerate() {
        add_string(combo, name);
        cmds.push(IDM_FONT_BASE + i as u16);
    }
    let listed = state
        .font_list
        .iter()
        .position(|n| n.eq_ignore_ascii_case(state.face));
    let selected = listed.unwrap_or_else(|| {
        add_string(combo, state.face);
        cmds.push(0);
        cmds.len() - 1
    });
    add_string(combo, MORE_FONTS);
    cmds.push(IDM_FONT_DIALOG);
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
    tb(|t| *t.font_cmds.borrow_mut() = cmds);
}

unsafe fn fill_size_combo(combo: HWND, size_tenths: u32) {
    let mut sizes: Vec<u32> = SIZES_PT.iter().map(|pt| pt * 10).collect();
    if !sizes.contains(&size_tenths) {
        sizes.push(size_tenths);
        sizes.sort_unstable();
    }
    SendMessageW(combo, CB_RESETCONTENT, 0, 0);
    for &t in &sizes {
        let label = if t % 10 == 0 {
            format!("{}", t / 10)
        } else {
            format!("{}.{}", t / 10, t % 10)
        };
        add_string(combo, &label);
    }
    let selected = sizes.iter().position(|&t| t == size_tenths).unwrap_or(0);
    SendMessageW(combo, CB_SETCURSEL, selected, 0);
    tb(|t| *t.sizes.borrow_mut() = sizes);
}

unsafe fn fill_theme_combo(combo: HWND, theme: usize) {
    if SendMessageW(combo, CB_GETCOUNT, 0, 0) == 0 {
        for preset in themes::PRESETS {
            add_string(combo, preset.name);
        }
        add_string(combo, themes::CUSTOM);
    }
    SendMessageW(combo, CB_SETCURSEL, theme, 0);
}

unsafe fn add_string(combo: HWND, text: &str) {
    let text = wide(text);
    SendMessageW(combo, CB_ADDSTRING, 0, text.as_ptr() as isize);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd, lparam);
            0
        }
        WM_MOUSELEAVE => {
            tb(|t| {
                t.is_tracking.set(false);
                t.hot.set(None);
            });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONDOWN => {
            let hit = hit_test(lparam);
            tb(|t| t.pressed.set(hit));
            if hit.is_some() {
                SetCapture(hwnd);
            }
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            on_button_up(hwnd, lparam);
            0
        }
        WM_CAPTURECHANGED => {
            tb(|t| t.pressed.set(None));
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_COMMAND => {
            on_combo(hwnd, wparam, lparam);
            0
        }
        WM_MEASUREITEM => {
            let mis = &mut *(lparam as *mut MEASUREITEMSTRUCT);
            mis.itemHeight = if mis.itemID == u32::MAX {
                s(THEME_FIELD_H)
            } else {
                s(THEME_ROW_H)
            } as u32;
            1
        }
        WM_DRAWITEM => {
            draw_theme_item(hwnd, &*(lparam as *const DRAWITEMSTRUCT));
            1
        }
        WM_CTLCOLORLISTBOX if themes::is_dark(tb(|t| t.colors.get()).bg) => {
            let colors = tb(|t| t.colors.get());
            SetTextColor(wparam as HDC, colors.fg);
            SetBkColor(wparam as HDC, colors.bg);
            tb(|t| t.list_brush.get()) as LRESULT
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn point(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam & 0xFFFF) as i16 as i32,
        y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
    }
}

fn hit_test(lparam: LPARAM) -> Option<usize> {
    let p = point(lparam);
    tb(|t| {
        t.buttons.borrow().iter().position(|b| {
            p.x >= b.rect.left && p.x < b.rect.right && p.y >= b.rect.top && p.y < b.rect.bottom
        })
    })
}

unsafe fn on_mouse_move(hwnd: HWND, lparam: LPARAM) {
    let hit = hit_test(lparam);
    let (changed, is_tracking) = tb(|t| (t.hot.replace(hit) != hit, t.is_tracking.replace(true)));
    if !is_tracking {
        let mut tme = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        TrackMouseEvent(&mut tme);
    }
    if changed {
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn on_button_up(hwnd: HWND, lparam: LPARAM) {
    let Some(pressed) = tb(|t| t.pressed.get()) else {
        return;
    };
    let hit = hit_test(lparam);
    ReleaseCapture();
    if hit == Some(pressed) {
        let id = tb(|t| t.buttons.borrow()[pressed].id);
        PostMessageW(GetParent(hwnd), WM_COMMAND, id as usize, 0);
    }
}

unsafe fn on_combo(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) {
    let code = ((wparam >> 16) & 0xFFFF) as u32;
    let combo = lparam as HWND;
    if wparam & 0xFFFF == ID_THEME_COMBO {
        on_theme_combo(GetParent(hwnd), combo, code);
        return;
    }
    if code != CBN_SELCHANGE {
        return;
    }
    let sel = SendMessageW(combo, CB_GETCURSEL, 0, 0);
    if sel < 0 {
        return;
    }
    let sel = sel as usize;
    let parent = GetParent(hwnd);
    match wparam & 0xFFFF {
        ID_FONT_COMBO => {
            let cmd = tb(|t| t.font_cmds.borrow().get(sel).copied().unwrap_or(0));
            if cmd != 0 {
                PostMessageW(parent, WM_COMMAND, cmd as usize, 0);
            }
        }
        ID_SIZE_COMBO => {
            if let Some(size) = tb(|t| t.sizes.borrow().get(sel).copied()) {
                PostMessageW(parent, WM_APP_SET_SIZE, size as usize, 0);
            }
        }
        _ => {}
    }
}

/// While the list is open, moving through it previews each theme; choosing one commits it, and
/// closing the list any other way goes back to the chosen theme.
unsafe fn on_theme_combo(main: HWND, combo: HWND, code: u32) {
    let sel = SendMessageW(combo, CB_GETCURSEL, 0, 0);
    let is_dropped = SendMessageW(combo, CB_GETDROPPEDSTATE, 0, 0) != 0;
    match code {
        CBN_SELCHANGE if sel >= 0 && is_dropped => preview(main, sel as usize),
        CBN_SELCHANGE | CBN_SELENDOK if sel >= 0 => {
            PostMessageW(main, WM_COMMAND, theme_id(sel as usize) as usize, 0);
        }
        CBN_SELENDCANCEL | CBN_CLOSEUP => {
            tb(|t| t.hover_theme.set(None));
            PostMessageW(main, WM_APP_PREVIEW_THEME, END_PREVIEW, 0);
        }
        _ => {}
    }
}

/// Shows the chosen theme in the dropdown again after browsing without choosing.
pub unsafe fn select_theme(index: usize) {
    let combo = tb(|t| t.combos.get())[Combo::Theme as usize];
    if SendMessageW(combo, CB_GETDROPPEDSTATE, 0, 0) == 0 {
        SendMessageW(combo, CB_SETCURSEL, index, 0);
    }
}

unsafe fn preview(main: HWND, index: usize) {
    if tb(|t| t.hover_theme.replace(Some(index))) != Some(index) {
        PostMessageW(main, WM_APP_PREVIEW_THEME, index, 0);
    }
}

/// Draws one row of the theme dropdown, or its closed field, in that theme's own colors.
/// The row under the pointer gets a frame and is previewed in the whole window.
unsafe fn draw_theme_item(toolbar: HWND, dis: &DRAWITEMSTRUCT) {
    if dis.itemID == u32::MAX {
        return;
    }
    let index = dis.itemID as usize;
    let colors = themes::colors_of(index, tb(|t| t.custom.get()));
    let is_field = dis.itemState & ODS_COMBOBOXEDIT != 0;
    let is_hot = !is_field && dis.itemState & ODS_SELECTED != 0;
    let rc = dis.rcItem;
    fill(dis.hDC, &rc, colors.bg);
    if is_hot {
        draw_frame(dis.hDC, &rc, colors.fg, s(HOVER_FRAME));
    }
    SetBkMode(dis.hDC, TRANSPARENT as i32);
    SetTextColor(dis.hDC, colors.fg);
    let mut text = RECT {
        left: rc.left + s(THEME_TEXT_PAD),
        ..rc
    };
    let name: Vec<u16> = themes::name_of(index).encode_utf16().collect();
    draw_text(
        dis.hDC,
        tb(|t| t.ui_font.get()),
        &name,
        &mut text,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    let is_dropped = SendMessageW(dis.hwndItem, CB_GETDROPPEDSTATE, 0, 0) != 0;
    if is_hot && is_dropped {
        preview(GetParent(toolbar), index);
    }
}

unsafe fn draw_frame(hdc: HDC, rc: &RECT, color: u32, width: i32) {
    let edges = [
        RECT {
            bottom: rc.top + width,
            ..*rc
        },
        RECT {
            top: rc.bottom - width,
            ..*rc
        },
        RECT {
            right: rc.left + width,
            ..*rc
        },
        RECT {
            left: rc.right - width,
            ..*rc
        },
    ];
    for edge in &edges {
        fill(hdc, edge, color);
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let mem = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
    let old_bmp = SelectObject(mem, bmp as HGDIOBJ);

    let colors = tb(|t| t.colors.get());
    fill(mem, &rc, themes::mix(colors.bg, colors.fg, BG_MIX));
    SetBkMode(mem, TRANSPARENT as i32);
    let count = tb(|t| t.buttons.borrow().len());
    for i in 0..count {
        draw_button(mem, i, colors);
    }
    draw_groups(mem, &rc, colors);
    let line = RECT {
        left: 0,
        top: rc.bottom - 1,
        right: rc.right,
        bottom: rc.bottom,
    };
    fill(mem, &line, themes::mix(colors.bg, colors.fg, LINE_MIX));

    BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old_bmp);
    DeleteObject(bmp as HGDIOBJ);
    DeleteDC(mem);
    EndPaint(hwnd, &ps);
}

unsafe fn draw_button(hdc: HDC, index: usize, colors: Colors) {
    let (rect, id, glyph, label, kind, hot, pressed, is_wrap) = tb(|t| {
        let b = &t.buttons.borrow()[index];
        (
            b.rect,
            b.id,
            b.glyph,
            b.label,
            b.kind,
            t.hot.get(),
            t.pressed.get(),
            t.is_wrap.get(),
        )
    });
    let is_hot = hot == Some(index);
    let is_down = is_hot && pressed == Some(index);
    let is_on = id == IDM_WRAP && is_wrap;
    if is_down || is_hot || is_on {
        let mix = if is_down { DOWN_MIX } else { HOT_MIX };
        round_fill(hdc, &rect, themes::mix(colors.bg, colors.fg, mix));
    }
    let is_dark = themes::is_dark(colors.bg);
    let (glyph, label) = match id {
        IDM_TOGGLE_DARK if is_dark => (GLYPH_SUN, "Light"),
        IDM_TOGGLE_DARK => (GLYPH_MOON, "Dark"),
        _ => (glyph, label),
    };
    SetTextColor(hdc, colors.fg);
    let (icon_font, small_icon_font, ui_font) =
        tb(|t| (t.icon_font.get(), t.small_icon_font.get(), t.ui_font.get()));
    let glyph_text = [glyph];
    let flags = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
    if kind == Kind::Large {
        let mut icon = RECT {
            left: rect.left,
            top: rect.top + s(6),
            right: rect.right,
            bottom: rect.top + s(34),
        };
        draw_text(hdc, icon_font, &glyph_text, &mut icon, flags);
        let mut text = RECT {
            left: rect.left,
            top: rect.top + s(36),
            right: rect.right,
            bottom: rect.bottom - s(4),
        };
        draw_text(
            hdc,
            ui_font,
            &label.encode_utf16().collect::<Vec<_>>(),
            &mut text,
            flags,
        );
        return;
    }
    let mut icon = RECT {
        left: rect.left,
        top: rect.top,
        right: rect.left + s(SMALL_ICON_W),
        bottom: rect.bottom,
    };
    draw_text(hdc, small_icon_font, &glyph_text, &mut icon, flags);
    draw_swatch(hdc, id, &icon, colors);
    if !label.is_empty() {
        let mut text = RECT {
            left: icon.right - s(2),
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        draw_text(
            hdc,
            ui_font,
            &label.encode_utf16().collect::<Vec<_>>(),
            &mut text,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

/// The color buttons show the current text or background color under the icon.
unsafe fn draw_swatch(hdc: HDC, id: u16, icon: &RECT, colors: Colors) {
    let color = match id {
        IDM_TEXT_COLOR => colors.fg,
        IDM_BACK_COLOR => colors.bg,
        _ => return,
    };
    let bar = RECT {
        left: icon.left + s(7),
        top: icon.bottom - s(5),
        right: icon.right - s(7),
        bottom: icon.bottom - s(5) + s(SWATCH_H),
    };
    let border = RECT {
        left: bar.left - 1,
        top: bar.top - 1,
        right: bar.right + 1,
        bottom: bar.bottom + 1,
    };
    fill(hdc, &border, themes::mix(colors.bg, colors.fg, DIM_MIX));
    fill(hdc, &bar, color);
}

unsafe fn draw_groups(hdc: HDC, rc: &RECT, colors: Colors) {
    let ui_font = tb(|t| t.ui_font.get());
    let dim = themes::mix(colors.bg, colors.fg, DIM_MIX);
    let line = themes::mix(colors.bg, colors.fg, LINE_MIX);
    SetTextColor(hdc, dim);
    let groups = tb(|t| {
        t.groups
            .borrow()
            .iter()
            .map(|g| (g.caption, g.left, g.right))
            .collect::<Vec<_>>()
    });
    let last = groups.len().saturating_sub(1);
    for (i, (caption, left, right)) in groups.into_iter().enumerate() {
        let mut text = RECT {
            left,
            top: rc.bottom - s(CAPTION_H) - s(2),
            right,
            bottom: rc.bottom - s(2),
        };
        draw_text(
            hdc,
            ui_font,
            &caption.encode_utf16().collect::<Vec<_>>(),
            &mut text,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if i < last {
            let x = right + s(GROUP_GAP) / 2;
            fill(
                hdc,
                &RECT {
                    left: x,
                    top: s(6),
                    right: x + 1,
                    bottom: rc.bottom - s(6),
                },
                line,
            );
        }
    }
}

unsafe fn draw_text(hdc: HDC, font: HFONT, text: &[u16], rect: &mut RECT, flags: u32) {
    let old = SelectObject(hdc, font as HGDIOBJ);
    DrawTextW(hdc, text.as_ptr(), text.len() as i32, rect, flags);
    SelectObject(hdc, old);
}

unsafe fn fill(hdc: HDC, rect: &RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, rect, brush);
    DeleteObject(brush as HGDIOBJ);
}

unsafe fn round_fill(hdc: HDC, rect: &RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(hdc, brush as HGDIOBJ);
    let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        s(CORNER),
        s(CORNER),
    );
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(brush as HGDIOBJ);
}
