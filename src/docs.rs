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
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{IsWindowEnabled, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::file::{self, Encoding, Format, Stamp};
use crate::languages::{self, Language};
use crate::window::{self, app, ask_yes_no_cancel, error_box, info_box, Answer};
use crate::{
    ask, diff, editor, format, mdview, menu, prefs, profile, sci, session, statusbar, tabs, theme,
    wait,
};

const UNTITLED: &str = "Untitled";
const MARKDOWN: &str = "Markdown";
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
    /// The user was told the file is gone and kept the tab open.
    is_known_deleted: bool,
    /// The disk state the taskbar button last flashed for, so one change flashes once.
    flashed_for: Option<Option<Stamp>>,
    /// The user asked to reload this file without asking while the tab has no unsaved edits.
    is_auto_reload: bool,
    /// Shown on the tab in place of "Untitled N" for a tab with no file, such as a comparison.
    caption: Option<String>,
    /// File in the session's backup folder holding this tab's unsaved text (session.rs).
    backup: Option<String>,
    /// The text changed since its backup was written.
    is_backup_stale: bool,
    /// `zpp --wait` callers (event names) to release when this tab closes.
    waiters: Vec<String>,
    /// Disk checks in a row that found the file missing (see check_disk_inner).
    missing_checks: std::cell::Cell<u32>,
    shown_modified: bool,
    /// Unsaved even though the text matches the file: it was deleted on disk, or the encoding
    /// changed. Scintilla's own modified state only follows edits to the text.
    is_forced_modified: bool,
    /// Markdown shown formatted (mdview.rs) instead of as source.
    pub is_rendered: bool,
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

/// Highlights tab `index` as `language` (None for plain text) and restyles it. A tab that is
/// no longer Markdown goes back to showing its source.
pub unsafe fn set_language(index: usize, language: Option<usize>) {
    let edit = edit_of(index);
    let was_rendered = is_rendered(index);
    let is_rendered = was_rendered && is_markdown(languages::get(language));
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        docs[index].language = language;
        docs[index].is_rendered = is_rendered;
        docs[index].look_gen = 0;
    });
    editor::set_lexer(edit, lexer_language(index));
    if was_rendered && !is_rendered {
        editor::set_wrap(edit, app(|a| a.is_wrap.get()));
    }
    style_if_stale(index);
    refresh();
}

fn is_markdown(language: Option<&Language>) -> bool {
    language.is_some_and(|l| l.name == MARKDOWN)
}

pub fn is_rendered(index: usize) -> bool {
    app(|a| a.docs.borrow().get(index).is_some_and(|d| d.is_rendered))
}

/// Whether the active tab can switch to the rendered view.
pub fn is_active_markdown() -> bool {
    is_markdown(language_of(active_index()))
}

/// The language the editor's lexer highlights: none while Markdown is rendered, because
/// mdview styles the text itself.
pub fn lexer_language(index: usize) -> Option<&'static Language> {
    if is_rendered(index) {
        None
    } else {
        language_of(index)
    }
}

/// Format > Bold, Italic, Underline: Markdown markup around the selection. Other files are
/// plain text and can't hold formatting; there Ctrl+U keeps its old job, lowercase.
pub unsafe fn format_selection(style: format::Style) {
    let edit = app(|a| a.edit.get());
    if is_active_markdown() {
        format::apply(edit, style);
        return;
    }
    if style == format::Style::Underline {
        SendMessageW(edit, sci::SCI_LOWERCASE, 0, 0);
        return;
    }
    statusbar::set_message(
        "Bold, italic, and underline are for Markdown files: plain text can't store them",
    );
    update_status();
}
/// View > Rendered Markdown: switches the active tab between its Markdown source and the
/// formatted view. Both edit the same text, and the caret stays where it was.
pub unsafe fn toggle_rendered() {
    let index = active_index();
    let is_on = !is_rendered(index);
    if is_on && !is_active_markdown() {
        statusbar::set_message("Rendered view is for Markdown files (Language > Markdown)");
        update_status();
        return;
    }
    let edit = edit_of(index);
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        docs[index].is_rendered = is_on;
        docs[index].look_gen = 0;
    });
    editor::set_lexer(edit, lexer_language(index));
    // Paragraphs are single long lines, so the formatted view always wraps them on screen.
    editor::set_wrap(edit, is_on || app(|a| a.is_wrap.get()));
    style_if_stale(index);
    editor::scroll_caret(edit);
    statusbar::set_message(if is_on {
        "Rendered Markdown: edits go straight into the Markdown"
    } else {
        "Markdown source"
    });
    refresh();
}

