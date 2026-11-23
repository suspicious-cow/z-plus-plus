//! The tab strip between the toolbar and the editor: one tab per open file.
//!
//! Clicking a tab selects and shows it; Ctrl+click adds or removes a tab from the selection,
//! Shift+click selects a range. Dragging moves the selected tabs (or the one dragged) to where
//! the insertion bar is; Escape cancels. Right-click offers Close, Close Others, and Close Tabs to
//! the Right. A tab's x (or a middle click) closes it, and a double click on the empty part of the
//! strip opens a new tab. Actions are posted to the main window as WM_APP_TAB with the action in
//! wparam and a tab index or insertion point in lparam; `selected` gives the selected tabs.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::gdi::{self, s};
use crate::themes::{self, Colors};
use crate::wide;

pub const WM_APP_TAB: u32 = WM_APP + 3;
pub const TAB_SELECT: usize = 0;
pub const TAB_CLOSE: usize = 1;
pub const TAB_NEW: usize = 2;
/// Move the selected tabs to insertion point lparam (0 = before the first tab).
pub const TAB_MOVE: usize = 3;
pub const TAB_CLOSE_SELECTED: usize = 4;
pub const TAB_CLOSE_OTHERS: usize = 5;
/// Close the tabs right of tab lparam.
pub const TAB_CLOSE_RIGHT: usize = 6;
/// For GUI checks: wparam is a tab index; returns its left and right edges as MAKELONG.
pub const WM_APP_TAB_RECT: u32 = WM_APP + 30;
/// For GUI checks (ZPP_TEST): the menu the last right-click built, as an HMENU; WM_COMMAND
/// with one of its ids then runs that item.
pub const WM_APP_TAB_MENU: u32 = WM_APP + 31;

const CLASS: *const u16 = w!("ZppTabs");
const HEIGHT: i32 = 30;
const TAB_PAD: i32 = 12;
const CLOSE_W: i32 = 22;
const MIN_TAB_W: i32 = 70;
const MAX_TAB_W: i32 = 240;
const TAB_GAP: i32 = 1;
const TOP_MARGIN: i32 = 4;
const ACCENT_H: i32 = 2;
const INSERT_W: i32 = 2;
const STRIP_MIX: u32 = 6;
const HOVER_MIX: u32 = 12;
const SELECTED_MIX: u32 = 16;
const SELECTED_ACCENT_MIX: u32 = 35;
const DIM_MIX: u32 = 55;
const ACCENT_MIX: u32 = 70;
const CLOSE_GLYPH: &str = "\u{00D7}";
const MODIFIED_GLYPH: &str = "\u{25CF}";

/// Key flags in a mouse message's wparam.
const MK_SHIFT: usize = 0x0004;
const MK_CONTROL: usize = 0x0008;

const MENU_CLOSE: usize = 1;
const MENU_CLOSE_OTHERS: usize = 2;
const MENU_CLOSE_RIGHT: usize = 3;

pub struct Tab {
    pub label: String,
    pub is_modified: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct Hit {
    index: usize,
    is_close: bool,
}

/// A mouse press on a tab that may become a drag.
#[derive(Clone, Copy)]
struct Press {
    index: usize,
    x: i32,
    y: i32,
    /// Plain click on a tab already in a multi-selection: on release without a drag, it
    /// becomes the only selected tab.
    is_narrow_on_release: bool,
    /// Insertion point while dragging.
    gap: Option<usize>,
}

struct Strip {
    hwnd: Cell<HWND>,
    tabs: RefCell<Vec<Tab>>,
    rects: RefCell<Vec<RECT>>,
    active: Cell<usize>,
    selected: RefCell<Vec<bool>>,
    anchor: Cell<usize>,
    hot: Cell<Option<Hit>>,
    is_tracking: Cell<bool>,
    press: Cell<Option<Press>>,
    colors: Cell<Colors>,
}

thread_local! {
    static TEST_MENU: Cell<Option<(HMENU, usize)>> = const { Cell::new(None) };
    static STRIP: Strip = const { Strip {
        hwnd: Cell::new(null_mut()),
        tabs: RefCell::new(Vec::new()),
        rects: RefCell::new(Vec::new()),
        active: Cell::new(0),
        selected: RefCell::new(Vec::new()),
        anchor: Cell::new(0),
        hot: Cell::new(None),
        is_tracking: Cell::new(false),
        press: Cell::new(None),
        colors: Cell::new(themes::PRESETS[0].colors),
    } };
}

fn strip<R>(f: impl FnOnce(&Strip) -> R) -> R {
    STRIP.with(f)
}

pub fn height() -> i32 {
    s(HEIGHT)
}

pub unsafe fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_DBLCLKS,
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
        WS_CHILD | WS_VISIBLE,
        0,
        0,
        0,
        0,
        parent,
        null_mut(),
        hinst,
        null(),
    );
    strip(|t| t.hwnd.set(hwnd));
    hwnd
}

