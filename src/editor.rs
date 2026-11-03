//! The RichEdit control, used in plain-text mode.

use crate::richedit::*;
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::UI::Controls::{
    EM_EMPTYUNDOBUFFER, EM_GETLINECOUNT, EM_GETMODIFY, EM_LINEINDEX, EM_REPLACESEL, EM_SCROLLCARET,
    EM_SETMODIFY, EM_SETSEL,
};
use windows_sys::Win32::UI::Shell::DragAcceptFiles;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const MAX_CHARS: isize = 0x7FFF_FFFE;
const CODEPAGE_UTF16: u32 = 1200;
const NO_WRAP: isize = 1;
const WRAP_TO_WINDOW: isize = 0;
const EDIT_ID: usize = 1;
const CR: u16 = b'\r' as u16;

pub unsafe fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    let style = WS_CHILD
        | WS_VISIBLE
        | WS_VSCROLL
        | WS_HSCROLL
        | (ES_MULTILINE | ES_AUTOVSCROLL | ES_AUTOHSCROLL | ES_NOHIDESEL | ES_WANTRETURN) as u32;
    let edit = CreateWindowExW(
        0,
        w!("RICHEDIT50W"),
        null(),
        style,
        0,
        0,
        0,
        0,
        parent,
        EDIT_ID as _,
        hinst,
        null(),
    );
    let mode = TM_PLAINTEXT | TM_MULTILEVELUNDO | TM_MULTICODEPAGE;
    SendMessageW(edit, EM_SETTEXTMODE, mode as usize, 0);
    SendMessageW(edit, EM_EXLIMITTEXT, 0, MAX_CHARS);
    SendMessageW(
        edit,
        EM_SETEVENTMASK,
        0,
        (ENM_CHANGE | ENM_SELCHANGE | ENM_DROPFILES) as isize,
    );
    // Files dropped on the text arrive as EN_DROPFILES; see window.rs.
    DragAcceptFiles(edit, 1);
    let lang = SendMessageW(edit, EM_GETLANGOPTIONS, 0, 0) as u32;
    SendMessageW(
        edit,
        EM_SETLANGOPTIONS,
        0,
        (lang & !(IMF_AUTOFONT | IMF_DUALFONT)) as isize,
    );
    edit
}

pub unsafe fn set_wrap(edit: HWND, is_wrap: bool) {
    let mode = if is_wrap { WRAP_TO_WINDOW } else { NO_WRAP };
    SendMessageW(edit, EM_SETTARGETDEVICE, 0, mode);
}

/// Replaces all text and clears the undo history and modified flag.
pub unsafe fn set_text(edit: HWND, text: &[u16]) {
    let mut buf = Vec::with_capacity(text.len() + 1);
    buf.extend_from_slice(text);
    buf.push(0);
    SendMessageW(edit, WM_SETTEXT, 0, buf.as_ptr() as isize);
    SendMessageW(edit, EM_EMPTYUNDOBUFFER, 0, 0);
    SendMessageW(edit, EM_SETSEL, 0, 0);
    set_modified(edit, false);
}

/// All text with CRLF line breaks, no trailing NUL.
pub unsafe fn text(edit: HWND) -> Vec<u16> {
    let gtl = GETTEXTLENGTHEX {
        flags: GTL_USECRLF | GTL_PRECISE | GTL_NUMCHARS,
        codepage: CODEPAGE_UTF16,
    };
    let len = SendMessageW(edit, EM_GETTEXTLENGTHEX, &gtl as *const _ as usize, 0).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let gt = GETTEXTEX {
        cb: (buf.len() * 2) as u32,
        flags: GT_USECRLF,
        codepage: CODEPAGE_UTF16,
        lpDefaultChar: null(),
        lpUsedDefChar: null_mut(),
    };
    let got = SendMessageW(
        edit,
        EM_GETTEXTEX,
        &gt as *const _ as usize,
        buf.as_mut_ptr() as isize,
    );
    buf.truncate(got.max(0) as usize);
    buf
}

pub unsafe fn is_modified(edit: HWND) -> bool {
    SendMessageW(edit, EM_GETMODIFY, 0, 0) != 0
}

pub unsafe fn set_modified(edit: HWND, is_modified: bool) {
    SendMessageW(edit, EM_SETMODIFY, is_modified as usize, 0);
}

pub unsafe fn send(edit: HWND, msg: u32) {
    SendMessageW(edit, msg, 0, 0);
}

pub unsafe fn select_all(edit: HWND) {
    SendMessageW(edit, EM_SETSEL, 0, -1);
}

pub unsafe fn selection(edit: HWND) -> CHARRANGE {
    let mut range = CHARRANGE::default();
    SendMessageW(edit, EM_EXGETSEL, 0, &mut range as *mut _ as isize);
    range
}

/// Selects `range` and scrolls it into view.
pub unsafe fn select(edit: HWND, range: CHARRANGE) {
    SendMessageW(edit, EM_EXSETSEL, 0, &range as *const _ as isize);
    SendMessageW(edit, EM_SCROLLCARET, 0, 0);
}

// The control's own line messages count wrapped screen rows, so with word wrap on, lines are
// counted from the text instead (each line break is one CR there).