/// Rendered styles on top of the base look, for a tab showing formatted Markdown.
pub unsafe fn style_rendered(index: usize, look: &theme::Look) {
    if is_rendered(index) {
        let edit = edit_of(index);
        mdview::apply_styles(edit, look);
        mdview::restyle(edit);
    }
}

fn is_rendered_edit(edit: HWND) -> bool {
    app(|a| {
        a.docs
            .borrow()
            .iter()
            .any(|d| d.edit == edit && d.is_rendered)
    })
}

/// SCN_STYLENEEDED, and caret moves: restyles a rendered tab, which also shows the markup
/// next to the caret and hides the markup it left.
pub unsafe fn restyle_if_rendered(edit: HWND) {
    if is_rendered_edit(edit) {
        mdview::restyle(edit);
    }
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
        None => doc
            .caption
            .clone()
            .unwrap_or_else(|| format!("{UNTITLED} {}", doc.untitled)),
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
            is_known_deleted: false,
            flashed_for: None,
            is_auto_reload: false,
            caption: None,
            backup: None,
            is_backup_stale: false,
            waiters: Vec::new(),
            missing_checks: std::cell::Cell::new(0),
            shown_modified: false,
            is_forced_modified: false,
            is_rendered: false,
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
    let look = theme::Look {
        colors,
        syntax,
        face: &face,
        size_tenths: size,
    };
    theme::style_editor(edit, &look, lexer_language(index));
    style_rendered(index, &look);
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
        let mut docs = a.docs.borrow_mut();
        let doc = docs.iter_mut().find(|d| d.edit == edit);
        doc.is_some_and(|d| {
            d.is_backup_stale = true;
            d.shown_modified != d.is_modified()
        })
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
    let path = session::tidy_path(path);
    if let Some(index) = find_open(&path) {
        activate(index);
        if let Some(open) = app(|a| a.docs.borrow()[index].path.clone()) {
            add_recent(&open);
        }
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
    match read_quietly(path) {
        Ok(read) => Some(read),
        Err(why) => {
            error_box(&why);
            None
        }
    }
}

/// Like `read`, but says why it failed instead of showing a dialog.
fn read_quietly(path: &Path) -> Result<(Vec<u16>, Format), String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    let (text, format) = file::decode(&bytes)
        .map_err(|why| format!("z++ will not open {} because {why}.", path.display()))?;
    Ok((
        file::normalize_line_breaks(&text, format.is_lf_only),
        format,
    ))
}