/// Replaces the tab list and repaints. The selection is kept while the tab count stays the
/// same and the shown tab is in it; otherwise only the shown tab is selected.
pub unsafe fn set(tabs: Vec<Tab>, active: usize) {
    strip(|t| {
        let count = tabs.len();
        *t.tabs.borrow_mut() = tabs;
        t.active.set(active);
        let mut selected = t.selected.borrow_mut();
        if selected.len() != count || !selected.get(active).copied().unwrap_or(false) {
            *selected = (0..count).map(|i| i == active).collect();
            t.anchor.set(active);
        }
    });
    relayout();
}

/// The selected tabs, left to right (always including the shown one).
pub fn selected() -> Vec<usize> {
    strip(|t| {
        let selected = t.selected.borrow();
        let mut list: Vec<usize> = (0..selected.len()).filter(|&i| selected[i]).collect();
        if list.is_empty() && t.active.get() < selected.len() {
            list.push(t.active.get());
        }
        list
    })
}

/// After tabs moved: which tabs are selected, by their new positions.
pub unsafe fn set_selected(indices: &[usize]) {
    strip(|t| {
        let mut selected = t.selected.borrow_mut();
        for (i, flag) in selected.iter_mut().enumerate() {
            *flag = indices.contains(&i);
        }
    });
    InvalidateRect(strip(|t| t.hwnd.get()), null(), 0);
}

pub unsafe fn set_colors(colors: Colors) {
    strip(|t| t.colors.set(colors));
    InvalidateRect(strip(|t| t.hwnd.get()), null(), 0);
}

/// Tab widths follow their labels, and shrink evenly when they do not all fit.
unsafe fn relayout() {
    let hwnd = strip(|t| t.hwnd.get());
    let mut client: RECT = zeroed();
    GetClientRect(hwnd, &mut client);
    let font = gdi::ui_font();
    let widths: Vec<i32> = strip(|t| {
        t.tabs
            .borrow()
            .iter()
            .map(|tab| {
                (gdi::measure(font, &tab.label) + s(TAB_PAD) * 2 + s(CLOSE_W))
                    .clamp(s(MIN_TAB_W), s(MAX_TAB_W))
            })
            .collect()
    });
    let total: i32 = widths.iter().sum::<i32>() + s(TAB_GAP) * widths.len() as i32;
    let available = client.right.max(1);
    let shrink = |w: i32| {
        if total <= available {
            w
        } else {
            (w * available / total).max(s(CLOSE_W) + s(TAB_PAD))
        }
    };
    let mut x = 0;
    let rects = widths
        .into_iter()
        .map(|w| {
            let w = shrink(w);
            let rc = RECT {
                left: x,
                top: s(TOP_MARGIN),
                right: x + w,
                bottom: client.bottom,
            };
            x += w + s(TAB_GAP);
            rc
        })
        .collect();
    strip(|t| *t.rects.borrow_mut() = rects);
    InvalidateRect(hwnd, null(), 0);
}

fn close_rect(tab: &RECT) -> RECT {
    RECT {
        left: tab.right - s(CLOSE_W),
        ..*tab
    }
}

fn point(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam & 0xFFFF) as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as i16 as i32,
    )
}

fn hit_test(lparam: LPARAM) -> Option<Hit> {
    let (x, y) = point(lparam);
    let inside = |rc: &RECT| x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom;
    strip(|t| {
        let rects = t.rects.borrow();
        let index = rects.iter().position(inside)?;
        Some(Hit {
            index,
            is_close: inside(&close_rect(&rects[index])),
        })
    })
}

