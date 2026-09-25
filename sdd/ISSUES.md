# Known Issues

**`Platform`** is one of **`Windows`** · **`Mac`** · **`Linux`** · **`Any`** — the platforms an
entry is known to affect, not where it was found. `Any` means reproduced on, or inherent to,
every platform; a named one means the others were checked and do not exhibit it. Before
narrowing an entry to a single platform, have that platform's peer seat fail to reproduce it
(POLICY § Manual integration testing) — behaviour found on one platform is not
platform-specific until someone else looks.

**`Scope`** is one of **`Test`** · **`Production`** · **`Project`** · **`Upstream`**.
`Test` affects only the suite or the pipeline; `Production` affects what a user runs;
`Project` is both. **`Upstream` means the defect is in a third-party library and we cannot
FIX it** — a workaround may exist, but the repair is not ours to make, so an `Upstream`
entry is not work waiting to be scheduled here. It is orthogonal to severity: an `Upstream`
entry can still be the worst thing in the register.

**Read an entry sceptically before building on it.** Across the five batches that emptied
this register down from eighteen entries, **four** recorded root causes were measured and
found WRONG, and three entries turned out not to be defects at all — one whose stated worry
was structurally impossible while a different, real defect sat underneath it, reachable only
because the reproduction was built anyway. An entry is a report plus somebody's best
inference at the time, and the inference ages worse than the symptom. Reproduce first; fix
the thing you measured, not the thing that was written down.

**Two tables, and the split is the point.** The first lists **open** debt — things someone
is expected to fix — carrying a severity you triage on. The second lists **closed** entries:
problems investigated to a finding of *no reachable fix*, kept precisely so nobody spends a
session rediscovering a settled dead end. A closed entry is **not** a fixed one; a fixed
issue is deleted outright, because this file is a snapshot of what is currently broken and
not a changelog. Closed entries carry a `CLSD-dd` number that is never reused or renumbered,
and they hold no severity, because they are not queued work.

**One defect can be filed twice.** A missing reading position, seen from two ends, was
carried here as two unrelated entries and was nearly fixed twice before anyone noticed they
were one thing. Before opening work on an entry, scan the others for the same mechanism
described from a different vantage point.

| ID | Platform | Scope | Issue | Severity |
|----|----------|-------|-------|----------|
| D | Any | Production | A large document leaves the process spinning a CPU core at ~100% while idle — a GTK/Pango relayout pass that re-shapes text every main-loop iteration and never converges | High |
| G | Linux | Test | A one-time ~12.6 MB allocation appears in step 5b's footprint samples on the GitHub Linux runner and on no development host, at a different sample each run. **Unattributed** — the runner logs `libEGL warning: DRI3 error: Could not get DRI3 device`, so a lazily created buffer in its software GL stack is a suspicion and nothing more. The growth gate tolerates one allocation by design (TDD 6.11), so this is not currently red; what is unknown is whether the sampler is measuring something the application does not own | Low |
| I | Mac | Upstream | macOS only: every native file-chooser invocation (Open, Save, Export) grows RSS by ~1.1 MB and does not give it back. Roughly four fifths is AppKit's own price for presenting an `NSSavePanel` — reproduced with no GTK in the process — with about a fifth GTK-attributable. Caching the panel upstream would recover ~95% | Medium |
| W | Mac | Production | Observed ONCE: after a compound find-bar run the Escape key stopped closing the find bar and then never worked again in that process — permanent, not transient, with the bar visibly open and the application otherwise responsive. Not reproduced in three isolated legs nor in a faithful replay of the whole compound sequence. The handler has since been hardened so that it declines the key when the bar did not actually close, which BOUNDS this rather than fixes it: the diagnosed cause is still unknown | High |
| Y | Any | Test | A PDF blockquote-panel tiling assertion fails in the display-free suite intermittently under pipeline load, and passes every time it is run directly. **No root cause is recorded, and six suspicions have been falsified** — a sprite-key collision, cross-thread mutation of the sprite cache, line wrapping, concurrency during the render, cross-thread sprite decoding, and any non-tile red ink; the body carries each one's measurement. Two captures agree the anomalies sit INSIDE a band, which the fill cannot produce. The test now prints every red row's pixel count on failure, which is the one thing both captures lacked | Medium |

## Closed issues

Intractable: no reachable fix, and the limitation is still real and present.
Do not reopen one without a new constraint. These numbers never change and are never
reused — unlike the letters above, which are positional and get reclaimed.

| ID | Platform | Scope | Issue |
|----|----------|-------|-------|
| CLSD-01 | Any | Upstream | Tables are selection islands; cells are individually selectable but not part of the continuous buffer |
| CLSD-02 | Any | Upstream | A paragraph that mixes fonts (any inline-code span) can lay out a few pixels wider than the wrap width it was given, summoning the preview's Automatic horizontal scrollbar and intermittently blanking the pane until a resize |
| CLSD-03 | Windows, Mac | Upstream | No screen reader on Windows or macOS can read the app's accessible names: neither backend publishes a provider tree (no UIA there, no NSAccessibility tree here), so every name the app sets is correct and unreachable. Linux/AT-SPI reads them |
| CLSD-04 | Mac | Upstream | In fullscreen, a click issued while the transition animation is still running is never delivered — AppKit blocks input for its own ~250-500ms window, in any Cocoa application. Not ours to fix, and not GTK's |


## CLSD-01. Tables are selection islands

