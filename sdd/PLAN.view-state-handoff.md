# Plan: View state handoff — what a view adopts when it is built, switched to, or re-entered

## Problem

Three separate reported defects are one design gap seen from three angles: **a view
takes its state from whatever happened to reach it, rather than from an
authoritative source it consults when it comes into existence.** Each is
independently user-visible, each is measured as pre-existing (reproduced on a binary
built at the current merge-base), and each is currently the highest severity band in
the debt register.

| Symptom | Surface |
|---|---|
| A live reading-theme change never reaches a tab the user has not activated yet — the page fill takes the new theme, the body ink and typeface do not, and it does not self-heal on a tab switch. Only a second theme change repairs it | Preview, background tab |
| The caret sits on the document's **last** line the first time the editor view is materialised, and the outline sidebar highlights the last heading to match — so the sidebar actively misreports the reading position | Editor, first entry |
| The reading position moves on every view-mode round trip, **cumulatively and without bound** — measured below, four preview↔split trips walk the reader from the middle of a document to its end, where it clamps. The edit round trip drifts the other way | Both panes, every mode switch |

Fixing them one at a time invites three different answers to the same question, which is how a
state-handoff bug becomes four. **Measurement has since re-cut them**: the caret and the
reading position are one missing mechanism and are batch 1; the theme fan-out does not
reproduce headlessly at all and is held for a live-display reproduction. See *Root cause*.

### Root cause

**The caret — MEASURED, and it is one mechanism, not two.** `window::actions::load_into_editor`
ends in `buf.set_text(&text)`, and `gtk_text_buffer_set_text` leaves the insert mark at the
**end** of the inserted text. A 27-line document therefore reports `Ln 28, Col 1` the moment
the buffer is filled — the reported figure exactly. Measured headlessly on both the
`sourceview::Buffer` the editor uses and a plain `gtk::TextBuffer`, which gives the same
answer, so nothing GtkSourceView adds is involved:

```
after load_into_editor:   cursor-position=207  line=27 (0-based)  line_count=28
plain GtkTextBuffer:      cursor-position=207  line=27
```

This retires the allocation explanation entirely. No `line_at_y` runs, no unallocated view is
queried, and ScrAP-263's seam is not on this path — the caret is simply never placed after the
load. It also dissolves the "warm path needs its own account" question that made this look
like two defects: on a warm round trip nothing *moves* the caret to the end, it has been there
since the document loaded and no mode switch ever displaces it. One placement fixes both.

**The reading position — reproduced headlessly, and worse than a block.**
`window::viewactions`'s `view-mode` handler captures `content_scroll_fraction`, and
`window::scrollsync::apply_content_scroll` replays that fraction into whichever panes the new
mode shows. A fraction is a ratio of view-specific content heights, and the preview is rebuilt
from scratch on every entry (`st.split.set_preview`), so precision is lost at both ends of
every trip. Driven over a 40-section fixture at 800×600, parked at the middle, four
preview↔split round trips:

| | start | trip 1 | trip 2 | trip 3 | trip 4 |
|---|---|---|---|---|---|
| top line | 79 | 110 | 152 | 158 | 158 |
| adjustment | 1827 | 2528 | 3173 | 3173 | 3620 |

Monotonic, accumulating, and it terminates by **clamping at the end of the document**
(`upper` = 3654) — so the recorded "one block per trip" understates it: four round trips walk
the reader from the middle of the document to its end. Fails TDD 7.5's "approximately the same
relative position", and takes 12.13 with it.

**The theme fan-out — the prime suspect was REFUTED, and the symptom does not reproduce
headlessly at all.** The suspect was that `window::zoom::rerender_tab_preview_in_place` returns
silently when a tab has no preview scroller, so a never-activated tab is skipped by
`app::setup::re_render_all_windows`. The skip is real — a deferred tab genuinely has no preview
at the moment of the sweep — but it is **harmless**, because that tab still carries
`needs_render`, and `window::tabs::switch::materialize_deferred_preview` then renders it
against the *current* theme when it is activated. Both background-tab shapes were driven under
Xvfb and both end up correct (Sepia's `#5b4636` ink and its Charter stack, identical to the
active tab):

| Background tab at the moment of the switch | Ink after | Face after |
|---|---|---|
| deferred, never activated, then activated | correct | correct |
| pre-rendered by the prerender pump, never activated | correct | correct |

