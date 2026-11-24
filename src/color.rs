//! Text color and highlight for the selection, in the files that can store them: Markdown and
//! HTML. Both are saved as HTML, `<span style="color:#c00000">red</span>` and
//! `<span style="background-color:#ffff00">marked</span>`, which Markdown allows inline. Plain
//! text and code can't hold color, so there the commands are dimmed and do nothing.
//!
//! The editor shows these spans (and `<u>` underline) in their colors, in the source and in the
//! Rendered view, with Scintilla indicators: the text itself never changes to show them.

use std::ops::Range;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;

use crate::format::Change;
use crate::sci;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Text,
    Highlight,
}

/// A `<span>` (or `<u>`) with its closing tag, and what it does to the text between.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tagged {
    pub open: Range<usize>,
    pub close: Range<usize>,
    pub fg: Option<u32>,
    pub bg: Option<u32>,
    pub is_underline: bool,
}

impl Tagged {
    pub fn inner(&self) -> Range<usize> {
        self.open.end..self.close.start
    }

    /// A span that does exactly one thing: sets the color `kind` sets.
    fn is_only(&self, kind: Kind) -> bool {
        match kind {
            Kind::Text => self.fg.is_some() && self.bg.is_none(),
            Kind::Highlight => self.bg.is_some() && self.fg.is_none(),
        }
    }
}

const CLOSE_SPAN: &[u8] = b"</span>";
const OPEN_U: &[u8] = b"<u>";
const CLOSE_U: &[u8] = b"</u>";
/// Longest opening `<span ...>` tag looked at; real ones are far shorter.
const MAX_TAG: usize = 512;

fn starts_ci(text: &[u8], prefix: &[u8]) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}

/// `#rgb` or `#rrggbb` as a COLORREF (0x00BBGGRR).
fn parse_hex(value: &str) -> Option<u32> {
    let digits = value.trim().strip_prefix('#')?;
    let full = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => digits.to_string(),
        _ => return None,
    };
    let v = u32::from_str_radix(&full, 16).ok()?;
    Some(crate::themes::hex(v))
}

/// A COLORREF as `#rrggbb`.
pub fn to_hex(color: u32) -> String {
    let (r, g, b) = (color & 0xFF, (color >> 8) & 0xFF, (color >> 16) & 0xFF);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The text and background colors set by a tag's `style` attribute.
fn style_colors(tag: &[u8]) -> (Option<u32>, Option<u32>) {
    let tag = String::from_utf8_lossy(tag);
    let lower = tag.to_ascii_lowercase();
    let Some(at) = lower.find("style=") else {
        return (None, None);
    };
    let rest = &tag[at + "style=".len()..];
    let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
        return (None, None);
    };
    let value = rest[1..].split(quote).next().unwrap_or("");
    let (mut fg, mut bg) = (None, None);
    for decl in value.split(';') {
        let Some((name, v)) = decl.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "color" => fg = parse_hex(v),
            "background-color" | "background" => bg = parse_hex(v),
            _ => {}
        }
    }
    (fg, bg)
}

/// An opening tag still waiting for its close.
struct Opened {
    is_span: bool,
    open: Range<usize>,
    fg: Option<u32>,
    bg: Option<u32>,
}

