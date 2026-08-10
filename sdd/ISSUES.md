# Known Issues

| ID | Issue | Severity |
|----|-------|----------|
| A | Tables are selection islands; cells are individually selectable but not part of the continuous buffer | Closed |
| D | A `~~strikethrough~~` fence that wraps other inline markup (`~~a **bold** b~~`) renders the `~~` literally | Low |
| E | A running instance doesn't repaint when the desktop switches dark↔light on KDE/X11; the new scheme only applies on restart | Low |
| F | Split mode intermittently blanks the view on an edit — a `GtkOverlay` snapshotted without a current allocation; rare, first-time-only, recovers on a mode toggle | Medium |
| G | A large document leaves the process spinning a CPU core at ~100% while idle — a GTK/Pango relayout pass that re-shapes text every main-loop iteration and never converges | High |
| J | The app dies occasionally with SIGSEGV inside GTK/GIO, with no reproduction and nothing recorded at the moment of death | Medium |
| N | A click that lands *inside* an existing preview selection never reaches the pane's own click affordances — the first click on a link/marker/checkbox under a selection does nothing | Low |
| O | A GTK4/Quartz autorelease-pool crash SIGABRTs the macOS integration suite in about two runs in three, most often on the one focus-churning test | Medium |
| Q | A wall-clock growth-ratio guard on tab normalisation fails on a loaded machine — the ratio is scheduler noise on a 5 ms baseline, not an exponent | Low |

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

## F. Split mode intermittently blanks the view on an edit (`GtkOverlay` snapshotted without an allocation)

**Severity**: Medium (user-visible — the pane blanks / stops rendering — but rare,
first-time-only per session, and recoverable by a mode toggle; not reliably reproducible)

Observed twice (2026-07-22), **both in split mode, both on a brand-new (`New`) document**:
1. Editing a new doc in a new window: selected some text and deleted it → the **entire view
   blanked / stopped rendering**. Recovered by forcing a mode change and back (Edit↔Split),
   with no code change.
2. Editing a new doc (typing empty task-list entries): the view **blanked momentarily** and
   recovered on its own.

Each time GTK logged (WARN at default `RUST_LOG`):

```
Trying to snapshot GtkOverlay 0x… without a current allocation
```

It has **not recurred after the first occurrence** in a session, and there is no known way
to reproduce it on demand.

**Not the benign member of this warning family** (ScrAP-257). The harmless variant names
`GtkGizmo` — GTK's own private leaf widget, in practice a scrollbar trough — and leaves the
previous frame's render node in place, so it repaints stale rather than empty and shows
nothing on screen. This one names a **`GtkOverlay`**, one of ours by composition, and the
outcome is a **blank** (no paint at all), so that stale-node cushion is not operating here.
Read the interpolated widget name before assuming which case you have.

**Not caused by the empty-task-list-marker fix (ScrAP-158).** Occurrence 1 predates
that change and involved no task lists (plain select-and-delete); the change only touches
`renderer/end.rs` (dropping an empty item's gutter marker) — no `GtkOverlay`, allocation, or
snapshot path. Occurrence 2 merely happened *while* exercising that fix.

**Mechanism (researcher-confirmed against GTK 4.6.9 source + verified against our code;
first-time-only reason and the remedy still PENDING a researcher follow-up).** The live-edit
path in split mode is `preview::re_render`, which **reuses the persistent** preview
`GtkOverlay → GtkScrolledWindow → CodePreviewView` and calls **`view.set_buffer(new_buf)`**
every edit (`render.rs`) — it does **not** append a fresh overlay. (An earlier draft of this
entry blamed a `set_preview` overlay *swap*; that swap only runs on mode-switch / external
reload / initial mount, not on edits. The warned `GtkOverlay` is the **persistent** preview
overlay.) The blank arises on that persistent overlay:
1. `set_buffer` (a content change) runs `queue_draw`, which **clears the cached `render_node`
   walking to the root** — so the persistent overlay's node goes NULL (gtkwidget.c:3541-3552).
2. The new buffer's first onscreen validation at `size_allocate` (heights 0→real,
   gtktextview.c:4771-4789) drives `changed_handler`, whose tail calls
   `gtk_widget_queue_resize()` **from inside the allocate cascade** (gtktextview.c:4935, gated
   on `old_height != new_height`). A `queue_resize` issued during `size_allocate` is **silently
   deferred** (gtkwidget.c:3647-3650), so the overlay carries `alloc_needed` into the **same
   frame's paint**.