**It DOES reproduce on the live display, and the ink is not what was filed.** Driven on the
operator's real session (KDE/X11, `DISPLAY=:0`, release build, two documents in one window,
second never activated, System → Sepia): tab 2 took Sepia's cream page fill and kept the
**dark desktop theme's near-white ink**, leaving the body text all but invisible on the light
page. The register recorded "pure black"; the truth is that the ink is whatever the PREVIOUS
theme's was — black when switching away from a light theme, white when switching away from a
dark one — which is a more useful statement of the same fault and points at the same place:
the fill moved and the ink did not. A second theme change (Sepia → Candy) repairs the tab
completely, headings, ink and all, confirming that half of the report too.

So the mechanism is not the one the code reading suggested, and a headless harness cannot see
this defect at all. Body ink and page fill are *both* CSS, emitted into one provider by
`preview::css::theme_css` onto two nodes of the same widget (`textview.scrib-preview` for
`color`/`font-family`, `> text` for `background-color`), which is why "fill new, ink old" is
hard to explain from the app's own state: one `load_from_data` re-cascades both. The remaining
candidates all sit outside what Xvfb reproduces — a real compositor/WM difference (the
GTK4Rs/AP-56 class), or a state the operator's session carries that a fresh test window does
not. **This half needs a live-display reproduction before any fix is designed.**

## Possible approaches

### ✅ 1. One authoritative reading position, resolved as a document position — **BATCH 1, LANDED**

Stop round-tripping a scroll fraction. Hold the reading position on the tab as a document
position (a source line, or a mapped preview/editor pair via `preview::sourcemap`), write it
once when a view is left, and have each view resolve it to its own geometry when entered — and
place the caret from that same position after a load, rather than leaving it wherever
`set_text` parked it.

**Landed** as `readingpos::DocPosition` (`src/readingpos.rs`), with
`window::scrollsync`'s `content_reading_position` / `apply_content_reading_position` carrying
it across a view-mode switch and `project_scroll` re-routed through the same conversions, so
the per-frame split projection and the mode hand-off share one implementation instead of two.
The caret half is `window::actions::load_into_editor`'s `place_cursor`. The fraction pair the
hand-off used to run on (`preview::scroll`'s `restore_preview_scroll`, `adj_fraction`,
`set_adj_fraction`) is deleted rather than left available. Rubrics TDD 7.5 (amended) and 7.21;
Reading-Position CAM row 5 and Document-Reference CAM row 7.

**Why it won**: it is the only option that removes the lossy conversion instead of coarsening
it, so the accumulation cannot come back; and once one authoritative position exists, the caret
fix is a read of it rather than a second mechanism. The two measured defects in batch 1 are the
same missing thing seen from two ends.

**What it obliges**: the split-pane scroll sync, session restore and navigation history all
read positions today and each must be checked against the new single source; and because the
preview↔source mapping is not exact for every block, "same position" needs a stated tolerance
rather than an equality.

### 💡 2. Make view state adoption a pull, not a push

Give a view an explicit "adopt current state" step it runs when it is built or becomes visible
— reading the active theme, the reading position and the caret from the tab — and let the
live-change paths merely update that state and re-render whoever already exists. A view that
did not exist when a change fired then gets the right answer because it asks on the way in.

**Still open, deliberately.** This is the right shape for the theme fan-out *if* that turns out
to be an adoption gap, and that is precisely the half with no reproduction yet. Deciding it now
would be deciding it without the measurement.

**Pros**: closes the whole class structurally, so a fourth piece of per-view state inherits the
guarantee.
**Cons**: the largest change here; needs an inventory of what "view state" comprises or it
becomes a grab-bag, and risks double work unless the push path is trimmed in the same change.

### ❌ 3. Patch each symptom at its own site

Re-render a missed tab on activation; defer the caret to allocation; coarsen the mode-switch
position so rounding cannot move a block. **Rejected for the position**: coarsening hides
accumulation rather than removing it, and TDD 12.13's re-selected entry would still be chosen
from an approximate position. Its caret half is not rejected so much as absorbed — approach 1
places the caret from the authoritative position, which is the same repair with a source.

### ❌ 4. Fix the position and theme now, hold the caret pending research

**Rejected: its premise is dead.** It assumed the caret's warm path was unexplained and might
need a researcher brief. The measurement in *Root cause* names one mechanism covering both the
cold and warm cases, so there is nothing to hold it for.

## Working rule: ONE COMMIT PER BATCH

**A batch lands as exactly one commit on `master`, or it has not landed.** This plan's work
is organised into batches (the symptom groups above are batch 1); every batch is developed on
its own `feature/<batch>` branch and merged as a **single** commit — squash the branch, or
merge it `--squash`, so `master` gains one revision per batch and no more.

