//! The Scintilla editing control. It holds text as UTF-8; positions are byte offsets.

use std::ptr::null;
use windows_sys::w;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::sci::*;

const EDIT_ID: usize = 1;
const TAB_WIDTH: usize = 4;
const MARGIN_NUMBERS: usize = 0;
const MARGIN_FOLD: usize = 2;
const MIN_NUMBER_DIGITS: usize = 3;
const TEXT_PADDING: isize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub start: isize,
    pub end: isize,
}

impl Span {
    pub fn at(pos: isize) -> Self {
        Span {
            start: pos,
            end: pos,
        }
    }
}

unsafe fn send(edit: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    SendMessageW(edit, msg, wparam, lparam)
}

/// Registers the "Scintilla" window class. Call once before the first `create`.
pub unsafe fn register(hinst: HINSTANCE) {
    Scintilla_RegisterClasses(hinst);
}

pub unsafe fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    let edit = CreateWindowExW(
        0,
        w!("Scintilla"),
        null(),
        WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_HSCROLL | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        parent,
        EDIT_ID as _,
        hinst,
        null(),
    );
    send(edit, SCI_SETCODEPAGE, SC_CP_UTF8, 0);
    send(edit, SCI_SETTABWIDTH, TAB_WIDTH, 0);
    send(edit, SCI_USEPOPUP, SC_POPUP_TEXT, 0);
    send(edit, SCI_SETSCROLLWIDTH, 1, 0);
    send(edit, SCI_SETSCROLLWIDTHTRACKING, 1, 0);
    send(edit, SCI_SETMARGINLEFT, 0, TEXT_PADDING);
    send(edit, SCI_SETMARGINTYPEN, MARGIN_NUMBERS, SC_MARGIN_NUMBER);
    send(edit, SCI_SETMARGINWIDTHN, 1, 0);
    send(edit, SCI_SETMARGINWIDTHN, MARGIN_FOLD, 0);
    // Insert and delete only: they drive EN_CHANGE, which updates the tab and title.
    send(
        edit,
        SCI_SETMODEVENTMASK,
        SC_MOD_INSERTTEXT | SC_MOD_DELETETEXT,
        0,
    );
    edit
}

pub unsafe fn set_wrap(edit: HWND, is_wrap: bool) {
    let mode = if is_wrap { SC_WRAP_WORD } else { SC_WRAP_NONE };
    send(edit, SCI_SETWRAPMODE, mode, 0);
}

/// Line numbers in the left margin, wide enough for the last line's number.
pub unsafe fn show_line_numbers(edit: HWND, is_shown: bool) {
    let width = if is_shown {
        let digits = line_count(edit).to_string().len().max(MIN_NUMBER_DIGITS);
        let sample = format!("_{}\0", "9".repeat(digits));
        send(
            edit,
            SCI_TEXTWIDTH,
            STYLE_LINENUMBER,
            sample.as_ptr() as isize,
        )
    } else {
        0
    };
    send(edit, SCI_SETMARGINWIDTHN, MARGIN_NUMBERS, width);
}

/// Replaces all text and clears the undo history and modified state.
pub unsafe fn set_text(edit: HWND, text: &[u16], is_lf_only: bool) {
    let mut utf8 = String::from_utf16_lossy(text).into_bytes();
    utf8.push(0);
    send(edit, SCI_SETTEXT, 0, utf8.as_ptr() as isize);
    send(edit, SCI_EMPTYUNDOBUFFER, 0, 0);
    set_eol_mode(edit, is_lf_only);
    send(edit, SCI_GOTOPOS, 0, 0);
    set_saved(edit);
}

/// All text with the document's own line breaks.
pub unsafe fn text(edit: HWND) -> Vec<u16> {
    let len = text_len(edit) as usize;
    let mut buf = vec![0u8; len + 1];
    send(edit, SCI_GETTEXT, len + 1, buf.as_mut_ptr() as isize);
    buf.truncate(len);
    String::from_utf8_lossy(&buf).encode_utf16().collect()
}

/// The length in bytes.
pub unsafe fn text_len(edit: HWND) -> isize {
    send(edit, SCI_GETLENGTH, 0, 0)
}

pub unsafe fn is_modified(edit: HWND) -> bool {
    send(edit, SCI_GETMODIFY, 0, 0) != 0
}

/// Marks the text as matching the file on disk.
pub unsafe fn set_saved(edit: HWND) {
    send(edit, SCI_SETSAVEPOINT, 0, 0);
}

unsafe fn set_eol_mode(edit: HWND, is_lf_only: bool) {
    let mode = if is_lf_only { SC_EOL_LF } else { SC_EOL_CRLF };
    send(edit, SCI_SETEOLMODE, mode, 0);
}

/// Converts every line break to LF or CRLF, as one undo step, and types new ones that way.
pub unsafe fn convert_line_endings(edit: HWND, is_lf_only: bool) {
    set_eol_mode(edit, is_lf_only);
    let mode = if is_lf_only { SC_EOL_LF } else { SC_EOL_CRLF };
    send(edit, SCI_CONVERTEOLS, mode, 0);
}

