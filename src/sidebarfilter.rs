//! The sidebar filter's display-free core: what a typed filter means, which text it
//! matches, and how a match is shown (TDD 12.26–12.29, 20.24–20.25).
//!
//! Both sidebar panes — the outline and the annotations viewer — filter through this one
//! matcher, so a query narrows the two lists by the same rule. The outline's tree-shaped
//! half (which ancestors stay as context) is `outline::filter`; the window wiring that
//! owns the filter boxes is `window::sidebarfilter`.
//!
//! **The rule.** A query is split on whitespace into words. A row matches when every word
//! occurs somewhere in its text, as a substring, in any order, ignoring case. Diacritics
//! are not folded: "resume" does not match "résumé". A row with several fields (an
//! annotation's comment and its quoted text) matches when each word occurs in at least one
//! of them, and every occurrence of every word is highlighted in whichever field holds it.

use std::ops::Range;

/// The placeholder a filtered outline shows when no heading matches.
pub(crate) const NO_MATCHING_HEADINGS: &str = "No matching headings";
/// The placeholder a filtered annotations list shows when no annotation matches.
pub(crate) const NO_MATCHING_COMMENTS: &str = "No matching comments";

/// One pane's filter as the reader left it: whether its box is open, and what it holds.
///
/// Held per DOCUMENT (`TabState`), because the filter boxes are window chrome shared by
/// every tab while the reader's filter belongs to the document they typed it against —
/// the same split the outline's folding memory makes. Not persisted across restarts.
///
/// Closing the box always empties it (GtkSearchBar clears its entry when it hides), so a
/// closed filter holding text is not a state the window produces; [`Self::query`] still
/// treats "closed" as inactive whatever the text, so no rows can ever be hidden by a
/// filter that is out of sight (TDD 12.27).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PaneFilter {
    pub(crate) open: bool,
    pub(crate) text: String,
}

impl PaneFilter {
    /// The query this filter applies, or `None` while it is inactive — closed, or open
    /// but holding only whitespace.
    pub(crate) fn query(&self) -> Option<Query> {
        if self.open {
            Query::parse(&self.text)
        } else {
            None
        }
    }
}

/// A parsed, non-empty filter query: its words, each already case-folded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Query {
    words: Vec<Vec<char>>,
}

impl Query {
    /// Parse `text` into a query, or `None` when it holds no words at all.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let words: Vec<Vec<char>> = text
            .split_whitespace()
            .map(|w| w.chars().flat_map(char::to_lowercase).collect())
            .collect();
        (!words.is_empty()).then_some(Self { words })
    }

    /// Whether `text` matches, and if so the byte ranges of `text` to highlight.
    pub(crate) fn find(&self, text: &str) -> Option<Vec<Range<usize>>> {
        self.find_in_fields(&[text])
            .and_then(|mut per_field| per_field.pop())
    }

    /// Match a row made of several text fields. `Some` when every word occurs in at
    /// least one field; the result holds, per field, the merged byte ranges of every
    /// occurrence of every word in that field.
    pub(crate) fn find_in_fields(&self, fields: &[&str]) -> Option<Vec<Vec<Range<usize>>>> {
        let folded: Vec<Vec<FoldedChar>> = fields.iter().map(|f| fold(f)).collect();
        let mut per_field: Vec<Vec<Range<usize>>> = vec![Vec::new(); fields.len()];
        for word in &self.words {
            let mut found = false;
            for (field, hay) in folded.iter().enumerate() {
                let hits = occurrences(hay, word);
                found |= !hits.is_empty();
                per_field[field].extend(hits);
            }
            if !found {
                return None;
            }
        }
        Some(per_field.into_iter().map(merge).collect())
    }
}

/// One case-folded character of a haystack, remembering the byte range of the original
/// character it came from. A character whose lowercase form is several characters (`İ`)
/// yields several entries sharing one range, so a match can never split an original
/// character: its highlight always covers whole characters of the text as displayed.
#[derive(Debug, Clone, Copy)]
struct FoldedChar {
    ch: char,
    start: usize,
    end: usize,
}

