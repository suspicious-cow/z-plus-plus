//! Open documents: one tab and one editor control per file. Opening, saving, closing,
//! reloading after outside changes, recent files, the saved session, and encoding changes.

use std::ffi::OsString;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::file::{self, Encoding, Format, Stamp};
use crate::richedit::CHARRANGE;
use crate::window::{self, app, ask_yes_no_cancel, error_box, info_box, Answer};
use crate::{editor, menu, prefs, statusbar, tabs, theme};

const UNTITLED: &str = "Untitled";
const PATH_BUF_LEN: usize = 64 * 1024;
const FILE_FILTER: &str = "All files\0*.*\0Text files\0*.txt\0\0";

pub struct Doc {
    pub edit: HWND,
    pub path: Option<PathBuf>,
    pub format: Format,
    untitled: u32,
    /// The file on disk when z++ last read or wrote it.
    stamp: Option<Stamp>,
    /// Which look (colors, font) the editor was last styled with; see window::paint_look.
    pub look_gen: u32,
    shown_modified: bool,
}

pub fn active_index() -> usize {
    app(|a| a.active.get())
}

pub fn active_format() -> Format {
    app(|a| {
        a.docs
            .borrow()
            .get(a.active.get())
            .map(|d| d.format)
            .unwrap_or_default()
    })
}

fn edit_of(index: usize) -> HWND {
    app(|a| a.docs.borrow()[index].edit)
}

fn count() -> usize {
    app(|a| a.docs.borrow().len())
}

fn label(doc: &Doc) -> String {
    match &doc.path {
        Some(p) => p.file_name().map_or_else(
            || p.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
        None => format!("{UNTITLED} {}", doc.untitled),
    }
}

/// Opens an empty Untitled tab and switches to it.
pub unsafe fn new_tab() -> usize {
    let (main, is_wrap) = app(|a| (a.main.get(), a.is_wrap.get()));
    let edit = editor::create(main, GetModuleHandleW(null()));
    ShowWindow(edit, SW_HIDE);
    editor::set_wrap(edit, is_wrap);
    let index = app(|a| {
        let untitled = a.next_untitled.get();
        a.next_untitled.set(untitled + 1);
        let mut docs = a.docs.borrow_mut();
        docs.push(Doc {
            edit,
            path: None,
            format: Format::default(),
            untitled,
            stamp: None,
            look_gen: 0,
            shown_modified: false,
        });
        docs.len() - 1
    });
    activate(index);
    index
}

/// Shows tab `index` and hides the one that was showing.
pub unsafe fn activate(index: usize) {
    if index >= count() {
        return;
    }
    let old = app(|a| a.edit.get());
    let new = edit_of(index);
    app(|a| {
        a.active.set(index);
        a.edit.set(new);
    });
    style_if_stale(index);
    window::layout();
    ShowWindow(new, SW_SHOW);
    if old != new && !old.is_null() {
        ShowWindow(old, SW_HIDE);
    }
    SetFocus(new);
    refresh();
    check_disk(index);
}

/// Each tab picks up a new theme or font the first time it is shown afterward.
unsafe fn style_if_stale(index: usize) {
    let gen = app(|a| a.look_gen.get());
    if app(|a| a.docs.borrow()[index].look_gen) == gen {
        return;
    }
    let (colors, face, size) = window::look_parts();
    theme::style_editor(
        edit_of(index),
        &theme::Look {
            colors,
            face: &face,
            size_tenths: size,
        },
    );
    app(|a| a.docs.borrow_mut()[index].look_gen = gen);
}

pub unsafe fn cycle(step: isize) {
    let n = count() as isize;
    if n > 1 {
        activate(((active_index() as isize + step).rem_euclid(n)) as usize);
    }
}

/// Tabs, title, status bar and menu checks after anything about the documents changes.
pub unsafe fn refresh() {
    let (list, active) = app(|a| {
        let mut docs = a.docs.borrow_mut();
        let list = docs
            .iter_mut()
            .map(|d| {
                d.shown_modified = editor::is_modified(d.edit);
                tabs::Tab {
                    label: label(d),
                    is_modified: d.shown_modified,
                }
            })
            .collect();
        (list, a.active.get())
    });
    tabs::set(list, active);
    update_title();
    update_status();
    window::sync_menu_checks();
}

/// EN_CHANGE from one of the editors: repaint its tab only when its modified mark flips.
pub unsafe fn on_edit_change(edit: HWND) {
    let flipped = app(|a| {
        a.docs
            .borrow()
            .iter()
            .any(|d| d.edit == edit && d.shown_modified != editor::is_modified(edit))
    });
    statusbar::set_message("");
    if flipped {
        refresh();
    } else if edit == app(|a| a.edit.get()) {
        update_status();
    }
}

pub unsafe fn update_title() {
    let (main, title) = app(|a| {
        let docs = a.docs.borrow();
        let title = docs.get(a.active.get()).map(|d| {
            let marker = if editor::is_modified(d.edit) { "*" } else { "" };
            let name = d
                .path
                .as_ref()
                .map_or_else(|| label(d), |p| p.display().to_string());
            format!("{marker}{name} - z++")
        });
        (a.main.get(), title)
    });
    if let Some(title) = title {
        let title = crate::wide(&title);
        SetWindowTextW(main, title.as_ptr());
    }
}

pub unsafe fn update_status() {
    let edit = app(|a| a.edit.get());
    if edit.is_null() {
        return;
    }
    let (line, col) = editor::caret_line_col(edit, app(|a| a.is_wrap.get()));
    let sel = editor::selection(edit);
    let selected = (sel.cpMax - sel.cpMin).max(0);
    let mut position = format!("Ln {line}, Col {col}");
    if selected > 0 {
        position.push_str(&format!("    {selected} selected"));
    }
    let format = active_format();
    statusbar::set(statusbar::Info {
        position,
        message: statusbar::message(),
        zoom: format!("{}%", editor::zoom_percent(edit)),
        line_endings: file::line_ending_label(format).to_string(),
        encoding: file::encoding_label(format.encoding).to_string(),
    });
}

/// File > Open: one or more files, each in its own tab.
pub unsafe fn cmd_open() {
    for path in file_dialog(false) {
        open_path(&path);
    }
}

/// Opens `path` in a tab, or switches to it if it is already open. Reuses an empty Untitled tab.
pub unsafe fn open_path(path: &Path) -> bool {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(index) = find_open(&path) {
        activate(index);
        return true;
    }
    let Some((text, format)) = read(&path) else {
        return false;
    };
    let index = if is_pristine(active_index()) {
        active_index()
    } else {
        new_tab()
    };
    let edit = edit_of(index);
    editor::set_text(edit, &text);
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        let doc = &mut docs[index];
        doc.path = Some(path.clone());
        doc.format = format;
        doc.stamp = file::stamp(&path);
    });
    add_recent(&path);
    activate(index);
    if format.is_mixed_eol {
        let eol = file::line_ending_label(format);
        statusbar::set_message(&format!(
            "Mixed line endings: saving will use {eol} throughout"
        ));
    }
    true
}

