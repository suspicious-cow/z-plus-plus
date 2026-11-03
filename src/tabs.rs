//! The tab strip between the toolbar and the editor: one tab per open file.
//! Clicking a tab selects it, its x (or a middle click) closes it, and a double click on the
//! empty part of the strip opens a new tab. Actions are posted to the main window as
//! WM_APP_TAB with the action in wparam and the tab index in lparam.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::gdi::{self, s};
use crate::themes::{self, Colors};

pub const WM_APP_TAB: u32 = WM_APP + 3;
pub const TAB_SELECT: usize = 0;
pub const TAB_CLOSE: usize = 1;
pub const TAB_NEW: usize = 2;

const CLASS: *const u16 = w!("ZppTabs");
const HEIGHT: i32 = 30;
const TAB_PAD: i32 = 12;
const CLOSE_W: i32 = 22;
const MIN_TAB_W: i32 = 70;
const MAX_TAB_W: i32 = 240;
const TAB_GAP: i32 = 1;
const TOP_MARGIN: i32 = 4;
const ACCENT_H: i32 = 2;
const STRIP_MIX: u32 = 6;
const HOVER_MIX: u32 = 12;
const DIM_MIX: u32 = 55;
const ACCENT_MIX: u32 = 70;
const CLOSE_GLYPH: &str = "\u{00D7}";
const MODIFIED_GLYPH: &str = "\u{25CF}";

pub struct Tab {
    pub label: String,
    pub is_modified: bool,
}

#[derive(Clone, Copy, PartialEq)]
struct Hit {
    index: usize,
    is_close: bool,
}

struct Strip {
    hwnd: Cell<HWND>,
    tabs: RefCell<Vec<Tab>>,
    rects: RefCell<Vec<RECT>>,
    active: Cell<usize>,
    hot: Cell<Option<Hit>>,
    is_tracking: Cell<bool>,
    colors: Cell<Colors>,
}

thread_local! {
    static STRIP: Strip = const { Strip {
        hwnd: Cell::new(null_mut()),
        tabs: RefCell::new(Vec::new()),
        rects: RefCell::new(Vec::new()),
        active: Cell::new(0),
        hot: Cell::new(None),
        is_tracking: Cell::new(false),
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

/// Replaces the tab list and repaints.
pub unsafe fn set(tabs: Vec<Tab>, active: usize) {
    strip(|t| {
        *t.tabs.borrow_mut() = tabs;
        t.active.set(active);
    });
    relayout();
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

fn hit_test(lparam: LPARAM) -> Option<Hit> {
    let (x, y) = (
        (lparam & 0xFFFF) as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as i16 as i32,
    );
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
            match hit_test(lparam) {
                Some(Hit {
                    index,
                    is_close: true,
                }) => post(TAB_CLOSE, index),
                Some(Hit { index, .. }) => post(TAB_SELECT, index),
                None => {}
            }
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
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn on_mouse_move(hwnd: HWND, lparam: LPARAM) {
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
    });
    EndPaint(hwnd, &ps);
}

unsafe fn draw_tab(hdc: HDC, index: usize, colors: Colors) {
    let (rect, label, is_modified, is_active, hot) = strip(|t| {
        let tab = &t.tabs.borrow()[index];
        (
            t.rects.borrow()[index],
            tab.label.clone(),
            tab.is_modified,
            t.active.get() == index,
            t.hot.get(),
        )
    });
    let is_hot = hot.is_some_and(|h| h.index == index);
    let background = if is_active {
        colors.bg
    } else if is_hot {
        themes::mix(colors.bg, colors.fg, HOVER_MIX)
    } else {
        themes::mix(colors.bg, colors.fg, STRIP_MIX)
    };
    gdi::fill(hdc, &rect, background);
    if is_active {
        let accent = RECT {
            bottom: rect.top + s(ACCENT_H),
            ..rect
        };
        gdi::fill(hdc, &accent, themes::mix(colors.bg, colors.fg, ACCENT_MIX));
    }
    let text_color = if is_active {
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
