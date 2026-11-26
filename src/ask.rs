//! A modal question drawn in z++'s own colors and font, with large labeled choices, for news
//! the user must answer before going on, such as a file changed or deleted on disk.
//!
//! Keys: Up/Down or Tab move between the choices and the check box, Enter or Space picks,
//! 1 to 9 pick a choice directly, Escape cancels. WM_COMMAND with a choice's id picks it and
//! WM_COMMAND with CHECK_ID toggles the check box, which is how the GUI checks drive it.

use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::gdi::{self, s};
use crate::themes::{self, Colors};
use crate::{profile, wide, window};

/// What `ask` returns for Escape.
pub const CANCELED: i32 = 2;
/// WM_COMMAND id that toggles the check box.
pub const CHECK_ID: i32 = 1;

const CLASS: *const u16 = w!("ZppAsk");
const WIDTH: i32 = 500;
const PAD: i32 = 24;
const GAP: i32 = 8;
const ROW_PAD_X: i32 = 14;
const ROW_PAD_Y: i32 = 12;
const NOTE_GAP: i32 = 2;
const BOX: i32 = 15;
const BOX_GAP: i32 = 8;
const BOX_RADIUS: i32 = 3;
const RADIUS: i32 = 6;
const BG_MIX: u32 = 4;
const ROW_MIX: u32 = 10;
const HOVER_MIX: u32 = 18;
const DIM_MIX: u32 = 62;
const FOCUS_MIX: u32 = 55;
const BORDER_MIX: u32 = 30;
const HEADING_SCALE: i32 = 135;
const SEMIBOLD: i32 = 600;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

pub struct Choice<'a> {
    /// Never CANCELED or CHECK_ID.
    pub id: i32,
    pub label: &'a str,
    /// Smaller, dimmer text under the label.
    pub note: &'a str,
}

pub struct Question<'a> {
    pub heading: &'a str,
    pub detail: &'a str,
    /// Shown dimmed on its own line, shortened in the middle if it is too long.
    pub path: &'a str,
    pub choices: &'a [Choice<'a>],
    pub default: i32,
    /// A check box under the choices, unchecked at first.
    pub check: Option<&'a str>,
}

#[derive(Clone)]
struct Owned {
    heading: String,
    detail: String,
    path: String,
    choices: Vec<(i32, String, String)>,
    check: Option<String>,
}

#[derive(Default)]
struct State {
    /// Choice rects, then the check box row if there is one.
    rows: Vec<RECT>,
    focus: usize,
    hot: Option<usize>,
    is_checked: bool,
    answer: Option<i32>,
    is_tracking: bool,
}

thread_local! {
    static QUESTION: RefCell<Option<Owned>> = const { RefCell::new(None) };
    static STATE: RefCell<State> = RefCell::new(State::default());
    static COLORS: Cell<Colors> = const { Cell::new(themes::PRESETS[0].colors) };
    static HEADING_FONT: Cell<HFONT> = const { Cell::new(null_mut()) };
    static LABEL_FONT: Cell<HFONT> = const { Cell::new(null_mut()) };
}

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|st| f(&mut st.borrow_mut()))
}

fn question() -> Owned {
    QUESTION.with(|q| q.borrow().clone().expect("a question is being asked"))
}

