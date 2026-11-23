//! Bold, Italic, and Underline for Markdown tabs (Format menu, toolbar, Ctrl+B / Ctrl+I /
//! Ctrl+U). Markdown is plain text, so they wrap the selection in markup: `**bold**`,
//! `*italic*`, and, since Markdown has no underline, the HTML tags `<u>underline</u>`. Pressing
//! the same command again takes the markup off. Each change is one undo step.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;

use crate::{editor, sci};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Bold,
    Italic,
    Underline,
    /// Inline code, `like this`.
    Code,
    /// `~~struck~~`.
    Strike,
}

/// Replace bytes `start..end` with `with`, then select `select`.
#[derive(Debug, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub end: usize,
    pub with: String,
    pub select: (usize, usize),
}

fn markers(style: Style) -> (&'static str, &'static str) {
    match style {
        Style::Bold => ("**", "**"),
        Style::Italic => ("*", "*"),
        Style::Underline => ("<u>", "</u>"),
        Style::Code => ("`", "`"),
        Style::Strike => ("~~", "~~"),
    }
}

fn stars_before(text: &[u8], at: usize) -> usize {
    text[..at].iter().rev().take_while(|&&b| b == b'*').count()
}

fn stars_after(text: &[u8], at: usize) -> usize {
    text[at..].iter().take_while(|&&b| b == b'*').count()
}

/// Whether star runs of these lengths hold this style: bold needs two stars, italic an odd
/// number (`*x*` or `***x***`).
fn runs_hold(style: Style, before: usize, after: usize) -> bool {
    match style {
        Style::Bold => before >= 2 && after >= 2,
        Style::Italic => before % 2 == 1 && after % 2 == 1,
        _ => false,
    }
}

/// The edit that turns `style` on or off for selection `from..to` (bytes) in `text`.
/// An empty selection inside a word works on the word; elsewhere it inserts an empty pair with
/// the caret between.
pub fn toggle(text: &[u8], from: usize, to: usize, word: (usize, usize), style: Style) -> Change {
    let (open, close) = markers(style);
    let (mut from, mut to) = (from.min(to), from.max(to).min(text.len()));
    if from == to && word.0 < word.1 {
        (from, to) = word;
    }
    // Word wraps the word, not the spaces a double click picks up.
    while from < to && text[from].is_ascii_whitespace() {
        from += 1;
    }
    while to > from && text[to - 1].is_ascii_whitespace() {
        to -= 1;
    }
    let inner = String::from_utf8_lossy(&text[from..to]).into_owned();
    // Markup just outside the selection: take it off.
    let is_outside = match style {
        Style::Bold | Style::Italic => {
            runs_hold(style, stars_before(text, from), stars_after(text, to))
        }
        _ => text[..from].ends_with(open.as_bytes()) && text[to..].starts_with(close.as_bytes()),
    };
    if is_outside {
        let start = from - open.len();
        return Change {
            start,
            end: to + close.len(),
            with: inner.clone(),
            select: (start, start + inner.len()),
        };
    }
    // Markup at the selection's own ends: take it off.
    let b = inner.as_bytes();
    let is_inside = b.len() >= open.len() + close.len()
        && match style {
            Style::Bold | Style::Italic => {
                runs_hold(style, stars_after(b, 0), stars_before(b, b.len()))
            }
            _ => inner.starts_with(open) && inner.ends_with(close),
        };
    if is_inside {
        let bare = inner[open.len()..inner.len() - close.len()].to_string();
        return Change {
            start: from,
            end: to,
            select: (from, from + bare.len()),
            with: bare,
        };
    }
    let start = from + open.len();
    Change {
        start: from,
        end: to,
        with: format!("{open}{inner}{close}"),
        select: (start, start + inner.len()),
    }
}

/// `[selection](url)` with `url` selected, ready to type the address over.
pub fn link(text: &[u8], from: usize, to: usize, word: (usize, usize)) -> Change {
    let (mut from, mut to) = (from.min(to), from.max(to).min(text.len()));
    if from == to && word.0 < word.1 {
        (from, to) = word;
    }
    let inner = String::from_utf8_lossy(&text[from..to]).into_owned();
    let url_at = from + 1 + inner.len() + 2;
    Change {
        start: from,
        end: to,
        with: format!("[{inner}](url)"),
        select: (url_at, url_at + "url".len()),
    }
}

