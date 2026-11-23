//! Line diff (Myers' algorithm) for applying a file reload as small edits, so the editor keeps
//! the caret, scroll position and undo history, and can mark the lines that changed.

/// Lines old[old_start..old_end] become new[new_start..new_end].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Hunk {
    pub old_start: usize,
    pub old_end: usize,
    pub new_start: usize,
    pub new_end: usize,
}

/// Past this many differing lines the exact diff costs too much; the changed middle is
/// replaced as one block instead. The saved search frontiers grow with its square (about
/// 16 MB at this cap).
const MAX_EDIT_DISTANCE: usize = 1024;

/// Splits after each '\n', keeping it, so joining the lines gives back the text.
pub fn lines(text: &[u8]) -> Vec<&[u8]> {
    text.split_inclusive(|&b| b == b'\n').collect()
}

/// The changed runs of lines, in order.
pub fn hunks(old: &[&[u8]], new: &[&[u8]]) -> Vec<Hunk> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (a, b) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    if a.is_empty() && b.is_empty() {
        return Vec::new();
    }
    let middle = myers(a, b).unwrap_or_else(|| {
        vec![Hunk {
            old_start: 0,
            old_end: a.len(),
            new_start: 0,
            new_end: b.len(),
        }]
    });
    middle
        .into_iter()
        .map(|h| Hunk {
            old_start: h.old_start + prefix,
            old_end: h.old_end + prefix,
            new_start: h.new_start + prefix,
            new_end: h.new_end + prefix,
        })
        .collect()
}

/// Myers' O((N+M)D) diff. None when the edit distance passes MAX_EDIT_DISTANCE.
fn myers(a: &[&[u8]], b: &[&[u8]]) -> Option<Vec<Hunk>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = (n + m).min(MAX_EDIT_DISTANCE as isize);
    let offset = max + 1;
    let mut v = vec![0isize; 2 * offset as usize + 1];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for d in 0..=max {
        trace.push(v.clone());
        for k in (-d..=d).step_by(2) {
            let i = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[i - 1] < v[i + 1]) {
                v[i + 1]
            } else {
                v[i - 1] + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[i] = x;
            if x >= n && y >= m {
                return Some(backtrack(&trace, a.len(), b.len(), offset));
            }
        }
    }
    None
}

/// Walks the saved frontiers back from the end and groups the steps into hunks.
fn backtrack(trace: &[Vec<isize>], n: usize, m: usize, offset: isize) -> Vec<Hunk> {
    let (mut x, mut y) = (n as isize, m as isize);
    // Steps from the end, as (x, y) before each insert or delete.
    let mut steps: Vec<(isize, isize, bool)> = Vec::new();
    for (d, v) in trace.iter().enumerate().rev() {
        let d = d as isize;
        if d == 0 {
            break;
        }
        let k = x - y;
        let at = |k: isize| v[(k + offset) as usize];
        let is_insert = k == -d || (k != d && at(k - 1) < at(k + 1));
        let prev_k = if is_insert { k + 1 } else { k - 1 };
        let prev_x = at(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
        }
        steps.push((prev_x, prev_y, is_insert));
        x = prev_x;
        y = prev_y;
    }
    steps.reverse();
    let mut hunks: Vec<Hunk> = Vec::new();
    for (x, y, is_insert) in steps {
        let (x, y) = (x as usize, y as usize);
        let (old_end, new_end) = if is_insert { (x, y + 1) } else { (x + 1, y) };
        match hunks.last_mut() {
            Some(h) if h.old_end == x && h.new_end == y => {
                h.old_end = old_end;
                h.new_end = new_end;
            }
            _ => hunks.push(Hunk {
                old_start: x,
                old_end,
                new_start: y,
                new_end,
            }),
        }
    }
    hunks
}

