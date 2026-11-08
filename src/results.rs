//! The Find in Files results panel under the editor: a read-only Scintilla control listing each
//! file and its matching lines. Double-clicking a line opens the file at the match.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use windows_sys::w;
use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::gdi::s;
use crate::sci::*;
use crate::search::{Outcome, Query, MAX_HITS};
use crate::themes::Syntax;
use crate::{docs, editor};

const RESULTS_ID: usize = 2;
const HEIGHT: i32 = 220;
const TEXT_PADDING: isize = 6;
const STYLE_HEADER: usize = 1;
const STYLE_FILE: usize = 2;
const STYLE_LINE_NUMBER: usize = 3;
const MATCH_INDICATOR: usize = 8;
const MATCH_ALPHA: isize = 70;
const MATCH_OUTLINE_ALPHA: isize = 140;
/// Width of the line number column in a hit line, before ": ".
const LINE_NUMBER_WIDTH: usize = 6;

thread_local! {
    static PANEL: Cell<HWND> = const { Cell::new(null_mut()) };
    /// Where each line of the results points: (file, line, column), counted from 1.
    static TARGETS: RefCell<Vec<Option<(PathBuf, usize, usize)>>> = const { RefCell::new(Vec::new()) };
}

pub fn panel() -> HWND {
    PANEL.with(|p| p.get())
}

pub unsafe fn is_shown() -> bool {
    !panel().is_null() && IsWindowVisible(panel()) != 0
}

/// Height to lay out: zero while hidden.
pub unsafe fn height() -> i32 {
    if is_shown() {
        s(HEIGHT)
    } else {
        0
    }
}

pub unsafe fn create(parent: HWND, hinst: HINSTANCE) -> HWND {
    let hwnd = CreateWindowExW(
        0,
        w!("Scintilla"),
        null(),
        WS_CHILD | WS_VSCROLL | WS_HSCROLL | WS_CLIPCHILDREN,
        0,
        0,
        0,
        0,
        parent,
        RESULTS_ID as _,
        hinst,
        null(),
    );
    let send = |m: u32, w: usize, l: isize| SendMessageW(hwnd, m, w, l);
    send(SCI_SETCODEPAGE, SC_CP_UTF8, 0);
    send(SCI_SETMODEVENTMASK, 0, 0); // no change notifications: this is not a document
    send(SCI_SETMARGINWIDTHN, 0, 0);
    send(SCI_SETMARGINWIDTHN, 1, 0);
    send(SCI_SETMARGINLEFT, 0, TEXT_PADDING);
    send(SCI_SETCARETLINEVISIBLE, 1, 0);
    send(SCI_SETSCROLLWIDTHTRACKING, 1, 0);
    send(SCI_SETSCROLLWIDTH, 1, 0);
    send(SCI_INDICSETSTYLE, MATCH_INDICATOR, INDIC_ROUNDBOX as isize);
    send(SCI_INDICSETUNDER, MATCH_INDICATOR, 1);
    send(SCI_INDICSETALPHA, MATCH_INDICATOR, MATCH_ALPHA);
    send(
        SCI_INDICSETOUTLINEALPHA,
        MATCH_INDICATOR,
        MATCH_OUTLINE_ALPHA,
    );
    send(SCI_SETREADONLY, 1, 0);
    PANEL.with(|p| p.set(hwnd));
    hwnd
}

pub unsafe fn show(is_shown: bool) {
    ShowWindow(panel(), if is_shown { SW_SHOW } else { SW_HIDE });
}

/// Colors for the headings, file names, line numbers and match highlight, after
/// theme::style_editor has set the base colors and font.
pub unsafe fn apply_syntax(syntax: &Syntax) {
    let hwnd = panel();
    let send = |m: u32, w: usize, l: isize| SendMessageW(hwnd, m, w, l);
    for (style, tok, is_bold) in [
        (STYLE_HEADER, syntax.comment, true),
        (STYLE_FILE, syntax.keyword, true),
        (STYLE_LINE_NUMBER, syntax.number, false),
    ] {
        send(SCI_STYLESETFORE, style, tok.fg as isize);
        send(SCI_STYLESETBOLD, style, is_bold as isize);
    }
    send(SCI_INDICSETFORE, MATCH_INDICATOR, syntax.string.fg as isize);
}

/// Lines of the panel, each with its style runs, match range and target.
struct Built {
    text: String,
    /// (byte start, byte length, style)
    runs: Vec<(usize, usize, usize)>,
    matches: Vec<(usize, usize)>,
    targets: Vec<Option<(PathBuf, usize, usize)>>,
}

