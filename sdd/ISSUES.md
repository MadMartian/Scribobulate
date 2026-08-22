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

| ID | Platform | Scope | Issue | Severity |
|----|----------|-------|-------|----------|
| A | Any | Upstream | Tables are selection islands; cells are individually selectable but not part of the continuous buffer | Closed |
| D | Any | Production | A `~~strikethrough~~` fence that wraps other inline markup (`~~a **bold** b~~`) renders the `~~` literally | Low |
| E | Linux | Production | A running instance doesn't repaint when the desktop switches dark↔light on KDE/X11; the new scheme only applies on restart | Low |
| G | Any | Production | A large document leaves the process spinning a CPU core at ~100% while idle — a GTK/Pango relayout pass that re-shapes text every main-loop iteration and never converges | High |
| N | Any | Production | A click that lands *inside* an existing preview selection never reaches the pane's own click affordances — the first click on a link/marker/checkbox under a selection does nothing | Low |
| O | Mac | Upstream | A GTK4/Quartz autorelease-pool crash SIGABRTs the macOS integration suite in about two runs in three, most often on the one focus-churning test | Medium |
| Q | Any | Test | Two wall-clock growth-ratio guards (tab normalisation, annotation extraction) go red on a loaded machine — the ratio is scheduler noise on a small baseline, not an exponent | Low |
| R | Mac | Production | macOS only, INTERMITTENT: the preview's hover cursor sometimes does not take over body text or a link, showing the default arrow; the drawn affordances that repaint on hover are always correct | Low |
| S | Mac | Upstream | macOS only: every native file-chooser invocation (Open, Save, Export) grows RSS by ~1.1 MB and does not give it back. Roughly four fifths is AppKit's own price for presenting an `NSSavePanel` — reproduced with no GTK in the process — with about a fifth GTK-attributable. Caching the panel upstream would recover ~95% | Medium |
| T | Any | Test | The cross-reference gate is TWO implementations of one rule — `scripts/lint-references.sh` and `.ps1`, ~3,400 lines — kept in step by hand. A single `cargo xtask` binary would retire the duplication and the whole parity apparatus with it | Medium |

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

## D. A strikethrough fence wrapping other inline markup renders the `~~` literally

**Severity**: Low (rare authoring pattern; plain `~~struck~~` is unaffected)

Superscript (`^x^`), subscript (`~x~`) and strikethrough (`~~x~~`) are parsed by this
crate's own tight-syntax scanner (`renderer::scan_scripts`), because pulldown-cmark's
native versions use flanking rules that reject the tight Pandoc syntax authors type and
fragment multi-tilde lines (see ScrAP-66). The scanner runs **per pulldown
`Text` event**. A `~~ … ~~` fence whose content contains *other inline markup* —
`~~a **bold** b~~` — is split by pulldown into `"~~a "`, `Strong("bold")`, `" b~~"`, so
the two `~~` halves never meet in one scanned run and the fence is not recognised: the
`**bold**` still renders bold, but the surrounding `~~` show as literal characters and
no strike is applied. Plain `~~struck text~~` (no nested markup), the overwhelmingly
common case, renders correctly.

A related, rarer edge: `x\^2\^` (escaped literal carets) cannot be distinguished from
`x^2^` after pulldown has consumed the backslash escapes, so it renders as a superscript.

**Mitigation options**:
- **Coalesce adjacent inline events before scanning**: buffer consecutive `Text` (and
  re-emit nested `Strong`/`Emphasis`/etc.) within a paragraph and scan the reassembled
  run, so a fence can span nested markup. Non-trivial control-flow change to the renderer.
- **Accept the limitation (current state)**: nested markup inside strikethrough is rare;
  plain strikethrough and all tight super/subscript work. **TDD 10.10** makes the
  boundary explicit — it blesses the plain `~~struck~~` case as a contract and scopes
  the wrapping case (`~~a **bold** b~~`) OUT as this accepted limitation.

## E. Live desktop dark↔light toggle doesn't repaint a running instance on KDE/X11

**Severity**: Low (cosmetic; the new scheme applies on the next launch, and users
rarely retheme mid-session — but a visible inconsistency with libadwaita/Qt apps,
which *do* update live)