fn find_open(path: &Path) -> Option<usize> {
    let key = session::path_key(path);
    app(|a| {
        a.docs
            .borrow()
            .iter()
            .position(|d| d.path.as_ref().is_some_and(|p| session::path_key(p) == key))
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
        doc.is_known_deleted = false;
        if let Some(backup) = doc.backup.take() {
            session::delete_backup(&backup);
        }
    });
    editor::set_saved(edit);
    // A name with no known extension keeps the language the tab already has.
    if let Some(language) = languages::for_path(path).filter(|_| is_new_path) {
        set_language(index, Some(language));
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
    if let Some(backup) = &doc.backup {
        session::delete_backup(backup);
    }
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

/// The new tab order after moving tabs `moving` (any order) to insertion point `gap` (0 is
/// before the first tab, `count` after the last): old indices, listed in their new places.
/// The moved tabs keep their order among themselves.
pub fn reorder(count: usize, moving: &[usize], gap: usize) -> Vec<usize> {
    let mut moved: Vec<usize> = moving.iter().copied().filter(|&i| i < count).collect();
    moved.sort_unstable();
    moved.dedup();
    let staying: Vec<usize> = (0..count).filter(|i| !moved.contains(i)).collect();
    let at = staying.iter().filter(|&&i| i < gap).count();
    let mut order = staying[..at].to_vec();
    order.extend(&moved);
    order.extend(&staying[at..]);
    order
}

/// Where a step left (-1) or right (+1) puts the selected tabs, as an insertion point.
pub fn step_gap(count: usize, moving: &[usize], step: isize) -> Option<usize> {
    let first = *moving.iter().min()?;
    let last = *moving.iter().max()?;
    match step {
        s if s < 0 && first > 0 => Some(first - 1),
        s if s > 0 && last + 1 < count => Some(last + 2),
        _ => None,
    }
}

/// Moves tabs `moving` to insertion point `gap`, keeping the shown tab shown and the moved
/// tabs selected.
pub unsafe fn move_tabs(moving: &[usize], gap: usize) {
    let order = reorder(count(), moving, gap);
    if order.iter().enumerate().all(|(new, &old)| new == old) {
        return;
    }
    let active = active_index();
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        let mut old: Vec<Option<Doc>> = docs.drain(..).map(Some).collect();
        docs.extend(order.iter().map(|&i| old[i].take().expect("each tab once")));
        a.active
            .set(order.iter().position(|&i| i == active).unwrap_or(0));
    });
    let selected: Vec<usize> = order
        .iter()
        .enumerate()
        .filter(|(_, old)| moving.contains(old))
        .map(|(new, _)| new)
        .collect();
    refresh();
    tabs::set_selected(&selected);
    save_session_now();
}

/// Ctrl+Shift+PgUp / PgDn: the selected tabs one place left or right.
pub unsafe fn move_selected(step: isize) {
    let moving = tabs::selected();
    if let Some(gap) = step_gap(count(), &moving, step) {
        move_tabs(&moving, gap);
    }
}

/// Closes tabs `indices`, right to left so the others keep their places, each asking about
/// unsaved changes. Cancel on one stops the rest.
pub unsafe fn close_tabs(indices: &[usize]) {
    let mut list = indices.to_vec();
    list.sort_unstable();
    list.dedup();
    for &index in list.iter().rev() {
        if !close_tab(index) {
            break;
        }
    }
    save_session_now();
}

/// Every tab except `keep`.
pub fn others(keep: &[usize]) -> Vec<usize> {
    (0..count()).filter(|i| !keep.contains(i)).collect()
}

/// The tabs right of tab `index`.
pub fn right_of(index: usize) -> Vec<usize> {
    (index + 1..count()).collect()
}

/// Before exit: asks about every tab with unsaved changes. False if the user cancels.
pub unsafe fn confirm_exit() -> bool {
    (0..count()).all(|i| confirm_discard(i))
}

/// Called every second, when z++ comes to the front, and when a tab is shown: notices files
/// changed or deleted by another program and asks what to do in a modal dialog. While z++ is in
/// the background it only flashes its taskbar button; the dialog comes up when the user returns.
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
        app(|a| a.docs.borrow()[index].missing_checks.set(0));
        return;
    }
    if now.is_none() {
        // Tools that save by deleting and renaming leave the file missing for a moment, and a
        // slow one for longer: only call it deleted once it stays gone for a few checks.
        let checks = app(|a| {
            let d = &a.docs.borrow()[index];
            d.missing_checks.set(d.missing_checks.get() + 1);
            d.missing_checks.get()
        });
        if checks < MISSING_CHECKS {
            return;
        }
    }
    if now.is_some() && app(|a| a.docs.borrow()[index].is_known_deleted) {
        // Back after a deletion the user already answered: only real edits count as unsaved.
        app(|a| {
            let mut docs = a.docs.borrow_mut();
            docs[index].is_known_deleted = false;
            docs[index].is_forced_modified = false;
        });
        refresh();
    }
    let is_quiet =
        now.is_some() && !is_modified(index) && app(|a| a.docs.borrow()[index].is_auto_reload);
    if is_quiet {
        set_stamp(index, now);
        reload(index, &path);
        return;
    }
    if !can_ask() {
        flash_once(index, now);
        return;
    }
    // Recorded before asking, so this change is asked about once, whatever the answer.
    set_stamp(index, now);
    if now.is_none() {
        ask_deleted(index, &path);
    } else {
        ask_changed(index, &path);
    }
}

