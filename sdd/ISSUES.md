# Known Issues

| ID | Issue | Severity |
|----|-------|----------|
| A | Tables are selection islands; cells are individually selectable but not part of the continuous buffer | Closed |
| D | A `~~strikethrough~~` fence that wraps other inline markup (`~~a **bold** b~~`) renders the `~~` literally | Low |
| E | A running instance doesn't repaint when the desktop switches dark↔light on KDE/X11; the new scheme only applies on restart | Low |
| G | A large document leaves the process spinning a CPU core at ~100% while idle — a GTK/Pango relayout pass that re-shapes text every main-loop iteration and never converges | High |
| N | A click that lands *inside* an existing preview selection never reaches the pane's own click affordances — the first click on a link/marker/checkbox under a selection does nothing | Low |
| O | A GTK4/Quartz autorelease-pool crash SIGABRTs the macOS integration suite in about two runs in three, most often on the one focus-churning test | Medium |
| Q | Two wall-clock growth-ratio guards (tab normalisation, annotation extraction) go red on a loaded machine — the ratio is scheduler noise on a small baseline, not an exponent | Low |
| R | macOS only, INTERMITTENT: the preview's hover cursor sometimes does not take over body text or a link, showing the default arrow; the drawn affordances that repaint on hover are always correct | Low |
| S | macOS only: every `GtkFileChooserNative` invocation (Open, Save, Export) grows RSS by ~1 MB and does not give it back; no plateau over 20 cycles. NOT a native-dialog property — Windows uses a native panel too and plateaus | Medium |
| T | Two File-menu mnemonics collide on `l` (`Save A_ll` vs `_Load Unsafe Linked Documents`), and the uniqueness test that exists to catch exactly this never sees eight of the table's entries | Low |
| U | Option+Left/Right word navigation on macOS only reaches the main document editor; every other in-app text field is unchanged | Low |
| V | The inline-tab pre-pass reaches two of the four parse sites, though its doc says "every parse site" — the outline and copy-as-Markdown parse the raw source | Low |

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

### Upstream bug — GTK GitLab filing recommendation

**Verdict (researcher-verified, 2026-06-30):** This is a **genuine GTK bug** — a wrong-predicate
oversight in `gtk_text_view_allocate_children`, not intentional design. Filing an upstream issue is
**justified and recommended** (moderate-to-good acceptance odds; fresh GNOME/GTK search found no
matching open issue). The bug is byte-identical from GTK 4.6.9 through current main (4.23.2).
Findings doc: `~/Documents/Projects/AI/Research/Gtk4Rust/gtk-issue-draft-anchored-container-grandchild-stale-paint.md`

**Keep the opaque-base workaround regardless** — even if the upstream patch lands it will not
backport to 4.6/4.8/etc.

<details>
<summary>Ready-to-file GTK GitLab issue text (paste as-is)</summary>

````markdown
**Title:**
GtkTextView: anchored *container* child's region is never invalidated when a *grandchild* changes (allocate_children gate uses self-only alloc_needed)

**Body:**

## Summary
When a widget is anchored in a GtkTextView via a child anchor, and that anchored widget is a *container*, a visual change to one of its *descendants* (e.g. `gtk_label_set_attributes()` on a grandchild label) is not repainted. The stale pixels remain until an unrelated event forces the view to re-validate the line (a scroll, or any buffer mutation). Anchoring the styled widget directly (so it is the anchored widget itself, not a grandchild) does NOT exhibit the bug — which pinpoints the cause.

## Steps to reproduce
1. Build and run the minimal C program below.
2. Click "Set BG" → a yellow background appears behind CELL TEXT.
3. Click "Clear BG" → `gtk_label_set_attributes(label, NULL)` clears the model, but the yellow background **stays painted**.
4. Scroll the GtkTextView, or type into the buffer → the yellow finally disappears.

**Expected**: clearing the attributes repaints the anchored region immediately (step 3).
**Actual**: the region is not invalidated; the old paint persists until step 4.

*Diagnostic asymmetry*: if you anchor the `GtkLabel` directly (replace the `GtkBox cell` with the `label` itself), the bug disappears — the region repaints correctly. The bug only appears when the restyled widget is a *descendant* of the anchored widget.

## Minimal reproducer (C)

