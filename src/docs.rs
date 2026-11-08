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
use crate::languages::{self, Language};
use crate::window::{self, app, ask_yes_no_cancel, error_box, info_box, Answer};
use crate::{editor, menu, notice, prefs, statusbar, tabs, theme, wait};

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
    /// Index into languages::LANGUAGES, or None for plain text.
    pub language: Option<usize>,
    /// News about the file on disk, shown in the notice bar while this tab is active.
    disk_notice: Option<DiskNotice>,
    /// `zpp --wait` callers (event names) to release when this tab closes.
    waiters: Vec<String>,
    shown_modified: bool,
    /// Unsaved even though the text matches the file: it was deleted on disk, or the encoding
    /// changed. Scintilla's own modified state only follows edits to the text.
    is_forced_modified: bool,
}

impl Doc {
    pub unsafe fn is_modified(&self) -> bool {
        self.is_forced_modified || editor::is_modified(self.edit)
    }
}

unsafe fn is_modified(index: usize) -> bool {
    app(|a| a.docs.borrow().get(index).is_some_and(|d| d.is_modified()))
}

unsafe fn force_modified(index: usize) {
    app(|a| a.docs.borrow_mut()[index].is_forced_modified = true);
}

/// Line numbers on or off, with the margin sized for the document's last line number.
pub unsafe fn fit_margin(edit: HWND) {
    editor::show_line_numbers(edit, app(|a| a.is_line_numbers.get()));
}

pub fn language_of(index: usize) -> Option<&'static Language> {
    languages::get(app(|a| a.docs.borrow().get(index).and_then(|d| d.language)))
}

/// Highlights tab `index` as `language` (None for plain text) and restyles it.
pub unsafe fn set_language(index: usize, language: Option<usize>) {
    let edit = edit_of(index);
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        docs[index].language = language;
        docs[index].look_gen = 0;
    });
    editor::set_lexer(edit, languages::get(language));
    style_if_stale(index);
    refresh();
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
            language: None,
            disk_notice: None,
            waiters: Vec::new(),
            shown_modified: false,
            is_forced_modified: false,
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
    let (colors, syntax, face, size) = window::look_parts();
    let edit = edit_of(index);
    theme::style_editor(
        edit,
        &theme::Look {
            colors,
            syntax,
            face: &face,
            size_tenths: size,
        },
        language_of(index),
    );
    fit_margin(edit);
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
                d.shown_modified = d.is_modified();
                tabs::Tab {
                    label: label(d),
                    is_modified: d.shown_modified,
                }
            })
            .collect();
        (list, a.active.get())
    });
    tabs::set(list, active);
    show_notice();
    update_title();
    update_status();
    window::sync_menu_checks();
}