unsafe fn set_stamp(index: usize, stamp: Option<Stamp>) {
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        docs[index].stamp = stamp;
        docs[index].flashed_for = None;
        docs[index].missing_checks.set(0);
    });
}

/// z++ is the window the user is using, with no other dialog of its own open.
unsafe fn can_ask() -> bool {
    let main = app(|a| a.main.get());
    if profile::is_test() {
        return IsIconic(main) == 0 && IsWindowEnabled(main) != 0;
    }
    let front = GetForegroundWindow();
    let is_ours = front == main || GetWindow(front, GW_OWNER) == main;
    is_ours && IsWindowEnabled(main) != 0
}

unsafe fn flash_once(index: usize, now: Option<Stamp>) {
    if app(|a| a.docs.borrow_mut()[index].flashed_for.replace(now)) == Some(now) {
        return;
    }
    let info = FLASHWINFO {
        cbSize: size_of::<FLASHWINFO>() as u32,
        hwnd: app(|a| a.main.get()),
        dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
        uCount: 0,
        dwTimeout: 0,
    };
    FlashWindowEx(&info);
}

const RELOAD: i32 = 100;
const KEEP: i32 = 101;
const COMPARE: i32 = 102;
const SAVE_AGAIN: i32 = 103;
const CLOSE: i32 = 104;
const COMPARE_CONTEXT: usize = 3;
/// Disk checks (about a second apart) a file must be missing before z++ calls it deleted.
const MISSING_CHECKS: u32 = 3;

unsafe fn ask_changed(index: usize, path: &Path) {
    let name = app(|a| label(&a.docs.borrow()[index]));
    let shown = path.display().to_string();
    activate(index);
    loop {
        let has_edits = is_modified(index);
        let heading = format!("{name} changed on disk");
        let detail = if has_edits {
            "Another program changed this file, and you have unsaved edits in this tab."
        } else {
            "Another program changed this file. You have no unsaved edits in this tab."
        };
        let reload_note = if has_edits {
            "Replaces this tab with the file on disk. Your edits are replaced (Undo still has them)."
        } else {
            "Shows the new version. The changed lines are marked, and Undo goes back."
        };
        let choices = [
            ask::Choice { id: RELOAD, label: "Reload from disk", note: reload_note },
            ask::Choice {
                id: KEEP,
                label: "Keep my version",
                note: "The tab stays as it is and counts as unsaved. Saving writes it over the file on disk.",
            },
            ask::Choice {
                id: COMPARE,
                label: "Compare",
                note: "Shows the differences in a new tab, then asks again.",
            },
        ];
        let question = ask::Question {
            heading: &heading,
            detail,
            path: &shown,
            choices: &choices,
            default: if has_edits { KEEP } else { RELOAD },
            check: (!has_edits)
                .then_some("Reload this file without asking while it has no unsaved edits"),
        };
        let (chosen, is_checked) = ask::ask(app(|a| a.main.get()), &question);
        // The comparison tab may be showing; the answer is about this one.
        if chosen != COMPARE {
            activate(index);
        }
        match chosen {
            RELOAD if has_edits && !confirm_lose_edits(&name, &shown) => continue,
            RELOAD => {
                app(|a| a.docs.borrow_mut()[index].is_auto_reload |= is_checked);
                reload(index, path);
            }
            COMPARE => {
                open_comparison(index, path);
                continue;
            }
            _ => keep_mine(index),
        }
        return;
    }
}