**Status**: Closed (intractable — every exit is walled *within* GTK's selection
machinery, source-verified to a measured verdict below; the one theoretical escape
leaves those bounds only by becoming a different project. Real and unresolved, not
fixed — retained as a documented permanent limitation. Not actionable.)

The preview is a single `GtkTextView` buffer with `GtkTextTag`s for formatting.
All prose, headings, code blocks, **blockquotes**, and inline content participate in
continuous cross-document selection. Tables are embedded as `GtkTextChildAnchor`
islands (a custom `ScribTableWidget` holding `GtkLabel` cells, each
`set_selectable(true)`); a cell's text is selectable on its own, but a drag-select
cannot span from body text into a table cell, nor across cells, in one gesture.

**Continuous cross-cell selection is unavoidable** (researcher-verified, gtk-4-6): an
anchored child occupies a single `U+FFFC` object-replacement char in the buffer, and
the `GtkTextView` selection model treats it as one opaque unit with no path into the
child's text. There is no cross-widget continuous selection in GTK 4.6. (Blockquotes
moved into buffer text precisely to get continuous selection where it *was* possible;
tables can't, because they need 2-D widget layout.) A drag cannot span from body
text into a cell, but selecting text *within* a cell copies that cell's own Markdown
source character-precisely (each cell carries its own `copymap`, formatting preserved
— TDD 2.8f); a buffer selection overlapping the table anchor copies the whole table
source.

### ⛔ Unmitigable within GTK's selection machinery — investigated, probed, closed

**Do not re-open this on a re-read.** Both routes past the anchor were taken to a measured
verdict (researcher + probe, gtk-4.6.9). The obvious designs all look viable on paper and
fail only at runtime, which is why the negative results are recorded here rather than
rediscovered.

**1. Mid-drag promotion — "let the label start the drag, take over when the pointer
escapes the cell" — is impossible, not merely hard.** `GtkLabel`'s lazily-created
selection machinery (`gtk_label_ensure_select_info`, `gtklabel.c:4826`) includes a
`GtkGestureClick` that **claims on press** (`:4313`). A claimed sequence sets `DENIED` on
*every gesture on parent widgets in the propagation chain* (`gtkgesture.c:84-92`), and
**`DENIED` is terminal** (`:1020-1035`). So by the time the pointer escapes, an ancestor
gesture can never claim. Observation was never the problem — capture-phase ancestors *do*
see the motion; **claiming** is.

**2. GTK's own sanctioned escape hatch — "claim early, decide late"** (`gtkgesture.c:94-99`:
a capture-phase ancestor claims on press, then denies to hand the press back, GTK
*emulating* it) — **was probed and fails twice** (Xvfb, double-click on a word, reading
`selection_bounds()`):

| Setup | Selection | |
|---|---|---|
| control — no ancestor gesture | `(0,5)` = `"alpha"` | ✅ word-select works unaided |
| deny on drag-update only (the documented shape) | **`None`** | ⛔ label receives nothing |
| + deny on release (that gap patched) | **`(0,11)` = `"alpha bravo"`** | ⛔ silently wrong |

- The documented shape **has no branch that fires for a click** — a click produces no
  motion (`drag_update = 0`), so a deny placed on drag-update never runs, the sequence
  stays claimed, and the press never reaches the label.
- Patching that doesn't save it: double-click then selects **two words**. The ancestor's
  claim emits `::cancel` on the gestures underneath (`gtkgesture.c:88-89`) →
  `gtk_gesture_click_cancel` (`gtkgestureclick.c:282-288`) → `_gtk_gesture_click_stop`,
  which **zeroes the counter**: `priv->current_button = 0; priv->n_presses = 0;`
  (`:112-113`). **The claim wipes the multi-click state on the way IN, before any
  emulation** — and the emulation replays *one event*, not the counter, so it is
  structurally incapable of rebuilding it. **`gtkgesture.c:94-99`'s "one similar event will
  be emulated" preserves event *coherence*, not gesture *state*** — the docs tell the
  literal truth and still mislead anyone designing this. Corollary: pre-empting a
  **stateless** gesture is recoverable; pre-empting a **stateful** one is not.
  *(The counter wipe is source-verified; the exact accounting for why the result is
  precisely two words rather than two independent single-clicks is unexplained, and
  deliberately not guessed at.)*
- The failure is **plausible-but-wrong**, not empty — it would feed Copy silently. Adopting
  it would break double-click word-select, which works correctly today.

**3. A keyboard-only trigger survives but isn't worth building.** `GtkLabel::move-cursor` is
a public keybinding signal (`:2205`) that fires *before* the default handler clamps, so a
boundary escape is observable — and it pre-empts no gesture. But a table where Shift+Down
crosses cells and **dragging does not** is less coherent than today's honest dead-stop.

*(Related GTK facts established during this investigation, in case they're wanted
elsewhere: `GtkLabel` exposes no cursor position — the public getter normalises
`anchor`/`end` away, `:2118-2120` — and the PRIMARY-clipboard "hole" GTK4Rs/AP-28 once alleged
**does not exist**; see GTK4Rs/AP-28 / GTK4Rs/AP-120.)*

**The limitation is accepted, and the impact is small.** In-cell selection is already
char-precise (TDD 2.8f); a buffer selection over the table anchor already copies the whole
table source. Nothing here is broken — the feature simply cannot be added through GTK's
selection machinery.