thread_local! {
    static IS_BATCH_EDIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `edits` without the per-change status and margin updates, then does them once.
pub unsafe fn batch_edit<T>(edit: HWND, edits: impl FnOnce() -> T) -> T {
    IS_BATCH_EDIT.with(|b| b.set(true));
    let result = edits();
    IS_BATCH_EDIT.with(|b| b.set(false));
    on_edit_change(edit);
    result
}

/// EN_CHANGE from one of the editors: repaint its tab only when its modified mark flips.
pub unsafe fn on_edit_change(edit: HWND) {
    if IS_BATCH_EDIT.with(|b| b.get()) {
        return;
    }
    let flipped = app(|a| {
        a.docs
            .borrow()
            .iter()
            .any(|d| d.edit == edit && d.shown_modified != d.is_modified())
    });
    statusbar::set_message("");
    fit_margin(edit);
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
            let marker = if d.is_modified() { "*" } else { "" };
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
    let (line, col) = editor::caret_line_col(edit);
    let selected = editor::char_count(edit, editor::selection(edit));
    let mut position = format!("Ln {line}, Col {col}");
    if selected > 0 {
        position.push_str(&format!("    {selected} selected"));
    }
    let format = active_format();
    statusbar::set(statusbar::Info {
        position,
        message: statusbar::message(),
        zoom: format!("{}%", zoom_percent(edit)),
        line_endings: file::line_ending_label(format).to_string(),
        encoding: file::encoding_label(format.encoding).to_string(),
    });
}

/// Ctrl+wheel zoom adds points to the font size; shown as a percentage of the chosen size.
unsafe fn zoom_percent(edit: HWND) -> isize {
    let tenths = app(|a| a.size.get()).max(1) as isize;
    let zoomed = (tenths + editor::zoom_points(edit) * 10).max(0);
    (zoomed * 100 + tenths / 2) / tenths
}

/// File > Open: one or more files, each in its own tab.
pub unsafe fn cmd_open() {
    for path in file_dialog(false) {
        open_path(&path);
    }
}

/// Splits `file:line` and `file:line:col` (both counted from 1), the way compilers, grep and
/// agents print locations. Windows file names cannot contain ':', so a trailing `:digits` is
/// always a location and never part of the name.
pub fn split_location(arg: &Path) -> (PathBuf, Option<(usize, usize)>) {
    let text = arg.to_string_lossy();
    let Some((head, last)) = split_number(&text) else {
        return (arg.to_path_buf(), None);
    };
    match split_number(head) {
        Some((path, line)) => (PathBuf::from(path), Some((line, last))),
        None => (PathBuf::from(head), Some((last, 1))),
    }
}

/// "name:12" as ("name", 12). The name must be longer than a drive letter.
fn split_number(text: &str) -> Option<(&str, usize)> {
    let (head, tail) = text.rsplit_once(':')?;
    let n = tail.parse::<usize>().ok().filter(|&n| n > 0)?;
    (head.len() > 1).then_some((head, n))
}

/// Opens a file given as `path`, `path:line` or `path:line:col` and moves the caret there.
/// Returns the tab it is in.
pub unsafe fn open_location(arg: &Path) -> Option<usize> {
    let (path, location) = split_location(arg);
    if !open_path(&path) {
        return None;
    }
    let index = active_index();
    if let Some((line, col)) = location {
        let edit = edit_of(index);
        editor::go_to(edit, line - 1, col - 1);
    }
    Some(index)
}

/// Remembers a `zpp --wait` caller to release when tab `index` closes.
pub fn add_waiter(index: usize, waiter: &str) {
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        let waiters = &mut docs[index].waiters;
        if !waiters.iter().any(|w| w == waiter) {
            waiters.push(waiter.to_string());
        }
    });
}

/// Releases waiters of a closed tab that no other open tab is holding.
unsafe fn release_waiters(waiters: &[String]) {
    for w in waiters {
        let is_held = app(|a| a.docs.borrow().iter().any(|d| d.waiters.contains(w)));
        if !is_held {
            wait::release(w);
        }
    }
}

/// On exit, every waiting caller is done.
pub unsafe fn release_all_waiters() {
    let waiters: Vec<String> = app(|a| {
        a.docs
            .borrow_mut()
            .iter_mut()
            .flat_map(|d| std::mem::take(&mut d.waiters))
            .collect()
    });
    for w in &waiters {
        wait::release(w);
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
    editor::set_text(edit, &text, format.is_lf_only);
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        let doc = &mut docs[index];
        doc.path = Some(path.clone());
        doc.format = format;
        doc.stamp = file::stamp(&path);
        doc.is_forced_modified = false;
    });
    fit_margin(edit);
    set_language(index, languages::for_path(&path));
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

/// The decoded file with one kind of line break, or None after telling the user why it cannot
/// be opened.
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
        Ok((text, format)) => Some((
            file::normalize_line_breaks(&text, format.is_lf_only),
            format,
        )),
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
    path_none && !is_modified(index) && editor::text_len(edit) == 0
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
        if is_modified(i) && !save(i) {
            return;
        }
    }
}

