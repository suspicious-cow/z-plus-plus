//! A bar above the text for news about the file, such as "changed on disk", with buttons.
//! It never blocks: the user (or an agent's edits) can carry on and answer later. Clicking a
//! button posts WM_APP_NOTICE with the button's index in wparam.

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

pub const WM_APP_NOTICE: u32 = WM_APP + 8;

const CLASS: *const u16 = w!("ZppNotice");
const HEIGHT: i32 = 32;
const PAD: i32 = 10;
const BUTTON_PAD: i32 = 12;
const BUTTON_GAP: i32 = 6;
const BUTTON_MARGIN: i32 = 5;
const RADIUS: i32 = 4;
const BG_MIX: u32 = 14;
const LINE_MIX: u32 = 25;
const BUTTON_MIX: u32 = 26;
const HOVER_MIX: u32 = 40;

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Notice {
    pub text: String,
    pub buttons: Vec<&'static str>,
}

struct Bar {
    hwnd: Cell<HWND>,
    notice: RefCell<Option<Notice>>,
    buttons: RefCell<Vec<RECT>>,
    hot: Cell<Option<usize>>,
    is_tracking: Cell<bool>,
    colors: Cell<Colors>,
}

thread_local! {
    static BAR: Bar = const {
        Bar {
            hwnd: Cell::new(null_mut()),
            notice: RefCell::new(None),
            buttons: RefCell::new(Vec::new()),
            hot: Cell::new(None),
            is_tracking: Cell::new(false),
            colors: Cell::new(themes::PRESETS[0].colors),
        }
    };
}

fn bar<R>(f: impl FnOnce(&Bar) -> R) -> R {
    BAR.with(f)
}

/// Height to lay out: zero while there is nothing to say.
pub fn height() -> i32 {
    if bar(|b| b.notice.borrow().is_some()) {
        s(HEIGHT)
    } else {
        0
    }
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

/// Shows `notice`, or hides the bar with None. True if that changes the layout.
pub unsafe fn set(notice: Option<Notice>) -> bool {
    let was_shown = bar(|b| b.notice.replace(notice.clone()).is_some());
    bar(|b| b.hot.set(None));
    InvalidateRect(bar(|b| b.hwnd.get()), null(), 0);
    was_shown != notice.is_some()
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
            if let Some(button) = hit_test(lparam) {
                PostMessageW(GetParent(hwnd), WM_APP_NOTICE, button, 0);
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
        b.buttons
            .borrow()
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
    let (colors, notice, hot) = bar(|b| (b.colors.get(), b.notice.borrow().clone(), b.hot.get()));
    let Some(notice) = notice else {
        EndPaint(hwnd, &ps);
        return;
    };
    let font = gdi::ui_font();
    let mut rects = Vec::new();
    gdi::buffered(hdc, &rc, |mem| {
        gdi::fill(mem, &rc, themes::mix(colors.bg, colors.fg, BG_MIX));
        gdi::fill(
            mem,
            &RECT {
                top: rc.bottom - 1,
                ..rc
            },
            themes::mix(colors.bg, colors.fg, LINE_MIX),
        );
        SetTextColor(mem, colors.fg);
        // Buttons from the left, after the text.
        let mut left = s(PAD) + gdi::measure(font, &notice.text) + s(PAD) * 2;
        for (i, label) in notice.buttons.iter().enumerate() {
            let w = gdi::measure(font, label) + s(BUTTON_PAD) * 2;
            let button = RECT {
                left,
                top: s(BUTTON_MARGIN),
                right: left + w,
                bottom: rc.bottom - s(BUTTON_MARGIN),
            };
            let mix = if hot == Some(i) {
                HOVER_MIX
            } else {
                BUTTON_MIX
            };
            gdi::round_fill(
                mem,
                &button,
                themes::mix(colors.bg, colors.fg, mix),
                s(RADIUS),
            );
            let mut r = button;
            gdi::draw_str(
                mem,
                font,
                label,
                &mut r,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            rects.push(button);
            left = button.right + s(BUTTON_GAP);
        }
        let mut text = RECT { left: s(PAD), ..rc };
        gdi::draw_str(
            mem,
            font,
            &notice.text,
            &mut text,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    });
    bar(|b| *b.buttons.borrow_mut() = rects);
    EndPaint(hwnd, &ps);
}
