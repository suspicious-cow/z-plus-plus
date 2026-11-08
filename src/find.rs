//! Find and Replace: a modeless dialog (Ctrl+F, Ctrl+H) plus F3 / Shift+F3. Searches can match
//! case, whole words, or a regular expression (ECMAScript syntax; \1 to \9 in the replacement).

use std::cell::{Cell, RefCell};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Controls::{
    CheckDlgButton, IsDlgButtonChecked, BST_CHECKED, BST_UNCHECKED,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::dialog::{self, Template, CLASS_BUTTON, CLASS_EDIT, CLASS_STATIC};
use crate::editor::{self, BadPattern, Span};
use crate::sci::{SCFIND_CXX11REGEX, SCFIND_MATCHCASE, SCFIND_REGEXP, SCFIND_WHOLEWORD};
use crate::window::app;
use crate::{docs, statusbar, wide};

// The find and replace boxes keep the IDs of the standard Windows find dialog (dlgs.h).
const ID_FIND_WHAT: u16 = 0x480;
const ID_REPLACE_WITH: u16 = 0x481;
const ID_REPLACE_LABEL: u16 = 0x482;
const ID_FIND_LABEL: u16 = 0x483;
const ID_WHOLE_WORD: u16 = 0x410;
const ID_MATCH_CASE: u16 = 0x411;
const ID_REGEX: u16 = 0x4A0;
const ID_FIND_PREV: u16 = 0x4A1;
const ID_REPLACE: u16 = 0x400;
const ID_REPLACE_ALL: u16 = 0x401;
const ID_FIND_NEXT: u16 = IDOK as u16;
const ID_CLOSE: u16 = IDCANCEL as u16;
const DIALOG_SIZE: [i16; 2] = [262, 96];
const REPLACE_ONLY: [u16; 4] = [
    ID_REPLACE_LABEL,
    ID_REPLACE_WITH,
    ID_REPLACE,
    ID_REPLACE_ALL,
];

#[derive(Default, Clone)]
struct Options {
    needle: String,
    replacement: String,
    is_match_case: bool,
    is_whole_word: bool,
    is_regex: bool,
}

thread_local! {
    static OPTIONS: RefCell<Options> = RefCell::new(Options::default());
    static DIALOG: Cell<HWND> = const { Cell::new(null_mut()) };
}

/// The open find dialog, for IsDialogMessageW in the message loop.
pub fn dialog() -> HWND {
    DIALOG.with(|d| d.get())
}

/// Opens Find (Ctrl+F) or Replace (Ctrl+H), starting with a one-line selection as the search text.
pub unsafe fn open(owner: HWND, edit: HWND, is_replace: bool) {
    let selected = editor::selected_text(edit);
    if !selected.is_empty() && !selected.contains(['\r', '\n']) {
        OPTIONS.with(|o| o.borrow_mut().needle = selected);
    }
    let mut hwnd = dialog();
    if hwnd.is_null() {
        hwnd = create(owner);
        DIALOG.with(|d| d.set(hwnd));
    }
    show_mode(hwnd, is_replace);
    load_options(hwnd);
    ShowWindow(hwnd, SW_SHOW);
    dialog::focus_all(hwnd, ID_FIND_WHAT);
}

unsafe fn create(owner: HWND) -> HWND {
    let style = DS_MODALFRAME as u32 | DS_CENTER as u32 | WS_POPUP | WS_CAPTION | WS_SYSMENU;
    let mut t = Template::new("Find", style, DIALOG_SIZE);
    let input = WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32;
    let check = WS_TABSTOP | BS_AUTOCHECKBOX as u32;
    let button = WS_TABSTOP | BS_PUSHBUTTON as u32;
    t.add(CLASS_STATIC, ID_FIND_LABEL, 0, [7, 9, 52, 8], "Fi&nd what:");
    t.add(CLASS_EDIT, ID_FIND_WHAT, input, [60, 7, 130, 13], "");
    t.add(
        CLASS_STATIC,
        ID_REPLACE_LABEL,
        0,
        [7, 26, 52, 8],
        "Re&place with:",
    );
    t.add(CLASS_EDIT, ID_REPLACE_WITH, input, [60, 24, 130, 13], "");
    t.add(
        CLASS_BUTTON,
        ID_MATCH_CASE,
        check,
        [7, 46, 120, 10],
        "Match &case",
    );
    t.add(
        CLASS_BUTTON,
        ID_WHOLE_WORD,
        check,
        [7, 59, 120, 10],
        "Match &whole word only",
    );
    t.add(
        CLASS_BUTTON,
        ID_REGEX,
        check,
        [7, 72, 120, 10],
        "Regular e&xpression",
    );
    let default = WS_TABSTOP | BS_DEFPUSHBUTTON as u32;
    t.add(
        CLASS_BUTTON,
        ID_FIND_NEXT,
        default,
        [198, 6, 57, 14],
        "&Find Next",
    );
    t.add(
        CLASS_BUTTON,
        ID_FIND_PREV,
        button,
        [198, 23, 57, 14],
        "Find Pre&vious",
    );
    t.add(
        CLASS_BUTTON,
        ID_REPLACE,
        button,
        [198, 40, 57, 14],
        "&Replace",
    );
    t.add(
        CLASS_BUTTON,
        ID_REPLACE_ALL,
        button,
        [198, 57, 57, 14],
        "Replace &All",
    );
    t.add(CLASS_BUTTON, ID_CLOSE, button, [198, 74, 57, 14], "Close");
    let template = t.finish();
    CreateDialogIndirectParamW(
        null_mut(),
        template.as_ptr() as *const DLGTEMPLATE,
        owner,
        Some(dialog_proc),
        0,
    )
}

unsafe fn show_mode(hwnd: HWND, is_replace: bool) {
    let show = if is_replace { SW_SHOW } else { SW_HIDE };
    for id in REPLACE_ONLY {
        ShowWindow(GetDlgItem(hwnd, id as i32), show);
    }
    let title = wide(if is_replace { "Replace" } else { "Find" });
    SetWindowTextW(hwnd, title.as_ptr());
}

/// Copied out first: setting the boxes' text sends the dialog notifications right away.
unsafe fn load_options(hwnd: HWND) {
    let o = OPTIONS.with(|o| o.borrow().clone());
    dialog::set_item_text(hwnd, ID_FIND_WHAT, &o.needle);
    dialog::set_item_text(hwnd, ID_REPLACE_WITH, &o.replacement);
    set_check(hwnd, ID_MATCH_CASE, o.is_match_case);
    set_check(hwnd, ID_WHOLE_WORD, o.is_whole_word);
    set_check(hwnd, ID_REGEX, o.is_regex);
}

unsafe fn save_options(hwnd: HWND) {
    let options = Options {
        needle: dialog::item_text(hwnd, ID_FIND_WHAT),
        replacement: dialog::item_text(hwnd, ID_REPLACE_WITH),
        is_match_case: is_checked(hwnd, ID_MATCH_CASE),
        is_whole_word: is_checked(hwnd, ID_WHOLE_WORD),
        is_regex: is_checked(hwnd, ID_REGEX),
    };
    OPTIONS.with(|o| *o.borrow_mut() = options);
}

unsafe fn set_check(hwnd: HWND, id: u16, is_on: bool) {
    let state = if is_on { BST_CHECKED } else { BST_UNCHECKED };
    CheckDlgButton(hwnd, id as i32, state);
}

unsafe fn is_checked(hwnd: HWND, id: u16) -> bool {
    IsDlgButtonChecked(hwnd, id as i32) == BST_CHECKED
}

unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, wparam: WPARAM, _: LPARAM) -> isize {
    match msg {
        WM_INITDIALOG => 1,
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u16;
            if ((wparam >> 16) & 0xFFFF) as u32 != BN_CLICKED {
                return 0; // typing in the boxes
            }
            if id == ID_CLOSE {
                save_options(hwnd);
                ShowWindow(hwnd, SW_HIDE);
                SetFocus(app(|a| a.edit.get()));
                return 1;
            }
            let edit = app(|a| a.edit.get());
            save_options(hwnd);
            match id {
                ID_FIND_NEXT => {
                    search(edit, true);
                }
                ID_FIND_PREV => {
                    search(edit, false);
                }
                ID_REPLACE => replace_one(edit),
                ID_REPLACE_ALL => replace_all(edit),
                _ => return 0,
            }
            1
        }
        _ => 0,
    }
}