```c
/* Build: cc stale-anchor-bg.c -o stale-anchor-bg $(pkg-config --cflags --libs gtk4)
 * Run:   GSK_RENDERER=cairo ./stale-anchor-bg
 */
#include <gtk/gtk.h>

static GtkWidget *label;

static void set_bg (GtkButton *b, gpointer u) {
  PangoAttrList *al = pango_attr_list_new ();
  pango_attr_list_insert (al, pango_attr_background_new (65535, 65535, 0)); /* yellow */
  gtk_label_set_attributes (GTK_LABEL (label), al);
  pango_attr_list_unref (al);
}

static void clear_bg (GtkButton *b, gpointer u) {
  gtk_label_set_attributes (GTK_LABEL (label), NULL); /* <-- background stays painted */
}

static void activate (GtkApplication *app, gpointer u) {
  GtkWidget *win = gtk_application_window_new (app);
  gtk_window_set_default_size (GTK_WINDOW (win), 400, 300);

  GtkWidget *box    = gtk_box_new (GTK_ORIENTATION_VERTICAL, 6);
  GtkWidget *btnset = gtk_button_new_with_label ("Set BG");
  GtkWidget *btnclr = gtk_button_new_with_label ("Clear BG");
  GtkWidget *tv     = gtk_text_view_new ();

  GtkTextBuffer *buf = gtk_text_view_get_buffer (GTK_TEXT_VIEW (tv));
  GtkTextIter it;
  gtk_text_buffer_get_start_iter (buf, &it);
  gtk_text_buffer_insert (buf, &it, "Anchored container child below:\n", -1);

  /* Anchored CONTAINER child; the grandchild label is what we restyle. */
  GtkTextChildAnchor *anchor = gtk_text_buffer_create_child_anchor (buf, &it);
  GtkWidget *cell = gtk_box_new (GTK_ORIENTATION_HORIZONTAL, 0);
  label = gtk_label_new ("CELL TEXT");
  gtk_box_append (GTK_BOX (cell), label);
  gtk_text_view_add_child_at_anchor (GTK_TEXT_VIEW (tv), cell, anchor);

  g_signal_connect (btnset, "clicked", G_CALLBACK (set_bg),   NULL);
  g_signal_connect (btnclr, "clicked", G_CALLBACK (clear_bg), NULL);

  gtk_box_append (GTK_BOX (box), btnset);
  gtk_box_append (GTK_BOX (box), btnclr);
  gtk_box_append (GTK_BOX (box), tv);
  gtk_window_set_child (GTK_WINDOW (win), box);
  gtk_window_present (GTK_WINDOW (win));
}

int main (int argc, char **argv) {
  GtkApplication *app = gtk_application_new ("org.example.staleanchorbg",
                                             G_APPLICATION_DEFAULT_FLAGS);
  g_signal_connect (app, "activate", G_CALLBACK (activate), NULL);
  int r = g_application_run (G_APPLICATION (app), argc, argv);
  g_object_unref (app);
  return r;
}
```

## Root cause

`gtk_text_view_allocate_children()` (`gtk/gtktextview.c`) invalidates the layout line around an anchor only when the anchored child reports it needs allocation:

```c
/* gtk/gtktextview.c — main (4.23.2): line 4574  |  4.6.9: line 4431 */
if (_gtk_widget_get_alloc_needed (child->widget))
  {
    GtkTextIter end = child_loc;
    gtk_text_iter_forward_char (&end);
    gtk_text_layout_invalidate (priv->layout, &child_loc, &end);
  }
```

`_gtk_widget_get_alloc_needed()` returns only the widget's **own** flag:

```c
/* gtk/gtkwidget.c — main: 11037  |  4.6.9: 10525 */
_gtk_widget_get_alloc_needed (GtkWidget *widget) { return widget->priv->alloc_needed; }
```

But a change to a descendant marks `alloc_needed` only on **that descendant**; ancestors receive only `alloc_needed_on_child`:

```c
/* gtk/gtkwidget.c — main: 11045  |  4.6.9: 10533 */
gtk_widget_set_alloc_needed (GtkWidget *widget) {
  widget->priv->alloc_needed = TRUE;
  do {
    if (priv->alloc_needed_on_child) break;
    priv->alloc_needed_on_child = TRUE;   /* ancestors get THIS, not alloc_needed */
    ...
    widget = priv->parent;
    ...
  } while (TRUE);
}
```