/// Shows the question centered on `owner` and waits. The chosen id (CANCELED for Escape) and
/// whether the check box was ticked.
pub unsafe fn ask(owner: HWND, q: &Question) -> (i32, bool) {
    let owned = Owned {
        heading: q.heading.into(),
        detail: q.detail.into(),
        path: q.path.into(),
        choices: q
            .choices
            .iter()
            .map(|c| (c.id, c.label.into(), c.note.into()))
            .collect(),
        check: q.check.map(Into::into),
    };
    let focus = q
        .choices
        .iter()
        .position(|c| c.id == q.default)
        .unwrap_or(0);
    QUESTION.with(|slot| *slot.borrow_mut() = Some(owned));
    state(|st| {
        *st = State {
            focus,
            ..State::default()
        }
    });
    COLORS.with(|c| c.set(window::chrome_colors()));
    // A minimized owner has a far-off position; the dialog belongs over the restored window.
    if IsIconic(owner) != 0 && !profile::is_test() {
        ShowWindow(owner, SW_RESTORE);
    }
    let hwnd = create(owner);
    if hwnd.is_null() {
        return (CANCELED, false);
    }
    EnableWindow(owner, 0);
    ShowWindow(hwnd, profile::show_cmd());
    if !profile::is_test() {
        SetForegroundWindow(hwnd);
    }
    SetFocus(hwnd);
    let mut msg: MSG = zeroed();
    while state(|st| st.answer.is_none()) {
        if GetMessageW(&mut msg, null_mut(), 0, 0) <= 0 {
            PostQuitMessage(msg.wParam as i32);
            state(|st| st.answer = Some(CANCELED));
            break;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    EnableWindow(owner, 1);
    DestroyWindow(hwnd);
    if !profile::is_test() {
        SetActiveWindow(owner);
    }
    QUESTION.with(|slot| slot.borrow_mut().take());
    state(|st| (st.answer.unwrap_or(CANCELED), st.is_checked))
}

unsafe fn create(owner: HWND) -> HWND {
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
    let height = layout();
    let width = s(WIDTH);
    let mut rc: RECT = zeroed();
    GetWindowRect(owner, &mut rc);
    // Test windows sit off screen on purpose; keep their dialogs with them.
    let work = (!profile::is_test()).then(|| work_area(owner));
    let (x, y) = place(&rc, work.as_ref(), width, height);
    let title = wide(&accessible_text());
    let hwnd = CreateWindowExW(
        0,
        CLASS,
        title.as_ptr(),
        WS_POPUP,
        x,
        y,
        width,
        height,
        owner,
        null_mut(),
        hinst,
        null(),
    );
    if !hwnd.is_null() {
        let round = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const u32 as *const _,
            size_of::<u32>() as u32,
        );
    }
    hwnd
}

/// The work area (the screen less the taskbar) of the monitor `owner` is on.
unsafe fn work_area(owner: HWND) -> RECT {
    let mut info: MONITORINFO = zeroed();
    info.cbSize = size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(
        MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST),
        &mut info,
    );
    info.rcWork
}

/// Top-left corner for a `width` x `height` dialog centered on the `owner` window, nudged only
/// as far as it takes to keep it all inside `work`, the work area of z++'s monitor.
fn place(owner: &RECT, work: Option<&RECT>, width: i32, height: i32) -> (i32, i32) {
    let mut x = owner.left + (owner.right - owner.left - width) / 2;
    let mut y = owner.top + (owner.bottom - owner.top - height) / 2;
    if let Some(w) = work {
        x = x.min(w.right - width).max(w.left);
        y = y.min(w.bottom - height).max(w.top);
    }
    (x, y)
}

/// Everything the dialog says, as its window text, for screen readers and tests.
fn accessible_text() -> String {
    let q = question();
    let mut parts = vec![q.heading, q.detail, q.path];
    for (_, label, note) in q.choices {
        parts.push(format!("{label}: {note}"));
    }
    parts.extend(q.check);
    parts.retain(|p| !p.is_empty());
    parts.join("\n")
}

/// The UI font scaled to `percent` with weight `weight`, created once into `slot`.
fn derived_font(
    slot: &'static std::thread::LocalKey<Cell<HFONT>>,
    percent: i32,
    weight: i32,
) -> HFONT {
    slot.with(|f| {
        if f.get().is_null() {
            unsafe {
                let mut lf: LOGFONTW = zeroed();
                GetObjectW(
                    gdi::ui_font() as HGDIOBJ,
                    size_of::<LOGFONTW>() as i32,
                    &mut lf as *mut _ as *mut _,
                );
                lf.lfHeight = lf.lfHeight * percent / 100;
                lf.lfWeight = weight;
                f.set(CreateFontIndirectW(&lf));
            }
        }
        f.get()
    })
}

