//! Rendered Markdown: a tab keeps its Markdown text and changes only how it looks.
//!
//! Markup such as `**`, `# `, and `](url)` is hidden, and shown dimmed while the caret is inside
//! the element it belongs to, so it can be edited. Headings are larger, and bold, italic, code,
//! links, quotes, lists, and tables are styled. Nothing is converted: typing, undo, find, and save
//! all work on the Markdown itself, so text that is not edited keeps its exact bytes.
//!
//! Styling is a full pass over the text (a 100 KB file takes well under a millisecond), handed
//! to Scintilla with SCI_SETSTYLINGEX, which only repaints the bytes whose style changed.

use std::ops::Range;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW;

use crate::sci::*;
use crate::theme::Look;
use crate::themes;

/// Markup that is out of the caret's way.
pub const HIDDEN: u8 = 1;
/// Markup shown dimmed: near the caret, or always (quote markers, table pipes, fences).
pub const MARK: u8 = 2;
pub const CODE_BLOCK: u8 = 3;
pub const LIST_MARK: u8 = 4;
pub const RULE: u8 = 5;
/// Styles 32 to 39 are Scintilla's own, so text styles start above them.
const FIRST_TEXT_STYLE: u8 = 40;

const BODY_FACE: &str = "Segoe UI";
const MARK_MIX_PERCENT: u32 = 45;
const QUOTE_MIX_PERCENT: u32 = 70;
const CODE_BACK_MIX_PERCENT: u32 = 9;
/// Heading sizes in percent of the body size. Scintilla gives every line the height of the
/// tallest style, so these stay modest.
const HEADING_PERCENT: [u32; 3] = [130, 115, 105];
const HUNDREDTHS_PER_TENTH: u32 = 10;
const MAX_ATX_LEVEL: usize = 6;
const MAX_BLOCK_INDENT: usize = 3;
const MIN_FENCE: usize = 3;
const MIN_RULE: usize = 3;
const MAX_LIST_DIGITS: usize = 9;
/// How far past an opening backtick run to look for its close. Real code spans are far shorter;
/// the limit keeps a long line of unmatched ones from slowing every caret move.
const MAX_SPAN: usize = 8 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Block {
    Body,
    Quote,
    H1,
    H2,
    H3,
    TableHead,
}

const BLOCKS: [Block; 6] = [
    Block::Body,
    Block::Quote,
    Block::H1,
    Block::H2,
    Block::H3,
    Block::TableHead,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inline {
    Plain,
    Bold,
    Italic,
    BoldItalic,
    Code,
    Link,
}

const INLINES: [Inline; 6] = [
    Inline::Plain,
    Inline::Bold,
    Inline::Italic,
    Inline::BoldItalic,
    Inline::Code,
    Inline::Link,
];

pub const fn text_style(block: Block, inline: Inline) -> u8 {
    FIRST_TEXT_STYLE + block as u8 * INLINES.len() as u8 + inline as u8
}

/// Plain body text, also used for line ends and blank lines.
pub const TEXT: u8 = text_style(Block::Body, Inline::Plain);

const BOLD: u8 = 1;
const ITALIC: u8 = 2;
const CODE: u8 = 4;
const LINK: u8 = 8;

fn inline_of(flags: u8) -> Inline {
    if flags & CODE != 0 {
        Inline::Code
    } else if flags & LINK != 0 {
        Inline::Link
    } else {
        match (flags & BOLD != 0, flags & ITALIC != 0) {
            (true, true) => Inline::BoldItalic,
            (true, false) => Inline::Bold,
            (false, true) => Inline::Italic,
            (false, false) => Inline::Plain,
        }
    }
}

/// Markup bytes and the whole element they belong to; the caret anywhere in the element
/// (ends included) shows them.
struct Markup {
    bytes: Range<usize>,
    element: Range<usize>,
}

/// A run of `*` or `_` that may open or close emphasis. Openers use up their bytes from the
/// right, closers from the left, so `left..right` is what is still unused.
struct Run {
    ch: u8,
    left: usize,
    right: usize,
    can_open: bool,
    can_close: bool,
}

/// The style of every byte of `text` with the selection over `selected` (just the caret when it is
/// empty). Markup of every element the selection touches shows, which also keeps it editable:
/// Scintilla protects invisible text, refusing to delete or type over a selection holding any.
pub fn styles(text: &[u8], selected: Range<usize>) -> Vec<u8> {
    let mut p = Parser {
        text,
        selected,
        out: vec![TEXT; text.len()],
    };
    p.blocks();
    p.out
}

/// Byte ranges of each line without its line break, plus where the next line starts.
fn lines(text: &[u8]) -> Vec<(Range<usize>, usize)> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let lf = text[start..].iter().position(|&b| b == b'\n' || b == b'\r');
        let Some(lf) = lf.map(|n| start + n) else {
            lines.push((start..text.len(), text.len()));
            break;
        };
        let next = if text[lf] == b'\r' && text.get(lf + 1) == Some(&b'\n') {
            lf + 2
        } else {
            lf + 1
        };
        lines.push((start..lf, next));
        start = next;
    }
    lines
}

