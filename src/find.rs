//! Find and Replace through the standard Windows find dialog, plus F3 / Shift+F3.
//! The dialog is modeless: it reports button presses with the registered FINDMSGSTRING
//! message, which the main window passes to `on_message`.

use std::cell::Cell;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::Controls::Dialogs::{
    FindTextW, ReplaceTextW, FINDMSGSTRINGW, FINDREPLACEW, FR_DIALOGTERM, FR_FINDNEXT, FR_REPLACE,
    FR_REPLACEALL,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::editor;
use crate::richedit::{CHARRANGE, FR_DOWN, FR_MATCHCASE, FR_WHOLEWORD};
use crate::{statusbar, wide};

const BUF_LEN: usize = 512;
/// The "Find what" box in the standard find dialog (edt1 in dlgs.h).
const ID_FIND_WHAT: i32 = 0x480;
const NUL: u16 = 0;
const CR: u16 = b'\r' as u16;
const LF: u16 = b'\n' as u16;

struct FindState {
    fr: FINDREPLACEW,
    find: [u16; BUF_LEN],
    replace: [u16; BUF_LEN],
}

thread_local! {
    /// Leaked once: the dialog keeps pointers into it for as long as it is open.
    static STATE: Cell<*mut FindState> = const { Cell::new(null_mut()) };
    static DIALOG: Cell<HWND> = const { Cell::new(null_mut()) };
    static IS_REPLACE: Cell<bool> = const { Cell::new(false) };
    static MESSAGE_ID: Cell<u32> = const { Cell::new(0) };
}

unsafe fn state() -> &'static mut FindState {
    STATE.with(|s| {
        if s.get().is_null() {
            let mut fs: Box<FindState> = Box::new(zeroed());
            fs.fr.lStructSize = size_of::<FINDREPLACEW>() as u32;
            fs.fr.Flags = FR_DOWN;
            s.set(Box::into_raw(fs));
        }
        &mut *s.get()
    })
}

/// The open find dialog, for IsDialogMessageW in the message loop.
pub fn dialog() -> HWND {
    DIALOG.with(|d| d.get())
}

/// The registered message the find dialog sends to its owner.
pub fn message_id() -> u32 {
    MESSAGE_ID.with(|m| {
        if m.get() == 0 {
            m.set(unsafe { RegisterWindowMessageW(FINDMSGSTRINGW) });
        }
        m.get()
    })
}

/// Opens Find (Ctrl+F) or Replace (Ctrl+H), starting with a one-line selection as the search text.
pub unsafe fn open(owner: HWND, edit: HWND, is_replace: bool) {
    let st = state();
    let selected = editor::selected_text(edit);
    if !selected.is_empty()
        && selected.len() < BUF_LEN
        && !selected.iter().any(|&c| c == CR || c == LF)
    {
        st.find = [0; BUF_LEN];
        st.find[..selected.len()].copy_from_slice(&selected);
    }
    let open = dialog();
    if !open.is_null() {
        if IS_REPLACE.with(|r| r.get()) == is_replace {
            // Keep the open dialog's box in step with the search text taken from the selection.
            SetDlgItemTextW(open, ID_FIND_WHAT, st.find.as_ptr());
            SetFocus(open);
            return;
        }
        DestroyWindow(open);
    }
    st.fr.hwndOwner = owner;
    st.fr.lpstrFindWhat = st.find.as_mut_ptr();
    st.fr.wFindWhatLen = BUF_LEN as u16;
    st.fr.lpstrReplaceWith = st.replace.as_mut_ptr();
    st.fr.wReplaceWithLen = BUF_LEN as u16;
    st.fr.Flags &= FR_DOWN | FR_MATCHCASE | FR_WHOLEWORD;
    let hwnd = if is_replace {
        ReplaceTextW(&mut st.fr)
    } else {
        FindTextW(&mut st.fr)
    };
    DIALOG.with(|d| d.set(hwnd));
    IS_REPLACE.with(|r| r.set(is_replace));
}

/// Handles a button press in the find dialog.
pub unsafe fn on_message(edit: HWND, lparam: LPARAM) {
    let fr = &*(lparam as *const FINDREPLACEW);
    if fr.Flags & FR_DIALOGTERM != 0 {
        DIALOG.with(|d| d.set(null_mut()));
        return;
    }
    let is_down = fr.Flags & FR_DOWN != 0;
    if fr.Flags & FR_REPLACEALL != 0 {
        replace_all(edit);
    } else if fr.Flags & FR_REPLACE != 0 {
        replace_one(edit);
    } else if fr.Flags & FR_FINDNEXT != 0 {
        search(edit, is_down);
    }
}

/// F3 and Shift+F3: repeat the last search, or open Find if there is none.
pub unsafe fn find_again(owner: HWND, edit: HWND, is_forward: bool) {
    if needle().is_empty() {
        open(owner, edit, false);
        return;
    }
    search(edit, is_forward);
}

fn needle() -> Vec<u16> {
    let st = unsafe { state() };
    let len = st.find.iter().position(|&c| c == NUL).unwrap_or(BUF_LEN);
    st.find[..len].to_vec()
}