/// A unified diff (as `diff -u` prints it) from `old` to `new`, with `context` unchanged lines
/// around each change. Empty when the texts have the same lines.
pub fn unified(old: &str, new: &str, old_name: &str, new_name: &str, context: usize) -> String {
    let (a, b) = (lines(old.as_bytes()), lines(new.as_bytes()));
    let all = hunks(&a, &b);
    if all.is_empty() {
        return String::new();
    }
    let mut out = format!("--- {old_name}\n+++ {new_name}\n");
    let mut i = 0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len() && all[j + 1].old_start - all[j].old_end <= 2 * context {
            j += 1;
        }
        let (first, last) = (all[i], all[j]);
        let old_from = first.old_start.saturating_sub(context);
        let old_to = (last.old_end + context).min(a.len());
        let new_from = first.new_start - (first.old_start - old_from);
        let new_to = last.new_end + (old_to - last.old_end);
        out.push_str(&format!(
            "@@ -{} +{} @@\n",
            range(old_from, old_to),
            range(new_from, new_to)
        ));
        let mut at = old_from;
        for h in &all[i..=j] {
            push_lines(&mut out, ' ', &a[at..h.old_start]);
            push_lines(&mut out, '-', &a[h.old_start..h.old_end]);
            push_lines(&mut out, '+', &b[h.new_start..h.new_end]);
            at = h.old_end;
        }
        push_lines(&mut out, ' ', &a[at..old_to]);
        i = j + 1;
    }
    out
}

/// A hunk header's "start,count", 1-based (start is the line before when count is 0).
fn range(from: usize, to: usize) -> String {
    let count = to - from;
    let start = if count == 0 { from } else { from + 1 };
    format!("{start},{count}")
}

fn push_lines(out: &mut String, mark: char, lines: &[&[u8]]) {
    for line in lines {
        out.push(mark);
        out.push_str(&String::from_utf8_lossy(line));
        if !line.ends_with(b"\n") {
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_diff_matches_diff_u() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
        let new = "1\n2\nthree\n4\n5\n6\n7\n8\n9\n10\neleven\n";
        assert_eq!(
            unified(old, new, "mine", "disk", 2),
            "--- mine\n+++ disk\n@@ -1,5 +1,5 @@\n 1\n 2\n-3\n+three\n 4\n 5\n\
             @@ -9,2 +9,3 @@\n 9\n 10\n+eleven\n"
        );
        // Changes closer than two contexts share one hunk.
        let near = unified("a\nb\nc\nd\n", "A\nb\nc\nD\n", "x", "y", 1);
        assert_eq!(near.matches("@@ -").count(), 1);
        assert_eq!(unified("same\n", "same\n", "x", "y", 3), "");
        assert!(unified("", "new", "x", "y", 3).contains("@@ -0,0 +1,1 @@\n+new\n"));
    }

    /// Applies the hunks to `old` and checks the result is `new`.
    fn round_trip(old: &str, new: &str) -> Vec<Hunk> {
        let (a, b) = (lines(old.as_bytes()), lines(new.as_bytes()));
        let hunks = hunks(&a, &b);
        let mut out: Vec<&[u8]> = a.clone();
        for h in hunks.iter().rev() {
            out.splice(
                h.old_start..h.old_end,
                b[h.new_start..h.new_end].iter().copied(),
            );
        }
        assert_eq!(out.concat(), new.as_bytes(), "{old:?} -> {new:?}");
        hunks
    }

    #[test]
    fn scattered_edits_become_separate_hunks() {
        let hunks = round_trip("a\nb\nc\nd\ne\nf\n", "a\nB\nc\nd\nE\nf\nnew\n");
        assert_eq!(hunks.len(), 3);
        assert_eq!(
            hunks[0],
            Hunk {
                old_start: 1,
                old_end: 2,
                new_start: 1,
                new_end: 2
            }
        );
    }

    #[test]
    fn inserts_deletes_and_edge_cases() {
        assert!(round_trip("same\n", "same\n").is_empty());
        round_trip("", "x\ny\n");
        round_trip("x\ny\n", "");
        round_trip("a\nb\nc", "a\nc");
        round_trip("1\n2\n3\n4\n", "0\n1\n3\n4\n5");
        round_trip("no newline", "no newline\n");
        round_trip("x\nx\nx\n", "x\ny\nx\nx\n");
    }

    #[test]
    fn many_changes_fall_back_to_one_block() {
        let old: String = (0..10_000).map(|i| format!("{i}\n")).collect();
        let new: String = (0..10_000).map(|i| format!("{}\n", i * 7)).collect();
        assert!(!round_trip(&old, &new).is_empty());
    }
}
