//! Tight, Pandoc-style inline decoration scanner — the crate parses superscript
//! `^x^`, subscript `~x~`, `~~strikethrough~~`, and `==highlight==` itself rather
//! than letting pulldown-cmark handle them (ScrAP-66; see [`super::normalize`]). The
//! last has no pulldown option at all — `Options` has no highlight/mark flag — so
//! tokenising it here is the only way `==text==` can render.

/// One inline decoration a run may carry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Script {
    None,
    Superscript,
    Subscript,
    Strikethrough,
    Highlight,
}

/// One construct [`scan_script_spans`] recognised, located in the scanned text.
///
/// `outer` is the whole construct **including its delimiters** — the span that must
/// never be split (a `{==…==}` annotation landing inside it produces markup that
/// parses as neither construct) — and `inner` is the content between them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct ScriptSpan {
    pub(crate) outer: std::ops::Range<usize>,
    pub(crate) inner: std::ops::Range<usize>,
    pub(crate) script: Script,
}

/// Locate the *tight*, Pandoc-style constructs this crate tokenises itself —
/// superscript `^x^`, subscript `~x~`, `~~strikethrough~~`, `==highlight==` — as
/// byte ranges within `text`.
///
/// Rules: superscript/subscript are tight — a marker opens a run only if a matching
/// closing marker appears before any whitespace and the content is non-empty (so
/// `H~2~O` matches but `a ~ b` and prose carets like `2^10` stay literal).
/// Strikethrough uses a `~~ … ~~` fence whose content MAY contain spaces; highlight
/// a `== … ==` fence whose markers must abut non-whitespace on their inner side.
/// Unmatched markers are literal text and yield no span.
///
/// **This is the single definition of what these four constructs are**, and it is
/// deliberately span-shaped rather than string-shaped: [`super::segments`] cuts
/// rendered runs from it, and `copymap::balance_source_span` decides annotation
/// boundaries from it.
/// A consumer that walks only pulldown-cmark's event stream cannot see these
/// constructs at all — they arrive as plain `Text` — so anything reasoning about
/// inline-construct extent must consult this scanner too (ScrAP-195).
///
/// **Scope: ONE run.** A fence that wraps other inline markup (`~~a **bold** b~~`)
/// is split across several pulldown `Text` events and cannot be seen from any one
/// of them; [`super::segments::BlockScripts`] is the caller that stitches a block's
/// runs back together before calling this, and every consumer goes through it.
pub(crate) fn scan_script_spans(text: &str) -> Vec<ScriptSpan> {
    let mut spans: Vec<ScriptSpan> = Vec::new();
    // `base` is the byte offset of `rest` within `text`, so every range pushed is
    // absolute in `text` rather than relative to the remaining slice.
    let mut base = 0usize;
    let mut rest = text;

    while let Some(mpos) = rest.find(['^', '~', '=']) {
        // `== … ==` highlight (mark) fence. Tight flanking — a marker abuts
        // non-whitespace on its INNER side — so ordinary prose (`a == b`, `x -= 1`)
        // stays literal; only `==marked==` (spaces allowed *between* the flanks, like
        // strikethrough) becomes a highlight. Same per-event limitation as the strike
        // fence below: a `==` fence wrapping other inline markup won't span across the
        // separate events pulldown splits it into — an accepted edge, as for `~~`.
        if rest.as_bytes()[mpos] == b'=' {
            if rest[mpos + 1..].starts_with('=') {
                let after = &rest[mpos + 2..];
                // Opener valid only if immediately followed by non-whitespace.
                if after.chars().next().is_some_and(|c| !c.is_whitespace()) {
                    if let Some(cpos) = after.find("==") {
                        let content = &after[..cpos];
                        // Closer valid only if immediately preceded by non-whitespace
                        // (`next_back()` is `None` for empty content, rejecting `====`).
                        if content
                            .chars()
                            .next_back()
                            .is_some_and(|c| !c.is_whitespace())
                        {
                            let inner = base + mpos + 2;
                            spans.push(ScriptSpan {
                                outer: base + mpos..inner + cpos + 2,
                                inner: inner..inner + cpos,
                                script: Script::Highlight,
                            });
                            base = inner + cpos + 2;
                            rest = &after[cpos + 2..];
                            continue;
                        }
                    }
                }
            }
            // Not a valid fence (lone `=`, or empty/space-flanked content): the `=` is
            // literal — skip it and re-scan from the next char.
            base += mpos + 1;
            rest = &rest[mpos + 1..];
            continue;
        }

        // `~~ … ~~` strikethrough fence (content may contain spaces).
        if rest.as_bytes()[mpos] == b'~' && rest[mpos + 1..].starts_with('~') {
            let after = &rest[mpos + 2..];
            if let Some(cpos) = after.find("~~") {
                if cpos > 0 {
                    let inner = base + mpos + 2;
                    spans.push(ScriptSpan {
                        outer: base + mpos..inner + cpos + 2,
                        inner: inner..inner + cpos,
                        script: Script::Strikethrough,
                    });
                    base = inner + cpos + 2;
                    rest = &after[cpos + 2..];
                    continue;
                }
            }
            // No closing fence: the first `~` is literal, re-scan from the second.
            base += mpos + 1;
            rest = &rest[mpos + 1..];
            continue;
        }

        // Tight superscript `^x^` / subscript `~x~` (no internal whitespace).
        let marker = rest.as_bytes()[mpos] as char;
        let script = if marker == '^' {
            Script::Superscript
        } else {
            Script::Subscript
        };
        let after = &rest[mpos + 1..];

        // Find the closing marker, aborting if whitespace intervenes first.
        let close = after.char_indices().find_map(|(i, c)| {
            if c == marker {
                Some(Some(i))
            } else if c.is_whitespace() {
                Some(None)
            } else {
                None
            }
        });

        match close.flatten() {
            // `ci > 0` rejects empty content (e.g. `^^`).
            Some(ci) if ci > 0 => {
                let inner = base + mpos + 1;
                spans.push(ScriptSpan {
                    outer: base + mpos..inner + ci + 1,
                    inner: inner..inner + ci,
                    script,
                });
                base = inner + ci + 1;
                rest = &after[ci + 1..];
            }
            _ => {
                // No tight match: the marker is literal.  Skip past it and continue.
                base += mpos + 1;
                rest = after;
            }
        }
    }

    spans
}

