//! Dialogs built from templates in memory, so z++ needs no resource script for them and gets the
//! standard dialog keyboard handling (Enter, Escape, Tab) for free. Here: Go to Line.

use std::ptr::null_mut;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Controls::EM_SETSEL;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::wide;

pub const CLASS_BUTTON: u16 = 0x0080;
pub const CLASS_EDIT: u16 = 0x0081;
pub const CLASS_STATIC: u16 = 0x0082;
const FONT_POINTS: u16 = 9;
const FONT_FACE: &str = "Segoe UI";
/// Where the control count sits in a DLGTEMPLATE, in u16 units.
const COUNT_AT: usize = 4;

const ID_LABEL: u16 = 100;
const ID_INPUT: u16 = 101;
const GOTO_W: i16 = 190;
const GOTO_H: i16 = 64;
const MAX_INPUT: usize = 16;

/// A DLGTEMPLATE and its DLGITEMTEMPLATEs, in dialog units.
pub struct Template {
    t: Vec<u16>,
    count: u16,
}

impl Template {
    pub fn new(title: &str, style: u32, size: [i16; 2]) -> Self {
        let mut t = Vec::new();
        push_u32(&mut t, style | DS_SETFONT as u32);
        push_u32(&mut t, 0);
        t.push(0); // control count, set by add()
        for v in [0, 0, size[0], size[1]] {
            t.push(v as u16);
        }
        t.push(0); // no menu
        t.push(0); // default dialog class
        push_str(&mut t, title);
        t.push(FONT_POINTS);
        push_str(&mut t, FONT_FACE);
        Template { t, count: 0 }
    }

    pub fn add(&mut self, class: u16, id: u16, style: u32, rect: [i16; 4], text: &str) {
        let t = &mut self.t;
        if t.len() % 2 == 1 {
            t.push(0); // items start on a 4-byte boundary
        }
        push_u32(t, style | WS_CHILD | WS_VISIBLE);
        push_u32(t, 0);
        for v in rect {
            t.push(v as u16);
        }
        t.push(id);
        t.push(0xFFFF);
        t.push(class);
        push_str(t, text);
        t.push(0); // no creation data
        self.count += 1;
        t[COUNT_AT] = self.count;
    }

    /// Packed into u32s so the template starts on the 4-byte boundary Windows requires.
    pub fn finish(self) -> Vec<u32> {
        self.t
            .chunks(2)
            .map(|c| c[0] as u32 | (c.get(1).copied().unwrap_or(0) as u32) << 16)
            .collect()
    }
}

fn push_u32(t: &mut Vec<u16>, v: u32) {
    t.push(v as u16);
    t.push((v >> 16) as u16);
}

fn push_str(t: &mut Vec<u16>, s: &str) {
    t.extend(s.encode_utf16());
    t.push(0);
}

/// The text of a dialog control.
pub unsafe fn item_text(dialog: HWND, id: u16) -> String {
    let item = GetDlgItem(dialog, id as i32);
    let len = GetWindowTextLengthW(item).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let got = GetWindowTextW(item, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buf[..got])
}

pub unsafe fn set_item_text(dialog: HWND, id: u16, text: &str) {
    let text = wide(text);
    SetDlgItemTextW(dialog, id as i32, text.as_ptr());
}

/// Selects all of an edit box and puts the caret there.
pub unsafe fn focus_all(dialog: HWND, id: u16) {
    let input = GetDlgItem(dialog, id as i32);
    SendMessageW(input, EM_SETSEL, 0, -1);
    SetFocus(input);
}

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
    let style = DS_MODALFRAME as u32 | DS_CENTER as u32 | WS_POPUP | WS_CAPTION | WS_SYSMENU;
    let mut t = Template::new(title, style, [GOTO_W, GOTO_H]);
    t.add(CLASS_STATIC, ID_LABEL, 0, [7, 8, 120, 10], "");
    let input = WS_BORDER | WS_TABSTOP | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32;
    t.add(CLASS_EDIT, ID_INPUT, input, [7, 22, 176, 13], "");
    let ok = WS_TABSTOP | BS_DEFPUSHBUTTON as u32;
    t.add(CLASS_BUTTON, IDOK as u16, ok, [79, 43, 50, 14], "OK");
    let cancel = WS_TABSTOP | BS_PUSHBUTTON as u32;
    t.add(
        CLASS_BUTTON,
        IDCANCEL as u16,
        cancel,
        [133, 43, 50, 14],
        "Cancel",
    );
    let template = t.finish();
    let request = Request {
        label: label.to_string(),
        initial,
        max,
    };
    let result = DialogBoxIndirectParamW(
        null_mut(),
        template.as_ptr() as *const DLGTEMPLATE,
        owner,
        Some(number_proc),
        &request as *const Request as LPARAM,
    );
    (result > 0).then_some(result as usize)
}

unsafe extern "system" fn number_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    match msg {
        WM_INITDIALOG => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, lparam);
            let request = &*(lparam as *const Request);
            set_item_text(hwnd, ID_LABEL, &request.label);
            set_item_text(hwnd, ID_INPUT, &request.initial.to_string());
            focus_all(hwnd, ID_INPUT);
            0 // focus was set here
        }
        WM_COMMAND => {
            match (wparam & 0xFFFF) as i32 {
                IDOK => on_number_ok(hwnd),
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

unsafe fn on_number_ok(hwnd: HWND) {
    let request = &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Request);
    let text = item_text(hwnd, ID_INPUT);
    match text.trim().parse::<usize>() {
        Ok(n) if (1..=request.max).contains(&n) && text.len() <= MAX_INPUT => {
            EndDialog(hwnd, n as isize);
        }
        _ => focus_all(hwnd, ID_INPUT),
    }
}