fn build(query: &Query, outcome: &Outcome) -> Built {
    let mut b = Built {
        text: String::new(),
        runs: Vec::new(),
        matches: Vec::new(),
        targets: Vec::new(),
    };
    let hits: usize = outcome.files.iter().map(|f| f.hits.len()).sum();
    let mut header = format!(
        "\"{}\" in {} ({}): {} {} in {} of {} {} searched",
        query.needle,
        query.folder.display(),
        if query.filters.trim().is_empty() {
            "all files"
        } else {
            query.filters.trim()
        },
        hits,
        if hits == 1 { "hit" } else { "hits" },
        outcome.files.len(),
        outcome.files_searched,
        if outcome.files_searched == 1 {
            "file"
        } else {
            "files"
        },
    );
    if outcome.is_capped {
        header.push_str(&format!(" (stopped at {MAX_HITS})"));
    }
    push_line(&mut b, &header, STYLE_HEADER, None);
    for file in &outcome.files {
        let label = format!("{} ({})", file.path.display(), file.hits.len());
        let first = file
            .hits
            .first()
            .map(|h| (file.path.clone(), h.line, h.col));
        push_line(&mut b, &label, STYLE_FILE, first);
        for h in &file.hits {
            let number = format!("{:>width$}: ", h.line, width = LINE_NUMBER_WIDTH);
            let start = b.text.len();
            b.runs.push((start, number.len(), STYLE_LINE_NUMBER));
            if h.end > h.start {
                b.matches
                    .push((start + number.len() + h.start, h.end - h.start));
            }
            b.text.push_str(&number);
            b.text.push_str(&h.text);
            b.text.push('\n');
            b.targets.push(Some((file.path.clone(), h.line, h.col)));
        }
    }
    b
}

fn push_line(b: &mut Built, line: &str, style: usize, target: Option<(PathBuf, usize, usize)>) {
    b.runs.push((b.text.len(), line.len(), style));
    b.text.push_str(line);
    b.text.push('\n');
    b.targets.push(target);
}

/// Fills the panel with a finished search and shows it.
pub unsafe fn set(query: &Query, outcome: &Outcome) {
    let b = build(query, outcome);
    let hwnd = panel();
    let send = |m: u32, w: usize, l: isize| SendMessageW(hwnd, m, w, l);
    let mut text = b.text.into_bytes();
    text.push(0);
    send(SCI_SETREADONLY, 0, 0);
    send(SCI_SETTEXT, 0, text.as_ptr() as isize);
    send(SCI_EMPTYUNDOBUFFER, 0, 0);
    for (start, len, style) in b.runs {
        send(SCI_STARTSTYLING, start, 0);
        send(SCI_SETSTYLING, len, style as isize);
    }
    send(SCI_SETINDICATORCURRENT, MATCH_INDICATOR, 0);
    for (start, len) in b.matches {
        send(SCI_INDICATORFILLRANGE, start, len as isize);
    }
    send(SCI_SETREADONLY, 1, 0);
    send(SCI_GOTOLINE, 0, 0);
    TARGETS.with(|t| *t.borrow_mut() = b.targets);
    show(true);
}

/// A double-click on line `line` (0-based): opens its file at the match.
pub unsafe fn open_line(line: usize) {
    let Some((path, line, col)) = TARGETS.with(|t| t.borrow().get(line).cloned().flatten()) else {
        return;
    };
    if docs::open_path(&path) {
        let edit = editor_hwnd();
        editor::go_to(edit, line - 1, col - 1);
        SetFocus(edit);
    }
}

fn editor_hwnd() -> HWND {
    crate::window::app(|a| a.edit.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{FileHits, Hit};

    #[test]
    fn every_line_has_a_target_slot_and_matches_land_on_the_word() {
        let q = Query {
            needle: "foo".into(),
            folder: PathBuf::from(r"C:\p"),
            ..Query::default()
        };
        let outcome = Outcome {
            files: vec![FileHits {
                path: PathBuf::from(r"C:\p\a.rs"),
                hits: vec![Hit {
                    line: 12,
                    col: 5,
                    text: "let foo = 1;".into(),
                    start: 4,
                    end: 7,
                }],
            }],
            files_searched: 3,
            ..Outcome::default()
        };
        let b = build(&q, &outcome);
        assert_eq!(b.text.lines().count(), b.targets.len());
        let (start, len) = b.matches[0];
        assert_eq!(&b.text[start..start + len], "foo");
        assert_eq!(b.targets[2], Some((PathBuf::from(r"C:\p\a.rs"), 12, 5)));
        assert!(b
            .text
            .starts_with("\"foo\" in C:\\p (all files): 1 hit in 1 of 3 files"));
    }
}
