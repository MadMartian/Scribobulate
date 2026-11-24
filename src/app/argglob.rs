//! Wildcards in the files named on the command line, expanded here when the shell did
//! not (TDD 8.18).
//!
//! Two shells hand the app a pattern instead of files. Bash, by default, passes a
//! pattern that matches nothing through unchanged: `scribobulate sdd/*.md` in a folder
//! whose `sdd/` holds no Markdown arrives as the literal `sdd/*.md`, and opening it gave
//! a window with one empty tab titled `*.md`. Windows' shells never expand at all, so
//! there `scribobulate *.md` arrived as `*.md` whatever the folder held.
//!
//! So a name that does not exist and carries a wildcard is expanded here, in the process
//! the user launched, against the folder it was launched from — before a second launch
//! forwards its file list to the running instance, which has its own working folder.
//! Where the shell already expanded (or the name exists, wildcard characters and all),
//! nothing changes. A pattern that matches nothing stays in the list, so whichever
//! process ends up opening the files — this one, or a running instance it forwards to —
//! sees it, recognises it ([`is_unmatched_pattern`]), and reports it in the window
//! instead of opening it as a new document.
//!
//! The rules are bash's defaults, which is what a reader on any platform will expect:
//! `*`, `?` and `[...]` (with `!` or `^` to negate, and ranges) within one path
//! component; no `**`; a leading `.` is matched only by a pattern that starts with one;
//! results sorted. Windows compares case-insensitively, as its file system does.

use std::path::{Component, Path, PathBuf};

/// Does `arg` carry a wildcard this module expands?
pub(crate) fn has_wildcard(arg: &str) -> bool {
    arg.contains(['*', '?', '['])
}

/// Split argv into its expanded form and the patterns that matched nothing. An
/// unmatched pattern stays in the expanded form too, in its place.
///
/// `args[0]` (the program) and anything option-shaped are left as they are; the `--`
/// rule has already rewritten a dash-led file name to `./<name>` by the time this runs
/// (`super::end_of_options_argv`). Idempotent: every name it produces exists, so a
/// second pass changes nothing.
pub(crate) fn expand_operands(args: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut out = Vec::with_capacity(args.len());
    let mut unmatched = Vec::new();
    for (index, arg) in args.into_iter().enumerate() {
        let operand = index > 0 && !arg.starts_with('-');
        if !operand || !has_wildcard(&arg) || Path::new(&arg).exists() {
            out.push(arg);
            continue;
        }
        let found = expand(&arg, cfg!(windows));
        if found.is_empty() {
            unmatched.push(arg.clone());
            out.push(arg);
        } else {
            out.extend(found);
        }
    }
    (out, unmatched)
}

/// Is `path` a file pattern that matched nothing, rather than a file to open? It names
/// nothing on disk and a component of it carries a wildcard. A missing name without one
/// is a new document, as it always was.
pub(crate) fn is_unmatched_pattern(path: &Path) -> bool {
    !path.exists()
        && path
            .components()
            .any(|c| matches!(c, Component::Normal(n) if has_wildcard(&n.to_string_lossy())))
}