So when `gtk_label_set_attributes()` (which `queue_resize`s the label) runs on a grandchild, the anchored container `child->widget` ends up with `alloc_needed == FALSE` (only `alloc_needed_on_child == TRUE`). The gate is skipped, the layout line is never invalidated, the anchor's cached display line is reused, and the region is never re-snapshotted — hence the stale paint. When the anchored widget IS the restyled label, its own `alloc_needed` is TRUE, the gate fires, and the region repaints correctly (the diagnostic asymmetry above).

## Suggested fix (one line)

Use the descendant-aware predicate `gtk_widget_needs_allocate()` — which already ORs in `alloc_needed_on_child` — in place of the self-only `_gtk_widget_get_alloc_needed()` at the gate:

```c
/* gtk/gtktextview.c, gtk_text_view_allocate_children
 * main (4.23.2): line 4574  |  4.6.9: line 4431 */
- if (_gtk_widget_get_alloc_needed (child->widget))
+ if (gtk_widget_needs_allocate (child->widget))
```

`gtk_widget_needs_allocate()` is defined a few lines from the getter the gate currently uses (`gtk/gtkwidget.c` — main: 11077, 4.6.9: 10561) and already returns `resize_queued || alloc_needed || alloc_needed_on_child`.

*Trade-off*: this will invalidate+revalidate the anchor line whenever any descendant of an anchored **container** needs allocation (marginally more layout work for container anchors), but that is exactly the condition under which the region must be re-validated.

## Affected versions

Verified by source reading that the gate and the `alloc_needed` / `alloc_needed_on_child` machinery are identical from **4.6.9 through current main (4.23.2)**. Expected on all of 4.6 / 4.8 / 4.10 / 4.12 / 4.14 / 4.16 / 4.18 / 4.20 / 4.22 and dev.

## Caveat

The suggested fix is a code-reading proposal — it has not been compiled or runtime-tested. The root-cause analysis and the reproducer are verified; the one-line change is offered for a maintainer to validate.
````

</details>



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

**Severity**: Medium (unbounded within everything measured — no plateau over 20
cycles — and it affects every file dialog in the application, not one feature. Not a
crash, not data loss, and slow enough that an ordinary session will not notice it.)

**macOS only, MEASURED.** Opening a `GtkFileChooserNative` and **cancelling it** —
never completing an operation — grows the process's RSS by roughly **0.9–1.4 MB per
invocation**, steadily, with no plateau across 20 cycles. A large one-time jump
(~24–28 MB) accompanies the *first* construction in a process and is separate: that
is the native-panel machinery warming up and it does not recur.

Measured on macOS 4.22.4/Quartz, release build, window fixed, RSS via `ps -o rss=`:

| probe | cycles | growth | per cycle |
|---|---|---|---|
| Export chooser, opened and **cancelled** | 19 | 170528 → 192592 KB | ~1160 KB |
| File ▸ Open chooser, opened and **cancelled**, fresh instance | 19 | 169776 → 186304 KB | ~870 KB |
| Full export cycle (chooser + real export, PDF) | 20 | 175648 → 198784 KB | ~1150 KB |
| Full export cycle (chooser + real export, HTML) | 10 | 197440 → 209504 KB | ~1206 KB |

**It is not the export path**, and the numbers above are what establish that: a
cancelled chooser that runs no export at all grows at the same rate as a completed
export, and the plain **Open** dialog — which has nothing to do with exporting —
grows the same way. Export is simply a new place that opens a chooser repeatedly,
which is how this surfaced. Do not file it against the export feature.

**It is not a classic leak.** `leaks <pid>` against a live instance after 30 combined
cycles reported **472 leaks / ~103 KB**, against an RSS climb of ~34 MB over the same
run — two orders of magnitude apart. A second instance measured 353 leaks / ~96 KB
against a comparable climb. Whatever is accumulating is still **reachable**, so it
reads as fragmentation or a growing cache (font map, panel construction/teardown, some
platform-side pool) rather than unreachable memory.

**The application's own reference discipline is not the suspect.** Both call sites —
`window/export.rs`'s `choose_destination` and `app/appactions.rs`'s open action — go
through `saferizer::native_dialog::NativeDialogHolder`, which takes exactly one
external strong reference and drops it once when `response` fires, and both call
`destroy()` on the dialog in the response handler. That is the shape ScrAP-41
prescribes, and it is the same code on every platform.

