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

#[cfg(test)]
mod tests {
    use super::*;

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
