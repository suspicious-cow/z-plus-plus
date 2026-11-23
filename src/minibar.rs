//! The mini toolbar, like Word's: after you select text with the mouse, a small bar of tools
//! appears just above the selection. It never takes focus, so the caret and typing stay in the
//! editor. Typing, a keyboard selection, scrolling, switching tabs, or moving the pointer away
//! hides it.
//!
//! In Markdown it offers Bold, Italic, Underline, Strikethrough, Code, Link, Heading, List, and
//! Quote (all as Markdown markup; see format.rs). Plain text can't hold formatting, so there it
//! offers Cut, Copy, UPPERCASE, and lowercase.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_LBUTTON};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::format::{self, Style};
use crate::gdi::{self, s};
use crate::themes::{self, Colors};
use crate::{editor, sci, window};

const CLASS: *const u16 = w!("ZppMiniBar");
const BUTTON_W: i32 = 30;
const WIDE_BUTTON_W: i32 = 44;
const HEIGHT: i32 = 32;
const PAD: i32 = 3;
const GAP_BELOW: i32 = 6;
const SEPARATOR_W: i32 = 9;
const RADIUS: i32 = 6;
const BG_MIX: u32 = 4;
const HOT_MIX: u32 = 16;
const BORDER_MIX: u32 = 30;
const SEPARATOR_MIX: u32 = 22;
/// Checks whether the mouse button is up, and how far away the pointer is.
const TIMER_ID: usize = 1;
const TIMER_MS: u32 = 60;
/// The bar hides once the pointer is this far (96 DPI pixels) from it.
const FAR: i32 = 120;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUNDSMALL: u32 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Style(Style),
    Link,
    Prefix(&'static str),
    Cut,
    Copy,
    Upper,
    Lower,
    Separator,
}

/// What a button shows: a label in a styled font.
#[derive(Clone, Copy)]
struct Face {
    label: &'static str,
    is_bold: bool,
    is_italic: bool,
    is_underline: bool,
    is_strike: bool,
    is_wide: bool,
}

const fn face(label: &'static str) -> Face {
    Face {
        label,
        is_bold: false,
        is_italic: false,
        is_underline: false,
        is_strike: false,
        is_wide: false,
    }
}

pub const MARKDOWN_TOOLS: [Tool; 10] = [
    Tool::Style(Style::Bold),
    Tool::Style(Style::Italic),
    Tool::Style(Style::Underline),
    Tool::Style(Style::Strike),
    Tool::Style(Style::Code),
    Tool::Link,
    Tool::Separator,
    Tool::Prefix("# "),
    Tool::Prefix("- "),
    Tool::Prefix("> "),
];

pub const TEXT_TOOLS: [Tool; 4] = [Tool::Cut, Tool::Copy, Tool::Upper, Tool::Lower];

fn face_of(tool: Tool) -> Face {
    match tool {
        Tool::Style(Style::Bold) => Face {
            is_bold: true,
            ..face("B")
        },
        Tool::Style(Style::Italic) => Face {
            is_italic: true,
            ..face("I")
        },
        Tool::Style(Style::Underline) => Face {
            is_underline: true,
            ..face("U")
        },
        Tool::Style(Style::Strike) => Face {
            is_strike: true,
            ..face("S")
        },
        Tool::Style(Style::Code) => face("</>"),
        Tool::Link => Face {
            is_wide: true,
            ..face("Link")
        },
        Tool::Prefix("# ") => face("H"),
        Tool::Prefix("- ") => face("\u{2022}"),
        Tool::Prefix(_) => face("\u{201C}"),
        Tool::Cut => Face {
            is_wide: true,
            ..face("Cut")
        },
        Tool::Copy => Face {
            is_wide: true,
            ..face("Copy")
        },
        Tool::Upper => face("AB"),
        Tool::Lower => face("ab"),
        Tool::Separator => face(""),
    }
}

fn tip_of(tool: Tool) -> &'static str {
    match tool {
        Tool::Style(Style::Bold) => "Bold (Ctrl+B)",
        Tool::Style(Style::Italic) => "Italic (Ctrl+I)",
        Tool::Style(Style::Underline) => "Underline (Ctrl+U), saved as <u>...</u>",
        Tool::Style(Style::Strike) => "Strikethrough, ~~text~~",
        Tool::Style(Style::Code) => "Inline code, `text`",
        Tool::Link => "Link, [text](url)",
        Tool::Prefix("# ") => "Heading",
        Tool::Prefix("- ") => "Bulleted list",
        Tool::Prefix(_) => "Quote",
        Tool::Cut => "Cut",
        Tool::Copy => "Copy",
        Tool::Upper => "UPPERCASE",
        Tool::Lower => "lowercase",
        Tool::Separator => "",
    }
}