With the app's reading theme left on **System**, switching the KDE global colour
scheme from dark to light (or back) while an instance is running leaves that
instance's chrome unchanged; the new scheme is only picked up when a fresh instance
launches. Confirmed on the operator's live KDE/X11 session (2026-07-19): a running
window stayed dark across a desktop dark→light switch, while a freshly-launched
instance rendered light. Native GNOME/libadwaita and Qt/KDE apps repaint live under
the same toggle, so the gap is app-visible.

**Root cause is a propagation-channel gap, not a missing feature.** The app already
wires a live-update path: it subscribes to `GtkSettings` `gtk-application-prefer-dark-theme`
and `gtk-theme-name` notifications and, on either, re-probes the desktop colours and
re-renders every window. The failure is upstream of that handler — KDE-on-X11 does
**not** push a live colour-scheme change through the **XSettings** channel that backs
those `GtkSettings` properties, so the notify never fires on a running client (a fresh
launch reads the current value once, which is why restart works). The apps that update
live listen on a *different* channel: the XDG desktop portal signal
`org.freedesktop.portal.Settings` → `SettingChanged("org.freedesktop.appearance",
"color-scheme")` (libadwaita's `AdwStyleManager` watches exactly this). Scribobulate is
pure gtk4-rs (no libadwaita), so it isn't listening there.

**The detection channel is now researcher-confirmed (2026-07-19, empirically on the
operator's own KDE/X11 box).** `xdg-desktop-portal-kde` 5.27.11 is installed, running,
and routed (`kde-portals.conf`: `org.freedesktop.impl.portal.Settings=kde`); a direct
`gdbus call … Settings.Read org.freedesktop.appearance color-scheme` returns the live
value. Its source derives `color-scheme` from Qt's `paletteChanged` (no X11/Wayland
branch) and `Q_EMIT`s `SettingChanged`, so the scheme rides the **portal**, exactly
what libadwaita/Qt watch and what our `GtkSettings`/XSettings handler cannot see. The
signal is `SettingChanged(namespace s, key s, value v)` on the session bus at
`org.freedesktop.portal.Desktop` `/org/freedesktop/portal/desktop`, iface
`org.freedesktop.portal.Settings`; value maps `0=no-preference, 1=dark, 2=light`. A
version gotcha for the *initial* read: `Read` double-wraps the variant and `ReadOne`
(single-wrap) exists only at interface **version ≥ 2** (xdg-desktop-portal ≥ 1.18);
the operator's box is version 1, so the initial read needs a double-unwrap (the
`SettingChanged` signal is single-wrapped on every version).

**Two gates remain before the fix can be committed (both need the operator's live
session — a `gdbus monitor --session --dest org.freedesktop.portal.Desktop` while
flipping the scheme):**
1. Confirm the live *push* actually fires (the static `Read` only proves the value is
   readable; the go/no-go is watching `SettingChanged` arrive on the toggle).
2. **Whether the app's existing re-render even reflects the new scheme once the signal
   fires.** The desktop-dark truth comes from `palette::desktop_is_dark()`, which
   probes `GtkStyleContext` colours — and on KDE/X11 *those are also stale* on a live
   flip (GTK never reloads the KDE theme without the XSettings/portal push). So the
   portal handler must likely use the signal's `color-scheme` value **directly** as the
   desktop-dark truth (overriding the stale probe) rather than re-probing; and the
   GTK-themed chrome (editor/toolbar, rendered by the KDE Breeze GTK CSS) may still not
   follow live without forcing a GTK theme reload — the reading-area palette we control
   can, the base-theme chrome is the open question. Confirm on the live session which
   surfaces actually switch before deciding whether the fix is "preview follows" or
   "everything follows".

**Mitigation options**:
- **Subscribe to the XDG desktop-portal `color-scheme` signal** (gio `DBusProxy` on the
  session bus, no new deps) alongside the two existing `GtkSettings` subscriptions, and
  on change drive the desktop-dark truth from the signal value + re-render. Gate on the
  two live checks above.
- **Accept the limitation**: the scheme applies on the next launch, users seldom
  retheme mid-session, and nothing is broken — only slower to follow than native apps.

## G. A large document pegs a CPU core at ~100% while idle (GTK/Pango relayout loop that never converges)

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

**Mitigation options**:
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