**Linux does not reproduce it — with an important caveat about what was measured.**
An in-process probe constructing, showing, cancelling and dropping 40 `FileChooserNative`
dialogs under Xvfb (GTK 4.6, release, RSS via `/proc/self/statm`) ended **+2612 KB over
40 cycles, ~65 KB/cycle, and non-monotonic** — the reading fell twice (+2936 KB at cycle
10, +664 at 20, +700 at 30, +2612 at 40), which is noise around a flat baseline rather
than growth. **But the caveat is the whole value of the result:** with no portal present,
GTK on Linux backs `FileChooserNative` with its own in-process `GtkFileChooserDialog`
widget (the run emits "GtkDialog mapped without a transient parent"), whereas macOS backs
it with a real `NSOpenPanel`/`NSSavePanel`. So this is a negative about **a different
implementation**, not about the same code path behaving differently. It narrows the
suspect to the platform panel; it does not exonerate anything portable.

**Windows does not reproduce it either — and that negative is the informative one.** The
Linux non-reproduction came with a caveat: GTK backs `FileChooserNative` with its own
in-process widget there, so it measured a different implementation. **Windows closes that
gap.** It uses a genuine native panel — the Win32 common dialog, a separate shell-owned
window, gated per cycle by reading its title — and over **40 open-and-cancel invocations**
grew **+5.9 MB total, decelerating** (+2.9 MB in the first five cycles, +1.6 MB across
cycles 20→40): lazy one-time initialisation of the shell dialog machinery, not a
per-invocation cost. macOS's sustained ~1 MB/invocation would have been ≈ +40 MB over the
same run.

So **"a native panel implies the climb" is refuted**: two native-panel platforms, opposite
behaviour. Whatever this is, it is specific to macOS's `NSOpenPanel`/`NSSavePanel` path
rather than a property of `GtkFileChooserNative`'s native-dialog architecture. That is a
real narrowing of the search and it should be the starting point for whoever attributes it.

**Next step**, when someone picks this up: attribute the growth before proposing a fix.
`leaks` has already answered "not unreachable", so the useful instruments are
`heap <pid>` / `malloc_history` with `MallocStackLogging=1`, or `vmmap` diffed across
cycles to see *which* region grows. A fix aimed at the Rust reference handling would be
aimed at the wrong layer on the evidence so far.