/// The decoded file, or None after telling the user why it cannot be opened.
unsafe fn read(path: &Path) -> Option<(Vec<u16>, Format)> {
    if path.is_dir() {
        return None;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            error_box(&format!("Could not open {}:\n{e}", path.display()));
            return None;
        }
    };
    match file::decode(&bytes) {
        Ok(decoded) => Some(decoded),
        Err(why) => {
            error_box(&format!(
                "z++ will not open {} because {why}.",
                path.display()
            ));
            None
        }
    }
}

fn find_open(path: &Path) -> Option<usize> {
    let key = path.to_string_lossy().to_lowercase();
    app(|a| {
        a.docs.borrow().iter().position(|d| {
            d.path
                .as_ref()
                .is_some_and(|p| p.to_string_lossy().to_lowercase() == key)
        })
    })
}

/// An Untitled tab nobody has typed in yet.
unsafe fn is_pristine(index: usize) -> bool {
    let (path_none, edit) = app(|a| {
        let docs = a.docs.borrow();
        docs.get(index)
            .map_or((false, null_mut()), |d| (d.path.is_none(), d.edit))
    });
    path_none && !editor::is_modified(edit) && editor::text_len(edit) == 0
}

pub unsafe fn save(index: usize) -> bool {
    match app(|a| a.docs.borrow()[index].path.clone()) {
        Some(p) => save_to(index, &p),
        None => save_as(index),
    }
}

pub unsafe fn save_as(index: usize) -> bool {
    activate(index);
    match file_dialog(true).into_iter().next() {
        Some(p) => {
            let is_saved = save_to(index, &p);
            if is_saved {
                add_recent(&p);
            }
            is_saved
        }
        None => false,
    }
}

pub unsafe fn save_all() {
    for i in 0..count() {
        if editor::is_modified(edit_of(i)) && !save(i) {
            return;
        }
    }
}

unsafe fn save_to(index: usize, path: &Path) -> bool {
    let (edit, format) = app(|a| {
        let d = &a.docs.borrow()[index];
        (d.edit, d.format)
    });
    let (bytes, used) = file::encode(&editor::text(edit), format);
    if let Err(e) = file::write_safely(path, &bytes) {
        error_box(&format!("Could not save {}:\n{e}", path.display()));
        return false;
    }
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        let doc = &mut docs[index];
        doc.format = used;
        doc.path = Some(path.to_path_buf());
        doc.stamp = file::stamp(path);
    });
    editor::set_modified(edit, false);
    refresh();
    if used.encoding != format.encoding {
        info_box(&format!(
            "Some characters are not in the {} code page, so the file was saved as {}.",
            file::encoding_label(format.encoding),
            file::encoding_label(used.encoding),
        ));
    }
    true
}