/// Button widths, with separators narrower.
fn widths(tools: &[Tool]) -> Vec<i32> {
    tools
        .iter()
        .map(|&t| match t {
            Tool::Separator => SEPARATOR_W,
            _ if face_of(t).is_wide => WIDE_BUTTON_W,
            _ => BUTTON_W,
        })
        .collect()
}

/// Where the bar goes: above `anchor` (the selection's top-left on screen), or below the line
/// (`line_h` tall) if there is no room above; kept inside `work`.
pub fn place(anchor: POINT, line_h: i32, size: (i32, i32), work: RECT, gap: i32) -> POINT {
    let (w, h) = size;
    let above = anchor.y - h - gap;
    let y = if above >= work.top {
        above
    } else {
        (anchor.y + line_h + gap).min(work.bottom - h)
    };
    let x = anchor.x.clamp(work.left, (work.right - w).max(work.left));
    POINT { x, y }
}

struct Bar {
    hwnd: Cell<HWND>,
    tools: RefCell<Vec<Tool>>,
    rects: RefCell<Vec<RECT>>,
    hot: Cell<Option<usize>>,
    /// The editor the bar acts on.
    edit: Cell<HWND>,
    /// A mouse selection is being made; show the bar when the button comes up.
    is_selecting: Cell<bool>,
    /// The selection the bar was shown for; a change to anything else hides it.
    shown_for: Cell<(isize, isize)>,
    colors: Cell<Colors>,
}

thread_local! {
    static BAR: Bar = const {
        Bar {
            hwnd: Cell::new(null_mut()),
            tools: RefCell::new(Vec::new()),
            rects: RefCell::new(Vec::new()),
            hot: Cell::new(None),
            edit: Cell::new(null_mut()),
            is_selecting: Cell::new(false),
            shown_for: Cell::new((0, 0)),
            colors: Cell::new(themes::PRESETS[0].colors),
        }
    };
}

fn bar<R>(f: impl FnOnce(&Bar) -> R) -> R {
    BAR.with(f)
}

pub fn is_shown() -> bool {
    let hwnd = bar(|b| b.hwnd.get());
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 }
}

pub unsafe fn hide() {
    let hwnd = bar(|b| b.hwnd.get());
    bar(|b| b.is_selecting.set(false));
    if !hwnd.is_null() {
        KillTimer(hwnd, TIMER_ID);
        ShowWindow(hwnd, SW_HIDE);
    }
}

/// From SCN_UPDATEUI when the selection changed in `edit` (`is_markdown`: the tab's language).
/// A selection made with the mouse shows the bar once the button is released; any other
/// change (typing, arrow keys) hides it.
pub unsafe fn on_selection_change(edit: HWND, is_markdown: bool) {
    let span = editor::selection(edit);
    let is_mouse_down = GetKeyState(VK_LBUTTON as i32) < 0;
    if span.start == span.end {
        hide();
        return;
    }
    if !is_mouse_down {
        let is_same = is_shown() && bar(|b| b.shown_for.get()) == (span.start, span.end);
        if !bar(|b| b.is_selecting.get()) && !is_same {
            hide();
        }
        return;
    }
    let hwnd = ensure_window();
    let tools = if is_markdown {
        MARKDOWN_TOOLS.to_vec()
    } else {
        TEXT_TOOLS.to_vec()
    };
    bar(|b| {
        b.edit.set(edit);
        b.is_selecting.set(true);
        *b.tools.borrow_mut() = tools;
    });
    ShowWindow(hwnd, SW_HIDE);
    SetTimer(hwnd, TIMER_ID, TIMER_MS, None);
}

/// For GUI checks (ZPP_TEST only): shows the bar for the current selection as if it had been
/// made with the mouse, since a check can't press the real mouse button.
pub unsafe fn show_for_test(edit: HWND, is_markdown: bool) {
    if !crate::profile::is_test() {
        return;
    }
    ensure_window();
    let tools = if is_markdown {
        MARKDOWN_TOOLS.to_vec()
    } else {
        TEXT_TOOLS.to_vec()
    };
    bar(|b| {
        b.edit.set(edit);
        *b.tools.borrow_mut() = tools;
    });
    show();
}

unsafe fn ensure_window() -> HWND {
    let existing = bar(|b| b.hwnd.get());
    if !existing.is_null() {
        return existing;
    }
    let hinst = GetModuleHandleW(null());
    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst,
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        lpszClassName: CLASS,
        ..zeroed()
    };
    RegisterClassExW(&wc);
    let owner = window::app(|a| a.main.get());
    let hwnd = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        CLASS,
        w!("z++ mini toolbar"),
        WS_POPUP,
        0,
        0,
        0,
        0,
        owner,
        null_mut(),
        hinst,
        null(),
    );
    let round = DWMWCP_ROUNDSMALL;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &round as *const u32 as *const _,
        size_of::<u32>() as u32,
    );
    bar(|b| b.hwnd.set(hwnd));
    hwnd
}