/// The insertion point nearest `x`: how many tabs have their middle left of it.
fn gap_at(x: i32) -> usize {
    strip(|t| {
        t.rects
            .borrow()
            .iter()
            .filter(|rc| (rc.left + rc.right) / 2 < x)
            .count()
    })
}

unsafe fn post(action: usize, index: usize) {
    let hwnd = strip(|t| t.hwnd.get());
    PostMessageW(GetParent(hwnd), WM_APP_TAB, action, index as isize);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_SIZE => {
            relayout();
            0
        }
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd, lparam);
            0
        }
        WM_MOUSELEAVE => {
            strip(|t| {
                t.is_tracking.set(false);
                t.hot.set(None);
            });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONDOWN => {
            on_button_down(hwnd, wparam, lparam);
            0
        }
        WM_LBUTTONUP => {
            on_button_up(hwnd);
            0
        }
        WM_CAPTURECHANGED => {
            cancel_drag(hwnd);
            0
        }
        WM_RBUTTONUP => {
            on_right_click(hwnd, lparam);
            0
        }
        WM_MBUTTONUP => {
            if let Some(hit) = hit_test(lparam) {
                post(TAB_CLOSE, hit.index);
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            if hit_test(lparam).is_none() {
                post(TAB_NEW, 0);
            }
            0
        }
        WM_APP_TAB_RECT => strip(|t| {
            t.rects.borrow().get(wparam).map_or(-1, |rc| {
                ((rc.left & 0xFFFF) | ((rc.right & 0xFFFF) << 16)) as LRESULT
            })
        }),
        WM_APP_TAB_MENU => TEST_MENU
            .with(|m| m.get())
            .map_or(0, |(menu, _)| menu as LRESULT),
        WM_COMMAND => {
            if let Some((_, index)) = TEST_MENU.with(|m| m.get()) {
                run_menu_item(wparam & 0xFFFF, index);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn on_button_down(hwnd: HWND, keys: WPARAM, lparam: LPARAM) {
    let Some(hit) = hit_test(lparam) else {
        return;
    };
    if hit.is_close {
        post(TAB_CLOSE, hit.index);
        return;
    }
    let is_ctrl = keys & MK_CONTROL != 0;
    let is_shift = keys & MK_SHIFT != 0;
    let index = hit.index;
    let mut is_narrow_on_release = false;
    let show = strip(|t| {
        let mut selected = t.selected.borrow_mut();
        if index >= selected.len() {
            return None;
        }
        if is_shift {
            let (a, b) = (t.anchor.get().min(index), t.anchor.get().max(index));
            for (i, flag) in selected.iter_mut().enumerate() {
                *flag = (a..=b).contains(&i) || (is_ctrl && *flag);
            }
            return Some(index);
        }
        t.anchor.set(index);
        if is_ctrl {
            if selected[index] && selected.iter().filter(|&&f| f).count() > 1 {
                // Taking the shown tab out shows another selected one.
                selected[index] = false;
                let next = (0..selected.len()).find(|&i| selected[i]);
                return if index == t.active.get() { next } else { None };
            }
            selected[index] = true;
            return Some(index);
        }
        let count = selected.iter().filter(|&&f| f).count();
        if selected[index] && count > 1 {
            is_narrow_on_release = true;
        } else {
            for (i, flag) in selected.iter_mut().enumerate() {
                *flag = i == index;
            }
        }
        Some(index)
    });
    let (x, y) = point(lparam);
    strip(|t| {
        t.press.set(Some(Press {
            index,
            x,
            y,
            is_narrow_on_release,
            gap: None,
        }))
    });
    SetCapture(hwnd);
    InvalidateRect(hwnd, null(), 0);
    if let Some(i) = show {
        post(TAB_SELECT, i);
    }
}

unsafe fn on_mouse_move(hwnd: HWND, lparam: LPARAM) {
    if strip(|t| t.press.get()).is_some() {
        on_drag(hwnd, lparam);
        return;
    }
    let hit = hit_test(lparam);
    let (changed, is_tracking) =
        strip(|t| (t.hot.replace(hit) != hit, t.is_tracking.replace(true)));
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

unsafe fn on_drag(hwnd: HWND, lparam: LPARAM) {
    let Some(mut press) = strip(|t| t.press.get()) else {
        return;
    };
    if GetAsyncKeyState(VK_ESCAPE as i32) < 0 {
        ReleaseCapture();
        return;
    }
    let (x, y) = point(lparam);
    let is_far = (x - press.x).abs() >= GetSystemMetrics(SM_CXDRAG)
        || (y - press.y).abs() >= GetSystemMetrics(SM_CYDRAG);
    if press.gap.is_none() && !is_far {
        return;
    }
    if press.gap.is_none() {
        // Dragging a tab outside the selection drags just that tab.
        let is_selected = strip(|t| {
            t.selected
                .borrow()
                .get(press.index)
                .copied()
                .unwrap_or(false)
        });
        if !is_selected {
            set_selected(&[press.index]);
        }
    }
    let gap = Some(gap_at(x));
    if press.gap != gap {
        press.gap = gap;
        strip(|t| t.press.set(Some(press)));
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn on_button_up(hwnd: HWND) {
    let Some(press) = strip(|t| t.press.take()) else {
        return;
    };
    ReleaseCapture();
    match press.gap {
        Some(gap) => post(TAB_MOVE, gap),
        None if press.is_narrow_on_release => {
            set_selected(&[press.index]);
            strip(|t| t.anchor.set(press.index));
        }
        None => {}
    }
    InvalidateRect(hwnd, null(), 0);
}

unsafe fn cancel_drag(hwnd: HWND) {
    if strip(|t| t.press.take()).is_some() {
        InvalidateRect(hwnd, null(), 0);
    }
}

unsafe fn on_right_click(hwnd: HWND, lparam: LPARAM) {
    let Some(hit) = hit_test(lparam) else {
        return;
    };
    let index = hit.index;
    let is_selected = strip(|t| t.selected.borrow().get(index).copied().unwrap_or(false));
    if !is_selected {
        set_selected(&[index]);
        strip(|t| t.anchor.set(index));
        post(TAB_SELECT, index);
    }
    let menu = build_menu(index);
    if crate::profile::is_test() {
        // A popup menu takes the keyboard; a GUI check reads it with WM_APP_TAB_MENU instead.
        let old = TEST_MENU.with(|m| m.replace(Some((menu, index))));
        if let Some((old, _)) = old {
            DestroyMenu(old);
        }
        return;
    }
    let (x, y) = point(lparam);
    let mut pt = POINT { x, y };
    ClientToScreen(hwnd, &mut pt);
    let flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY;
    let id = TrackPopupMenu(menu, flags, pt.x, pt.y, 0, GetParent(hwnd), null()) as usize;
    DestroyMenu(menu);
    run_menu_item(id, index);
}

/// Close (or Close N Tabs), Close Others, Close Tabs to the Right, for a right-click on `index`.
unsafe fn build_menu(index: usize) -> HMENU {
    let count = strip(|t| t.tabs.borrow().len());
    let chosen = selected().len();
    let close = if chosen > 1 {
        format!("Close {chosen} Tabs")
    } else {
        "Close".to_string()
    };
    let menu = CreatePopupMenu();
    let items = [
        (MENU_CLOSE, close.as_str(), true),
        (MENU_CLOSE_OTHERS, "Close Others", chosen < count),
        (
            MENU_CLOSE_RIGHT,
            "Close Tabs to the Right",
            index + 1 < count,
        ),
    ];
    for (id, label, is_enabled) in items {
        let text = wide(label);
        let flags = if is_enabled {
            MF_STRING
        } else {
            MF_STRING | MF_GRAYED
        };
        AppendMenuW(menu, flags, id, text.as_ptr());
    }
    menu
}

unsafe fn run_menu_item(id: usize, index: usize) {
    match id {
        MENU_CLOSE => post(TAB_CLOSE_SELECTED, index),
        MENU_CLOSE_OTHERS => post(TAB_CLOSE_OTHERS, index),
        MENU_CLOSE_RIGHT => post(TAB_CLOSE_RIGHT, index),
        _ => {}
    }
}
unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    let colors = strip(|t| t.colors.get());
    gdi::buffered(hdc, &rc, |mem| {
        gdi::fill(mem, &rc, themes::mix(colors.bg, colors.fg, STRIP_MIX));
        let count = strip(|t| t.tabs.borrow().len());
        for i in 0..count {
            draw_tab(mem, i, colors);
        }
        draw_insertion(mem, &rc, colors);
    });
    EndPaint(hwnd, &ps);
}

/// The bar showing where dragged tabs will land.
unsafe fn draw_insertion(hdc: HDC, client: &RECT, colors: Colors) {
    let Some(gap) = strip(|t| t.press.get()).and_then(|p| p.gap) else {
        return;
    };
    let x = strip(|t| {
        let rects = t.rects.borrow();
        match rects.get(gap) {
            Some(rc) => rc.left - s(TAB_GAP),
            None => rects.last().map_or(0, |rc| rc.right),
        }
    });
    let bar = RECT {
        left: (x - s(INSERT_W) / 2).max(0),
        top: s(TOP_MARGIN),
        right: (x - s(INSERT_W) / 2).max(0) + s(INSERT_W),
        bottom: client.bottom,
    };
    gdi::fill(hdc, &bar, colors.fg);
}

unsafe fn draw_tab(hdc: HDC, index: usize, colors: Colors) {
    let (rect, label, is_modified, is_active, is_selected, hot) = strip(|t| {
        let tab = &t.tabs.borrow()[index];
        (
            t.rects.borrow()[index],
            tab.label.clone(),
            tab.is_modified,
            t.active.get() == index,
            t.selected.borrow().get(index).copied().unwrap_or(false),
            t.hot.get(),
        )
    });
    let is_hot = hot.is_some_and(|h| h.index == index);
    let background = if is_active {
        colors.bg
    } else if is_selected {
        themes::mix(colors.bg, colors.fg, SELECTED_MIX)
    } else if is_hot {
        themes::mix(colors.bg, colors.fg, HOVER_MIX)
    } else {
        themes::mix(colors.bg, colors.fg, STRIP_MIX)
    };
    gdi::fill(hdc, &rect, background);
    if is_active || is_selected {
        let accent = RECT {
            bottom: rect.top + s(ACCENT_H),
            ..rect
        };
        let mix = if is_active {
            ACCENT_MIX
        } else {
            SELECTED_ACCENT_MIX
        };
        gdi::fill(hdc, &accent, themes::mix(colors.bg, colors.fg, mix));
    }
    let text_color = if is_active || is_selected {
        colors.fg
    } else {
        themes::mix(colors.bg, colors.fg, DIM_MIX + 25)
    };
    SetTextColor(hdc, text_color);
    let font = gdi::ui_font();
    let close = close_rect(&rect);
    let mut text = RECT {
        left: rect.left + s(TAB_PAD),
        right: close.left,
        ..rect
    };
    gdi::draw_str(
        hdc,
        font,
        &label,
        &mut text,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    let glyph = if is_hot {
        Some(CLOSE_GLYPH)
    } else if is_modified {
        Some(MODIFIED_GLYPH)
    } else {
        is_active.then_some(CLOSE_GLYPH)
    };
    draw_close(
        hdc,
        close,
        glyph,
        hot == Some(Hit {
            index,
            is_close: true,
        }),
        colors,
    );
}

/// The x, or for a modified tab a dot until the pointer is over the tab.
unsafe fn draw_close(hdc: HDC, rc: RECT, glyph: Option<&str>, is_hot: bool, colors: Colors) {
    let Some(glyph) = glyph else {
        return;
    };
    if is_hot {
        let pad = s(4);
        let spot = RECT {
            left: rc.left + pad / 2,
            top: rc.top + pad,
            right: rc.right - pad,
            bottom: rc.bottom - pad,
        };
        gdi::round_fill(
            hdc,
            &spot,
            themes::mix(colors.bg, colors.fg, HOVER_MIX + 8),
            s(4),
        );
    }
    SetTextColor(
        hdc,
        themes::mix(
            colors.bg,
            colors.fg,
            if is_hot { 100 } else { DIM_MIX + 20 },
        ),
    );
    let mut r = rc;
    gdi::draw_str(
        hdc,
        gdi::ui_font(),
        glyph,
        &mut r,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}