/// The inline [`TagName`](crate::tags::TagName) that a non-`None` `Script` maps to,
/// if any.
pub(super) fn script_tag(script: Script) -> Option<crate::tags::TagName> {
    use crate::tags::TagName;
    match script {
        Script::Superscript => Some(TagName::Superscript),
        Script::Subscript => Some(TagName::Subscript),
        Script::Strikethrough => Some(TagName::Strike),
        Script::Highlight => Some(TagName::Mark),
        Script::None => None,
    }
}

#[cfg(test)]
mod tight_construct_tests {
    use super::Script;
    use crate::renderer::segments_of;

    /// The rendered runs of `s` — every non-delimiter segment, markers stripped —
    /// which is the view the renderer, the outline and the export walk all take.
    fn runs(s: &str) -> Vec<(String, Script)> {
        segments_of(s)
            .into_iter()
            .filter(|g| !g.marker)
            .map(|g| (g.text(s).to_string(), g.script))
            .collect()
    }

    #[test]
    fn tight_superscript_matches_pandoc() {
        assert_eq!(
            runs("E=mc^2^"),
            vec![
                ("E=mc".into(), Script::None),
                ("2".into(), Script::Superscript),
            ],
        );
    }

    #[test]
    fn tight_subscript_matches_pandoc() {
        assert_eq!(
            runs("H~2~O"),
            vec![
                ("H".into(), Script::None),
                ("2".into(), Script::Subscript),
                ("O".into(), Script::None),
            ],
        );
    }