fn indent(line: &[u8]) -> usize {
    line.iter().take_while(|&&b| b == b' ').count()
}

/// `#` count and the length of the marker with its trailing spaces.
fn heading(line: &[u8]) -> Option<(usize, usize)> {
    let ind = indent(line);
    if ind > MAX_BLOCK_INDENT {
        return None;
    }
    let level = line[ind..].iter().take_while(|&&b| b == b'#').count();
    let after = ind + level;
    let is_spaced = after == line.len() || line[after] == b' ' || line[after] == b'\t';
    if level == 0 || level > MAX_ATX_LEVEL || !is_spaced {
        return None;
    }
    let spaces = line[after..]
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    Some((level, after + spaces))
}

fn is_rule(line: &[u8]) -> bool {
    if indent(line) > MAX_BLOCK_INDENT {
        return false;
    }
    let marks: Vec<u8> = line
        .iter()
        .copied()
        .filter(|&b| b != b' ' && b != b'\t')
        .collect();
    marks.len() >= MIN_RULE
        && matches!(marks[0], b'-' | b'*' | b'_')
        && marks.iter().all(|&b| b == marks[0])
}

/// The fence character and length of a line that opens a fenced code block.
fn fence(line: &[u8]) -> Option<(u8, usize)> {
    let ind = indent(line);
    let ch = *line.get(ind)?;
    if ind > MAX_BLOCK_INDENT || (ch != b'`' && ch != b'~') {
        return None;
    }
    let n = line[ind..].iter().take_while(|&&b| b == ch).count();
    let is_info_ok = ch == b'~' || !line[ind + n..].contains(&b'`');
    (n >= MIN_FENCE && is_info_ok).then_some((ch, n))
}

fn closes_fence(line: &[u8], ch: u8, n: usize) -> bool {
    let ind = indent(line);
    let run = line[ind..].iter().take_while(|&&b| b == ch).count();
    ind <= MAX_BLOCK_INDENT && run >= n && line[ind + run..].iter().all(|b| b.is_ascii_whitespace())
}

fn is_table_row(line: &[u8]) -> bool {
    let ind = indent(line);
    line.get(ind) == Some(&b'|') && line[ind + 1..].contains(&b'|')
}

fn is_table_delimiter(line: &[u8]) -> bool {
    is_table_row(line)
        && line.contains(&b'-')
        && line
            .iter()
            .all(|b| matches!(b, b'|' | b'-' | b':' | b' ' | b'\t'))
}

/// Length of the `>` markers (with one space after each) at the start of `line`.
fn quote_prefix(line: &[u8]) -> usize {
    let mut at = 0;
    loop {
        let ind = indent(&line[at..]);
        if ind > MAX_BLOCK_INDENT || line.get(at + ind) != Some(&b'>') {
            return at;
        }
        at += ind + 1;
        if line.get(at) == Some(&b' ') {
            at += 1;
        }
    }
}