/// Asks about unsaved changes in tab `index`. True when it is fine to let them go.
unsafe fn confirm_discard(index: usize) -> bool {
    if !editor::is_modified(edit_of(index)) {
        return true;
    }
    activate(index);
    let name = app(|a| label(&a.docs.borrow()[index]));
    match ask_yes_no_cancel(&format!("Save changes to {name}?")) {
        Answer::Yes => save(index),
        Answer::No => true,
        Answer::Cancel => false,
    }
}

/// Closes a tab after asking about unsaved changes. The last tab is replaced by an empty one.
pub unsafe fn close_tab(index: usize) -> bool {
    if index >= count() || !confirm_discard(index) {
        return false;
    }
    let active = active_index();
    let doc = app(|a| a.docs.borrow_mut().remove(index));
    if doc.edit == app(|a| a.edit.get()) {
        app(|a| a.edit.set(null_mut()));
    }
    DestroyWindow(doc.edit);
    if count() == 0 {
        new_tab();
        return true;
    }
    let next = if index < active {
        active - 1
    } else {
        active.min(count() - 1)
    };
    activate(next);
    true
}

/// Before exit: asks about every tab with unsaved changes. False if the user cancels.
pub unsafe fn confirm_exit() -> bool {
    (0..count()).all(|i| confirm_discard(i))
}

/// Called when z++ comes to the front and when a tab is shown: notices files changed or
/// deleted by another program. Unchanged tabs reload quietly; tabs with edits ask first.
pub unsafe fn check_disk(index: usize) {
    if app(|a| a.is_checking_disk.replace(true)) {
        return;
    }
    check_disk_inner(index);
    app(|a| a.is_checking_disk.set(false));
}

unsafe fn check_disk_inner(index: usize) {
    let Some((path, old, edit)) = app(|a| {
        let docs = a.docs.borrow();
        let d = docs.get(index)?;
        Some((d.path.clone()?, d.stamp, d.edit))
    }) else {
        return;
    };
    let now = file::stamp(&path);
    if now == old {
        return;
    }
    app(|a| a.docs.borrow_mut()[index].stamp = now);
    let name = path.display();
    if now.is_none() {
        editor::set_modified(edit, true);
        refresh();
        info_box(&format!("{name} was deleted or moved by another program.\n\nIt is still open in z++. Save it to keep it."));
        return;
    }
    if editor::is_modified(edit) {
        let question = format!(
            "{name} was changed by another program.\n\nReload it and lose your changes in z++?"
        );
        if ask_yes_no_cancel(&question) != Answer::Yes {
            return;
        }
    }
    if !reload(index, &path) {
        // Probably caught mid-write by the other program: look again next time.
        app(|a| a.docs.borrow_mut()[index].stamp = old);
    }
}

unsafe fn reload(index: usize, path: &Path) -> bool {
    let Some((text, format)) = read(path) else {
        return false;
    };
    let edit = edit_of(index);
    let sel = editor::selection(edit);
    editor::set_text(edit, &text);
    let end = editor::text_len(edit);
    let caret = sel.cpMin.min(end);
    editor::select(
        edit,
        CHARRANGE {
            cpMin: caret,
            cpMax: caret,
        },
    );
    app(|a| a.docs.borrow_mut()[index].format = format);
    statusbar::set_message("Reloaded: the file changed on disk");
    refresh();
    true
}

/// Format > Encoding: the text stays the same; the next save writes the new encoding.
pub unsafe fn set_encoding(encoding: Encoding) {
    change_format(|f| f.encoding = encoding);
}

/// Format > Line Endings: the next save writes LF or CRLF line breaks.
pub unsafe fn set_line_endings(is_lf_only: bool) {
    change_format(|f| f.is_lf_only = is_lf_only);
}

unsafe fn change_format(change: impl FnOnce(&mut Format)) {
    let index = active_index();
    let (before, after) = app(|a| {
        let mut docs = a.docs.borrow_mut();
        let doc = &mut docs[index];
        let before = doc.format;
        change(&mut doc.format);
        (before, doc.format)
    });
    if before.encoding != after.encoding || before.is_lf_only != after.is_lf_only {
        editor::set_modified(edit_of(index), true);
        refresh();
    }
}

pub unsafe fn add_recent(path: &Path) {
    let key = path.to_string_lossy().to_lowercase();
    let recent = app(|a| {
        let mut recent = a.recent.borrow_mut();
        recent.retain(|p| p.to_string_lossy().to_lowercase() != key);
        recent.insert(0, path.to_path_buf());
        recent.truncate(menu::MAX_RECENT);
        recent.clone()
    });
    menu::set_recent(app(|a| a.menus.get().recent), &recent);
    prefs::save_recent(&recent);
}