/// Lays the buttons out and shows the bar over the selection.
unsafe fn show() {
    let (hwnd, edit, tools) = bar(|b| (b.hwnd.get(), b.edit.get(), b.tools.borrow().clone()));
    let span = editor::selection(edit);
    if span.start == span.end || hwnd.is_null() {
        return;
    }
    let mut x = s(PAD);
    let rects: Vec<RECT> = widths(&tools)
        .into_iter()
        .map(|w| {
            let rc = RECT {
                left: x,
                top: s(PAD),
                right: x + s(w),
                bottom: s(HEIGHT) - s(PAD),
            };
            x += s(w);
            rc
        })
        .collect();
    let size = (x + s(PAD), s(HEIGHT));
    let first = span.start.min(span.end) as usize;
    let mut anchor = POINT {
        x: SendMessageW(edit, sci::SCI_POINTXFROMPOSITION, 0, first as isize) as i32,
        y: SendMessageW(edit, sci::SCI_POINTYFROMPOSITION, 0, first as isize) as i32,
    };
    ClientToScreen(edit, &mut anchor);
    let line_h = SendMessageW(edit, sci::SCI_TEXTHEIGHT, 0, 0) as i32;
    let at = place(anchor, line_h, size, work_area(edit), s(GAP_BELOW));
    bar(|b| {
        *b.rects.borrow_mut() = rects;
        b.hot.set(None);
        b.colors.set(window::look_parts().0);
        b.shown_for.set((span.start, span.end));
    });
    SetWindowPos(
        hwnd,
        null_mut(),
        at.x,
        at.y,
        size.0,
        size.1,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER,
    );
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    InvalidateRect(hwnd, null(), 0);
}