/// F3 and Shift+F3: repeat the last search, or open Find if there is none.
pub unsafe fn find_again(owner: HWND, edit: HWND, is_forward: bool) {
    if OPTIONS.with(|o| o.borrow().needle.is_empty()) {
        open(owner, edit, false);
        return;
    }
    search(edit, is_forward);
}

fn flags() -> usize {
    OPTIONS.with(|o| {
        let o = o.borrow();
        let mut flags = 0;
        if o.is_match_case {
            flags |= SCFIND_MATCHCASE;
        }
        if o.is_whole_word {
            flags |= SCFIND_WHOLEWORD;
        }
        if o.is_regex {
            flags |= SCFIND_REGEXP | SCFIND_CXX11REGEX;
        }
        flags
    })
}

fn needle() -> String {
    OPTIONS.with(|o| o.borrow().needle.clone())
}

/// Selects the next match after the selection (or before it), wrapping around the file once.
unsafe fn search(edit: HWND, is_forward: bool) -> bool {
    let needle = needle();
    if needle.is_empty() {
        return false;
    }
    let sel = editor::selection(edit);
    let end = editor::text_len(edit);
    let (first, wrapped) = if is_forward {
        ((sel.end, end), (0, end))
    } else {
        ((sel.start, 0), (end, 0))
    };
    let flags = flags();
    let found = match find_moving(edit, first, sel, &needle, flags) {
        Err(BadPattern) => return bad_pattern(),
        Ok(Some(span)) => Some((span, false)),
        Ok(None) => match find_moving(edit, wrapped, sel, &needle, flags) {
            Err(BadPattern) => return bad_pattern(),
            Ok(found) => found.map(|span| (span, true)),
        },
    };
    let Some((span, is_wrapped)) = found else {
        not_found(&needle);
        return false;
    };
    editor::select(edit, span);
    let side = if is_forward { "top" } else { "bottom" };
    let message = if is_wrapped {
        format!("Search continued from the {side}")
    } else {
        String::new()
    };
    statusbar::set_message(&message);
    true
}

