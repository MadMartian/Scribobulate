# Known Issues

**`Platform`** is one of **`Windows`** · **`Mac`** · **`Linux`** · **`Any`** — the platforms an
entry is known to affect, not where it was found. `Any` means reproduced on, or inherent to,
every platform; a named one means the others were checked and do not exhibit it. Before
narrowing an entry to a single platform, have that platform's peer seat fail to reproduce it
(POLICY § Verifying a change on macOS) — behaviour found on one platform is not
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

**One defect can be filed twice.** A missing reading position, seen from two ends, was
carried here as two unrelated entries and was nearly fixed twice before anyone noticed they
were one thing. Before opening work on an entry, scan the others for the same mechanism
described from a different vantage point.

| ID | Platform | Scope | Issue | Severity |
|----|----------|-------|-------|----------|
| A | Any | Upstream | Tables are selection islands; cells are individually selectable but not part of the continuous buffer | Closed |
| D | Any | Production | A large document leaves the process spinning a CPU core at ~100% while idle — a GTK/Pango relayout pass that re-shapes text every main-loop iteration and never converges | High |
| F | Mac | Upstream | A GTK4/Quartz autorelease-pool crash SIGABRTs the macOS integration suite in roughly one full run in four, at a varying site | Medium |
| G | Any | Test | Two wall-clock growth-ratio guards (tab normalisation, annotation extraction) go red on a loaded machine — the ratio is scheduler noise on a small baseline, not an exponent | Low |
| H | Mac | Production | macOS only, INTERMITTENT: the preview's hover cursor sometimes does not take over body text or a link, showing the default arrow; the drawn affordances that repaint on hover are always correct | Low |
| I | Mac | Upstream | macOS only: every native file-chooser invocation (Open, Save, Export) grows RSS by ~1.1 MB and does not give it back. Roughly four fifths is AppKit's own price for presenting an `NSSavePanel` — reproduced with no GTK in the process — with about a fifth GTK-attributable. Caching the panel upstream would recover ~95% | Medium |
| J | Any | Upstream | A paragraph that mixes fonts (any inline-code span) can lay out a few pixels wider than the wrap width it was given, summoning the preview's Automatic horizontal scrollbar and intermittently blanking the pane until a resize | Closed |
| M | Windows | Production | On a machine with no Visual C++ runtime the app installs and then fails to start; the installer's bootstrapper for it has landed but has never been verified against that condition | Medium |
| N | Any | Production | A document embedding a large SVG stalls the main thread for a fifth of a second on EVERY preview render — the decode is synchronous and uncached, so a zoom step, a disclosure toggle and each debounced keystroke in split mode all pay it again | Medium |

## A. Tables are selection islands

**Severity**: Closed (intractable — every exit is walled *within* GTK's selection
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
**does not exist**; see GTK4Rs/AP-28 / ScrAP-135.)*

**Severity stays Low and the limitation is accepted.** In-cell selection is already
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
free today, as the probe's control demonstrates — to un-break a Low-severity limitation
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
`442ae1f`) on `tests/fixtures/large-doc.md` (3 MB / 41,785 lines): `pidstat` averaged **99.83%**
across a 60 s idle window (20 samples, all ~100%, process alive throughout). A normal document
(`tests/fixtures/lists.md`) opened the same way idles at **~0%**, isolating the spin to document
size.

**Second consequence, established 2026-08-07.** While the layout is invalid, GTK keeps its
incremental line-height validation idle permanently ready, and that starves anything the app
schedules below it. Far navigation (Ctrl+Home/End, Go To Line, find, outline) is deferred until
validation completes for correctness reasons (ScrAP-260), so on a document caught in this spin
that navigation would never arrive at all. It is bounded rather than exposed — the deferral
carries a timer-based deadline above the validate idle's priority, which degrades to a partial
landing instead of hanging — but that mitigation exists *because of this issue* and would be
unnecessary without it. Measured counter-point: a 200 000-line plain-prose file settles to 0 %
CPU in ~30 s and does **not** reproduce the spin, so whatever drives it is not size alone.

Surfaced during the macOS-port bring-up, where a stack sample suggested a GtkSourceView
incremental-highlighter feedback loop (its progress `mark-set` re-dirtying the highlighter's own
region). Confirmed here to reproduce on Linux — so it is **not platform-specific** — but an
independent trace on this side **does not support the highlighter theory**.

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
*no widget re-queued a resize* (ScrAP-251). Restoring that key requires a locally built,
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

