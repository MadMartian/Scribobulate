//! The preview's matcher — **one** implementation of "where does this query occur in
//! this text", for every text the preview searches.
//!
//! The editor does not use this: `GtkSourceSearchContext` implements the same three
//! options natively against the source buffer, and re-implementing them there would be
//! a second search engine over one buffer. The preview has no such engine, because its
//! matches are not all in a buffer — body text is, table-cell matches live in `GtkLabel`
//! children, and matches inside a collapsed disclosure live only in the document source.
//! Those three used to carry three separate hard-wired matchers, each case-insensitive
//! literal; adding options to three matchers is how the counts in one pane come to
//! disagree with each other, let alone with the other pane.
//!
//! # Why GRegex and not the `regex` crate
//!
//! The editor's regular-expression mode is `GtkSourceSearchSettings:regex-enabled`,
//! which compiles the pattern with GRegex (PCRE). If this file used the `regex` crate
//! the *same pattern typed into the same box* would mean two different things depending
//! on which pane happened to be visible — lookbehind and backreferences accepted in one
//! and rejected in the other, with nothing on screen to explain it. `glib::Regex` binds
//! the same engine, and glib is already a direct dependency, so parity costs nothing.
//!
//! # Byte ranges, always
//!
//! Every function here indexes the `&str` it was handed, in bytes. That is the index
//! space a Pango attribute wants for a cell label, and the caller that needs buffer
//! *character* offsets for body text converts once, at the boundary, where it holds the
//! map. Returning two coordinate systems from one function is how this file's ancestor
//! came to have a byte offset and a char offset in adjacent variables.

use super::options::FindOptions;

/// How a regular expression is bounded to whole words. **The non-capturing group is the
/// whole point**: `\b` around a bare alternation binds each anchor to one branch, so
/// `note|book` becomes `\bnote|book\b` — "a word starting `note`, or a word ending
/// `book`" — and `notebook` matches both halves of that while being neither word.
///
/// **GtkSourceView does NOT group it**, MEASURED on 5.20.0 by the macOS seat: its own
/// regex whole-word wrapper is the ungrouped `\b%s\b`, so the editor counted `note|book`
/// over `note notebook book` as 4 where the preview counted 2. This is why
/// [`editor_pattern`] exists — the application wraps the query ITSELF and turns
/// `at-word-boundaries` off for the regex case, so both panes run the same pattern
/// rather than each wrapping its own way.
///
/// Recorded at length because the first parity fixture for this could not see it: `cat`
/// sat at a word END and `dog` at a word START, and the grouped and ungrouped forms
/// agree on that text. The distinguishing fixture needs ONE word holding the first
/// branch at its start and the second at its end.
const WORD_WRAPPED: &str = r"\b(?:{})\b";

/// The pattern to hand the EDITOR's engine for `query` under `opts`, and the pattern
/// this module compiles for the preview — one spelling, so the two panes cannot wrap a
/// query differently.
///
/// Only the regular-expression case differs from the query itself. A LITERAL whole-word
/// search is left to `GtkSourceSearchSettings:at-word-boundaries`, whose word predicate
/// was measured to agree with this module's (`super::parity`); pre-wrapping a literal
/// would turn it into a pattern, which is a different search entirely.
pub(crate) fn editor_pattern(query: &str, opts: FindOptions) -> String {
    if opts.regex && opts.whole_word && !query.is_empty() {
        WORD_WRAPPED.replace("{}", query)
    } else {
        query.to_string()
    }
}

/// Whether the editor's engine should be left to apply its own whole-word bounding.
///
/// False for a regular expression, because its wrapper is ungrouped and would re-wrap
/// what [`editor_pattern`] has already wrapped correctly.
pub(crate) fn engine_applies_word_boundaries(opts: FindOptions) -> bool {
    opts.whole_word && !opts.regex
}