**The one theoretical escape, priced honestly and not recommended**: drop
`set_selectable(true)` and have `ScribTableWidget` own selection outright — hit-test via
Pango `xy_to_index`, draw the highlight in a `snapshot()` override. This sidesteps gesture
arbitration entirely (exactly what defeated the routes above) and is *technically*
possible, so this entry says "unmitigable **within GTK's selection machinery**" rather than
"impossible" outright. But it means reimplementing char selection, double-click-word,
triple-click-line, keyboard selection and PRIMARY ownership — all of which GTK provides
free today, as the probe's control demonstrates — to un-break a minor limitation
nobody has asked for. It would be a deliberate project chosen on product grounds, not an
increment, and it should not be started from this entry.

## D. A large document pegs a CPU core at ~100% while idle (GTK/Pango relayout loop that never converges)

**Severity**: High (the symptom is a full CPU core held at ~100% **indefinitely while idle**,
which directly contradicts the product's negligible-footprint thesis — but it is gated to
LARGE documents, tens of thousands of lines; typical small files are unaffected. Agent-generated
reports and plans, the product's own primary use case, can be that large, so it is reachable in
normal use rather than a corner case.)

Opening a large document leaves the process at ~100% CPU **forever, even after it is fully
rendered and sitting idle with no input**. Characterised headless (Xvfb, release build of
2026-07-26) on `tests/fixtures/large-doc.md` (3 MB / 41,785 lines): `pidstat` averaged **99.83%**
across a 60 s idle window (20 samples, all ~100%, process alive throughout). A normal document
(`tests/fixtures/lists.md`) opened the same way idles at **~0%**, isolating the spin to document
size.

**Second consequence, established 2026-08-07.** While the layout is invalid, GTK keeps its
incremental line-height validation idle permanently ready, and that starves anything the app
schedules below it. Far navigation (Ctrl+Home/End, Go To Line, find, outline) is deferred until
validation completes for correctness reasons (GTK4Rs/AP-260), so on a document caught in this spin
that navigation would never arrive at all. It is bounded rather than exposed — the deferral
carries a timer-based deadline above the validate idle's priority, which degrades to a partial
landing instead of hanging — but that mitigation exists *because of this issue* and would be
unnecessary without it. Measured counter-point: a 200 000-line plain-prose file settles to 0 %
CPU in ~30 s and does **not** reproduce the spin, so whatever drives it is not size alone.

Surfaced during the macOS-port bring-up, where a stack sample suggested a GtkSourceView
incremental-highlighter feedback loop (its progress `mark-set` re-dirtying the highlighter's own
region). Confirmed here to reproduce on Linux — so it is **not platform-specific** — but an
independent trace on this side **does not support the highlighter theory**.

**That disagreement is now sharper, not resolved — read it with the Pango-shaping claim
below, which it contradicts.** MEASURED 2026-09-15 on the status-bar build: the macOS seat's `sample(1)`
put the main thread in 2320 of 2332 samples under `g_application_run` →
`g_main_context_iteration` → `idle_worker` (libgtksourceview-5.0) → `update_syntax` →
`gtk_source_region_add_subregion` → `gtk_text_buffer_set_mark` → `g_signal_emit`, on this
fixture with no interaction — the first trace to NAME the highlighter. The Linux side
re-measured the same fixture that day: 100% of one core across 60 s with no settling,
against 0% for a one-page control. So two traces of one symptom point at different
machinery; reconcile them before choosing a mechanism, and do not treat either as settled.

**The threshold is not size alone, in both directions.** `sdd/TDD.md` (~3,800 lines,
markup-dense) burns ~11 s and then **settles by itself**; `large-doc.md` (41,785 lines)
never converges; the 200,000-line plain-prose file above settles in ~30 s. A reproduction
attempt that varies only line count can therefore miss this entirely — vary the construct
mix too.

**This is not confined to the ordinary idle context, which widens both its reach and its
reproduction.** MEASURED 2026-09-16 on macOS/Quartz: exporting an 18.5 MB document of the
spin-prone shape ran past 100 s without completing, three times, where the same export had
taken 47 s the day before; `sample(1)` put the main thread in
`gtk_print_operation_run` → `print_pages` → `g_main_loop_run` — GtkPrintOperation's **own
nested main loop** — and one of the sources that loop was dispatching was GtkSourceView's
`idle_scan_cb` → `scan_region_forward` → `scan_subregion`, i.e. this entry's highlighter
idle, still re-arming. So the spin competes for any loop that services the same main
context, not just the one the application runs; an export on such a document is a *faster*
reproduction than waiting for it to show up as idle CPU.

⚠ **Do not read a stalled export as a broken Cancel.** The two are separable and look
identical from outside: MEASURED on Linux, a 31 MB export drew no pages for minutes while
the window repainted normally and a Cancel click WAS delivered and logged — cancel takes
effect only between pages, so with no page ever drawn a delivered cancel sits idle. Judge
that path by a log line at the click handler, never by the export ending.

⚠ **The `Any` classification rests on TWO platforms, not three.** Reproduced on macOS and
Linux; **Windows has never been asked**. `Any` is still the right call — the trace lands in
Pango text shaping under a recursive GTK measure/layout pass, which is toolkit machinery
common to every backend, so this is `Any` by *inherence* rather than by a third
reproduction, which the header's definition admits. Recorded because the header also tells a
reader that a platform label is evidence-backed, and here one third of that evidence is an
inference. A Windows reproduction would upgrade it; a Windows *non*-reproduction would be a
significant finding about the backend and must not be read as merely narrowing the label.