/// Every existing path `pattern` names, sorted, spelled from the same starting point the
/// pattern was (relative stays relative).
fn expand(pattern: &str, fold_case: bool) -> Vec<String> {
    let mut bases = vec![PathBuf::new()];
    for component in Path::new(pattern).components() {
        let next_bases = match component {
            Component::Normal(part) => {
                let part = part.to_string_lossy();
                if !has_wildcard(&part) {
                    bases.iter().map(|b| b.join(part.as_ref())).collect()
                } else {
                    let mut matched = Vec::new();
                    for base in &bases {
                        let dir = if base.as_os_str().is_empty() {
                            Path::new(".")
                        } else {
                            base.as_path()
                        };
                        let Ok(entries) = std::fs::read_dir(dir) else {
                            continue;
                        };
                        let mut names: Vec<String> = entries
                            .filter_map(Result::ok)
                            .filter_map(|e| e.file_name().into_string().ok())
                            .filter(|name| component_matches(&part, name, fold_case))
                            .collect();
                        sort_names(&mut names, fold_case);
                        matched.extend(names.into_iter().map(|n| base.join(n)));
                    }
                    matched
                }
            }
            other => bases.iter().map(|b| b.join(other.as_os_str())).collect(),
        };
        bases = next_bases;
        if bases.is_empty() {
            break;
        }
    }
    bases
        .into_iter()
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Sort matched names the way the platform lists them: by byte where case matters,
/// and case-folded (ties broken by byte) on Windows, as Explorer and `dir` do.
fn sort_names(names: &mut [String], fold_case: bool) {
    if fold_case {
        names.sort_by_cached_key(|n| (n.to_lowercase(), n.clone()));
    } else {
        names.sort();
    }
}

/// Does one path component `name` match the pattern component `pattern`?
fn component_matches(pattern: &str, name: &str, fold_case: bool) -> bool {
    if name.starts_with('.') && !pattern.starts_with('.') {
        return false;
    }
    let fold = |s: &str| -> Vec<char> {
        if fold_case {
            s.to_lowercase().chars().collect()
        } else {
            s.chars().collect()
        }
    };
    wildcard_match(&fold(pattern), &fold(name))
}

/// `*`, `?` and `[...]` over chars, with backtracking for `*`.
fn wildcard_match(p: &[char], n: &[char]) -> bool {
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        let step = match p.get(pi) {
            Some('*') => {
                star = Some((pi, ni));
                pi += 1;
                continue;
            }
            Some('?') => Some(pi + 1),
            Some('[') => match class_match(&p[pi..], n[ni]) {
                Some((true, len)) => Some(pi + len),
                Some((false, _)) => None,
                // An unterminated `[` is an ordinary character, as in bash.
                None => (n[ni] == '[').then_some(pi + 1),
            },
            Some(c) => (*c == n[ni]).then_some(pi + 1),
            None => None,
        };
        match step {
            Some(next) => {
                pi = next;
                ni += 1;
            }
            None => match star {
                Some((sp, sn)) => {
                    pi = sp + 1;
                    ni = sn + 1;
                    star = Some((sp, sn + 1));
                }
                None => return false,
            },
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Match `c` against the class at the start of `p` (which begins with `[`): whether it
/// matched and how many pattern chars the class spans, or `None` if it never closes.
fn class_match(p: &[char], c: char) -> Option<(bool, usize)> {
    let mut i = 1;
    let negate = matches!(p.get(i), Some('!') | Some('^'));
    if negate {
        i += 1;
    }
    let mut hit = false;
    let mut first = true;
    while let Some(&ch) = p.get(i) {
        if ch == ']' && !first {
            return Some((hit != negate, i + 1));
        }
        first = false;
        if p.get(i + 1) == Some(&'-') && p.get(i + 2).is_some_and(|e| *e != ']') {
            let end = p[i + 2];
            hit |= ch <= c && c <= end;
            i += 3;
        } else {
            hit |= ch == c;
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, n: &str) -> bool {
        component_matches(p, n, false)
    }

    #[test]
    fn star_question_and_classes_match_as_bash_does() {
        assert!(m("*.md", "notes.md"));
        assert!(!m("*.md", "notes.mdx"));
        assert!(m("n?tes.md", "notes.md"));
        assert!(m("[nm]otes.md", "notes.md"));
        assert!(!m("[!n]otes.md", "notes.md"));
        assert!(m("[^m]otes.md", "notes.md"));
        assert!(m("[a-z]*", "x1"));
        assert!(!m("[a-c]*", "x1"));
        assert!(m("*a*b*", "xxaxxbxx"));
        assert!(!m("*a*b", "xxaxxbxx"));
        assert!(m("[]]", "]"), "a `]` first in a class is a member");
        assert!(m("[", "["), "an unterminated `[` is literal");
    }

    #[test]
    fn a_leading_dot_is_matched_only_by_a_pattern_that_starts_with_one() {
        assert!(!m("*", ".hidden"));
        assert!(!m("*.md", ".notes.md"));
        assert!(m(".*", ".hidden"));
    }

    #[test]
    fn names_sort_by_byte_where_case_matters_and_folded_where_it_does_not() {
        let mut names = vec!["c.md".to_string(), "B.md".into(), "a.md".into()];
        sort_names(&mut names, false);
        assert_eq!(names, ["B.md", "a.md", "c.md"]);
        sort_names(&mut names, true);
        assert_eq!(names, ["a.md", "B.md", "c.md"], "Windows lists a, B, c");
    }

    #[test]
    fn case_folds_only_where_asked() {
        assert!(!component_matches("*.MD", "notes.md", false));
        assert!(component_matches("*.MD", "notes.md", true));
    }

    /// The reported case: a pattern the shell left alone because nothing matched is
    /// kept and recognised, so it is reported instead of becoming an empty tab `*.md`.
    #[test]
    fn a_pattern_that_matches_nothing_is_kept_and_recognised() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sdd")).unwrap();
        let pattern = dir.path().join("sdd").join("*.md");
        let pattern = pattern.to_string_lossy().into_owned();
        let (args, unmatched) = expand_operands(vec!["scribobulate".into(), pattern.clone()]);
        assert_eq!(args, vec!["scribobulate".to_string(), pattern.clone()]);
        assert_eq!(unmatched, vec![pattern.clone()]);
        assert!(is_unmatched_pattern(Path::new(&pattern)));
        assert!(
            !is_unmatched_pattern(&dir.path().join("sdd")),
            "an existing name"
        );
        assert!(
            !is_unmatched_pattern(&dir.path().join("new.md")),
            "a new document"
        );
    }

    /// Windows' case: the shell never expands, so the app does — sorted, hidden files
    /// left out, files of other types left out, across a wildcard directory too.
    #[test]
    fn a_pattern_the_shell_did_not_expand_opens_what_it_matches() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        for sub in ["a", "b"] {
            std::fs::create_dir(base.join(sub)).unwrap();
        }
        for f in [
            "a/two.md",
            "a/one.md",
            "a/.hidden.md",
            "a/x.txt",
            "b/three.md",
        ] {
            std::fs::write(base.join(f), "").unwrap();
        }
        let pattern = base.join("*").join("*.md").to_string_lossy().into_owned();
        let (args, unmatched) = expand_operands(vec!["scribobulate".into(), pattern]);
        let expect: Vec<String> = [("a", "one.md"), ("a", "two.md"), ("b", "three.md")]
            .iter()
            .map(|(d, f)| base.join(d).join(f).to_string_lossy().into_owned())
            .collect();
        assert_eq!(&args[1..], expect.as_slice());
        assert!(unmatched.is_empty());
    }

    #[test]
    fn existing_names_options_and_the_program_are_left_alone_and_a_second_pass_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let odd = dir.path().join("why[1].md");
        std::fs::write(&odd, "").unwrap();
        let odd = odd.to_string_lossy().into_owned();
        let plain = dir.path().join("missing.md").to_string_lossy().into_owned();
        let args: Vec<String> = vec!["scrib*".into(), "--help".into(), odd, plain];
        let (once, unmatched) = expand_operands(args.clone());
        assert_eq!(
            once, args,
            "an existing name with wildcard characters is a file"
        );
        assert!(
            unmatched.is_empty(),
            "a missing name WITHOUT a wildcard is a new file"
        );
        let (twice, _) = expand_operands(once.clone());
        assert_eq!(twice, once);
    }
}
