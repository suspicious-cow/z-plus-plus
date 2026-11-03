//! A small modal dialog that asks for a number (Go to Line). The dialog template is built in
//! memory, so z++ needs no resource script for it and gets the standard dialog keyboard
//! handling (Enter, Escape, Tab) for free.

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Controls::EM_SETSEL;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::wide;

const ID_LABEL: u16 = 100;
const ID_INPUT: u16 = 101;
const CLASS_BUTTON: u16 = 0x0080;
const CLASS_EDIT: u16 = 0x0081;
const CLASS_STATIC: u16 = 0x0082;
const FONT_POINTS: u16 = 9;
const FONT_FACE: &str = "Segoe UI";
const DIALOG_W: i16 = 190;
const DIALOG_H: i16 = 64;
const MAX_INPUT: usize = 16;

struct Request {
    label: String,
    initial: usize,
    max: usize,
}

/// Asks for a whole number from 1 to `max`. None if the user cancels.
pub unsafe fn ask_number(
    owner: HWND,
    title: &str,
    label: &str,
    initial: usize,
    max: usize,
) -> Option<usize> {
    let template = build_template(title);
    let request = Request {
        label: label.to_string(),
        initial,
        max,
    };
    let result = DialogBoxIndirectParamW(
        null_mut(),
        template.as_ptr() as *const DLGTEMPLATE,
        owner,
        Some(dialog_proc),
        &request as *const Request as LPARAM,
    );
    (result > 0).then_some(result as usize)
}

fn build_template(title: &str) -> Vec<u32> {
    let mut t = Vec::new();
    let style = DS_MODALFRAME as u32
        | DS_SETFONT as u32
        | DS_CENTER as u32
        | WS_POPUP
        | WS_CAPTION
        | WS_SYSMENU;
    push_u32(&mut t, style);
    push_u32(&mut t, 0);
    t.push(4); // controls
    for v in [0, 0, DIALOG_W, DIALOG_H] {
        t.push(v as u16);
    }
    t.push(0); // no menu
    t.push(0); // default dialog class
    push_str(&mut t, title);
    t.push(FONT_POINTS);
    push_str(&mut t, FONT_FACE);
    let child = WS_CHILD | WS_VISIBLE;
    add_item(&mut t, child, [7, 8, 120, 10], ID_LABEL, CLASS_STATIC, "");
    let input = child | WS_BORDER | WS_TABSTOP | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32;
    add_item(&mut t, input, [7, 22, 176, 13], ID_INPUT, CLASS_EDIT, "");
    let ok = child | WS_TABSTOP | BS_DEFPUSHBUTTON as u32;
    add_item(
        &mut t,
        ok,
        [79, 43, 50, 14],
        IDOK as u16,
        CLASS_BUTTON,
        "OK",
    );
    let cancel = child | WS_TABSTOP | BS_PUSHBUTTON as u32;
    add_item(
        &mut t,
        cancel,
        [133, 43, 50, 14],
        IDCANCEL as u16,
        CLASS_BUTTON,
        "Cancel",
    );
    // Packed into u32s so the template starts on the 4-byte boundary Windows requires.
    t.chunks(2)
        .map(|c| c[0] as u32 | (c.get(1).copied().unwrap_or(0) as u32) << 16)
        .collect()
}

fn add_item(t: &mut Vec<u16>, style: u32, rect: [i16; 4], id: u16, class: u16, text: &str) {
    if t.len() % 2 == 1 {
        t.push(0); // items start on a 4-byte boundary
    }
    push_u32(t, style);
    push_u32(t, 0);
    for v in rect {
        t.push(v as u16);
    }
    t.push(id);
    t.push(0xFFFF);
    t.push(class);
    push_str(t, text);
    t.push(0); // no creation data
}

fn push_u32(t: &mut Vec<u16>, v: u32) {
    t.push(v as u16);
    t.push((v >> 16) as u16);
}

fn push_str(t: &mut Vec<u16>, s: &str) {
    t.extend(s.encode_utf16());
    t.push(0);
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match msg {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
            let request = &*(lparam as *const Request);
            let label = wide(&request.label);
            SetDlgItemTextW(hwnd, ID_LABEL as i32, label.as_ptr());
            let initial = wide(&request.initial.to_string());
            SetDlgItemTextW(hwnd, ID_INPUT as i32, initial.as_ptr());
            let input = GetDlgItem(hwnd, ID_INPUT as i32);
            SendMessageW(input, EM_SETSEL, 0, -1);
            SetFocus(input);
            0 // focus was set here
        }
        WM_COMMAND => {
            match (wparam & 0xFFFF) as i32 {
                IDOK => on_ok(hwnd),
                IDCANCEL => {
                    EndDialog(hwnd, 0);
                }
                _ => {}
            }
            1
        }
        _ => 0,
    }
}

unsafe fn on_ok(hwnd: HWND) {
    let request = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Request);
    let mut buf = [0u16; MAX_INPUT];
    let len = GetDlgItemTextW(hwnd, ID_INPUT as i32, buf.as_mut_ptr(), buf.len() as i32);
    let value = String::from_utf16_lossy(&buf[..len as usize])
        .trim()
        .parse::<usize>();
    match value {
        Ok(n) if (1..=request.max).contains(&n) => {
            EndDialog(hwnd, n as isize);
        }
        _ => {
            let input = GetDlgItem(hwnd, ID_INPUT as i32);
            SendMessageW(input, EM_SETSEL, 0, -1);
            SetFocus(input);
        }
    }
}