/// Line and column of the caret, both counted from 1.
pub unsafe fn caret_line_col(edit: HWND, is_wrap: bool) -> (usize, usize) {
    let caret = selection(edit).cpMax.max(0);
    if is_wrap {
        let before = text_range(edit, 0, caret);
        let line = before.iter().filter(|&&c| c == CR).count();
        let start = before.iter().rposition(|&c| c == CR).map_or(0, |i| i + 1);
        return (line + 1, before.len() - start + 1);
    }
    let line = SendMessageW(edit, EM_EXLINEFROMCHAR, 0, caret as isize).max(0);
    let start = SendMessageW(edit, EM_LINEINDEX, line as usize, 0).max(0);
    (
        line as usize + 1,
        (caret as isize - start).max(0) as usize + 1,
    )
}

pub unsafe fn line_count(edit: HWND, is_wrap: bool) -> usize {
    if is_wrap {
        return raw_text(edit).iter().filter(|&&c| c == CR).count() + 1;
    }
    SendMessageW(edit, EM_GETLINECOUNT, 0, 0).max(1) as usize
}

/// Character position where a 0-based line starts.
pub unsafe fn line_start(edit: HWND, line: usize, is_wrap: bool) -> i32 {
    if !is_wrap {
        return SendMessageW(edit, EM_LINEINDEX, line, 0).max(0) as i32;
    }
    if line == 0 {
        return 0;
    }
    let text = raw_text(edit);
    let mut breaks = text.iter().enumerate().filter(|(_, &c)| c == CR);
    breaks
        .nth(line - 1)
        .map_or(text.len() as i32, |(i, _)| i as i32 + 1)
}

/// Characters `from..to`, counted the way the control counts them.
unsafe fn text_range(edit: HWND, from: i32, to: i32) -> Vec<u16> {
    let len = (to - from).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let mut tr = TEXTRANGEW {
        chrg: CHARRANGE {
            cpMin: from,
            cpMax: to,
        },
        lpstrText: buf.as_mut_ptr(),
    };
    let got = SendMessageW(edit, EM_GETTEXTRANGE, 0, &mut tr as *mut _ as isize);
    buf.truncate(got.max(0) as usize);
    buf
}

/// The Ctrl+mouse wheel zoom, in percent.
pub unsafe fn zoom_percent(edit: HWND) -> u32 {
    let (mut num, mut den) = (0i32, 0i32);
    SendMessageW(
        edit,
        EM_GETZOOM,
        &mut num as *mut _ as usize,
        &mut den as *mut _ as isize,
    );
    if num <= 0 || den <= 0 {
        return 100;
    }
    (num as u32 * 100 + den as u32 / 2) / den as u32
}

/// Finds `needle` in `range` (search backward when cpMin > cpMax). `flags` are FR_* values.
pub unsafe fn find(edit: HWND, range: CHARRANGE, needle: &[u16], flags: u32) -> Option<CHARRANGE> {
    let mut text = needle.to_vec();
    text.push(0);
    let mut ft = FINDTEXTEXW {
        chrg: range,
        lpstrText: text.as_ptr(),
        chrgText: CHARRANGE::default(),
    };
    let pos = SendMessageW(
        edit,
        EM_FINDTEXTEXW,
        flags as usize,
        &mut ft as *mut _ as isize,
    );
    (pos >= 0).then_some(ft.chrgText)
}

/// Text as the control stores it (lone CR line breaks), so indexes match character positions.
pub unsafe fn raw_text(edit: HWND) -> Vec<u16> {
    let gtl = GETTEXTLENGTHEX {
        flags: GTL_PRECISE | GTL_NUMCHARS,
        codepage: CODEPAGE_UTF16,
    };
    let len = SendMessageW(edit, EM_GETTEXTLENGTHEX, &gtl as *const _ as usize, 0).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let gt = GETTEXTEX {
        cb: (buf.len() * 2) as u32,
        flags: GT_DEFAULT,
        codepage: CODEPAGE_UTF16,
        lpDefaultChar: null(),
        lpUsedDefChar: null_mut(),
    };
    let got = SendMessageW(
        edit,
        EM_GETTEXTEX,
        &gt as *const _ as usize,
        buf.as_mut_ptr() as isize,
    );
    buf.truncate(got.max(0) as usize);
    buf
}

pub unsafe fn text_len(edit: HWND) -> i32 {
    let gtl = GETTEXTLENGTHEX {
        flags: GTL_DEFAULT | GTL_PRECISE | GTL_NUMCHARS,
        codepage: CODEPAGE_UTF16,
    };
    SendMessageW(edit, EM_GETTEXTLENGTHEX, &gtl as *const _ as usize, 0).max(0) as i32
}

/// Replaces all text as one undo step, marking the document modified.
pub unsafe fn replace_all_text(edit: HWND, text: &[u16]) {
    let mut buf = text.to_vec();
    buf.push(0);
    let st = SETTEXTEX {
        flags: ST_KEEPUNDO,
        codepage: CODEPAGE_UTF16,
    };
    SendMessageW(
        edit,
        EM_SETTEXTEX,
        &st as *const _ as usize,
        buf.as_ptr() as isize,
    );
    set_modified(edit, true);
}

/// Replaces the selection; the change can be undone.
pub unsafe fn replace_selection(edit: HWND, text: &[u16]) {
    let mut buf = text.to_vec();
    buf.push(0);
    SendMessageW(edit, EM_REPLACESEL, 1, buf.as_ptr() as isize);
}

pub unsafe fn selected_text(edit: HWND) -> Vec<u16> {
    let range = selection(edit);
    let len = (range.cpMax - range.cpMin).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let got = SendMessageW(edit, EM_GETSELTEXT, 0, buf.as_mut_ptr() as isize);
    buf.truncate(got.max(0) as usize);
    buf
}