fn replacement() -> Vec<u16> {
    let st = unsafe { state() };
    let len = st.replace.iter().position(|&c| c == NUL).unwrap_or(BUF_LEN);
    st.replace[..len].to_vec()
}

/// FR_MATCHCASE and FR_WHOLEWORD as chosen in the dialog.
fn match_flags() -> u32 {
    unsafe { state() }.fr.Flags & (FR_MATCHCASE | FR_WHOLEWORD)
}

/// Selects the next match after the selection (or before it), wrapping around the file once.
unsafe fn search(edit: HWND, is_forward: bool) -> bool {
    let needle = needle();
    if needle.is_empty() {
        return false;
    }
    let sel = editor::selection(edit);
    let end = editor::text_len(edit);
    let flags = match_flags() | if is_forward { FR_DOWN } else { 0 };
    let (first, wrapped) = if is_forward {
        (
            CHARRANGE {
                cpMin: sel.cpMax,
                cpMax: -1,
            },
            CHARRANGE {
                cpMin: 0,
                cpMax: -1,
            },
        )
    } else {
        (
            CHARRANGE {
                cpMin: sel.cpMin,
                cpMax: 0,
            },
            CHARRANGE {
                cpMin: end,
                cpMax: 0,
            },
        )
    };
    if let Some(found) = editor::find(edit, first, &needle, flags) {
        editor::select(edit, found);
        statusbar::set_message("");
        return true;
    }
    if let Some(found) = editor::find(edit, wrapped, &needle, flags) {
        editor::select(edit, found);
        let side = if is_forward { "top" } else { "bottom" };
        statusbar::set_message(&format!("Search continued from the {side}"));
        return true;
    }
    not_found(&needle);
    false
}

unsafe fn not_found(needle: &[u16]) {
    let text = format!("Cannot find \"{}\".", String::from_utf16_lossy(needle));
    statusbar::set_message(&text);
    let owner = if dialog().is_null() {
        GetActiveWindow()
    } else {
        dialog()
    };
    let text = wide(&text);
    let caption = wide("z++");
    MessageBoxW(
        owner,
        text.as_ptr(),
        caption.as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}

/// Replace: if the selection is a match, replace it, then select the next match.
unsafe fn replace_one(edit: HWND) {
    let needle = needle();
    let sel = editor::selection(edit);
    let is_match = sel.cpMax > sel.cpMin
        && editor::find(edit, sel, &needle, match_flags() | FR_DOWN) == Some(sel);
    if is_match {
        editor::replace_selection(edit, &replacement());
    }
    search(edit, true);
}

/// Replace All, as a single undo step.
unsafe fn replace_all(edit: HWND) {
    let needle = needle();
    if needle.is_empty() {
        return;
    }
    let flags = match_flags() | FR_DOWN;
    let mut ranges = Vec::new();
    let mut from = 0;
    while let Some(found) = editor::find(
        edit,
        CHARRANGE {
            cpMin: from,
            cpMax: -1,
        },
        &needle,
        flags,
    ) {
        ranges.push(found);
        from = found.cpMax.max(found.cpMin + 1);
    }
    if ranges.is_empty() {
        not_found(&needle);
        return;
    }
    let caret = editor::selection(edit).cpMin;
    let text = splice(&editor::raw_text(edit), &ranges, &replacement());
    editor::replace_all_text(edit, &text);
    crate::docs::refresh(); // EM_SETTEXTEX sends no EN_CHANGE, so update the tab and title here
    let caret = caret.min(text.len() as i32);
    editor::select(
        edit,
        CHARRANGE {
            cpMin: caret,
            cpMax: caret,
        },
    );
    let count = ranges.len();
    let noun = if count == 1 { "match" } else { "matches" };
    statusbar::set_message(&format!("Replaced {count} {noun}"));
}

/// `text` with each range (sorted, not overlapping) replaced by `with`.
fn splice(text: &[u16], ranges: &[CHARRANGE], with: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0usize;
    for r in ranges {
        let (start, end) = (r.cpMin.max(0) as usize, r.cpMax.max(0) as usize);
        if start < at || end > text.len() {
            continue;
        }
        out.extend_from_slice(&text[at..start]);
        out.extend_from_slice(with);
        at = end;
    }
    out.extend_from_slice(&text[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn r(a: i32, b: i32) -> CHARRANGE {
        CHARRANGE { cpMin: a, cpMax: b }
    }

    #[test]
    fn splice_replaces_each_range() {
        let text = w("cat hat cat");
        assert_eq!(
            splice(&text, &[r(0, 3), r(8, 11)], &w("dog")),
            w("dog hat dog")
        );
    }

    #[test]
    fn splice_handles_empty_replacement_and_no_ranges() {
        let text = w("a-b-c");
        assert_eq!(splice(&text, &[r(1, 2), r(3, 4)], &[]), w("abc"));
        assert_eq!(splice(&text, &[], &w("x")), text);
    }

    #[test]
    fn splice_skips_bad_ranges() {
        let text = w("abc");
        assert_eq!(
            splice(&text, &[r(1, 2), r(0, 1), r(2, 9)], &w("X")),
            w("aXc")
        );
    }
}