fn fold(text: &str) -> Vec<FoldedChar> {
    let mut out = Vec::with_capacity(text.len());
    for (start, original) in text.char_indices() {
        let end = start + original.len_utf8();
        for ch in original.to_lowercase() {
            out.push(FoldedChar { ch, start, end });
        }
    }
    out
}

/// Every occurrence of `word` in `hay`, overlapping ones included, as original-text byte
/// ranges. Quadratic in the worst case, which is fine at the size of a heading or a
/// comment — the rows this filters are short, and the filter runs once per settled
/// keystroke, not per frame.
fn occurrences(hay: &[FoldedChar], word: &[char]) -> Vec<Range<usize>> {
    if word.is_empty() || word.len() > hay.len() {
        return Vec::new();
    }
    (0..=hay.len() - word.len())
        .filter(|&i| {
            hay[i..i + word.len()]
                .iter()
                .zip(word)
                .all(|(h, w)| h.ch == *w)
        })
        .map(|i| hay[i].start..hay[i + word.len() - 1].end)
        .collect()
}

/// Sort and merge overlapping or touching ranges, so a highlight is one span per run.
fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// Pango markup for `text` with `ranges` highlighted (bold and underlined), every
/// fragment escaped. The sidebar rows are GTK chrome, not the themed preview, so the
/// highlight is expressed in weight and underline — relative to whatever font and ink the
/// desktop theme gives the row — rather than in a colour the theme would have to supply.
///
/// Built through one escaping funnel because a row caption is document text: a heading
/// titled `R&D <draft>` must not be read as markup (GTK4Rs/AP-154). Ranges are the
/// matcher's own output, so they fall on character boundaries by construction.
pub(crate) fn highlight_markup(text: &str, ranges: &[Range<usize>]) -> String {
    let mut out = String::with_capacity(text.len() + ranges.len() * 32);
    let mut at = 0;
    for r in ranges {
        if r.start < at || r.end > text.len() {
            continue;
        }
        out.push_str(&gtk::glib::markup_escape_text(&text[at..r.start]));
        out.push_str("<span weight=\"bold\" underline=\"single\">");
        out.push_str(&gtk::glib::markup_escape_text(&text[r.clone()]));
        out.push_str("</span>");
        at = r.end;
    }
    out.push_str(&gtk::glib::markup_escape_text(&text[at..]));
    out
}

/// The words a suggested replacement is quoted as, wherever it is quoted — the card, the
/// annotations viewer's row (and so the text its filter searches) and the export:
/// `old → new` (TDD 17.72, 20.2).
pub(crate) fn suggestion_text(old: &str, new: &str) -> String {
    format!("{old}{SUGGESTION_ARROW}{new}")
}

const SUGGESTION_ARROW: &str = " \u{2192} ";

/// [`suggestion_text`] as Pango markup: the old side struck through, the new side in
/// bold, so the change reads at a glance (TDD 17.72). `hits` are a filter's matches in
/// [`suggestion_text`]'s own coordinates and are shown as [`highlight_markup`] shows
/// them, cut where the two sides meet; a hit across the arrow is dropped rather than
/// crossing a tag.
pub(crate) fn suggestion_markup(old: &str, new: &str, hits: &[Range<usize>]) -> String {
    let arrow_at = old.len();
    let new_at = arrow_at + SUGGESTION_ARROW.len();
    let within = |lo: usize, hi: usize| -> Vec<Range<usize>> {
        hits.iter()
            .filter(|r| r.start >= lo && r.end <= hi)
            .map(|r| r.start - lo..r.end - lo)
            .collect()
    };
    format!(
        "<s>{}</s>{}<b>{}</b>",
        highlight_markup(old, &within(0, arrow_at)),
        gtk::glib::markup_escape_text(SUGGESTION_ARROW),
        highlight_markup(new, &within(new_at, new_at + new.len())),
    )
}