unsafe fn save_to(index: usize, path: &Path) -> bool {
    let (edit, format, is_new_path) = app(|a| {
        let d = &a.docs.borrow()[index];
        (d.edit, d.format, d.path.as_deref() != Some(path))
    });
    // Undoing a line-ending change, a regex replace or a paste can mix line endings back in.
    let text = file::normalize_line_breaks(&editor::text(edit), format.is_lf_only);
    let (bytes, used) = file::encode(&text, format);
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
        doc.is_forced_modified = false;
        doc.disk_notice = None;
    });
    editor::set_saved(edit);
    if is_new_path {
        set_language(index, languages::for_path(path));
    }
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
    if !is_modified(index) {
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
    release_waiters(&doc.waiters);
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
/// deleted by another program. Unchanged tabs reload quietly; tabs with edits get a notice.
pub unsafe fn check_disk(index: usize) {
    if app(|a| a.is_checking_disk.replace(true)) {
        return;
    }
    check_disk_inner(index);
    app(|a| a.is_checking_disk.set(false));
}

unsafe fn check_disk_inner(index: usize) {
    let Some((path, old)) = app(|a| {
        let docs = a.docs.borrow();
        let d = docs.get(index)?;
        Some((d.path.clone()?, d.stamp))
    }) else {
        return;
    };
    let now = file::stamp(&path);
    if now == old {
        return;
    }
    app(|a| a.docs.borrow_mut()[index].stamp = now);
    // Ask through the notice bar, never a dialog: an agent may be editing the file right now.
    let notice = if now.is_none() {
        force_modified(index);
        Some(DiskNotice::Deleted)
    } else if is_modified(index) {
        Some(DiskNotice::Changed)
    } else {
        None
    };
    if notice.is_some() {
        app(|a| a.docs.borrow_mut()[index].disk_notice = notice);
        refresh();
        return;
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
    // Only the changed lines are replaced, so the caret and scroll position stay put and the
    // change margin shows what the other program changed.
    let hunks = editor::replace_changed_lines(edit, &text, format.is_lf_only);
    fit_margin(edit);
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        docs[index].format = format;
        docs[index].is_forced_modified = false;
        docs[index].disk_notice = None;
    });
    refresh();
    let noun = if hunks == 1 { "place" } else { "places" };
    statusbar::set_message(&format!(
        "Reloaded: changed on disk in {hunks} {noun} (Undo goes back)"
    ));
    true
}

/// What the notice bar says about a tab's file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiskNotice {
    /// Changed by another program while the tab had unsaved edits.
    Changed,
    Deleted,
}

const CHANGED_BUTTONS: [&str; 2] = ["Reload", "Keep mine"];
const DELETED_BUTTONS: [&str; 2] = ["Save", "Dismiss"];

/// Shows the active tab's notice, or hides the bar.
unsafe fn show_notice() {
    let active = active_index();
    let notice = app(|a| {
        let docs = a.docs.borrow();
        let d = docs.get(active)?;
        let name = label(d);
        Some(match d.disk_notice? {
            DiskNotice::Changed => notice::Notice {
                text: format!("{name} changed on disk, and you have unsaved edits here."),
                buttons: CHANGED_BUTTONS.to_vec(),
            },
            DiskNotice::Deleted => notice::Notice {
                text: format!("{name} was deleted or moved on disk. It is still open here."),
                buttons: DELETED_BUTTONS.to_vec(),
            },
        })
    });
    if notice::set(notice) {
        window::layout();
    }
}

/// A click on the notice bar's button `button`, for the active tab.
pub unsafe fn on_notice_button(button: usize) {
    let index = active_index();
    let Some((kind, path)) = app(|a| {
        let docs = a.docs.borrow();
        let d = docs.get(index)?;
        Some((d.disk_notice?, d.path.clone()?))
    }) else {
        return;
    };
    let clear = || app(|a| a.docs.borrow_mut()[index].disk_notice = None);
    match (kind, button) {
        (DiskNotice::Changed, 0) => {
            reload(index, &path);
        }
        (DiskNotice::Deleted, 0) => {
            save(index);
        }
        _ => clear(),
    }
    refresh();
}

/// Format > Encoding: the text stays the same; the next save writes the new encoding.
pub unsafe fn set_encoding(encoding: Encoding) {
    let index = active_index();
    let before =
        app(|a| std::mem::replace(&mut a.docs.borrow_mut()[index].format.encoding, encoding));
    if before != encoding {
        force_modified(index);
        refresh();
    }
}

/// Format > Line Endings: converts every line break now (one undo step).
pub unsafe fn set_line_endings(is_lf_only: bool) {
    let index = active_index();
    let before = app(|a| {
        std::mem::replace(
            &mut a.docs.borrow_mut()[index].format.is_lf_only,
            is_lf_only,
        )
    });
    if before != is_lf_only {
        editor::convert_line_endings(edit_of(index), is_lf_only);
        // Undo can bring the text back to its save point, but the format stays changed.
        force_modified(index);
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