fn heading_font() -> HFONT {
    derived_font(&HEADING_FONT, HEADING_SCALE, SEMIBOLD)
}

fn label_font() -> HFONT {
    derived_font(&LABEL_FONT, 100, SEMIBOLD)
}

/// Height of `text` in `font` when wrapped to `width`.
unsafe fn wrapped_height(font: HFONT, text: &str, width: i32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let screen = GetDC(null_mut());
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: 0,
    };
    gdi::draw_str(
        screen,
        font,
        text,
        &mut rc,
        DT_WORDBREAK | DT_CALCRECT | DT_NOPREFIX,
    );
    ReleaseDC(null_mut(), screen);
    rc.bottom
}

unsafe fn row_height(label: &str, note: &str, text_w: i32) -> i32 {
    let note_h = if note.is_empty() {
        0
    } else {
        s(NOTE_GAP) + wrapped_height(gdi::ui_font(), note, text_w)
    };
    2 * s(ROW_PAD_Y) + wrapped_height(label_font(), label, text_w) + note_h
}

/// Where the choices start: below the heading, the detail, and the path.
unsafe fn choices_top(q: &Owned, inner: i32) -> i32 {
    let font = gdi::ui_font();
    let mut y = s(PAD) + wrapped_height(heading_font(), "Ag", inner) + s(GAP);
    y += wrapped_height(font, &q.detail, inner);
    if !q.path.is_empty() {
        y += wrapped_height(font, "Ag", inner);
    }
    y + s(PAD) - s(GAP)
}

/// Works out the rows' rects and returns the window height.
unsafe fn layout() -> i32 {
    let q = question();
    let inner = s(WIDTH) - 2 * s(PAD);
    let text_w = inner - 2 * s(ROW_PAD_X);
    let mut y = choices_top(&q, inner);
    let mut rows = Vec::new();
    let mut push = |y: &mut i32, h: i32| {
        rows.push(RECT {
            left: s(PAD),
            top: *y,
            right: s(PAD) + inner,
            bottom: *y + h,
        });
        *y += h + s(GAP);
    };
    for (_, label, note) in &q.choices {
        push(&mut y, row_height(label, note, text_w));
    }
    if let Some(check) = &q.check {
        y += s(GAP);
        let h = wrapped_height(gdi::ui_font(), check, inner - s(BOX) - s(BOX_GAP)).max(s(BOX));
        push(&mut y, h);
    }
    state(|st| st.rows = rows);
    y + s(PAD) - s(GAP)
}

fn choice_count() -> usize {
    QUESTION.with(|q| q.borrow().as_ref().map_or(0, |q| q.choices.len()))
}

fn choice_id(row: usize) -> Option<i32> {
    QUESTION.with(|q| q.borrow().as_ref()?.choices.get(row).map(|c| c.0))
}