pub unsafe fn open_recent(i: usize) {
    let Some(path) = app(|a| a.recent.borrow().get(i).cloned()) else {
        return;
    };
    if !open_path(&path) && !path.exists() {
        let recent = app(|a| {
            let mut recent = a.recent.borrow_mut();
            recent.remove(i);
            recent.clone()
        });
        menu::set_recent(app(|a| a.menus.get().recent), &recent);
        prefs::save_recent(&recent);
    }
}

pub unsafe fn clear_recent() {
    app(|a| a.recent.borrow_mut().clear());
    menu::set_recent(app(|a| a.menus.get().recent), &[]);
    prefs::save_recent(&[]);
}

/// Saved files open now, in tab order, and the position of the active one among them.
pub fn session() -> (Vec<PathBuf>, usize) {
    app(|a| {
        let docs = a.docs.borrow();
        let active = a.active.get();
        let mut files = Vec::new();
        let mut active_file = 0;
        for (i, d) in docs.iter().enumerate() {
            if let Some(p) = &d.path {
                if i == active {
                    active_file = files.len();
                }
                files.push(p.clone());
            }
        }
        (files, active_file)
    })
}

/// Reopens last time's files that still exist, and shows the one that was active.
pub unsafe fn restore_session(files: &[PathBuf], active: usize) {
    let mut opened = Vec::new();
    for (i, path) in files.iter().enumerate() {
        if path.is_file() && open_path(path) {
            opened.push((i, active_index()));
        }
    }
    if let Some(&(_, tab)) = opened.iter().find(|(i, _)| *i == active) {
        activate(tab);
    }
}

/// The Open (one or more files) or Save As dialog.
unsafe fn file_dialog(is_save: bool) -> Vec<PathBuf> {
    let mut buf = vec![0u16; PATH_BUF_LEN];
    let current = app(|a| {
        a.docs
            .borrow()
            .get(a.active.get())
            .and_then(|d| d.path.clone())
    });
    let mut initial_dir: Option<Vec<u16>> = None;
    if let Some(p) = &current {
        if is_save {
            let name: Vec<u16> = p.file_name().unwrap_or_default().encode_wide().collect();
            let n = name.len().min(PATH_BUF_LEN - 1);
            buf[..n].copy_from_slice(&name[..n]);
        }
        initial_dir = p
            .parent()
            .map(|d| d.as_os_str().encode_wide().chain([0]).collect());
    }
    let filter: Vec<u16> = FILE_FILTER.encode_utf16().collect();
    let mut ofn: OPENFILENAMEW = zeroed();
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = app(|a| a.main.get());
    ofn.lpstrFilter = filter.as_ptr();
    ofn.nFilterIndex = 1;
    ofn.lpstrFile = buf.as_mut_ptr();
    ofn.nMaxFile = PATH_BUF_LEN as u32;
    ofn.lpstrInitialDir = initial_dir.as_ref().map_or(null(), |d| d.as_ptr());
    ofn.Flags = OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
    let is_ok = if is_save {
        ofn.Flags |= OFN_OVERWRITEPROMPT;
        GetSaveFileNameW(&mut ofn)
    } else {
        ofn.Flags |= OFN_FILEMUSTEXIST | OFN_ALLOWMULTISELECT;
        GetOpenFileNameW(&mut ofn)
    };
    if is_ok == 0 {
        return Vec::new();
    }
    parse_selection(&buf)
}

/// One file: "path\0\0". Several: "folder\0name1\0name2\0\0".
fn parse_selection(buf: &[u16]) -> Vec<PathBuf> {
    let parts: Vec<&[u16]> = buf
        .split(|&c| c == 0)
        .take_while(|s| !s.is_empty())
        .collect();
    let to_path = |s: &[u16]| PathBuf::from(OsString::from_wide(s));
    match parts.as_slice() {
        [] => Vec::new(),
        [single] => vec![to_path(single)],
        [dir, names @ ..] => names
            .iter()
            .map(|n| to_path(dir).join(to_path(n)))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn parses_single_and_multiple_selection() {
        let single = w("C:\\a\\b.txt\0\0");
        assert_eq!(
            parse_selection(&single),
            vec![PathBuf::from("C:\\a\\b.txt")]
        );
        let many = w("C:\\a\0b.txt\0c.md\0\0");
        assert_eq!(
            parse_selection(&many),
            vec![PathBuf::from("C:\\a\\b.txt"), PathBuf::from("C:\\a\\c.md")]
        );
        assert!(parse_selection(&w("\0\0")).is_empty());
    }
}