unsafe fn work_area(edit: HWND) -> RECT {
    let monitor = MonitorFromWindow(edit, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = zeroed();
    info.cbSize = size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(monitor, &mut info);
    info.rcWork
}

unsafe fn on_timer(hwnd: HWND) {
    if bar(|b| b.is_selecting.get()) {
        if GetKeyState(VK_LBUTTON as i32) < 0 {
            return;
        }
        bar(|b| b.is_selecting.set(false));
        show();
        return;
    }
    if IsWindowVisible(hwnd) == 0 {
        KillTimer(hwnd, TIMER_ID);
        return;
    }
    let mut pt: POINT = zeroed();
    GetCursorPos(&mut pt);
    let mut rc: RECT = zeroed();
    GetWindowRect(hwnd, &mut rc);
    let dx = (rc.left - pt.x).max(pt.x - rc.right).max(0);
    let dy = (rc.top - pt.y).max(pt.y - rc.bottom).max(0);
    if dx.max(dy) > s(FAR) {
        hide();
    }
}

unsafe fn run(tool: Tool) {
    let edit = bar(|b| b.edit.get());
    match tool {
        Tool::Style(style) => format::apply(edit, style),
        Tool::Link => format::apply_link(edit),
        Tool::Prefix(prefix) => format::apply_prefix(edit, prefix),
        Tool::Cut => editor::cut(edit),
        Tool::Copy => editor::copy(edit),
        Tool::Upper => {
            SendMessageW(edit, sci::SCI_UPPERCASE, 0, 0);
        }
        Tool::Lower => {
            SendMessageW(edit, sci::SCI_LOWERCASE, 0, 0);
        }
        Tool::Separator => return,
    }
    // Cut leaves nothing selected; the other tools keep working on the new selection.
    if editor::selection(edit).start == editor::selection(edit).end {
        hide();
    } else {
        show();
    }
}

fn hit_test(lparam: LPARAM) -> Option<usize> {
    let (x, y) = (
        (lparam & 0xFFFF) as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as i16 as i32,
    );
    bar(|b| {
        let tools = b.tools.borrow();
        b.rects
            .borrow()
            .iter()
            .position(|rc| x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom)
            .filter(|&i| tools[i] != Tool::Separator)
    })
}

/// For GUI checks: WM_COMMAND with a button's index runs that tool.
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PRINTCLIENT => {
            let mut rc: RECT = zeroed();
            GetClientRect(hwnd, &mut rc);
            draw(wparam as HDC, &rc);
            0
        }
        WM_TIMER => {
            on_timer(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let hit = hit_test(lparam);
            if bar(|b| b.hot.replace(hit)) != hit {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            if let Some(i) = hit_test(lparam) {
                run(bar(|b| b.tools.borrow()[i]));
            }
            0
        }
        WM_COMMAND => {
            let tool = bar(|b| b.tools.borrow().get(wparam & 0xFFFF).copied());
            if let Some(tool) = tool {
                run(tool);
            }
            0
        }
        WM_GETTEXT | WM_GETTEXTLENGTH => {
            // Screen readers and checks read the tools as the window text.
            let text = bar(|b| {
                b.tools
                    .borrow()
                    .iter()
                    .filter(|&&t| t != Tool::Separator)
                    .map(|&t| tip_of(t))
                    .collect::<Vec<_>>()
                    .join(" | ")
            });
            let units: Vec<u16> = text.encode_utf16().collect();
            if msg == WM_GETTEXTLENGTH {
                return units.len() as LRESULT;
            }
            let n = units.len().min(wparam.saturating_sub(1));
            std::ptr::copy_nonoverlapping(units.as_ptr(), lparam as *mut u16, n);
            *(lparam as *mut u16).add(n) = 0;
            n as LRESULT
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc: RECT = zeroed();
    GetClientRect(hwnd, &mut rc);
    draw(hdc, &rc);
    EndPaint(hwnd, &ps);
}

unsafe fn draw(hdc: HDC, rc: &RECT) {
    let rc = *rc;
    let (colors, tools, rects, hot) = bar(|b| {
        (
            b.colors.get(),
            b.tools.borrow().clone(),
            b.rects.borrow().clone(),
            b.hot.get(),
        )
    });
    gdi::buffered(hdc, &rc, |mem| {
        gdi::fill(mem, &rc, themes::mix(colors.bg, colors.fg, BG_MIX));
        gdi::draw_frame(mem, &rc, themes::mix(colors.bg, colors.fg, BORDER_MIX), 1);
        for (i, (&tool, button)) in tools.iter().zip(&rects).enumerate() {
            if tool == Tool::Separator {
                let line = RECT {
                    left: (button.left + button.right) / 2,
                    right: (button.left + button.right) / 2 + 1,
                    top: button.top + s(4),
                    bottom: button.bottom - s(4),
                };
                gdi::fill(mem, &line, themes::mix(colors.bg, colors.fg, SEPARATOR_MIX));
                continue;
            }
            if hot == Some(i) {
                let mix = themes::mix(colors.bg, colors.fg, HOT_MIX);
                gdi::round_fill(mem, button, mix, s(RADIUS));
            }
            draw_face(mem, face_of(tool), *button, colors.fg);
        }
    });
}

unsafe fn draw_face(mem: HDC, face: Face, rc: RECT, color: u32) {
    let mut lf: LOGFONTW = zeroed();
    GetObjectW(
        gdi::ui_font() as HGDIOBJ,
        size_of::<LOGFONTW>() as i32,
        &mut lf as *mut _ as *mut _,
    );
    lf.lfWeight = if face.is_bold {
        FW_BOLD as i32
    } else {
        FW_NORMAL as i32
    };
    lf.lfItalic = face.is_italic as u8;
    lf.lfUnderline = face.is_underline as u8;
    lf.lfStrikeOut = face.is_strike as u8;
    let font = CreateFontIndirectW(&lf);
    SetTextColor(mem, color);
    let mut at = rc;
    gdi::draw_str(
        mem,
        font,
        face.label,
        &mut at,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    DeleteObject(font as HGDIOBJ);
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: RECT = RECT {
        left: 0,
        top: 0,
        right: 1000,
        bottom: 800,
    };

    #[test]
    fn goes_above_the_selection_or_below_when_no_room() {
        let p = place(POINT { x: 100, y: 300 }, 20, (300, 32), WORK, 6);
        assert_eq!((p.x, p.y), (100, 262));
        let p = place(POINT { x: 100, y: 10 }, 20, (300, 32), WORK, 6);
        assert_eq!(
            (p.x, p.y),
            (100, 36),
            "below the line at the top of the screen"
        );
        let p = place(POINT { x: 900, y: 300 }, 20, (300, 32), WORK, 6);
        assert_eq!(p.x, 700, "kept on screen at the right edge");
    }

    #[test]
    fn every_tool_has_a_label_and_a_tip() {
        for &t in MARKDOWN_TOOLS.iter().chain(&TEXT_TOOLS) {
            if t != Tool::Separator {
                assert!(
                    !face_of(t).label.is_empty() && !tip_of(t).is_empty(),
                    "{t:?}"
                );
            }
        }
        assert_eq!(widths(&TEXT_TOOLS).len(), TEXT_TOOLS.len());
    }
}
