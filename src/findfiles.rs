//! Find in Files (Ctrl+Shift+F): a modeless dialog for the text, file filter and folder. The
//! search runs on a background thread (search.rs) and its results fill the panel (results.rs).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU64, Ordering};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Controls::{
    CheckDlgButton, IsDlgButtonChecked, BST_CHECKED, BST_UNCHECKED,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::Shell::{
    SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::dialog::{self, Template, CLASS_BUTTON, CLASS_EDIT, CLASS_STATIC};
use crate::search::{self, Outcome, Query};
use crate::window::{app, error_box};
use crate::{docs, results, statusbar, wide};

/// Posted to the main window when a search finishes: wparam is the search's generation,
/// lparam a Box<(Query, Outcome)> the receiver takes back.
pub const WM_APP_SEARCH_DONE: u32 = WM_APP + 9;

const ID_FIND_WHAT: u16 = 0x480;
const ID_FILTERS: u16 = 0x490;
const ID_FOLDER: u16 = 0x491;
const ID_BROWSE: u16 = 0x492;
const ID_WHOLE_WORD: u16 = 0x410;
const ID_MATCH_CASE: u16 = 0x411;
const ID_REGEX: u16 = 0x4A0;
const ID_FIND_ALL: u16 = IDOK as u16;
const ID_CLOSE: u16 = IDCANCEL as u16;
const DIALOG_SIZE: [i16; 2] = [300, 112];
const MAX_PATH_UNITS: usize = 1024;
const SEARCHING: &str = "Searching";

thread_local! {
    static QUERY: RefCell<Query> = RefCell::new(Query::default());
    static DIALOG: Cell<HWND> = const { Cell::new(null_mut()) };
}

/// Bumped by every search; a running search stops when it no longer matches.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn dialog() -> HWND {
    DIALOG.with(|d| d.get())
}

/// Opens the dialog with the selection as the text and the active file's folder.
pub unsafe fn open(owner: HWND, edit: HWND) {
    let selected = crate::editor::selected_text(edit);
    QUERY.with(|q| {
        let mut q = q.borrow_mut();
        if !selected.is_empty() && !selected.contains(['\r', '\n']) {
            q.needle = selected;
        }
        if q.folder.as_os_str().is_empty() {
            q.folder = default_folder();
        }
    });
    let mut hwnd = dialog();
    if hwnd.is_null() {
        hwnd = create(owner);
        DIALOG.with(|d| d.set(hwnd));
    }
    load(hwnd);
    ShowWindow(hwnd, SW_SHOW);
    dialog::focus_all(hwnd, ID_FIND_WHAT);
}

fn default_folder() -> PathBuf {
    let active = app(|a| {
        a.docs
            .borrow()
            .get(a.active.get())
            .and_then(|d| d.path.clone())
    });
    active
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default()
}

unsafe fn create(owner: HWND) -> HWND {
    let style = DS_MODALFRAME as u32 | DS_CENTER as u32 | WS_POPUP | WS_CAPTION | WS_SYSMENU;
    let mut t = Template::new("Find in Files", style, DIALOG_SIZE);
    let input = WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32;
    let check = WS_TABSTOP | BS_AUTOCHECKBOX as u32;
    let button = WS_TABSTOP | BS_PUSHBUTTON as u32;
    t.add(CLASS_STATIC, 0, 0, [7, 9, 44, 8], "Fi&nd what:");
    t.add(CLASS_EDIT, ID_FIND_WHAT, input, [52, 7, 176, 13], "");
    t.add(CLASS_STATIC, 0, 0, [7, 26, 44, 8], "F&iles:");
    t.add(CLASS_EDIT, ID_FILTERS, input, [52, 24, 176, 13], "");
    t.add(CLASS_STATIC, 0, 0, [7, 43, 44, 8], "F&older:");
    t.add(CLASS_EDIT, ID_FOLDER, input, [52, 41, 156, 13], "");
    t.add(CLASS_BUTTON, ID_BROWSE, button, [210, 41, 18, 13], "...");
    t.add(
        CLASS_BUTTON,
        ID_MATCH_CASE,
        check,
        [52, 60, 120, 10],
        "Match &case",
    );
    t.add(
        CLASS_BUTTON,
        ID_WHOLE_WORD,
        check,
        [52, 73, 120, 10],
        "Match &whole word only",
    );
    t.add(
        CLASS_BUTTON,
        ID_REGEX,
        check,
        [52, 86, 160, 10],
        "Regular e&xpression (Rust syntax)",
    );
    let default = WS_TABSTOP | BS_DEFPUSHBUTTON as u32;
    t.add(
        CLASS_BUTTON,
        ID_FIND_ALL,
        default,
        [236, 6, 57, 14],
        "&Find All",
    );
    t.add(CLASS_BUTTON, ID_CLOSE, button, [236, 23, 57, 14], "Close");
    let template = t.finish();
    CreateDialogIndirectParamW(
        null_mut(),
        template.as_ptr() as *const DLGTEMPLATE,
        owner,
        Some(dialog_proc),
        0,
    )
}

