//! The status bar under the editor: caret position and selection on the left, a short message,
//! and zoom, line endings and encoding on the right. Clicking the encoding or line endings posts
//! WM_APP_STATUS_CLICK with the part in wparam, so the main window can offer a conversion menu.

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

pub const WM_APP_STATUS_CLICK: u32 = WM_APP + 4;
pub const PART_ENCODING: usize = 0;
pub const PART_LINE_ENDINGS: usize = 1;

const CLASS: *const u16 = w!("ZppStatusBar");
const HEIGHT: i32 = 24;
const PAD: i32 = 10;
const GAP: i32 = 4;
const BG_MIX: u32 = 6;
const LINE_MIX: u32 = 18;
const HOVER_MIX: u32 = 16;
const DIM_MIX: u32 = 70;

#[derive(Default, Clone)]
pub struct Info {
    pub position: String,
    pub message: String,
    pub zoom: String,
    pub line_endings: String,
    pub encoding: String,
}

struct Bar {
    hwnd: Cell<HWND>,
    info: RefCell<Info>,
    /// Clickable parts: encoding, line endings.
    parts: Cell<[RECT; 2]>,
    hot: Cell<Option<usize>>,
    is_tracking: Cell<bool>,
    colors: Cell<Colors>,
}

thread_local! {
    static BAR: Bar = Bar {
        hwnd: Cell::new(null_mut()),
        info: RefCell::new(Info::default()),
        parts: Cell::new([RECT { left: 0, top: 0, right: 0, bottom: 0 }; 2]),
        hot: Cell::new(None),
        is_tracking: Cell::new(false),
        colors: Cell::new(themes::PRESETS[0].colors),
    };
}

fn bar<R>(f: impl FnOnce(&Bar) -> R) -> R {
    BAR.with(f)
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
    bar(|b| b.hwnd.set(hwnd));
    hwnd
}

pub unsafe fn set(info: Info) {
    bar(|b| *b.info.borrow_mut() = info);
    InvalidateRect(bar(|b| b.hwnd.get()), null(), 0);
}

/// Changes only the message, for "Replaced 3" and similar notes.
pub unsafe fn set_message(message: &str) {
    bar(|b| b.info.borrow_mut().message = message.to_string());
    InvalidateRect(bar(|b| b.hwnd.get()), null(), 0);
}

pub fn message() -> String {
    bar(|b| b.info.borrow().message.clone())
}

pub unsafe fn set_colors(colors: Colors) {
    bar(|b| b.colors.set(colors));
    InvalidateRect(bar(|b| b.hwnd.get()), null(), 0);
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
            bar(|b| {
                b.is_tracking.set(false);
                b.hot.set(None);
            });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            if let Some(part) = hit_test(lparam) {
                PostMessageW(GetParent(hwnd), WM_APP_STATUS_CLICK, part, 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn hit_test(lparam: LPARAM) -> Option<usize> {
    let (x, y) = (
        (lparam & 0xFFFF) as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as i16 as i32,
    );
    bar(|b| {
        b.parts
            .get()
            .iter()
            .position(|rc| x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom)
    })
}

unsafe fn on_mouse_move(hwnd: HWND, lparam: LPARAM) {
    let hit = hit_test(lparam);
    let (changed, is_tracking) = bar(|b| (b.hot.replace(hit) != hit, b.is_tracking.replace(true)));
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
    let (colors, info, hot) = bar(|b| (b.colors.get(), b.info.borrow().clone(), b.hot.get()));
    let font = gdi::ui_font();
    let mut parts = [RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }; 2];
    gdi::buffered(hdc, &rc, |mem| {
        gdi::fill(mem, &rc, themes::mix(colors.bg, colors.fg, BG_MIX));
        gdi::fill(
            mem,
            &RECT { bottom: 1, ..rc },
            themes::mix(colors.bg, colors.fg, LINE_MIX),
        );
        let dim = themes::mix(colors.bg, colors.fg, DIM_MIX);
        // Right side, laid out from the right edge: encoding, line endings, zoom.
        let mut right = rc.right - s(GAP);
        let segments = [
            (&info.encoding, Some(PART_ENCODING)),
            (&info.line_endings, Some(PART_LINE_ENDINGS)),
            (&info.zoom, None),
        ];
        for (text, part) in segments {
            let w = gdi::measure(font, text) + s(PAD) * 2;
            let seg = RECT {
                left: right - w,
                top: 1,
                right,
                bottom: rc.bottom,
            };
            if let Some(p) = part {
                parts[p] = seg;
                if hot == Some(p) {
                    gdi::fill(mem, &seg, themes::mix(colors.bg, colors.fg, HOVER_MIX));
                }
            }
            SetTextColor(mem, colors.fg);
            let mut r = seg;
            gdi::draw_str(
                mem,
                font,
                text,
                &mut r,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            right = seg.left;
        }
        SetTextColor(mem, colors.fg);
        let mut left = RECT {
            left: s(PAD),
            top: 1,
            right,
            bottom: rc.bottom,
        };
        gdi::draw_str(
            mem,
            font,
            &info.position,
            &mut left,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let message_left = s(PAD) + gdi::measure(font, &info.position) + s(PAD) * 3;
        SetTextColor(mem, dim);
        let mut msg = RECT {
            left: message_left,
            top: 1,
            right: right - s(PAD),
            bottom: rc.bottom,
        };
        gdi::draw_str(
            mem,
            font,
            &info.message,
            &mut msg,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    });
    bar(|b| b.parts.set(parts));
    EndPaint(hwnd, &ps);
}
