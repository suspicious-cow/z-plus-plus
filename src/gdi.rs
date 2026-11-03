//! Drawing helpers shared by the custom-drawn strips: toolbar, tabs and status bar.

use std::cell::Cell;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
};

const BASE_DPI: i32 = 96;

thread_local! {
    static DPI: Cell<i32> = const { Cell::new(0) };
    static UI_FONT: Cell<HFONT> = const { Cell::new(null_mut()) };
}

/// Scales a length given at 96 DPI to the screen's DPI.
pub fn s(v: i32) -> i32 {
    let dpi = DPI.with(|d| {
        if d.get() == 0 {
            unsafe {
                let screen = GetDC(null_mut());
                d.set(GetDeviceCaps(screen, LOGPIXELSY as _));
                ReleaseDC(null_mut(), screen);
            }
        }
        d.get()
    });
    v * dpi / BASE_DPI
}

/// The Windows message font (Segoe UI 9 pt by default), created once.
pub fn ui_font() -> HFONT {
    UI_FONT.with(|f| {
        if f.get().is_null() {
            unsafe {
                let mut ncm: NONCLIENTMETRICSW = zeroed();
                ncm.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
                SystemParametersInfoW(
                    SPI_GETNONCLIENTMETRICS,
                    ncm.cbSize,
                    &mut ncm as *mut _ as *mut _,
                    0,
                );
                f.set(CreateFontIndirectW(&ncm.lfMessageFont));
            }
        }
        f.get()
    })
}

pub unsafe fn fill(hdc: HDC, rect: &RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, rect, brush);
    DeleteObject(brush as HGDIOBJ);
}

pub unsafe fn round_fill(hdc: HDC, rect: &RECT, color: u32, radius: i32) {
    let brush = CreateSolidBrush(color);
    let old_brush = SelectObject(hdc, brush as HGDIOBJ);
    let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(brush as HGDIOBJ);
}

pub unsafe fn draw_frame(hdc: HDC, rc: &RECT, color: u32, width: i32) {
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

pub unsafe fn draw_text(hdc: HDC, font: HFONT, text: &[u16], rect: &mut RECT, flags: u32) {
    // An empty slice's pointer is not a real address, and DrawTextW reads it even at length 0.
    if text.is_empty() {
        return;
    }
    let old = SelectObject(hdc, font as HGDIOBJ);
    DrawTextW(hdc, text.as_ptr(), text.len() as i32, rect, flags);
    SelectObject(hdc, old);
}

pub unsafe fn draw_str(hdc: HDC, font: HFONT, text: &str, rect: &mut RECT, flags: u32) {
    let units: Vec<u16> = text.encode_utf16().collect();
    draw_text(hdc, font, &units, rect, flags);
}

/// Width of `text` in pixels; `hdc` must already have the font selected.
pub unsafe fn text_width(hdc: HDC, text: &str) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut size: SIZE = zeroed();
    GetTextExtentPoint32W(hdc, units.as_ptr(), units.len() as i32, &mut size);
    size.cx
}

/// Width of `text` in `font`, measured on a screen DC.
pub unsafe fn measure(font: HFONT, text: &str) -> i32 {
    let screen = GetDC(null_mut());
    let old = SelectObject(screen, font as HGDIOBJ);
    let width = text_width(screen, text);
    SelectObject(screen, old);
    ReleaseDC(null_mut(), screen);
    width
}

/// Double-buffered painting: `draw` paints into an off-screen bitmap the size of `rc`.
pub unsafe fn buffered(hdc: HDC, rc: &RECT, draw: impl FnOnce(HDC)) {
    let mem = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, rc.right, rc.bottom);
    let old_bmp = SelectObject(mem, bmp as HGDIOBJ);
    SetBkMode(mem, TRANSPARENT as i32);
    draw(mem);
    BitBlt(hdc, 0, 0, rc.right, rc.bottom, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old_bmp);
    DeleteObject(bmp as HGDIOBJ);
    DeleteDC(mem);
}