/// The filter bar's match count while its filter is active: `"3 of 40"` — how many rows
/// match out of the pane's total.
pub(crate) fn match_count(matched: usize, total: usize) -> String {
    format!("{matched} of {total}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One highlight span, spelled so clippy does not read it as a mistaken range-vec.
    fn one(r: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
        vec![r]
    }

    fn q(text: &str) -> Query {
        Query::parse(text).expect("a query with words")
    }

    #[test]
    fn a_blank_query_is_no_query() {
        assert!(Query::parse("").is_none());
        assert!(Query::parse("   \t ").is_none());
    }

    #[test]
    fn a_closed_or_blank_filter_is_inactive() {
        let closed = PaneFilter {
            open: false,
            text: "base".into(),
        };
        assert!(
            closed.query().is_none(),
            "a filter out of sight hides nothing"
        );
        let blank = PaneFilter {
            open: true,
            text: "  ".into(),
        };
        assert!(blank.query().is_none());
        let live = PaneFilter {
            open: true,
            text: "base".into(),
        };
        assert!(live.query().is_some());
    }

    #[test]
    fn every_word_must_occur_in_any_order_ignoring_case_as_a_substring() {
        let query = q("SETUP base");
        assert!(query.find("Base Setup").is_some(), "any order, any case");
        assert!(
            query.find("Database setups").is_some(),
            "partial words count"
        );
        assert!(query.find("Setup only").is_none(), "every word must occur");
    }

    #[test]
    fn every_occurrence_of_every_word_is_highlighted_and_merged() {
        let ranges = q("an").find("Banana plan").unwrap();
        // "Banana" holds "an" at 1 and 3 (touching → merged), "plan" at 9.
        assert_eq!(ranges, vec![1..5, 9..11]);
        let ranges = q("ab bc").find("abc").unwrap();
        assert_eq!(ranges, one(0..3), "overlapping words merge into one span");
    }

    #[test]
    fn diacritics_are_not_folded() {
        assert!(q("resume").find("Résumé").is_none());
        assert!(q("résumé").find("RÉSUMÉ").is_some(), "case still folds");
    }

    #[test]
    fn ranges_are_byte_ranges_of_the_original_text() {
        let text = "Ünïcode — Straße";
        let ranges = q("straße").find(text).unwrap();
        assert_eq!(&text[ranges[0].clone()], "Straße");
        // A character whose lowercase is two characters still matches whole.
        let text = "İstanbul";
        let ranges = q("i̇s").find(text).unwrap();
        assert_eq!(&text[ranges[0].clone()], "İs");
    }

    #[test]
    fn a_multi_field_row_needs_each_word_in_some_field() {
        let query = q("claim note");
        let hit = query
            .find_in_fields(&["a note here", "the claim"])
            .expect("each word occurs in one field or the other");
        assert_eq!(hit, vec![one(2..6), one(4..9)]);
        assert!(query.find_in_fields(&["a note here", "nothing"]).is_none());
    }

    #[test]
    fn a_suggestion_strikes_its_old_side_and_keeps_filter_hits_on_each_side() {
        assert_eq!(suggestion_text(",", ":"), ", \u{2192} :");
        assert_eq!(
            suggestion_markup("a&b", "c", &[]),
            "<s>a&amp;b</s> \u{2192} <b>c</b>"
        );
        let text = suggestion_text("cat", "dog");
        let hits = [q("a").find(&text).unwrap(), q("o").find(&text).unwrap()].concat();
        assert_eq!(
            suggestion_markup("cat", "dog", &hits),
            "<s>c<span weight=\"bold\" underline=\"single\">a</span>t</s> \u{2192} \
             <b>d<span weight=\"bold\" underline=\"single\">o</span>g</b>"
        );
    }

    #[test]
    fn highlight_markup_escapes_every_fragment() {
        let text = "R&D <draft> notes";
        let ranges = q("draft").find(text).unwrap();
        assert_eq!(
            highlight_markup(text, &ranges),
            "R&amp;D &lt;<span weight=\"bold\" underline=\"single\">draft</span>&gt; notes"
        );
        assert_eq!(highlight_markup("a&b", &[]), "a&amp;b");
    }

    #[test]
    fn highlight_markup_ignores_a_range_it_cannot_honour() {
        // Out of order or out of bounds: dropped rather than panicking.
        assert_eq!(
            highlight_markup("abc", &[2..3, 0..1, 1..9]),
            "ab<span weight=\"bold\" underline=\"single\">c</span>"
        );
    }

    #[test]
    fn the_match_count_reads_matches_of_the_total() {
        assert_eq!(match_count(3, 40), "3 of 40");
    }
}
