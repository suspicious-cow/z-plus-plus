//! Find in Files: walks a folder on a background thread and finds a text or regular expression
//! in every file whose name matches the filter. Uses the regex crate (Rust syntax, linear time).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use regex::{Regex, RegexBuilder};

use crate::file;

/// Folders nobody means to search: version control, dependencies, build output.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ipynb_checkpoints",
];
/// Bigger files are skipped; they are almost always data or build output, not text to edit.
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// Hit lines are cut to this many characters in the results.
const MAX_LINE_CHARS: usize = 300;
/// A search stops collecting after this many hits, so a too-common word stays usable.
pub const MAX_HITS: usize = 20_000;

#[derive(Clone, Default, Debug)]
pub struct Query {
    pub needle: String,
    /// File name patterns separated by ';' or spaces, such as "*.rs;*.toml". Empty means all.
    pub filters: String,
    pub folder: PathBuf,
    pub is_match_case: bool,
    pub is_whole_word: bool,
    pub is_regex: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Counted from 1.
    pub line: usize,
    /// Character column, counted from 1.
    pub col: usize,
    pub text: String,
    /// Byte range of the match within `text` (for highlighting); empty when cut off.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug)]
pub struct FileHits {
    pub path: PathBuf,
    pub hits: Vec<Hit>,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub files: Vec<FileHits>,
    pub files_searched: usize,
    pub is_capped: bool,
    pub is_canceled: bool,
}

pub fn build_regex(q: &Query) -> Result<Regex, String> {
    let pattern = if q.is_regex {
        q.needle.clone()
    } else {
        regex::escape(&q.needle)
    };
    let pattern = if q.is_whole_word {
        format!(r"\b(?:{pattern})\b")
    } else {
        pattern
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!q.is_match_case)
        .multi_line(true)
        .build()
        .map_err(|e| e.to_string())
}

/// True if `name` matches one of the patterns (`*` and `?`, case-insensitive).
pub fn matches_filter(filters: &str, name: &str) -> bool {
    let mut patterns = filters
        .split([';', ' ', ','])
        .filter(|p| !p.is_empty())
        .peekable();
    if patterns.peek().is_none() {
        return true;
    }
    let name = name.to_lowercase();
    patterns.any(|p| glob(&p.to_lowercase(), &name))
}

fn glob(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

/// Searches every matching file under `q.folder`. Stops early once `generation` no longer
/// equals `mine` (a newer search started).
pub fn run(q: &Query, re: &Regex, generation: &AtomicU64, mine: u64) -> Outcome {
    let mut outcome = Outcome::default();
    let mut total = 0usize;
    let mut pending = vec![q.folder.clone()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if generation.load(Ordering::Relaxed) != mine {
                outcome.is_canceled = true;
                return outcome;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                if !SKIPPED_DIRS.contains(&name.as_str()) {
                    pending.push(entry.path());
                }
                continue;
            }
            if !kind.is_file() || !matches_filter(&q.filters, &name) {
                continue;
            }
            outcome.files_searched += 1;
            let hits = search_file(&entry.path(), re, MAX_HITS - total);
            if hits.is_empty() {
                continue;
            }
            total += hits.len();
            outcome.files.push(FileHits {
                path: entry.path(),
                hits,
            });
            if total >= MAX_HITS {
                outcome.is_capped = true;
                return outcome;
            }
        }
    }
    outcome
}

/// The hits in one file. Binary and very large files give none.
fn search_file(path: &Path, re: &Regex, limit: usize) -> Vec<Hit> {
    let is_small = fs::metadata(path).is_ok_and(|m| m.len() <= MAX_FILE_BYTES);
    let Some(bytes) = is_small.then(|| fs::read(path).ok()).flatten() else {
        return Vec::new();
    };
    let Ok((units, _)) = file::decode(&bytes) else {
        return Vec::new();
    };
    search_text(&String::from_utf16_lossy(&units), re, limit)
}

pub fn search_text(text: &str, re: &Regex, limit: usize) -> Vec<Hit> {
    let mut hits = Vec::new();
    for (i, line) in text.lines().enumerate() {
        for m in re.find_iter(line) {
            if hits.len() >= limit {
                return hits;
            }
            hits.push(hit(i + 1, line, m.start(), m.end()));
        }
    }
    hits
}

fn hit(line_no: usize, line: &str, start: usize, end: usize) -> Hit {
    let col = line[..start].chars().count() + 1;
    let trimmed = line.trim_start();
    let cut = line.len() - trimmed.len();
    let text: String = trimmed.chars().take(MAX_LINE_CHARS).collect();
    let (start, end) = (start.saturating_sub(cut), end.saturating_sub(cut));
    let (start, end) = if end <= text.len() && start <= end {
        (start, end)
    } else {
        (0, 0)
    };
    Hit {
        line: line_no,
        col,
        text,
        start,
        end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(needle: &str) -> Query {
        Query {
            needle: needle.to_string(),
            ..Query::default()
        }
    }

    #[test]
    fn filters_use_star_and_question_mark() {
        assert!(matches_filter("", "anything.bin"));
        assert!(matches_filter("*.rs;*.toml", "Cargo.TOML"));
        assert!(matches_filter("*.rs *.md", "README.md"));
        assert!(!matches_filter("*.rs", "main.rsx"));
        assert!(matches_filter("main.?s", "main.rs"));
        assert!(matches_filter("*test*", "my_tests.py"));
    }

    #[test]
    fn plain_text_is_not_a_pattern_and_ignores_case() {
        let re = build_regex(&query("a.b(")).unwrap();
        let hits = search_text("xx A.B( yy\naxb(\n", &re, 10);
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].line, hits[0].col), (1, 4));
    }

    #[test]
    fn whole_word_regex_and_columns_in_characters() {
        let q = Query {
            is_regex: true,
            is_whole_word: true,
            is_match_case: true,
            ..query(r"fo+")
        };
        let re = build_regex(&q).unwrap();
        let hits = search_text("é foo foobar Foo\n    fooo", &re, 10);
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].line, hits[0].col), (1, 3));
        assert_eq!((hits[1].line, hits[1].col), (2, 5));
        assert_eq!(&hits[1].text[hits[1].start..hits[1].end], "fooo");
        assert!(build_regex(&Query {
            is_regex: true,
            ..query("(")
        })
        .is_err());
    }

    #[test]
    fn walks_folders_skips_build_output_and_honors_the_filter() {
        let root = std::env::temp_dir().join(format!("zpp-search-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src").join("a.rs"), "fn needle() {}\n").unwrap();
        fs::write(root.join("notes.txt"), "needle\nneedle\n").unwrap();
        fs::write(root.join("target").join("b.rs"), "needle\n").unwrap();
        fs::write(root.join("blob.rs"), b"needle\0binary").unwrap();
        let q = Query {
            folder: root.clone(),
            filters: "*.rs".into(),
            ..query("needle")
        };
        let re = build_regex(&q).unwrap();
        let generation = AtomicU64::new(1);
        let outcome = run(&q, &re, &generation, 1);
        let names: Vec<_> = outcome.files.iter().map(|f| f.path.clone()).collect();
        assert_eq!(names, vec![root.join("src").join("a.rs")]);
        assert_eq!(outcome.files[0].hits[0].col, 4);
        let _ = fs::remove_dir_all(&root);
    }
}