/// Every `<span ...>...</span>` and `<u>...</u>` in `text`, nested ones included, in the order
/// they open. Unmatched tags are left out.
pub fn tags(text: &[u8]) -> Vec<Tagged> {
    // Open tags waiting for their close.
    let mut stack: Vec<Opened> = Vec::new();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].iter().position(|&b| b == b'<') {
        let at = i + off;
        let rest = &text[at..];
        let closes = [(CLOSE_SPAN, true), (CLOSE_U, false)];
        if let Some(&(close, is_span)) = closes.iter().find(|(c, _)| starts_ci(rest, c)) {
            i = at + close.len();
            let Some(k) = stack.iter().rposition(|e| e.is_span == is_span) else {
                continue;
            };
            let o = stack.remove(k);
            out.push(Tagged {
                open: o.open,
                close: at..i,
                fg: o.fg,
                bg: o.bg,
                is_underline: !is_span,
            });
            continue;
        }
        if starts_ci(rest, OPEN_U) {
            stack.push(Opened {
                is_span: false,
                open: at..at + OPEN_U.len(),
                fg: None,
                bg: None,
            });
            i = at + OPEN_U.len();
            continue;
        }
        let is_span = starts_ci(rest, b"<span")
            && rest
                .get(5)
                .is_some_and(|&b| b == b'>' || b.is_ascii_whitespace());
        let end = rest.iter().take(MAX_TAG).position(|&b| b == b'>');
        if let (true, Some(gt)) = (is_span, end) {
            let (fg, bg) = style_colors(&rest[..gt]);
            stack.push(Opened {
                is_span: true,
                open: at..at + gt + 1,
                fg,
                bg,
            });
            i = at + gt + 1;
            continue;
        }
        i = at + 1;
    }
    out.sort_by_key(|t| t.open.start);
    out
}

fn open_tag(kind: Kind, color: u32) -> String {
    let property = match kind {
        Kind::Text => "color",
        Kind::Highlight => "background-color",
    };
    format!("<span style=\"{property}:{}\">", to_hex(color))
}

fn span_text(kind: Kind, color: u32, inner: &str) -> String {
    format!("{}{inner}</span>", open_tag(kind, color))
}

/// The selection with an empty one widened to the word and spaces at the ends left out.
fn target(text: &[u8], from: usize, to: usize, word: (usize, usize)) -> (usize, usize) {
    let (mut from, mut to) = (from.min(to), from.max(to).min(text.len()));
    if from == to && word.0 < word.1 {
        (from, to) = word;
    }
    while from < to && text[from].is_ascii_whitespace() {
        from += 1;
    }
    while to > from && text[to - 1].is_ascii_whitespace() {
        to -= 1;
    }
    (from, to)
}

/// Whether a color command has text to work on: a selection, or a word at the caret.
pub fn has_target(text: &[u8], from: usize, to: usize, word: (usize, usize)) -> bool {
    let (from, to) = target(text, from, to, word);
    from < to
}

/// The one-job span of `kind` that is the selection, or holds exactly the selection.
fn exact_span(text: &[u8], from: usize, to: usize, kind: Kind) -> Option<Tagged> {
    tags(text).into_iter().find(|t| {
        t.is_only(kind)
            && ((t.open.end == from && t.close.start == to)
                || (t.open.start == from && t.close.end == to))
    })
}