**Trace** (gdb `thread apply all bt` on the spinning process, main thread, at idle). Every
worker thread is parked (futex / `cond_wait`); the hot main thread is entirely in text SHAPING
under a recursive GTK measure/layout pass:

```
#0–7   libharfbuzz   hb_shape_plan_execute / hb_shape_full          (text shaping)
#8–14  libpango      pango_shape_item + layout
#15–24 libgtk-4      measure / allocate / snapshot   (frames #18–22 are ONE return address ×5 → recursive widget-tree measure)
#25–27 glib          g_main_context_dispatch → g_main_context_iteration
#28    gio           g_application_run → main
```

There are **no GtkSourceView / highlight / mark / region frames anywhere**, and **no app-own
frames in the hot path**. So the CPU burns in a GTK **relayout / re-shape loop that never
converges** — a `size_allocate` / `queue_resize` pass re-shaping the large widget tree's text
via Pango/HarfBuzz on every main-loop iteration — not the incremental highlighter.

**Symptom vs driver — not yet fully pinned.** One stop-sample shows *where* the CPU is spent
(Pango shaping under GTK measure), not *what keeps scheduling* the pass. The macOS-side
`mark-set`-handler theory therefore survives only as a candidate **driver**: a handler that
re-`queue_resize`s in response to a signal the relayout itself emits would produce exactly this.
The buffer's `mark-set` is listened for in four places — `window/tabs/lifecycle.rs` (`:95`),
`window/editbar/overlay.rs` (`:155`), `preview/interactions.rs` (`:23`),
`preview/annotate/overlay.rs` (`:843`) — which are the suspects to bisect. (Several already
coalesce because `mark-set` is chatty, so the culprit is more likely a coalescing timer that
keeps re-arming than a naive re-dirty.)

**Distinct from F** (same *preview-overlay relayout* family, opposite outcome): F is a **rare,
recoverable blank** from a `GtkOverlay` snapshotted without an allocation; this one is a
**permanent ~100% CPU spin** with no blank. (Both entries previously called each other N and O
— letters that no longer name them.)

⚠ **The `GTK_DEBUG=geometry` probe both entries recommended CANNOT RUN on the reference
host, and its silence is not evidence.** Measured 2026-08-04: a distribution GTK is built
without debug support, so every informational `GTK_DEBUG`/`GDK_DEBUG`/`GSK_DEBUG` key reports
`[unavailable]` and emits nothing — an empty log therefore means *the instrument is dark*, not
*no widget re-queued a resize* (GTK4Rs/AP-251). Restoring that key requires a locally built,
debug-enabled GTK loaded ahead of the distribution one; `sdd/PLAN.profiling.md` records the
cost and the alternatives.

**PREREQUISITE — [`sdd/PLAN.profiling.md`](PLAN.profiling.md) is implemented FIRST, not
alongside** (operator, 2026-08-28). This entry is the one place in the register with no
oracle: the trace says where the CPU goes and not what keeps scheduling the pass, the
`GTK_DEBUG=geometry` key that would answer it is dark on this host, and every mitigation
below opens with "take several samples". Doing that with ad-hoc instrumentation is how the
work becomes open-ended — which is why the budget for it has to be agreed up front. Build
the instrument, then aim it. The plan is also the place that records what a debug-enabled
GTK costs, so the decision about whether to pay it is made once, in the open, rather than
midway through a bisect.

**Mitigation options** (all of them assume the instrument above exists):
- **Root-cause the driver** (recommended; not yet done): take several samples to confirm the
  loop consistently sits in shaping/layout — `perf record` against the unstripped debug binary
  gives named application frames today, with no change to the tree, and is the substitute for
  the unavailable geometry key; then bisect the four `mark-set` handlers by
  disabling each and re-measuring idle CPU. If one stops the spin, that handler is the driver;
  if none does, the driver is not `mark-set`, and the search moves to whatever re-invalidates
  the (likely preview) widget tree's layout every iteration.
- **Likely fix shapes** (pending the driver): make the offending handler idempotent so it does
  not re-invalidate the region it reacts to; coalesce/gate the relayout so it converges; or
  ensure an incremental idle returns `G_SOURCE_REMOVE` once stable. Left open deliberately —
  fixing the wrong layer (e.g. throttling shaping) would mask the loop rather than end it.
- **Accept the limitation**: not viable long-term — an idle full-core spin on the product's own
  primary use case (large agent-generated documents) defeats the negligible-footprint thesis the
  project exists to honour.

## I. Every native file chooser invocation grows RSS on macOS

**Severity**: Medium. Monotonic within everything measured at the per-invocation scale, but the
cost is overwhelmingly AppKit's own price for presenting an `NSSavePanel`, and it is not
reachable by any change this project can make.

**Re-measured 2026-08-27 with a control, which sharpened the claim in both directions.** Ten
`File ▸ Open` invocations, CANCELLED every time so no document ever loaded, grew RSS
222,432 → 232,288 KiB — monotonic, never reclaimed, **≈985 KiB per invocation**. The control
is what makes that a cause rather than a coincidence: ten cycles at the same cadence, same
frontmost-and-Escape driving, chooser never opened, moved RSS by **+32 KiB total**. So the
growth is the chooser, not elapsed time and not the driving method.