## F. A GTK4/Quartz autorelease-pool crash intermittently SIGABRTs the macOS integration suite

**Re-measured 2026-08-31 by the macOS seat, and the DISCRIMINATOR is now sharp.** 5 aborts
in 15 full `gtk_suite` runs — **33%, one in three**, spread across two trees (3 on one, 2 on
the other), so it is unmoved by the work that happened to be under test. Abort case indices
234, 234, 235, 16, and one inside a pipeline run: clustered, with one far outlier. And the
finding that narrows it most: **0 aborts in 93 FILTERED runs** of two cases per process. It
needs suite DEPTH, not any particular body — consistent with an accumulating pool imbalance
rather than one bad test, and it means a bisect-by-test cannot reach it.

**Re-measured 2026-08-27, and the rate and shape are both narrower than first recorded.**
Roughly **one abort in four FULL pipeline runs**, not two in three, and the abort site varies
rather than concentrating on the focus-churning test — the observed one was a find-cursor
test. The discriminator: `gtk_suite` run standalone, three times consecutively, passed clean
every time (323 passed). So this is a property of the FULL run rather than of any one test,
which is what a fix would have to account for and what a bisect-by-test would never find.

**Severity**: Medium (the macOS GTK suite cannot be trusted to complete; no data at risk,
and no Scribobulate code is implicated — but a red run there means nothing until re-run)

`cargo test --features gtk-integration-tests --test gtk_suite` intermittently aborts the
whole test process on macOS. Not one specific test — whichever happens to trigger a
text-view mark-set at the wrong moment relative to macOS's input-method state.

**Measured** via four independent **Apple crash reports** — the system crash reporter, not
a Rust panic (`termination: {namespace: OBJC, flags: 646, code: 1}`) — across four separate
runs. That distinction is the one that matters diagnostically: the Rust harness reports
only that the process died, which is equally consistent with a defective test, and the
discriminating evidence exists only at OS level. All four stacks are identical in
signature:

```
gtk_text_view_mark_set_handler                                   (libgtk-4.1.dylib)
  -> discard_preedit                                             (libgtk-4.1.dylib)
  -> +[NSTextInputContext currentInputContext_withFirstResponderSync:]   (AppKit)
  -> TSM input-method session (de)activation                     (HIToolbox)
  -> nested CFRunLoop pump for NSPasteboard promised-data resolution
  -> objc_autoreleasePoolPop -> AutoreleasePoolPage::busted_die() (libobjc.A.dylib)
```

**No Scribobulate frame appears anywhere in the faulting stack.** The cause is GTK4's
Quartz backend firing `discard_preedit` on any `GtkTextView` mark-set — i.e. on any caret
or selection change — which activates/deactivates the macOS input-method bridge, which
pumps a nested run loop for pasteboard-promise resolution and corrupts the autorelease
pool stack. Not reachable from application code.

**Rate and distribution** (n=6 full-suite runs, isolated): **4 crashed, 2 clean — about
two in three.** Of the 4 crashes, **3 were on the same test**,
`select_all_stands_down_for_every_text_entry_and_recovers_for_the_editor`; the single
outlier was the first observation, which was also a contaminated run (a concurrent build
on the same machine). All 4 crash reports carry a byte-identical stack signature.

**Why this is still not filed against that test**, even though it is the dominant trigger
— the argument is the stack, not the distribution. No application frame appears in it, so
nothing in that test's *code* is faulting; what the test does is arrive at the toolkit
path more often. It exists to verify select-all standing down across *every* text entry,
so its body is mostly rapid focus-switching between entries — which is precisely what
drives `discard_preedit` / `NSTextInputContext` activation churn. A test that exercises
the mechanism hardest crashing most is consistent with the mechanism, not evidence of a
defect in the test.

> **An earlier version of this entry claimed the opposite and was wrong.** On n=3 it read
> "2 crashed on *different* tests, so a defective test would fail on the same one every
> time" — and offered that distribution as the load-bearing proof. A larger sample
> inverted it: the spread was an artefact of a small n whose one cross-test data point
> came from the contaminated run. The mechanism argument survived unchanged because it
> never rested on the distribution; the distribution argument did not. Recorded because
> the retracted reasoning is more instructive than the correction — a frequency pattern
> read off three samples is a hypothesis, and it was stated here as evidence.