/// Put `opts` on the editor engine's settings: the three option properties, exactly as
/// the find bar hands them over. The one definition of that mapping, so the parity
/// fixtures (`super::parity`) measure the engine the application drives rather than a
/// copy of it.
pub(crate) fn configure_engine_options(settings: &sourceview::SearchSettings, opts: FindOptions) {
    use sourceview::prelude::SearchSettingsExt;
    let FindOptions {
        case_sensitive,
        whole_word: _,
        regex,
    } = opts;
    settings.set_case_sensitive(case_sensitive);
    // NOT the reader's `whole_word` outright: for a regular expression the wrapping is
    // the application's, because the engine's own is ungrouped (`WORD_WRAPPED`), and
    // letting it wrap again would nest one correct bounding inside one incorrect one.
    settings.set_at_word_boundaries(engine_applies_word_boundaries(opts));
    settings.set_regex_enabled(regex);
}

/// Put `query` on the editor engine's settings as the pattern [`editor_pattern`] makes
/// of it. An empty query clears the search text rather than setting `""`.
pub(crate) fn configure_engine_query(
    settings: &sourceview::SearchSettings,
    query: &str,
    opts: FindOptions,
) {
    use sourceview::prelude::SearchSettingsExt;
    let pattern = editor_pattern(query, opts);
    settings.set_search_text((!pattern.is_empty()).then_some(pattern.as_str()));
}

/// A compiled query, ready to be run against any number of texts.
pub(crate) enum Matcher {
    /// An empty query. Matches nothing, in every mode — including regex, where an
    /// empty pattern would otherwise match at every position and report a count equal
    /// to the document's length. "No query" is not "a query that matches everywhere".
    Never,
    /// A literal substring, with the two literal options folded in.
    Literal {
        /// The query's characters, already case-folded when the search is
        /// case-insensitive, so the fold is not redone per candidate position.
        needle: Vec<char>,
        case_sensitive: bool,
        whole_word: bool,
    },
    /// A compiled regular expression. Whole-word and case-insensitivity are compiled
    /// *into* it rather than checked afterwards, because a regular expression can match
    /// across the boundaries a post-hoc check would apply.
    Regex(glib::Regex),
}

/// Why a query could not be compiled. Only regular expressions can fail; a literal is
/// always a valid literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatternError {
    /// GRegex's own message, shown to the reader. It names the offending construct and
    /// its position, which is more use than anything this module could phrase.
    pub message: String,
}

/// The message, and nothing around it. The find bar puts this in a tooltip beside a
/// fixed "Invalid pattern" readout, so any wrapper this added would be a second,
/// unwanted sentence in a place with room for one.
impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Matcher {
    /// Compile `query` under `opts`.
    ///
    /// An empty query yields [`Matcher::Never`] rather than an error: it is the state
    /// the bar is in before the reader has typed anything, and it is not a mistake.
    pub(crate) fn compile(query: &str, opts: FindOptions) -> Result<Self, PatternError> {
        if query.is_empty() {
            return Ok(Matcher::Never);
        }
        if !opts.regex {
            let fold_case = !opts.case_sensitive;
            return Ok(Matcher::Literal {
                needle: query.chars().map(|c| fold(c, fold_case)).collect(),
                case_sensitive: opts.case_sensitive,
                whole_word: opts.whole_word,
            });
        }
        let pattern = editor_pattern(query, opts);
        let mut flags = glib::RegexCompileFlags::OPTIMIZE | glib::RegexCompileFlags::MULTILINE;
        if !opts.case_sensitive {
            flags |= glib::RegexCompileFlags::CASELESS;
        }
        match glib::Regex::new(&pattern, flags, glib::RegexMatchFlags::empty()) {
            // `Ok(None)` is g_regex_new returning NULL without setting an error. It is
            // not documented to happen, which is exactly why it is not `unreachable!()`:
            // a panic here is a process abort inside a GTK signal handler, and the
            // honest fallback — "this pattern does not compile" — is already a state the
            // reader is shown.
            Ok(None) => Err(PatternError {
                message: "the pattern could not be compiled".to_string(),
            }),
            Ok(Some(re)) => Ok(Matcher::Regex(re)),
            Err(e) => Err(PatternError {
                message: e.message().to_string(),
            }),
        }
    }

    /// Every non-overlapping match of this query in `hay`, as ascending byte ranges.
    ///
    /// Non-overlapping and left-to-right, which is what a find bar means by "the next
    /// match" — `aa` in `aaa` is one match, not two.
    pub(crate) fn ranges(&self, hay: &str) -> Vec<Range> {
        match self {
            Matcher::Never => Vec::new(),
            Matcher::Literal {
                needle,
                case_sensitive,
                whole_word,
            } => literal_ranges(hay, needle, *case_sensitive, *whole_word),
            Matcher::Regex(re) => regex_ranges(re, hay),
        }
    }

    /// How many times this query occurs in `hay`. Separate from [`ranges`](Self::ranges)
    /// only at the call site that needs a count and nothing else (a collapsed
    /// disclosure's body, which has no positions to highlight); it is the same walk.
    pub(crate) fn count(&self, hay: &str) -> usize {
        self.ranges(hay).len()
    }
}