pub unsafe fn undo(edit: HWND) {
    send(edit, SCI_UNDO, 0, 0);
}

pub unsafe fn redo(edit: HWND) {
    send(edit, SCI_REDO, 0, 0);
}

pub unsafe fn cut(edit: HWND) {
    send(edit, SCI_CUT, 0, 0);
}

pub unsafe fn copy(edit: HWND) {
    send(edit, SCI_COPY, 0, 0);
}

pub unsafe fn paste(edit: HWND) {
    send(edit, SCI_PASTE, 0, 0);
}

pub unsafe fn select_all(edit: HWND) {
    send(edit, SCI_SELECTALL, 0, 0);
}

pub unsafe fn selection(edit: HWND) -> Span {
    Span {
        start: send(edit, SCI_GETSELECTIONSTART, 0, 0),
        end: send(edit, SCI_GETSELECTIONEND, 0, 0),
    }
}

/// Selects `span` and scrolls it into view.
pub unsafe fn select(edit: HWND, span: Span) {
    send(edit, SCI_SETSEL, span.start as usize, span.end);
    send(edit, SCI_SCROLLCARET, 0, 0);
}

/// Line and column of the caret, both counted from 1. Columns count characters.
pub unsafe fn caret_line_col(edit: HWND) -> (usize, usize) {
    let caret = send(edit, SCI_GETCURRENTPOS, 0, 0);
    let line = send(edit, SCI_LINEFROMPOSITION, caret as usize, 0);
    let start = line_start(edit, line as usize);
    let col = send(edit, SCI_COUNTCHARACTERS, start as usize, caret);
    (line as usize + 1, col as usize + 1)
}

/// Characters (not bytes) in `span`.
pub unsafe fn char_count(edit: HWND, span: Span) -> usize {
    send(edit, SCI_COUNTCHARACTERS, span.start as usize, span.end).max(0) as usize
}

pub unsafe fn line_count(edit: HWND) -> usize {
    send(edit, SCI_GETLINECOUNT, 0, 0).max(1) as usize
}

/// Position where a 0-based line starts.
pub unsafe fn line_start(edit: HWND, line: usize) -> isize {
    send(edit, SCI_POSITIONFROMLINE, line, 0).max(0)
}

pub unsafe fn first_visible_line(edit: HWND) -> usize {
    send(edit, SCI_GETFIRSTVISIBLELINE, 0, 0).max(0) as usize
}

/// Scrolls so `line` (a display line) is at the top.
pub unsafe fn set_first_visible_line(edit: HWND, line: usize) {
    send(edit, SCI_SETFIRSTVISIBLELINE, line, 0);
}

/// Points added to every font size by Ctrl+wheel (negative when zoomed out).
pub unsafe fn zoom_points(edit: HWND) -> isize {
    send(edit, SCI_GETZOOM, 0, 0)
}

/// A regular expression Scintilla could not compile.
#[derive(Debug, PartialEq, Eq)]
pub struct BadPattern;

/// Finds `needle` between `from` and `to` (searches backward when from > to).
/// `flags` are SCFIND_* values.
pub unsafe fn find(
    edit: HWND,
    from: isize,
    to: isize,
    needle: &str,
    flags: usize,
) -> Result<Option<Span>, BadPattern> {
    send(edit, SCI_SETSEARCHFLAGS, flags, 0);
    send(edit, SCI_SETTARGETRANGE, from as usize, to);
    // The regex engines read the pattern up to a NUL and ignore the length.
    let pattern = nul_terminated(needle);
    send(edit, SCI_SETSTATUS, SC_STATUS_OK, 0);
    let start = send(
        edit,
        SCI_SEARCHINTARGET,
        needle.len(),
        pattern.as_ptr() as isize,
    );
    // A bad regex reads as "not found"; the status tells them apart.
    if send(edit, SCI_GETSTATUS, 0, 0) == SC_STATUS_WARN_REGEX {
        send(edit, SCI_SETSTATUS, SC_STATUS_OK, 0);
        return Err(BadPattern);
    }
    Ok((start >= 0).then(|| Span {
        start,
        end: send(edit, SCI_GETTARGETEND, 0, 0),
    }))
}

fn nul_terminated(s: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(s.len() + 1);
    bytes.extend_from_slice(s.as_bytes());
    bytes.push(0);
    bytes
}

/// The position one character after `pos` (a character can be several bytes).
pub unsafe fn position_after(edit: HWND, pos: isize) -> isize {
    send(edit, SCI_POSITIONAFTER, pos as usize, 0)
}

/// The position one character before `pos`.
pub unsafe fn position_before(edit: HWND, pos: isize) -> isize {
    send(edit, SCI_POSITIONBEFORE, pos as usize, 0)
}