/// Where a list marker (with its space and any task box) starts and ends in `line`.
fn list_marker(line: &[u8]) -> Option<Range<usize>> {
    let start = line
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    let rest = &line[start..];
    let marker = match rest.first()? {
        b'-' | b'*' | b'+' => 1,
        b'0'..=b'9' => {
            let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
            let is_delim = matches!(rest.get(digits), Some(b'.' | b')'));
            if digits > MAX_LIST_DIGITS || !is_delim {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let mut end = start + marker;
    match line.get(end) {
        None => return Some(start..end),
        Some(b' ' | b'\t') => end += 1,
        Some(_) => return None,
    }
    let is_task = line.len() >= end + 4
        && line[end] == b'['
        && matches!(line[end + 1], b' ' | b'x' | b'X')
        && line[end + 2] == b']'
        && line[end + 3] == b' ';
    if is_task {
        end += 4;
    }
    Some(start..end)
}

fn run_length(text: &[u8], at: usize, end: usize, ch: u8) -> usize {
    text[at..end].iter().take_while(|&&b| b == ch).count()
}

/// Start of the next run of exactly `n` backticks in `from..end`.
fn closing_backticks(text: &[u8], from: usize, end: usize, n: usize) -> Option<usize> {
    let end = end.min(from.saturating_add(MAX_SPAN));
    let mut i = from;
    while i < end {
        if text[i] != b'`' {
            i += 1;
            continue;
        }
        let run = run_length(text, i, end, b'`');
        if run == n {
            return Some(i);
        }
        i += run;
    }
    None
}

/// Where each `[` and `(` in `range` closes (escaped ones skipped), found in one pass so a
/// line full of unmatched brackets costs no more than any other line. Indexed from
/// `range.start`; NONE where a bracket does not close.
fn bracket_pairs(text: &[u8], range: Range<usize>) -> Vec<usize> {
    let mut closes = vec![NONE; range.len()];
    let (mut squares, mut parens) = (Vec::new(), Vec::new());
    let mut i = range.start;
    while i < range.end {
        match text[i] {
            b'\\' => i += 1,
            b'[' => squares.push(i),
            b'(' => parens.push(i),
            b']' => {
                if let Some(open) = squares.pop() {
                    closes[open - range.start] = i;
                }
            }
            b')' => {
                if let Some(open) = parens.pop() {
                    closes[open - range.start] = i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    closes
}

const NONE: usize = usize::MAX;

/// For `[` at `open`: the index of the matching `]` and of the `)` that ends `(destination)`.
fn link_ends(text: &[u8], pairs: &[usize], start: usize, open: usize) -> Option<(usize, usize)> {
    let close = pairs[open - start];
    if close == NONE || text.get(close + 1) != Some(&b'(') {
        return None;
    }
    let paren = *pairs.get(close + 1 - start)?;
    (paren != NONE).then_some((close, paren))
}

const URL_SCHEMES: [&[u8]; 2] = [b"https://", b"http://"];
const AUTOLINK_SCHEMES: [&[u8]; 3] = [b"https://", b"http://", b"mailto:"];

fn starts_with_any(text: &[u8], schemes: &[&[u8]]) -> bool {
    schemes.iter().any(|s| text.starts_with(s))
}

/// End of a bare URL starting at `at`, leaving out trailing punctuation.
fn bare_url_end(text: &[u8], at: usize, end: usize) -> usize {
    let mut j = at;
    while j < end && !text[j].is_ascii_whitespace() && text[j] != b'<' {
        j += 1;
    }
    let has_open_paren = text[at..j].contains(&b'(');
    while j > at {
        let b = text[j - 1];
        let is_trailing = matches!(
            b,
            b'.' | b',' | b';' | b':' | b'!' | b'?' | b'\'' | b'"' | b'*' | b'_'
        ) || (b == b')' && !has_open_paren);
        if !is_trailing {
            break;
        }
        j -= 1;
    }
    j
}

/// CommonMark's flanking rules for a delimiter run between `before` and `after`.
fn can_open_close(ch: u8, before: u8, after: u8) -> (bool, bool) {
    let ws = |b: u8| b.is_ascii_whitespace();
    let punct = |b: u8| b.is_ascii_punctuation();
    let left = !ws(after) && (!punct(after) || ws(before) || punct(before));
    let right = !ws(before) && (!punct(before) || ws(after) || punct(after));
    if ch == b'*' {
        (left, right)
    } else {
        (
            left && (!right || punct(before)),
            right && (!left || punct(after)),
        )
    }
}

struct Parser<'a> {
    text: &'a [u8],
    selected: Range<usize>,
    out: Vec<u8>,
}

impl Parser<'_> {
    fn fill(&mut self, range: Range<usize>, style: u8) {
        self.out[range].fill(style);
    }

    fn is_near(&self, element: &Range<usize>) -> bool {
        element.start <= self.selected.end && self.selected.start <= element.end
    }

    fn markup(&mut self, m: &Markup) {
        let style = if self.is_near(&m.element) {
            MARK
        } else {
            HIDDEN
        };
        self.fill(m.bytes.clone(), style);
    }

    fn blocks(&mut self) {
        let lines = lines(self.text);
        let mut open_fence: Option<(u8, usize)> = None;
        for (n, (line, next)) in lines.iter().enumerate() {
            let text = &self.text[line.clone()];
            if let Some((ch, len)) = open_fence {
                if closes_fence(text, ch, len) {
                    open_fence = None;
                    self.fill(line.clone(), MARK);
                } else {
                    self.fill(line.start..*next, CODE_BLOCK);
                }
                continue;
            }
            if let Some(f) = fence(text) {
                open_fence = Some(f);
                self.fill(line.clone(), MARK);
                continue;
            }
            let next_line = lines.get(n + 1).map(|(r, _)| &self.text[r.clone()]);
            self.line(line.clone(), next_line);
        }
    }

    fn line(&mut self, line: Range<usize>, next: Option<&[u8]>) {
        let text = &self.text[line.clone()];
        if let Some((level, marker)) = heading(text) {
            let block = match level {
                1 => Block::H1,
                2 => Block::H2,
                _ => Block::H3,
            };
            self.markup(&Markup {
                bytes: line.start..line.start + marker,
                element: line.clone(),
            });
            self.inline(line.start + marker..line.end, block, 0);
            return;
        }
        if is_rule(text) {
            self.fill(line, RULE);
            return;
        }
        if is_table_row(text) {
            self.table_row(line, next);
            return;
        }
        let quote = quote_prefix(text);
        let block = if quote > 0 { Block::Quote } else { Block::Body };
        self.fill(line.start..line.start + quote, MARK);
        let mut start = line.start + quote;
        if let Some(marker) = list_marker(&self.text[start..line.end]) {
            self.fill(start + marker.start..start + marker.end, LIST_MARK);
            start += marker.end;
        }
        self.inline(start..line.end, block, 0);
    }

    fn table_row(&mut self, line: Range<usize>, next: Option<&[u8]>) {
        let text = &self.text[line.clone()];
        if is_table_delimiter(text) {
            self.fill(line, MARK);
            return;
        }
        let is_head = next.is_some_and(is_table_delimiter);
        let block = if is_head {
            Block::TableHead
        } else {
            Block::Body
        };
        self.inline(line.clone(), block, 0);
        let code = text_style(block, Inline::Code);
        for i in line {
            if self.text[i] == b'|' && self.out[i] != code {
                self.out[i] = MARK;
            }
        }
    }

    /// Styles the inline text in `range`: code spans, links, and emphasis.
    fn inline(&mut self, range: Range<usize>, block: Block, base: u8) {
        let mut flags = vec![base; range.len()];
        let mut marks = Vec::new();
        let mut labels = Vec::new();
        let runs = self.scan(range.clone(), &mut flags, &mut marks, &mut labels);
        emphasis(runs, range.start, &mut flags, &mut marks);
        for (i, f) in flags.iter().enumerate() {
            self.out[range.start + i] = text_style(block, inline_of(*f));
        }
        for m in &marks {
            self.markup(m);
        }
        // Emphasis does not cross into a link's text; the text gets its own pass.
        for label in labels {
            let outer = flags[label.start - range.start];
            self.inline(label, block, outer | LINK);
        }
    }

    /// Finds code spans, escapes, links, and URLs, and returns the emphasis delimiter runs
    /// outside them.
    fn scan(
        &self,
        range: Range<usize>,
        flags: &mut [u8],
        marks: &mut Vec<Markup>,
        labels: &mut Vec<Range<usize>>,
    ) -> Vec<Run> {
        let (t, start, end) = (self.text, range.start, range.end);
        let is_in_link = flags.first().is_some_and(|f| f & LINK != 0);
        let mut set = |r: Range<usize>, f: u8| {
            flags[r.start - start..r.end - start]
                .iter_mut()
                .for_each(|x| *x |= f)
        };
        let pairs = bracket_pairs(t, range.clone());
        let mut runs = Vec::new();
        let mut i = start;
        while i < end {
            match t[i] {
                b'\\' if i + 1 < end && t[i + 1].is_ascii_punctuation() => {
                    marks.push(Markup {
                        bytes: i..i + 1,
                        element: i..i + 2,
                    });
                    i += 2;
                }
                b'`' => {
                    let n = run_length(t, i, end, b'`');
                    let Some(close) = closing_backticks(t, i + n, end, n) else {
                        i += n;
                        continue;
                    };
                    let element = i..close + n;
                    marks.push(Markup {
                        bytes: i..i + n,
                        element: element.clone(),
                    });
                    marks.push(Markup {
                        bytes: close..close + n,
                        element,
                    });
                    set(i + n..close, CODE);
                    i = close + n;
                }
                // Links do not contain links, so a link's text is not searched for more.
                b'[' | b'!' if !is_in_link => {
                    let open = if t[i] == b'!' { i + 1 } else { i };
                    let ends = (t.get(open) == Some(&b'['))
                        .then(|| link_ends(t, &pairs, start, open))
                        .flatten();
                    let Some((close, paren)) = ends else {
                        i += 1;
                        continue;
                    };
                    let element = i..paren + 1;
                    marks.push(Markup {
                        bytes: i..open + 1,
                        element: element.clone(),
                    });
                    marks.push(Markup {
                        bytes: close..paren + 1,
                        element,
                    });
                    labels.push(open + 1..close);
                    i = paren + 1;
                }
                b'<' if starts_with_any(&t[i + 1..end], &AUTOLINK_SCHEMES) => {
                    let close = t[i..end]
                        .iter()
                        .position(|&b| b == b'>' || b.is_ascii_whitespace())
                        .map(|n| i + n)
                        .filter(|&c| t[c] == b'>');
                    let Some(close) = close else {
                        i += 1;
                        continue;
                    };
                    let element = i..close + 1;
                    marks.push(Markup {
                        bytes: i..i + 1,
                        element: element.clone(),
                    });
                    marks.push(Markup {
                        bytes: close..close + 1,
                        element,
                    });
                    set(i + 1..close, LINK);
                    i = close + 1;
                }
                b'h' if (i == start || !t[i - 1].is_ascii_alphanumeric())
                    && starts_with_any(&t[i..end], &URL_SCHEMES) =>
                {
                    let url_end = bare_url_end(t, i, end);
                    set(i..url_end, LINK);
                    i = url_end.max(i + 1);
                }
                ch @ (b'*' | b'_') => {
                    let n = run_length(t, i, end, ch);
                    let before = if i > start { t[i - 1] } else { b' ' };
                    let after = if i + n < end { t[i + n] } else { b' ' };
                    let (can_open, can_close) = can_open_close(ch, before, after);
                    runs.push(Run {
                        ch,
                        left: i,
                        right: i + n,
                        can_open,
                        can_close,
                    });
                    i += n;
                }
                _ => i += 1,
            }
        }
        runs
    }
}

/// Pairs delimiter runs into bold (two characters) and italic (one), CommonMark style.
fn emphasis(mut runs: Vec<Run>, start: usize, flags: &mut [u8], marks: &mut Vec<Markup>) {
    let mut openers: Vec<usize> = Vec::new();
    for k in 0..runs.len() {
        if runs[k].can_close {
            while runs[k].right > runs[k].left {
                let ch = runs[k].ch;
                let Some(at) = openers.iter().rposition(|&o| runs[o].ch == ch) else {
                    break;
                };
                let o = openers[at];
                let (open_len, close_len) =
                    (runs[o].right - runs[o].left, runs[k].right - runs[k].left);
                let n = if open_len >= 2 && close_len >= 2 {
                    2
                } else {
                    1
                };
                let open_mark = runs[o].right - n..runs[o].right;
                let close_mark = runs[k].left..runs[k].left + n;
                let flag = if n == 2 { BOLD } else { ITALIC };
                for f in &mut flags[open_mark.end - start..close_mark.start - start] {
                    *f |= flag;
                }
                let element = open_mark.start..close_mark.end;
                marks.push(Markup {
                    bytes: open_mark,
                    element: element.clone(),
                });
                marks.push(Markup {
                    bytes: close_mark,
                    element,
                });
                runs[o].right -= n;
                runs[k].left += n;
                openers.truncate(at + 1);
                if runs[o].right == runs[o].left {
                    openers.pop();
                }
            }
        }
        if runs[k].can_open && runs[k].right > runs[k].left {
            openers.push(k);
        }
    }
}

unsafe fn send(edit: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    SendMessageW(edit, msg, wparam, lparam)
}

fn nul(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

struct StyleSpec<'a> {
    face: &'a [u8],
    hundredths: u32,
    fore: u32,
    back: u32,
    is_bold: bool,
    is_italic: bool,
    is_underline: bool,
}

unsafe fn define(edit: HWND, id: u8, s: &StyleSpec) {
    let id = id as usize;
    send(edit, SCI_STYLESETFONT, id, s.face.as_ptr() as isize);
    send(edit, SCI_STYLESETSIZEFRACTIONAL, id, s.hundredths as isize);
    send(edit, SCI_STYLESETFORE, id, s.fore as isize);
    send(edit, SCI_STYLESETBACK, id, s.back as isize);
    send(edit, SCI_STYLESETBOLD, id, s.is_bold as isize);
    send(edit, SCI_STYLESETITALIC, id, s.is_italic as isize);
    send(edit, SCI_STYLESETUNDERLINE, id, s.is_underline as isize);
}

/// Defines the rendered styles. Runs after theme::style_editor, which sets the base look.
pub unsafe fn apply_styles(edit: HWND, look: &Look) {
    let (fg, bg) = (look.colors.fg, look.colors.bg);
    let body = nul(BODY_FACE);
    let code = nul(look.face);
    let size = look.size_tenths * HUNDREDTHS_PER_TENTH;
    let dim = themes::mix(bg, fg, MARK_MIX_PERCENT);
    let link = look.syntax.keyword.fg;
    let code_back = themes::mix(bg, fg, CODE_BACK_MIX_PERCENT);
    for block in BLOCKS {
        for inline in INLINES {
            let percent = match block {
                Block::H1 => HEADING_PERCENT[0],
                Block::H2 => HEADING_PERCENT[1],
                Block::H3 => HEADING_PERCENT[2],
                _ => 100,
            };
            let is_heading = matches!(block, Block::H1 | Block::H2 | Block::H3 | Block::TableHead);
            let fore = match (inline, block) {
                (Inline::Link, _) => link,
                (_, Block::Quote) => themes::mix(bg, fg, QUOTE_MIX_PERCENT),
                _ => fg,
            };
            let spec = StyleSpec {
                face: if inline == Inline::Code { &code } else { &body },
                hundredths: size * percent / 100,
                fore,
                back: if inline == Inline::Code {
                    code_back
                } else {
                    bg
                },
                is_bold: is_heading || matches!(inline, Inline::Bold | Inline::BoldItalic),
                is_italic: block == Block::Quote
                    || matches!(inline, Inline::Italic | Inline::BoldItalic),
                is_underline: inline == Inline::Link,
            };
            define(edit, text_style(block, inline), &spec);
        }
    }
    define(edit, 0, &plain(&body, size, fg, bg, false));
    define(edit, HIDDEN, &plain(&body, size, dim, bg, false));
    send(edit, SCI_STYLESETVISIBLE, HIDDEN as usize, 0);
    define(edit, MARK, &plain(&body, size, dim, bg, false));
    define(edit, CODE_BLOCK, &plain(&code, size, fg, code_back, false));
    send(edit, SCI_STYLESETEOLFILLED, CODE_BLOCK as usize, 1);
    define(edit, LIST_MARK, &plain(&body, size, link, bg, true));
    define(edit, RULE, &plain(&body, size, dim, bg, false));
}

fn plain(face: &[u8], hundredths: u32, fore: u32, back: u32, is_bold: bool) -> StyleSpec<'_> {
    StyleSpec {
        face,
        hundredths,
        fore,
        back,
        is_bold,
        is_italic: false,
        is_underline: false,
    }
}

/// Everything selected, from the first selection's start to the last one's end; just the
/// caret when nothing is selected.
unsafe fn selected(edit: HWND) -> Range<usize> {
    let count = send(edit, SCI_GETSELECTIONS, 0, 0).max(1) as usize;
    let (mut start, mut end) = (usize::MAX, 0);
    for n in 0..count {
        start = start.min(send(edit, SCI_GETSELECTIONNSTART, n, 0).max(0) as usize);
        end = end.max(send(edit, SCI_GETSELECTIONNEND, n, 0).max(0) as usize);
    }
    start.min(end)..end
}

/// Restyles the whole text for the current caret and selection.
pub unsafe fn restyle(edit: HWND) {
    let len = send(edit, SCI_GETLENGTH, 0, 0).max(0) as usize;
    let selected = selected(edit);
    let ptr = send(edit, SCI_GETCHARACTERPOINTER, 0, 0) as *const u8;
    if ptr.is_null() {
        return;
    }
    // The pointer stays valid until the text changes; styling does not change it.
    let text = std::slice::from_raw_parts(ptr, len);
    let styles = styles(text, selected);
    send(edit, SCI_STARTSTYLING, 0, 0);
    send(
        edit,
        SCI_SETSTYLINGEX,
        styles.len(),
        styles.as_ptr() as isize,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAR: usize = usize::MAX;

    /// The text as it reads with the caret at `caret`: hidden bytes left out.
    fn visible(text: &str, caret: usize) -> String {
        let s = styles(text.as_bytes(), caret..caret);
        let bytes: Vec<u8> = text
            .bytes()
            .zip(&s)
            .filter(|(_, &st)| st != HIDDEN)
            .map(|(b, _)| b)
            .collect();
        String::from_utf8(bytes).unwrap()
    }

    fn style_at(text: &str, needle: &str) -> u8 {
        let at = text.find(needle).unwrap();
        styles(text.as_bytes(), FAR..FAR)[at]
    }

    #[test]
    fn hides_inline_markup_away_from_the_caret() {
        let t = "Some **bold**, *italic*, `code` and a [link](https://x.org/a_b).";
        assert_eq!(visible(t, FAR), "Some bold, italic, code and a link.");
        assert_eq!(style_at(t, "bold"), text_style(Block::Body, Inline::Bold));
        assert_eq!(
            style_at(t, "italic"),
            text_style(Block::Body, Inline::Italic)
        );
        assert_eq!(style_at(t, "code"), text_style(Block::Body, Inline::Code));
        assert_eq!(style_at(t, "link"), text_style(Block::Body, Inline::Link));
    }

    #[test]
    fn shows_the_markup_of_the_element_under_the_caret() {
        let t = "a **b** c [d](e) f";
        let in_bold = t.find('b').unwrap();
        assert_eq!(visible(t, in_bold), "a **b** c d f");
        let in_link = t.find('d').unwrap();
        assert_eq!(visible(t, in_link), "a b c [d](e) f");
        // Right after the closing markup still counts, so typing there is visible.
        assert_eq!(visible(t, t.find(" c").unwrap()), "a **b** c d f");
    }

    #[test]
    fn headings_quotes_lists_and_rules() {
        let t = "# Title\n\n> quoted *word*\n- item\n12. numbered\n- [x] done\n---\n";
        assert_eq!(
            visible(t, FAR),
            "Title\n\n> quoted word\n- item\n12. numbered\n- [x] done\n---\n"
        );
        assert_eq!(style_at(t, "Title"), text_style(Block::H1, Inline::Plain));
        assert_eq!(
            style_at(t, "quoted"),
            text_style(Block::Quote, Inline::Plain)
        );
        assert_eq!(
            style_at(t, "word"),
            text_style(Block::Quote, Inline::Italic)
        );
        assert_eq!(style_at(t, "- item"), LIST_MARK);
        assert_eq!(style_at(t, "12."), LIST_MARK);
        assert_eq!(style_at(t, "[x]"), LIST_MARK);
        assert_eq!(style_at(t, "---"), RULE);
        assert_eq!(
            visible("# Title", 3),
            "# Title",
            "caret on the heading shows its #"
        );
    }

    #[test]
    fn fenced_code_is_left_alone() {
        let t = "```rust\nlet **x** = [a](b);\n```\nafter **b**\n";
        assert_eq!(
            visible(t, FAR),
            "```rust\nlet **x** = [a](b);\n```\nafter b\n"
        );
        assert_eq!(style_at(t, "let"), CODE_BLOCK);
        assert_eq!(style_at(t, "```rust"), MARK);
    }

    #[test]
    fn tables_keep_their_pipes() {
        let t = "| Name | **Score** |\n|---|---:|\n| a | 1 |\n";
        assert_eq!(visible(t, FAR), "| Name | Score |\n|---|---:|\n| a | 1 |\n");
        assert_eq!(
            style_at(t, "Name"),
            text_style(Block::TableHead, Inline::Plain)
        );
        assert_eq!(
            style_at(t, "Score"),
            text_style(Block::TableHead, Inline::Bold)
        );
        assert_eq!(style_at(t, "a |"), TEXT);
        assert_eq!(style_at(t, "|---"), MARK);
    }

    #[test]
    fn emphasis_follows_commonmark_flanking() {
        assert_eq!(
            visible("snake_case_name and 2 * 3 * 4", FAR),
            "snake_case_name and 2 * 3 * 4"
        );
        assert_eq!(
            visible("***both*** and **unclosed", FAR),
            "both and **unclosed"
        );
        assert_eq!(visible("*a **b** c*", FAR), "a b c");
        let t = "*a **b** c*";
        assert_eq!(
            style_at(t, "b"),
            text_style(Block::Body, Inline::BoldItalic)
        );
        assert_eq!(visible(r"not \*emphasis\*", FAR), "not *emphasis*");
    }

    #[test]
    fn urls_are_links_and_their_underscores_are_not_emphasis() {
        let t = "See https://a.org/x_y_z. Or <https://b.org/q> and **[bold link](u)**.";
        assert_eq!(
            visible(t, FAR),
            "See https://a.org/x_y_z. Or https://b.org/q and bold link."
        );
        assert_eq!(
            style_at(t, "https://a"),
            text_style(Block::Body, Inline::Link)
        );
        assert_eq!(
            style_at(t, "bold link"),
            text_style(Block::Body, Inline::Link)
        );
    }

    #[test]
    fn crlf_lines_and_utf8_are_styled_byte_for_byte() {
        let t = "# Héllo\r\n**wörld**\r\n";
        let s = styles(t.as_bytes(), FAR..FAR);
        assert_eq!(s.len(), t.len());
        assert_eq!(visible(t, FAR), "Héllo\r\nwörld\r\n");
    }

    mod round_trip {
        use super::*;
        use crate::editor::tests::{test_editor, SCINTILLA};
        use crate::{editor, file, languages, themes};
        use std::path::{Path, PathBuf};
        use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyWindow, WM_CHAR};

        const TYPED: &str = "XYZ";
        const SCI_DELETEBACK: u32 = 2326;
        const CARET_STEP: usize = 97;

        /// The committed sample plus, when present, the private copies in tests/fixtures/private
        /// (real drafts and papers, kept out of the public repo by .gitignore).
        fn fixtures() -> Vec<PathBuf> {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
            let mut found = Vec::new();
            for dir in [root.clone(), root.join("private")] {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for e in entries.flatten() {
                    if e.path().extension().is_some_and(|x| x == "md") {
                        found.push(e.path());
                    }
                }
            }
            found.sort();
            found
        }

        fn markdown() -> &'static languages::Language {
            languages::LANGUAGES
                .iter()
                .find(|l| l.name == "Markdown")
                .unwrap()
        }

        unsafe fn render(edit: HWND) {
            let colors = themes::PRESETS[0].colors;
            let look = Look {
                colors,
                syntax: themes::syntax_of(0, colors),
                face: "Consolas",
                size_tenths: 110,
            };
            editor::set_lexer(edit, None);
            apply_styles(edit, &look);
            restyle(edit);
        }

        unsafe fn utf8(edit: HWND) -> String {
            String::from_utf16(&editor::text(edit)).unwrap()
        }

        /// Places to type into: inside bold text, the end of a link's text, a heading, a table
        /// cell, the middle of the longest line, and the very end.
        fn edit_points(text: &str) -> Vec<usize> {
            let mut points: Vec<usize> = [("**", 3), ("](", 0), ("\n## ", 5), ("\n| ", 3)]
                .iter()
                .filter_map(|(needle, offset)| text.find(needle).map(|at| at + offset))
                .collect();
            let longest = text
                .split_inclusive('\n')
                .scan(0, |start, line| {
                    let at = *start;
                    *start += line.len();
                    Some((at, line))
                })
                .max_by_key(|(_, line)| line.len());
            if let Some((at, line)) = longest {
                points.push(at + line.len() / 2);
            }
            points.push(text.len());
            points.retain(|&p| p <= text.len() && text.is_char_boundary(p));
            points.sort_unstable();
            points.dedup();
            points
        }

        unsafe fn type_at(edit: HWND, pos: usize) {
            send(edit, SCI_GOTOPOS, pos, 0);
            restyle(edit);
            for ch in TYPED.chars() {
                send(edit, WM_CHAR, ch as usize, 0);
                restyle(edit);
            }
            // One Backspace, as a typo fix would.
            send(edit, SCI_DELETEBACK, 0, 0);
            restyle(edit);
        }

        /// Reads, renders, walks the caret through, types, and toggles a real file, then checks
        /// that the Markdown is exactly the original plus what was typed.
        #[test]
        fn real_files_keep_their_bytes() {
            let _turn = SCINTILLA.lock().unwrap_or_else(|e| e.into_inner());
            let paths = fixtures();
            assert!(!paths.is_empty());
            for path in &paths {
                let name = path.display();
                let bytes = std::fs::read(path).unwrap();
                let (units, format) = file::decode(&bytes).unwrap();
                let units = file::normalize_line_breaks(&units, format.is_lf_only);
                if !format.is_mixed_eol {
                    assert_eq!(
                        file::encode(&units, format).0,
                        bytes,
                        "{name}: save as read"
                    );
                }
                let original = String::from_utf16(&units).unwrap();
                unsafe {
                    let edit = test_editor(&original);
                    render(edit);
                    let far = styles(original.as_bytes(), usize::MAX..usize::MAX);
                    assert_eq!(far.len(), original.len(), "{name}");
                    for caret in (0..=original.len()).step_by(CARET_STEP) {
                        send(edit, SCI_GOTOPOS, caret, 0);
                        restyle(edit);
                    }
                    assert_eq!(utf8(edit), original, "{name}: moving the caret");
                    assert!(!editor::is_modified(edit), "{name}");

                    let points = edit_points(&original);
                    let kept = &TYPED[..TYPED.len() - 1];
                    let mut expected = original.clone();
                    for &p in points.iter().rev() {
                        type_at(edit, p);
                        expected.insert_str(p, kept);
                    }
                    editor::set_lexer(edit, Some(markdown()));
                    assert_eq!(utf8(edit), expected, "{name}: edits in the rendered view");
                    render(edit);
                    editor::set_lexer(edit, Some(markdown()));
                    assert_eq!(utf8(edit), expected, "{name}: toggling again");
                    while send(edit, SCI_CANUNDO, 0, 0) != 0 {
                        editor::undo(edit);
                    }
                    assert_eq!(utf8(edit), original, "{name}: undo");
                    DestroyWindow(edit);
                }
            }
            eprintln!("round trip checked on {} files", paths.len());
        }

        /// Scintilla refuses to delete or type over invisible text, so a selection must show
        /// the markup it covers before the user acts on it.
        #[test]
        fn selections_across_markup_can_be_deleted_and_typed_over() {
            const SCI_CLEAR: u32 = 2180;
            let _turn = SCINTILLA.lock().unwrap_or_else(|e| e.into_inner());
            let t = "one **two** three [four](five) six";
            unsafe {
                let edit = test_editor(t);
                render(edit);
                let (from, to) = (t.find("tw").unwrap(), t.find("ur]").unwrap());
                send(edit, SCI_SETSEL, from, to as isize);
                restyle(edit);
                send(edit, WM_CHAR, 'X' as usize, 0);
                restyle(edit);
                assert_eq!(utf8(edit), "one **Xur](five) six");
                send(edit, SCI_SELECTALL, 0, 0);
                restyle(edit);
                send(edit, SCI_CLEAR, 0, 0);
                assert_eq!(utf8(edit), "");
                DestroyWindow(edit);
            }
        }

        /// With the caret far away, no link address, heading marker, or bold marker is left
        /// showing outside code (the sample has some on purpose inside code).
        #[test]
        fn real_files_hide_their_markup() {
            for path in fixtures() {
                let text = std::fs::read_to_string(&path).unwrap();
                let s = styles(text.as_bytes(), usize::MAX..usize::MAX);
                let shown: String = text
                    .char_indices()
                    .filter(|&(i, _)| s[i] != HIDDEN && s[i] != CODE_BLOCK)
                    .filter(|&(i, _)| s[i] != text_style(Block::Body, Inline::Code))
                    .map(|(_, c)| c)
                    .collect();
                let name = path.display();
                assert!(!shown.contains("](http"), "{name}: a link address shows");
                assert!(!shown.contains("\n# "), "{name}: a heading marker shows");
                assert!(!shown.contains("**"), "{name}: a bold marker shows");
            }
        }
    }

    #[test]
    fn a_selection_shows_the_markup_of_everything_it_touches() {
        let t = "**a** b [c](d) e *f*";
        let s = styles(t.as_bytes(), 2..10);
        let shown: String = t
            .bytes()
            .zip(&s)
            .filter(|(_, &st)| st != HIDDEN)
            .map(|(b, _)| b as char)
            .collect();
        assert_eq!(shown, "**a** b [c](d) e f");
    }

    #[test]
    fn long_lines_of_unmatched_brackets_stay_fast() {
        let t = "[`".repeat(200_000);
        let started = std::time::Instant::now();
        assert_eq!(styles(t.as_bytes(), 0..0).len(), t.len());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let nested = format!("{}a{}", "[".repeat(10_000), "](b)".repeat(10_000));
        assert_eq!(styles(nested.as_bytes(), 0..0).len(), nested.len());
    }

    #[test]
    fn every_byte_gets_a_style_on_odd_input() {
        for t in [
            "",
            "*",
            "**",
            "[",
            "[a](",
            "`",
            "``x`",
            "> ",
            "#",
            "|",
            "|-|",
            "- ",
            "1.",
            "\r",
            "\n\r\n",
            "~~~\n",
            "<https://",
        ] {
            let s = styles(t.as_bytes(), 0..0);
            assert_eq!(s.len(), t.len(), "{t:?}");
        }
    }
}