/// One match, in bytes into the text it was found in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Range {
    pub start: usize,
    pub end: usize,
}

/// Simple per-character lowercase folding — 1:1 and byte-exact, so a folded match's
/// length is the query's length. Exotic multi-character foldings (`ß`→`ss`) are not
/// matched, which is the behaviour this file has always had and is adequate for a
/// Markdown find; the alternative is a length-changing fold that no longer maps back
/// onto the haystack's own byte offsets.
fn fold(c: char, folding: bool) -> char {
    if folding {
        c.to_lowercase().next().unwrap_or(c)
    } else {
        c
    }
}

/// Whether `c` is part of a word for the purposes of whole-word matching.
///
/// Deliberately the same predicate GRegex's `\b` uses for a Unicode pattern, so the
/// literal and regular-expression paths agree with each other about where a word ends
/// — a reader who ticks *whole word* and then ticks *regular expression* must not see
/// the count change for a query that is its own regular expression.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether a match spanning `[start, end)` in `hay` is bounded by non-word characters
/// on both sides. The ends of the text count as boundaries.
fn is_whole_word(hay: &str, start: usize, end: usize) -> bool {
    let before_ok = hay[..start]
        .chars()
        .next_back()
        .is_none_or(|c| !is_word_char(c));
    let after_ok = hay[end..].chars().next().is_none_or(|c| !is_word_char(c));
    before_ok && after_ok
}