unsafe fn confirm_lose_edits(name: &str, shown: &str) -> bool {
    let heading = format!("Replace your edits to {name}?");
    let choices = [
        ask::Choice {
            id: RELOAD,
            label: "Reload and replace my edits",
            note: "Undo can still bring them back until you close the tab.",
        },
        ask::Choice {
            id: KEEP,
            label: "Go back",
            note: "",
        },
    ];
    let question = ask::Question {
        heading: &heading,
        detail: "The tab will show the file on disk instead of your unsaved edits.",
        path: shown,
        choices: &choices,
        default: KEEP,
        check: None,
    };
    ask::ask(app(|a| a.main.get()), &question).0 == RELOAD
}

/// The tab keeps its text and counts as unsaved, so the user sees it differs from the disk.
unsafe fn keep_mine(index: usize) {
    force_modified(index);
    refresh();
}

unsafe fn ask_deleted(index: usize, path: &Path) {
    let name = app(|a| label(&a.docs.borrow()[index]));
    let shown = path.display().to_string();
    let heading = format!("{name} was deleted or moved");
    let choices = [
        ask::Choice {
            id: KEEP,
            label: "Keep it open",
            note: "The tab stays open and counts as unsaved.",
        },
        ask::Choice {
            id: SAVE_AGAIN,
            label: "Save it again",
            note: "Writes this tab back to the same place.",
        },
        ask::Choice {
            id: CLOSE,
            label: "Close the tab",
            note: "",
        },
    ];
    let question = ask::Question {
        heading: &heading,
        detail: "The file is no longer on disk. Its text is still open in this tab.",
        path: &shown,
        choices: &choices,
        default: KEEP,
        check: None,
    };
    activate(index);
    app(|a| a.docs.borrow_mut()[index].is_known_deleted = true);
    force_modified(index);
    refresh();
    let (chosen, _) = ask::ask(app(|a| a.main.get()), &question);
    match chosen {
        SAVE_AGAIN => {
            save(index);
        }
        CLOSE => {
            // Asks about real edits only: the deletion itself is what the user is answering.
            if !editor::is_modified(edit_of(index)) {
                app(|a| a.docs.borrow_mut()[index].is_forced_modified = false);
            }
            close_tab(index);
        }
        _ => {}
    }
}
/// A read-only tab with a unified diff from this tab's text to the file on disk.
unsafe fn open_comparison(index: usize, path: &Path) {
    let name = app(|a| label(&a.docs.borrow()[index]));
    let mine = lf_text(&editor::text(edit_of(index)));
    let disk = match read_quietly(path) {
        Ok((text, _)) => lf_text(&text),
        Err(why) => {
            error_box(&why);
            return;
        }
    };
    let mut text = diff::unified(
        &mine,
        &disk,
        &format!("{name} (this tab)"),
        &format!("{name} (on disk)"),
        COMPARE_CONTEXT,
    );
    if text.is_empty() {
        text = format!("{name}: this tab and the file on disk have the same text.\n");
    }
    let tab = new_tab();
    let edit = edit_of(tab);
    let units: Vec<u16> = text.encode_utf16().collect();
    editor::set_text(edit, &units, true);
    SendMessageW(edit, sci::SCI_SETREADONLY, 1, 0);
    app(|a| a.docs.borrow_mut()[tab].caption = Some(format!("Compare: {name}")));
    set_language(tab, languages::for_path(Path::new("compare.diff")));
}

fn lf_text(units: &[u16]) -> String {
    String::from_utf16_lossy(units).replace("\r\n", "\n")
}