**Unverified**: whether it reproduces on other macOS or GTK versions. Linux and Windows
have no equivalent Quartz/AppKit/TSM path, so it is plausibly macOS-only *by
construction* — but that is an argument, not a test result.

**Mitigation options**
- **Re-run and treat a single abort as inconclusive** — what the macOS seat does today.
  Cheap, but it means the suite's silence is weaker evidence there than on Linux.
- **Raise it upstream** with the two crash reports. The stack is specific enough to be
  actionable and nothing about it is project-specific.
- **Re-test on a newer GTK** when one is available; this is the kind of interaction an
  upstream fix moves without anyone here doing anything.

Measured on macOS 26.6.1 (25G76), GTK 4.22.4 (Homebrew), by the macOS seat. Primary
evidence is machine-local and not transferable:
`~/Library/Logs/DiagnosticReports/gtk_suite-9047c36e3af692e9-2026-08-07-232935.ips` and
`…-2026-08-08-015722.ips`.

---

## G. A wall-clock growth-ratio guard fails on a loaded machine

**Severity**: Low (no user-visible effect; it degrades a *gate* rather than the app)

The main instance is fixed: `.cargo/config.toml`'s `[env]` now pins `XDG_CONFIG_HOME` to a
scratch directory, `Config::load()` carries a mutation-tested assertion that it is set, and
the two-host differential that exposed this (`scripts/coverage.sh` against the same run with
a scrubbed config dir) is now byte-identical where it used to differ by four lines.

**Two paths remain, and they are the same defect wearing different variables:**

1. **`theme.rs`'s search path**, measured at 3 lines. `find_themes_file()` walks
   `system_data_dirs()` — `XDG_DATA_HOME`, then each entry of `XDG_DATA_DIRS` — so how much
   of that loop executes depends on how many data directories the host advertises. A
   developer box reports more than a hosted runner, so those lines are covered here and not
   there. **Do not fix this by pinning `XDG_DATA_DIRS`**: it is how GTK finds icon themes and
   GSettings schemas, and pinning it would break icon resolution across the integration
   suite. The right fix is a deterministic unit test over the path assembly, which means
   making the directory list a parameter rather than an ambient read.