/// The literal scan.
///
/// Walks `hay` by character so the fold is applied per character rather than per byte,
/// and records BYTE offsets, because those are what every consumer indexes with. The
/// two are carried in named fields rather than a `(usize, char)` tuple: this function's
/// entire subject is byte-versus-character arithmetic, and the positional form had the
/// same variable read as a byte on one line and as a character on the next.
fn literal_ranges(
    hay: &str,
    needle: &[char],
    case_sensitive: bool,
    whole_word: bool,
) -> Vec<Range> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    struct HayChar {
        byte: usize,
        ch: char,
    }
    let folding = !case_sensitive;
    let chars: Vec<HayChar> = hay
        .char_indices()
        .map(|(byte, ch)| HayChar { byte, ch })
        .collect();
    let n = needle.len();
    let mut i = 0;
    while i + n <= chars.len() {
        let matched = (0..n).all(|k| fold(chars[i + k].ch, folding) == needle[k]);
        if matched {
            let start = chars[i].byte;
            let end = chars
                .get(i + n)
                .map_or_else(|| hay.len(), |HayChar { byte, .. }| *byte);
            if !whole_word || is_whole_word(hay, start, end) {
                out.push(Range { start, end });
                i += n; // non-overlapping
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The regular-expression scan.
///
/// `MatchInfo::next` is the iteration GRegex provides, and it is what handles the one
/// hazard here: a pattern that can match nothing (`a*`, `\b`, `^`) matches at every
/// position, so a naive "resume at the match end" loop never advances and hangs the
/// main thread. The progress guard below is belt-and-braces against that — it costs one
/// comparison per match and converts a hang into a truncated result, which is the right
/// trade for a loop driven by an untrusted pattern.
///
/// **A zero-length match is discarded, because the editor discards it.** MEASURED
/// against a live `GtkSourceSearchContext` over the same fixtures (`super::parity`):
/// `a*` over `abcabc` is two matches to the editor and was eight here, `\b` and `x?` and
/// `^` are all zero matches to the editor and were the text's length here. The editor is
/// the engine the reader also sees in the other pane, so it decides. Dropping them is
/// also the only answer that makes "N of M" navigable — an empty match has nothing to
/// highlight and nothing to scroll to, so a count including them promises positions the
/// reader can never be taken to, which is TDD 11.8's confidently-wrong answer.
fn regex_ranges(re: &glib::Regex, hay: &str) -> Vec<Range> {
    let mut out = Vec::new();
    // `match_` borrows the haystack as a NUL-terminated GLib string; it must outlive
    // the `MatchInfo`.
    let subject = glib::GString::from(hay.to_owned());
    let info = match re.match_(subject.as_gstr(), glib::RegexMatchFlags::empty()) {
        Ok(info) => info,
        Err(e) => {
            // A compiled pattern can still fail at match time (GRegex's backtracking
            // limit on a pathological pattern). Report what was found rather than
            // pretending the text has no matches.
            log::warn!(
                "find: the regular expression failed while matching: {}",
                e.message()
            );
            return out;
        }
    };
    let mut progressed_past = None;
    while info.matches() {
        let Some((start, end)) = info.fetch_pos(0) else {
            break;
        };
        let range = Range {
            start: start.max(0) as usize,
            end: end.max(0) as usize,
        };
        if progressed_past.is_some_and(|last| range.start < last) {
            break;
        }
        progressed_past = Some(range.start.saturating_add(1));
        // Not a match: see this function's doc. Iteration continues — a pattern that can
        // match empty can also match non-empty later (`a*` over `bba`).
        if range.end > range.start {
            out.push(range);
        }
        match info.next() {
            Ok(true) => (),
            Ok(false) => break,
            Err(e) => {
                log::warn!(
                    "find: the regular expression stopped advancing after {} matches: {}",
                    out.len(),
                    e.message()
                );
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{FindOptions, Matcher, Range};

    fn opts(case_sensitive: bool, whole_word: bool, regex: bool) -> FindOptions {
        FindOptions {
            case_sensitive,
            whole_word,
            regex,
        }
    }

    fn found(hay: &str, query: &str, o: FindOptions) -> Vec<String> {
        Matcher::compile(query, o)
            .expect("the fixture patterns all compile")
            .ranges(hay)
            .into_iter()
            .map(|Range { start, end }| hay[start..end].to_string())
            .collect()
    }

    /// The behaviour every other case is measured against: the default options are the
    /// case-insensitive literal substring search the bar has always done.
    #[test]
    fn the_default_is_a_case_insensitive_substring() {
        assert_eq!(
            found(
                "Note the note in notebook",
                "note",
                opts(false, false, false)
            ),
            vec!["Note", "note", "note"]
        );
    }

    #[test]
    fn case_sensitivity_distinguishes_otherwise_identical_words() {
        assert_eq!(
            found("Note the note", "Note", opts(true, false, false)),
            vec!["Note"]
        );
    }

    /// Whole word is about the characters AROUND the match, so it has to be checked on
    /// both sides and at both ends of the text.
    #[test]
    fn whole_word_rejects_a_match_inside_a_longer_word() {
        assert_eq!(
            found(
                "note notebook keynote note",
                "note",
                opts(false, true, false)
            ),
            vec!["note", "note"]
        );
    }

    #[test]
    fn whole_word_accepts_a_match_at_either_end_of_the_text() {
        assert_eq!(
            found("note", "note", opts(false, true, false)),
            vec!["note"]
        );
    }

    #[test]
    fn whole_word_treats_an_underscore_and_a_digit_as_part_of_the_word() {
        assert!(found("note_2", "note", opts(false, true, false)).is_empty());
        assert!(found("note2", "note", opts(false, true, false)).is_empty());
        assert_eq!(
            found("note-2", "note", opts(false, true, false)),
            vec!["note"]
        );
    }

    /// Non-overlapping, left to right — what "the next match" means in a find bar.
    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(found("aaaa", "aa", opts(false, false, false)).len(), 2);
    }

    /// Byte ranges must index the haystack, not a folded copy of it, or a highlight
    /// lands on the wrong characters the moment the text is not ASCII.
    #[test]
    fn a_multibyte_haystack_yields_ranges_that_index_it() {
        let hay = "café CAFÉ";
        let ranges = Matcher::compile("café", opts(false, false, false))
            .expect("literal")
            .ranges(hay);
        assert_eq!(ranges.len(), 2);
        for Range { start, end } in ranges {
            // Slicing on a non-boundary panics, which is the failure this guards.
            assert_eq!(hay[start..end].to_lowercase(), "café");
        }
    }

    #[test]
    fn an_empty_query_matches_nothing_in_every_mode() {
        for o in [
            opts(false, false, false),
            opts(true, true, false),
            opts(false, false, true),
            opts(true, true, true),
        ] {
            assert!(
                found("anything at all", "", o).is_empty(),
                "an empty query matched something with {o:?}"
            );
        }
    }

    #[test]
    fn a_regular_expression_matches_by_pattern() {
        assert_eq!(
            found("a1 b22 c333", r"[a-z]\d+", opts(false, false, true)),
            vec!["a1", "b22", "c333"]
        );
    }

    #[test]
    fn a_regular_expression_honours_case_sensitivity() {
        assert_eq!(
            found("Cat cat", "cat", opts(true, false, true)),
            vec!["cat"]
        );
        assert_eq!(
            found("Cat cat", "cat", opts(false, false, true)),
            vec!["Cat", "cat"]
        );
    }

    /// The wrapper has to be a non-capturing GROUP: `\bcat|dog\b` binds each anchor to
    /// one branch of the alternation and quietly matches `dog` inside `dogma`.
    /// TDD 11.14.
    #[test]
    fn whole_word_wraps_an_alternation_as_a_group() {
        assert_eq!(
            found("dogma cat dog", "cat|dog", opts(false, true, true)),
            vec!["cat", "dog"]
        );
    }

    /// A pattern that can match nothing matches at every position. The guard's job is
    /// that this terminates at all; what it returns is secondary.
    ///
    /// What it returns is nevertheless pinned, because it was MEASURED against the
    /// editor's engine and made to agree with it: a zero-length match is not a match.
    /// `a*` over `banana` is the three runs of `a`, not the seven positions at which an
    /// empty string sits. `super::super::parity` is where the two engines are compared;
    /// this is the same rule held without a display.
    #[test]
    fn a_pattern_that_can_match_nothing_terminates() {
        let m = Matcher::compile("a*", opts(false, false, true)).expect("compiles");
        assert_eq!(
            found("banana", "a*", opts(false, false, true)),
            ["a", "a", "a"]
        );
        assert!(m.ranges("banana").iter().all(|r| r.end > r.start));
        // A pattern with no non-empty match at all has no matches, rather than one per
        // position — the state `\b` and `x?` put the engine in.
        assert!(m.ranges("").is_empty());
        for query in [r"\b", "x?", "^"] {
            assert!(
                found("banana", query, opts(false, false, true)).is_empty(),
                "{query} matched only empty strings and must therefore match nothing"
            );
        }
    }

    /// `^` under MULTILINE means the start of each line, which is what a reader
    /// searching a Markdown document expects of it.
    #[test]
    fn an_anchor_means_the_start_of_a_line() {
        assert_eq!(
            found("# one\ntwo\n# three", r"^# \w+", opts(false, false, true)).len(),
            2
        );
    }

    /// The reader is told the pattern is broken; the caller is not handed an empty
    /// result that reads as "no matches".
    #[test]
    fn a_malformed_pattern_is_an_error_not_an_empty_result() {
        let Err(err) = Matcher::compile("(unclosed", opts(false, false, true)) else {
            panic!("an unclosed group must not compile");
        };
        assert!(
            !err.message.is_empty(),
            "the error carries no message to show the reader"
        );
    }

    /// The same text treated as a literal is not a pattern — a reader who has not
    /// ticked the box must not have their punctuation interpreted.
    #[test]
    fn a_literal_query_is_not_interpreted_as_a_pattern() {
        assert_eq!(
            found("a.c abc", "a.c", opts(false, false, false)),
            vec!["a.c"]
        );
    }

    #[test]
    fn count_counts_every_case_insensitive_occurrence() {
        let m = Matcher::compile("note", opts(false, false, false)).expect("literal");
        assert_eq!(m.count("note NOTE notebook"), 3);
    }
}