/// Copied out first: setting the boxes' text sends the dialog notifications right away.
unsafe fn load(hwnd: HWND) {
    let q = QUERY.with(|q| q.borrow().clone());
    dialog::set_item_text(hwnd, ID_FIND_WHAT, &q.needle);
    dialog::set_item_text(hwnd, ID_FILTERS, &q.filters);
    dialog::set_item_text(hwnd, ID_FOLDER, &q.folder.to_string_lossy());
    set_check(hwnd, ID_MATCH_CASE, q.is_match_case);
    set_check(hwnd, ID_WHOLE_WORD, q.is_whole_word);
    set_check(hwnd, ID_REGEX, q.is_regex);
}

unsafe fn save(hwnd: HWND) -> Query {
    let q = Query {
        needle: dialog::item_text(hwnd, ID_FIND_WHAT),
        filters: dialog::item_text(hwnd, ID_FILTERS),
        folder: PathBuf::from(dialog::item_text(hwnd, ID_FOLDER).trim()),
        is_match_case: is_checked(hwnd, ID_MATCH_CASE),
        is_whole_word: is_checked(hwnd, ID_WHOLE_WORD),
        is_regex: is_checked(hwnd, ID_REGEX),
    };
    QUERY.with(|cell| *cell.borrow_mut() = q.clone());
    q
}

unsafe fn set_check(hwnd: HWND, id: u16, is_on: bool) {
    let state = if is_on { BST_CHECKED } else { BST_UNCHECKED };
    CheckDlgButton(hwnd, id as i32, state);
}

unsafe fn is_checked(hwnd: HWND, id: u16) -> bool {
    IsDlgButtonChecked(hwnd, id as i32) == BST_CHECKED
}

unsafe extern "system" fn dialog_proc(hwnd: HWND, msg: u32, wparam: WPARAM, _: LPARAM) -> isize {
    if msg != WM_COMMAND || ((wparam >> 16) & 0xFFFF) as u32 != BN_CLICKED {
        return (msg == WM_INITDIALOG) as isize;
    }
    match (wparam & 0xFFFF) as u16 {
        ID_FIND_ALL => {
            let q = save(hwnd);
            if start(hwnd, q) {
                ShowWindow(hwnd, SW_HIDE);
                SetFocus(app(|a| a.edit.get()));
            }
        }
        ID_BROWSE => browse(hwnd),
        ID_CLOSE => {
            save(hwnd);
            ShowWindow(hwnd, SW_HIDE);
            SetFocus(app(|a| a.edit.get()));
        }
        _ => return 0,
    }
    1
}

/// Checks the query and starts the background search. False (after saying why) if it can't.
unsafe fn start(hwnd: HWND, q: Query) -> bool {
    if q.needle.is_empty() {
        dialog::focus_all(hwnd, ID_FIND_WHAT);
        return false;
    }
    if !q.folder.is_dir() {
        error_box(&format!("There is no folder {}.", q.folder.display()));
        dialog::focus_all(hwnd, ID_FOLDER);
        return false;
    }
    let re = match search::build_regex(&q) {
        Ok(re) => re,
        Err(e) => {
            error_box(&format!("That is not a valid regular expression:\n\n{e}"));
            dialog::focus_all(hwnd, ID_FIND_WHAT);
            return false;
        }
    };
    let mine = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    // HWNDs are not Send; the main window outlives the search or ignores its result.
    let main = app(|a| a.main.get()) as isize;
    statusbar::set_message(&format!("{SEARCHING} {} ...", q.folder.display()));
    std::thread::spawn(move || {
        let outcome = search::run(&q, &re, &GENERATION, mine);
        let result = Box::into_raw(Box::new((q, outcome)));
        let is_posted = unsafe {
            PostMessageW(
                main as HWND,
                WM_APP_SEARCH_DONE,
                mine as usize,
                result as isize,
            ) != 0
        };
        if !is_posted {
            drop(unsafe { Box::from_raw(result) });
        }
    });
    true
}

/// WM_APP_SEARCH_DONE: shows the results, unless a newer search has started since.
pub unsafe fn on_done(generation: usize, lparam: LPARAM) {
    let result = Box::from_raw(lparam as *mut (Query, Outcome));
    if generation as u64 != GENERATION.load(Ordering::Relaxed) || result.1.is_canceled {
        return;
    }
    let (q, outcome) = *result;
    results::set(&q, &outcome);
    statusbar::set_message("");
    crate::window::layout();
    crate::window::sync_menu_checks();
    docs::update_status();
}

/// Stops a running search, for example when the results panel is closed.
pub unsafe fn cancel() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    if statusbar::message().starts_with(SEARCHING) {
        statusbar::set_message("");
    }
}

unsafe fn browse(hwnd: HWND) {
    let title = wide("Search in this folder:");
    let info = BROWSEINFOW {
        hwndOwner: hwnd,
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
        ..std::mem::zeroed()
    };
    let list = SHBrowseForFolderW(&info);
    if list.is_null() {
        return;
    }
    let mut buf = vec![0u16; MAX_PATH_UNITS];
    if SHGetPathFromIDListW(list, buf.as_mut_ptr()) != 0 {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        dialog::set_item_text(hwnd, ID_FOLDER, &String::from_utf16_lossy(&buf[..len]));
    }
    CoTaskMemFree(list as *const _);
}