This is a rule and not a preference because of what a batch *is* here: a set of symptoms that
share one mechanism and are therefore fixed together or not at all. Splitting one into three
commits publishes intermediate revisions in which the mechanism is half-replaced — the old
fraction hand-off gone and the new position source not yet read by every consumer, say — and
those revisions are exactly what a `git bisect` lands on and what a seat fetches mid-flight.
One commit per batch keeps every revision on `master` a state the whole batch's tests describe.

**How it is held**, in order — the first two are mechanical, the third is the fallback:

1. **Develop on `feature/<batch>`**, freely, with as many working commits as the work wants —
   they are the branch's business and are never published.
2. **Land with `git merge --squash feature/<batch>` followed by one commit**, whose message
   names the batch and enumerates the symptoms it closes. Never `git merge` a batch branch
   fast-forward, and never cherry-pick half of one.
3. **A batch that will not fit one commit is not one batch** — that is the signal to re-cut it,
   not to relax the rule. Two mechanisms wearing one batch's name is the thing this prevents.

The seat branches POLICY § Cross-machine seat branches describes are unaffected: a `mac/…` or
`windows/…` branch is integrated into the batch branch, and the squash carries it, so a seat's
work reaches `master` inside its batch's single commit rather than beside it.

## Recommendation

The diagnosis in *Root cause* is now measured, and it re-cuts the batch rather than confirming
the original three-way split. **Batch 1 is the caret and the reading position, under approach 1;
the theme fan-out is not implementable yet** and belongs in a batch of its own once it has a
reproduction. The two batch-1 symptoms are one missing thing seen from two ends — an
authoritative document position, written once when a view is left, read when a view is entered,
and used to place the caret after a load instead of letting `set_text` park it at the end.

**Batch 1 is done.** Both of its symptoms are fixed, guarded and deleted from the debt
register. The guards were mutation-tested rather than merely written: reinstating the fraction
at *both* ends of the hand-off reproduces the original walk (79 → 108 → 147 → 158, against the
79 → 110 → 152 → 158 first measured) and fails both, and removing the caret placement fails
its guard with the original 27-line/`Ln 28` signature. Worth knowing before touching this
again: mutating only the *apply* end does **not** reproduce the drift — a lossy conversion
against an exact captured position yields a constant offset, not an accumulation — so a
round-trip guard has to be mutated at both ends or it certifies nothing.

**The theme fan-out is batch 2, and its next step is a live-display reproduction, not code.** It needs the
operator's real session (KDE/X11 compositor, a real themes file, the session's own tab state),
because two headless shapes of the same scenario both come out correct. Until it reproduces
somewhere a fix can be verified, any change to the theme path would be unfalsifiable.

## Technical details preserved

- **A pixel-valued regression guard for the drift would be flaky by construction.** The
  drift's direction is fixture-dependent (a 40-section fixture drifted forward where a
  shorter one drifted back), and the same binary produced two different sequences across
  two runs, varying by one to two outline rows. Assert on the block/document position,
  with a stated tolerance; never on a pixel offset.
- **The drift guard must not park the reader at the document's end.** The measured sequence
  terminates by clamping at `upper`, and a fixture that starts near the bottom would clamp on
  trip one and then look perfectly stable — a stationary reading exactly where the defect is
  worst. Park mid-document and assert across several trips, not one.
- **Discriminators already measured for the theme symptom**, from clean state with no user
  themes file: activating both tabs *before* the switch leaves both correct, and launching
  with the theme already in `session.toml` leaves the late tab correct. So the fault is
  specific to a **live** switch reaching a view that does not exist yet — not to
  background tabs in general, and not to theme resolution.
- **Discriminators already measured for the caret symptom**: reproduced on three routes into
  edit mode (the action, the toolbar button, and session restore) and on three documents of
  different lengths, each landing on that document's *own* last line — which is what
  `set_text`'s end-parked insert mark predicts for every one of them. The scroll position
  restores correctly; only the caret is at the far end.
- **All three are pre-existing**, reproduced on a binary built at the current branch's
  merge-base — the theme one pixel-identical across five captures. None was introduced by
  the decoration work.
- **The theme symptom has no headless reproduction.** Two background-tab shapes were driven
  under Xvfb (deferred-then-activated, and pump-pre-rendered-then-never-activated) and both
  come out with the correct ink and face. Do not re-derive this: the next attempt on that half
  starts at the live display, not at another headless harness.
- **The editor widget is persistent across mode switches** (ScrAP-58): reparenting it
  re-ran `gtk_scrolled_window_set_child` → `notify::vadjustment` → the gutter's binding,
  which read an already-freed controller. Any fix must keep it mounted; only the preview
  is safe to rebuild or free.