**And the counter-evidence, recorded because it is the half that would otherwise be
mis-read.** A separately-observed instance sitting at 257 MiB after ~1.5 h of ordinary use
was NOT this issue accumulating: watched across a further window it went 257 → 251.5 →
230.5 MiB — *downward*. "Idle instance at a high RSS" reads as corroboration and is the
opposite. The per-invocation leak is real; something reclaims at a larger scale, and the
shape of that reclamation is unmeasured. Do not describe this entry as unbounded growth.

**Symptom**: opening a `GtkFileChooserNative` and cancelling it grows resident memory on
macOS, per invocation, and the memory never returns. Neither Linux nor Windows reproduces it.

**Status: ATTRIBUTED TO THE PLATFORM, and OWNER-BLOCKED.** Not a GTK defect and not this
project's reference discipline — both call sites were audited and cleared. Roughly 93% is
recoverable upstream by REUSING the panel rather than releasing it; a patch sketch exists,
must be authored twice because the 4.6 and current variants differ, and two naive forms of it
ship a use-after-free. The upstream filing is written and reviewed and **cannot be submitted
from any seat here** — it needs the operator's credentials or explicit instruction.

**What this project does about it**: nothing, deliberately. There is no application-side fix,
the exposure is one panel per invocation on one platform, and TDD §6's ceiling is not
threatened by it.

**The full investigation is `probes/native-chooser-rss-investigation.md`**, beside the probe
that produced it — measurements with their conditions, the retain-cycle analysis, the patch
sketch and its hazards, and the instrument failures that shaped it. It lives there rather than
here because this entry exists in order to be deleted when the defect is fixed, and the
evidence must outlive it. Do not restate its figures here; several carry caveats that do not
survive summarising, and the transferable lessons already have permanent homes in
`sdd/ANTI-PATTERNS.md`.

## CLSD-02. A paragraph that mixes fonts lays out wider than the wrap width it was given

**Status**: Closed (no public API at the GTK 4.6 floor makes the layout report a width the
wrap budget respects; the two reachable correctives both cost more than the defect)

**Platform**: Any — the mechanism is Pango's line-extent accounting, not a backend's. Only
Linux/GTK 4.6.9 was measured; font metrics differ per platform, so *which* window widths
exhibit it will differ, not *whether* it can.

A `GtkTextTag` that changes the font FAMILY over a character range — in this project, every
inline-code span — splits the paragraph into separate Pango items at the tag boundary. A space
that lands on a wrap point is granted for free by the break logic (`find_break_extra_width`),
but the routine that collapses that hanging space afterwards (`zero_line_final_space`) is keyed
on the last run's last glyph, which is a different object once the items are split. The space
therefore stays, sitting a few pixels past the wrap width, and `GtkTextLayout` reports the
line's LOGICAL extent — hanging space included — as the layout width. That becomes
`hadjustment.upper`, which exceeds `page_size`, which summons the Automatic horizontal
scrollbar, whose appearance and disappearance re-arms the width↔height-for-width churn that
leaves the preview stuck blank until a manual resize (GTK4Rs/AP-22, GTK4Rs/AP-23).

MEASURED (GTK 4.6.9, gtk4-rs 0.10, X11/Xvfb, `#[gtktest::test]`, this repository's own
`sdd/ANTI-PATTERNS.md` as the corpus): a sweep of 41 window widths (600–1000 step 10) at zoom
1.0 found 2 widths over-wide, by 5px and 7px. Isolation is decisive in both directions —
removing ONLY the tag's `set_family` takes the overflow to zero at every width and zoom tried;
removing ONLY the tag's `set_wrap_mode` changes nothing (wrap mode is a paragraph attribute
taken from the view, so a character-range tag never alters it), and the tag's background does
not participate in width.

Impact is narrow and real: on roughly 5% of window widths for a code-dense document the reader
gets a horizontal scrollbar it cannot use and a pane that intermittently blanks while
scrolling. It is invisible at every other width.

**What walls each exit** — recorded so the dead ends are not re-explored:

- **Reserve slack in the wrap budget** (extra `right_margin`, CSS padding, or the private
  `gtk_text_layout_set_screen_width`) — REFUTED BY MEASUREMENT, not by argument. Any change to
  the wrap budget moves the breakpoint, so the failure RELOCATES rather than clearing: the same
  41-width sweep failed at exactly 2 widths with 0px, 8px and 16px of extra right margin, only
  at different widths each time. A single-width control cannot see this, and reads as a fix.
- **Derive the slack from the fonts' space advances** — the quantity does not exist. The hang is
  however much of the granted glyph sits past the wrap point plus accumulated shaping error at
  the item seams plus the layout's `ceil`, not a glyph metric: measured hangs of 4px and 6px
  against a body space of 3px and a code space of 8px. Any constant is a guess, and it would
  relocate anyway per the point above.
- **Clamp `hadjustment.upper` down to `page_size` when the excess is below a threshold**, from a
  `size_allocate` override after chaining up. This one WOULD close the invariant without
  relocating, and does not re-arm the churn. Declined: it is a symptom gate, not a wrap fix; the
  threshold can only ever be an observed bound from a width sweep rather than a derived
  quantity; and because the hang is not always pure whitespace it may clip 1–3px off a real
  glyph — trading a rare scrollbar for rare silent truncation of the reader's text.
- **Drop the monospace family on inline code** — removes the trigger completely and is the
  positive control that proves the mechanism. Declined: inline code reading as code is the
  product, so this trades a rare layout defect for a permanent, universal regression in what the
  reader sees.
- **`hscrollbar_policy = Never`** — banned outright and independently of this entry: it makes
  `GtkScrolledWindow` adopt the child's minimum width and ratchet, so the window can no longer
  shrink to fit (GTK4Rs/AP-139).