/// Replaces the tab's text with the file's. False, with the reason in the status bar, if the
/// file can't be read.
unsafe fn reload(index: usize, path: &Path) -> bool {
    let (text, format) = match read_quietly(path) {
        Ok(read) => read,
        Err(why) => {
            statusbar::set_message(&why);
            return false;
        }
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
    });
    refresh();
    if hunks > 0 {
        let noun = if hunks == 1 { "place" } else { "places" };
        statusbar::set_message(&format!(
            "Reloaded: changed on disk in {hunks} {noun} (Undo goes back)"
        ));
    }
    true
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
    let key = session::path_key(path);
    // Start from the saved list: another z++ window may have added files since this one started.
    let mut recent = prefs::load_recent();
    recent.retain(|p| session::path_key(p) != key);
    recent.insert(0, path.to_path_buf());
    recent.truncate(menu::MAX_RECENT);
    app(|a| *a.recent.borrow_mut() = recent.clone());
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

thread_local! {
    /// The tab records last written, so an unchanged session is not written again.
    static LAST_SAVED: std::cell::RefCell<Option<(Vec<String>, usize)>> =
        const { std::cell::RefCell::new(None) };
}

/// Whether this window keeps the session (the first z++, not a `--new-window` one) and
/// remembers unsaved edits across restarts.
pub fn is_remembering() -> bool {
    app(|a| a.is_session_owner.get() && a.is_remember_unsaved.get())
}

/// Saves the open tabs, and the text of tabs with unsaved edits, if anything changed. Called
/// every couple of seconds and right after a tab closes.
pub unsafe fn save_session_now() {
    if !app(|a| a.is_session_owner.get()) {
        return;
    }
    let remember = is_remembering();
    let mut tabs = Vec::new();
    let mut active_tab = 0;
    for index in 0..count() {
        if let Some(tab) = session_tab(index, remember) {
            if index == active_index() {
                active_tab = tabs.len();
            }
            tabs.push(tab);
        }
    }
    let records: Vec<String> = tabs.iter().map(session::encode).collect();
    let is_same = LAST_SAVED.with(|l| l.borrow().as_ref() == Some(&(records.clone(), active_tab)));
    if !is_same {
        prefs::save_tabs(&records, active_tab);
        LAST_SAVED.with(|l| *l.borrow_mut() = Some((records, active_tab)));
    }
}

/// Tab `index` as a session record, writing its backup first if its unsaved text changed.
/// None for a tab with nothing to remember (an empty Untitled tab, a comparison).
unsafe fn session_tab(index: usize, remember: bool) -> Option<session::Tab> {
    let (path, edit, backup, is_stale, caption, is_rendered) = app(|a| {
        let d = &a.docs.borrow()[index];
        (
            d.path.clone(),
            d.edit,
            d.backup.clone(),
            d.is_backup_stale,
            d.caption.is_some(),
            d.is_rendered,
        )
    });
    if caption {
        return None;
    }
    let keep_text = remember && is_modified(index);
    let backup = if keep_text {
        let name = backup.unwrap_or_else(session::new_backup_name);
        if is_stale || !session::backup_dir().join(&name).exists() {
            let text = String::from_utf16_lossy(&editor::text(edit));
            if let Err(e) = session::write_backup(&name, &text) {
                statusbar::set_message(&format!("Could not keep unsaved edits: {e}"));
            }
        }
        Some(name)
    } else {
        if let Some(old) = backup {
            session::delete_backup(&old);
        }
        None
    };
    app(|a| {
        let mut docs = a.docs.borrow_mut();
        docs[index].backup = backup.clone();
        docs[index].is_backup_stale = false;
    });
    if path.is_none() && backup.is_none() {
        return None;
    }
    let (caret, first_line) = editor::view(edit);
    Some(session::Tab {
        path,
        backup,
        caret,
        first_line,
        is_rendered,
    })
}

/// Before exit: every tab's unsaved text goes to its backup file and the session is saved, so
/// nothing needs asking.
pub unsafe fn keep_unsaved_for_next_time() {
    app(|a| {
        for d in a.docs.borrow_mut().iter_mut() {
            d.is_backup_stale = true;
        }
    });
    save_session_now();
}