## N. A click inside an existing selection never reaches the preview's click affordances

**Severity**: Low (one wasted click, self-correcting — the click clears the selection and
the next one works; no data at risk)

With text selected in the preview, a primary click whose press lands **inside** that
selection does not activate the affordance under it: a link does not open, a margin
comment marker does not open its card, a gutter checkbox does not toggle, a code block's
copy button does not copy. The click clears the selection instead, and a second click
behaves normally.

**Measured, and pre-existing.** Reproduced identically on the binary from before the
complete-click fix (ScrAP-238) and on the one after, under Xvfb: press+release on a
fragment link with a selection covering it produced no scroll on either build, while the
same click with nothing selected navigated on both. So the seam that now pairs press with
release neither introduced nor addresses it.

**Cause — measured, then traced to the two lines that implement it.** Instrumented build,
Xvfb: on the swallowed click the app's gesture receives `pressed` and then **nothing** — no
`released`, and no `cancel` either, which is why it fails silently.

`gtk_text_view_click_gesture_pressed` (`gtktextview.c`, GTK 4.6.9) handles a single
non-touch press whose iter lies inside the selection by claiming the sequence for its own
drag gesture — *"Claim the sequence on the drag gesture, but attach no selection data,
this is a special case to start DnD"* — unconditionally, not gated on the view being
editable. A claim denies every other gesture handling that sequence, and
`gtk_gesture_click_end` (`gtkgestureclick.c`) emits `released` only
`if (current == sequence && state != GTK_EVENT_SEQUENCE_DENIED && interpreted)`. DENIED is
terminal (the same arbitration wall issue A documents), and it is not a cancellation, so
no `cancel` fires to tell the app anything happened.

So the release is not late or misrouted — GTK deliberately withholds it, and the app's
affordance cannot observe the click at all.