**Mitigation options**:
- Accept the limitation (chosen). A reader who hits it can resize the window a little; the
  defect is a property of the width, so any nearby width clears it.
- Revisit if the toolkit floor rises — this was checked against GTK 4.12 and the width
  computation is unchanged, so a fix would have to come from Pango's collapse logic rather than
  from GTK.
- Revisit if the clamp above stops being a symptom gate — if a way appears to distinguish a
  hanging space from a clipped glyph, the clamp becomes safe and this reopens.

## CLSD-03. No screen reader on Windows or macOS can read the app's accessible names

**Status**: Closed (inherent to GTK4's Windows and Quartz backends — the app sets the
names correctly and neither platform publishes a tree that can carry them)

⚠ The `Platform` cell reads `Windows, Mac` rather than one of the four single values the
header defines: the consequence is identical on both and the causes are backend-specific,
so filing it twice would be the header's own "one defect filed twice" trap, and `Any` would
be false — Linux/AT-SPI reads these names correctly.

MEASURED on both seats while ratifying the status bar (2026-09-15). **Windows** (GTK 4.22.4
gvsbuild, Win10 19045): UI Automation returns the toplevel (class `gdkSurfaceToplevel`)
with **zero descendants**, against a positive control of Notepad returning two. **macOS**
(GTK 4.22.4/Quartz): the window exposes 4 chrome elements and its "entire contents" is
**empty**, with the reader validated in the same run (the native menu bar enumerates, and
Save/Save As report their real enabled states). So no screen reader on either platform
reaches any control — not the toolbar, not the sidebars, not the status-bar indicators.

The consequence for verification is the part that misleads: the accessibility rubrics
(TDD 16.5, 16.7, 16.17) are **unrunnable** on Windows rather than failing. Their names
ARE set — `a11y.rs` is the single choke point and `clippy.toml` bans the bare tooltip
setter — and they are read correctly by AT-SPI on Linux, so a Windows run that reports
these checks as "not observed" is reporting this gap, not a defect in the code under test.

**Mitigation options**:

- **Accept it** — the position taken. Every exit is walled outside this project: the
  provider tree is GTK's to publish, there is no application-side API to attach one, and
  writing a UIA provider for another toolkit's widgets is a different project.
- **Verify these rubrics on Linux and macOS only**, and record the Windows limb as a
  platform gap in the run rather than as missing coverage.
- **Re-check on a future GTK** — if the Windows backend ever gains a UIA bridge this
  reopens at Low/Medium/High, since the names are already in place to be read.


## W. Escape stopped closing the find bar, permanently, once

Reported by the `mac` seat while verifying the window-level Escape handler. After the
annotation-card leg of a full compound pass: Escape #1 closed the card and left the bar
open (correct), **Escape #2 did not close the bar, and Escape never worked again in that
process** — five further presses with the frontmost window re-asserted each time, plus a
click into the editor to restore focus, all no-ops. The bar stayed visibly open. The
application was otherwise responsive.

**Severity is High because of the shape, not the frequency.** It is a functional wedge
of a key the whole application shares, it is permanent within the process, and the only
escape from it is to quit. Compare ISSUES Z, which is superficially similar — one
occurrence, compound run only — but is a log line with no user-visible effect.

**Not reproduced.** Three isolated legs pass (edit mode with no selection, edit mode with
a selection, the annotate card). A faithful replay of the entire compound sequence in
order — preview Cmd+F, type, Enter, click, Escape, three further Escapes, history
popover, theme drop-down, a click on a disabled menu item, a mode switch, the Go To Line
dialog, select, annotate, Escape, Escape — passes end to end. The disabled-menu-item
click was also tried alone and passes.

**What the hardening does and does not do.** `wire_find_bar`'s window-level handler used
to return `Propagation::Stop` whenever the bar was revealed, on the assumption that the
close it had just called worked. That spelling has a latent permanent wedge in it: if the
bar is still open afterwards, the next press takes the same branch, does nothing, and
swallows Escape again — for every other consumer in the window, for the life of the
process, which is exactly the reported shape. The handler now re-reads the revealer and
**declines** when the bar did not close, and asks both `reveals_child` and
`child_revealed` so a desync between the target and drawn states cannot hide the bar from
it either. **This bounds the blast radius; it is not a diagnosis.** Neither mechanism has
been shown to be what happened.

**The evidence from the one occurrence is gone**, and the way it was lost is worth
keeping: the seat reset an isolated `HOME` to clear session state before replaying, which
deleted the wedged run's persistent log with it. stdout was empty, so that log was the
only place anything could have been recorded. **Copy the state directory aside before
resetting anything, on any run that has already shown an anomaly.**

**That re-run has now happened, and it did not reproduce.** Two full compound passes
against the hardened build, identical sequence, state directory copied aside before each
reset. Every leg correct both times, including the wedge probe (five further Escapes plus
a focus-restoring click). **`find: Escape did not close the find bar` appeared zero
times.**

**Why that absence is worth something.** "The sink is live" was necessary and not
sufficient: the line being watched for is WARN, and everything these runs emit on their
own is info or debug, so *no warn line* and *warn cannot get through* would look
identical. The `mac` seat forced one — a malformed `themes.toml` in the isolated config,
producing a real `WARN scribobulate::theme::spec` line in the same persistent log, same
binary, same `RUST_LOG` — then removed the control so it could not contaminate the run.
The absence is therefore measured rather than inferred.