/// Reopens last time's tabs with their unsaved edits, caret, and scroll position, and shows
/// the one that was active. Files that are gone and had no unsaved edits are skipped.
pub unsafe fn restore_session(tabs: &[session::Tab], active: usize) {
    let mut kept = Vec::new();
    let mut shown = None;
    let mut missing = 0;
    for (i, tab) in tabs.iter().enumerate() {
        let Some(index) = restore_tab(tab) else {
            missing += 1;
            continue;
        };
        kept.extend(app(|a| a.docs.borrow()[index].backup.clone()));
        if i == active {
            shown = Some(index);
        }
    }
    session::delete_unused_backups(&kept);
    if let Some(index) = shown {
        activate(index);
    }
    if missing > 0 {
        let noun = if missing == 1 { "file" } else { "files" };
        statusbar::set_message(&format!(
            "{missing} {noun} from last time could not be found and were not reopened"
        ));
        update_status();
    }
    save_session_now();
}

unsafe fn restore_tab(tab: &session::Tab) -> Option<usize> {
    let kept_text = tab.backup.as_deref().and_then(session::read_backup);
    let index = match (&tab.path, &kept_text) {
        (Some(path), _) if path.is_file() => {
            if !open_path(path) {
                return None;
            }
            active_index()
        }
        (Some(path), Some(_)) => {
            // The file is gone, but the unsaved edits were kept: show them, unsaved.
            let index = reuse_or_new_tab();
            app(|a| {
                let mut docs = a.docs.borrow_mut();
                docs[index].path = Some(path.clone());
                docs[index].is_known_deleted = true;
            });
            force_modified(index);
            set_language(index, languages::for_path(path));
            index
        }
        (None, Some(_)) => reuse_or_new_tab(),
        _ => return None,
    };
    let edit = edit_of(index);
    if let Some(text) = &kept_text {
        editor::replace_all_unsaved(edit, text);
        app(|a| a.docs.borrow_mut()[index].backup = tab.backup.clone());
        fit_margin(edit);
    }
    activate(index);
    if tab.is_rendered && is_active_markdown() && !is_rendered(index) {
        toggle_rendered();
    }
    editor::set_view(edit, tab.caret, tab.first_line);
    refresh();
    Some(index)
}

/// The active tab if it is an empty, untouched Untitled one, else a new tab.
unsafe fn reuse_or_new_tab() -> usize {
    if is_pristine(active_index()) {
        active_index()
    } else {
        new_tab()
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
    fn reorder_moves_groups_to_the_insertion_point() {
        assert_eq!(reorder(5, &[1, 3], 5), [0, 2, 4, 1, 3]);
        assert_eq!(reorder(5, &[3, 1], 0), [1, 3, 0, 2, 4]);
        assert_eq!(reorder(5, &[1, 3], 3), [0, 2, 1, 3, 4]);
        // Dropping a tab next to itself changes nothing.
        assert_eq!(reorder(4, &[2], 2), [0, 1, 2, 3]);
        assert_eq!(reorder(4, &[2], 3), [0, 1, 2, 3]);
        assert_eq!(reorder(3, &[0, 1, 2], 1), [0, 1, 2]);
        assert_eq!(reorder(3, &[7], 1), [0, 1, 2], "out of range is ignored");
    }

    #[test]
    fn steps_stop_at_the_ends() {
        assert_eq!(step_gap(5, &[2], -1), Some(1));
        assert_eq!(step_gap(5, &[2], 1), Some(4));
        assert_eq!(step_gap(5, &[0, 1], -1), None);
        assert_eq!(step_gap(5, &[3, 4], 1), None);
        assert_eq!(
            reorder(5, &[2], step_gap(5, &[2], 1).unwrap()),
            [0, 1, 3, 2, 4]
        );
        assert_eq!(
            reorder(5, &[2], step_gap(5, &[2], -1).unwrap()),
            [0, 2, 1, 3, 4]
        );
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