/// The edit that gives `from..to` color `color`. A span of the same kind already around the
/// selection gets the new color instead of a second span. A selection over several lines gets
/// a span on each line, so each line's color shows in the Rendered view.
pub fn apply(
    text: &[u8],
    from: usize,
    to: usize,
    word: (usize, usize),
    kind: Kind,
    color: u32,
) -> Option<Change> {
    let (from, to) = target(text, from, to, word);
    if from == to {
        return None;
    }
    if let Some(span) = exact_span(text, from, to, kind) {
        let inner = String::from_utf8_lossy(&text[span.inner()]).into_owned();
        let open = open_tag(kind, color);
        let start = span.open.start;
        return Some(Change {
            start,
            end: span.close.end,
            select: (start + open.len(), start + open.len() + inner.len()),
            with: span_text(kind, color, &inner),
        });
    }
    let block = String::from_utf8_lossy(&text[from..to]).into_owned();
    let with = block
        .split('\n')
        .map(|line| {
            let body = line.trim_end_matches('\r');
            let trimmed = body.trim();
            if trimmed.is_empty() {
                return line.to_string();
            }
            let lead = &body[..body.len() - body.trim_start().len()];
            let tail = &line[lead.len() + trimmed.len()..];
            format!("{lead}{}{tail}", span_text(kind, color, trimmed))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let select = match block.contains('\n') {
        true => (from, from + with.len()),
        // Spaces `target` left (non-ASCII ones) stay outside the span; select just the text.
        false => {
            let body = block.trim_end_matches('\r');
            let lead = body.len() - body.trim_start().len();
            let start = from + lead + open_tag(kind, color).len();
            (start, start + body.trim().len())
        }
    };
    Some(Change {
        start: from,
        end: to,
        with,
        select,
    })
}

/// Takes off the innermost text color or highlight span around (or exactly at) the selection.
pub fn remove(text: &[u8], from: usize, to: usize, word: (usize, usize)) -> Option<Change> {
    let (from, to) = target(text, from, to, word);
    let span = tags(text)
        .into_iter()
        .filter(|t| t.fg.is_some() || t.bg.is_some())
        .filter(|t| t.open.start <= from && to <= t.close.end)
        .min_by_key(|t| t.close.end - t.open.start)?;
    let inner = String::from_utf8_lossy(&text[span.inner()]).into_owned();
    let start = span.open.start;
    Some(Change {
        start,
        end: span.close.end,
        select: (start, start + inner.len()),
        with: inner,
    })
}

const TEXT_INDICATOR: usize = 9;
const BACK_INDICATOR: usize = 10;
const UNDERLINE_INDICATOR: usize = 11;
const OPAQUE: isize = 255;

unsafe fn send(edit: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    SendMessageW(edit, msg, wparam, lparam)
}

unsafe fn define(edit: HWND, fg: u32) {
    send(
        edit,
        sci::SCI_INDICSETSTYLE,
        TEXT_INDICATOR,
        sci::INDIC_TEXTFORE as isize,
    );
    send(
        edit,
        sci::SCI_INDICSETFLAGS,
        TEXT_INDICATOR,
        sci::SC_INDICFLAG_VALUEFORE,
    );
    send(
        edit,
        sci::SCI_INDICSETSTYLE,
        BACK_INDICATOR,
        sci::INDIC_STRAIGHTBOX as isize,
    );
    send(
        edit,
        sci::SCI_INDICSETFLAGS,
        BACK_INDICATOR,
        sci::SC_INDICFLAG_VALUEFORE,
    );
    send(edit, sci::SCI_INDICSETUNDER, BACK_INDICATOR, 1);
    send(edit, sci::SCI_INDICSETALPHA, BACK_INDICATOR, OPAQUE);
    send(edit, sci::SCI_INDICSETOUTLINEALPHA, BACK_INDICATOR, 0);
    send(
        edit,
        sci::SCI_INDICSETSTYLE,
        UNDERLINE_INDICATOR,
        sci::INDIC_PLAIN as isize,
    );
    send(
        edit,
        sci::SCI_INDICSETFORE,
        UNDERLINE_INDICATOR,
        fg as isize,
    );
}

unsafe fn fill(edit: HWND, indicator: usize, range: Range<usize>, value: u32) {
    send(edit, sci::SCI_SETINDICATORCURRENT, indicator, 0);
    send(
        edit,
        sci::SCI_SETINDICATORVALUE,
        (value | sci::SC_INDICVALUEBIT) as usize,
        0,
    );
    send(
        edit,
        sci::SCI_INDICATORFILLRANGE,
        range.start,
        range.len() as isize,
    );
}

/// Shows the colors and underlines in `edit`'s text, or clears them when `is_on` is false (a
/// file type that can't hold them). `fg` is the editor's text color, for underlines.
pub unsafe fn paint(edit: HWND, is_on: bool, fg: u32) {
    let len = send(edit, sci::SCI_GETLENGTH, 0, 0).max(0) as usize;
    for indicator in [TEXT_INDICATOR, BACK_INDICATOR, UNDERLINE_INDICATOR] {
        send(edit, sci::SCI_SETINDICATORCURRENT, indicator, 0);
        send(edit, sci::SCI_INDICATORCLEARRANGE, 0, len as isize);
    }
    if !is_on {
        return;
    }
    define(edit, fg);
    let ptr = send(edit, sci::SCI_GETCHARACTERPOINTER, 0, 0) as *const u8;
    if ptr.is_null() {
        return;
    }
    let text = std::slice::from_raw_parts(ptr, len);
    // Outer spans first, so a span inside another paints over it.
    for t in tags(text) {
        if let Some(fg) = t.fg {
            fill(edit, TEXT_INDICATOR, t.inner(), fg);
        }
        if let Some(bg) = t.bg {
            fill(edit, BACK_INDICATOR, t.inner(), bg);
        }
        if t.is_underline {
            fill(edit, UNDERLINE_INDICATOR, t.inner(), fg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::hex;

    const RED: u32 = hex(0xC00000);
    const BLUE: u32 = hex(0x0000FF);
    const YELLOW: u32 = hex(0xFFFF00);

    /// Applies `c` to `text` and marks the new selection with [ and ].
    fn show(text: &str, c: Change) -> String {
        let mut out = String::new();
        out.push_str(&text[..c.start]);
        out.push_str(&c.with);
        out.push_str(&text[c.end..]);
        out.insert(c.select.1, ']');
        out.insert(c.select.0, '[');
        out
    }

    fn color(marked: &str, kind: Kind, value: u32) -> String {
        let from = marked.find('[').unwrap();
        let to = marked.find(']').unwrap() - 1;
        let text = marked.replacen('[', "", 1).replacen(']', "", 1);
        let c = apply(text.as_bytes(), from, to, (from, from), kind, value).unwrap();
        show(&text, c)
    }

    #[test]
    fn colors_and_highlights_the_selection() {
        assert_eq!(
            color("a [big] cat", Kind::Text, RED),
            "a <span style=\"color:#c00000\">[big]</span> cat"
        );
        assert_eq!(
            color("a [big] cat", Kind::Highlight, YELLOW),
            "a <span style=\"background-color:#ffff00\">[big]</span> cat"
        );
        assert_eq!(
            color("a[ big ]cat", Kind::Text, RED),
            "a <span style=\"color:#c00000\">[big]</span> cat"
        );
    }

    #[test]
    fn a_second_color_replaces_the_first() {
        let once = "a <span style=\"color:#c00000\">[big]</span> cat";
        assert_eq!(
            color(once, Kind::Text, BLUE),
            "a <span style=\"color:#0000ff\">[big]</span> cat"
        );
        // A highlight on colored text nests instead of replacing the color.
        assert_eq!(
            color(once, Kind::Highlight, YELLOW),
            "a <span style=\"color:#c00000\"><span style=\"background-color:#ffff00\">[big]</span></span> cat"
        );
    }

    #[test]
    fn a_leading_non_breaking_space_stays_outside_and_out_of_the_selection() {
        assert_eq!(
            color("a [\u{a0}big] cat", Kind::Text, RED),
            "a \u{a0}<span style=\"color:#c00000\">[big]</span> cat"
        );
        assert!(!has_target(b"a   b", 2, 2, (2, 2)));
    }

    #[test]
    fn every_line_gets_its_own_span() {
        let c = color("[one\n\ntwo]", Kind::Text, RED);
        assert_eq!(
            c,
            "[<span style=\"color:#c00000\">one</span>\n\n<span style=\"color:#c00000\">two</span>]"
        );
    }

    #[test]
    fn removes_the_innermost_color() {
        let text = "a <span style=\"color:#c00000\">big</span> cat";
        let at = text.find("big").unwrap();
        let c = remove(text.as_bytes(), at + 1, at + 1, (at, at + 3)).unwrap();
        assert_eq!(show(text, c), "a [big] cat");
        assert!(remove(b"plain words", 2, 4, (0, 5)).is_none());
    }

    #[test]
    fn finds_nested_spans_and_underlines() {
        let text =
            b"<SPAN style='color:#f00; background-color:#0000ff'>x<u>y</u></SPAN> <span>z</span>";
        let t = tags(text);
        assert_eq!(t.len(), 3);
        assert_eq!(
            (t[0].fg, t[0].bg),
            (Some(hex(0xFF0000)), Some(hex(0x0000FF)))
        );
        assert!(t[1].is_underline);
        assert_eq!(&text[t[1].inner()], b"y");
        assert_eq!((t[2].fg, t[2].bg), (None, None));
        assert!(tags(b"<span style=\"color:#f00\">never closed").is_empty());
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(to_hex(RED), "#c00000");
        assert_eq!(parse_hex("#c00000"), Some(RED));
        assert_eq!(parse_hex("red"), None);
    }
}