**It does not clear the suspected path.** Two clean passes fail to catch the
Stop-on-assumption mechanism; they do not exclude it. Nothing here upgrades this entry
from suspicion to mechanism, and the hardening remains a bound on blast radius. What can
be said is narrower and still useful: **whatever wedged did not announce itself on the
path that is now instrumented.**

**Where to look on the next sighting.** Not at the sequence — it now has four clean
replays against it. The original wedge occurred in a *hand-paced* run with screenshots,
accessibility-tree walks and menu enumerations interleaved; the replays are scripted with
fixed two-second settles. Wall-clock timing and AX traffic are the two things that
differed, and the accessibility bus is a plausible source of both extra main-loop work
and extra focus churn.

## Y. A PDF blockquote-panel tiling assertion fails intermittently in the display-free suite

`export::pdf::measure::tests::a_blockquote_panel_sprite_tiles_across_the_page_and_keeps_one_grid`
fails inside a full pipeline run and passes when run on its own. It has never been
reproduced on demand.

**Two suspicions are now falsified. Neither is a root cause; both are recorded so the
next reader does not re-derive them.**

1. *A sprite-key collision.* Ruled out when the entry was opened — the fixture writes its
   sprite to a unique temp directory.
2. *Parallel libtest threads mutating a shared sprite cache.* This is what the entry used
   to assert, and it is **wrong**: the cache is `thread_local!` (`src/sprite.rs`, the
   `NATURAL` / `RESAMPLED` / `SURFACES` triple), so each test thread holds its own and
   there is nothing between them to race. At least ten test files do call
   `sprite::clear_cache()`, which is what made the story plausible — but every one of
   those calls clears only its own thread's copy.

**What the one captured failure actually says.** The assertion collapses the marker rows
to run starts and requires them to share one residue modulo the point pitch (12). The
observed starts were `[60, 72, 81, 96, 108, 120, 132]`: every one is a multiple of 12
except `81`, which should have been `84`. So the page was **not** drawn on the wrong
lattice — a systematic error would move every start, and the assertion message's own
second hypothesis ("the lattice is in PIXELS and the tile is printing 4/3 oversize")
predicts exactly that. One start out of seven is off by three rows, which is a single
row-gap inside one marker band, not a wrong grid.

That reframes the search: the question is what makes **one row** of a band fail to match
the marker colour exactly, not what would rescale the tile.

**A second capture, and it says the same thing louder.** The GitHub Linux runner failed
it on 2026-09-23 with starts `[60, 65, 72, 84, 96, 101, 108, 119]`. Six of the eight are
multiples of 12; the odd ones are `65` (five rows into the band at 60), `101` (five rows
into the band at 96) and `119` (one row *before* 120, which is itself absent). Modulo 16
they share nothing either, so the oversize-lattice branch is falsified on this sample too.
Across both captures every anomaly is **inside** a band — a band split in two, or its
first matching row slipping one to three rows early — and never a band displaced as a
whole. Two independent samples now agree on the shape.

**The same tree passed and failed twelve minutes apart.** That CI failure was the merge
commit, whose diff against the branch head it merged is EMPTY, and the branch head's own
run on the same runner image had passed. Identical code, identical image, opposite
verdicts — so nothing about the source decides this, and any bisect is wasted effort.
It also moves the reproduction off this host for the first time: it is not a property of
one machine's storage or CPU.

**Four families are now measured out, on Linux on 2026-09-23. None of them is the
cause; each cost a session's worth of guessing before, and the numbers are here so the
next reader spends theirs elsewhere.**

| Ruled out | How | Result |
|---|---|---|
| Line wrapping / font metrics moving the rects | the same fixture rendered at 30 different word counts, every resulting layout checked against the lattice | 0 off-lattice |
| Concurrency during the render | 8 threads rendering this exact page 160,000 times **while the whole suite ran alongside it** | 0 off-lattice |
| Low parallelism and slow timing, as on the runner | **the exact step-4 command** (`cargo test`, not `--lib`) pinned to two cores with `taskset`, 12 runs | 0 failures |
| A test mutating the global font state under it | searched for `set_resolution`, `FontMap::set_default`, `set_font_options`, `FontOptions`, `set_antialias`, `set_hint` across `src/` | no such call exists in the tree |
| Cross-thread sprite decoding | 8 threads decoding 8 distinct sprites in a loop, every pixel compared against its own thread's colour | 0 bad of 122,880 |
| Some other red ink on the page (a glyph, the bar, a band) | the same fixture with a tile that has **no red row in it at all**, counting pure-red pixels | 0 |

⚠️ **Read the concurrency row in the direction it was measured.** That stress pushed
parallelism UP — 8 threads on a 32-core host — while the GitHub runner that failed this
has 2 cores, so libtest runs it with 2 threads. It is therefore evidence against a
cause that needs *contention*, and much weaker evidence against one that needs slow,
serialised timing. The two-core row was added afterwards for exactly that reason, and
it is the closer match to the environment that actually produces the failure.

**And one thing the fill cannot do, by construction.** The vertical anchor is
`floor(y / pitch) * pitch` (`export::pdf::ink::tile_origin`), so a repeat can only ever
begin on the lattice; the one way a rect's own top can expose red earlier is if that top
falls *inside* the band, which bounds the residue to 0..2. Both captured failures have
residues outside that (`9`; and `5`, `5`, `11`). So either the red on those rows is not
the fill at all, or something upstream of `tile_origin` is not what it appears to be.