    #[test]
    fn whitespace_in_content_is_literal() {
        // Pandoc forbids spaces in tight scripts; leave the markers untouched.
        assert_eq!(runs("a^b c^d"), vec![("a^b c^d".into(), Script::None)]);
        assert_eq!(
            runs("1~2 approx"),
            vec![("1~2 approx".into(), Script::None)]
        );
    }

    #[test]
    fn unmatched_and_empty_markers_are_literal() {
        assert_eq!(
            runs("2^10 no close"),
            vec![("2^10 no close".into(), Script::None)]
        );
        assert_eq!(runs("a^^b"), vec![("a^^b".into(), Script::None)]);
        assert_eq!(runs("plain"), vec![("plain".into(), Script::None)]);
    }

    #[test]
    fn multiple_scripts_in_one_run() {
        assert_eq!(
            runs("x^2^ + y~n~"),
            vec![
                ("x".into(), Script::None),
                ("2".into(), Script::Superscript),
                (" + y".into(), Script::None),
                ("n".into(), Script::Subscript),
            ],
        );
    }

    #[test]
    fn strikethrough_fence_allows_spaces() {
        assert_eq!(
            runs("~~gone now~~ ok"),
            vec![
                ("gone now".into(), Script::Strikethrough),
                (" ok".into(), Script::None),
            ],
        );
    }

    #[test]
    fn double_tilde_wins_over_subscript() {
        // A `~~x~~` fence is strikethrough, not two subscripts.
        assert_eq!(runs("~~x~~"), vec![("x".into(), Script::Strikethrough)]);
        // Multi-tilde tight subscript line (the fragmentation regression).
        assert_eq!(
            runs("H~2~O and CO~2~ here"),
            vec![
                ("H".into(), Script::None),
                ("2".into(), Script::Subscript),
                ("O and CO".into(), Script::None),
                ("2".into(), Script::Subscript),
                (" here".into(), Script::None),
            ],
        );
    }

    #[test]
    fn unterminated_double_tilde_is_literal() {
        assert_eq!(
            runs("~~open only"),
            vec![("~~open only".into(), Script::None)]
        );
    }

    #[test]
    fn multibyte_content_slices_correctly() {
        assert_eq!(
            runs("Å^π^"),
            vec![
                ("Å".into(), Script::None),
                ("π".into(), Script::Superscript),
            ],
        );
    }

    #[test]
    fn highlight_fence_matches_and_allows_internal_spaces() {
        assert_eq!(
            runs("==marked=="),
            vec![("marked".into(), Script::Highlight)]
        );
        assert_eq!(
            runs("a ==big deal== ok"),
            vec![
                ("a ".into(), Script::None),
                ("big deal".into(), Script::Highlight),
                (" ok".into(), Script::None),
            ],
        );
    }

    #[test]
    fn prose_equals_stays_literal() {
        // Comparisons and operators must NOT become highlights (tight flanking).
        assert_eq!(runs("a == b"), vec![("a == b".into(), Script::None)]);
        assert_eq!(
            runs("x -= 1 and y == 2"),
            vec![("x -= 1 and y == 2".into(), Script::None)]
        );
        // A space just inside either marker rejects the fence.
        assert_eq!(
            runs("== spaced =="),
            vec![("== spaced ==".into(), Script::None)]
        );
        // Empty (`====`) and lone `=` stay literal.
        assert_eq!(runs("===="), vec![("====".into(), Script::None)]);
        assert_eq!(runs("a = b"), vec![("a = b".into(), Script::None)]);
    }

    #[test]
    fn highlight_coexists_with_tight_scripts() {
        assert_eq!(
            runs("E=mc^2^ then ==note=="),
            vec![
                ("E=mc".into(), Script::None),
                ("2".into(), Script::Superscript),
                (" then ".into(), Script::None),
                ("note".into(), Script::Highlight),
            ],
        );
    }
}