fn pick(row: usize) {
    match choice_id(row) {
        Some(id) => state(|st| st.answer = Some(id)),
        None => state(|st| st.is_checked = !st.is_checked),
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PRINTCLIENT => {
            let mut rc: RECT = zeroed();
            GetClientRect(hwnd, &mut rc);
            let colors = COLORS.with(|c| c.get());
            gdi::buffered(wparam as HDC, &rc, |mem| draw(mem, &rc, colors));
            0
        }
        WM_KEYDOWN => {
            on_key(hwnd, wparam as u16);
            0
        }
        WM_CHAR => {
            let digit = (wparam as u32).wrapping_sub(u32::from(b'1')) as usize;
            if digit < choice_count() {
                pick(digit);
            }
            0
        }
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd, lparam);
            0
        }
        WM_MOUSELEAVE => {
            state(|st| {
                st.is_tracking = false;
                st.hot = None;
            });
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            if let Some(row) = hit_test(lparam) {
                state(|st| st.focus = row);
                pick(row);
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_COMMAND => {
            on_command((wparam & 0xFFFF) as i32);
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_CLOSE => {
            state(|st| st.answer = Some(CANCELED));
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn on_command(id: i32) {
    if id == CHECK_ID {
        state(|st| st.is_checked = !st.is_checked);
    } else if (0..choice_count()).any(|row| choice_id(row) == Some(id)) {
        state(|st| st.answer = Some(id));
    }
}

unsafe fn on_key(hwnd: HWND, key: u16) {
    let rows = state(|st| st.rows.len()).max(1);
    let is_shift = GetKeyState(VK_SHIFT as i32) < 0;
    let back = |st: &mut State| st.focus = (st.focus + rows - 1) % rows;
    let ahead = |st: &mut State| st.focus = (st.focus + 1) % rows;
    match key {
        VK_ESCAPE => state(|st| st.answer = Some(CANCELED)),
        VK_RETURN | VK_SPACE => pick(state(|st| st.focus)),
        VK_DOWN | VK_RIGHT => state(ahead),
        VK_UP | VK_LEFT => state(back),
        VK_TAB if is_shift => state(back),
        VK_TAB => state(ahead),
        _ => return,
    }
    InvalidateRect(hwnd, null(), 0);
}

fn hit_test(lparam: LPARAM) -> Option<usize> {
    let (x, y) = (
        (lparam & 0xFFFF) as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as i16 as i32,
    );
    state(|st| {
        st.rows
            .iter()
            .position(|rc| x >= rc.left && x < rc.right && y >= rc.top && y < rc.bottom)
    })
}

unsafe fn on_mouse_move(hwnd: HWND, lparam: LPARAM) {
    let hit = hit_test(lparam);
    let (changed, was_tracking) = state(|st| {
        let changed = st.hot != hit;
        st.hot = hit;
        (changed, std::mem::replace(&mut st.is_tracking, true))
    });
    if !was_tracking {
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
    let colors = COLORS.with(|c| c.get());
    gdi::buffered(hdc, &rc, |mem| draw(mem, &rc, colors));
    EndPaint(hwnd, &ps);
}

unsafe fn draw(mem: HDC, rc: &RECT, colors: Colors) {
    let q = question();
    let font = gdi::ui_font();
    let dim = themes::mix(colors.bg, colors.fg, DIM_MIX);
    gdi::fill(mem, rc, themes::mix(colors.bg, colors.fg, BG_MIX));
    gdi::draw_frame(mem, rc, themes::mix(colors.bg, colors.fg, BORDER_MIX), 1);
    let inner = rc.right - 2 * s(PAD);
    let line = |y: i32, h: i32| RECT {
        left: s(PAD),
        top: y,
        right: s(PAD) + inner,
        bottom: y + h,
    };
    let wrap = DT_WORDBREAK | DT_NOPREFIX;
    let mut y = s(PAD);
    SetTextColor(mem, colors.fg);
    // One line, shortened if need be, so a long file name never pushes the choices down.
    let h = wrapped_height(heading_font(), "Ag", inner);
    let one_line = DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX;
    gdi::draw_str(mem, heading_font(), &q.heading, &mut line(y, h), one_line);
    y += h + s(GAP);
    let h = wrapped_height(font, &q.detail, inner);
    gdi::draw_str(mem, font, &q.detail, &mut line(y, h), wrap);
    y += h;
    if !q.path.is_empty() {
        SetTextColor(mem, dim);
        let h = wrapped_height(font, "Ag", inner);
        let flags = DT_SINGLELINE | DT_PATH_ELLIPSIS | DT_NOPREFIX;
        gdi::draw_str(mem, font, &q.path, &mut line(y, h), flags);
    }
    let (rows, focus, hot, is_checked) =
        state(|st| (st.rows.clone(), st.focus, st.hot, st.is_checked));
    let ring = themes::mix(colors.bg, colors.fg, FOCUS_MIX);
    for (i, (_, label, note)) in q.choices.iter().enumerate() {
        let row = rows[i];
        let mix = if hot == Some(i) { HOVER_MIX } else { ROW_MIX };
        gdi::round_fill(mem, &row, themes::mix(colors.bg, colors.fg, mix), s(RADIUS));
        if focus == i {
            draw_ring(mem, &row, ring);
        }
        let mut at = RECT {
            left: row.left + s(ROW_PAD_X),
            top: row.top + s(ROW_PAD_Y),
            right: row.right - s(ROW_PAD_X),
            bottom: row.bottom,
        };
        SetTextColor(mem, colors.fg);
        let label_h = wrapped_height(label_font(), label, at.right - at.left);
        // A choice with no note is a plain button: its label sits in the middle.
        let label_flags = if note.is_empty() {
            wrap | DT_CENTER
        } else {
            wrap
        };
        gdi::draw_str(mem, label_font(), label, &mut at.clone(), label_flags);
        at.top += label_h + s(NOTE_GAP);
        SetTextColor(mem, dim);
        gdi::draw_str(mem, font, note, &mut at, wrap);
    }
    if let Some(check) = &q.check {
        let row = rows[q.choices.len()];
        let bx = RECT {
            left: row.left,
            top: row.top + s(1),
            right: row.left + s(BOX),
            bottom: row.top + s(1) + s(BOX),
        };
        if is_checked {
            gdi::round_fill(mem, &bx, colors.fg, s(BOX_RADIUS));
            SetTextColor(mem, colors.bg);
            let mut tick = bx;
            let flags = DT_CENTER | DT_VCENTER | DT_SINGLELINE;
            gdi::draw_str(mem, font, "\u{2713}", &mut tick, flags);
        } else {
            gdi::draw_frame(mem, &bx, ring, 1);
        }
        if focus == q.choices.len() {
            let around = RECT {
                left: bx.left - s(3),
                top: bx.top - s(3),
                right: bx.right + s(3),
                bottom: bx.bottom + s(3),
            };
            draw_ring(mem, &around, ring);
        }
        SetTextColor(mem, colors.fg);
        let mut at = RECT {
            left: bx.right + s(BOX_GAP),
            ..row
        };
        gdi::draw_str(mem, font, check, &mut at, wrap);
    }
}

/// A thin rounded outline on `rc`, marking what Enter or Space picks.
unsafe fn draw_ring(mem: HDC, rc: &RECT, color: u32) {
    let pen = CreatePen(PS_SOLID, s(1).max(1), color);
    let old_pen = SelectObject(mem, pen as HGDIOBJ);
    let old_brush = SelectObject(mem, GetStockObject(NULL_BRUSH));
    RoundRect(
        mem,
        rc.left,
        rc.top,
        rc.right,
        rc.bottom,
        s(RADIUS),
        s(RADIUS),
    );
    SelectObject(mem, old_brush);
    SelectObject(mem, old_pen);
    DeleteObject(pen as HGDIOBJ);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn centered_on_the_window_not_the_screen() {
        let work = rect(0, 0, 1920, 1040);
        assert_eq!(
            place(&rect(100, 100, 900, 700), Some(&work), 400, 200),
            (300, 300)
        );
    }

    #[test]
    fn nudged_only_back_onto_the_monitor() {
        let work = rect(0, 0, 1920, 1040);
        // Hanging off the right and bottom edges.
        assert_eq!(
            place(&rect(1700, 900, 2100, 1300), Some(&work), 500, 300),
            (1420, 740)
        );
        // A minimized window's position: pulled to the top-left of the work area.
        let far = rect(-32000, -32000, -31840, -31972);
        assert_eq!(place(&far, Some(&work), 500, 300), (0, 0));
    }

    #[test]
    fn test_windows_keep_their_dialog_with_them() {
        assert_eq!(
            place(&rect(-20000, -20000, -19000, -19200), None, 500, 300),
            (-19750, -19750)
        );
    }
}