**Consequence for the export rubrics**: TDD 25.15 ("an export does not move the
footprint") **passes on Windows** — 15 verified exports cost +4.3 MB against 40 chooser-only
cycles costing +5.9 MB, so the export contributes nothing distinguishable from opening the
chooser, and the per-process GPU counters read a flat 0 B throughout. It remains
**unverified on macOS**, not failing — export's own contribution to RSS
cannot be separated from the chooser's while every measurement cycle opens one. The GPU
half of that rubric *is* verified: the process never appears as a GPU client, before or
after 30 export cycles, at either document size.

## T. Two File-menu mnemonics collide, and the guard that exists to catch it cannot see them

**Severity**: Low (a colliding access key makes one of the two items unreachable by
`Alt`+letter and is a nuisance, not a defect in any command's behaviour. Both items
remain reachable by pointer and by their accelerators.)

**The collision, MEASURED.** In the File popover, `Save All` is marked `Save A_ll` and
`Load Unsafe Linked Documents` is marked `_Load Unsafe Linked Documents`. **Both take
`l`.** Both are live `FILE_CMDS` entries (`src/app/commands.rs`) and both appear in the
File menu today.

**Why nothing catches it.** `menu_mnemonics_unique_per_popover`
(`src/app/mnemonics.rs`) exists for precisely this property, and it passes — because
its per-popover label lists are **not derived from the menu**, they are a second,
hand-maintained copy, and **eight entries of `MENU_MNEMONICS` appear in no list at
all**:

`Save All` · `Rename…` · `Export` · `PDF` · `HTML` · `Edit Link…` · `Edit Image…` ·
`Reading Theme`

`Save All` is one of them, so the colliding pair is never compared. Adding the missing
File entries to the test makes it fail immediately and by name:

```
File popover: access key 'l' collides — "Save All" vs "Load Unsafe Linked Documents"
```

**The guard is green because its input set is incomplete, not because the property
holds** — the same shape as the false-PASS family in the anti-pattern register, arriving
through a hand-maintained list rather than through a wrong assertion.

**Two things to fix, and the second matters more than the first.**

1. **Resolve the collision.** `Load Unsafe Linked Documents` has `U`, `n`, `k` and `d`
   free; `Save All` has little room, and `Save A_ll` pairs visually with `Save _As…`,
   which is what a reader scans for. So moving the *Load* item is the lower-cost change.
   This is user-visible, so it ships with its `tests/MANUAL-TEST.md` check
   (build-pipeline step 7).
2. **Close the guard's input gap**, and prefer *deriving* the popover lists from the
   menu model over extending the hand-maintained copy — a second copy is how this one
   silently stopped matching, and extending it fixes today's eight while leaving the
   mechanism intact. The `Export` submenu needs to appear as **its own popover**, since
   `_PDF` and `_HTML` are only claimed to be safe *because* the submenu is a separate
   popover from the File menu one level up, and that claim is currently made in a code
   comment with nothing holding it.

**Not introduced by the export feature.** `_Export`, `_PDF` and `_HTML` are correct and
do not collide with anything; they are simply three of the eight entries the guard never
checks. Found while confirming the export items had mnemonics at all.

## U. Option+Left/Right word navigation on macOS only reaches the main document editor

**Severity**: Low (nothing else claims that key inside these fields, so it is a missing
convenience, not a defect — Ctrl+Left/Right still works everywhere, matching every
platform.)

Fixed this cycle for the surface the report was about: the main editor's Option+Left/
Right now moves the caret by a word (`src/macwordnav.rs`), pre-empting `GtkSourceView`'s
own `move-words` class binding, which used to transpose the word at the cursor instead.

**Every other in-app text field still lacks it.** The annotation comment entry, Go To
Line, and the rename/URL dialogs are plain `GtkEntry`/`GtkText` widgets — GTK does not
bind Option+Left/Right there either, and nothing in this app has wired the equivalent
key controller onto them, so a Mac reader's Option+Left in one of those fields simply
does nothing.

**Mitigation options.** Generalize `macwordnav::word_movement` (already a pure,
display-free decision, kept reusable for exactly this) into a small wiring helper for
`GtkText`/`GtkEditable`, and attach it at each field's construction site — there is no
single choke point today the way `build_tab_editor` is for the main editor, so it would
need one call per surface (or a shared builder wrapper, if one gets introduced first).

## V. The inline-tab pre-pass reaches two of the four parse sites

**Severity**: Low (it takes a hard TAB inside a GFM table to observe anything, the
preview — the surface a reader actually looks at — is one of the two sites that IS
covered, and the divergence has not been shown to produce a wrong result on either
uncovered surface. Found by inspection during the lone-CR review, not from a report.)

`renderer::normalize_inline_tabs` (ScrAP-75) documents itself as "Used at every parse
site (preview render, outline, copymap)". It is called at exactly two: `preview/build.rs`
and `export/doc.rs`. `outline.rs` and `copymap.rs` both build a `pulldown_cmark::Parser`
over the raw source instead.

**This is an instance of a class, not a one-off.** A doc comment that asserts a behaviour
the code does not implement is worse than no comment, because it **terminates the audit it
appears to serve**: a reader checking whether every parse site is covered finds the
sentence saying they are and stops. A second instance of exactly this was found
independently in `export/pdf.rs` during the same period — a `table()` rustdoc claiming
conformance to two TDD rubrics that nothing in the file implemented. The general form is
worth stating wherever it recurs: **a comment claiming a contract is satisfied is a claim
that needs a TEST behind it, not prose.**

**Why it might matter.** The preview and `copymap` are supposed to be two readings of the
same document, and `copymap` is what turns a preview selection back into Markdown source.
Feed them a document with a tab-delimited GFM table and they disagree about its
*structure* — the preview sees a table, `copymap` sees a paragraph — so a copy spanning
that region can resolve to the wrong span. Offsets themselves stay aligned, since the
pre-pass is length-preserving by design, which is why this is a structural divergence and
not a drift.

**Not verified.** Nobody has produced a user-visible wrong result from it; the reasoning
above is inference from the call sites. Establishing whether the defect is real is the
first step, not the fix — and a check that a tab-delimited table copies correctly out of
the preview is what would settle it either way.

**Mitigation.** Either route the two uncovered parsers through the pre-pass, or correct
the doc comment to say which sites it actually covers and why the others are exempt. The
second is not a cop-out if the exemption is real: a comment that overstates its reach is
what made this look settled for as long as it did.