/// Adds `prefix` (`# `, `- `, `> `) to every line the selection touches, or takes it off if
/// they all have it.
pub fn toggle_prefix(text: &[u8], from: usize, to: usize, prefix: &str) -> Change {
    let (from, to) = (from.min(to).min(text.len()), from.max(to).min(text.len()));
    let start = text[..from]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let end = text[to..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(text.len(), |i| to + i);
    let block = String::from_utf8_lossy(&text[start..end]).into_owned();
    let lines: Vec<&str> = block.split('\n').collect();
    let is_all = lines.iter().all(|l| l.starts_with(prefix));
    let with = lines
        .iter()
        .map(|l| match is_all {
            true => l[prefix.len()..].to_string(),
            false => format!("{prefix}{l}"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    Change {
        start,
        end,
        select: (start, start + with.len()),
        with,
    }
}

/// The text, the selection, and the word around the caret.
unsafe fn selection_and_word(edit: HWND) -> (Vec<u8>, usize, usize, (usize, usize)) {
    let text = editor::utf8(edit);
    let span = editor::selection(edit);
    let caret = span.start.max(0) as usize;
    let word = (
        SendMessageW(edit, sci::SCI_WORDSTARTPOSITION, caret, 1).max(0) as usize,
        SendMessageW(edit, sci::SCI_WORDENDPOSITION, caret, 1).max(0) as usize,
    );
    let (from, to) = (span.start.max(0) as usize, span.end.max(0) as usize);
    (text, from, to, word)
}

unsafe fn apply_change(edit: HWND, change: &Change) {
    editor::begin_undo(edit);
    editor::replace_range(edit, change.start, change.end, &change.with);
    editor::end_undo(edit);
    SendMessageW(
        edit,
        sci::SCI_SETSEL,
        change.select.0,
        change.select.1 as isize,
    );
}

/// Applies `style` to the selection in `edit` as one undo step.
pub unsafe fn apply(edit: HWND, style: Style) {
    let (text, from, to, word) = selection_and_word(edit);
    apply_change(edit, &toggle(&text, from, to, word, style));
}

pub unsafe fn apply_link(edit: HWND) {
    let (text, from, to, word) = selection_and_word(edit);
    apply_change(edit, &link(&text, from, to, word));
}

pub unsafe fn apply_prefix(edit: HWND, prefix: &str) {
    let (text, from, to, _) = selection_and_word(edit);
    apply_change(edit, &toggle_prefix(&text, from, to, prefix));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the change to `text` (selection marked with [ and ]) and returns the result with
    /// the new selection marked the same way.
    fn run(marked: &str, style: Style) -> String {
        let from = marked.find('[').unwrap();
        let to = marked.find(']').unwrap() - 1;
        let text = marked.replacen('[', "", 1).replacen(']', "", 1);
        let bytes = text.as_bytes();
        let word = word_at(bytes, from);
        let c = toggle(bytes, from, to, word, style);
        let mut out = String::new();
        out.push_str(&text[..c.start]);
        out.push_str(&c.with);
        out.push_str(&text[c.end..]);
        out.insert(c.select.1, ']');
        out.insert(c.select.0, '[');
        out
    }

    fn word_at(text: &[u8], at: usize) -> (usize, usize) {
        let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let start = at - text[..at].iter().rev().take_while(|&&b| is_word(b)).count();
        let end = at + text[at..].iter().take_while(|&&b| is_word(b)).count();
        (start, end)
    }

    #[test]
    fn wraps_and_unwraps() {
        assert_eq!(run("a [big] cat", Style::Bold), "a **[big]** cat");
        assert_eq!(run("a **[big]** cat", Style::Bold), "a [big] cat");
        assert_eq!(run("a [**big**] cat", Style::Bold), "a [big] cat");
        assert_eq!(run("a [big] cat", Style::Italic), "a *[big]* cat");
        assert_eq!(run("a *[big]* cat", Style::Italic), "a [big] cat");
        assert_eq!(run("a [big] cat", Style::Underline), "a <u>[big]</u> cat");
        assert_eq!(run("a <u>[big]</u> cat", Style::Underline), "a [big] cat");
        assert_eq!(run("a [<u>big</u>] cat", Style::Underline), "a [big] cat");
    }

    #[test]
    fn bold_and_italic_nest() {
        // Italic inside bold adds a star instead of eating one of the bold pair.
        assert_eq!(run("**[big]**", Style::Italic), "***[big]***");
        assert_eq!(run("***[big]***", Style::Italic), "**[big]**");
        assert_eq!(run("***[big]***", Style::Bold), "*[big]*");
        assert_eq!(run("*[big]*", Style::Bold), "***[big]***");
    }

    #[test]
    fn empty_selection_uses_the_word_or_inserts_a_pair() {
        assert_eq!(run("a bi[]g cat", Style::Bold), "a **[big]** cat");
        assert_eq!(run("a **bi[]g** cat", Style::Bold), "a [big] cat");
        assert_eq!(run("end [] ", Style::Italic), "end *[]* ");
    }

    #[test]
    fn code_strike_and_link() {
        assert_eq!(run("a [big] cat", Style::Code), "a `[big]` cat");
        assert_eq!(run("a `[big]` cat", Style::Code), "a [big] cat");
        assert_eq!(run("a [big] cat", Style::Strike), "a ~~[big]~~ cat");
        assert_eq!(run("a ~~[big]~~ cat", Style::Strike), "a [big] cat");
        let c = link(b"see docs now", 4, 8, (4, 8));
        assert_eq!(c.with, "[docs](url)");
        assert_eq!(&c.with[c.select.0 - c.start..c.select.1 - c.start], "url");
    }

    #[test]
    fn prefixes_go_on_every_touched_line_and_come_off() {
        let c = toggle_prefix(b"one\ntwo\nthree", 1, 5, "- ");
        assert_eq!((c.start, c.end, c.with.as_str()), (0, 7, "- one\n- two"));
        let c = toggle_prefix(b"> one\n> two\nthree", 2, 8, "> ");
        assert_eq!(c.with, "one\ntwo");
        assert_eq!(toggle_prefix(b"title", 0, 0, "# ").with, "# title");
    }

    #[test]
    fn spaces_at_the_ends_stay_outside() {
        assert_eq!(run("a[ big ]cat", Style::Bold), "a **[big]** cat");
    }
}