/// Like `editor::find`, but an empty match (a regex like ^ or $) equal to the current empty
/// selection is skipped, so Find Next always moves.
unsafe fn find_moving(
    edit: HWND,
    (from, to): (isize, isize),
    sel: Span,
    needle: &str,
    flags: usize,
) -> Result<Option<Span>, BadPattern> {
    let found = editor::find(edit, from, to, needle, flags)?;
    if found != Some(sel) || sel.start != sel.end {
        return Ok(found);
    }
    let next = if from <= to {
        editor::position_after(edit, from)
    } else {
        editor::position_before(edit, from)
    };
    if next == from {
        return Ok(None);
    }
    editor::find(edit, next, to, needle, flags)
}

unsafe fn bad_pattern() -> bool {
    message("That is not a valid regular expression.");
    false
}

unsafe fn not_found(needle: &str) {
    message(&format!("Cannot find \"{needle}\"."));
}

unsafe fn message(text: &str) {
    statusbar::set_message(text);
    let owner = if IsWindowVisible(dialog()) != 0 {
        dialog()
    } else {
        GetActiveWindow()
    };
    let text = wide(text);
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
    let (needle, with, is_regex) = OPTIONS.with(|o| {
        let o = o.borrow();
        (o.needle.clone(), o.replacement.clone(), o.is_regex)
    });
    if needle.is_empty() {
        return;
    }
    // An empty selection can be an empty regex match too (from ^ or $).
    let sel = editor::selection(edit);
    match editor::find(edit, sel.start, sel.end, &needle, flags()) {
        Ok(Some(span)) if span == sel => {
            let end = editor::replace_found(edit, &with, is_regex);
            editor::select(edit, Span::at(end));
        }
        Err(BadPattern) => {
            bad_pattern();
            return;
        }
        _ => {}
    }
    search(edit, true);
}

/// Replace All, as a single undo step.
unsafe fn replace_all(edit: HWND) {
    let (needle, with, is_regex) = OPTIONS.with(|o| {
        let o = o.borrow();
        (o.needle.clone(), o.replacement.clone(), o.is_regex)
    });
    if needle.is_empty() {
        return;
    }
    let flags = flags();
    let caret = editor::selection(edit).start;
    let Some(count) = docs::batch_edit(edit, || replace_each(edit, &needle, &with, is_regex, flags))
    else {
        bad_pattern();
        return;
    };
    if count == 0 {
        not_found(&needle);
        return;
    }
    editor::select(edit, Span::at(editor::snap(edit, caret)));
    let noun = if count == 1 { "match" } else { "matches" };
    statusbar::set_message(&format!("Replaced {count} {noun}"));
}

/// Replaces every match as one undo step. None if the pattern is not a valid regex.
unsafe fn replace_each(
    edit: HWND,
    needle: &str,
    with: &str,
    is_regex: bool,
    flags: usize,
) -> Option<usize> {
    let mut count = 0usize;
    let mut from = 0;
    editor::begin_undo(edit);
    loop {
        let span = match editor::find(edit, from, editor::text_len(edit), needle, flags) {
            Ok(Some(span)) => span,
            Ok(None) => break,
            Err(BadPattern) => {
                editor::end_undo(edit);
                return None;
            }
        };
        let end = editor::replace_found(edit, with, is_regex);
        count += 1;
        // An empty match (a regex like ^) would match the same spot forever: step past it.
        from = if span.start == span.end {
            editor::position_after(edit, end)
        } else {
            end
        };
        if from >= editor::text_len(edit) && span.start == span.end {
            break;
        }
    }
    editor::end_undo(edit);
    Some(count)
}