/// Replaces the last match found by `find`. With a regex, \1 to \9 insert its groups.
/// Returns where the replacement ends.
pub unsafe fn replace_found(edit: HWND, with: &str, is_regex: bool) -> isize {
    let msg = if is_regex {
        SCI_REPLACETARGETRE
    } else {
        SCI_REPLACETARGET
    };
    let text = nul_terminated(with);
    send(edit, msg, with.len(), text.as_ptr() as isize);
    send(edit, SCI_GETTARGETEND, 0, 0)
}

/// Groups edits made between `begin_undo` and `end_undo` into one undo step.
pub unsafe fn begin_undo(edit: HWND) {
    send(edit, SCI_BEGINUNDOACTION, 0, 0);
}

pub unsafe fn end_undo(edit: HWND) {
    send(edit, SCI_ENDUNDOACTION, 0, 0);
}

/// Moves `pos` back to the start of the character it falls in, so it never splits a UTF-8
/// sequence or a CRLF. Positions past the end become the end.
pub unsafe fn snap(edit: HWND, pos: isize) -> isize {
    let len = text_len(edit);
    if pos >= len {
        return len;
    }
    position_before(edit, position_after(edit, pos.max(0)))
}

pub unsafe fn selected_text(edit: HWND) -> String {
    // A rectangular selection adds a line end per row, so ask for the size first.
    let len = send(edit, SCI_GETSELTEXT, 0, 0).max(0) as usize;
    let mut buf = vec![0u8; len + 1];
    send(edit, SCI_GETSELTEXT, 0, buf.as_mut_ptr() as isize);
    buf.truncate(len);
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    /// Scintilla is used from one thread in z++, so tests take turns.
    static SCINTILLA: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static REGISTER: std::sync::Once = std::sync::Once::new();

    /// A hidden Scintilla window for tests.
    unsafe fn test_editor(text: &str) -> HWND {
        let hinst = GetModuleHandleW(null());
        REGISTER.call_once(|| register(hinst));
        let edit = CreateWindowExW(
            0,
            w!("Scintilla"),
            null(),
            WS_POPUP,
            0,
            0,
            100,
            100,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        );
        assert!(!edit.is_null());
        send(edit, SCI_SETCODEPAGE, SC_CP_UTF8, 0);
        let units: Vec<u16> = text.encode_utf16().collect();
        set_text(edit, &units, true);
        edit
    }

    const REGEX: usize = SCFIND_REGEXP | SCFIND_CXX11REGEX;

    #[test]
    fn regex_find_and_replace_with_groups() {
        let _turn = SCINTILLA.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let edit = test_editor("beta two\ngamma three\n");
            let span = find(edit, 0, text_len(edit), r"(\w+) t(wo|hree)", REGEX)
                .unwrap()
                .unwrap();
            assert_eq!(span, Span { start: 0, end: 8 });
            replace_found(edit, r"[\1]", true);
            assert_eq!(
                String::from_utf16(&text(edit)).unwrap(),
                "[beta]\ngamma three\n"
            );
            assert_eq!(find(edit, 0, text_len(edit), "([", REGEX), Err(BadPattern));
            DestroyWindow(edit);
        }
    }

    #[test]
    fn regex_replace_all_on_crlf_text() {
        let _turn = SCINTILLA.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let edit = test_editor("OMEGA one\r\nbeta two\r\ngamma three\r\n");
            let found = find(edit, 0, text_len(edit), r"(\w+) t(wo|hree)", REGEX).unwrap();
            assert_eq!(found, Some(Span { start: 11, end: 19 }));
            DestroyWindow(edit);
        }
    }

    #[test]
    fn plain_find_backward_and_character_counts() {
        let _turn = SCINTILLA.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let edit = test_editor("héllo héllo");
            let len = text_len(edit);
            let back = find(edit, len, 0, "héllo", 0).unwrap().unwrap();
            assert_eq!(back.start, 7); // é is two bytes in UTF-8
            assert_eq!(char_count(edit, Span { start: 0, end: len }), 11);
            DestroyWindow(edit);
        }
    }

    #[test]
    fn snap_never_splits_a_character_or_crlf() {
        let _turn = SCINTILLA.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let edit = test_editor("hé\r\nx");
            assert_eq!(snap(edit, 2), 1); // inside é
            assert_eq!(snap(edit, 3), 3); // before CR
            assert_eq!(snap(edit, 4), 3); // between CR and LF
            assert_eq!(snap(edit, 5), 5);
            assert_eq!(snap(edit, 99), text_len(edit));
            DestroyWindow(edit);
        }
    }

    #[test]
    fn selected_text_fits_the_buffer() {
        let _turn = SCINTILLA.lock().unwrap_or_else(|e| e.into_inner());
        unsafe {
            let edit = test_editor("one héllo two");
            select(edit, Span { start: 4, end: 10 });
            assert_eq!(selected_text(edit), "héllo");
            DestroyWindow(edit);
        }
    }
}
