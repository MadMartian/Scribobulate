# Plan: Spell Checking

**Status: ready for TDD rubrics.** The mechanism is chosen and the four facts it
rests on were measured on this machine's real GTK 4.6.9 / GtkSourceView 5.4.1,
not inferred (see [Measured findings](#measured-findings)). Per the SDD
plan-kickoff rule, propose TDD rubrics to the operator and get a response
**before** writing implementation code.

## Problem

Scribobulate is a Markdown **prose** editor with no spell checking. Prose is the
dominant content type it is used for, and every comparable editor flags
misspellings inline and offers corrections. The absence is felt in the one pane
where the user actually types — the editor `GtkSourceView`.

The requested feature, as the operator specified it:

1. Spell checking is **toggleable** — enable / disable.
2. **Toggling always refreshes the state.** Turning it on re-checks from scratch;
   turning it off clears. No cached or partial carry-over across a toggle.
3. Correcting a misspelled word is offered from the **context menu**.
4. It is enabled from the **Edit menu** and from the **toolbar's Edit section**.
5. It has an **accelerator, but not a shortcut key** — see
   [Terminology](#terminology-accelerator-vs-shortcut-key), a real distinction in
   this codebase.

### Terminology: "accelerator" vs "shortcut key"

**Confirmed by the operator.** It means a **menu access key** (the underlined
letter, reached via Alt-navigation and as a bare letter inside the context-menu
popover), and **no `<Primary>`-style key binding**. Recorded rather than left
implicit, because it decides three things:

- A `Cmd` row with **`accel: ""`**, exactly like the existing `win.auto-reload`
  and `win.allow-outside-links` toggles.
- **No** `INLINE_ACCEL_CMDS` row and **no** Keyboard Shortcuts help-window row —
  correct, because that window lists key bindings and this command has none.
- One new `MENU_MNEMONICS` entry (`src/app/mnemonics.rs`) whose access letter is
  unique within the Edit menu.

The Action CAM's accelerator row is scoped to "commands that *have* an
accelerator only", so it is satisfied vacuously — this command is correctly
absent from the Keyboard Shortcuts help window rather than missing from it.

## Measured findings

Five claims the design depends on. Each was **measured**, because each had been
either hedged in the research answer or was version-specific. Measured headless
under Xvfb on **GTK 4.6.9 / GtkSourceView 5.4.1**, with a throwaway probe that has
since been deleted — the numbers below are the artefact worth keeping, and
[the checks become real tests](#these-five-checks-become-gtktests-not-a-probe) at
implementation time.

### P1 — A foreign `GtkTextTag` SURVIVES GtkSourceView's re-highlighting. ✅

This was the fact flagged as most likely to decide the whole approach. Our own
`underline = Error` tag stayed applied across **all four** paths that re-highlight:

| Path | Tag still applied |
|---|---|
| after `apply_tag` | ✅ |
| after explicit `ensure_highlight` over the region | ✅ |
| **after a real edit on the same line** (the case that matters) | ✅ |
| after `set_highlight_syntax(false)` → `(true)` → re-highlight | ✅ |

Mechanism, researcher-sourced from `gtksourcecontextengine.c`: `highlight_region`
→ `unhighlight_region` iterates `ce->tags` and removes **only tags the engine
itself created**; our tag is not in that set. Engine tags are also deliberately
given *low* priority (`gtk_text_tag_set_priority (new_tag, ce->n_tags)`, commented
"lower than user tags"), so the engine owns fg/bg while we own `underline`, and
the split is stable across re-highlights.

**Consequence: the only reason to pay libspelling's portability tax is gone.**

### P2 — GtkSourceView already marks most skip-regions, but NOT all. ✅ / ⚠️

The markdown language definition populates the `no-spell-check` context class,
readable via `buffer.iter_has_context_class(&iter, "no-spell-check")` (present in
the `sourceview5` 0.10 bindings). Measured coverage:

| Region | Context classes | Correct? |
|---|---|---|
| prose | `[]` | ✅ check it |
| inline code span | `["no-spell-check"]` | ✅ skip |
| fenced code block | `["no-spell-check"]` | ✅ skip |
| link **destination** | `["no-spell-check"]` | ✅ skip |
| image path | `["no-spell-check"]` | ✅ skip |
| link **text** | `[]` | ✅ check it — link text *is* prose |
| **bare/autolink URL** | `[]` | ⚠️ **gap — would be flagged** |
| **HTML attribute value** | `["string"]` | ⚠️ **gap — `string`, not `no-spell-check`** |

This is a much better starting point than assumed — including the subtle
correctness of checking link *text* while skipping link *destinations*. But the
two gaps are real and must be closed, or the first document with a bare URL in it
teaches the user to switch the feature off.

### P3 — GTK's word iteration SPLITS `don't` and `well-known`. ❌ **Refutes the research answer.**

The research answer stated these are kept whole (UAX#29 MidLetter). **Measured
false** on this stack via `GtkTextIter::forward_word_end` / `backward_word_start`:

```
input:  They don't like well-known jargon.
output: ["They", "don", "t", "like", "well", "known", "jargon"]
```

So the API you would actually reach for hands a checker `don` and `t` as separate
words. This is precisely the "worse than no checker" outcome, and it would have
been discovered only after implementation. **The plan must own its own
tokenizer** treating `'` and `-` as word-internal.

The likely mechanism (unconfirmed, referred back to the researcher): Pango's log
attrs distinguish `is_word_boundary` (the UAX#29 predicate) from
`is_word_start`/`is_word_end` (Pango's own coarser word notion), and the
`GtkTextIter` word API is built on the latter. **The documentation's UAX#29 claim
and the behaviour of the function you call are not the same thing.**

### P4 — A whole-buffer re-tag does NOT move the viewport. ✅

Measured on a view configured exactly as the product configures it
(`WrapMode::Word`), scrolled to the middle of an 800-line document:

| | value | upper |
|---|---|---|
| before | 21363.00 | 43326.00 |
| after `apply_tag` over the entire buffer | 21363.00 | 43326.00 |
| after `remove_tag` over the entire buffer | 21363.00 | 43326.00 |

Delta **0.00** on both, both directions. **Consequence: the Reading-Position
Preservation CAM does not gain a row** (see the Obligations section).

**How this measurement was nearly a false PASS**, recorded because the near-miss
is the lesson: the first version of the probe used short filler lines, so
`WrapMode::Word` was **inert** — nothing could re-wrap, and the run "passed"
while testing nothing. The fixture now uses long lines and **asserts the
precondition loudly** (wrapped line height 54 vs single-line 18, a 3× check that
aborts the probe if wrapping is not actually happening). This is the ScrAP-183
shape: a measurement that fails on an earlier precondition proves nothing about
the thing under test.

### P5 — Tagging does NOT dirty the document. ✅

Checked because the failure would have been silent and severe: if applying tags
marked the buffer modified, then merely *switching spell-check on* would mark
every open document dirty, badge every tab, enable Save, and prompt "unsaved
changes" on close — without the user having typed a character.

| | before | after whole-word `apply_tag` |
|---|---|---|
| `buffer.is_modified()` | false | **false** |
| buffer text identical | — | **true** |

Safe on both counts, and safe for two independent reasons — GTK does not set the
modified flag for a tag-only change, *and* this project's dirty test is
`editor_text() != *saved_baseline` (`TabState::is_dirty`), a pure text comparison
that tags cannot perturb even in principle. Recorded so nobody re-derives it.

### These five checks become `#[gtk::test]`s, not a probe

The probe that produced the numbers above was an `examples/` binary, and it has
been **deleted rather than kept**. Keeping it would have been the worse choice on
both counts that matter:

- **As a build target it rots.** A product tree carrying a scratch example nobody
  runs is maintenance surface with no owner, and its value expires the moment its
  findings are written down — which they now are, with numbers.
- **As a verification mechanism it is weaker than what we already have.** P1, P2
  and P4 are not one-time facts; they are **ongoing invariants that can silently
  regress under us**. If a GtkSourceView update changes `no-spell-check` coverage,
  or starts touching foreign tags, the feature degrades quietly and no example
  anyone has to remember to run will catch it.

So at implementation time these become **headless `#[gtk::test]`s** alongside the
feature. That is strictly better for the cross-platform question too: the ports
get the checks **automatically when they run the suite**, on their own
GTK 4.22 / GtkSourceView 5.x, instead of being asked to remember to run a
throwaway binary before someone deletes it.

Three earn a permanent test (P1 tag survival, P2 the coverage table, P4 the
viewport). P3 becomes the tokenizer's own boundary-table unit test — pure, no
display. P5 needs no ongoing test: it is a property of GTK plus a text comparison,
neither of which can drift.

### ScrAP Finding

`GtkTextIter`'s word API splits `don't` and `well-known` — it is not the UAX#29 boundary predicate (whatever UAX#29 is, smells like a dead reference).  During planning out this feature, this anti-pattern surfaced and poses as a candidate for entry into the register.

**Symptom**: word-level processing built on `GtkTextIter::forward_word_end` / `backward_word_start` returns fragments. Measured on GTK 4.6.9, iterating `They don't like well-known jargon.`:

```
["They", "don", "t", "like", "well", "known", "jargon"]
```

For a spell checker this is fatal in a way that is worse than not shipping: `don` and `t` are both "misspelled", so ordinary correct English prose is underlined everywhere, and the user turns the feature off permanently within a minute of meeting it.

**What was tried / how it was nearly missed**: a sourced research answer stated the opposite — that GTK/Pango word breaks implement UAX#29, under which an apostrophe is `MidLetter` and `don't` is a single word, and that hyphenated compounds are "typically" one word. That is a true statement *about UAX#29*. It is not a true statement about the function you call. The claim was flagged as worth verifying only because a hedge ("typically") appeared in it, and the measurement then refuted the unhedged half too.

**Root cause**: Pango computes several distinct per-character predicates into its log attrs, and two of them are easy to conflate — `is_word_boundary` (the UAX#29 notion) versus `is_word_start` / `is_word_end` (Pango's coarser "a word for editing purposes" notion, which treats punctuation as separating). `GtkTextIter`'s word-movement API is built on the latter, because its job is *caret movement for a text editor* — where stopping at the apostrophe in `don't` is arguably the right editing behaviour — not linguistic segmentation. The API is not broken; it is answering a different question from the one a spell checker asks.

**Resolution**: own the tokenizer for any *linguistic* consumer.

- Treat `'` and `-` as word-internal when they fall **between** letters (so `don't` and `well-known` survive, while a trailing possessive `students'` and an em-dash still break).
- Keep it display-free and unit-test the boundary table directly — `don't`, `well-known`, `rock'n'roll`, `students'`, `state-of-the-art`, an em-dash — rather than through a widget.
- `GtkTextIter`'s word API remains correct for what it is for: caret movement, double-click selection, and word-wise delete.

**Lesson**: two levels. Concretely — **a documented standard name is not an API contract**; "implements UAX#29" describes a specification, and the binding you reach for may expose a different, coarser predicate for its own good reasons. Generally — this is the cheapest possible class of finding to verify (one probe, one line of output, no display needed) and the most expensive to discover after implementation, because by then the tokenizer is load-bearing under a feature. When a claim is about *what a specific function returns for a specific input*, measure it; the cost is minutes and the alternative is a rewrite.


## Obligations — the CAM analysis

The operator's instruction was that this "requires CAM compliance because we're
changing something that affects the edit pane". Working out *which* matrices
apply is the substance of that. Three are in scope, one is out, and each verdict
is recorded with its reasoning, because "that CAM doesn't apply here" is exactly
the move that ships a latent gap.

### 1. Action CAM — Edit action. Applies in full.

| Obligation | How it is satisfied |
|---|---|
| Menu-bar item (Edit menu) | A `Cmd` row in `EDIT_CMDS`, `is_toggle: true` — the menu item renders checkable automatically |
| Toolbar section | The same row; `toolbar.rs`'s edit-section loop already renders an `is_toggle` row as a `GtkToggleButton` |
| Context menu | The same row; `contextmenu.rs` builds its rows from `EDIT_CMDS`. **See the caveat** |
| Accelerator surfaced everywhere it is mirrored | Vacuous — no accelerator |
| Single `GAction` source of truth | One stateful boolean `SimpleAction` `win.spell-check`; every surface binds it by name |
| Consistent enabled/sensitivity | One rule in `window/actions.rs` alongside its siblings |

**The context-menu cell has a wrinkle that must not be papered over.** That menu
must carry *two different things*:

- the **toggle** (the `EDIT_CMDS` row — free, it comes with the table); and
- the **suggestions** for the word under the pointer — dynamic, variable in
  number, and not a command in any table.

Only the first is delivered by adding a table row. The second is new machinery in
`contextmenu.rs` and is the bulk of the UI work.

**The "toggleable action" sub-matrix does NOT apply.** CAM.md scopes "toggleable"
to an action that *applies-or-removes a markup* (bold, heading, quote) and
requires reverse-on-reapply, applied-state detection, and empty-selection
behaviour. `win.spell-check` is a **stateful mode toggle** — the
`win.auto-reload` / `win.allow-outside-links` shape. It mutates no document text
and produces no markup to detect. The word "toggle" appearing in both is a
collision of vocabulary, not of obligation. Recorded so nobody hunts for a
reverse-on-reapply test that should not exist.

### 2. Reading-Position Preservation CAM — does NOT apply. Measured, not assumed.

Requirement 2 means enabling the checker tags the **entire buffer** in one pass,
which is the largest possible re-tag — and that matrix exists because the
viewport silently jumps toward the top when `upper` transiently shrinks during
GTK's lazy line-height re-validation.

**P4 measured a delta of exactly 0.00 in both `value` and `upper`**, for both
apply and remove over a whole 800-line wrapped buffer. An underline changes no
metrics, and on 4.6.9 the tag application does not trigger the re-validation that
causes the clamp.

This is a *measured* "does not apply" with a reproducible probe, not an
intuition — which is the standard this matrix demands, since a wrong answer here
ships a viewport jump on an event nobody thought was scroll-perturbing.

**One residual risk this does NOT cover, and the implementation must respect:**
the initial full-buffer pass on a **freshly loaded, cold** view could run
concurrently with session-restore's scroll restore, which *is* a
`notify::upper`-driven progressive restore (ScrAP-115). P4 measured a *warm,
settled* view. The mitigation is ordering, not tagging: **do not start the first
spell pass until the restore has converged.** Verify this specific interaction
during implementation rather than assuming P4 covers it — it does not.

### 3. Derived-view CAM — applies by analogy, and is where the real latent gap lives.

The set of misspelled ranges is a **projection of document content** — recomputed
from the text, holding no truth of its own, able to disagree with what the user
sees without the user being able to tell. That is the Derived-view CAM's
definition almost word for word. It differs from existing rows only in *where* it
is displayed: in the document pane rather than a mirror surface.

That difference does not exempt it. The four event classes map straight on:

| Class | Event | What goes stale |
|---|---|---|
| **A** in-session mutation | typing, a format action, **undo/redo**, task-checkbox toggle | Tags over edited text; undo especially, since it rewrites ranges without a normal edit gesture |
| **B** persistence event | **external reload** (live and prompted), open, new | The buffer is **replaced** — every tag on it is gone and the underlines simply vanish |
| **C** in-place view rebuild | theme switch, zoom, view-mode switch, live-preview re-render | Tags live on the buffer (P1), so they survive a *view* rebuild — but a buffer swap is class B |
| **D** host change | **tab switch**, tab close, cross-window move / pop-out, session restore, deferred background-tab materialise | A tab materialised in the background has never been checked |

**Requirement 2 must not be mistaken for the answer to any of these.** "Toggling
always refreshes" gives the user a manual way to force a re-check — but CAM.md
states plainly that *"it corrects itself on the next tab switch / mode switch /
reload" is a **fail**, not a mitigation*, and "the user toggles it off and on
again" is a worse version of that excuse. Requirement 2 is a **guarantee about
the toggle**, not a licence for the other events to be stale.

So this feature needs **one choke point** — `refresh_spelling(tab)`, in the shape
of `refresh_outline` / `refresh_annotations` — that every event above calls, with
the toggle as merely one caller. A new Derived-view CAM row naming that choke
point is part of the deliverable.

The CAM's one legitimate deferral applies: a **background tab** may carry
unchecked text provided it is checked on activation, via the existing
`materialize_deferred_preview` path. Nothing on screen may be stale.

### 4. Document Rendering CAM — does NOT apply.

It governs "how document **markup** is rendered in the **preview**". A spelling
underline is neither markup (not in the document, does not round-trip through
save/reload, no source syntax) nor preview. Its container-context, copy-fidelity
and undo-atomicity rows are all meaningless for a decoration that is not document
content.

**Operator's ruling, and the sharper form of the reason: the feature applies only
to the editor pane, because that is the pane where a correction can actually be
made.** That is a stronger boundary than "the checker happens to run on the
editor". The preview is read-only, so a misspelling shown there would be a defect
the user cannot act on — the underline would be an accusation with no remedy.
Spell-checking the preview is therefore **out of scope by design, not by
omission**, and a future reader should not "complete" the feature by extending it
there.

**Two of its concerns are imported anyway**, being real here even though the
matrix is not:

- **Theme sourcing** (row 9 / POLICY "no hard-coded styling"): resolved by *not
  setting a colour at all* — see [Styling](#styling-the-underline).
- **Zoom** (row 13): **not applicable — verified, not assumed.** Zoom never
  reaches the editor pane, on two independent counts: `apply_zoom` returns early
  when there is no preview scroller (`// edit mode: nothing to zoom`), and its CSS
  rule is selector-scoped to `textview.scrib-preview`, a class the editor's
  `sourceview::View` does not carry. So there is no zoom level at which the
  editor's type scale changes, and the underline has nothing to track.

  One nuance worth recording so the Reading-Position CAM's row 1 is not misread as
  contradicting this: that row marks the **Editor** column ✓ for zoom, but that is
  about *preserving the editor's scroll* — in split mode the preview's
  post-re-render validation can drag the editor through scroll-sync (which is why
  `rerender_and_restore_scroll` forces the editor as sync driver before the swap).
  It is not a claim that zoom rescales the editor. Both statements are true.

## Approach

### Decision: pure-Rust `spellbook` + vendored Hunspell dictionaries selected by UI locale, driving our own tags and menu.

The options, and why the others lose:

| # | Option | Verdict |
|---|---|---|
| 1 | **`libspelling`** (GNOME/GtkSourceView-5) | ❌ **Unusable.** Floor is gtk4 **≥ 4.8** / gtksourceview **≥ 5.6** even at its oldest release (0.2.1); main needs **≥ 4.15.5** / **≥ 5.10**. Our compile surface is **4.6**. Also **no maintained crates.io binding** — we would own gir/sys — plus a Windows/macOS packaging tax. See [What upgrading to 4.8 would cost](#what-upgrading-to-48-would-cost) |
| 2 | **`gspell`** | ❌ GTK3-era; its GTK4 successor *is* libspelling |
| 3 | **`spellbook` (pure Rust)** | ✅ **Chosen.** 0.4.2, released 2026-06-03, MPL-2.0, used by Helix, reads Hunspell `.aff`/`.dic`, **produces suggestions**. No C dependency for the ports to vendor |
| 3b | `zspell` | ❌ Suggestions behind an `unstable-suggestions` feature; last release 2024-06 |
| 4 | **`enchant`/`hunspell` FFI** | ❌ `hunspell-rs` last released 2022-09; C dependency cost without the integration benefit; an OS dictionary matrix to support |
| 5 | Defer | ❌ Recorded so it is not re-proposed |

### What upgrading to 4.8 would cost

The operator asked what the impact of moving the baseline to GTK 4.8 would be, so
that libspelling 0.2.1 comes into range. Answered with facts rather than
impressions, because it is a reasonable question with an unobvious answer.

**It is not one upgrade, it is two.** libspelling 0.2.1 needs gtk4 ≥ 4.8 **and
gtksourceview ≥ 5.6**. This machine has GtkSourceView **5.4.1**. So the
GtkSourceView floor binds independently of the GTK one, and both would have to
move.

**Neither is available from the distribution.** This machine is Ubuntu **22.04.5
LTS**, and `apt-cache policy` reports the candidate versions as *identical to the
installed ones* — `libgtk-4-1` 4.6.9, `libgtksourceview-5-0` 5.4.1. There is no
newer package to upgrade to. Moving the baseline therefore means leaving the LTS's
packages entirely — a PPA, a Flatpak runtime, or a source build — **not only on
this development machine but on every Linux user's machine**. That converts a
library choice into a distribution-support decision.

**The compile-surface change itself is trivial and is not the cost.** Enabling
gtk4-rs's `v4_8` feature is a one-line `Cargo.toml` edit. What it buys is API; what
it costs is that the binary no longer runs against a 4.6 runtime — which is what
the current LTS ships.

**The dominant cost is unchanged by any version bump: there is no maintained
crates.io binding for libspelling.** We would own the gir/sys generation and its
ongoing maintenance, for a C library, across three platforms — on top of the
macOS/Windows packaging work. That cost is identical at 4.8 and at 4.22, so
upgrading does not reduce it.

**And 0.2.1 is the *old* release.** libspelling's main branch has already moved to
gtk4 ≥ 4.15.5 / gtksourceview ≥ 5.10. Adopting 0.2.1 means adopting a superseded
version of an actively moving library and facing this same question again on the
next bump.

**The decisive point, though, is that P1 removed the reason to want it.** The one
thing libspelling offered that we could not easily do ourselves was cooperating
with the syntax-highlighting engine — and P1 measured that **no cooperation is
needed**: our tag survives every re-highlight path. What remains of libspelling's
value is a dictionary and suggestions, which `spellbook` supplies as a pure-Rust
crate with no portability cost at all.

**Conclusion: the upgrade is expensive, it is a project-wide decision with a blast
radius far beyond this feature (every anti-pattern entry keyed to 4.6 behaviour,
both ports' assumptions, and the supported-distribution story), and it would buy
something we have measured that we do not need.** If the baseline is ever raised,
it should be for reasons of its own — not to enable a spell checker that does not
require it.

**Why the DIY route is cheaper here than it would be for a typical app**, which
is what makes this an easy call rather than a compromise: we already hand-roll
the context menu (a `GtkPopover` of `GtkButton` rows rebuilt per right-click, with
its own `GtkStack` submenu machinery), we already own a `GtkTextTag` set and a
theme engine, and **P3 means we must own word iteration regardless**. An
integrated library would be supplying dictionary + suggestions + iteration; we
only actually want the first two, and P1 removed the one thing it could have
supplied that we can't easily do ourselves.

### Where the enabled flag lives

**Application preference, persisted** (`config.toml` via `config.rs`), applied to
every window's editor pane — the GNOME convention per the research answer, and
not per-document for v1.

This choice has a consequence worth stating: because it is app-scoped, **every
open window's action state must follow a change**, in the shape of the existing
`re_render_all_windows` fan-out. It also keeps `session.rs` and therefore
`SCHEMA.md` untouched, which a per-tab flag would not.

### What counts as a word

Two layers, because P2 and P3 each rule out doing it with one:

1. **Skip-regions — `no-spell-check` first, our own supplement second.** Use
   `iter_has_context_class(iter, "no-spell-check")` as the primary filter: P2
   shows it already covers inline code, fenced blocks, link destinations and
   image paths, and correctly does *not* cover link text. Then close P2's two
   measured gaps — **bare/autolink URLs** and **HTML attribute values** — with a
   narrow supplement. A full `pulldown-cmark` skip-region derivation is **not**
   needed for v1 and should not be built speculatively; the gaps are specific and
   small.
2. **Tokenization — ours, not GTK's.** P3 rules out `GtkTextIter`'s word API.
   Treat `'` and `-` as word-internal; unit-test the boundary table directly
   (`don't`, `well-known`, `rock'n'roll`, an em-dash, a trailing possessive
   `students'`). This is display-free logic and belongs in a pure module, tested
   without a display.

### Styling the underline

`GtkTextTag` with `underline = PangoUnderline::Error`, and **deliberately no
`underline-rgba`**. This satisfies POLICY's no-hard-coded-styling rule by
*setting no colour at all* rather than by sourcing one: GTK draws the error
underline itself. There is **no standard `GtkSourceStyleScheme` "spelling" style**
to own it, so the style scheme stays responsible for syntax colours only.

Worth noting plainly: this means the underline colour is **not themeable by us**,
by design. That is the correct trade — a literal would violate policy, and
inventing a theme key for something GTK already draws would be a second source of
truth.

### Scheduling and debouncing

Never apply tags inside the mutation signal itself — a trap this project has
already been bitten by. Everything below runs *after* the edit, off a timer.

**Reuse the existing debounce; do not introduce a second timer.**
`window/livepreview.rs` already owns a **300 ms** cancel-and-reschedule debounce
on the editor buffer's `changed` signal, and it already fans out to
`refresh_outline` and `refresh_annotations`. Spelling has the **same trigger**
(the buffer changed) and the **same gates** (editor-visible mode; suppressed while
`st.loading` is set for a programmatic buffer replacement), so `refresh_spelling`
belongs in that same callback rather than in a parallel timer racing it on every
keystroke.

Inherit its two hard-won details rather than rediscovering them:

- Resolve the tab through its **`content_box`**, never a captured `window` — a
  captured window keeps operating on the *origin* window's active tab after a
  cross-window tab move (QA round-1 H2), which would silently spell-check the
  wrong document.
- **Re-check the mode inside the timeout.** The user can leave editor-visible
  modes during the 300 ms.

**Debounce interval is the wrong tool for the real problem.** The thing that makes
a spell checker feel broken is not latency, it is **flagging the word the user is
currently typing** — a half-typed word is always misspelled, so underlines flash
under the caret on almost every word. No debounce fixes that; a longer one just
delays the flicker. The correct rule is a **caret-word exclusion**: never mark the
word the insertion point is currently inside.

**That exclusion creates the obligation everyone forgets:** the skipped word must
be checked once the caret *leaves* it, which is **not a buffer change**. So there
is a second trigger — the caret moving (`notify::cursor-position` / `mark-set`) —
and without it a genuinely misspelled word stays permanently unflagged simply
because it was the last one typed. Re-check the *previously* excluded word on
caret move; this needs no debounce of its own, as it is one word.

**Scan the dirty region, not the document.** Track the affected offset range from
`insert-text` / `delete-range`, expand it outward to whole word (and, for safety
against a `no-spell-check` region boundary shifting, whole line) boundaries, and
rescan only that. Rescanning the whole buffer on each debounce is O(document) per
typing burst and will be felt on a large file.

**The full pass is a different path with different rules.** Enabling the toggle,
and every class-B/D event in the Derived-view matrix, needs a whole-buffer scan.
That one must be **chunked across idles** so the first pass on a large document
cannot stall the main loop, and it must **supersede any pending incremental
rescan** rather than running alongside it. Disabling is the cheap direction: one
`remove_tag` over the whole buffer, which P4 measured as viewport-neutral.

Ordering constraint, restated from the CAM analysis because it belongs here too:
the initial full pass on a freshly loaded view must not race session-restore's
progressive scroll restore.

## Further considerations

Concerns that are neither the mechanism nor a CAM cell, but each of which can
sink the feature or violate a project rule if left to implementation time.

### CAM exception — GRANTED

The **"Correct to *word*" rows cannot satisfy the Action CAM**: its cells demand a
menu-bar item and a toolbar button, and a correction command can have neither — it
is generated per right-click and its label *is* its argument, so there is no stable
command to place on either surface. Those two cells are not inconvenient here,
they are meaningless.

**Approved by the operator at plan time and recorded in CAM.md's Granted CAM
exceptions list.** Scope: the dynamic correction rows only. The `win.spell-check`
toggle is an ordinary Edit action and satisfies every cell normally.

### Undo atomicity — now a CAM obligation, not just a note

A correction is a programmatic buffer edit, and this project has already been
bitten here: `undo()`/`redo()` leave **no** undo barrier, so the next programmatic
edit merges into the redone group (ScrAP-110). Without a barrier, undoing a
correction can swallow an unrelated preceding edit.

Flush an **empty `begin_user_action` / `end_user_action` pair** before applying the
replacement, then wrap the replacement in its own user action, so it is exactly one
undo step.

**Per the operator's ruling this is a CAM concern, and CAM.md has been amended
accordingly** — the Action CAMs matrix gained an *"Atomically undoable"* invariant
row, scoped to commands that mutate the document buffer. This closed a real gap in
the matrices rather than only serving this feature: undo atomicity was previously
expressed **only** as Document Rendering CAM row 4, which binds on markup/rendering
features, so every *non-markup* buffer-mutating command — a spelling correction
being exactly one — had no matrix carrying the obligation at all. The two rows are
cross-referenced so they cannot drift.

### What the correction targets — pointer, not caret, and never the selection

The suggestions are computed for the word **under the pointer**, but the
replacement must be applied to *that* word, not to wherever the caret happens to
be, and it must not disturb an existing selection elsewhere in the buffer. Three
specific hazards:

- **Right-click does not necessarily move the insertion point.** Do not assume
  caret == click target; carry the word's offsets from the click through to the
  replacement.
- **Anchor by offsets captured at click time, but re-validate before applying** —
  the popover is non-modal and the debounced re-check can run while it is open.
  Applying stale offsets after a buffer mutation corrupts text.
- **PRIMARY-selection hazards are live here** (the AP-120/ScrAP family this project
  has already hit): a selection change can be perturbed by unrelated machinery.
  Replacing a range while a selection exists elsewhere must not clobber it.

### Dictionary lifecycle — bound to the toggle state (operator decision)

**The dictionary's lifetime tracks the toggle, not first use.** Enabling loads it;
disabling **unloads** it. This was chosen over lazy-on-first-need, and it is the
better rule for a reason worth recording: it makes the resident cost **exactly
predictable from a state the user controls and can see**. Lazy loading would mean
the feature is "off" but the memory is still held, which is the kind of divergence
between visible state and actual cost that is impossible to reason about later.

It also composes cleanly with requirement 2: *toggling always refreshes the state*
becomes literally true at every level — enable ⇒ load dictionary, scan whole
buffer; disable ⇒ clear tags, drop dictionary. There is no cached middle state to
go stale.

- **At startup**, the persisted preference decides: on ⇒ load during
  initialisation; off ⇒ never touched, and a user who leaves it off pays nothing.
- **One process-wide instance**, resolved like `Config` — never per tab or per
  window, or resident memory multiplies by the tab count for zero benefit.
- **Measure against the footprint gate.** A dictionary is a "significant change"
  in the TDD §6 sense; record the RSS delta in both states rather than assuming it
  is small.
- **The unload must actually free.** Dropping the last reference is the point; if
  the dictionary ends up behind a `OnceCell`-style singleton it cannot be unloaded
  and the decision above is silently reversed. Choose the container accordingly.

### Toggle sensitivity — decided by the preference ruling

**The toggle is an application-wide preference** (operator decision), and that
settles the rule that was open:

- **It stays sensitive in preview-only mode.** A preference is something the user
  sets ahead of time; greying it out because no editor happens to be visible right
  now would be treating it as a mode. It has effect the moment an editor is shown,
  which is exactly what a preference should do.
- **It goes insensitive only when no dictionary is available** for the resolved
  locale, with the reason discoverable in a tooltip. An enabled toggle that cannot
  do anything is worse than a disabled one.

Being app-wide also means a change must propagate to **every open window**, in the
shape of the existing `re_render_all_windows` fan-out — one preference, every
window's action state and every visible editor following it immediately. A window
that keeps the old state until it is next touched is the Derived-view CAM's
"self-healing is a fail" case.

### Accessibility — the underline is a visual-only signal

A misspelling marked solely by an error underline conveys nothing to a screen
reader; the information is carried entirely in colour and decoration. At minimum
this should be acknowledged rather than silently shipped, and the context menu's
suggestion rows must be reachable and labelled for keyboard and AT users — they
are the actual remedy path, and they are the part that *can* be made accessible
cheaply.

### Ignore list / personal dictionary — persistent, in the user's home

**Operator decision: it persists to the user's home directory.** So this is not a
session-scoped convenience; it is durable user data, and that carries obligations
the rest of the feature does not have.

**The two commands, and why they can share one file.** "Add to Dictionary" (this
is a real word) and "Ignore All" (this is not a word but leave it alone here) are
distinct in intent but identical in storage: both are a set of strings that
suppress a marking. Ship them as one persisted set unless a reason to separate
them appears; two files for one concept is the more expensive mistake.

**Location — use `glib::user_data_dir()`. Two traps are avoided by that one
choice, and the obvious alternatives hit both.**

- **Never `glib::user_config_dir()`.** This process **redirects
  `XDG_CONFIG_HOME`** at startup to survive the GTK 4.6 XCompose crash (ScrAP-3),
  and GLib reads the environment **exactly once and caches it** — so that call
  returns the *redirected throwaway* directory. The ignore list would be written
  somewhere it can never be read back from, and would silently vanish between
  runs.
- **Not `config::user_config_dir()` either**, despite it being this project's
  redirect-safe accessor. It is deliberately **XDG-only** — `XDG_CONFIG_HOME`,
  falling back to `$HOME/.config`, with no platform branches — so on **Windows**,
  where `HOME` is typically unset, it returns `None` and the feature would
  silently have nowhere to persist.
- **`glib::user_data_dir()` is correct on both counts.** The redirect touches
  `XDG_CONFIG_HOME` only — `config.rs`'s own comment states that `XDG_DATA_HOME`
  is untouched and that `glib::user_data_dir()` "and friends are fine" — and GLib
  maps the data directory to the right platform location on Windows and macOS.
  It is also the semantically correct home: an ignore list is user **data**, not
  configuration.

**Writing.** Use `atomic_io::write_atomic` (write-temp-then-rename) like every
other persisted file here, so a crash mid-write cannot truncate the list. Note
this brings the self-delete guard interaction into view if the file is ever
watched — it is not today, so do not add a monitor for it.

**Schema.** It crosses the application's boundary and is user-editable in
principle, so it needs a **`SCHEMA.md` entry** giving its exact shape. Keep the
format boring — one entry per line, or a small TOML table — and define the answers
that a register always needs: is matching **case-sensitive**? Is the list
**per-language** (an ignored word in an English document should probably not be
ignored in a French one)? What happens to an entry whose language is no longer
installed? Deciding these in SCHEMA.md now is far cheaper than migrating a file
format later.

**Scope interaction with the toggle.** The list is app-wide, like the preference —
but it must survive the dictionary being unloaded when the feature is switched
off. The ignore list is *user data*; the dictionary is a *resource*. Do not couple
their lifetimes.

### Where each obligation is tested

Worth settling now, because it changes how the code is factored:

- **Pure, no display**: the tokenizer and its boundary table, the skip-region
  supplement (bare URL / HTML attribute), suggestion ranking. These belong in a
  display-free module, unit-tested directly — the same discipline as `format/` and
  `outline.rs`.
- **`#[gtk::test]`, headless**: tag application over ranges, the `no-spell-check`
  filter against a real `GtkSourceBuffer`, the choke point firing on each event
  class, undo-step atomicity.
- **`tests/MANUAL-TEST.md`**: everything with a pointer in it — the context menu's
  suggestion rows, the degenerate click positions, and the toggle's visible
  refresh. Note that `PLAN.lib-bin-split.md` records why integration tests cannot
  reach crate internals; keep the pure layer reachable from in-crate unit tests
  rather than planning an integration test that cannot be written.

## Deliverables checklist

- [ ] One stateful `win.spell-check` action; one `EDIT_CMDS` row (`is_toggle: true`,
      `accel: ""`); one `MENU_MNEMONICS` entry with an Edit-unique access letter;
      one `Icon` enum variant (symbolic).
- [ ] `config.toml` key + `config.rs` plumbing; all open windows follow a change.
- [ ] One `refresh_spelling` choke point called by **every** event class A–D —
      not only by the toggle.
- [ ] Pure tokenizer owning `'` and `-`, unit-tested against a boundary table.
- [ ] Skip-regions: `no-spell-check` context class **plus** the bare-URL and
      HTML-attribute supplement.
- [ ] Dictionary resolved from the **UI locale** (exact tag → base language →
      `en_US` → feature unavailable, toggle insensitive). Never grade one language
      with another's dictionary.
- [ ] `spellbook` + vendored `.aff`/`.dic` for the shipped locale set. **Verify each
      dictionary's own
      licence** — it is not the crate's licence — and measure resident memory
      against the footprint gate.
- [ ] Dynamic suggestion rows **inline at the top** of the context menu (≈3–5,
      the GNOME convention), above the static `EDIT_CMDS` rows, with a separator;
      "no word here" renders no spelling section at all.
- [ ] Degenerate click positions yield no spelling section: past end-of-line, in
      the gutter, on whitespace, inside a `no-spell-check` region.
- [ ] Coordinates: `window_to_buffer_coords(TextWindowType::Widget, x, y)` from
      the capture-phase gesture — **`Widget`**, not `Text`, or the mapping is off
      by the gutter width near the left margin.
- [ ] Ordering guard: the first full pass must not race session-restore's
      progressive scroll restore (see CAM §2's residual risk).
- [ ] `refresh_spelling` hangs off the **existing** 300 ms live-preview debounce
      (same trigger, same gates) — no second timer; inherit its `content_box`
      resolution and its in-timeout mode re-check.
- [ ] **Caret-word exclusion**: never flag the word the insertion point is inside,
      **and** re-check that word when the caret leaves it (a caret move, not a
      buffer change — the trigger that is easy to omit and leaves a misspelling
      permanently unflagged).
- [ ] Dirty-region rescan (expanded to word/line bounds), not a whole-buffer
      rescan per keystroke burst; the full pass is chunked across idles and
      supersedes any pending incremental one.
- [x] **CAM exception granted** for the dynamic correction rows and recorded in
      CAM.md's Granted CAM exceptions list.
- [x] **CAM.md amended** with an *"Atomically undoable"* invariant row on the
      Action CAMs matrix, cross-referenced to Document Rendering CAM row 4.
- [ ] Empty `begin/end_user_action` barrier before applying a correction
      (ScrAP-110); the replacement is exactly one undo step.
- [ ] Dictionary lifetime **bound to the toggle**: enable ⇒ load, disable ⇒ unload
      (and the container must actually permit the drop — not a `OnceCell`).
- [ ] Ignore list / personal dictionary persisted under **`glib::user_data_dir()`**
      — not `glib::user_config_dir()` (ScrAP-3 redirect) and not
      `config::user_config_dir()` (XDG-only, `None` on Windows) — written via
      `atomic_io`, with a `SCHEMA.md` entry defining case-sensitivity and
      per-language scope.
- [ ] Dictionaries **embedded in the GResource**, not looked up in system paths.
- [ ] Locale resolved via **`glib::language_names()`**, not `LANG`.
- [ ] Preference change fans out to **every open window** immediately.
- [ ] Correction applies to the **pointer's** word by offsets captured at click
      time and re-validated before use; no selection clobbering.
- [ ] Dictionary loaded **lazily**, one process-wide instance; RSS delta measured
      against the footprint gate.
- [ ] Toggle sensitivity rule decided and applied uniformly (preview-only mode; no
      dictionary for the locale).
- [ ] Suggestion rows keyboard-reachable and labelled; the visual-only nature of
      the underline acknowledged.
- [ ] A Derived-view CAM row naming the choke point. **No** Reading-Position row —
      P4 measured it out.
- [ ] TDD rubrics proposed to the operator **before** implementation (SDD
      plan-kickoff rule), including one for requirement 2 that asserts the
      *refresh*, not merely the flag flip.
- [ ] `tests/MANUAL-TEST.md` checks derived from each applicable CAM cell.
- [ ] P1 / P2 / P4 land as headless `#[gtk::test]`s beside the feature, so the
      ports verify them by running the suite rather than by hand.

## Will this work on Windows and macOS?

Assessed per component rather than as one verdict, because the risk is not evenly
distributed. Both ports build the same tree against **GTK 4.22 / GtkSourceView
5.x** (macOS/Quartz, Windows/MSVC via gvsbuild).

| Component | Portable? | Why |
|---|---|---|
| **`spellbook`** | ✅ **No risk** | Pure Rust, no C dependency, nothing for the ports to vendor. This is the single biggest reason the DIY route beat libspelling |
| **Our tokenizer** | ✅ **No risk** | Pure Rust and display-free. Note this makes P3's refutation *immunising* — if GTK's word iteration differs by platform or locale, we are not using it |
| **`GtkTextTag` + `underline = Error`** | ✅ Core GTK, stable API since 4.0 | |
| **P5 (tagging doesn't dirty)** | ✅ Core GTK semantics | And our `is_dirty()` is a text comparison, which is platform-independent by construction |
| **P1 (foreign tag survives re-highlight)** | 🟡 **Very likely** | The engine code is the same across the 5.x line, and the researcher read **5.10** while I measured **5.4.1** — the two agree. Worth confirming, not worth worrying about |
| **P4 (viewport unmoved)** | 🟡 **Very likely** | Core `GtkTextView` behaviour, but it is exactly the class of thing that has differed by backend before |
| **P2 (`no-spell-check` coverage)** | ⚠️ **Genuinely unverified** | The classification comes from the `markdown.lang` definition shipped *with GtkSourceView*, and the ports run a **different version**. A newer definition could classify bare URLs or HTML differently — in either direction |
| **Dictionary files** | ⚠️ **Design decision, easily got wrong** | See below |
| **Locale resolution** | ⚠️ **Design decision, easily got wrong** | See below |
| **Ignore-list path** | ⚠️ **Design decision, already corrected** | See below |

### The three portability decisions that matter more than the GTK questions

**1. Embed the dictionaries in the GResource — do not look them up on the system.**
Reading `/usr/share/hunspell` (or any system dictionary path) is a Linux-only
solution that would leave both ports with no dictionary and a feature that is
permanently insensitive. This tree already compiles a GResource in `build.rs`, so
embedding is a proven, fully portable route: the dictionary ships inside the
binary and is identical on all three platforms. It also makes the footprint
predictable and removes "is a dictionary installed?" from the support matrix.

**2. Resolve the locale with `glib::language_names()`, not `LANG`.** Reading the
`LANG` environment variable is the reflex, and it is Linux-shaped: Windows does not
set it, so locale resolution would silently fall through to the `en_US` default
for every Windows user. GLib already abstracts the platform differences —
use it.

**3. The ignore-list path — `glib::user_data_dir()`.** This was corrected *because*
of this cross-platform review. The two natural choices are both wrong:
`glib::user_config_dir()` returns the ScrAP-3 redirect target, and this project's
own `config::user_config_dir()` is deliberately XDG-only and returns `None` on
Windows. `glib::user_data_dir()` avoids the redirect (which touches
`XDG_CONFIG_HOME` only) and maps correctly on all three platforms.

### What the ports should actually do

**Nothing, until implementation — and then nothing special.** P1, P2 and P4 ship
as headless `#[gtk::test]`s beside the feature, so the ports verify them simply by
running the suite on their own GTK 4.22 / GtkSourceView 5.x. That is deliberate:
asking a port to remember to run a throwaway probe is a check that gets skipped,
and it would have to be re-run on every GtkSourceView bump anyway.

**P2 is the one where a differing result is plausible.** If the coverage table
differs on the newer language definition, the skip-region supplement changes size —
the design does not. When it happens, record the per-version answer in this plan's
table rather than replacing the 5.4.1 row; knowing *which* versions differ is the
useful part.

## Open questions

1. **Suggestion count** — 3–5 is the convention; pick one. *(The only question
   still genuinely open.)*
2. **"~~Add to Dictionary~~" / "~~Ignore All~~"** — **decided: in scope, and
   persistent to the user's home directory.** Design recorded under
   [Ignore list / personal dictionary](#ignore-list--personal-dictionary--persistent-in-the-users-home).
   The residual sub-decisions are format-level (case-sensitivity, per-language
   scope) and belong in `SCHEMA.md` at implementation time, not here.
3. **~~Language selection~~** — **decided: the UI locale**, not a hardcoded
   `en_US`. Resolve the dictionary from the locale at startup, with a documented
   fallback chain when no dictionary is installed for it: exact tag (`en_GB`) →
   base language (`en`) → `en_US` → **feature silently unavailable** (the toggle
   goes insensitive rather than the checker underlining everything). Vendoring one
   dictionary while resolving by locale means a `fr_FR` user gets *no* checker
   rather than an English one grading French — the failure has to be absence, not
   nonsense. **This makes "which dictionaries do we ship?" a real packaging
   question** (size vs coverage), so it is called out here rather than buried.
4. **~~Zoom interaction~~** — **resolved: not applicable**, and your reasoning
   holds with a stronger basis than the preview-pane one. Verified two independent
   ways: `apply_zoom` early-returns when there is no preview scroller, and the zoom
   CSS is selector-scoped to `textview.scrib-preview`, which the editor's
   `sourceview::View` does not carry. Zoom cannot change the editor's type scale at
   all, so there is nothing for the underline to track. (The Reading-Position CAM's
   ✓ in row 1's Editor column is about preserving the editor's *scroll* across a
   split-mode re-render, not about rescaling it — not a contradiction.)