3. The snapshot guard early-returns **without building a node** when `alloc_needed` is set
   (gtkwidget.c:11614-11617) and `snapshot_child` **appends nothing** for a NULL `render_node`
   (gtkwidget.c:12045-12047) → the whole pane the overlay covers paints **blank** (not stale —
   the node was cleared in step 1). This is the snapshot-without-allocation family
   (ScrAP-22/ScrAP-23/ScrAP-29) and the same guard as B, but here it covers the *entire* pane.

The **editor** pane is also `GtkOverlay`-wrapped yet never blanks — because its buffer is
edited **in place** (incremental validation), never `set_buffer`-swapped. That contrast is
the likely shape of the fix. Full sourced analysis:
`~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-split-preview-overlay-swap-snapshot-blank-issueN.md`.

**Why first-time-only (researcher-resolved + source-verified).** A descendant's tail
`queue_resize` firing *every* edit does **not** blank in steady state, because
`gtk_widget_size_allocate` clears `alloc_needed`/`alloc_needed_on_child` at its **tail**,
*after* the vfunc (gtkwidget.c:~4103-4105). The layout cascade is **top-down**, so when the
`CodePreviewView`'s validation calls `queue_resize` walking UP to set `overlay.alloc_needed =
TRUE`, control then unwinds through `scrolledwindow.size_allocate` → `overlay.size_allocate`,
and **each ancestor's tail sets `alloc_needed = FALSE` again** — so a mid-cascade descendant
resize never leaves the overlay flagged for *that* frame's paint; it only re-arms one harmless
extra LAYOUT→PAINT. (`GTK_DEBUG=geometry` still *prints* — it reads `resize_needed` before the
clear — but the paint is clean.) The blank needs the overlay painted with `alloc_needed` set
**without** its own `size_allocate` clearing it that frame — which happens **only in the
unsettled initial mount**: entering split on a fresh doc, the view takes its FIRST content with
`first_validate_idle` pending, and `GtkTextView` validates *synchronously* at both the
`size_allocate` tail (gtktextview.c:4771-4789) **and** at paint start (`while(first_validate_idle)
flush_first_validate`, gtktextview.c:~5803), and can expose right after size-allocate without
returning to the main loop. In that window a paint reaches the overlay with a just-set
`alloc_needed` and no full overlay allocate to clear it → NULL node → blank. Same family as the
first-show STUCK blank (recovers only on a relayout = the Edit↔Split toggle); occurrence 2's
momentary self-heal is the borderline case.

**Fix applied 2026-07-22 (mitigation landed; live confirmation still owed — entry stays open).**
The researcher's Round-3 (source-verified, GTK 4.6.9) retired two of the earlier candidates and
pinned the working shape:
- **Idle-defer is a NO-OP** (retracted). The live re-render is *already* async (a 300 ms
  `glib::timeout_add_local_once`, `livepreview.rs`), and the decoupling is intrinsic to
  `set_buffer`'s own re-invalidation (`gtk_text_view_set_buffer` → `queue_draw` clears the overlay
  node to root + `gtk_text_view_invalidate` resets `onscreen_validated=FALSE` and re-arms
  `first_validate_idle`, gtktextview.c:2260-2264/4838-4846) — so *where* `set_buffer` is called
  from does not matter.
- **Pre-warm-while-hidden is a NO-OP** — `validate_onscreen` is gated `SCREEN_HEIGHT>0`
  (gtktextview.c:4715); a `set_child_visible(false)` view never really validates, so revealing it
  re-validates → same blank.
- **Root asymmetry:** the editor is *constructed with content and mapped once*, so its first
  content-validation rides the window build. The preview is mounted **empty**, shown, then filled
  via `set_buffer` **while already visible** → its first REAL content-validation is a decoupled
  on-screen pass that can be **terminal** (occurrence 1). A non-empty doc mounts with content and
  never blanks (why it's new-empty-doc-specific).

**What landed (option (ii), "guarantee a follow-up frame"):** `scrollsync::arm_first_content_repaint`
— after the FIRST live re-render since a fresh preview mount (gated by
`SplitView::preview_first_render_pending`, armed in `set_preview`), a one-shot
`FrameClock::connect_after_paint` forces one `preview_overlay.queue_draw()`. By that follow-up
frame the first-content validation has settled (`alloc_needed` cleared on the interim layout), so
the repaint gives the overlay a real render node — converting a *terminal* blank into a self-healed
one (occurrence 2 was already this borderline). Frame-clock-anchored (not wall-clock, ScrAP-134),
self-disconnecting, weak overlay (ScrAP-152 / GTK4Rs/AP-128). Steady-state edits don't re-arm it. **Verified: all
605 tests pass (no regression); the belt is inherently safe (a forced extra repaint cannot break
anything).** Not applied: option (i) "mount with content" is already our status quo for non-empty
docs and cannot apply to an empty doc (no content to mount); a synchronous allocate/validate is
unsanctioned (`queue_allocate` only flags — gtkwidget.c:~4103-4105; no "relayout now").

**Still owed:** a live confirmation the terminal blank is gone (a possible ≤1-frame flicker may
remain by design — acceptable). If it recurs *terminally*, escalate to a structural fix (don't
reveal the preview pane until its first content has validated during a relayout).

**⚠ Every `GTK_DEBUG=geometry` instruction in this entry is unrunnable on the reference host,
and reads as a clean result when it is dark.** Measured 2026-08-04: a distribution GTK is
built without debug support, so that key — like every informational `GTK_DEBUG`/`GDK_DEBUG`/
`GSK_DEBUG` key — reports `[unavailable]` and emits nothing at all (ScrAP-251). The reasoning
below about *reading* its output remains correct and is retained for whoever obtains a
debug-enabled GTK; it simply cannot be acted on as written here. `sdd/PLAN.profiling.md`
records what that costs and what substitutes for it.

**Live confirmation on the next occurrence** (belt-and-suspenders, since it isn't reproducible
on demand): run once with **`GTK_DEBUG=geometry`** — it names the subtree that `queue_resize`d
during `size_allocate` (gtkwidget.c:4085-4091). The symbol-free parent-walk trap on the warned
pointer (message-scoped `g_log_set_writer_func` → resolve to `gtk::Widget` → walk
`gtk_widget_get_parent`, no debug symbols — see B / GTK4Rs/AP-141) confirms it is the
preview overlay. Full sourced Round-1+2 analysis in the findings doc cited above.

**⚠ Reading the confirmation AFTER a fix (researcher note):** the `GTK_DEBUG=geometry` line is
NOT itself the failure — the tail `queue_resize` fires on every fresh-content validation and a
settled subtree absorbs it in a clean next-frame LAYOUT. So post-fix you may still see a
geometry line with **no blank**, and that is expected and benign. **The BLANK is the tell, not
the geometry warning** — judge the fix by the absence of the blank/`GtkOverlay` snapshot
warning, not by the absence of the geometry line.

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

## J. Occasional SIGSEGV inside GTK/GIO, with nothing recorded at the moment of death

**Severity**: Medium (real and user-facing — the application vanishes mid-work — but
now rare and untriggerable, with no known reproduction. **Two of the five occurrences
below are accounted for and fixed** (2026-08-06): they were the dangling
line-display cache described in ANTI-PATTERNS #258, which the preview's re-render no
longer creates. Three are not, and this entry is what remains of them.
Its worst consequence stays bounded — crash recovery snapshots unsaved edits while
they are unsaved, so an occurrence costs seconds of work and an interrupted session
rather than the whole unsaved buffer — but a bounded symptom is not a diagnosed cause)

Recovered from the operator's kernel log and journald, before the forensics kit
existed:

| When | Signal | Fault addr | Faulting frame |
|---|---|---|---|
| 2026-07-03 11:05:30 | SIGSEGV | `0x18` | `libgtk-4.so.1.600.9` vaddr `0x1DD28A` (static fn, unnamed) |
| 2026-07-03 11:08:17 | SIGSEGV | `0x18` | same IP |
| 2026-07-29 13:43:44 | SIGSEGV | `0x1` | `libgio-2.0.so.0.7200.4` **`g_file_equal +0x20`** |

Both remaining instructions are **reads** (`error 4`) of a small offset off a
null-or-garbage base — the signature of a field read on a freed or invalid GObject —
and the `0x1DD28A` pair recurred minutes apart, so the path is repeatable even though
the trigger is unknown. (Two further occurrences, 2026-07-13 and 2026-07-29 01:54, both
at vaddr `0x375219`, were the fixed defect; they are dropped from this table rather
than kept as history.)

**The one identified frame is a lead, not a diagnosis.** `g_file_equal +0x20`
faulting on address `0x1` is GIO dereferencing an invalid `GFile`. The app never
calls `File::equal` itself, so a `GFile` handed to GTK/GIO was freed and compared
later. Two candidates, unconfirmed: the live-reload `FileMonitor` (`app/open.rs`,
monitor held on `TabState::file_monitor`) — asynchronous disk events fit the
"different circumstances each time" description — and `FileChooserNative`, which
per ScrAP-41 has inverted liveness and needs an external owning reference.

**The recording gap is closed; the crash is not.** The application carries a
crash-forensics kit (`src/forensics/`, TECH.md § Diagnostics and crash forensics):
a persistent log in the state directory, an always-on breadcrumb ring, and a
fatal-signal handler that writes a report naming the signal, the fault address,
the instruction pointer, the build, the last 64 things the application did, and the
process's executable mappings — the last of which makes a frame resolve to
module + offset without the core dump Ubuntu's apport will not produce and the
symbols the distribution does not ship. Lifecycle breadcrumbs are recorded at
`info` regardless of `RUST_LOG`, and they specifically cover both suspects: every
`FileMonitor` event (including one delivered for a tab that no longer exists) and
every native dialog's show and response.

**One recording gap remains, and it under-counts.** The handler traps fatal signals;
it does **not** trap `SIGTRAP`, which is how a glib `G_LOG_LEVEL_ERROR` (`g_error`)
kills the process. A death by that route leaves no report at all — the fixed defect
took that arm roughly half the time, so any future occurrence may too. Trapping it is
small and worth doing.

**What is still open is the diagnosis.** The next occurrence should produce a
report in `$XDG_STATE_HOME/scribobulate/`, and the next launch after it says so.
The question to take to that report is which of the two `GFile` candidates was live
in the seconds before the fault; with that answer, the follow-up is a targeted
`G_DEBUG=fatal-warnings` run under gdb on the implicated path, since the first GTK
CRITICAL is usually upstream of the eventual segfault.

**Resolving a frame from a kernel log** (still needed for a crash from a build
without the kit, and for correlating a report against `journalctl -k`): see
ScrAP-204 — the naive reading of a `segfault at … ip … in <lib>[<VMA>+<size>]`
line names the wrong function.

**Do not treat a clean run as evidence of a fix.** These surfaced three times in four
weeks, so an absence of crashes over a session or a test pass says nothing.

**Mitigation options**:
- **Wait for a report and diagnose from it** (current state): the kit now records what
  the earlier occurrences could not, and the two `GFile` suspects are both
  breadcrumbed.
- **Trap `SIGTRAP` in the forensics handler**: independent of the diagnosis and worth
  doing regardless — without it, a glib-fatal death is invisible.
- **Audit the two `GFile` suspects directly** rather than waiting: the `FileMonitor`
  held on a tab that may already be gone, and the native dialog's inverted liveness.
  Cheaper than a crash, but speculative — neither is confirmed.
- **Accept the limitation**: not viable. An unexplained SIGSEGV in the toolkit is
  worth chasing on its own terms.

## N. A click inside an existing selection never reaches the preview's click affordances

**Severity**: Low (one wasted click, self-correcting — the click clears the selection and
the next one works; no data at risk)

With text selected in the preview, a primary click whose press lands **inside** that
selection does not activate the affordance under it: a link does not open, a margin
comment marker does not open its card, a gutter checkbox does not toggle. The click
clears the selection instead, and a second click behaves normally.

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