2. **The Windows config directory.** `config_home_fallback()` resolves through `APPDATA`
   there, which is not pinned and should not be — it is a directory the whole toolchain
   uses. The behavioural hazard (a test reading the developer's real `config.toml`) therefore
   persists on Windows, where the assertion in `Config::load()` is deliberately `unix`-gated
   so it cannot fire on a correct run. Coverage is unaffected there, since step 6 is
   contract-declared non-applicable on Windows.

**The general form, which is the part worth keeping**: coverage produced by the *ambient
environment* rather than by a test is false comfort. It looks like tested code and is
nobody's assertion, so it silently moves with the host — and pinning a variable only
relocates the accident. `config_home_fallback()` illustrates the trade honestly: pinning
`XDG_CONFIG_HOME` made its three lines *uncovered* on every host, which is worse-looking and
strictly more truthful, because nothing ever tested them. A deterministic test is the real
answer in all three places.

**Consequence for the ratchet — now bounded rather than tracked**: `FLOOR` is a whole
number and advances one whole point at a time (POLICY step 6). That is deliberately wider
than this entry's residual: ~0.02pt cannot move a threshold quoted in points, so the two
paths above no longer put the gate at risk of a false red. They still cost something real —
the floor cannot rise to the next integer until coverage clears it *with margin on every
host*, and a residual that moves the figure is exactly what eats that margin — so closing
them is still worth doing. It is no longer urgent. `scripts/coverage.sh` is the source of
truth for the value and carries the arithmetic; ScrAP-123 carries the lesson.

---

## Q. A complexity guard measures an exponent with a wall clock, and flakes on a loaded machine

**Severity**: Low (a false red, never a false green — it cannot hide a regression, only
invent one; but a required pipeline step goes red on a correct tree, and the failure text
accuses a specific, already-fixed regression by name, so the next reader starts by
re-auditing code that is fine)

`renderer::normalize`'s `tab_normalisation_over_a_single_enormous_line_grows_linearly`
times normalisation of 128 KiB and 512 KiB inputs and asserts the ratio is `< 8.0`, on the
reasoning that the *exponent* is the property that regressed and a ratio is
machine-independent where a wall-clock bound is "either flaky or blind". The reasoning is
right about the property and wrong about the immunity.

**Measured, on two hosts of the same platform:**

- *Hosted Linux runner*: `8.7x (6.32ms -> 54.918ms)`, failing the pipeline at step 4. The
  same commit passes locally 5 runs out of 5, and a second CI run of the identical tree
  **passed** — so it is intermittent rather than a property of that machine. The tell is in
  the absolute numbers: the runner's small sample is ~6.3 ms where this box is well under a
  millisecond, so CI is not merely noisier, it is roughly an order of magnitude slower per
  byte. A ratio taken from a 6 ms denominator on a shared vCPU has scheduling jitter of the
  same order as the signal it is measuring.
- *Reference host*: one failure in 40 consecutive `scripts/coverage.sh` runs, 2026-08-09 —
  `normalisation grew 8.1x for 4x the input (4.814539ms -> 38.939987ms)`. Same shape, much
  rarer. The instrumented (llvm-cov) build widens the window further.

**A ratio is machine-independent only while both samples sit in the same regime.** Two
plausible contributors, and they are not exclusive: scheduler preemption on a shared core,
and the 512 KiB input (plus an equal-sized output) crossing a cache boundary the 128 KiB one
does not — on a smaller-cache machine the larger sample pays a per-byte penalty the smaller
never sees, and the ratio inflates with the exponent unchanged.

Not the same defect as the SIGSEGV that used to share this symptom (a coverage run going
red about one time in three); that one was the fatal-signal handler outliving its own
test and is fixed — see ScrAP-265. This is the residue that reproduced *instead* of it.

**Options, roughly in order of honesty:**

1. **Count operations, not time.** The property is algorithmic, so measure the algorithm —
   instrument the line walk with a step counter and assert *that* grows linearly.
   Machine-independent by construction rather than by hope. Costs a counter reachable from
   tests.
2. **Move both inputs outside cache** (e.g. 4 MiB and 16 MiB) so both are bandwidth-bound and
   the ratio reflects the exponent again. Simple; costs run time.
3. **Best-of-N per size.** Timing noise is one-sided, so a minimum is the robust estimator.
   Helps with preemption, does nothing about a cache-regime difference.
4. **Widen the threshold.** Cheapest and the worst: 8.7 already sits between linear (~4x) and
   quadratic (~16x), so widening spends the discrimination the guard exists for.

Whichever is taken, take it for both: the same shape is used by at least one sibling guard
(`annotate::scan`'s), so the choice is not local to this test.

Do **not** simply delete it — it guards a real regression that has happened once (a per-tab
backwards line walk, pre-fix cost measured in tens of seconds). The companion absolute
ceiling (3 s for 512 KiB) still holds and is not implicated.

**Workaround**: re-run. A ratio between 8 and roughly 10 on an otherwise-green suite is
this issue; a genuine return of the quadratic walk reads ~16x and does not come and go.

**The sibling guard has now been measured failing too, and it is already best-of-N**
(2026-08-21, reference host, `cargo test` step 4 of a routine pipeline run):
`annotate::scan::tests::extraction_over_adversarial_input_grows_linearly_not_quadratically`
went red once and passed on an immediate isolated re-run of the same binary. That matters
for the choice above, because it is evidence *against* option 3 — the annotation guard
already takes the best of N repetitions per sample, and a preemption still moved the ratio
across the threshold, so best-of-N narrows the window rather than closing it. Options 1 and
2 are unaffected by this measurement, and counting operations is still the only one that
stops being a timing test.

---

## H. macOS: the preview's hover cursor sometimes does not take over body text or a link

**Severity**: Low (nothing is unreachable — links still activate on click and the copy
button still copies; what is lost is the *affordance*, i.e. every link on the page looks
un-clickable to a macOS reader until they try it)

**READ THIS FIRST: it is INTERMITTENT, and that was established late.** The table below is
what was measured, and every cell of it is real — but a later cold launch, same fixture,
same point, same procedure, produced the *correct* text-beam where two earlier independent
cold launches had produced the arrow. So the macOS column is not reliably reproducible, and
any theory built only on the table (including the two this entry has already discarded)
will over-explain it. Whatever gates this is not captured by "cold versus warm", and is not
named here because it is not known.

MEASURED on macOS (GTK 4.22.4/Quartz/Homebrew) against the identical build on Linux
(GTK 4.6.9/X11), same fixtures, cursor identity read by name rather than judged from a
screenshot — `XFixesGetCursorImage` on X11, screenshots on Quartz:

| pointer over | asks for | Linux/X11 | Windows/GDK-Win32 | macOS/Quartz |
|---|---|---|---|---|
| plain body text | `text` | `text` | IBEAM | **arrow** |
| a body link | `pointer` | `pointer` | HAND | **arrow** |
| a task checkbox | `pointer` | `pointer` | HAND | hand |
| a code-block copy button | `pointer` | `pointer` | HAND | hand |

**Three platforms, and macOS is the outlier** — Windows read via `GetCursorInfo().hCursor`
against `LoadCursorW(NULL, IDC_*)` handles after a settle, with the copy-button cell
corroborated by accent-pixel count so the pointer was provably on the button when the
handle was sampled.

**All four go through one `set_cursor_from_name` call in one motion handler**
(`preview::interactions::wire_link_gestures`); only the string differs. So this is not
about links, and not about any hit-test: two of the four cells take effect on Quartz and
two do not.

**Ruled out, measured, not assumed.** The first hypothesis was that GTK's tooltip resets
the cursor — a link is the only one of the four that makes `query-tooltip` return true and
put a window on screen. Sampled at ~200 ms (pre-tooltip) and ~2000 ms (tooltip visibly up),
same pointer position, no re-entry: **arrow both times**. The tooltip is innocent; the
cursor never takes at all on those two cells.

**Standing hypothesis, NOT confirmed, and now bounded to one backend.** The only
structural difference between the working and failing cells is that the checkbox and
copy-button branches also `queue_draw()` when the hovered identity changes, while the link
and plain-text branches draw nothing — suggesting a cursor set via `gtk_widget_set_cursor`
does not take effect until something invalidates the surface. **The Windows result refutes
the general form**: there the two "draw nothing" cells get their cursors, and get two
*different* ones (IBEAM and HAND), so this is not a property of GDK backends at large. If
it holds anywhere it holds on Quartz alone.

**TWO HYPOTHESES HAVE BEEN TRIED AND NEITHER SURVIVED. Do not re-derive them.**

1. *The tooltip resets the cursor* — a link is the only cell that makes `query-tooltip`
   return true. Refuted by sampling at ~200 ms (pre-tooltip) and ~2000 ms (tooltip visibly
   up): arrow both times.
2. *The cursor does not take until the surface is repainted, so only the cells whose hover
   fires a `queue_draw` work* — which fitted every observation at the time, including the
   original three-way result (run with a fresh instance per target, so the "working" cells
   are exactly the ones that repaint themselves). Refuted twice over: on Windows the two
   "draw nothing" cells get their cursors and get two *different* ones, so it is not a GDK
   property; and the motion-only test that would have confirmed it on Quartz — approach one
   spot of plain text from other plain text versus straight off the copy button — returned
   the correct text-beam **both** ways, on an instance where plain text had already started
   behaving before the test began.

**What is left is the intermittency itself, and it is not yet characterised.** A cold
launch has produced the arrow twice (independently, same instance) and the correct
text-beam once (a different instance), with no known difference in procedure. Until that
reproduces on demand there is nothing to hand a researcher: a mechanism proposed against an
observation this unstable will fit and still be wrong, which is how both hypotheses above
were reached. **The next useful step is a reproduction rate, not a theory** — the same cold
launch and single hover, repeated enough times to say how often it bites, which turns an
anecdote into something a mechanism can be tested against.

**Pre-existing, and older than the affordance that exposed it.** Nothing here was
introduced by the code-block copy button; that button is one of the two cells that *works*,
and the pattern only became visible once an affordance existed that happens to repaint on
hover. Do not file it against that change.

**Dead end, so it is not re-attempted.** Forcing a repaint through the macOS menu bar
failed three ways: keyboard-only menu navigation quit the application partway through;
`System Events` `click menu item` on the theme's leaf entry failed `-1728` reproducibly
with and without its emoji prefix, on a fresh instance, *while querying that same item's
name succeeded*; and a raw-coordinate fallback both moved the pointer (which the test
forbids) and missed. A theme switch would also have been ambiguous even if it had landed —
it re-renders the whole preview, so an arrow afterwards could mean either "the repaint did
not help" or "the re-render reset the cursor".

**Harness note for whoever picks this up.** "Did the cursor change" is not a question a
screenshot answers reliably on either platform. On X11 read the cursor's *name* directly
(`XFixesGetCursorImage` via ctypes — it returns `b'pointer'` / `b'text'`). On macOS,
`CGSCurrentCursorSeed` was tried as a corroborating signal and is **unusable the way it was
built**: the seed increments on `screencapture -C` itself, measured by running the identical
sampling loop over blank space with no hover target and seeing the same climb. An instrument
inside its own measurement, and the artefact it produced happened to match the hypothesis
under test.

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

## J. A paragraph that mixes fonts lays out wider than the wrap width it was given

**Severity**: Closed (no public API at the GTK 4.6 floor makes the layout report a width the
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
leaves the preview stuck blank until a manual resize (ScrAP-22, ScrAP-23).

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
  shrink to fit (ScrAP-23a).

**Mitigation options**:
- Accept the limitation (chosen). A reader who hits it can resize the window a little; the
  defect is a property of the width, so any nearby width clears it.
- Revisit if the toolkit floor rises — this was checked against GTK 4.12 and the width
  computation is unchanged, so a fix would have to come from Pango's collapse logic rather than
  from GTK.
- Revisit if the clamp above stops being a symptom gate — if a way appears to distinguish a
  hanging space from a clipped glyph, the clamp becomes safe and this reopens.

## M. The Windows installer's Visual C++ runtime bootstrapper is unverified

**Severity**: Medium (a first-run failure on a clean machine, and the last thing the
installer does is launch the app — so it reads as "it would not install")

`scribobulate.exe` and the staged GTK tree import `VCRUNTIME140.dll` (plus
`VCRUNTIME140_1.dll`, via `cairo-2.dll`). Windows does not ship it; the
`api-ms-win-crt-*` imports beside it are the UCRT, which it does. The installer neither
installs it nor checks for it, so on a machine that has never had a Visual C++
redistributable the app cannot start. `scribobulate.iss`'s `[Run]` section launches the
app post-install, so the failure is the last thing the user sees.

**MEASURED** by the Windows seat (`dumpbin /DEPENDENTS`, VS2022 14.44.35207): 33 staged
modules import `VCRUNTIME140.dll`; zero CRT DLLs are staged; the gvsbuild prefix has none
to stage. **A standing gap, never a regression** — `git log` on `scribobulate.iss` shows
one commit in its whole history, and `git log -S vcruntime -- packaging/windows/stage.ps1`
is empty, so this line has never shipped it.

⛔ **Do NOT fix this by staging `vcruntime140.dll` into `stage.ps1`.** That is the obvious
move and it is the one the remedy below explicitly reversed: copying the DLLs in makes the
project a redistributor of Microsoft's Distributable Code, whose terms require an
end-user click-through that no file vendored into this repository can present. The licence
problem arrives with the DLLs.

**THE REMEDY IS NOW IN THIS TREE**, landed by the `ci` merge: `scribobulate.iss` carries a
`PrepareToInstall` `[Code]` block that runs Microsoft's own `vc_redist.x64.exe` when a
registry probe finds the runtime absent or below the embedded redist's version, its
`dontcopy` source entry, and the redist discovery in `package.ps1`. Running Microsoft's
installer is what satisfies the click-through, which is why that shape was chosen — the
project never becomes a redistributor. The `stage.ps1` half of `1fd4f5c` is *removal* of an
app-local copy, and it merged as such: nothing in the staged tree copies a CRT DLL.

**A FIELD DISCRIMINATOR, so a future report can be placed without a clean image.** The
bootstrapper is EMBEDDED, so it shows up in the artefact's size: a `ci`-line installer
measures ~37.7 MB (39,509,846 bytes, measured by the Windows seat on 6604ae5) against
~15.7 MB for a master-line build with no bootstrapper. Cite the SIZE CLASS, never the
constant — the same build shape already moved from 38,595,643 bytes at `1fd4f5c`. That is
also independent evidence the bootstrapper is WIRED rather than merely present in the
`.iss`, which the `.iss` alone cannot show.

**INDEPENDENTLY CONFIRMED FROM CI, which closes the weaker half.** The hosted Windows
runner's artefact measures 39,146,361 B (run 33357529291), squarely the bootstrapper size
class — so the artefact CI publishes demonstrably carries `vc_redist.x64.exe`, established
from a machine that is not the Windows seat's own. That answers "does the shipped artefact
contain the remedy at all" and leaves untouched the question below.

**WHY THIS ENTRY IS STILL OPEN, and it is the part to read before closing it: the remedy
has never been verified against the condition it exists for.** Every machine able to build
this project already has the CRT, so a staged launch on a build box proves nothing either
way. The observation that means something is a PAIR — runtime absent with the bootstrapper
disabled must fail to start, and the bootstrapper must then make it start — and that needs
a clean Windows image no seat currently has. An unverified remedy in the tree is not a
smaller problem than one on a branch; it is the same problem wearing a green tick, which is
why the severity is unchanged. Two live possibilities also remain open: the original report
may have come from a `ci` build, in which case the fault is *in* the bootstrapper (a 32-bit
Setup reading a redirected registry view, a declined elevation, a redist below the compiled
floor) rather than in its absence.

**The attribution half is discharged.** `THIRD-PARTY-LICENSES.md` is now generated from
`notices/*.md` at build time, and `notices/20-msvc.md` covers the embedded
`vc_redist.x64.exe`. That was the other obligation this entry was carrying; only the
verification remains.

---

## N. A large SVG re-decodes on every render, on the main thread

**Severity**: Medium. Not a correctness defect and nothing is lost, but it is a visible
stall on an ordinary document, and it multiplies with exactly the interactions that
re-render (zoom, disclosure toggle, typing in split mode).

**MEASURED** on the Linux reference host (`probes/svg-rasterise-rs`, librsvg 2.52.5 /
gdk-pixbuf 2.42.8), against `sdd/system-overview.svg` — this project's own architecture
diagram, 1000×1112:

| Operation | Cost |
|---|---|
| `Pixbuf::file_info` (the header probe) | 0.7 ms |
| `Pixbuf::from_file` (natural size) | 239 ms |
| `Pixbuf::from_file_at_scale` at 3× | 227-294 ms |

A raster image of comparable dimensions costs a fraction of a millisecond to probe and a
few milliseconds to decode. The cost is librsvg parsing and rendering the document, and it
is paid **per render**, on the GTK main thread, because the local-image path has no cache
of any kind — `renderer::start::load_texture` decodes afresh every time the walk reaches
the image tag.

The macOS seat measured the same file at roughly half these figures on Homebrew GTK 4.22.4
(`from_file` 103 ms, `at_scale` at 3× 111 ms), so the cost is real on both platforms and
Linux is the worse of the two. Run-to-run spread on one host is wide enough that the 3×
render and the natural-size decode cannot be reliably separated.

**This is pre-existing and was not introduced by the zoom re-render.** The natural-size
decode already cost this; asking for a 3× raster measures about the same, which is the
counter-intuitive part and worth knowing before anyone assumes the zoom work caused it.
What the zoom work did was make renders more frequent and give the cost a second reason to
be noticed.

**Two remedies, and they compose.** A process-wide cache keyed on the source path, its
mtime and the quantised target width would make every render after the first free — the
zoom ladder has about six distinct steps, so the working set is small, and `imagecache`
(remote textures) and `sprite`'s `RESAMPLED` map are both this shape already. Decoding off
the main thread is the larger fix and the loader permits it: the SVG loader declares
`GDK_PIXBUF_FORMAT_THREADSAFE`. The cache is the cheap half and should be measured first.

⚠ **Do not reach for the header probe as a cheap guard.** `Pixbuf::file_info` on an SVG
parses the whole document and merely skips the render (researcher-measured at 27 ms on a
6000-element file, against 0.078 ms for a raster header sniff), so it is not the free
question its raster behaviour suggests.