**Mitigation options**:
- Claim the sequence ourselves on a press that lands on an affordance — **priced and not
  recommended**. It is the only way to out-rank GTK's claim, and it buys one click at the
  cost of the case it steals from: a press on a link is then no longer available to start
  a drag, so dragging the selection (GTK's DnD) from a point that happens to sit over a
  link stops working, and — since intent is unknowable at press time — a press that does
  turn into a drag would end in nothing happening at all, which is worse than today. The
  press cannot be disambiguated at the moment the decision must be made.
- Leave it, which is the standing choice: the failure is one wasted click that fixes
  itself, and every route past it trades a silent no-op for a silent loss of the drag.


## O. A GTK4/Quartz autorelease-pool crash intermittently SIGABRTs the macOS integration suite

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

## Q. A wall-clock growth-ratio guard fails on a loaded machine

**Severity**: Low (nothing shipped is affected; a required pipeline step goes red on a
correct tree, and the failure text accuses a specific, already-fixed regression by name,
so the next reader starts by re-auditing code that is fine)

`renderer::normalize::normalize_inline_tabs_tests::tab_normalisation_over_a_single_enormous_line_grows_linearly`
asserts that normalising 512 KiB of tabs takes less than **8x** the time of 128 KiB —
4x the input. The intent is sound and deliberately machine-independent: the exponent is
what regressed once (a per-tab backwards line walk), and the test's own doc argues,
correctly, that an absolute wall-clock bound would be "either flaky or blind".

**Measured**: one failure in 40 consecutive `scripts/coverage.sh` runs on the reference
host, 2026-08-09 — `normalisation grew 8.1x for 4x the input (4.814539ms ->
38.939987ms)`. The ratio is a quotient of two wall-clock samples whose **numerator is
about 5 ms**, so a single scheduler preemption of the small run is enough to move it
across a threshold set at 8 against an expectation of 4. The instrumented (llvm-cov)
build widens the window further, and 39 of the 40 runs passed.

Not the same defect as the SIGSEGV that used to share this symptom (a coverage run going
red about one time in three); that one was the fatal-signal handler outliving its own
test and is fixed — see ScrAP-265. This is the residue that reproduced *instead* of it.

**Why it is not fixed here**: the repair is a judgement about how to make a complexity
assertion robust, not a one-line threshold bump, and the same shape is used by at least
one sibling guard (`annotate::scan`'s), so whatever is chosen should be chosen for both.
The candidates, none of them free: take the best of N repetitions rather than one sample
(kills preemption noise, costs runtime); raise the baseline until scheduler noise is a
rounding error (costs runtime, and the absolute-ceiling half of the test already bounds
that); or count a proxy for work done — iterations, comparisons — instead of time, which
is the only variant that is not a timing test at all and is the one worth pricing first.

**Workaround**: re-run. A ratio between 8 and roughly 10 on an otherwise-green suite is
this issue; a genuine return of the quadratic walk reads ~16x and does not come and go.

**The sibling guard has now been measured failing too, and it is already best-of-N**
(2026-08-21, reference host, `cargo test` step 4 of a routine pipeline run):
`annotate::scan::tests::extraction_over_adversarial_input_grows_linearly_not_quadratically`
went red once and passed on an immediate isolated re-run of the same binary. That matters
for the choice above, because it is evidence *against* the cheapest candidate — the
annotation guard takes the best of N repetitions per sample and a preemption still moved
the ratio across the threshold, so best-of-N narrows the window rather than closing it.
The remaining two candidates (raise the baseline, or count a proxy for work done) are
unaffected by this measurement, and counting work done is still the only one that stops
being a timing test.

---

## R. macOS: the preview's hover cursor sometimes does not take over body text or a link

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

## S. Every native file chooser invocation grows RSS on macOS

**Severity**: Medium. Unbounded within everything measured — no plateau observed — but the
cost is overwhelmingly AppKit's own price for presenting an `NSSavePanel`, and it is not
reachable by any change this project can make.

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

## T. The cross-reference gate is two implementations of one rule

**Severity**: Medium. Nothing is broken today and the two ports currently agree; the cost is
that every change to the gate is made twice, and every divergence between them has been found
by a human rather than by a gate.

**Symptom**: `scripts/lint-references.sh` (1,669 lines) and `scripts/lint-references.ps1`
(1,720 lines) implement the same fourteen checks. They hand-sync patterns, six `--self-test`
corpora, file lists, thresholds and exclusions. POLICY § Build pipeline step 9 requires them
to share a pattern and a corpus "string-for-string" and to be diffed by hand via
`--list-scan` / `-ListScan`, because no automated test was thought able to compare them.

**What this has actually cost.** One QA round found seven separate defects that exist only
because there are two ports: carve-outs announced but never applied, a missing provenance
warning, a self-test that could not reach its own executor, two check predicates that
returned opposite verdicts on the same input, an `exclude` prefix matched case-insensitively
on one side, and a recorded parity proof that had gone stale without saying so. Each was real,
each was fixed, and none would have existed with one implementation.

**The premise the second port rests on is FALSE, and it was measured.** Step 9 states that
"neither platform has the other's shell". The Windows seat measured its own box: GNU bash
5.2.37 and Perl 5.38.2, both bundled with Git for Windows, plus a second independent copy from
MSYS2 — which `packaging/windows/README.md` already lists as a genuine build prerequisite. The
existing bash gate runs there **unmodified**: `--self-test` PASS, all checks PASS, exit 0, no
path-separator or `git ls-files` friction, and byte-identical scan output to the PowerShell
port on the same box. It also catches a planted path containing a raw newline, and renders it
more legibly than its twin.

**Decision, operator, 2026-08-21: unify on a `cargo xtask` binary — DEFERRED, not rejected.**
Not bash and not Perl. `cargo` is already invoked by eighteen contract commands and a Rust
toolchain is a prerequisite on all three platforms, so an xtask adds no dependency and no
locate-it step; bash and Perl each add the latter, since none of those tools is on `PATH` on
Windows even where present. Its self-tests become ordinary `cargo test` cases rather than
hand-rolled corpus runners — which is where this round's defects concentrated.

**Scope, when it is picked up.** The two LINT ports only. The two PIPELINE RUNNERS are a
different question and are NOT retired by this: `packaging/windows/pipeline.ps1` exists to set
MSVC/GTK environment and drive steps on Windows, which is not a shell-parity problem.
Retiring the lint duplication also retires the parity apparatus built to police it — the
`--list-scan` diff procedure, the string-for-string corpus rule, and the scan-parity check —
so those come out in the same change rather than being left to describe a comparison that no
longer has two things to compare.
