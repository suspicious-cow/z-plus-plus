//! The RichEdit control, used in plain-text mode.

use crate::richedit::*;
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::UI::Controls::{EM_EMPTYUNDOBUFFER, EM_GETMODIFY, EM_SETMODIFY, EM_SETSEL};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const MAX_CHARS: isize = 0x7FFF_FFFE;
const CODEPAGE_UTF16: u32 = 1200;
const NO_WRAP: isize = 1;
const WRAP_TO_WINDOW: isize = 0;
const EDIT_ID: usize = 1;

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
    SendMessageW(edit, EM_SETEVENTMASK, 0, ENM_CHANGE as isize);
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