**Both captures look like TWO phases of the SAME pitch, which points at the transform.**
Capture 2's gaps run `5, 7, 12, 12, 12, 5, 7, 11` — and `5 + 7 = 12`, so it reads as the
expected lattice at phase 0 (`60, 72, 84, 96, 108`) with a second set at phase 5 (`65`,
`101`) laid over it. Capture 1 is the same shape with one stray band, at phase 9. Two
phases of pitch 12 cannot come from `tile_origin`, which only ever returns multiples of
12 — but they are exactly what an extra translation in force for *some* of the rects
would produce. Every `save`/`restore` on this path swallows its error (`cr.save().ok()`),
so an unbalanced pair would leak a transform silently and only for the rest of that page.
That was the obvious place to look, and it has now been looked at twice. Every
`save`/`restore` pair in `export::pdf::ink` balances by inspection, including the two
arms that `continue` out of the line loop — and, because those calls discard their
errors and an imbalance would therefore be silent, the transform was also checked AT
RUNTIME: the context's matrix was reported on entry to every quote-panel paint across a
whole `cargo test --lib` run, and it was the identity every time, 0 exceptions. So the
transform is not leaking on any path the suite exercises. The two-phase reading stands
and is still unexplained, which is the most useful thing in this entry.

**The test now says which.** Its panic prints every red row with its pixel count and x
extent. A band row spans the panel — around 458 px of the 64..521 column, less the
quoted text's overdraw. A handful of pixels on a row is not a displaced lattice and
sends the search somewhere else entirely. Two failures have been captured in the wild
and neither could answer this, which is the whole reason the message was rebuilt rather
than another theory added.

**Do not make the oracle ignore thin rows yet.** It is the obvious move once the panic
starts printing pixel counts — discard any row carrying a handful of pixels, since the
fill cannot draw one — and it would very likely turn this green. That is the objection
to it: it would turn it green whether or not the thin rows are the cause, and nobody
would ever find out, because the only evidence this defect produces is the failure
itself. Wait for one real capture that shows the widths, then narrow the oracle to what
that capture proves it should measure.

**Repeated negative controls, same date.** Five direct runs of the test alone: pass.
**Twenty-eight** full `cargo test --lib` runs: pass, 1,871 cases each. **Twelve** runs
of the real step-4 command on two cores: pass. Every cheap
instrument here is green on a build that fails, so anything claiming a fix has to
survive repeated *pipeline* runs — and the next real failure is worth more than another
local loop, because it will now arrive carrying its own evidence.

**Do not chase this from the find-bar branch it was observed on.** It is unrelated to it:
the branch touches the find bar, the toolbar wrap box and three packaging scripts, none
of which reach PDF export, sprites or rasterisation, and an immediately preceding
pipeline run of the same code passed.

## CLSD-04. In fullscreen on macOS, a click during the transition animation is never delivered

**Status**: Closed (no repair of ours is reachable). Kept so the
investigation is not run a second time.

**The report**: on the macOS build in fullscreen, clicking a toolbar button did nothing and
a second click was needed; the menu bar behaved the same way, and a toolbar button appeared
to activate on the wasted click.

**All three parts are accounted for, and none is a defect in this project or in GTK.**

- **The wasted toolbar click is AppKit's animation blocking its own input.** Measured as a
  timing sweep from the zoom button's mouse-up to the test click, real input throughout, no
  synthetic pointer placement: 0 ms, 100 ms and 250 ms all fail **silently** — the click
  never reaches GTK at all, invisible even to window-level instrumentation — while 500 ms,
  1 s and 2 s all work. The boundary matches an ordinary `NSWindow` fullscreen transition's
  duration. Every Cocoa application has this; a reader who hits the green button and reaches
  straight for a toolbar command is clicking inside that window.
- **The menu bar needing two clicks is macOS auto-hiding it in fullscreen**: a cold click at
  the top edge is spent on the reveal.
- **The phantom activation was a keyboard focus ring**, which in a screenshot is
  indistinguishable from a button that was just pressed. Confirmed by Escape moving the ring
  with no click involved. Assert on whether an action fires, never on what a screenshot
  looks like.

**A genuine toolkit defect was found on the way and is NOT this.** After a fullscreen
transition has fully settled, placing the pointer with `CGWarpMouseCursorPosition` — which
moves the cursor without posting a motion event — and then clicking makes the first click
skip the picked widget's own controllers entirely, while an ancestor's capture-phase gesture
still sees it and `pick()` resolves correctly. Reproducible with zero application code on
GTK 4.22.4 / macOS 27.0; `probes/macos-fullscreen-first-click.c` holds the measurement. No
mouse or trackpad gesture teleports a cursor, so no user meets it — it is recorded as an
upstream curiosity, not as this entry's subject.

**Dead ends, each closed by measurement, and not to be revisited**: a coordinate-space or
title-bar-origin offset; the macOS behaviour where the click that activates an inactive
window is not delivered; lost or mis-picked input; a stale implicit grab; this project's own
macOS pointer-crossing seam; a disabled action or insensitive widget; and any
ours-versus-upstream asymmetry — both binaries agree once entry and pointer arrival are held
constant. Returning to a fullscreen Space from another application is also clean.

**Linux and Windows have not been checked**, so the `Mac` narrowing is provisional — the
register's rule is that behaviour seen on one platform is not platform-specific until a
peer seat looks.
