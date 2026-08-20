# Anti-Patterns

Lessons from building Scribobulate's native GTK4/Rust rendering stack. This file is now a compact **project index**: per entry — *symptom* · *where Scribobulate implements the fix* · *pointer into the `gtk4-rs` skill* (+ findings doc). The full transferable lesson, dead ends, and GTK source tracing live in the **`gtk4-rs` skill** — the standing GTK4/Rust anti-pattern knowledge base this project was originally built alongside and which is highly recommended (though not required) when working here. It is referred to **by name only, never by filesystem path**: the skill may not be installed on every machine this repository lives on, so a path would rot. AGENTS.md carries what it is and where to find it; the original self-contained essays remain in this file's **git history**.

**Citation convention.** Two anti-pattern registers are in play and their numbering spaces are unrelated. An entry in **this** file is cited **`ScrAP-N`**; an entry in the `gtk4-rs` skill is cited **`GTK4Rs/AP-N`**, and one of that skill's numbered *techniques* (its `T-n` register — how-to recipes rather than traps, frozen and never reused, exactly like its `AP-n`) is cited **`GTK4Rs/T-N`**. **A bare `AP-N` is illegal anywhere in the tree** — not "it means the skill", illegal — and `lint-references` **check 8** fails on one. Both legal forms are deliberately SINGLE TOKENS with no space: a two-word form is split by any Markdown or `rustfmt` wrap, and two such citations were already broken that way when the gate was written. `ScrAP-` and `GTK4Rs/` are both unique, so a citation resolves to the same register whether it appears in a code comment, in `sdd/`, or inside this file. A bare `#N` inside **this file's own body** is the local shorthand for an entry here (never a skill entry); write `ScrAP-N` in full everywhere outside this file. When a lesson is held by BOTH registers, cite `ScrAP-N`: this register is always resolvable, whereas the skill may not be installed on the machine — which is also why a `GTK4Rs/AP-N` is checked for FORM only and its correctness rests on a human audit of a greppable list (#231). **`GTK4Rs/T-N` inherits that treatment, and its prefix is load-bearing in a way `AP-`'s is not:** a bare `T-N` is NOT gated and cannot be, because this tree already uses bare `T-n` for an unrelated purpose (four in `src/window/outline_nav.rs`, citing a testing register that does not exist here), so a bare-`T-N` check would fire on those and a check with false positives gets disabled. Always write the `GTK4Rs/` prefix on a technique citation; nothing will catch you if you don't. The retired forms — a bare `AP-N` meaning a project entry, `ANTI-PATTERNS #N`, and the two-word `skill AP-N` — are gone from the tree as of 2026-08-01, this time measured rather than asserted (#231 records why the first sweep's identical claim was false).

> **ROUTING RULE — apply it when you MINT an entry, not in a migration later.** A lesson only
> leaves this register if it has somewhere reusable to go, and today the **only** such destination
> is the `gtk4-rs` skill. So one question decides it: **is this a lesson about gtk4-rs itself?**
>
> - **Yes** → weave it into the skill, and keep a stub here citing `GTK4Rs/AP-N`.
> - **No** → **it stays here, in full.** That is Scribobulate's own internals *and* equally every
>   dependency that is not gtk4-rs — **Pango**, **GtkSourceView**, **pulldown-cmark/CommonMark**,
>   **librsvg**, **syntect**, **serde/`toml`**, the OS, the toolchain — *and* every process/tooling
>   concern (testing, CI, packaging, review discipline). None of those components is reused outside
>   Scribobulate, so extracting their lessons would buy no reuse and cost a hop. **Do not fold them
>   into the core skill.** (A separate home for the purely general engineering-discipline lessons is
>   under consideration but undecided; until it exists, they stay here too.)
>
> **`glib`/`gio` are IN scope for the skill** (operator, 2026-08): its stated coverage is *gtk4,
> glib, gio, gdk*, so a GLib or GIO lesson routes there like any other gtk4-rs lesson — the earlier
> reading of GLib as "third-party" is retired. A lesson already carried there does not need a full
> body here (ScrAP-232 → GTK4Rs/AP-167, ScrAP-243 → GTK4Rs/AP-243); one that is not yet carried
> (ScrAP-128) is a weave candidate, not a resident.
>
> **Remaining edge — Pango.** Pango is NOT in that scope line, yet one Pango-flagged lesson has
> landed skill-side (ScrAP-163 → GTK4Rs/AP-154) on the argument that it is really a `GtkLabel`
> method contract and Pango is only the mechanism underneath. That test — **route on whose API
> contract the lesson is about, not on whose code the mechanism lives in** — is not yet ratified.
> ScrAP-4 remains deliberately excluded from the skill. Raise a Pango lesson before routing it.
>
> A lesson may **split**: fold the core-GTK half into the skill, keep the remainder here, and say so
> in **both** places — **#36** → GTK4Rs/AP-46 and **#124** → GTK4Rs/AP-98 are the precedents. A
> transferred or split entry's stub stays a little fuller than a bare pointer (root cause +
> citations inline); the full essay lives in git history. **The per-entry answer is the index's
> `Disp` column — never re-enumerate it in prose here.** Doing exactly that is how this note
> previously grew into a stale, 3 kB duplicate of the column it was shadowing. Edited only by the
> Scribobulate maintainer agent.

## Stub structure — the shape a compressed entry MUST take

> **A stub is exactly four lines: the `##` heading and three one-line fields, with no
> blank lines between them and no other sections.** Copy this template literally.
>
> ```
> ## N. Title stating the trap, not the fix
> **Symptom**: what a reader OBSERVES, in one sentence — the surprising behaviour, not the cause.
> **Scribobulate**: where THIS project implements the fix, plus its regression guards.
> **See**: gtk4-rs skill → <module> (GTK4Rs/AP-N).
> ```
>
> **Why it is written down here.** The stub form is not derivable from the routing rule
> above, and the file's own long entries actively mislead about it: most `A`-tagged
> entries carrying full essays (#232, #243, #260 …) are **migration backlog**, not
> exemplars, so an agent that samples a neighbour and copies its shape produces another
> wall of prose and believes it has stubbed. This section exists so the shape is read,
> not inferred. The majority of entries in this file are already in exactly this form —
> when in doubt, look at #1, #11, #22, #100 or #104.
>
> **Field rules.**
> - **One line per field.** Long is fine (see #63); wrapped is not, and neither is a
>   second paragraph. If a field wants to become prose, the content belongs in the
>   external register the `**See**` line points at, or in a code comment.
> - **`**Symptom**`** is the *observation*, phrased so someone hitting the bug
>   recognises it — that is what makes the index searchable by symptom. Mechanism,
>   measurements, dead ends and lessons all belong in the skill entry, never here.
> - **`**Scribobulate**`** is the one thing no external register can carry, and lint
>   check 10 enforces its presence. Name the module/seam and the regression guards.
>   When there genuinely is no implementation, say so in the standard words rather than
>   dropping the field: *"none — a discipline lesson with no implementation in this
>   tree. (Stated, not omitted: an absent field and a dropped one look identical.)"*
> - **`**See**`** names the skill module (bare name — no `references/` prefix, no
>   `.md`) and the `GTK4Rs/AP-N` citation. Append `Findings: <file>` on the SAME line if
>   there is a findings doc; it is never a fourth field. For a lesson with no external
>   home, this line reads *"project-specific; the fix + rationale live in a code comment
>   at <site>."*
>
> **When to stub.** At MINT time, per the routing rule above — a lesson about gtk4-rs
> itself is woven into the skill and stubbed here in the same change, never written full
> and compressed later. A `B`/`C` lesson (no reusable home) stays here **in full** and is
> not a stub; those are the entries legitimately running to dozens of lines.
>
> **One caveat when stubbing at mint time:** the usual "the full essay lives in this
> file's git history" does not apply, because it was never committed in full. Say where
> the canonical text went, so nobody searches `git log` for an essay that was never there.

## Number reservations — read before minting an entry

> **The gaps here are RESERVED, and the highest number in this file is NOT the next
> free one.** Anti-pattern numbers are frozen IDs, so slots are held for entries that
> currently live on in-progress branches rather than being recycled — recycling one
> would give two different lessons the same ID the moment those branches land. This
> already happened once and cost a renumber (a port's 165 became 172 on the way here).
>
> | Range | Held by | State |
> |-------|---------|-------|
> | 176–179 | Windows port — **holder gone; see below** | 176 was reported authored, 177–179 claimed. Neither landed. |
> | 186 | `feat/spelling` | authored, platform-neutral, inbound |
> | 276–289 | unmerged branches (`ci`, `feat/spelling`, `windows/*`, and seats' own clones) | **allocated and written, invisible from `master`.** Measured 2026-08-16: every one of 276–289 already carries a `## N.` heading somewhere in reachable history, and the `gtk4-rs` skill's mirror notes name three numbers in this range. Nothing in it is free. (Numbers deliberately un-sigilled, as in the 176–179 note below — a citable form here is the dangling register-to-register reference check 3 exists to catch.) |
>
> The Windows port's 165–168 and 175 rows are gone for the same reason the macOS
> rows below are: those entries **landed** in this file when `feat/windows-port`
> merged.
>
> ⚠️ **176–179 is a reservation whose holder no longer exists, and it is deliberately
> NOT released.** Measured 2026-07-29: `feat/windows-port` is merged and deleted, no
> branch in the repository holds it, and a citation of number 176 appears **nowhere in
> any reachable history** — so the entry reported as "authored" never landed, and the
> branch that was holding it is gone. (Stated without the `ScrAP-` sigil on purpose:
> writing the citable form here would itself be the dangling register-to-register
> reference check 3 exists to catch — which is how this paragraph was caught.) That leaves two possibilities and this file
> cannot tell them apart: the work exists uncommitted on the Windows seat's machine,
> or it was lost when the branch was deleted. Numbers stay held because the cost of
> being wrong is asymmetric — a released-then-reclaimed number is the exact collision
> this table exists to prevent, while four permanently-skipped integers cost nothing.
> **Resolving this needs the operator**, not an inference from here.
>
> The macOS port's rows are gone because its entries **landed** in this file when
> `platform/mac` merged (171–174, 180–182, 184–185). A reservation row exists only
> while a number is held *somewhere this file cannot see*; once the entry is here,
> the row is noise that will eventually be read as a live claim. Clear a row in the
> same commit that merges the entries it was holding.
>
> **The next free number is 292.** (269–275 landed below; 276–289 are held by unmerged
> branches — see the row above, which is what the previous "next free is 269" would have
> collided with had anyone derived a number from this file's own tail; 290 and 291 were
> claimed on 2026-08-16 for the tab-strip layout pair.) Do not derive it from the highest entry below —
> unmerged branches hold ranges that are invisible from this file, which is exactly
> how a collision happens. Check the table, **announce the range you are claiming**,
> and never fill a reserved gap.

> **Provenance.** This registry was duplicated until now: the copy above lived inside
> entry #164, and a **stale** second copy inside entry #168 (measured a day earlier) still
> announced **182** as the next free number — by then long occupied. A registry buried in
> the body of an unrelated entry is invisible to anyone minting a number *and* deletable by
> any edit to that entry, so both were lifted here and the stale one retired; every row it
> held is resolved above. **Keep this the only copy.**

---
**Disposition tags (migration bucket, provisional).** `A` = GTK4-core, belongs in the `gtk4-rs`
skill (body here becomes a pointer stub); `B` = general-engineering discipline, destination ON
HOLD pending operator review — treat as provisional; `C` = stays in this register, per the
routing rule above; `D` = dead landing-spot stub for a merged/superseded number. **Numbers are immutable: never renumbered,
never reused; a deleted entry keeps its `## N.` heading forever.**

| #   | Anti-pattern | Disp |
|-----|--------------|------|
| 1 | Rendering a document viewer with a GPU-compositing UI stack | A |
| 2 | Assuming "disable hardware acceleration" makes a web engine render on the CPU | A |
| 3 | Using environment variables to prevent GTK from crashing on a large XCompose file | A |
| 4 | Using Pango `<a href>` markup in GtkLabel for standalone link widgets | C — reasons superseded by #259 |
| 5 | Reading GtkTextBuffer text to track trailing newlines when child anchors are present | A |
| 6 | Using a horizontal rule to indicate a blockquote | D |
| 7 | Placing the blockquote `DrawingArea` in an outer overlay outside the `ScrolledWindow` | D |
| 8 | Redirecting `XDG_CONFIG_HOME` without carrying `mimeapps.list` | A |
| 9 | Duplicating action logic across context menu, main menu, and keyboard shortcut | C |
| 10 | Walking the widget tree to re-discover anchor-embedded GtkLabel widgets | C |
| 11 | Expecting `g_menu_item_set_icon()` to render icons in a GTK4 menu bar | A |
| 12 | Using untyped GLib qdata as the canonical store for per-window state | A |
| 13 | Restoring GtkTextView scroll via a GTK adjustment (on `changed`, or `set_value` after `set_buffer`) before the layout has validated | A |
| 14 | Restoring `GtkTextView` scroll via adjustment manipulation after `set_buffer` | D |
| 15 | Using `iter_at_location` to get the iter at the top of a `GtkTextView` viewport | A |
| 16 | Mirroring split-pane scroll synchronously inside `value-changed` | A |
| 17 | Parsing a "new instance" CLI switch after the GApplication has registered | A |
| 18 | Bridging glib↔Rust `log` with the wrong handler (stack overflow / dropped Gtk-CRITICAL) | A |
| 19 | Trying to override a `GtkDropDown`'s empty-state "(None)" caption | A |
| 20 | Gating a widget-scoped action on raw per-widget focus | A |
| 21 | Indicating a text block (code block / blockquote) with the wrong GTK primitive instead of self-drawing it | A |
| 22 | Forcing `GtkTextView` layout validation inside the snapshot/draw or size-allocate path | A |
| 23 | Embedding height-for-width block content as a widget at a `GtkTextChildAnchor` | A |
| 23a | Bounding an anchored child to the content column while it sits at an INDENTED margin | C |
| 24 | Relying on the system theme to paint a `GtkTreeExpander`'s disclosure chevron | A |
| 25 | Assuming the `.heading` / `.title-N` typographic CSS classes require libadwaita | A |
| 26 | Pointing a `GtkPopover` at an anchor rect outside the visible viewport | A |
| 27 | Searching find-next from the caret after `select_range` (re-finds the current match) | C |
| 28 | Tracking a selectable `GtkLabel`'s selection via a `notify::cursor-position` | A |
| 29 | Re-laying-out an anchored child via `queue_resize` after `parent_size_allocate` in the same `size_allocate` | A |
| 30 | Dismissing/unparenting a popover from inside a descendant's click handler | A |
| 31 | Resolving an untrusted document's local image `src` against the CWD (or with a lexical-only containment check) | B |
| 32 | Anchoring a `GtkPicture` in a `GtkTextView` without a nonzero width request | A |
| 33 | Testing a rebuilt single-instance GApplication while a stale primary is still running | A |
| 34 | Remote image loading blocks the main thread; "refused" ≠ "unresolvable" in a multi-outcome resolution enum | A |
| 36 | Letting the editor `GtkSourceSearchContext` `notify::occurrences-count` overwrite the preview buffer's `forward_search` count in preview mode | C |
| 37 | `GtkTextView` never repaints an anchored child when a descendant's Pango background is REMOVED | A |
| 35 | Reading `st.source` for a programmatic preview re-render in split mode | C |
| 38 | Driving derived UI state from a delta-only signal, missing lifecycle boundary events | A |
| 39 | Specifying GNOME-specific icon names absent from non-GNOME themes | A |
| 40 | `GtkAboutDialog` `authors` entries with `<url>` format open `mailto:` | A |
| 41 | `FileChooserNative`/transient-dialog lifetime is backend- and widget-type-dependent | A |
| 42 | Predictable, reused path under the shared temp dir for a config-redirect workaround (security) | B |
| 43 | Relying on `GtkNotebook`'s `create-window` signal for "drag a tab to the desktop to spawn a new window" on Wayland | A |
| 44 | Using a `<Shift>` + digit/punctuation `GtkApplication` accelerator | A |
| 45 | A `GtkNotebook` with `show-tabs` false cannot be a cross-window tab-drag drop target | D |
| 46 | An idempotent signal-rewire check keyed only on widget identity misses a stale closure after a cross-window reparent | A |
| 47 | `gtk_window_present()` from a D-Bus `open` handler doesn't raise+focus a tokenless (bare-terminal) launch — legitimate WM behavior, not a bug | A |
| 48 | Adding an ancestor `GtkEventControllerKey` for Escape doesn't catch Escape while a `GtkSearchEntry` descendant has focus | A |
| 49 | `cargo valgrind` on a GTK4 app reports hundreds of toolkit-internal "leak"/uninitialised-value errors that are NOT application bugs — but valgrind still catches real app UAFs here, so triage by stack, don't dismiss wholesale | A |
| 50 | `GtkNotebook`'s native cross-window tab-detach DnD is unsafe on GTK 4.6.9 — a NULL deref inside GTK's own `dnd_finished_cb`, not (only) a freed source notebook | A |
| 51 | A `GtkSourceSearchContext` `occurrences-count` handler that strong-captures its own context is a permanent self-reference leak | C |
| 52 | Swapping in a brand-new `GtkScrolledWindow`/`GtkAdjustment` on external auto-reload without re-wiring the scroll-spy signal bound to the old one | A |
| 53 | Holding a `RefCell` `Ref` alive across a GTK setter that synchronously re-enters and borrows the same cell aborts the process | A |
| 54 | `write_atomic`'s crash-safe write-temp-then-rename makes GIO's `GFileMonitor` report every save as a deletion | A |
| 55 | Caching "which slot is active" as a raw `Vec` index across a drag-reorder that moves entries within the same `Vec` | A |
| 56 | Toggling a sibling widget's visibility from inside a container's own `size_allocate` | A |
| 57 | A signal handler that captures its host `ApplicationWindow` by weak ref silently stops working after the widget subtree is re-homed to another window (split scroll-sync) | A |
| 58 | Reparenting a reused `GtkSourceView` across view-mode containers re-fires its gutter's never-unbound `vadjustment` binding → a use-after-free | C |
| 59 | Mounting a scrolling pane in a `GtkBox` without `vexpand` collapses it to its natural height (a lazily-validating `GtkTextView` then paints only ~2 lines) | A |
| 60 | A closure owned by a widget's own machinery that strong-captures self (or an ancestor) is an uncollectable cycle — on window close it strands the entire descendant subtree | A |
| 61 | Building N `GtkMenuButton` menu-models in a synchronous startup burst forces N×items accelerator-label font resolutions → a multi-second UI freeze | A |
| 62 | A custom tab/stack widget leaves its active-index model unset for the default-visible first page | C |
| 63 | A shared app-level menubar model can't carry per-window content — a per-window submenu needs a self-built `GtkPopoverMenuBar` + selection-as-action-state + deferred `GMenu` mutation | A |
| 64 | A process-global CSS provider with an unscoped selector collides across windows (last-loaded wins) | A |
| 65 | Preserving a `GtkTextView` reading position across a re-render drifts, and can wedge input dead | A |
| 66 | Relying on pulldown-cmark's native superscript/subscript for tight `E=mc^2^` / `H~2~O` | C |
| 67 | Screenshotting an open GTK4 `GtkPopoverMenu` under kwin/X11 to verify a menu | A |
| 68 | Deferring a `set_visible(false)`→`measure()` read as if it were the GtkTextView lazy-validation family | A |
| 69 | Putting mnemonic `_` markers in a command label shared across menu + tooltip + context-menu surfaces | A |
| 70 | Getting bare-letter access keys (with a visible underline) in a plain `GtkPopover` via mnemonics / use-underline | A |
| 71 | Nesting a submenu as a child `GtkPopover` inside a plain autohide `GtkPopover` context menu | A |
| 72 | Gating/targeting a multi-pane action on the first-found view instead of the focused pane | A |
| 73 | Reconstructing character-precise copied Markdown from sparse parser waypoints, and mis-reading pulldown-cmark offset semantics | C |
| 74 | Aligning char offsets with `GtkTextBuffer::get_text()` — it omits anchored children | C |
| 75 | A hard tab in a GFM table breaks table recognition; normalise tabs — but length-preservingly | C |
| 76 | A paragraph-attribute `GtkTextTag` applied as one continuous range over a multi-paragraph region drops the attribute on toggle-free middle lines | A |
| 77 | UI-testing a formatter over the selectable read-only Preview pane | C |
| 78 | `Options::all()` (or any enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of degrading to literal text | C |
| 79 | A container-level `GtkGestureClick` also fires on presses that land on a child `GtkButton` — a bar-wide "activate" gesture activates a tab even when the press was on its × close button | A |
| 80 | Tracking a "reading line" only from a wheel `EventControllerScroll` misses scrollbar-drag and keyboard scrolling — the re-anchor goes stale | A |
| 81 | Persisting "all windows" from each window's `close-request` + a sequential-close quit loses every window but the last | A |
| 82 | A one-shot `scroll_to_mark` restoring a FAR reading position onto a freshly-rebuilt GtkTextView lands near the top | A |
| 83 | `GtkShortcutsWindow`'s programmatic `add_section`/`add_group`/`add_shortcut` API is GTK 4.14+ — on 4.6 you must build it from Builder XML | A |
| 84 | `GtkTreeListModel` `autoexpand=true` makes true recursive Collapse all impossible; build `autoexpand=false` + explicit expand pass. Collapse DESTROYS the subtree (it does not cache expanded flags) | A |
| 85 | A bundled (gresource) `*-symbolic` icon is only a fallback — a host theme that ships the same name overrides it | A |
| 86 | Probing a broader Markdown marker before a narrower one that embeds it mis-parses the input — test narrowest-first | C |
| 87 | A `#[gtk::test]` that maps + pumps to full allocation BEFORE calling the code under test validates line heights first, masking an unvalidated-heights bug | A |
| 88 | Bounding a blocking `MainContext::iteration(true)` pump loop with a between-iterations wall-clock check instead of a timeout SOURCE — it can hang forever on an idle display | A |
| 89 | Gating a programmatic `GtkSingleSelection` change with a transient "we're setting it" bool — it re-emits `selected-item` after the setter returns, escaping the bool | A |
| 90 | A `GtkPopover` attached with `set_parent()` is NOT auto-unparented — the parent widget's `dispose()` must unparent it, or teardown floods "GtkPopover is not a child of …" | A |
| 91 | An always-on scrollbar with default (overlay) scrolling floats over the `GtkTextView`'s right margin, stealing clicks meant for margin-drawn affordances | A |
| 92 | A mutation path that edits the buffer but leans on a MODE-GATED live-preview refresh leaves the preview stale | C |
| 93 | Anchoring positions by pulldown-cmark source offset against ALL events maps onto a block-structure event whose range spans the whole block | C |
| 94 | A signal handler connected to a `GtkTextView`'s BUFFER is silently dropped when `set_buffer` swaps the buffer — re-wire buffer-dependent handlers on the new buffer | A |
| 95 | A shown `GtkPopover` does not grow its surface when its child grows — pre-size it (homogeneous `GtkStack`), don't re-present it | A |
| 96 | Committing an action that rebuilds the widget subtree synchronously inside a `GtkButton` `clicked` handler breaks active-state accounting | A |
| 97 | Inferring "inline vs block" from non-empty source delimiter bytes engulfs whole paragraphs | C |
| 98 | A `GtkPopover` hosting a typing entry is unwinnable on X11 (autohide steals focus via its seat grab; non-autohide can drop clicks) — host a typing entry as an in-surface `GtkOverlay` child instead | A |
| 99 | A translucent text-tag highlight is painted over by a later opaque-background tag — GTK text-tag backgrounds don't composite; the highest-priority tag wins | A |
| 100 | Measuring a widget while it is `visible=false` returns 0 — center an overlay child off a hidden measure and it collapses to a left-edge anchor | A |
| 101 | UI-test tooling: kwin-on-Xvfb won't deliver a synthetic `xdotool` click to a non-autohide `GtkPopover` surface — verify such flows via a keyboard-triggerable action, not a synthetic popover click | A |
| 102 | Positioning a widget via `set_margin_*` then re-measuring it double-counts the margin — GTK folds a widget's own margins into `preferred_size()`/`measure()` | A |
| 103 | Refreshing a `GtkTextView` via `set_buffer` for a change that leaves the rendered text identical repaints the whole document and jumps the scroll | A |
| 104 | A persisted `GtkTextMark` re-resolved after a `set_buffer` swap is a cross-buffer footgun that aborts with `gtk_text_btree_line_number couldn't find line` | A |
| 105 | `iter_location` (any line-DISPLAY-caching geometry read) right after a `set_buffer` swap, before re-allocation, aborts with `gtk_text_btree_line_number couldn't find line` | A |
| 106 | A selectable `GtkLabel` in a popover auto-selects all its text on open — the popover focuses it, and a selectable label selects-all on focus-in | A |
| 107 | A menu-activated action that synchronously raises a focus-grabbing in-surface widget has its focus stolen by the menu popover's pop-down focus-restore — defer the raise to idle | A |
| 108 | `GtkTextBuffer::redo()`/`undo()` leaves no undo barrier — the next edit merges into the redone action's group, so one later Undo reverts two edits | A |
| 109 | Mapping GtkTextView buffer coords ↔ an anchored-child cell's interior under incremental allocation | A |
| 110 | Driving selection-dependent UI for a selectable-`GtkLabel` cell (a selection island) — buffer signals never fire; use the primary clipboard, wired on the live view | A |
| 111 | The in-place buffer-tag refresh can't repaint an anchored-child cell decoration — reconcile the cell labels in place, unconditionally | C |
| 112 | `GDK_IS_SURFACE` criticals are a stale TOOLTIP timer over an unrealized grabbing popover — reuse popovers, don't destroy per use | A |
| 113 | The first popup of a view-parented popover forces a one-shot table revalidation that scrolls the view and drops the click — pre-warm it | A |
| 114 | An in-place live-buffer edit that skips the canonical source-of-truth vanishes on the next fresh render | C |
| 115 | Highlighting a char range in an existing Pango-markup string via `find` wraps the wrong (first) occurrence | C |
| 116 | Activating a nested-submenu item in a `GtkPopoverMenuBar` leaves a sibling top-level menu popped open — the bar clears its open menu through only one channel (a top-level popover's `unmap`) | A |
| 117 | Clearing a `GtkLabel` `set_attributes` overlay in place needs a transient markup-STRING change to repaint — a same-string `set_markup` is a no-op | A |
| 118 | A list-item hanging indent (`left-margin` + negative first-line `indent`) is unreliable across paragraphs; the durable fix is to DROP the hanging indent (draw the marker in a gutter, uniform margin) | A |
| 119 | A `GtkPaned` with the default narrow handle silently swallows presses in a strip at a child pane's edge | A |
| 120 | `WidgetExt::color()` is gated behind the gtk-rs `v4_10` feature — the compile error never mentions it | A |
| 121 | Two `GtkTextTag`s that both set `left-margin` on a line (a list item inside a blockquote) do not compose | A |
| 122 | Translating a stripped-then-parsed document's ranges back to original coordinates instead of per-position translation silently swallows the stripped bytes (the range-merge gotcha) | C |
| 123 | A coverage ratchet's floor recorded as stale prose drifts from the real (climbing) figure, silently loosening the gate | C |
| 124 | A test suite gated behind a Cargo feature is invisible to every gate that does not enable it — it rots until it stops compiling, and every line it covers reads as 0% | A |
| 125 | Scheduling work that depends on paint-populated state via `idle_add_local_once` reads the previous frame's state and silently no-ops | A |
| 126 | Styling a `GtkTextView`'s background via `textview { background-color }` alone works on Default but is defeated by the user's system theme | A |
| 127 | Reaching for CSS selector specificity to arbitrate between two `GtkCssProvider`s is a category error | A |
| 128 | `g_get_user_config_dir()`'s process-global lazy cache makes a mid-startup `XDG_CONFIG_HOME` redirect and an honest config-dir read mutually exclusive | A |
| 129 | `g_app_info_launch_default_for_uri(uri, NULL, …)` silently emits no activation token, so a WM's focus-stealing prevention refuses to raise the handler | A |
| 130 | A hand-authored SVG that renders fine in Inkscape can be invalid XML that librsvg (and GTK) rejects outright | C |
| 131 | A refactor that REDEFINES what an existing field means keeps compiling at every call site, and silently changes behaviour | B |
| 132 | A source-scanning guard test whose scope filter is wrong scans nothing and passes forever | B |
| 133 | A hard-coded Xvfb display lets one crashed run orphan a server that silently serves stale windows to every run after it | A |
| 134 | Bounding a wait on a FRAME COUNT when the thing waited on is measured in WALL-CLOCK | A |
| 135 | `GtkText` writes PRIMARY on every selection change — and a widget claiming PRIMARY CLEARS the previous owner's selection | A |
| 136 | Seeding live UI state from the persisted-session snapshot | B |
| 137 | A window `GAction` accelerator BEATS a focused `GtkText`'s own keybinding — and *disabling* the action is what hands the key back | A |
| 138 | Polling a `GtkEntry`'s own `has_focus()` in a test spins forever — focus lands on its internal `GtkText` delegate | A |
| 139 | A `GtkText`/`GtkEntry` selects ALL its text on focus-in, silently undoing a caret set BEFORE `grab_focus` — and the hazard IS guardable headlessly, if the toplevel is MAPPED | A |
| 140 | A security gate answering a DIFFERENT question than the one being asked | B |
| 141 | A "this will misbehave" theory read from a construction site, never executed | B |
| 142 | A capture-phase ancestor gesture cannot pre-empt a child's gesture and hand it back cleanly — "one similar event will be emulated" preserves event COHERENCE, not gesture STATE | A |
| 143 | A PERMANENT register entry citing an EPHEMERAL artifact (an ISSUES entry, a PLAN file) | B |
| 144 | `unparent()` on an OPEN GtkPopover does not emit `closed` — it skips the close path entirely | A |
| 145 | Two registers numbering their entries with the SAME prefix — every cross-citation is wrong-but-plausible | B |
| 146 | Assuming `GdkTexture::from_file` ignores installed gdk-pixbuf loaders, and adding a manual `Pixbuf` fallback | A |
| 147 | Raw-HTML `<picture>`/`<img>` silently dropped — block HTML is emitted per-line, wrapped in `Tag::HtmlBlock` | C |
| 148 | Splicing at an offset mapped OUT of a delimiter-stripped coordinate space | C |
| 149 | Two overlapping async scroll-drivers over one adjustment, neither cancelled by a newer navigation | A |
| 150 | A self-drawn decoration re-adding padding that the line's own tags already put inside its `line_yrange` | A |
| 151 | Detecting a URL scheme with "the text before the first colon" (`split_once(':')`) | B |
| 152 | A deferred idle closure that strong-captures a widget fires against it after teardown — and the reflexive guards each miss | A |
| 153 | A `#[gtk::test]` integration suite renders on the default GskGLRenderer, not the renderer `main()` selects — and its GL texture cache SIGABRTs at teardown under a headless display | A |
| 154 | Migrating a hand-rolled weak capture to `glib::clone!` is not a blind find/replace — its single hoisted upgrade changes behaviour at several site shapes | A |
| 155 | A per-render widget whose `Rc` dismiss closure strong-captures its own container, while controllers on that container hold the `Rc` — an uncollectable cycle that strands the subtree every rebuild (unbounded reload leak); plus naming a GTK-internal allocator leak with no debug symbols | A |
| 156 | Reading a `GtkTextView` selection's anchor y from a wall-clock debounce after a scroll — the read lands before validation, so an on-viewport selection is suppressed | A |
| 157 | Collapsing a large `GtkTreeListModel` while the `GtkListView` is scrolled to the bottom strands a stale far-end row | A |
| 158 | A content-less list item still emits a full item (and task marker) — an unconditional per-item gutter decoration draws a stray marker | C |
| 159 | Centering a gutter marker on `line_yrange`'s height centers it over ALL of a soft-wrapped item's rows, not the first | A |
| 160 | syntect's bundled default syntax set has no TypeScript/TSX/TOML — a fence in one of those languages silently falls back to plain text and renders as one flat colour | C |
| 161 | A CSS `margin-*` silently ADDS to a code-set `gtk_widget_set_margin_*` on the same axis — the stylesheet can never reduce the inset, so `margin: 0` still stops short of the edge | A |
| 162 | `GtkTextView` reading position drifts toward the top under repeated horizontal resize — the re-wrap re-validation clamp, and the one width-changing path with no re-anchor hook | A |
| 163 | Switching a `GtkLabel` to `set_markup` silently makes every interpolated string a Pango-markup injection/breakage surface — an un-escaped filename metacharacter renders the label EMPTY, with no crash | C |
| 164 | Committing a test fixture whose filename is itself the invalid input breaks checkout on other platforms | B |
| 165 | Clearing an env var the wrong way gives a false confirmation | B |
| 166 | Never diagnose a hung test suite from a parallel run | B |
| 167 | An `Option`-returning lookup whose `None` is also a legitimate answer will fail silently forever | B |
| 168 | A popover's layout pass resizes the TOPLEVEL — from GTK's stale remembered size — collapsing a natively-maximized window | A |
| 169 | A pruned `-symbolic` icon name degrades to a legacy raster instead of failing — and `has_icon` is *stricter* than the render path, so the audit scores it green | A |
| 170 | Symbolic icon art drawn with strokes silently changes shape — the SVG rasterizer you preview in is not the renderer that ships it | A |
| 171 | Every `#[gtk::test]` aborts on macOS before its body runs — the harness dispatches onto a worker thread, and GTK there requires the main one | A |
| 172 | A synthesized-click UI-automation tool can be silently broken, making a real bug look unfixable across several attempts | B |
| 173 | Freezing a drag icon with `current_image()` AFTER dimming the source widget captures a blank — `queue_draw` has already cleared the render node | A |
| 174 | A single-instance guarantee that lives in a backend, not an API, fails silently where the backend is absent — and the platform's *other* launch path will falsely confirm it works | A |
| 175 | A defect whose CONSEQUENCE is platform-dependent while the defect itself is not — the platform that never triggers it never tests for it, and a guard written on the triggering platform's symptom is permanently green where the bug actually lives | B |
| 180 | A `set_parent`'d child left on a `GtkTextView` at dispose is an INFINITE loop, not a warning — and the suite that stayed green was the one never disposing anything | A |
| 181 | A suite that has never RUN on a platform is full of assertions that only look portable | B |
| 182 | A readiness probe stronger than the behaviour it gates fails on its own terms — `has_focus()` needs an ACTIVE toplevel, `notify::focus-widget` does not | A |
| 183 | A mutation that fails on an earlier precondition proves nothing about the guard under test | B |
| 184 | Four green checks, none of them the outcome — the plumbing was verified and the user-visible result was not | B |
| 185 | An idle queued from a native OS callback is not dispatched for seconds — and deferring the *read* answers the event with the wrong value | A |
| 187 | A byte range captured at build time and applied at click time is a bet, not a coordinate | C |
| 188 | "It broke when I removed X, so X was providing it" — a temporal correlation dressed as a mechanism | B |
| 189 | A GTK doc comment promised scroll-tracking the code never implemented — and the same API silently feeds the view's minimum size | A |
| 190 | A `show`-vfunc invariant has a hole exactly on the re-present path — showing an already-visible widget never runs it | A |
| 191 | A "pre-warm" of a reused widget becomes a teardown of a live session the moment that widget owns state | A |
| 192 | `popdown()` is not animated, `closed` fires from `hide` — so your own transient hide trips your own "is it still open?" backstop | A |
| 193 | Driving/screenshotting a GTK4/Quartz app from an agent on macOS: no established recipe, and every gap in one reads as an app defect until isolated | A |
| 194 | A shared per-line helper that hands out a RAW line makes every block transform blind to the container prefix — one rule, four copies to get wrong | C |
| 195 | A decision driven off one parser's event stream cannot see the constructs a second tokeniser owns | C |
| 196 | A fallback keyed on a symptom, not a cause, silently swallows the next cause that shares the symptom | C |
| 197 | A `#[path]`-included module's children resolve against the attribute's directory, not the module's own name | B |
| 198 | `pub use` cannot widen `pub(crate)` visibility — there is no test-façade shortcut around it | B |
| 199 | Treating an `insert-text` of `"\n"` as "the user pressed Enter" — a paste is many `insert-text`s, one of them a bare newline, and acting on it is undefined behaviour | A |
| 200 | `GtkSourceIndenter` is unusable from gtk-rs — the subclass trampoline frees the caller's `GtkTextIter` | C |
| 201 | A custom `harness = false` runner that ignores libtest's `--skip` turns a carve-out into a selection — silently, and green | B |
| 202 | A gate in front of a paint-carried dispatch: the settled state is the one that queues no paint | A |
| 203 | Restoring `SIG_DFL` and re-raising inside a fatal-signal handler exits *normally* with status 139 — the signal is blocked for the handler's own duration | B |
| 204 | Resolving a kernel segfault `ip` against `nm` output — the kernel's VMA base is the executable *segment*, not the ELF load base | B |
| 205 | Predicting one platform's rendering from another's at the same toolkit version — the distributor's theme decides, not the version number | A |
| 206 | A reference gate whose pattern demands a file extension the codebase's citations never write — clean, green, and blind to every dangler of that shape | B |
| 207 | Two ports of one gate that share a pattern but not a file ENUMERATION — the parity claim is false, and the platform nobody runs is the lenient one | B |
| 208 | A proc macro that moves the annotated item's attributes onto the generated BODY instead of the harness item — `#[ignore]` silently does nothing | B |
| 209 | A guard test whose setup prevents the resource from ever existing cannot observe the leak it guards — it passes with the fix deleted | B |
| 210 | Windows PowerShell converting a value on your behalf instead of failing — the call site reads correctly in every instance | B |
| 211 | A verification whose result nothing consumes — it reported the mismatch, and the corrupted payload was applied one line later | B |
| 212 | `#[cfg(unix)]` on a test and "skipped on Windows" are indistinguishable in the report, and only one of them is true | B |
| 213 | An artifact that describes what you meant to do, shipped beside what you actually did, and never reconciled | B |
| 214 | `backtrace_symbols`'s BSD twin is not the safe half of the pair — the async-signal-safety argument you inherited is about a different hazard | B |
| 215 | Verifying a behaviour-preserving refactor with hand-written expectations tests your belief about the code, not the change you made | B |
| 216 | A gate that checks a citation EXISTS cannot see one that points at the wrong real thing | B |
| 217 | A negative result is worthless without a positive control — "it was prevented" and "I cannot see it" produce identical output | B |
| 218 | Confidence ratchets across a relay — the hedge is dropped by whoever summarises, and nobody does anything wrong | B |
| 219 | A remedy that lives inside one consumer reaches the consumers that already knew about it | B |
| 220 | A regression guard built from the instance you fixed has coverage exactly equal to the fix | B |
| 221 | A comment explaining why a test asserts less than its name promises is where a false premise hides | B |
| 222 | Two gates, each correct, enforcing opposite things — and neither can see the other | B |
| 223 | Write a finding as a testable proposition, not as a conclusion — a conclusion recruits agreement, a proposition recruits a measurement | B |
| 224 | A squash makes single-seat authorship unprovable — on a deadline nobody is watching | B |
| 225 | Four denial-of-service paths in four subsystems were one omission: nobody had said the project had an opinion about input size | B |
| 226 | The check your self-test does not cover is the one that ships broken — and a single-file corpus cannot falsify a multi-file bug | B |
| 227 | A two-axis hazard gated on one axis — the seam passes, and the assertion it exists to prevent fires through it | A |
| 228 | A property implemented on one branch, documented as a property of the whole function | C |
| 229 | A seam named for a guarantee it delivers on one platform — and the permission model that lives in the directory, not the file | B |
| 230 | A clippy method ban does not cover the builder property of the same name | A |
| 231 | Retiring an ambiguous citation form by LEGALISING it instead of banning it — and a completeness claim with no predicate | B |
| 232 | `g_file_replace_contents` is atomic only under the right flags — and its one remaining fallback deletes the previous file before failing | A |
| 233 | Delegating a delimited format's unforgeable-terminator invariant to a third-party serialiser's escaping | C |
| 234 | Asserting one of a feature's two representations, and reading the green suite as evidence about both | B |
| 235 | Wiring a startup feature into one framework entry point and assuming it covers launch | A |
| 236 | A screen-coordinate capture is not a window capture | A |
| 237 | A `cfg`-gated gate proves nothing about the branches it did not compile | B |
| 238 | Activating on a click gesture's `released` alone — the release that ends a drag is not a click | A |
| 239 | `git stash pop` restores the source, not the binary — a control run that silently drives the old build | B |
| 240 | A detector that enumerates the VOCABULARY of a free-text citation is defeated by a synonym | B |
| 241 | A process NAME is not an identity — pid reuse defeats every liveness probe, on every platform | B |
| 242 | `clippy --all-targets` WITHOUT the feature flag reports dead-code errors in files you never touched | B |
| 243 | GLib's I/O thread pool is one process-wide pool of ten — moving I/O off the main thread makes it contend with the crash-recovery writer | A |
| 244 | Making a window-scoped operation async turns "which tab is active?" into two different questions | B |
| 245 | An Xvfb UI drive can deliver nothing and look exactly like one that delivered everything | A |
| 246 | GDK-Win32 refuses an empty window title and substitutes a literal "." | A |
| 247 | "No handler is registered for this scheme" is not a safety property | A |
| 248 | A randomly-minted identity correlates only with the mechanism that persisted it | B/C |
| 249 | A capability whose backend is a HELPER EXECUTABLE is a packaging obligation, and the dev tree cannot fail the test | B |
| 250 | A widget swapped in for one feature's sake moves its text out of every text-walker's reach | C |
| 251 | A distribution GTK has its entire introspection surface compiled out, and the three channels fail in three different ways — one of them by reporting a number that means "healthy" | A/B |
| 252 | A drive step routed through an app command inherits that command's own enablement gate — and a disabled `GAction` swallows the step in silence | A |
| 253 | The `org.gtk.Actions` probe answers about the operator's app when addressed by the well-known name — a `--new-instance` app must be probed by its UNIQUE name | A |
| 254 | An invariant held by two sufficient mechanisms is mutation-proof one at a time — so the mutation test calls each of them dead code | B |
| 255 | A construct whose glyphs are buffered at its `End` event is not opaque — it is char-precise in a coordinate space nobody wrote down | C |
| 256 | A gate's threshold is copied by hand out of a multi-metric report — so maintaining the gate is how you break it | B |
| 257 | `Trying to snapshot GtkGizmo … without a current allocation` is GTK's own scrollbar trough — the one benign member of a warning family whose other members are real bugs | A |
| 258 | Replacing a live `GtkTextView`'s buffer is a use-after-free, not a swap — the layout's line-display cache survives `set_buffer` and dangles | A |
| 259 | A rendering feature built for one of a construct's widget shapes leaves the others inert, and the reader sees one capability behaving at random | A |
| 260 | A `GtkTextView` scroll aimed past the lazily-validated frontier is parked and never re-issued — and the validation idle cancels the one scroll it does animate | A |
| 261 | A derived-state hook installed at the producer misses the rebuild shape the producer also has | C |
| 262 | A restore seam's "nothing to do at the boundary" shortcut is a claim about its first caller, and the second caller loses a real destination | B |
| 263 | `line_at_y` on a not-yet-allocated `GtkTextView` reports the buffer's LAST line, so a viewport read taken in the turn a view is built in is maximally wrong | A |
| 264 | A focused anchored child swallows its host `GtkTextView`'s navigation keys, and the document silently refuses to move | A |
| 265 | A test that arms a process-global fatal-signal handler and never disarms it re-points the rest of the suite — and displaces the runtime's own stack-overflow guard, so a later overflow stops naming itself | B |
| 266 | A focused `set_parent`ed popover is its own `GtkNative`, so every application accelerator silently stops working for as long as it is open | A |
| 267 | `GtkSingleSelection`'s builder property ORDER decides whether a list opens with a phantom selection | A |
| 268 | A GLib **fatal** log message dies of `SIGTRAP`, not `abort()` — a crash handler that enumerates the classic four signals reports nothing for the whole `g_error` class | A |
| 269 | Two sufficient monitor-cancel mechanisms made the rename guard pass with its own fix deleted — and a freshly attached `GFileMonitor` is not yet watching | A |
| 270 | Asking GIO what a file is called after you renamed it, and being told what you asked for | A |
| 271 | Matching a directory entry by `id::file`, which identifies the FILE and not the entry | A |
| 272 | A plan obligation written as a property of an artefact, which reads as done once the artefact exists | B |
| 273 | A runtime skip announcement shredded by libtest's own progress output — and one shred read `SKIPPED [rubric]: ok` | B |
| 274 | A provenance tally that counts measurements instead of outcomes, and so reports the opposite of its evidence | B |
| 275 | A `GFileMonitor` created while its parent DIRECTORY is absent is permanently dead on Windows, and self-heals everywhere else | A |
| 290 | A custom widget that CACHES child positions derived from child sizes re-derives them on nothing — `queue_resize` re-runs the layout against the stale cache | A |
| 291 | Every `GtkAdjustment` write is clamped to `upper − page_size`, so revealing a child added in the same turn scrolls short by exactly that child's width | A |
| 292 | GIO resolves an `https://` URI only where a GVfs backend claims the scheme, so a `GFile`-based remote fetch is dead on every platform without the Linux desktop stack | A |
| 293 | Sizing a drawn affordance from a row height measured in the VIEW's font, then requiring it to fit a container laid out in a TAG's font — the minimum-size case is the only one it fails, and it fails silently | A |
| 294 | Adding untestable wiring to an in-scope file drops the coverage ratchet under the floor, and the cheap fix hides the code rather than testing it | C |
| 295 | A PID-qualified AppleScript process reference silently decays to name resolution once stored in a variable — and name resolution picks a fixed process among duplicates, not the frontmost | B |
| 296 | A DERIVED screen coordinate is only as good as its derivation — a locator with a wrong assumption produces confident, repeatable, wrong numbers and reads as a defect in the app | B |
| 297 | Finalizing a CANCELLED `GFileMonitor` after the main context has dispatched corrupts the process heap on Windows — the application is safe by construction, not by a gate | A |
| 298 | A TIGHT list item's content arrives as bare inline events with no `Tag::Paragraph` — a consumer that paragraphs per event splits the item into one block per token, and a single-token fixture cannot see it | C |
| 299 | A suite-ordering defect that is deterministic on one platform and INVISIBLE in the canonical platform's full suite — the gate platform's green is the reassurance that hides it, and the seats that declare a step not-applicable stop being witnesses to it | B |


Stub legend: **Symptom** (one line) · **Scribobulate** (the project's implementation pointer) · **See** (skill module, and findings doc where one exists).

---

## Cross-cutting meta-pattern: GTK's deferred-work model and the single-threaded ordering-race family

**For the `gtk4-rs` skill maintainer — teach this model FIRST; a large fraction of the entries below are instances of it, and a user who holds the model recognises "that family" instead of rediscovering each instance by trial-and-error.**

GTK4 is single-threaded, yet many pitfalls below present as **races** — intermittent, timing-dependent, "a debug `eprintln` or a `sleep` makes them disappear." They are **not thread races**: no second thread touches the widget, and a mutex would not help. They are **temporal-ordering hazards inside the one main loop**. GTK defers its expensive work — layout, size-allocation, line-height validation, paint — off the synchronous call path and runs it later at ranked idle / frame-clock priorities. So state you set *now* is not *settled* now, and *which* deferred step runs first depends on wall-clock timing, document size (how many incremental validation passes), and external events (resize, focus). It is a single-threaded **eventual-consistency** system with a **leaky seam**: GTK exposes no clean "await settled" primitive, so you must **force**, **defer**, or **gate** manually. Compounding it, GObject **signals emit synchronously, mid-mutation, on the same call stack** (a long-standing default), so a setter can re-enter the very object you are in the middle of mutating. (These two — no await-settled primitive + synchronous mid-mutation signals — are the root design weaknesses; naming them tells the user *why* the discipline is required, not just the ritual.)

**The family (sub-class → member entries):**
- **Lazy height/layout validation** — a value read before the layout settles is stale / biased too-small: **#13, #14, #15, #22, #59, #65, ScrAP-162** (162 is the *resize* instance: a re-wrap's transient `upper` underestimate clamps `value` toward the top).
- **Size-allocation-pass timing** — a `queue_resize` / visibility change issued at the wrong point in the allocate cycle is dropped or re-enters: **#23, #29, #56**.
- **Animation-driven adjustment** — `scroll_to_mark` scrolls by animation, and `size_allocate` skips refreshing the adjustment *while it animates* → a frozen, collapsed scroll range: **#65**.
- **Synchronous signal re-entrancy** — a handler mutates/reads the same object mid-emission: **#16, #30, #53**.
- **Reuse / reparent across the deferred lifecycle** — a binding, closure, or cached index outlives the state it assumed: **#46, #52, #55, #58**.

**The correct way — four canonical mitigations, in order of preference:**
1. **Force the settle, then read.** `GtkTextView` line geometry is valid only after validation. **Myth-bust #1 — CONFIRMED VESTIGIAL (researcher, GTK 4.6.9 C-source trace):** `line_yrange(&end_iter)` does **not** force validation on *any* path — `gtk_text_view_get_line_yrange` (gtktextview.c:2452-2466) → `ensure_layout` (:7851-7942, only creates `priv->layout` if NULL, never validates lines before return) → `gtk_text_layout_get_line_yrange` (gtktextlayout.c:2985-3008) → btree cached-height reads (gtktextbtree.c:1464-1529/5583-5596); it never calls `validate_yrange` / `validate` / `validate_line` / `gtk_text_layout_wrap`. The returned `y`/`height` are always already-cached (stale/zero when unvalidated). It is a **pure read whose result you discard** — so a `line_yrange` call placed purely as a "validation primer" is dead code and should be **deleted** (done: the former primers in `codeview.rs::scroll_to_buffer_offset`, `preview/scroll.rs::restore_textview_scroll_to_line`, and the `window/zoom.rs` restore have all been removed — no behavioral change). One nuance: the *first-ever* `ensure_layout` on a view can create the layout object and queue invalidate idles as a construction side effect, but that still doesn't validate before returning and is a no-op once the layout exists (the case at every already-shown restore site). GTK's genuine force paths are its internal `validate_onscreen` / `validate_yrange` / `flush_scroll` / incremental validation; at the public API you reach them by a deferred `scroll_to_mark` (it calls `flush_scroll`, which validates) rather than by a bare geometry getter. So treat "measure geometry" and "guarantee validation" as two steps: schedule the read past the settle (mitigation 2) and/or trigger a real flush, don't rely on a getter to validate. Gating on `page_size > 0` is necessary but **not** sufficient (#13). (The `gtk4-rs` skill §22 already states this correctly.) **Distinct, still-valid use:** `line_yrange(&mark)` where you *consume* `.y` (e.g. #82's progressive `set_value(line_yrange(mark).y)` off `notify::upper`) is a legitimate read of the current cached offset — that is NOT a primer and stays.
2. **Defer your action past the settle.** Run it on `glib::idle_add_local_once` (`G_PRIORITY_DEFAULT_IDLE` = 200: fires *after the current idle wave* — after `first_validate` 108, `REDRAW`/paint 120, and incremental `VALIDATE` 125 in the same loop iteration — but this is **not** a guarantee the buffer is *fully* validated, and there is **no** GTK4 "RESIZE/VALIDATE idle" driving layout: layout runs in frame-clock phases), **once, after the final mutation** — never synchronously inside the notification that triggered it (#14, #16). Note `add_tick_callback` is the **wrong** tool for "read settled geometry this frame": it fires in the frame-clock **UPDATE** phase, which runs **before** layout/allocate (allocate is later in the same `paint_idle` at `REDRAW` pri 120; gtkwidget.c:3067-3069). Use `add_tick_callback` only to coalesce *your own* per-frame work, not to read post-layout geometry. Gate on a settling proxy signal (`notify::upper`, `changed`) when you need the exact moment the height changed.

**Deferred-work priority ladder (GTK 4.6.9, one main-loop iteration):** frame-clock `tick`/**UPDATE** phase (pre-layout) · `first_validate` 108 (RESIZE−2) · `REDRAW`/paint 120 (layout/allocate happens here) · incremental `VALIDATE` 125 · `DEFAULT_IDLE` 200 (`idle_add_local_once`). "After this wave" (200) ≠ "fully validated."
3. **Cache the intent; never re-derive from live-but-transient state.** Store the target as a stable anchor (a buffer **line** or a persistent `GtkTextMark`), not a fraction re-read from an adjustment a prior animation may still be driving (#65's `user_scrolling`-gated line cache). Inverse warning: a cached raw index/pointer a reorder or reparent silently invalidates (#55, #46).
4. **Make it idempotent / re-convergent.** Prefer a coalesced per-frame projection that re-runs harmlessly until it converges over synchronous mirroring that oscillates (#16, the `GtkSourceMap` pattern).

**Re-entrancy corollary:** never hold a `RefCell` borrow across a GTK setter that can synchronously re-enter (#53); never unparent/dismiss a widget from inside its own descendant's handler (#30); defer such structural mutations to an idle. Assume every setter may emit signals synchronously before it returns.

**Primitives cheat-sheet (encode the right tool next to every "don't"):**

| Need | Use | Not |
|------|-----|-----|
| Force text layout valid *now* | a deferred `scroll_to_mark` (calls `flush_scroll` → validates) / skill §22 primitive | `line_yrange(&end_iter)` as a *forcing* call — it's a **stale cached-height read**, not a validation force; also reading `upper`/`value` right after `set_buffer` (biased low) |
| Scroll a `TextView`, validation-safe | `scroll_to_mark` + a **persistent** mark (deferred) | `scroll_to_iter` (immediate, pre-validation → blanks) |
| Run after the current idle wave (≠ fully validated) | `idle_add_local_once` (`DEFAULT_IDLE` 200) | acting inside the triggering signal |
| Coalesce *your own* work once per frame (**pre-layout**) | `add_tick_callback` (UPDATE phase) | synchronous mirroring in `value-changed`; **and** don't use it to read post-layout geometry |
| Un-freeze an adjustment after an animated scroll | a **non-animating** `set_value` clamp (`queue_resize`) | assuming `size_allocate` refreshes it (it won't *while animating*) |
| Preserve a reading position across a re-render **or a resize** | a buffer **line** / mark anchor, restored after validation (resize: re-anchor on a raw-width change — ScrAP-162) | a pixel `value/(upper − page_size)` fraction; a one-shot `set_value` (the re-wrap clamp re-resets it) |

**Numbering reconciliation — this register's entries are cited as `ScrAP-N`; the `gtk4-rs` skill's as `AP-N`.** The two registers were seeded together, but the skill maintainer now assigns its own independent ids, so the numbers are **not** a 1:1 map — this file's `## 74.`–`## 77.` are unrelated to the skill's 74–77, and the overlap is total across the shared range. Sharing the `AP-` prefix made every cross-citation resolve to a real-but-wrong entry (#145), so **this register renames**: `ScrAP-N` is unique, greppable, and unambiguous wherever it is typed — in this file, in `sdd/`, and in code comments alike. There is no longer a positional rule to remember (the old "bare `AP-N` means the project outside this file, the skill inside it" is retired, and it is what #145 records the cost of). Write `ScrAP-N` for an entry here and `GTK4Rs/AP-N` for a skill entry — one token each, never a bare number and never two words (a bare `#N` inside this file's own body is the local shorthand for an entry here). The bare form is now **illegal and gated** (`lint-references` check 8); the first sweep merely legalised it and claimed completeness it did not have, which #231 records with the measurement. Confirmed skill-side landings (See-links repointed accordingly): this file's ScrAP-55→GTK4Rs/AP-74, ScrAP-62→GTK4Rs/AP-75, ScrAP-63→GTK4Rs/AP-76, ScrAP-64→GTK4Rs/AP-77. Further See-stubs are repointed only once the maintainer confirms each has landed.

**Skill-maintainer checklist:** (1) lead the skill with this model, then present the specific entries as instances; (2) state up front these are single-threaded *ordering* hazards, not thread races — dispels the instinct to reach for locks; (3) pair every "don't" with the primitive that does it right (the table); (4) name the leaky-seam root cause so the user understands *why*; (5) the deferral priorities and settle-points are barely documented — verifying against `gtktextview.c` / `gtkwidget.c` source (or routing to a researcher) is the reliable path when a **new** instance appears, and the finding should be folded back here + into the skill.

---

## 1. Rendering a document viewer with a GPU-compositing UI stack
**Symptom**: a GPU-canvas/WebKit-class document viewer holds hundreds of MiB of VRAM to display one Markdown file.
**Scribobulate**: the founding decision — native GTK4 + Cairo **software** rendering (no GPU compositor); see PRODUCT.md / TECH.md.
**See**: gtk4-rs skill → architecture-and-rendering (GTK4Rs/AP-1).

## 2. Assuming "disable hardware acceleration" makes a web engine render on the CPU
**Symptom**: a GTK4 + WebKitGTK build set `HardwareAccelerationPolicy::Never` yet still allocated GPU/VRAM.
**Scribobulate**: WebKit rejected outright; native widgets only.
**See**: gtk4-rs skill → architecture-and-rendering (GTK4Rs/AP-2).

## 3. Using environment variables to prevent GTK from crashing on a large XCompose file
**Symptom**: GTK 4.6 aborts at startup parsing a large `~/.XCompose`; an `XDG_CONFIG_HOME` env redirect alone does not prevent it.
**Scribobulate**: a dedicated startup workaround function, run **before** GTK init.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-3).

## 4. Using Pango `<a href>` markup in GtkLabel for standalone link widgets
> *Non-core (Pango) — condensed; full essay in git history.*
> **⚠ Its stated reasons were CORRECTED by #259 (measured against the 4.6.9 source);
> its conclusion stands on different grounds. Read both before citing this entry.**

**Symptom**: a link rendered via Pango `<a href>` in a `GtkLabel` was believed to style and activate but with no pointer cursor on hover and activation on button-*press* rather than *release*.
**Root cause** *(as recorded — since disproved)*: `GtkLabel` was thought to handle `<a href>` without `GtkLinkButton`'s full interaction model. **At 4.6.9 it does both**: `gtk_label_update_cursor` sets `"pointer"` over an active link (`gtklabel.c:737`) and `gtk_label_click_gesture_released` emits `activate-link` on **release** (`:4400`). See #259.
**Resolution**: for a cell that IS a single link, use `GtkLinkButton` (`has_frame = false`) — now on its real merits (focusable, carries the URL as a property, frame-less button padding), not on an interaction deficit that does not exist. Pango `<a href>` is the correct and fully-featured route for an *inline* link inside mixed-content label text (bold/italic/link interleaved), where `connect_activate_link` is the open hook — and it must return `Propagation::Stop`, because `GtkLabel`'s default handler `gtk_show_uri`s the raw href (`:2081`).

## 5. Reading GtkTextBuffer text to track trailing newlines when child anchors are present
**Symptom**: counting trailing blank lines by inspecting buffer contents miscounts when child anchors are present — `get_text` **drops** each anchor entirely (#74) while `get_slice`/iters count it as one `U+FFFC`.
**Scribobulate**: an explicit trailing-newline counter maintained alongside rendering (with reset sites), never buffer-content inspection.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-5); #74 (the get_text-vs-get_slice offset-basis distinction).

## 6. Using a horizontal rule to indicate a blockquote
**MERGED into #21** — same mechanism (self-drawn block chrome in the view's snapshot layer over the block's buffer range). Retained as a numbered landing spot.

## 7. Placing the blockquote `DrawingArea` in an outer overlay outside the `ScrolledWindow`
**SUPERSEDED** — no `DrawingArea` exists; all block chrome is self-drawn in the view's snapshot layer in buffer coords (scroll-correct by construction). Superseded by #22 (snapshot-layer measure discipline) and the merged block-chrome entry #21. Retained as a numbered landing spot.

## 8. Redirecting `XDG_CONFIG_HOME` without carrying `mimeapps.list`
**Symptom**: after the XCompose workaround redirects `XDG_CONFIG_HOME`, the desktop loses the user's default-application associations (`mimeapps.list`) → opening links fails.
**Scribobulate**: the `mimeapps.list` symlink created inside the startup XCompose workaround.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-8).

## 9. Duplicating action logic across context menu, main menu, and keyboard shortcut
**Symptom**: the main-menu Copy stayed enabled regardless of selection — the surfaces drifted out of sync.
**Scribobulate**: one `win.copy` `SimpleAction`; every surface (menu / toolbar / context menu / accelerator) binds to it by name — POLICY single-source-of-truth.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-9).

## 10. Walking the widget tree to re-discover anchor-embedded GtkLabel widgets
**Symptom**: a recursive `find_selectable_labels()` tree-walk to re-find anchored cell labels is fragile and breaks across re-renders.
**Scribobulate**: a qdata handoff from the render pass to the copy-action wiring step.
**Now enforced by**: `saferizer::QdataKey<T>` — the handoff is a phantom-typed const; a key/type mismatch is unrepresentable and no `unsafe` reaches the call site.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-10).

## 11. Expecting `g_menu_item_set_icon()` to render icons in a GTK4 menu bar
**Symptom**: `item.set_icon(...)` on a `GMenuItem` renders nothing in a `GtkPopoverMenuBar`.
**Scribobulate**: text-only menu items; icons live on the toolbar buttons.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-11).

## 12. Using untyped GLib qdata as the canonical store for per-window state
**Symptom**: per-window state spread across many untyped qdata keys is unsafe and unmaintainable.
**Scribobulate**: a typed per-window state registry (`thread_local`, keyed by window pointer), plus pure decision fns.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-12).

## 13. Restoring GtkTextView scroll via a GTK adjustment (on `changed`, or `set_value` after `set_buffer`) before the layout has validated
**Symptom**: restoring on `changed` fires before layout settles, and `adj.set_value()` after `set_buffer` is ignored — the new layout is not validated yet. `page_size > 0` is a legitimate GATE but NOT a completion signal (gtkviewport.c:169-175 nonzero-while-draft; gtkadjustment.c:207-209 resting zero), so it is necessary-not-sufficient.
**Scribobulate**: never restore via the adjustment; the buffer-offset scroll helper uses a persistent left-gravity mark + `scroll_to_mark` on a coalesced idle. (Taxonomy: Resistant — stays prose.)
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-13 / §13, §14).

## 14. Restoring `GtkTextView` scroll via adjustment manipulation after `set_buffer`
**MERGED into #13** — identical fix, seam, and class (adjustment vs `scroll_to_mark` after `set_buffer`). Retained as a landing spot; `ScrAP-14` src citations resolve here.

## 15. Using `iter_at_location` to get the iter at the top of a `GtkTextView` viewport
**Symptom**: `iter_at_location` is an over-a-glyph hit-test (returns None at x=0 / in the margins), so it is wrong for the top-of-viewport iter.
**Scribobulate**: top iter from `visible_rect().y()` + `line_at_y`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-15, §15).

## 16. Mirroring split-pane scroll synchronously inside `value-changed`
**Symptom**: mirroring a follower pane synchronously in `value-changed` oscillates wildly during the re-render validation thrash.
**Scribobulate**: a coalesced once-per-frame scroll-sync projection (GtkSourceMap pattern), on `value-changed` AND `notify::upper`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-16, §16); findings: scroll-sync-validation-coalescing.md.

## 17. Parsing a "new instance" CLI switch after the GApplication has registered
**Symptom**: a `--new-instance` flag read too late — argv is forwarded to the primary at *register*, before activate/open/command-line.
**Scribobulate**: decide `NON_UNIQUE` (same app-id) **before** the GApplication registers, at the application's construction site.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-17).

## 18. Bridging glib↔Rust `log` with the wrong handler (stack overflow / dropped Gtk-CRITICAL)
**Symptom**: pairing `GlibLogger` with `log_set_writer_func` recurses to a stack overflow; `log_set_default_handler` drops structured `Gtk-CRITICAL` records.
**Scribobulate**: a single one-direction writer bridge (`forward` + `init`) wired first in `main()`; POLICY.md §Logging.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-18).

## 19. Trying to override a `GtkDropDown`'s empty-state "(None)" caption
**Symptom**: a `GtkDropDown` used as a momentary picker shows a "(None)" caption you cannot relabel.
**Scribobulate**: the heading control is a `GtkMenuButton` with a `(Hn)` caption, not a `GtkDropDown`.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-19).

## 20. Gating a widget-scoped action on raw per-widget focus
**Symptom**: gating a command on raw per-widget focus flickers and desensitises it while the user is operating an adjacent surface.
**Scribobulate**: a window-level `connect_focus_widget_notify` + `is_ancestor` *sticky* gate (transient surfaces — toolbar, menus, popovers, find bar — leave it untouched); `set_focus_on_click(false)` on the Format buttons, wired via a dedicated focus-gate setup function.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-20).

## 21. Indicating a text block (code block / blockquote) with the wrong GTK primitive instead of self-drawing it
**Symptom**: a `paragraph-background` tag's fill is pinned to the text margin — it cannot pad a code block; and a blockquote drawn as a horizontal rule (or an anchored `DrawingArea` bar) is both wrong and does not scroll with the content.
**Scribobulate**: block chrome is self-drawn in the view's own draw/snapshot layer — code-block backgrounds on the BelowText layer (text inset past the rect by the code-block tag's margins), and the blockquote accent bar over the quote's buffer range. Blockquotes and code are buffer text, never widgets.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-21, §6/§21).

## 22. Forcing `GtkTextView` layout validation inside the snapshot/draw or size-allocate path
**Symptom**: measuring full content extents inside the draw forces layout validation, which blanks the view.
**Scribobulate**: the preview view measures block extents visible-only, clamped to the viewport, in the draw/snapshot path — never a full-content measure.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-22).

## 23. Embedding height-for-width block content as a widget at a `GtkTextChildAnchor`
**Symptom**: an anchored height-for-width widget (a wide table) re-arms "snapshot … without a current allocation" — the validator measures it at its OWN min width, never the viewport, so a width-clamp can't fix it.
**Scribobulate**: renders 1-D content as buffer text with self-drawn chrome. FOUR widget kinds anchor at a `GtkTextChildAnchor`, but only TWO carry the width-dependent height-for-width contract: tables (`ScribTableWidget`) and images (`GtkPicture`) are constant-size widgets whose width is cached and re-bounded from the view's live content column, reserving GTK's cursor-space slack so they never overflow into a scrollbar. The other two need no width-bounding — a horizontal rule is a stock `GtkSeparator` (`renderer/events.rs` `Event::Rule`; the only anchored widget the app doesn't build, `preview/css.rs` `.scrib-rule`), and a failed image decode anchors an `image-missing` `GtkImage` placeholder (fixed-size icon, `preview/build.rs`). "Tables and images" understates the anchored set.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-23); findings: custom-anchored-table-widget-contract.md.

## 23a. Bounding an anchored child to the content column while it sits at an INDENTED margin
**Symptom**: on a table-heavy document a *marginal* horizontal overflow appears in the preview — `hadjustment.upper` exceeds `page_size` by a fixed handful of px (~27px in the field case), summoning the Automatic h-scrollbar whose appear/disappear re-arms the ScrAP-22/ScrAP-23 width↔height-for-width churn → the pane intermittently blanks until a forced re-render. It is NOT `SPACE_FOR_CURSOR`/`ANCHORED_LINE_END_SLACK` (those are 1px) and NOT a cell's residual minimum.
**Root cause**: ScrAP-23's width-bounding (`CodePreviewView::size_allocate`) bounds every anchored child to `content − 1`, where `content = width − view.left_margin − view.right_margin` — i.e. as if the child began at the view's content EDGE. But an anchored child inside a **list item** or **blockquote** is laid out at that block's INDENTED margin (its line carries the `li-{depth}` / `blockquote` left-margin tag), so its real extent is `indent + bound`, which overflows the viewport by exactly `indent`. Diagnosed by a `#[gtk::test]` scanning every char's laid-out right edge: the widest point was a table anchored at x = view-margin + list-indent, at the full column width. Two indent sources compound and differ in shape: a **list** adds only a LEFT margin (`depth * list_step`), a **blockquote** sets BOTH a left and a right margin (`tags.rs` `view_lm+bar+gap` / `view_rm+bar+gap`), so it steals `2 × (bar+gap)`. Horizontal rules had the same latent bug (their `width_bounded` inset was hard-coded `0`).
**Tempting wrong fix**: "pin the horizontal scrollbar policy to `Never`/`External` so a marginal over-wide can't toggle it." Rejected — researcher-verified (gtk-4.6, documented at `preview/render.rs`): `Never` makes `GtkScrolledWindow` adopt the child's *minimum* width and ratchet (the child never re-wraps on shrink, `gtkscrolledwindow.c:1817-1820`), so the window can no longer shrink to fit. The policy MUST stay `Automatic`; the invariant to uphold is "content never goes over-wide" (ScrAP-23), not "hide the bar."
**Resolution**: give every anchored bounded child an **inset** = the horizontal margin its enclosing block steals, and bound it to `content − 1 − inset`. `Renderer::block_inset()` (`renderer/emit.rs`) computes it from the renderer's live list depth + blockquote state — list = left-only `depth*list_step`, blockquote = both-sides `2*(bar+gap)`, zoom-scaled — reusing the SAME theme metrics `tags.rs` applies (one source of truth). Tables store it via `ScribTableWidget::set_bound_inset` (subtracted in `set_bound_width`); rules pass it through the existing per-child `width_bounded` inset. Zero for top-level content ⇒ no-op there. Measured 27px → 0px on the field fixture. Guarded by a **mutation-checked** regression test (`preview::render` `indented_wide_table_does_not_force_a_horizontal_scrollbar` — reverting `block_inset` to 0 fails it) + a widget unit test (`set_bound_inset_shrinks_the_fit_target`). CONFIRMED LIVE (2026-07-20, operator's KDE/X11 + breeze-dark, previewing a doc with indented wide tables — no h-scrollbar, no blank; GTK4Rs/AP-56 gate satisfied). Field footnote: a stale pre-fix binary re-exhibits the bar, so "still see it" after a fix here means *rebuild first*.
**See**: extends #23 (this is the indent axis #23's "reserving cursor slack so they never overflow into a scrollbar" did not cover); gtk4-rs skill → textview-anchored-and-integration.

## 24. Relying on the system theme to paint a `GtkTreeExpander`'s disclosure chevron
**Symptom**: the `GtkTreeExpander` disclosure chevron renders blank under some themes.
**Scribobulate**: the outline view supplies its own chevron CSS.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-24).

## 25. Assuming the `.heading` / `.title-N` typographic CSS classes require libadwaita
**Symptom**: assuming libadwaita is needed for the `.heading`/`.title-N` typographic classes.
**Scribobulate**: the "Outline" sidebar caption uses the `.heading` class with plain GTK CSS — no libadwaita dependency.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-25).

## 26. Pointing a `GtkPopover` at an anchor rect outside the visible viewport
**Symptom**: pointing a popover at an off-viewport rect makes GTK allocate negative content height and trips `gdk_monitor_get_geometry` (no on-screen placement).
**Scribobulate**: the caret-format overlay's positioning logic guards/clamps the anchor into the visible viewport before `set_pointing_to`.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-26).

## 27. Searching find-next from the caret after `select_range` (re-finds the current match)
> *Non-core (GtkSourceView) — condensed; full essay in git history.*

**Layer**: GtkSourceView / `GtkTextBuffer` (`GtkSourceSearchContext`).
**Symptom**: "Find next" sticks on the current match while "find previous" works — the asymmetry is the tell.
**Root cause**: `select_range(ins, bound)` parks the caret (`cursor-position`) at the match **start**; `sc.forward(caret)` returns the first match at-or-after the caret — the same match, forever. Backward happens to advance, so only *next* looks broken.
**Resolution**: step from the **far edge of the current selection in the direction of travel** — forward from `selection_bounds().end`, backward from `.start`; fall back to the caret only with no selection. Same rule for a plain `TextIter::forward_search` loop.

## 28. Tracking a selectable `GtkLabel`'s selection via a `notify::cursor-position`
**Symptom**: `GtkLabel` has **no** `cursor-position`/selection property, so a `notify::cursor-position` handler is a silent no-op — the dependent action never updates.
**Scribobulate**: a copy-enabled recomputation reads buffer + the anchored-label handoff fresh, driven by the buffer `has-selection` and the **primary clipboard** `changed` signal (handler id tracked via qdata, disconnected before re-adding to avoid accumulation).
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-28); threading-async-and-memory.

## 29. Re-laying-out an anchored child via `queue_resize` after `parent_size_allocate` in the same `size_allocate`
**Symptom**: a `queue_resize` after chaining up `parent_size_allocate` is dropped that pass → the child stays `alloc_needed` → next snapshot bails ("snapshot … without a current allocation").
**Scribobulate**: the view's `size_allocate` override derives the bound from the `width` argument and applies it to the anchored children's bound-width **before** chaining up, so children re-validate in the same pass.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-29).

## 30. Dismissing/unparenting a popover from inside a descendant's click handler
**Symptom**: `popover.popdown()` (→ `closed` → `unparent`) synchronously inside its own `clicked` tears the subtree down mid-dispatch → "Broken accounting of active state" + a burst of `g_object_unref: G_IS_OBJECT`.
**Scribobulate**: the context-menu dismiss path defers the popdown to `glib::idle_add_local_once`, out of the event dispatch.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-30); findings: popover-teardown-in-handler.md.

## 31. Resolving an untrusted document's local image `src` against the CWD (or with a lexical-only containment check)
**Symptom**: a Markdown `![alt](img.png)` renders blank — `gdk::Texture::from_file(gio::File::for_path("img.png"))` resolves the **relative** path against the process **CWD**, not the document's folder; and a containment gate that only checks `..`/absolute *lexically* is blind to a symlink under the doc dir whose target escapes it.
**Scribobulate**: the contained-image resolver joins the document directory with the source path, then `dunce::canonicalize`s it (resolves `..` **and** symlinks), admitting the result only if it `starts_with` the canonicalized document directory — **component-wise `Path::starts_with`**, never a string prefix.
**See**: general-engineering-principles (GEP-46); gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-57, incl. the unresolvable-vs-refused enum split, GTK4Rs/AP-34b); findings: researcher-findings-image-src-path-containment.md.

## 32. Anchoring a `GtkPicture` in a `GtkTextView` without a nonzero width request
**Symptom**: a Markdown image loads (valid `GdkTexture`) but the anchored `GtkPicture` renders BLANK (height 0) — `can_shrink` reports `min_width = 0`, so `GtkTextView` measures height at `for_size = 0` → 0 (image face of ScrAP-22/ScrAP-23).
**Scribobulate**: the image-rendering path sets a definite `set_size_request(seed_w, seed_h)`; the view's `size_allocate` re-clamps it to the live column on each real width change — `w = min(natural, content)`, `h = max_h·w/max_w` (aspect preserved, `max-width: 100%`, no upscaling past natural).
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-58); findings: researcher-findings-anchored-picture-blank.md.

## 33. Testing a rebuilt single-instance GApplication while a stale primary is still running
**Symptom**: a rebuilt binary appears to have NO effect — it behaves like an older build.
**Scribobulate**: launch with `--new-instance` / `-n` (ScrAP-17) when verifying a change interactively, or quit the running primary first; headless smoke tests already use `-n`.
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-43).

## 34. Remote image loading blocks the main thread; "refused" ≠ "unresolvable" in a multi-outcome resolution enum
**Symptom (34a)**: `Texture::from_file(&gio::File::for_uri(url))` is synchronous even for a remote URI — rendering remote images freezes the UI per fetch. **(34b)**: an untitled buffer showed a broken-image icon (implies "refused") for an unresolvable relative `src` because the gate collapsed both `None` reasons into `Refused`; the same collapse survived one level deeper for four more months — a *contained* image whose file had not landed yet still read as blocked, and since the placeholder's tooltip names its reason, it told the reader to switch the safety gate OFF to see a file the gate had never objected to (a live reload is where this bites: a document and its images arrive together and the image can lose the race by a frame).
**Scribobulate**: 34a accepted for the opt-in "Show Unsafe Images" path (the image-tag rendering site); 34b — the containment gate reports its *reason* (`Containment::Inside`/`Escapes`/`Absent`) through **one** routine both the image and link resolvers call, so `Refused` means a file that is really there and outside, and everything else unresolvable is `Missing`. The lesson the second half adds: an enum split applied at the call site fixes the case in front of you and leaves the gate itself still collapsing — **split it where the information is lost, not where the symptom appeared**, or the next caller inherits the same collapse (the link resolver had disambiguated by hand since it was written; the image one never did).
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-44); app-lifecycle-and-env (GTK4Rs/AP-57 / GTK4Rs/AP-34b enum split).

## 36. Letting the editor `GtkSourceSearchContext` `notify::occurrences-count` overwrite the preview buffer's `forward_search` count in preview mode
> *Non-core (GtkSourceView) root cause — condensed; full essay in git history. The
> core-GTK cell-highlight/two-step-scroll mechanics were extracted to the skill (GTK4Rs/AP-46).*

**Symptom**: in preview mode the find count shows more matches than navigation can reach — the editor source context counts `| cell |` markdown the preview buffer can't navigate to (table cell text lives in `GtkLabel` child widgets, never in the buffer's btree).
**Root cause**: `set_search_text()` triggers `notify::occurrences-count` on the *editor* context, whose handler ran unconditionally and overwrote the correct preview (body-only) count with the inflated editor count.
**Scribobulate**: gate the handler with an early return when the active view mode is preview; reach cell text via a dedicated preview-hits builder.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-46, cell-highlight + two-step scroll); findings: researcher-findings-textview-search-anchored-cell-text.md.

## 37. `GtkTextView` never repaints an anchored child when a descendant's Pango background is REMOVED
**Symptom**: a table-cell Pango-background find highlight goes stale on removal/shrink (adding shows fine) — a genuine upstream GTK bug (wrong predicate at the `allocate_children` gate, unchanged 4.6.9 → `main`).
**Scribobulate**: the preview highlight-apply path paints a **match-only** highlight (no base) and forces the anchored child to re-snapshot on every add/recolour/removal via a dedicated cell-repaint forcer — the #117 transient no-attr `<span>`-wrapper markup toggle.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-45); #117 (the forced-repaint primitive).

## 35. Reading `st.source` for a programmatic preview re-render in split mode
**Symptom**: in split mode a programmatic preview re-render (zoom/toggle/theme) uses stale content — just-typed editor text vanishes until a mode round-trip.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the split-mode preview re-render site.

## 38. Driving derived UI state from a delta-only signal, missing lifecycle boundary events
**Symptom**: find-bar highlights absent when the bar is reopened with the same term (no new typing → no `search-changed` to drive them); the outline scroll-spy's `value-changed`-derived highlight similarly stayed stale across a mode/window boundary that preserves scroll position but fires no delta. **The same lesson bit again at THREE more boundaries** (2026-07-19): the preview find-match highlights (`scrib-search-hl` buffer tags + table-cell Pango attrs) were erased by **theme switch**, **view-mode switch** (edit↔split↔preview), and **external reload** — each installs a FRESH preview `GtkTextBuffer`, which carries none of the tags, so the matches vanished until the user next edited the query or stepped a match (which re-runs `search-changed`). The highlight is derived state layered on the buffer; every boundary that rebuilds the buffer must re-apply it, exactly as `refresh_outline`/`refresh_annotations` already were in those same sweeps — find was the overlay left out.
**Scribobulate**: the find bar re-runs its search-changed logic on reveal, the outline scroll-spy fires an explicit deferred initial scroll in every mode, and the preview find-highlight re-sync now runs at **every** preview-rebuild boundary — tab switch (`tabs/switch.rs`, pre-existing), theme sweep (`re_render_all_windows`), mode switch (`viewactions.rs`), and external reload (`reload.rs`).
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-47); POLICY Document Rendering CAM row 8.

## 39. Specifying GNOME-specific icon names absent from non-GNOME themes
**Symptom**: toolbar icons render as a ⚠ placeholder — the name is a valid Adwaita icon but Adwaita is not in the active theme's inheritance chain (e.g. `breeze-dark → breeze → hicolor`).
**Scribobulate**: the split-arrangement buttons and the view-command table use icon names confirmed present in `breeze-dark/actions/symbolic/` as well as Adwaita.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-48); inverse hazard (bundled name PRESENT in the host theme → theme overrides your bundle) — see #85.

## 40. `GtkAboutDialog` `authors` entries with `<url>` format open `mailto:`
**Symptom**: clicking a link in the Credits tab of `GtkAboutDialog` launches the default email client with a `mailto:https://…` URI instead of the browser.
**Scribobulate**: the About-dialog action — removed the `<url>` from `authors`; use `.website()` + `.website_label()` instead.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-50).

## 41. `FileChooserNative`/transient-dialog lifetime is backend- and widget-type-dependent
**Symptom**: a leak fix that uniformly converted every dialog's self-closure from strong `.clone()` to weak `.downgrade()` broke every `FileChooserNative` dialog outright (failed to appear/respond).
**Scribobulate**: split by type — `gtk::Window` dialogs keep a weak self-ref (toplevel list pins them) while any `NativeDialog` keeps ONE strong ref in an external `Rc<RefCell<Option<…>>>` holder, dropped in `connect_response` after `.destroy()`.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-41).

## 42. Predictable, reused path under the shared temp dir for a config-redirect workaround (security)
**Symptom**: a fixed, predictable, world-writable path (`temp_dir().join(...)`) trusted as "not yet created" lets a local attacker pre-plant `settings.ini`, achieving code execution via its `dlopen()`-capable `gtk-modules` key.
**Scribobulate**: the temp-dir helper prefers `$XDG_RUNTIME_DIR` (0700) and makes a PID+timestamp dir with exclusive no-clobber semantics (`DirBuilder::mode(0o700).create`, fails on `AlreadyExists`).
**See**: general-engineering-principles (GEP-47); gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-8, the predictable-temp-path security subsection).

## 43. Relying on `GtkNotebook`'s `create-window` signal for "drag a tab to the desktop to spawn a new window" on Wayland
**Symptom**: `set_group_name` + `create-window` looks complete, but on Wayland a tab dragged to the bare desktop snaps back (X11 works with identical code) — the cancel-reason guard only passes for X11's `NO_TARGET`.
**Scribobulate**: the custom tab widget that superseded `GtkNotebook` (#50) reimplements the drag-to-desktop path portably — a `GtkDragSource`/`GtkDropTarget` pair whose cancel handler treats `DragCancelReason::NoTarget` as "spawn a new window hosting this tab" (`window/tabs/dnd.rs`), plus a first-class portable **"Move Tab to New Window"** action (`window/tabs/actions.rs`). No `GtkNotebook` `set_group_name`/`create-window` remains.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-49).

## 44. Using a `<Shift>` + digit/punctuation `GtkApplication` accelerator
**Symptom**: `<Shift>` + a digit/punctuation accelerator (e.g. `<Alt><Shift>2`) never fires on a real keyboard — menu/toolbar work, the key combo does nothing, no warning. `<Shift>` + a letter is fine.
**Scribobulate**: the view- and format-command tables drop `<Shift>` and use another modifier on the same physical key (`<Alt><Shift>2` → `<Alt>2`).
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-51).

## 45. A `GtkNotebook` with `show-tabs` false cannot be a cross-window tab-drag drop target
**SUPERSEDED by #50** — `GtkNotebook` is gone (replaced by the custom `widgets/tab/` strip), so the hidden-strip drop-target failure can no longer occur by construction. NOTE: the `#45` tokens in #115/#117 refer to the *gtk4-rs skill's* GTK4Rs/AP-45 (anchored-child repaint on ink removal), not this register entry. Retained as a numbered landing spot.

## 46. An idempotent signal-rewire check keyed only on widget identity misses a stale closure after a cross-window reparent
**Symptom**: the outline scroll-spy stopped updating for a tab drag-moved between windows (preview scrolled fine, outline never re-highlighted).
**Scribobulate**: the per-tab scroll-spy connection state adds a third field — the bound window pointer — alongside the `ScrolledWindow` + `SignalHandlerId`. Superseded for scroll-spy specifically by the ScrAP-57 dynamic host-window resolution pattern; the window-pointer field remains a harmless belt-and-suspenders for the case where the scroller itself is genuinely replaced.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-52); ScrAP-57 (the host_window resolution pattern this was migrated to).

## 47. `gtk_window_present()` from a D-Bus `open` handler doesn't raise+focus a tokenless (bare-terminal) launch — legitimate WM behavior, not a bug
**Symptom**: a second `scribobulate <path>` typed in a terminal forwards over D-Bus correctly (no dup window) but never raises/focuses the existing window.
**Scribobulate**: no code change — a bare shell launch carries no `DESKTOP_STARTUP_ID`, so user-time is 0 and WM focus-stealing-prevention correctly declines. TDD 8.2 tests the tokened path.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-54); #129 (the launch-path sibling — same literal `0`, opposite meaning, decided by which GDK path consumes it). Researcher-sourced against gtk-4-6 @492b44f20c (4.6.9).

## 48. Adding an ancestor `GtkEventControllerKey` for Escape doesn't catch Escape while a `GtkSearchEntry` descendant has focus
**Symptom**: a find bar's ancestor-`EventControllerKey` Escape-to-close works for every widget except the `GtkSearchEntry` itself.
**Scribobulate**: `GtkSearchEntry`'s own class keybinding (`Escape` → `stop-search`) consumes it first; the find-bar wiring shares one close closure across the button, the ancestor controller, and `connect_stop_search`.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-53).

## 49. `cargo valgrind` on a GTK4 app reports hundreds of toolkit-internal "leak"/uninitialised-value errors that are NOT application bugs — but valgrind still catches real app UAFs here, so triage by stack, don't dismiss wholesale
**Symptom**: `cargo valgrind run` against a live window reports ~700 errors whose stacks bottom out in `gtk_at_context_create`, Pango, Fontconfig, and IM compose code — symbol-less, none naming a `scribobulate::` frame.
**Scribobulate**: for that ~700-error live-window run, grepping every stack for a `scribobulate::` frame found zero — GTK4's by-design OS-reclaims-at-exit retained memory absent a `gtk.supp`/`glib.supp` suppression file. But the technique is triage, not blanket dismissal: valgrind DID pinpoint a real read-after-free on this project — the reused `GtkSourceView` gutter's unbound `vadjustment` binding fired against a tree mid-teardown (#58, "Valgrind proved the read-after-free directly"). A stack that names a `scribobulate::` frame is signal, not noise.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-59).

## 50. `GtkNotebook`'s native cross-window tab-detach DnD is unsafe on GTK 4.6.9 — a NULL deref inside GTK's own `dnd_finished_cb`, not (only) a freed source notebook
**Symptom**: dragging the ONLY tab of window A onto window B (native group-name drag) intermittently `SIGSEGV`s — a live GTK bug: a local drag makes X11's GDK finish synchronously, whose `gtk_notebook_dnd_finished_cb` derefs an already-NULL `detached_tab` in the unguarded `rootwindow_drop` branch.
**Scribobulate**: stop using native detach — `set_group_name(None)` + `set_tab_detachable(false)`; reimplement cross-window move with a Shift-gated custom `GtkDragSource`/`GtkDropTarget`.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-60, incl. meta-lessons); findings: researcher-findings-notebook-dnd-null-detached-tab-CORRECTION.md.

## 51. A `GtkSourceSearchContext` `occurrences-count` handler that strong-captures its own context is a permanent self-reference leak
> *Non-core (GtkSourceView) — the GtkSourceView-specific detail is kept here; the
> general GObject kernel is folded into the gtk4-rs skill as GTK4Rs/AP-63.*

**Symptom**: no crash/warning — steady unbounded growth, one `GtkSourceSearchContext` (plus its `SearchSettings` and the buffer's tag table) leaked per closed tab.
**Root cause**: a signal connected ON the context, whose closure captures a strong `.clone()` of that same context — a GObject self-reference cycle (refcount-only, no cycle collector). The buffer↔context relationship itself is weak both ways in GtkSourceView source.
**Resolution**: read the emitter from the signal's own first parameter (`move |sc, _| …`), capturing nothing.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63, the general self-capture kernel); findings: researcher-findings-searchcontext-self-capture-signal-cycle.md.

## 52. Swapping in a brand-new `GtkScrolledWindow`/`GtkAdjustment` on external auto-reload without re-wiring the scroll-spy signal bound to the old one
**Symptom**: after an external-change auto-reload (Preview, silent path), the outline scroll-spy freezes — scrolling never re-highlights.
**Scribobulate**: the external-reload path's Preview branch rebuilds a fresh `ScrolledWindow` (new adjustment) via the preview render step, orphaning the old listener; fixed by re-wiring the scroll-spy after refreshing the outline.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-55); controllers-and-bindings (GTK4Rs/AP-52).

## 53. Holding a `RefCell` `Ref` alive across a GTK setter that synchronously re-enters and borrows the same cell aborts the process
**Symptom**: switching tabs with an active find query hard-**aborts** (core dump) — `RefCell already borrowed` then `panic in a function that cannot unwind`.
**Scribobulate**: the active-tab-changed handler set the find entry's text directly from a borrowed `RefCell` value — the temporary `Ref` lived across `set_text`, which synchronously emits `search_changed` → `borrow_mut()` on the same cell. Fix: clone out first, then `set_text`.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-61).

## 54. `write_atomic`'s crash-safe write-temp-then-rename makes GIO's `GFileMonitor` report every save as a deletion
**Symptom**: after every `Ctrl+S`, the status bar showed "File deleted on disk" though the file exists with the new content.
**Scribobulate**: the atomic-write helper's `rename()` changes the inode; a `FileMonitorFlags::NONE` monitor reports a plain `Deleted`. Fixed with a dedicated self-delete guard (armed before the rename, consumed once on `Deleted`, cleared on `Changed`/`Created`), reset at monitor (re)creation.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-62).

## 55. Caching "which slot is active" as a raw `Vec` index across a drag-reorder that moves entries within the same `Vec`
**Symptom**: right-clicking a tab that had just been drag-reordered (but was never itself the *active* one) and choosing "Move to New Window" silently moved a **different** tab instead. The tab strip's own CSS `.active` highlight kept showing the correct tab, masking the divergence.
**Scribobulate**: the custom tab-bar widget tracks the active slot as a `Cell<Option<usize>>` separately from the widget-identity-bound `.active` CSS class; the reorder handler snapshots the previously-active entry's OWN identity (a widget pointer) before mutating the order and re-derives the active slot from that identity immediately afterward, never trusting the raw index across a move.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-74).

## 56. Toggling a sibling widget's visibility from inside a container's own `size_allocate`
**Symptom**: opening a window with an overflowing tab strip logged `Gtk-WARNING: Trying to snapshot GtkButton/GtkOverlay … without a current allocation`, reproducible only on a real X11/KDE desktop with a genuinely overflowing tab count and a live compositor — five debugging rounds each traded one crash mode for another.
**Scribobulate**: chevrons are hidden by GEOMETRY (`size_allocate`d out of the clip region), never `:visible` or a degenerate size; the tab-view container's construction sets `stack.set_hhomogeneous(false)`/`vhomogeneous(false)` (bubbling `queue_resize`, clean pre-layout schedule) with `transition-type=NONE`, and the content `GtkScrolledWindow` stays vertically decoupled so the resulting reflow is absorbed as a scroll-range change, not a visible jump.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-104).

## 57. A signal handler that captures its host `ApplicationWindow` by weak ref silently stops working after the widget subtree is re-homed to another window (split scroll-sync)
**Symptom**: entering split view then Move-Tab-to-New-Window leaves the destination window's two panes no longer scroll-synced — the handlers keep firing, just against the wrong (source) window. The same bug independently hit the outline scroll-spy and the caret-format overlay's per-editor driver signals.
**Scribobulate**: the split scroll-sync, outline scroll-spy, and caret-format overlay handlers resolve the host window from the pane widget's live tree root AT EMISSION TIME through the shared `window::host_window()` seam (`tabs/lifecycle.rs`: `widget.root()?.dynamic_cast::<ApplicationWindow>()`), never a captured weak ref that goes stale under cross-window reparenting. `host_window()` is the named choke point the three former hand-inlined copies now call. It is NOT universal, though: three non-scroll sites still hand-roll the identical walk (`window/contextmenu.rs`, `window/tabs/dnd.rs`, `winstate/registry.rs::tab_for_descendant`) — legitimate, since they resolve at click/emission time and aren't subject to the reparent-staleness hazard the seam guards against.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-52, dynamic `root()` resolution scope); related project entries #46, #52, #55.

## 58. Reparenting a reused `GtkSourceView` across view-mode containers re-fires its gutter's never-unbound `vadjustment` binding → a use-after-free
> *Non-core (GtkSourceView) — kept full: root cause + citations inline.*

**Symptom**: switching view mode (Preview ↔ Edit ↔ Split) emits six `g_object_unref: assertion 'G_IS_OBJECT (object)' failed` per switch, only on the 2nd+ switch — a genuine read-after-free.
**Root cause (GtkSourceView 5.4.1 C source, confirmed via gdb/valgrind/researcher)**: the reused `GtkSourceView`'s gutter binds `view."vadjustment"` with `G_BINDING_SYNC_CREATE`, but `connect_view` never stores the returned `GBinding` (an upstream defect, unchanged in `main`), so it's never explicitly unbound. Rebuilding the mode container REPARENTS the reused view; every reparent re-runs `notify::vadjustment`, re-firing the binding against a tree mid-teardown. Valgrind proved the read-after-free directly.
**Resolution**: a custom `GtkWidget` container subclass mounts the editor's `GtkScrolledWindow` **once** and NEVER reassigns its child slot again; mode/orientation/order are pure layout parameters (`set_child_visible`, allocation order), never a `set_child` call.
**Generalized lesson**: a widget you deliberately REUSE across containers (to preserve internal state) must never be *reparented* — make its layout position a parameter of a custom container instead.
**See**: gtk4-rs skill → state-and-subclassing (custom container that holds reused children as layout parameters). Findings: researcher-findings-gtksourceview-reparent-gutter-vadjustment-binding-unref.md.

## 59. Mounting a scrolling pane in a `GtkBox` without `vexpand` collapses it to its natural height (a lazily-validating `GtkTextView` then paints only ~2 lines)
**Symptom**: a `GtkSourceView`/`GtkTextView` inside a `GtkScrolledWindow` child of a vertical `GtkBox` renders only the top ~2 lines — a *sizing* bug that looks like a validation/paint bug.
**Scribobulate**: the split-view container's construction sets `hexpand`/`vexpand` on the persistent widget itself (expand flags don't transfer when consolidating several individually-expanding widgets into one container).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-105).

## 60. A closure owned by a widget's own machinery that strong-captures self (or an ancestor) is an uncollectable cycle — on window close it strands the entire descendant subtree
**Symptom**: no crash, no warning — a slow resource leak. Closing a `GtkApplicationWindow` doesn't reclaim its content subtree even though the WINDOW GObject itself finalizes cleanly.
**Scribobulate**: the window-chrome build step's `content_paned.connect_map(move |_| …)` had captured a strong clone of the SAME paned; fixed by using the handler's own emitter argument (`move |paned| …`) instead. Two secondary self-cycles in the custom tab-bar/tab-view widgets fixed the same way (weak-captured).
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63).

## 61. Building N `GtkMenuButton` menu-models in a synchronous startup burst forces N×items accelerator-label font resolutions → a multi-second UI freeze
**Symptom**: opening many documents/tabs at once froze input for several seconds a few frames after first paint, scaling with tab count not document size; `perf` fingerprinted `FcFontSetSort`/fontconfig inside `gtk_accelerator_get_label`.
**Scribobulate**: exactly ONE shared caret-format overlay per window, re-parented onto the active tab's editor per switch — one heading-menu materialization ever, independent of tab count.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-106); app-lifecycle-and-env. Findings: researcher-findings-popover-set-parent-superlinear-startup-freeze.md.

## 62. A custom tab/stack widget leaves its active-index model unset for the default-visible first page
**Symptom**: moving/closing the FIRST tab of a window (never explicitly switched to) left the source window with a blank content pane and the moved tab's stale outline — a "phantom tab".
**Root cause**: the custom tab-bar/tab-view widget tracks the active tab in its own `Cell<Option<usize>>`, set only by its switch-to-index method — but a window's INITIAL page is shown by the `GtkStack`'s default (first child visible) and never travels through that path.
**Scribobulate**: a dedicated first-page-active marker, called once right after appending the first page, sets the active slot to `Some(0)` WITHOUT firing a switch callback.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-75); related #55 (GTK4Rs/AP-74), #56.

## 63. A shared app-level menubar model can't carry per-window content — a per-window submenu needs a self-built `GtkPopoverMenuBar` + selection-as-action-state + deferred `GMenu` mutation
**Symptom**: a `View ▸ Documents` per-window tab-list submenu can't be built by mutating the app's shared `Documents` `gio::Menu` — every window's menubar renders the SAME model.
**Scribobulate**: each `ApplicationWindow` self-builds its own `GtkPopoverMenuBar::from_model` (drops `app.set_menubar()`); "which tab is active" is a stateful radio `win.select-tab` action (a switch mutates NO menu content); `Documents` rebuilds are coalesced behind a dirty flag into a single `idle_add_local`.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-76); app-lifecycle-and-env (`set_menubar` D-Bus export, F10 self-registration). Findings: researcher-findings-per-window-menubar-documents-submenu.md.

## 64. A process-global CSS provider with an unscoped selector collides across windows (last-loaded wins)
**Symptom**: with ≥2 windows open, preview zoom shifted content but did not resize text in every window except the last-built one; opening a second window instantly reset the first window's zoomed font.
**Scribobulate**: each window's rule is scoped to a per-window CSS class; a cross-window tab move ALSO re-renders the arriving tab's pixel geometry at the destination zoom (a dedicated tab-arrival wiring step) — the CSS half self-heals via tree-matching, the imperative half needed an explicit re-sync.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-77); app-lifecycle-and-env (CSS-provider display lifecycle).

## 65. Preserving a `GtkTextView` reading position across a re-render drifts, and can wedge input dead
**Symptom**: (a) drift — a preview zoom step nudged the reading position toward the top on repeated/fast zooms; (b) wedge — intermittently, mouse wheel AND PageUp/PageDown both went input-dead after a mode switch right after a zoom.
**Scribobulate**: restore anchors to a buffer LINE via a persistent mark + deferred `scroll_to_mark`, followed (for the generic editor restore) by a non-animating `set_value` clamp; the preview view caches the reading line only while a `user_scrolling` flag is set, immune to mid-animation reads during rapid zoom.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-14; kin GTK4Rs/AP-115, GTK4Rs/AP-153).

## 66. Relying on pulldown-cmark's native superscript/subscript for tight `E=mc^2^` / `H~2~O`
> *Non-core (pulldown-cmark) — full; this is third-party parser behaviour, NOT a core-GTK lesson. Do not fold into the gtk4-rs skill.*

**Symptom**: tight, Pandoc-style superscript/subscript (`E=mc^2^`, `H~2~O`) never rendered — the literal `^`/`~` showed instead; a multi-tilde line also lost its SECOND subscript once native superscript was disabled.
**Root cause**: pulldown-cmark recognises `^`/`~`/`~~` with CommonMark FLANKING-delimiter rules (like emphasis) — the inverse of Pandoc's TIGHT rule; and any enabled tilde feature FRAGMENTS a paragraph across multiple `Text` events at a stray unpaired marker, defeating a per-event scanner.
**Resolution**: the Markdown-options setup disables `ENABLE_SUPERSCRIPT`/`SUBSCRIPT`/`STRIKETHROUGH` at every parse site; a dedicated scan step tokenises `^x^`/`~x~`/`~~x~~` ourselves with tight Pandoc semantics on clean, unfragmented text.
**See**: pulldown-cmark 0.13 `Options`; CommonMark §6.2 flanking rules; Pandoc superscript/subscript extension. Empirically bisected.

## 67. Screenshotting an open GTK4 `GtkPopoverMenu` under kwin/X11 to verify a menu
> *Non-core (UI-test tooling on the operator's kwin/X11 session), full; this is a screenshot-capture gotcha, NOT a GTK code lesson. Do not fold into the gtk4-rs skill's code guidance — keep it with the UI-testing notes.*

**Symptom**: an opened `View ▸ Toolbar` submenu (click/focus confirmed correct) never appeared in any `xwd -id <mainwin>` or `xwd -root` screenshot.
**Root cause**: a GTK4 `GtkPopoverMenu` presents as a SEPARATE override-redirect top-level surface (not drawn inside its toplevel) — `xwd -id <mainwin>` structurally cannot hold it, and kwin doesn't fold the popup surface into the root pixmap either.
**Resolution**: verify the menu's EFFECT on a capturable surface (the main window, or the action/persisted state) instead of trying to screenshot the popover; also banked that `xdotool windowmove` sets the frame origin, so client-area clicks need the decoration-height offset added.
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-67, the D-Bus GAction-state screenshot alternative). Observed live on GTK 4.6, kwin_x11.

## 68. Deferring a `set_visible(false)`→`measure()` read as if it were the GtkTextView lazy-validation family
**Symptom**: after `child.set_visible(false)`, a synchronous `parent.measure(...)` was assumed to return stale numbers (the GtkTextView #13/#15/#22 reflex) and deferred to an idle for zero benefit.
**Root cause (GTK 4.6.9)**: `gtk_widget_hide` clears the size-request cache SYNCHRONOUSLY before `set_visible` returns — cache invalidation, unrelated to GtkTextView's lazy line-height *validation* (a different subsystem entirely).
**Scribobulate**: the toolbar min-width update takes no deferral at all — a plain `queue_resize` is enough.
**See**: gtk4-rs skill → textview-layout-and-drawing (**GTK4Rs/T-2**, the hide→measure synchronous-cache-invalidation contrast; `T-n` is the skill's technique register, numbered and frozen like its `AP-n`).

## 69. Putting mnemonic `_` markers in a command label shared across menu + tooltip + context-menu surfaces
**Symptom**: injecting `_` into the ONE shared `Cmd.label` (feeding menu, tooltip, and a hand-rolled context menu) leaked a literal underscore into tooltips and mis-set/hid an access key on the manual button.
**Scribobulate**: mnemonics injected ONLY at menu-build time (a dedicated mnemonics table + helper function), reused by the context menus; the shared command label stays literal so toolbar tooltips are unaffected. Dedicated well-formedness/uniqueness guard tests catch drift.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-68). Findings: researcher-findings-popovermenubar-mnemonics.md.

## 70. Getting bare-letter access keys (with a visible underline) in a plain `GtkPopover` via mnemonics / use-underline
**Symptom**: a plain `GtkPopover` context menu (deliberately not `GtkPopoverMenu`) needs a BARE-letter access key with a visible underline; `Button::with_mnemonic`/`use-underline` requires Alt and only shows the underline while Alt is held.
**Root cause**: managed mnemonics default `mnemonics-modifiers = Alt`; `GtkPopoverMenu` gets bare letters only via private calls unavailable to a plain popover, and a label's `_` underline draws only while `mnemonics-visible` is set.
**Scribobulate**: a dedicated access-markup/access-shortcut helper builds a `GtkShortcutController` (Capture/Local phase) with one `KeyvalTrigger(keyval, NO modifiers)` per row, gated on `is_sensitive()`; the underline is drawn manually with Pango `<u>` markup.
**See**: gtk4-rs skill → controllers-and-bindings (**GTK4Rs/T-1**, bare-letter access keys via ShortcutController + KeyvalTrigger). Findings: researcher-findings-plain-popover-access-keys.md.

## 71. Nesting a submenu as a child `GtkPopover` inside a plain autohide `GtkPopover` context menu
**Symptom**: a submenu (Change Case ▸) as a child `GtkPopover`/`GtkMenuButton` popover parented to a row runs into the parent autohide popover's grab.
**Scribobulate**: the context-menu implementation uses a single-surface `GtkStack` (`main`/submenu pages, `SlideLeftRight`) mirroring what `GtkPopoverMenu` itself does for submenus, omitting its spurious-scrollbar-causing `ScrolledWindow` wrap; access keys are page-gated (#70's controller, same physical key means different things per page).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-69). Findings: researcher-findings-plain-popover-nested-submenu.md.

## 72. Gating/targeting a multi-pane action on the first-found view instead of the focused pane
**Symptom**: in split (two-`GtkTextView`) layout, Copy/Select All always acted on the editor — Copy stayed disabled while the PREVIEW held a selection.
**Scribobulate**: a dedicated focused-text-view resolver tracks a sticky focused pane (updated by `focus-widget-notify`, ignoring transient popovers/find-bar exactly like ScrAP-20), falling back to the single view otherwise.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-70; kin GTK4Rs/AP-20).

## 73. Reconstructing character-precise copied Markdown from sparse parser waypoints, and mis-reading pulldown-cmark offset semantics
> *Non-core (pulldown-cmark) — condensed; full essay in git history.*

**Layer**: pulldown-cmark 0.13 offset iterator (`Parser::into_offset_iter`).
**Symptom**: copying a *partial* preview selection returned the WHOLE enclosing block's Markdown source (four letters of a heading → the entire `# Heading` line).
**Root cause**: the sparse waypoint map records source offsets only at pulldown event boundaries and snaps outward — block-granular by construction. A block's Start/End range includes the TRAILING newline, an escaped char's `Text` token DROPS the backslash, and an entity tokenises apart from its rendered char.
**Resolution**: the copy-map builder constructs a buffer-annotated construct TREE in the same render pass that fills the buffer, reconstructing delimiters from source only when a selection crosses a construct's content boundary; leaf runs interpolate char-precisely.
**Lesson**: to reconstruct *balanced* source from a rendered selection you need a construct tree annotated with real render offsets, not a flat source-offset map.

## 74. Aligning char offsets with `GtkTextBuffer::get_text()` — it omits anchored children
**Symptom**: a debug assertion (and any offset-indexed logic) drifted between the buffer's character offsets and a `buf.text()` string — by one char per anchored child.
**Root cause**: `gtk_text_buffer_get_text()`/`get_iter_text()` silently OMIT anchored children, but `char_count()`, `iter.offset()`, `slice()`/`get_slice()`, and `selection_bounds()` all count each as one `U+FFFC`. A `get_text`-derived char array and any iter/`char_count`-derived offset diverge silently, only on documents that HAVE anchors.
**Scribobulate**: the copymap drift guard and the copy path both use `buf.slice()`, never `text()`, when correlating with iter/char_count offsets.
**Now enforced by**: `saferizer::BufferText` — its only constructors call `slice()`; raw `TextBufferExt::text` is banned crate-wide (`clippy.toml`), so a new extraction site cannot silently drift.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-5).

## 75. A hard tab in a GFM table breaks table recognition; normalise tabs — but length-preservingly
> *Non-core (pulldown-cmark / CommonMark GFM) — full; this is third-party parser + spec behaviour, NOT a core-GTK lesson. Do not fold into the gtk4-rs skill.*

**Symptom**: a table pasted from a spreadsheet, cells separated by hard TABS, rendered as a literal paragraph (`---` even turned into an em-dash via smart punctuation) — the byte-identical table with spaces parsed fine.
**Root cause**: a GFM delimiter row's grammar admits only `-`, `:`, `|`, spaces — a tab is a CommonMark/GFM-conformant rejection, not a pulldown bug.
**Resolution**: a dedicated tab-normalization step replaces a hard tab with ONE space — LENGTH- and POSITION-preserving (so copymap/scroll-sync byte offsets never drift) — exempting leading indentation and verbatim code regions (found via a structural pre-parse).
**See**: pulldown-cmark 0.13 `ENABLE_TABLES`; GFM spec §Tables; CommonMark §2.2/§6.2.

## 76. A paragraph-attribute `GtkTextTag` applied as one continuous range over a multi-paragraph region drops the attribute on toggle-free middle lines
**Symptom**: a `blockquote` tag (`left-margin`/`right-margin`) spanning several GtkTextView paragraphs renders the FIRST and LAST paragraphs correctly but drops the margin on toggle-free MIDDLE paragraphs — width-dependent, non-self-healing.
**Scribobulate**: a dedicated per-line tag-application step tags each logical line's CONTENT ONLY, leaving every terminating `\n` untagged — the untagged gaps prevent coalescing, so every line gets its own toggle.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-72). Findings: researcher-findings-textview-blockquote-left-margin-multipara.md.

## 77. UI-testing a formatter over the selectable read-only Preview pane
**Symptom**: a formatter click over a visibly-selected Preview pane silently no-opped for every command — a selection in a selectable READ-ONLY view looks identical to an editor selection, but the format action is correctly disabled there.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: `src/window/editbar/focusgate.rs` — the window focus-widget gate (`connect_focus_widget_notify` + `is_ancestor`) that keys `win.format` on editor focus, not on selection presence; a read-only-preview selection therefore leaves it disabled and the format click no-ops. Sibling of #20 (the sticky focus-gate).

## 78. `Options::all()` (or any enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of degrading to literal text
> *Non-core (pulldown-cmark), full. Same family as #66/#75 — a parser-configuration trap, not a GTK lesson. Keep in-repo; do not fold into the gtk4-rs skill.*

**Symptom**: math (`$E=mc^2$`) rendered as nothing, footnote refs (`[^1]`) vanished, and YAML/`+++` frontmatter leaked into the body as a stray paragraph — silent content loss, no warning.
**Root cause**: `Options::all()` turns on EVERY pulldown-cmark extension, including ones the renderer has no handler for; the dispatcher's catch-all silently drops standalone events and leaks a container's inner `Text`.
**Resolution**: the Markdown-options setup is an explicit ALLOWLIST of only the extensions actually handled (`TABLES | TASKLISTS | SMART_PUNCTUATION | HEADING_ATTRIBUTES | GFM`); anything else degrades to literal `Text` rather than vanishing.

## 79. A container-level `GtkGestureClick` also fires on presses that land on a child `GtkButton` — a bar-wide "activate" gesture activates a tab even when the press was on its × close button
**Symptom**: clicking a BACKGROUND tab's × close button closed the right tab but also silently switched the active document — the closed tab's neighbour became active instead of the previously-active tab staying active.
**Scribobulate**: a dedicated close-button hit-test resolves the real target with `WidgetExt::pick()` and bails early when it (or an ancestor) carries the close-button CSS class.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-109).

## 80. Tracking a "reading line" only from a wheel `EventControllerScroll` misses scrollbar-drag and keyboard scrolling — the re-anchor goes stale
> *Core GTK4. Candidate for the gtk4-rs skill (scroll / adjustment lifecycle). Sibling of #65.*

**Symptom**: after a SCROLLBAR-drag or KEYBOARD scroll (not the mouse wheel), a subsequent zoom/reload re-anchor snapped the viewport back toward the top instead of holding the reading position.
**Root cause**: `user_scrolling` was set only by a wheel `GtkEventControllerScroll`; a scrollbar thumb-drag/trough-click and keyboard nav move the `GtkAdjustment` directly, emitting no scroll event, so `value-changed` never updated the cached reading line.
**Scribobulate**: a dedicated scroll-position-tracking wiring step also hooks a `GtkGestureClick` on the scrollbar and a scroll-key `GtkEventControllerKey`; every programmatic scroll resets the `user_scrolling` flag to false at its start, so a rapid-zoom burst's own animation frames stay excluded (burst-safe).
**See**: gtk4-rs skill → controllers-and-bindings / textview-scrolling-and-adjustments (input-source wiring companion, sibling of the GTK4Rs/AP-14 family / #65).

## 81. Persisting "all windows" from each window's `close-request` + a sequential-close quit loses every window but the last
**Symptom**: quitting with several windows open (a sequential `for w in app.windows() { w.close() }`, needed so `close-request` still fires the unsaved-changes prompt) persisted only the LAST-closed window's session.
**Scribobulate**: a dedicated quit-all-windows routine snapshots the full window set ONCE up front, freezes session-save via a `thread_local` latch for the close sequence, and thaws it on a cancelled prompt / backed-out Save-As.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-113).

## 82. A one-shot `scroll_to_mark` restoring a FAR reading position onto a freshly-rebuilt GtkTextView lands near the top
**Symptom**: an external file reload rebuilds the preview from scratch (fresh `GtkTextView`/adjustment); restoring a FAR reading position on a very large document snapped near the TOP — the same mark + `scroll_to_mark` restore worked fine on a WARM view.
**Scribobulate**: a dedicated fresh-view scroll-restore routine drives a PROGRESSIVE non-animating `set_value(line_yrange(mark).y)` off `notify::upper` until `line_at_y` converges; the one-shot `scroll_to_mark` is reserved for warm views (outline-nav, zoom, small docs).
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-115). Findings: researcher-findings-textview-far-scroll-fresh-unvalidated.md.

## 83. `GtkShortcutsWindow`'s programmatic `add_section`/`add_group`/`add_shortcut` API is GTK 4.14+ — on 4.6 you must build it from Builder XML
> *Core GTK4. Candidate for the gtk4-rs skill (menus / actions / app-lifecycle — help overlay). Version-availability trap; verified against the 4.6.9 runtime with `nm`.*

**Symptom**: the natural programmatic `add_section`/`add_group`/`add_shortcut` construction compiles cleanly against gtk4-rs 0.10 but references C symbols absent before GTK 4.14 — an undefined-symbol link/load failure on 4.6, not a graceful degradation.
**Root cause**: the gtk4-rs `doc(cfg(v4_14))` marker affects only docs.rs rendering, NOT a real compile gate; `nm -D libgtk-4.so.1` confirms the symbols are absent on 4.6.9.
**Resolution**: a dedicated shortcuts-window builder generates a `GtkBuilder` interface XML from the command tables (the stable `Buildable` path since GTK 4.0) and fetches the object; `set_help_overlay` (`GtkApplicationWindowExt`, present on 4.6) wires it.
**See**: gtk4-rs skill → versioning-and-features (GTK4Rs/AP-114).

## 84. `GtkTreeListModel` `autoexpand=true` makes true recursive Collapse all impossible; build `autoexpand=false` + explicit expand pass. Collapse DESTROYS the subtree (it does not cache expanded flags)
**Symptom**: with `autoexpand=true`, Collapse-all only reaches "collapse to roots" — re-expanding a single root always springs its ENTIRE subtree open, never just its direct children.
**Scribobulate**: model built `autoexpand=false` + an explicit forward-walk expand-all pass at build time for the default-open TOC; Collapse-all need only collapse the depth-0 roots (destroying each wipes everything below).
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-111).

## 85. A bundled (gresource) `*-symbolic` icon is only a fallback — a host theme that ships the same name overrides it
**Symptom**: redrawing a bundled `*-symbolic` SVG changed nothing on a real desktop whose theme (e.g. breeze-dark) ships the same icon name; a headless Adwaita screenshot deceptively showed the new bundled art.
**Scribobulate**: the outline Expand/Collapse buttons keep standard icon names (`expand-all-symbolic`) so KDE/breeze supplies its native chevrons; the bundled SVGs are the Adwaita/headless fallback only.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-102).

## 86. Probing a broader Markdown marker before a narrower one that embeds it mis-parses the input — test narrowest-first
> *Non-core (Scribobulate's Markdown formatting core — CommonMark/GFM marker parsers). Not a GTK or third-party-library defect; a self-inflicted precedence hazard.*

**Symptom**: a GFM task item (`- [ ] foo`) auto-continued on Enter as if a plain bullet, leaving the checkbox dangling — everything looked right for plain bullets/numbered lists, so the bug only showed on the newest marker type.
**Root cause**: a task marker (`- [ ] `) is a bullet marker plus more; in an `if let … else if let …` chain, the FIRST-tested broader parser (bullet) matched the shared prefix and short-circuited before the narrower task parser ever ran.
**Resolution**: order the parser chain NARROWEST-first — the task-marker parser before the bullet-marker parser before the ordered-marker parser; give the narrow parser its own detector for the discriminating part (the checkbox).
**Non-core (Scribobulate's Markdown formatting core) — do NOT fold into the gtk4-rs skill.**

## 87. A `#[gtk::test]` that maps + pumps to full allocation BEFORE calling the code under test validates line heights first, masking an unvalidated-heights bug
> *Core GTK4. In the gtk4-rs skill as GTK4Rs/AP-78 (threading-async-and-memory), source-confirmed against gtktextview.c 4.6.9.*

**Symptom**: a `#[gtk::test]` for a fresh-view far-restore fix (ScrAP-82) passed under BOTH the fixed AND the pre-fix (buggy) code — a mutation test exposed it as non-discriminating.
**Root cause**: mapping + pumping the loop to full allocation is exactly what DRIVES GtkTextView's lazy line-height validation — by the time the test calls the restore, the transient (unvalidated-heights) precondition the bug depends on is already gone.
**Resolution**: scope each automated test to what it CAN decide deterministically (mark placement, no-panic, no re-arming ScrAP-22); mark the true regression guard as a load-bearing MANUAL integration test; always mutation-test a regression guard (flip the fix, confirm the test fails) before trusting it.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-78).

## 88. Bounding a blocking `MainContext::iteration(true)` pump loop with a between-iterations wall-clock check instead of a timeout SOURCE — it can hang forever on an idle display
> *Core GTK4. In the gtk4-rs skill as GTK4Rs/AP-79 (threading-async-and-memory), source-confirmed against glib 0.20.12.*

**Symptom**: a `#[gtk::test]` pump loop "bounded" by an `Instant`-based wall-clock check BETWEEN iterations looked fine on a busy display but could hang indefinitely on a genuinely idle one — the watchdog assert never fires.
**Root cause**: `iteration(true)` BLOCKS until the context has dispatchable work; a deadline check placed between iterations only runs when `iteration` RETURNS — dead code exactly when the loop is stuck.
**Resolution**: install a real `glib::timeout_add_local_once` SOURCE before the loop as the watchdog — a ready timeout is dispatchable work, so `iteration(true)` is GUARANTEED to return by the deadline; remove the source on a converged (normal) exit.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-79).

## 89. Gating a programmatic `GtkSingleSelection` change with a transient "we're setting it" bool — it re-emits `selected-item` after the setter returns, escaping the bool
**Symptom**: an outline scroll-spy's programmatic selection change, guarded by a transient bool bracketing the setter call, still spuriously navigated the preview whenever the user expanded/collapsed a tree node.
**Scribobulate**: TWO complementary per-tab guards, because the bool alone can't cover the async echoes — `outline_spy_selecting` (a transient `Cell<bool>`, `winstate/tab.rs`) catches the synchronous `notify::selected-item` inside `set_selected`, and `outline_spy_doc: Cell<Option<usize>>` (the doc **index** the spy currently owns, matched by equality — not GObject identity) catches the emissions `GtkSingleSelection` fires AFTER the bool resets: deferred, and again per `items-changed` during expand/collapse. The activation handler (`window/outline_nav.rs`) suppresses navigation when either fires; a genuine user click on a different heading matches neither, so navigation still works.
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-112).

## 90. A `GtkPopover` attached with `set_parent()` is NOT auto-unparented — the parent widget's `dispose()` must unparent it, or teardown floods "GtkPopover is not a child of …"
**Symptom**: a `gtk-integration-tests` teardown (any preview-view dispose) flooded 15+ identical `Gtk-WARNING: GtkPopover is not a child of …`.
**Scribobulate**: THREE mechanisms, now with a unified handle for the hazardous case. (1) The view-parented persistent popovers (codeview marker popover + selection overlay, window format overlay) are owned by `saferizer::PersistentPopover` (Wave 7): its `teardown()` runs `popdown()`→`unparent()` in the one safe order (#123), guarded against double-unparent, called from the view's `ObjectImpl::dispose` — so "left parented at dispose" (this entry's flood) is unrepresentable through the handle. (2) The two transient context menus (`window/contextmenu.rs`, `window/tabs/contextmenu.rs`) `connect_closed(|p| p.unparent())` per invocation. (3) Custom containers (`SplitView`, tab widgets) unparent non-popover children via `widgets::unparent_all_children` in their own dispose.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-80).

## 91. An always-on scrollbar with default (overlay) scrolling floats over the `GtkTextView`'s right margin, stealing clicks meant for margin-drawn affordances
**Symptom**: right-margin CriticMarkup marker chips, painted and hit-tested correctly, never opened their popover on click.
**Scribobulate**: the preview-rendering setup builds the preview scroller with `.overlay_scrolling(false)`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-81).

## 92. A mutation path that edits the buffer but leans on a MODE-GATED live-preview refresh leaves the preview stale
**Symptom**: creating/editing/removing an annotation in preview-only mode wrote correct source but left the preview's highlights/markers/popover text stale until a manual reload.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the mode-agnostic annotation re-render site.

## 93. Anchoring positions by pulldown-cmark source offset against ALL events maps onto a block-structure event whose range spans the whole block
> *Non-core (pulldown-cmark). Its `into_offset_iter()` gives a source *range* per event; a `Start(Paragraph)` can emit only the inter-block separator while its range spans the entire block.*

**Symptom**: a CriticMarkup comment marker/highlight placed by mapping a cleaned-source offset to a buffer position landed on the BLANK-LINE separator above its paragraph instead of the paragraph itself.
**Root cause**: pulldown's offset iterator reports the source range of the ENTIRE BLOCK for a block Start/End event, even though this renderer emits only the block separator there — an all-events offset lookup resolves a paragraph interior onto that misleading range.
**Resolution**: the preview-build offset-anchoring map is restricted to CONTENT events only (`Text`/`Code`/`Break`), excluding `Start`/`End` block-structure events.
**Non-core (pulldown-cmark) — do NOT fold into the gtk4-rs skill.**

## 94. A signal handler connected to a `GtkTextView`'s BUFFER is silently dropped when `set_buffer` swaps the buffer — re-wire buffer-dependent handlers on the new buffer
**Symptom**: the preview's "select → show Annotate overlay" worked for the FIRST annotation only — after one annotation, further selections drove nothing.
**Scribobulate**: the annotation-overlay wiring re-invokes its selection-connect closure from `view.connect_notify_local("buffer", …)` — a VIEW-level hook that survives every swap.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-82, GtkTextView buffer-signal lifecycle).

## 95. A shown `GtkPopover` does not grow its surface when its child grows — pre-size it (homogeneous `GtkStack`), don't re-present it
**Symptom**: swapping a popover's narrow button child for a wider comment `GtkEntry` clipped the entry — the popover kept the smaller child's surface width.
**Scribobulate**: the live instance is the two-page context-menu popover (`window/contextmenu.rs`) — a single `GtkStack` built once with both pages present, relying on `GtkStack`'s *implicit* `hhomogeneous=true` default (it only overrides `vhomogeneous`/`interpolate-size`), so the popover pops up at the widest page's width from the first show and page swaps never resize the surface. (Prior revisions of this entry claimed an annotation-overlay `GtkStack` with an explicit `hhomogeneous=true` — a fabrication: `git log -S 'gtk::Stack' -- src/preview/` = 0 commits, and `hhomogeneous` is set only in the tab widget, to `false`. The annotation comment popover avoids the sizing hazard differently — it never re-presents; see #98.)
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-86).

## 96. Committing an action that rebuilds the widget subtree synchronously inside a `GtkButton` `clicked` handler breaks active-state accounting
**Symptom**: committing an annotation (create/edit/remove) from a popover's Save button worked but logged ~11 "Broken accounting of active state for widget" lines up the document's ancestor chain.
**Scribobulate**: the annotation commit paths defer the rebuild with `glib::idle_add_local_once` so the gesture unwinds first.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-30 — rebuilding a widget inside its own event emission; defer with `idle_add_local_once`).

## 97. Inferring "inline vs block" from non-empty source delimiter bytes engulfs whole paragraphs
> *Non-core (pulldown-cmark / this project's copymap). A construct node's source "open"/"close" byte spans are non-empty for plenty of non-inline constructs.*

**Symptom**: annotating a single plain word in a paragraph highlighted the ENTIRE paragraph.
**Root cause**: `wrap_span` inferred "inline" from whether a node's source open/close delimiter byte-ranges were non-empty — a paragraph ALSO has non-empty trailing "close" bytes, so it was mis-flagged inline and taken whole.
**Resolution**: the copy-map's branch-node representation carries an explicit kind set from the CONSTRUCT KIND at build time, never inferred from byte shape (originally an `inline: bool`; now the `BranchKind` enum — see ScrAP-255 for why the boolean pair became one enum).
**Non-core (pulldown-cmark / this project's copymap) — do NOT fold into the gtk4-rs skill.**

## 98. A `GtkPopover` hosting a typing entry is unwinnable on X11 (autohide steals focus via its seat grab; non-autohide can drop clicks) — host a typing entry as an in-surface `GtkOverlay` child instead
**Symptom**: a preview "Annotate" overlay hosting a comment entry was unwinnable on X11 — `autohide(false)` dropped clicks/keys to whatever was beneath (WM-arbitrated, no grab, real WM only, invisible on WM-less Xvfb); `autohide(true)` took a real X11 seat grab whose popover-LOCAL coordinates resolve against the toplevel tree with the popover→window offset DROPPED, stealing focus onto the chrome underneath.
**Scribobulate**: the preview-rendering setup wraps the preview `ScrolledWindow` in a `GtkOverlay`; the comment entry lives there (a dedicated in-surface overlay component); the Annotate action button stays a non-autohide popover.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-83).

## 99. A translucent text-tag highlight is painted over by a later opaque-background tag — GTK text-tag backgrounds don't composite; the highest-priority tag wins
**Symptom**: annotating a claim overlapping inline `code` wrote correct CriticMarkup and correct tag data, but the amber highlight was INVISIBLE — the code's opaque grey background painted over it.
**Scribobulate**: the tag-table setup raises the annotation-highlight tag to `table.size()-1` (top priority) after all other tags are added.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-84).

## 100. Measuring a widget while it is `visible=false` returns 0 — center an overlay child off a hidden measure and it collapses to a left-edge anchor
**Symptom**: a comment-entry overlay card meant to CENTER on the selection midpoint sat to the RIGHT of the anchor — centering degenerated to a left-edge anchor.
**Scribobulate**: the annotation-overlay wiring shows the entry card BEFORE positioning it (measurement needs VISIBILITY, not allocation).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-85).

## 101. UI-test tooling: kwin-on-Xvfb won't deliver a synthetic `xdotool` click to a non-autohide `GtkPopover` surface — verify such flows via a keyboard-triggerable action, not a synthetic popover click
**Symptom**: a synthetic `xdotool` click squarely on a non-autohide `GtkPopover`'s button, under kwin-on-Xvfb, never activated it — an established (working-on-real-KDE) popover was equally unresponsive under the same harness, proving it wasn't a code bug.
**Scribobulate**: the `win.annotate` GAction is the keyboard-triggerable path used to test entry-card positioning headlessly (also how ScrAP-102 was found and verified).
**See**: gtk4-rs skill → ui-testing-interaction (GTK4Rs/AP-175); gtk4-rs skill → automated-UI-testing (ui-testing-interaction module).

## 102. Positioning a widget via `set_margin_*` then re-measuring it double-counts the margin — GTK folds a widget's own margins into `preferred_size()`/`measure()`
**Symptom**: a comment-entry card positioned by `set_margin_start`/`set_margin_top` landed correctly the FIRST show but drifted on every LATER show, jumping to the TOP near the viewport bottom.
**Scribobulate**: the card-positioning routine zeroes `margin_start`/`margin_top` BEFORE measuring, then applies the freshly-computed margins.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-87).

## 103. Refreshing a `GtkTextView` via `set_buffer` for a change that leaves the rendered text identical repaints the whole document and jumps the scroll
**Symptom**: adding/removing a CriticMarkup annotation made the whole preview pane visibly JUMP — a full repaint plus a top-flash-then-restore — even though only decorations (tags/markers) changed, not the rendered text.
**Scribobulate**: a dedicated in-place annotation-refresh path re-tags + re-markers the LIVE buffer in place (no `set_buffer`) whenever the freshly-parsed text is structurally identical to what's on screen, falling back to a full re-render only if it isn't.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-90).

## 104. A persisted `GtkTextMark` re-resolved after a `set_buffer` swap is a cross-buffer footgun that aborts with `gtk_text_btree_line_number couldn't find line`
**Symptom**: a fatal, real-session-only crash (`gtk_text_btree_line_number couldn't find line` / SIGSEGV) on reload of a document carrying CriticMarkup annotations — never reproduced across an extensive headless Xvfb battery.
**Scribobulate**: every persisted-mark resolution site guards `mark.buffer().as_ref() == Some(&view.buffer())` before resolving; mutation-tested (removing the guard reproduces the exact crash).
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-89).

## 105. `iter_location` (any line-DISPLAY-caching geometry read) right after a `set_buffer` swap, before re-allocation, aborts with `gtk_text_btree_line_number couldn't find line`
**Symptom**: the same fatal `couldn't find line` crash as #104, reached through GTK's own line-display cache — at three successive call sites in turn (our tick, our paint, then GTK's OWN `parent_snapshot`), real-session-only.
**Scribobulate**: root fix — the reload-from-disk path sets a loading guard flag around the editor-load step; defense in depth — the scroll-sync and preview draw/snapshot paths read the cache-free `line_yrange` instead of `iter_location` on a possibly-just-swapped view.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-89; kin GTK4Rs/AP-258).

## 106. A selectable `GtkLabel` in a popover auto-selects all its text on open — the popover focuses it, and a selectable label selects-all on focus-in
**Symptom**: clicking a margin annotation marker opened its popover with the comment `GtkLabel` already fully selected, every time.
**Scribobulate**: the marker-popover builder drops `set_selectable(true)` on the comment label.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-107).

## 107. A menu-activated action that synchronously raises a focus-grabbing in-surface widget has its focus stolen by the menu popover's pop-down focus-restore — defer the raise to idle
**Symptom**: Edit ▸ Annotate did nothing from the MENU (flashed the comment card then it vanished) but worked perfectly from its keyboard accelerator.
**Scribobulate**: the Annotate action's registration defers the raise via `glib::idle_add_local_once` so the pop-down + focus-restore settle first.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-116).

## 108. `GtkTextBuffer::redo()`/`undo()` leaves no undo barrier — the next edit merges into the redone action's group, so one later Undo reverts two edits
**Symptom**: annotate → Undo → Redo → annotate something ELSE → one later Undo reverted BOTH annotations, not just the last.
**Scribobulate**: **originally this entry claimed the flush happened "before each discrete edit" — it did not; it was at ONE of the three routines** (the annotation splice), while every format command (`editbar/edit.rs`) and smart-newline continuation (`editbar/newline.rs`) lacked it, so the double-revert was live on every format command and smart-newline for long after this entry was written. The contract is now a single RAII seam, `window::undo::UndoGroup` — its constructor flushes the barrier and opens the action, its `Drop` closes it — and raw `begin_user_action`/`end_user_action` are banned via `clippy.toml`'s `disallowed-methods`, so a new discrete-edit routine cannot re-introduce the merge (the same construction-contract shape as ScrAP-74's `slice()`-only buffer newtype).
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-110, the GtkTextBuffer/GtkTextHistory undo-barrier lesson).

## 109. Mapping GtkTextView buffer coords ↔ an anchored-child cell's interior under incremental allocation
**Symptom**: mapping a buffer position to the INTERIOR of an anchored-child table cell via `translate_coordinates(cell → view)` intermittently landed a marker/scroll a whole row too high, self-healing only on a full rebuild.
**Scribobulate**: a dedicated cell-row-geometry routine computes table-top buffer-Y from `line_yrange(iter_at_child_anchor(table_anchor))` (cache-free) PLUS `translate_coordinates(cell → table widget)` (a local, placeholder-immune subtree transform) — recomputed every frame in the draw/snapshot layer, no cache.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-91).

## 110. Driving selection-dependent UI for a selectable-`GtkLabel` cell (a selection island) — buffer signals never fire; use the primary clipboard, wired on the live view
**Symptom**: selecting text INSIDE a table cell drove neither the `win.annotate` action state nor the auto-showing selection overlay, though body-text selection drove both.
**Scribobulate**: the annotation-overlay wiring connects `view.primary_clipboard().connect_changed` PER-RENDER (guaranteed-live view), disconnected in `dispose`; cell selections clear on a genuine buffer-cursor placement (otherwise sticky).
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-28).

## 111. The in-place buffer-tag refresh can't repaint an anchored-child cell decoration — reconcile the cell labels in place, unconditionally
**Symptom**: creating a cell annotation didn't show its amber highlight; removing the last cell annotation didn't clear it — both fixed only by an unrelated full re-render.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the in-place annotation-refresh site (the anchored-child cell-label reconciliation).

## 112. `GDK_IS_SURFACE` criticals are a stale TOOLTIP timer over an unrealized grabbing popover — reuse popovers, don't destroy per use
**Symptom**: interacting with popovers over a `GtkTextView` on real X11/kwin repeatedly logged `GDK_IS_SURFACE`-failed assertions — invisible on WM-less Xvfb even under `fatal-criticals`.
**Scribobulate**: the interactive/grabbing popovers OVER THE `GtkTextView` — the codeview marker popover and the selection-action/format overlay — are REUSED (created + `set_parent`'d once, only `popup()`/`popdown()`, content rebuilt per use), now behind `saferizer::PersistentPopover`; these are the surfaces where the assertion fired. The two transient CONTEXT menus (`window/contextmenu.rs`, `window/tabs/contextmenu.rs`) are the deliberate exception — `Popover::new()` + `set_parent` + `connect_closed(|p| p.unparent())` per right-click, NOT reused. Read the fix as scoped to the view-parented interactive popovers: the context menus rely on being short-lived, not on reuse.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-117). Findings: researcher-findings-popover-tooltip-surface-assertion.md.

## 113. The first popup of a view-parented popover forces a one-shot table revalidation that scrolls the view and drops the click — pre-warm it
**Symptom**: clicking a marker chip in a tall table visibly SCROLLED the preview (toward the table top) and sometimes dropped the click — only on the FIRST activation of a session, or after annotating then popping up mid-table.
**Scribobulate**: a dedicated popover pre-warm routine pre-warms the persistent popover once at first `map`, at scroll 0 (absorbs first-validation churn); the marker-popover open routine holds the saved vadj value across the popup's settle via a `value-changed` re-pin guard, wall-clock-bounded (`REPIN_GUARD_US`, 1.5 s — a `Deadline`, not a tick count; see #125) to span the deferred validation scroll, and disarmed the instant the user scrolls.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-118).

## 114. An in-place live-buffer edit that skips the canonical source-of-truth vanishes on the next fresh render
**Symptom**: an annotation created in preview-only mode vanished on a mode switch, then reappeared on the next toggle.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the mode-switch source-flush site.

## 115. Highlighting a char range in an existing Pango-markup string via `find` wraps the wrong (first) occurrence
> *Non-core (Pango markup manipulation) — kept fuller here, not folded into the core GTK skill (delineation rule). Related to GTK4Rs/AP-45 / #111.*

**Symptom**: annotating a word inside a formatted table cell highlighted a DIFFERENT (the FIRST) occurrence of the same word elsewhere in the cell.
**Root cause**: the highlight was injected via `result.find(escaped_slice)` — a TEXT search returning the first occurrence, not the annotated char position; a range crossing an inline-format boundary also isn't one contiguous substring.
**Resolution**: a dedicated char-range markup-wrapping routine walks the markup tracking the PLAIN-char index (tags=0, entities=1) and opens/closes the span POSITIONALLY, closing before and reopening after every existing tag to preserve well-nesting.
**Non-core (Pango markup manipulation) — do NOT fold into the gtk4-rs skill.**

## 116. Activating a nested-submenu item in a `GtkPopoverMenuBar` leaves a sibling top-level menu popped open — the bar clears its open menu through only one channel (a top-level popover's `unmap`)
**Symptom**: activating an item in a NESTED submenu (e.g. Format ▸ Heading ▸ Heading 1) fired correctly but left a SIBLING top-level menu (always index N−1) visibly popped open.
**Scribobulate**: `window::actions::dismiss_stray_menubar_popovers` `popdown()`s any still-mapped top-level popover on idle after a nested-submenu action (public-API only — safe against the ScrAP-63 UAF). **Now enforced by** *(structurally, Wave 6)*: a family of choke-point constructors — `nested_submenu_action` (`activate`-driven `win.`), `nested_submenu_stateful_action` (`change-state` `win.`, also applies `set_state`), and `nested_submenu_app_stateful_action` (`change-state` `app.`, dismissal routed to the active window) — is the **only** path that wires the dismissal. `dismiss_stray_menubar_popovers` was made **module-private** once all four call sites (`change-case`, `format`, `select-tab`, `preview-theme`) routed through a constructor, so the raw dismissal is no longer callable by hand at all — the opt-in latent regression this entry records is now *unrepresentable* rather than merely discouraged. Ordering nuance: schedule the dismissal **before** running the handler, not after — dismissal only *enqueues* an idle, so if it were enqueued after a handler that defers its own `grab_focus` to idle, the popover's pop-down focus-restore would run last and steal the just-grabbed focus (ScrAP-107 focus-steal).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-108). Findings: researcher-findings-popovermenubar-submenu-stays-open.md.

## 117. Clearing a `GtkLabel` `set_attributes` overlay in place needs a transient markup-STRING change to repaint — a same-string `set_markup` is a no-op
**Symptom**: clearing a find-bar highlight painted as a `set_attributes` overlay on table-cell `GtkLabel`s left the old highlight ink on screen — a same-string `set_markup` after `set_attributes(None)` was also a no-op.
**Scribobulate**: the preview-highlight clear routine does `set_attributes(None)` then a transient no-attribute `<span>` wrapper `set_markup` followed by reverting to the clean markup — two genuine string changes, zero visual difference.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-92's toggle technique, companion).

## 118. A list-item hanging indent (`left-margin` + negative first-line `indent`) is unreliable across paragraphs; the durable fix is to DROP the hanging indent (draw the marker in a gutter, uniform margin)
> *Core GTK4. Teach in the gtk4-rs skill (GtkTextView paragraph attribute resolution) — a corollary to #76.*

**Symptom**: a list item's hanging indent rendered correctly for single-line/soft-wrapped items but outdented every CONTINUATION paragraph of a multi-paragraph item to the marker column.
**Root cause**: `indent` is a first-line paragraph attribute resolved through the SAME per-line style cache as `left-margin` (#76), but — unlike `left-margin` — it must differ between a marker line and its continuations, and no single/two-tag scheme survives the cache's intermittent resolution.
**Resolution (final)**: DROP the hanging indent entirely — draw the marker in a left GUTTER (the draw/snapshot layer, out of the buffer) with a uniform per-level `left-margin` and `indent=0`, applied per logical line.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-95; kin GTK4Rs/AP-72).

## 119. A `GtkPaned` with the default narrow handle silently swallows presses in a strip at a child pane's edge
> *Core GTK4. Researcher-sourced from gtkpaned.c 4.6.9.*

**Symptom**: a gutter checkbox at a pane's left edge received NO clicks — a dead zone (not a coordinate offset), which coincidentally sat near `left_margin` and was misdiagnosed as a coordinate/margin bug for three rounds.
**Root cause**: the default narrow handle's `get_handle_area()` inflates its hit-area by `HANDLE_EXTRA_SIZE`=6px/side, and the handle's drag gesture runs in CAPTURE phase on the ancestor `GtkPaned`, claiming the sequence before the child's own gesture runs; touch is worse (`TOUCH_EXTRA_AREA_WIDTH`=50, unaffected by wide-handle).
**Resolution**: `paned.set_wide_handle(true)` (inset 0, hit-area = handle widget only); probe CAPTURE delivery per ancestor level to diagnose an edge dead-zone, never assume coordinates.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-93).

## 120. `WidgetExt::color()` is gated behind the gtk-rs `v4_10` feature — the compile error never mentions it
> *Core GTK4/gtk-rs versioning.*

**Symptom**: `widget.color()` failed to compile on a 4.6 / no-`v4_10` build target with a generic trait-bound error that never names the missing feature gate — easy to mistake for a wrong receiver type.
**Root cause**: `WidgetExt::color()` is gated behind the gtk-rs `v4_10` feature (GTK 4.10 API); with that feature disabled it is simply out of scope, and the compiler doesn't say why.
**Resolution**: use `widget.style_context().color()` (GTK 4.0; deprecated only at `v4_10`, so no warning below 4.10) on 4.6–4.12 targets.
**See**: gtk4-rs skill → versioning-and-features (GTK4Rs/AP-94).

## 121. Two `GtkTextTag`s that both set `left-margin` on a line (a list item inside a blockquote) do not compose
> *Core GTK4. Corollary of #76.*

**Symptom**: a quoted list item's `left-margin` broke out LEFT of its blockquote's own accent bar — an absolute per-depth list margin, added to the tag table after the blockquote tag, silently OVERRODE it.
**Root cause**: GTK resolves `left_margin` as the highest-priority NON-accumulative tag's value PLUS the sum of every ACCUMULATIVE tag's value; `accumulative-margin` defaults false, so two non-accumulative margin tags never compose — the later-added (higher-priority) one wins outright.
**Resolution**: make the per-depth list tag ACCUMULATIVE with an indent RELATIVE to its container (adds onto the view default OR the blockquote's margin); enforce exactly one depth tag per line (an inner tag's `TagEnd` fires before its outer, so the deepest lands first — avoids double-stacking a nested list's own margin).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-96).

## 122. Translating a stripped-then-parsed document's ranges back to original coordinates instead of per-position translation silently swallows the stripped bytes (the range-merge gotcha)
> *Non-core (pulldown-cmark) — the shape bites any delete-then-parse pipeline, not just CriticMarkup. Do NOT fold into the gtk4-rs skill.*

**Symptom**: an inline annotation adjacent to other CriticMarkup in the same paragraph (`the earth is {==flat==}{>>cite?<<} ok`) produced downstream text that silently OMITTED the stripped delimiter bytes whenever a translated RANGE was used.
**Root cause**: with delimiters stripped from the parsed ("cleaned") text before parsing, pulldown-cmark emits the surrounding prose as ONE `Text` event whose CLEANED range maps to a NON-CONTIGUOUS original region (the deleted bytes sit in the middle); a range has two endpoints and cannot express that hole, so translating it once (range-in, range-out) silently drops the gap.
**Resolution**: keep render maps in CLEANED coordinates and translate PER-POSITION at the point of use (a dedicated cleaned-to-original translation step), never as a range; identity-translate when the shift table is empty so unannotated documents keep byte-identical behaviour.
**Non-core (pulldown-cmark) — do NOT fold into the gtk4-rs skill.**
**See**: the CriticMarkup cleaned↔original shift-table mapping (verified against pulldown-cmark 0.13.4).

## 123. A coverage ratchet's floor recorded as stale prose drifts from the real (climbing) figure, silently loosening the gate
> *Non-core (tooling/process) — do NOT fold into the gtk4-rs skill.*

**Symptom**: POLICY's coverage prose cited "~66.48% lines" while the real figure had climbed to 67.83% over several unrelated cycles — nothing alerted, since a ratchet only fires on a DROP.
**Root cause**: the figure lived in three places (the coverage-gate script's floor constant, POLICY's inline snippet, prose) with no single source of truth; a contributing misread nearly set the floor from `cargo-llvm-cov`'s eye-catching FIRST (Regions) column instead of the gated LINES column.
**Resolution**: ratchet the floor and the stated figure together whenever coverage rises; verify a ratchet change by the gate's EXIT CODE, never the printed percentage.
**Non-core (tooling/process) — do NOT fold into the gtk4-rs skill.**

## 124. A test suite gated behind a Cargo feature is invisible to every gate that does not enable it — it rots until it stops compiling, and every line it covers reads as 0%
> *Core-GTK half in the gtk4-rs skill as GTK4Rs/AP-98 (automated-UI-testing); the tooling/process remainder (which pipeline step compiles this?) stays project-specific — same precedent as #36.*

**Symptom**: Scribobulate's 398 `gtk-integration-tests` (the only suite covering real GTK wiring — real windows, real paint) went invisible to every pipeline gate; a type refactor broke their build while fmt/clippy/build/test/coverage all stayed green through multiple debrief cycles.
**Root cause**: the tests are `#[cfg(all(test, feature = "gtk-integration-tests"))]`; neither plain `cargo test` nor plain `cargo clippy --all-targets` compiles feature-gated code at all — the feature was documented as HOW to run them, never enforced as a pipeline step ("documented" masqueraded as "enforced").
**Resolution**: put the feature in the pipeline — `clippy --features gtk-integration-tests` and a dedicated `xvfb-run -a cargo test --features gtk-integration-tests` step (a display is not an excuse).
**Non-core (tooling/process remainder — which CI step compiles a gated suite) — do NOT fold into the gtk4-rs core skill.** Sibling of #123.
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-98, the core "green ≠ covered for a gated suite" lesson).

**Second case — the same blindness, seen by the COVERAGE gate (2026-08-07).** The gate above compiles the suite; `scripts/coverage.sh` deliberately does not (it is unit-only, and the floor is calibrated that way). So a module whose tests are *entirely* feature-gated reports **0%** while being thoroughly tested. `src/farscroll.rs` and `src/saferizer/scrollpos.rs`, both added by `6d73875`, read 0% while carrying seven `#[gtktest::test]` bodies between them, and their uncovered lines took scoped line coverage from ≥76.76 to **76.20** — the ratchet was red on `master` and stayed red, because step 6 was prose on Linux and nobody ran it. It surfaced the first time an executable pipeline ran the step.
Two traps for whoever tries to fix it, both measured here:
- **Adding unit tests is a weak lever**, because test bodies count in the DENOMINATOR too — extracting farscroll's pure decision cores and writing 15 tests moved the total only 76.20 → **76.31** (58 added lines bought 2 net covered production lines). Reaching a floor this way needs absurd volume: ~355 fully-covered new lines to gain the 0.56pt actually required.
- **The obvious fix is not sufficient either** — excluding `farscroll.rs` from `IGNORE` was measured at **76.66**, still short, because the breach spanned two modules rather than one. Verify an exclusion by running it, not by arithmetic on one file.
**Resolution taken**: `FLOOR` lowered 76.76 → 76.30 by operator decision, with the alternatives and the reason recorded beside the constant — because the rule is *never lower the floor*, so an unexplained drop is indistinguishable from the drift #123 exists to catch.
**The general form, and the reason this is one entry and not two**: a Cargo feature gate does not merely hide a suite from the compiler — it makes every line that suite covers read as *uncovered*, so **"0% coverage" and "no tests at all" are identical in the report and only one of them is true.** That is #212's shape ("`#[cfg(unix)]` on a test and 'skipped on Windows' are indistinguishable in the report") arriving through the coverage tool instead of the test report.

## 125. Scheduling work that depends on paint-populated state via `idle_add_local_once` reads the previous frame's state and silently no-ops
> *Core GTK4. Instance of the deferred-work/ordering family — presents as a race, is really a temporal-ordering hazard.*

**Symptom**: a marker-popover "open next" a11y action scrolled the view to an off-screen marker then silently found no hit-box and never opened a popover — no error, no log; the view visibly scrolled and nothing else happened.
**Root cause**: the marker hit-box cache is populated ONLY by the draw/snapshot layer (an actual paint); `scroll_to_mark` defers its real scroll onto its OWN coalesced idle and ANIMATES over several frames, so the naive idle fires against the PRE-scroll paint's hit-boxes — `idle` answers "after the current main-loop turn", never "after the next paint".
**Resolution**: never bound this on a FRAME COUNT — the thing being waited on is wall-clock (GTK's scroll animation is a fixed 200 ms, `ANIMATION_DURATION` gtkscrolledwindow.c:196), while a tick fires per frame at a refresh rate the app does not control; 45 ticks is 750 ms at 60 Hz but 187 ms at 240 Hz — *shorter than the animation itself* — and wildly over-generous when `gtk-enable-animations` is FALSE (duration 0 ⇒ instant). Prefer, in order: (1) **generate the completion event yourself** where you own the paint — repopulate the state in `snapshot_layer`, then dispatch the waiting work with `idle_add_local_once` (never inline: you are in the draw path, ScrAP-22/ScrAP-30). GTK offers no signal to wait on — `GtkTextView` has none, `GtkTextLayout`'s `changed`/`invalidated` are not public GTK4 API, and `GDK_FRAME_CLOCK_PHASE_AFTER_PAINT` is documented "should not be handled by applications". (2) Failing that, poll the frame clock but bound it with a **wall-clock deadline** (`glib::monotonic_time`, ~1.5–2 s) — an absolute stamp, never an accumulated per-frame delta. Make the give-up branch **observable** (leave the visible side effect applied), never a silent no-op. Verify with a mutation test (reverting to a single idle must make the covering test FAIL). **Landed** as the `Deadline` newtype (`monotonic_time` absolute stamp) + `NAV_BUDGET_US` (2 s) / `REPIN_GUARD_US` (1.5 s) in `src/codeview/markers.rs`, replacing the former `MAX_FRAMES = 45`; the wall-clock-vs-frame-count lesson is #134.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-97).

## 126. Styling a `GtkTextView`'s background via `textview { background-color }` alone works on Default but is defeated by the user's system theme
**Symptom**: a reading theme's sepia background rendered correctly under GTK's Default theme but showed white sans text over an OPAQUE dark background on Breeze-Dark — theme-sensitive, so both source-reading and a Default-theme test passed while the shipped app was broken.
**Scribobulate**: the preview CSS theming step styles BOTH nodes — `textview { color; font-family }` for the widget node (also read by GTK's own caret/color paths) + `textview > text { background-color }` for the fill.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-100).

## 127. Reaching for CSS selector specificity to arbitrate between two `GtkCssProvider`s is a category error
> *Core GTK4 — teach in the gtk4-rs skill; researcher-sourced from gtk-4-6 @492b44f20c (4.6.9).*

**Symptom**: a carefully-scoped selector in provider B did not beat a bare, unscoped selector in provider A at the same priority — GTK's cascade is NOT web CSS.
**Root cause**: GTK resolves a property by scanning providers highest-priority-FIRST, taking the FIRST value found permanently; specificity only breaks ties WITHIN one provider, never across providers — equal priority is decided purely by add order (last-added scanned first). The `font:` shorthand also silently clobbers a sibling provider's `font-size` by expanding to six longhands.
**Resolution**: never let two providers own the same property — give each provider a DISJOINT set of properties so they compose without arbitration (in Scribobulate the zoom provider owns CSS `font-size` exclusively, while the theme owns Pango *scale* — a tag attribute GTK multiplies onto the CSS base, a different lookup slot — enforced by the `the_theme_sheet_never_writes_a_property_zoom_owns` guard test in `src/preview/css.rs`); never emit the `font:` shorthand, which expands to six longhands including `font-size`. (A prior revision's example — "bake a theme's base scale into the zoom provider's `font-size`" — was fabricated: it describes exactly what that guard test forbids.)
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-101).

## 128. `g_get_user_config_dir()`'s process-global lazy cache makes a mid-startup `XDG_CONFIG_HOME` redirect and an honest config-dir read mutually exclusive
> *GLib/XDG dir caching + app-lifecycle. **Landed in the gtk4-rs skill as GTK4Rs/AP-173** (glib/gio are inside that skill's scope — see the routing rule); researcher-sourced from GLib 2.72.4 gutils.c/genviron.c + GTK 4.6.9 gtkimcontextsimple.c, probed both orderings.*

**Symptom**: whichever call resolves the user config dir FIRST wins permanently for the whole process — resolving it early ("just to be safe") silently RE-ARMS the ScrAP-3 XCompose crash; resolving it late (after the redirect) silently loses the user's real `~/.config` overrides for anything reading the cache afterward.
**Root cause**: `g_get_user_config_dir()` caches into a global static on FIRST call (gutils.c:1865-1878, GLib 2.72.4), and GTK's own compose-table read, GIO's MIME/default-app lookup (which additionally probes a `<desktop>-mimeapps.list` FIRST, per lowercased `XDG_CURRENT_DESKTOP` entry), and the GTK-3-path bookmarks file (`gtkbookmarksmanager.c`) all consume that SAME cache — a process-global lazy cache turns "which call happens first" into a permanent, invisible correctness decision.
**Resolution**: snapshot CONFIG from `std::env` BEFORE the redirect and never call `glib::user_config_dir()` anywhere in the app or its dependencies; mitigate each known reader individually — symlink `mimeapps.list` (deriving the desktop-specific filename set from live `XDG_CURRENT_DESKTOP`, not hardcoding) and symlink the `gtk-3.0` DIRECTORY (never a file — a crash-safe write-temp-then-rename writer defeats a file-level symlink).
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-173; kin GTK4Rs/AP-3).

## 129. `g_app_info_launch_default_for_uri(uri, NULL, …)` silently emits no activation token, so a WM's focus-stealing prevention refuses to raise the handler
**Symptom**: clicking a link opened the browser tab BEHIND the app and returned `Ok` — no warning, no log; only reading the child process's ENVIRONMENT (`DESKTOP_STARTUP_ID`) reveals the missing token.
**Scribobulate**: `gtk_show_uri_full(None, uri, 0, None, cb)` builds the launch context automatically — `None` as parent is DELIBERATE (a parent buys no extra token, only a `PARENT_WINDOW_ID` at the cost of an `gtk_window_export_handle` unexport warning on EVERY call on 4.6 X11, fixed upstream in 4.8 but never backported). Timestamp `0` here is correct and must not be "fixed" — see #47 for why the launch path substitutes it while the focus path does not.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-99).

## 130. A hand-authored SVG that renders fine in Inkscape can be invalid XML that librsvg (and GTK) rejects outright
> *Non-core (librsvg/docs tooling) — do NOT fold into the gtk4-rs core skill.*

**Symptom**: `sdd/system-overview.svg` rendered perfectly in Inkscape but the app itself showed a broken-image placeholder for its own architecture diagram.
**Root cause**: three `<text>` elements carried a DUPLICATE `class` attribute — a fatal XML well-formedness error; Inkscape's libxml2 recovery mode silently keeps the first occurrence and continues, while librsvg parses strictly and fails the WHOLE document with no partial render.
**Resolution**: `xmllint --noout file.svg` is the gate for any hand-authored/generated SVG, run BEFORE ever trusting a render; confirm in the actual (strict) consumer, never the lenient authoring tool. Corollary: librsvg ignores `prefers-color-scheme: dark`, so light-theme defaults must self-sufficiently fill every box.
**Non-core (librsvg/docs tooling) — do NOT fold into the gtk4-rs core skill.**

## 131. A refactor that REDEFINES what an existing field means keeps compiling at every call site, and silently changes behaviour
**Symptom**: splitting `Palette` for theming redefined `is_dark` from *"the desktop theme is dark"* to *"the rendered page is dark"* — the same name, same `bool`, same struct. Every existing reader compiled untouched. Two of them set the **editor's** GtkSourceView style scheme, so selecting a light reading theme on a dark desktop would have flipped the editor to a light scheme — a pane the theme is explicitly not supposed to touch. Nothing failed; the tests stayed green.
**Scribobulate**: the preview palette no longer carries a page-lightness field; a comment records why, and anything outside the preview probes the desktop's lightness through a dedicated helper. TDD 18.7.
**See**: general-engineering-principles (GEP-40).

## 132. A source-scanning guard test whose scope filter is wrong scans nothing and passes forever
**Symptom**: a regression guard asserting that no module resolves the config dir through `glib::user_config_dir()` (which would silently re-arm the XCompose crash and/or lose the user's theme overrides — #128 family) passed. It also passed with a deliberate violation injected. It was scanning an empty string.
**Scribobulate**: this entry describes the now-retired config-dir-resolution scanner (the "no module resolves the config dir through `glib::user_config_dir()`" guard) — its `#[cfg(test)]`-truncation bug, and later its hardcoded-4-file scope (itself this very species), meant it never scanned the sites it policed. That invariant is now enforced compiler-wide by a `clippy.toml` `disallowed-methods` ban on `glib::user_config_dir` (N10) — a gate with no scope to get wrong. The one surviving injection-verified absence-guard is `workaround.rs::the_redirect_touches_config_home_only` (the redirect's `set_var(` lines must name `XDG_CONFIG_HOME` and never the data dirs), whose single-file scope is correct because `set_var` only happens there.
**See**: general-engineering-principles (GEP-1).

## 133. A hard-coded Xvfb display lets one crashed run orphan a server that silently serves stale windows to every run after it

**Symptom**: two sequential headless captures, each pinning a *different* reading theme through its own private session file, both came out in the **same** theme — the frame specified as Terminal rendered Synthwave. Every screenshot was a valid PNG of a real, correctly-rendered Scribobulate window; it was simply the *wrong window*. Suspicion landed on the theme engine (session parsing, theme resolution, the `-n` new-instance flag), all of which were fine. An isolated capture of the same theme rendered perfectly, which made it look intermittent.

**Root cause**: an earlier run had failed at its screenshot step and, under `set -e`, aborted **before** its cleanup lines, orphaning `Xvfb :99`. With the display number hard-coded, every later `Xvfb :99 &` exits immediately ("server already active") — but the launcher never checks: `$!` is a already-dead PID, so the matching `kill` reaps nothing, and the app happily connects to the **pre-existing** server. The window search then finds whatever stale windows are lying around on it. Nothing errors, nothing warns; the capture just reads someone else's screen. The failure is invisible precisely because every individual step "succeeds".

**Resolution**: never hard-code the display — let Xvfb allocate a free one and report it (`Xvfb -displayfd 1 … >file`, then read the number back), so two runs can never collide. Make teardown unconditional: record every spawned PID and reap it from an `EXIT` trap, because `set -e` skips the teardown exactly when a failure has made teardown most necessary. Verified after the fix by asserting on pixels rather than trusting the run: Terminal captured true-black `srgb(0,0,0)`, Synthwave indigo `srgb(26,16,51)`.

**Scribobulate**: the capture stage of `scripts/gen-splash.sh` (display allocation + `SPAWNED_PIDS` / `trap … EXIT`).

**Lesson**: a **fixed global resource id** — an X display, a port, a lock path, a fixed temp dir — converts one crashed run into a booby trap for every run after it, because the second process does not fail, it *silently attaches to the first one's leftovers*. Two rules: **allocate the id dynamically and let the tool tell you which one it got**, and **make cleanup unconditional (a trap), never a trailing line an early exit can skip**. The diagnostic corollary generalises past X11: when a headless capture disagrees with the state you configured, suspect the **harness/environment before the application** — an artifact that is *wrong but internally valid* is the signature of reading the wrong source, not of a broken feature. **Belongs in the `gtk4-rs` skill's automated-UI-testing methodology** (launch/teardown discipline, alongside the existing "only ever kill the PID you launched" rule) — sent to the skill maintainer.

**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-131).

## 134. Bounding a wait on a FRAME COUNT when the thing waited on is measured in WALL-CLOCK
**Symptom**: an `add_tick_callback` poll bounded by `if ticks >= N` works on the dev box and fails on a user's high-refresh panel — **silently**, because the give-up branch does nothing. Nobody reproduces it, because nobody's test box is 240 Hz.
**Scribobulate**: `Deadline` + `NAV_BUDGET_US` / `REPIN_GUARD_US` in `src/codeview/markers.rs`. The two defective sites were a `MAX_FRAMES = 45` ("~0.75s at 60Hz") navigation poll and a `ticks >= 48` ("~0.8s @60fps") re-pin guard. A sweep confirmed no third site (`preview/scroll.rs` is already wall-clock; `scrollsync.rs` breaks after one tick).
**See**: gtk4-rs skill → deferred-work-and-ordering (GTK4Rs/AP-122).

## 135. `GtkText` writes PRIMARY on every selection change — and a widget claiming PRIMARY CLEARS the previous owner's selection
**Symptom**: with an in-surface comment card open and text typed, **Ctrl+A *or* Shift+Home** dismissed the card and silently discarded the text. Two keystrokes — and the obvious explanation (a window-accelerator collision) covered only one of them, which is exactly why a fix built on it would have "worked" and left the other broken.
**Scribobulate**: `preview/annotate/overlay.rs`, inside `schedule`'s debounce timer — the code's own comment already *claimed* the contract ("the selection it was anchored to changed") but never **checked** it; it inferred it from "a signal fired".
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-120).

## 136. Seeding live UI state from the persisted-session snapshot
**Symptom**: a UI preference toggled mid-session is silently ignored by the next window opened — it comes up with the last-**persisted** value instead. Presents as per-path ("pop-out is broken") but is shared by every new-window path. It survives a restart *correctly*, which misdirects diagnosis toward the persistence layer, where nothing is in fact wrong.
**Scribobulate**: `session::LiveChrome` + `update_live_chrome` (a `thread_local`; GTK is single-threaded), read by `window/mod.rs`'s `build_window` — **since retired.** The live app-wide cache was a correct fix to the *read* staleness and a wrong answer to the underlying question: the state was never app-wide. It is now window-scoped (`session::ChromeSession` inside each `WindowSession`), and the live source of truth is each window's own `win.*` action states — which cannot go stale, because the toggle handlers that write them are the only claimants. `window::read_window_chrome` is the one reader, shared by the seed path (`inherit_from`) and the persist path.
**See**: general-engineering-principles (GEP-41).

## 137. A window `GAction` accelerator BEATS a focused `GtkText`'s own keybinding — and *disabling* the action is what hands the key back
**Symptom**: Ctrl+A in an entry inside your window selects the whole **document** instead of the entry's text, and the entry appears to have no select-all at all. Any window accel shadowing a standard text-editing key (Ctrl+A/C/X/Z…) has this against **every** `GtkEntry` in the window.
**Scribobulate**: `focus_in_text_entry` (`window/actions.rs`) gating `win.select-all` (`window/editoractions.rs`). It began life as `focus_in_annotation_card`, a CSS-class-ancestor check scoped to one card, and had to be widened to a type check once the mechanism was understood — the find and replace entries had the identical bug.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-121).

## 138. Polling a `GtkEntry`'s own `has_focus()` in a test spins forever — focus lands on its internal `GtkText` delegate
**Symptom**: a headless test grabs focus into a `GtkEntry`/`GtkSearchEntry`, then pumps the main loop until `entry.has_focus()` — and never returns, timing out or hanging. The widget is visibly focused and behaving correctly; the probe simply never observes it.
**Scribobulate**: the readiness probes in `window/editoractions.rs`'s select-all standdown rubric, and `focus_in_text_entry` (`window/actions.rs`), which is keyed on `gtk::Text` for exactly this reason.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-119).

## 139. A `GtkText`/`GtkEntry` selects ALL its text on focus-in, silently undoing a caret set BEFORE `grab_focus` — and the hazard IS guardable headlessly, if the toplevel is MAPPED
**Symptom**: an entry pre-filled with text the user must not lose opens **fully selected**, so their first keystroke silently replaces the whole value. Nothing warns; the pre-fill is visibly present and completely useless. It bit **both** annotation surfaces here, each time in the *first working build of the fix whose entire purpose was to stop silent data loss* — the loss walking back in through the door the fix opened.
**Scribobulate**: `preview/annotate/overlay.rs` and `window/editor_annotate.rs` both `set_position(-1)` after `grab_focus`; the editor card's ordering guard uses a `raise_card_over_mapped` helper for exactly this reason.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-124); #106 (same GTK select-on-focus behaviour, `GtkLabel` in a popover), #138 (the wrapper's `has_focus()` is not the delegate's).

## 140. A security gate answering a DIFFERENT question than the one being asked
**Symptom**: a relative Markdown link (`[architecture](TECH.md)`) produced no navigation, no error, no dialog — only a WARN log line ("refusing to open URL with disallowed scheme"). Looked like a missing feature. It was a **correct gate being asked the wrong question**.
**Scribobulate**: `links.rs` (`is_allowed_url` / `resolve_doc_link` / `scheme_of`) + `window/linknav.rs` (the dispatcher).
**See**: general-engineering-principles (GEP-45).

## 141. A "this will misbehave" theory read from a construction site, never executed
**Symptom** (of the *process*, not the app): a plausible, code-grounded bug report that **does not reproduce**. Here: reading a `WindowInit`-style seeding struct showed a per-tab toggle's mirrored `GAction` being seeded from a hardcoded construction-time default, independent of whichever tab lands there, with nothing *obviously* re-syncing it on the transplant path — a live "the checkbox lies about what the tab will do" bug, in a **security** toggle.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-15).

## 142. A capture-phase ancestor gesture cannot pre-empt a child's gesture and hand it back cleanly — "one similar event will be emulated" preserves event COHERENCE, not gesture STATE
> *Core GTK4. Candidate for the gtk4-rs skill (controllers-and-bindings). Probed on GTK 4.6.9 with real `xdotool` clicks; both failure modes measured.*

**The tempting design**: you want an ancestor to *maybe* take over an interaction that starts inside a child (e.g. promote a selection out of a table cell once it escapes). GTK appears to sanction "claim early, decide late": have a capture-phase ancestor gesture **claim** the sequence on press, then **DENY** it once you know you don't want it — the docs say a denied sequence has "one similar event emulated" for the other gestures, so the child should proceed as if nothing happened. **It does not work. Both halves fail, and the second fails silently.**

**Measured** (selectable `GtkLabel` reading `selection_bounds()` after a real double-click):

| Setup | Result | |
|---|---|---|
| control — no ancestor gesture | `(0,5)` = `"alpha"` | word-select works unaided |
| deny on drag-update only (the literal recipe) | **`None`** | the label receives **nothing** |
| + deny on release (the gap patched) | **`(0,11)` = `"alpha bravo"`** | **silently wrong** |

**Why**: (1) **a click has no motion** — `drag_update` never fires, so a deny placed only there is **dead code**; the sequence stays CLAIMED and the press never reaches the child at all. (2) Patching that with a release-deny does deliver the emulated presses, but **`GtkGestureClick`'s n-press counter does not survive being pre-empted and re-emulated** — a double-click selects **two words**.

**And the exit is one-way**: the child's own gesture claims on press (`gtklabel.c:4313`), which sets **DENIED on every ancestor gesture in the chain** (`gtkgesture.c:84-92`), and **DENIED is terminal** (`:1020-1035`). So an ancestor can never claim once a drag is underway — mid-drag promotion is not merely hard, it is foreclosed.

**Resolution**: don't pre-empt a child's gesture. Either let the child own the interaction and observe it through a **public signal with a before/after state comparison** (e.g. `GtkLabel::move-cursor` is a keybinding signal that fires *before* the default handler clamps, so a boundary escape is observable via `connect_after` + comparing `selection_bounds()`), or own the interaction outright and rebuild what you displaced. **Price the second honestly**: the control run above is what it costs — word-select, line-select, keyboard selection and PRIMARY integration all come free from the child and would all have to be reimplemented.

**The trace** (why the exit is one-way): `GtkLabel`'s lazily-created selection machinery (`gtk_label_ensure_select_info`, `gtklabel.c:4826`) includes a `GtkGestureClick` that **claims on press** (`:4313`). A claimed sequence sets `DENIED` on **every gesture on parent widgets in the propagation chain** (`gtkgesture.c:84-92`), and **`DENIED` is terminal** (`:1020-1035`). By the time the pointer escapes the child, an ancestor can never claim. Capture-phase ancestors *do* see the motion — **observation was never the problem; claiming is.**

**The reduced repro** (trace on a double-click: `drag_begin=2, drag_update=0, denied=0, drag_end=2` — note `drag_update=0`, which is why the documented shape is dead code):
```rust
let drag = gtk::GestureDrag::new();
drag.set_propagation_phase(gtk::PropagationPhase::Capture);
drag.connect_drag_begin(|g, _, _| g.set_state(gtk::EventSequenceState::Claimed));
drag.connect_drag_update(|g, _, oy| {
    if oy.abs() <= 30.0 { g.set_state(gtk::EventSequenceState::Denied); }
});
drag.connect_drag_end(|g, _, _| g.set_state(gtk::EventSequenceState::Denied)); // REQUIRED, and still not enough
table.add_controller(drag);
```

**Scribobulate**: probed for a table-cell selection-promotion design; the routes above are why that design is not viable, and why table cells remain selection islands.

**Two probe traps, if you re-run this**: a selectable `GtkLabel` **selects-all on focus-in** (#106/#139), so a fresh window reads `(0, len)` *before any gesture* — clear the selection immediately before measuring, or you will record that select-all as your gesture's result. And `widget.allocation()` is **parent-relative and excludes ancestor margins**, so clicking at `alloc.x()+n` can land in the container's margin, outside the widget.

**Lesson**: **a documented guarantee about EVENTS is not a guarantee about STATE.** "One similar event will be emulated" is true and was never the question — the events do come back; the *stateful gesture that was counting them* does not come back with them. We read the promise, assumed it meant "as if nothing happened", and did not check the after-state. That is the same failure as #135 (relying on what a signal's *name* implies) and #141 (reasoning from a construction site without executing) — **three variants in one session of trusting a description instead of measuring the outcome.**

Note the failure **shape**, because it is the teeth: not `None` (a user reports that), not the whole line (a coherent triple-click) — but **plausible-but-wrong**, `"alpha bravo"` flowing into Copy. Like the hypothesis-shaped probe in #135, **it does not fail loudly; it succeeds at being wrong**. When evaluating an interaction design, measure a **control** first (what does the widget do unaided?) — it both proves your harness can see the truth and prices the rebuild if you take the interaction over. **Belongs in the `gtk4-rs` skill** — sent to the skill maintainer.

**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-171).

## 143. A PERMANENT register entry citing an EPHEMERAL artifact (an ISSUES entry, a PLAN file)
**Symptom**: an ANTI-PATTERNS entry says "see `sdd/PLAN.<topic>.md` for the trace and the repro" — and the plan is gone. The entry does not break loudly; it just **stops resolving**, and the next reader concludes the evidence was never captured and re-derives it. Or worse, the ID has been recycled and the pointer resolves to something unrelated: it lies quietly.
**Scribobulate**: #142 was filed citing a plan file that was deleted ~20 minutes later, when the probe killed the design and its findings were folded into the issue register. The entry now inlines the gesture trace and the reduced repro and cites nothing ephemeral.
**See**: general-engineering-principles (GEP-23).

## 144. `unparent()` on an OPEN GtkPopover does not emit `closed` — it skips the close path entirely
> *Core GTK4. **In the gtk4-rs skill as its GTK4Rs/AP-123** (landed `a619a05`), framed there as a sharper corollary of **the skill's GTK4Rs/AP-80** — which already carried the weaker form ("a popover open at teardown never emits `closed`, so the dispose sweep is load-bearing"). This entry's new value is the actionable ORDER rule and the trap synthesis, which GTK4Rs/AP-80 does not state. Instance of the teardown-ordering family. Completes the popover-teardown set with **#90** (must unparent), **#112** (must not destroy per use) and **#98** (autohide ⇒ real seat grab, and real-compositor-only) — each of which pushes toward a different wrong fix; see "The trap this sits in the middle of". `closed`-never-fires half is **measured + mutation-proven both ways**; the stranded-grab half is **CONFIRMED live on real KDE/X11** (2026-07-17) — a reverted-fix binary goes input-dead after an auto-reload destroys an open autohide marker popover, while the fixed binary stays alive, and a **non-autohide** popover (the create-annotation overlay) under the identical reload stays alive on the *same* reverted binary. That non-autohide control isolates the mechanism to the seat grab: the only difference between dead and alive is `autohide`. Xvfb cannot observe this (#98) — it took the operator's live session.*
>
> *⚠ **Citation hygiene** — this entry originally cited the bare numbers **90/98/112/117** for the three neighbours above, and every one was wrong: under the *retired* positional convention a bare number in this file denoted a **gtk4-rs skill** entry while `#N` denoted an entry **here**. GTK4Rs/AP-98 is the CI/feature-gate lesson (from this register's #124), not the seat grab; and project #117 is a GtkLabel-attributes lesson with nothing to do with popovers. See #145.*

**Symptom**: a popover is torn down (its parent widget is destroyed, or `dispose` unparents it) while still open, and everything a `closed` handler was supposed to do simply never happens. Nothing warns. The most visible form here: an "is a popover open?" flag cleared in `connect_closed` is left stuck **`true`** on a destroyed view, so the app's own source of truth for that popover reports open forever.

**Root cause**: **`gtk_widget_unparent()` is a structural detach, not a close.** It unrealizes the popover's surface and drops it out of the widget tree without ever running the popover's close path — so `closed` is never emitted. `popdown()` is what emits `closed`. The two are not alternatives that differ in tidiness; they do **different things**, and only one of them tells the rest of your program that the popover is gone.

**Measured, not inferred** (Scribobulate, gtk4-rs, Xvfb): with `popdown()` removed from `dispose` and only `unparent()` left, a `connect_closed` handler installed on a confirmed-open popover **does not fire** — reverting the fix fails the guard, restoring it passes. That is the discriminator the regression test is built on: `closed` fires on a genuine popdown and on nothing else, which makes it the exact observable for "was this closed properly or just detached?".

**Why this bites harder than a missed callback**: GTK4's default is `autohide = TRUE`, and an autohide popover takes a **real seat grab** (#98, `gdk/x11/gdksurface-x11.c:1901`). So the popover destroyed by the structural path is exactly the popover holding a grab — and the close path that would release it is the one being skipped. *(The in-tree instance, **confirmed live** 2026-07-17: app dead to clicks and keys while the main loop stays alive — the reload toast still renders and the window still accepts WM focus (`windowactivate --sync` succeeds), so it is not a hung loop; input is intercepted by the stranded grab. The differential is a reverted-fix vs. fixed binary under the identical reload-with-popover-open, plus a **non-autohide** popover as the negative control: on the *same reverted binary*, the non-autohide create-annotation overlay survives the reload while the autohide marker popover does not — so the mechanism is the seat grab specifically, `autohide` being the only variable. The `closed`-never-fires half remains the separately-measured half; both are now established.)*

**Resolution**: **`popdown()` THEN `unparent()`.** Both. In order.

```rust
fn dispose(&self) {
    for slot in [&self.overlay_popover, &self.marker_popover] {
        if let Some(p) = slot.borrow_mut().take() {
            p.popdown();               // release the grab / run the close path
            if p.parent().is_some() {
                p.unparent();          // still mandatory — see #90
            }
        }
    }
}
```

Unconditional is fine and simpler than gating on an open-check: **`popdown()` no-ops on an already-closed popover.** Verify only that your `closed` handler is safe to run during teardown — clearing a `Cell` through a weak ref is; re-entering layout or unparenting from `closed` is not (that unrealize is #112).

**The trap this sits in the middle of.** Each neighbouring rule pushes toward a *different* wrong fix, which is why the correct one is easy to miss:
- **#90** (GTK4Rs/AP-80) — you MUST unparent a `set_parent()`ed popover at dispose; it is not auto-unparented, so it leaks. "Just don't unparent" is wrong.
- **#112** (GTK4Rs/AP-117) — you must NOT unparent/destroy a popover per use; an unrealized popover strands a tooltip timer on a NULL surface (`GDK_IS_SURFACE` criticals). Keep one persistent instance and only `popup()`/`popdown()` it. So "unparent it whenever it closes" is wrong.
- **#144 (this)** — you must not unparent it *while open*. So "unparent unconditionally at dispose", which satisfies both of the above, is *still* wrong.

Only `popdown()` **then** `unparent()`, once, at dispose, satisfies all three. A comment at such a site guarding one of these errors reads as if the question is settled — Scribobulate's `dispose` carried a #90 comment ("you MUST unparent") that made the code look deliberate and reviewed while it was silently doing the #144 thing.

**Lesson**: **when a widget has both a lifecycle operation and a structural operation, they are not interchangeable, and the structural one will not run the lifecycle one for you.** Ask what the object must *announce* before it is detached, not merely what must be detached. The general tell: if a teardown path unrealizes a widget that owns a grab, a timer, or an input target, look for the announce-step it skipped — and pick your test's observable to be the announcement itself (`closed`), never a state flag your own handler sets, because the flag is exactly what goes stale when the announcement is skipped.

## 145. Two registers numbering their entries with the SAME prefix — every cross-citation is wrong-but-plausible
**Symptom**: an entry cites the bare number **98** for a fact that the entry it resolves to does not contain. Nothing dangles, nothing 404s — the number **resolves**, to a real entry, about something else entirely. The reader follows it, finds a plausible-looking lesson that doesn't say what the citing text claimed, and concludes they have misunderstood the *code* rather than that the *citation* is wrong.
**Scribobulate**: this register's `ScrAP-N` citation prefix and the citation-convention paragraph at the top of this file; `lint-references` check 8, which makes the ambiguous bare form illegal rather than defaulted.
**See**: general-engineering-principles (GEP-24).

## 146. Assuming `GdkTexture::from_file` ignores installed gdk-pixbuf loaders, and adding a manual `Pixbuf` fallback
**Symptom**: a WebP renders as the broken-image marker even with `webp-pixbuf-loader` installed. The intuitive read — "`GdkTexture` has a fixed native loader set and won't use gdk-pixbuf's runtime loaders" — leads you to add a manual `Pixbuf::from_file` → `Texture::for_pixbuf` fallback.
**Root cause**: **that read is FALSE on GTK 4.6.x** (measured, not assumed). `gdk_texture_new_from_file` → `gdk_texture_new_from_bytes` tries the native set (PNG/JPEG/TIFF), then on an unsupported format FALLS BACK to `gdk_texture_new_from_bytes_pixbuf` → `gdk_pixbuf_new_from_stream` (`gdktexture.c`, 4.6.9) — so `Texture::from_file` **does** consult gdk-pixbuf's installed runtime loaders. A format failing *with the loader installed* is therefore a loader-**registration** problem: the loader is missing from the `loaders.cache` the PROCESS actually reads (stale cache / `GDK_PIXBUF_MODULE_FILE` / sandbox — GTK4Rs/AP-66), even when it is on disk (here the CLI `gdk-pixbuf-query-loaders` printed nothing while the process's cache held the webp entry ×5). And the "fallback" is *less capable*: on 4.6.9, `Pixbuf::from_file` on an animated WebP errors **"Cannot create WebP decoder"**, while `Texture::from_file` and `Pixbuf::from_stream` (the path `Texture` uses internally) both decode it.
**Resolution**: no manual fallback — load via `Texture::from_file` and let its built-in chain handle native + registered-pixbuf formats. If a format won't render despite an installed loader, fix the **registration** (regenerate `loaders.cache`, point `GDK_PIXBUF_MODULE_FILE` at the right one), don't route around `GdkTexture`. **Scope: verified GTK 4.6.9 only** — a later GTK that dropped the internal pixbuf fallback would make a manual fallback version-gated; re-verify before assuming.
**Lesson**: verify which layer already does the work *before* adding a "fallback" — the toolkit call you think is limited may already do exactly what you're about to reimplement, and your reimplementation can be strictly worse (`Pixbuf::from_file` vs the `from_stream` path `Texture` uses). A capability that "doesn't work" is often a registration/config gap, not a missing feature. This entry **retracts** its own first draft (which asserted `GdkTexture::from_file` never consults runtime loaders) — a source-plausible hypothesis that a ten-minute execution probe disproved. Verified: probe on 4.6.9 → `Texture::from_file`=Ok, `Pixbuf::from_file`="Cannot create WebP decoder", `Pixbuf::from_stream`=Ok.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-66; kin GTK4Rs/AP-34).

## 147. Raw-HTML `<picture>`/`<img>` silently dropped — block HTML is emitted per-line, wrapped in `Tag::HtmlBlock`
> *Non-core (pulldown-cmark/CommonMark) — parser event-stream behaviour, not a GTK lesson. Do not fold into the gtk4-rs skill. Sibling of #66/#75 (pulldown flanking/fragmentation).*

**Symptom**: three related failures. (a) A GitHub-style `<picture>…</picture>` hero (WebP `<source>` + GIF `<img>` fallback) renders as NOTHING in-app, even though it displays fine on GitHub — the app's own README hero was invisible when the app opened its own README. (b) After (a) was fixed, a `<source>` and `<img>` WITHOUT an enclosing `<picture>` rendered only ONE image (the `<source>` wrongly suppressed the sibling `<img>`), when nothing links them so both should render. (c) A *single-line* `<picture><source><img></picture>` rendered two images instead of one — the `<picture>` grouping was lost.
**Root cause**:
- (a) The renderer's pulldown-cmark event loop drops `Event::Html`/`Event::InlineHtml` via a catch-all `_ => {}` (sanitize-by-omission — correct for untrusted HTML). pulldown-cmark 0.13 emits a **block** HTML construct **line-by-line** — one `Event::Html` per source line — **wrapped** in `Event::Start(Tag::HtmlBlock)` … `Event::End(TagEnd::HtmlBlock)`. Rendering each `Html` event independently mangles the multi-line block; a "flush on the next non-Html event" heuristic is fragile.
- (b) Treating any block that contains a `<source>` **and** an `<img>` as a `<picture>` fallback group. The fallback (source overrides img) is a semantic the **enclosing `<picture>`** establishes; applying it to ungrouped elements wrongly suppresses an independent image.
- (c) **A single-line `<picture>…</picture>` is NOT a CommonMark HTML block** — HTML-block *type 7* requires the opening tag to be followed by only whitespace to end of line, so `<picture><source>…` (tag followed by another tag) fails it and pulldown parses the line as a **paragraph**, emitting each tag as a SEPARATE `Event::InlineHtml`. Grouping candidates *within a single parse call* therefore loses the `<picture>` across the event boundary.
**Scribobulate**: a pure, unit-tested scanner turns a fragment into an ordered tag stream (`PictureOpen` / `PictureClose` / `Candidate(src)`); the renderer replays that stream against a `<picture>` grouping state carried **on the `Renderer`, across events** (`feed_html`/`picture_open`). Block HTML accumulates between `Tag::HtmlBlock` start/end (reliable delimiter) and is fed at the close; inline HTML feeds per event; an open group is flushed at `</picture>` and at its container's end (`TagEnd::HtmlBlock`/`Paragraph`) so a malformed/unclosed `<picture>` can't swallow later content. A `<picture>` = one image (first decodable candidate wins — `<source srcset>`s in order, then the `<img>` fallback; broken-image marker only if none decode); each **ungrouped** `<img>`/`<source>` renders independently. A fragment with no `<source>`/`<img>` renders nothing, so all other raw HTML (`<script>`, `<iframe>`, `<div>`) stays dropped. Only `srcset`/`src` are read — an `onerror=` handler is never executed, and every `src` still passes `resolve_image` — so this widens what is *rendered*, never what is *trusted*.
**Lesson**: when a renderer mirrors an HTML element's semantics, honour the element's **grouping/scoping**, not just the presence of the child tags — and remember the *same* logical construct reaches you as **either a block or inline events** depending on formatting the author didn't think about, so grouping state must live where it survives the event boundary (on the walker), not inside a per-event parse.
**See**: TDD 2.23; TECH.md § Rendering (the rich-images work — `<picture>`/`<img>` + WebP fallback — was retired into this entry + #146). Cross-refs #66/#75 (pulldown-cmark event quirks), #31/#32/#34 (the shared image path), #146 (WebP renders via `GdkTexture::from_file`'s own loader chain when the loader is registered — no manual fallback).

**Family**: parser event-stream blind spots — ScrAP-147 (element grouping vs child tags), ScrAP-158 (item emitted ≠ content produced), ScrAP-194 (a shared helper's RAW line hides the container prefix), ScrAP-195 (a second tokeniser's syntax is invisible to the first's events), ScrAP-196 (a symptom-keyed fallback swallows the next cause). Distinct mechanisms, one theme: **the event stream does not say what you assume** — kept whole rather than merged, because the case table that would hold them loses the mechanism that made each expensive.

## 148. Splicing at an offset mapped OUT of a delimiter-stripped coordinate space
> *Non-core (CriticMarkup/copymap offset-bookkeeping) — a coordinate-mapping lesson, not a GTK one. Do not fold into the gtk4-rs skill. Sibling of #66/#75 (offset/event bookkeeping); the same discipline as the CriticMarkup cleaned↔original shift-table math.*

**Symptom**: making a preview annotation from a selection that (a) spans more than one block AND (b) ends part-way through an existing `{==highlight==}{>>comment<<}` did **nothing the user could see** — no new comment chip appeared — and the reviewer's typed comment vanished. It *looked* like a silent no-op, but the source had actually been **corrupted**: the new `{>>comment<<}` was spliced into the middle of the existing construct (`{==fl{>>comment<<}at==}{>>cite<<}`), which the scanner re-reads as the comment being *part of the highlighted claim*. The `new == old` apply-guard never fired because the text genuinely changed — it just changed into garbage.

**Root cause**: **an offset translated out of a delimiter-stripped ("cleaned") coordinate space is safe to READ from but not safe to INSERT AT.** A cross-block selection's end is mapped cleaned→original through the shift table (`cleaned_to_original`), and any cleaned offset that falls within a kept highlight's *content* maps to a byte **strictly inside** the original `{==…==}` — the `{==` and `==}` were deleted in cleaned, so they are invisible to a coordinate that lives in cleaned space. Reading the character there is fine; splicing a *new delimited construct* there nests two constructs and breaks both. The trap is that the corruption is silent in three independent ways at once: the string-diff guard passes (text changed), the scanner still parses (it just parses the wrong thing), and the visible outcome ("nothing happened") is indistinguishable from the innocent empty-comment case.

**A false lead worth recording**: the two candidate mechanisms — "the resolver returns `None` so nothing is applied" vs. "the resolver returns a `Point` whose splice corrupts" — *present identically* as "nothing happened", and the first is the intuitive guess (it was the issue's leading hypothesis). A one-test probe at the pure boundary (`selection_target` → `insert_point_comment` → `extract`) disproved it: the resolver returns a perfectly valid `Point`; the anchor is the problem. **Prove which candidate is live before fixing — the plausible one was wrong.**

**Resolution**: **before splicing at an anchor that came out of a stripped space, snap it to a boundary the stripped space could not see.** A point comment must land cleanly *outside* any construct — it never extends one (extending is the intra-block highlight path's job; a cross-block selection was deliberately never wired to it). `point_comment_anchor(source, at)` re-extracts the *original* source and, if `at` falls strictly inside a construct's `src_span`, snaps it to that construct's END, so the comment attaches as its own standalone `{>>…<<}` immediately after the whole `{==…==}{>>…<<}`. Constructs never overlap, so the snapped end is a clean boundary and one pass suffices.

**Scribobulate**: `annotate::point_comment_anchor` (pure, unit-tested in `annotate/mutate.rs`), applied at the single commit choke point `window::annotate::apply_annotation_edit`'s `Point` arm — which **both** the preview sink and the editor Create card route through, so neither call site can forget the guard. The pure boundary regression lives in `preview/annotate.rs` (`multiblock_over_existing_annotation_point_lands_outside_the_construct`) and the snap unit tests in `annotate/mutate.rs`. TDD 17.44.

**Lesson**: **a coordinate is only as trustworthy as the space it was measured in.** An offset from a projection that *deleted* structure (a cleaned/stripped/normalised view) can be dereferenced for reading but must be re-validated against the *full* text before it is used as an insertion or deletion point — the deleted bytes are exactly the ones it cannot represent, and they are exactly the delimiters whose interior you must not split. Guard at the point where the two coordinate spaces meet, at the one choke point every writer shares, and pin it with a test that asserts the *re-extracted* result (two well-formed annotations), not merely that the string changed — because "the string changed" is precisely what a silent corruption also satisfies.
**See**: TDD 17.44 (the cross-block point-comment contract + the deliberate no-extend decision); #143 (an ANTI-PATTERNS entry must be self-contained — this one inlines the mechanism rather than citing the now-deleted issue that reported it).

## 149. Two overlapping async scroll-drivers over one adjustment, neither cancelled by a newer navigation
**Symptom**: Navigating the annotations viewer to scattered rows in quick succession — clicking (or arrowing to) several before each scroll settled — left the document **desynced**: it parked at an *earlier* target's position while a *different*, later-clicked row showed as selected. Intermittent by nature: it needed interaction faster than a scroll converges, so it survived every demo and single-step test and only surfaced under real fast use (it was the operator's "difficult to reproduce" bug).
**Scribobulate**: `codeview::CodePreviewView` `nav_generation`; bumped in `open_marker_popover_at`; checked in `converge_and_scroll_to_offset`'s tick and in `open_marker_popover`'s re-pin `value-changed` handler + disconnect-tick. TDD 20.16. Verified on the operator's live session (the timing behaviour is not observable headlessly — #56).
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-172).

## 150. A self-drawn decoration re-adding padding that the line's own tags already put inside its `line_yrange`
**Symptom**: In the rendered preview, a fenced code block's colored card **overlapped the text of the line immediately below it** — the following line's glyphs were painted over the card's bottom edge. Only reproduced in one narrow construct: a **loose (hard-broken) continuation paragraph wedged directly under a code block inside a nested list item** (e.g. a bold `**OR**` line between two fenced blocks of an ordered sub-list). Every ordinary code block looked perfect. Source and editor pane were unaffected — a preview-only overlap.
**Scribobulate**: `codeview::mod` `snapshot_layer(BelowText)` code-block + blockquote loops; `codeview::geometry::span_card_y_extent`; the `code-block-top`/`code-block-bottom` tags in `tags.rs`. The abutting-`\n` construct comes from `start.rs`'s loose-list-paragraph branch (one `newline()`, not `block_sep`).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-127).

## 151. Detecting a URL scheme with "the text before the first colon" (`split_once(':')`)
**Symptom**: A Markdown-referenced **local file whose path contains a colon** silently failed to render or navigate. Images (`![](assets/notes:v2.png)`, `![](C:\pics\x.png)`) drew the broken-image placeholder with alt text suppressed; document links (`[x](report:draft.md)`) went inert. The source and the file were perfectly valid; the reference was simply refused before it ever reached the filesystem. Absolute *or* relative, on Linux as well as Windows — any colon anywhere in the path triggered it.
**Scribobulate**: `links::scheme_of` (now the single source, shared by `is_allowed_url`, the doc-link gate, and `resolve_image` — the last had inlined its own `split_once(':')`).
**See**: general-engineering-principles (GEP-46).

## 152. A deferred idle closure that strong-captures a widget fires against it after teardown — and the reflexive guards each miss
**Symptom**: A `glib::idle_add_local_once` closure that defers a `GtkTextView` scroll (`scroll_to_mark` on the next idle, to let lazy line-height validation settle — the standard #22 deferral) captured the view with a strong `self.clone()`. If the view's window is destroyed in the single idle-priority tick between the scroll being *scheduled* and the idle *firing*, the idle still runs — against a widget whose toplevel is gone. Deterministic, reproduced 4/4 in-suite and again under Xvfb by a mutation test: a real scroll requested, then `window.destroy()` with no main-loop pump, then a later pump. The exact chain (GTK 4.6.9):
```
Gtk-WARNING: Calling gtk_widget_realize() on a widget that isn't inside a toplevel window …
Gdk-CRITICAL: gdk_surface_new_popup: assertion 'GDK_IS_SURFACE (parent)' failed
→ SIGSEGV (signal 11)
```
**Scribobulate**: `codeview::geometry`'s `scroll_to_buffer_offset` and `scroll_to_cell_offset` (both idles + the nested inner refine idle, all sharing one `scroll_idle` slot); the cancel lives in `CodePreviewView`'s `WidgetImpl::unrealize`. The file's tick-callbacks already avoided the pin by taking the view from the callback's emitter arg — an `idle_add_local_once` has no emitter arg, which is why these two needed the explicit `WeakRef`.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-128).

## 153. A `#[gtk::test]` integration suite renders on the default GskGLRenderer, not the renderer `main()` selects — and its GL texture cache SIGABRTs at teardown under a headless display
**Symptom**: The full `cargo test --features gtk-integration-tests` suite intermittently aborts (SIGABRT, exit 134) at **process teardown** under Xvfb — roughly 2–3 in 10–30 full runs; re-running passes, and every test passes in isolation. Historically also reported as a SIGSEGV, but that turned out to be a *separate* mechanism (#152, since fixed). The remaining abort's signature:
```
Gdk-CRITICAL: gdk_monitor_get_geometry: assertion 'GDK_IS_MONITOR (monitor)' failed
Gdk-CRITICAL: gdk_texture_new_for_surface: assertion 'cairo_image_surface_get_width (surface) > 0' failed
Gsk-CRITICAL: gsk_gl_driver_load_texture: assertion 'GDK_IS_TEXTURE (texture)' failed
Gsk:ERROR: ../../../gsk/gl/gskgldriver.c:713: gsk_gl_driver_cache_texture: assertion failed: (texture_id > 0)  → abort
```
**Scribobulate**: `.cargo/config.toml` `[env] GSK_RENDERER = "cairo"`, mirroring `main.rs`'s in-process override; documented in POLICY § Architecture rules. This entry retires a former known-issue register entry — it inlines the mechanism rather than citing that ephemeral ID (#143).
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-129).

## 154. Migrating a hand-rolled weak capture to `glib::clone!` is not a blind find/replace — its single hoisted upgrade changes behaviour at several site shapes
> *Core gtk-rs (glib::clone!). Candidate for the gtk4-rs skill's threading-async-and-memory module (weak-capture idiom; ScrAP-60 / ScrAP-152 kin). Verified glib 0.21 / gtk4-rs 0.10.*

**Symptom**: Converting the project's hand-rolled weak-capture idiom — `{ let x = obj.downgrade(); w.connect_sig(move |..| { if let Some(x) = x.upgrade() { BODY } }); }` — to `glib::clone!(#[weak(rename_to = x)] obj, move |..| { BODY })` across many sites. A whole-closure `#[weak]` is behaviour-preserving for the *common* case (a `()`-returning closure that did nothing when the widget was gone), but at a minority of sites it either fails to compile or silently changes behaviour.

**What was tried**: Treating all `downgrade()` sites as mechanically identical. `clone!` rewrites the closure so that a *single* upgrade-or-fallback runs at the very top: if any captured `#[weak]` fails to upgrade, the fallback value is returned immediately and **the body never runs**. That transformation is not equivalent to the hand-rolled form at every site.

**Root cause**: The hand-rolled form lets you place the upgrade guard *anywhere* and run arbitrary code outside it; `clone!` forces one guard at the top with one fallback. The mismatch bites in five recurring shapes:
1. **Non-GObject wrapper targets.** `#[weak]`/`#[strong]` require the target implement glib's `Downgrade`/`Upgrade`. GObjects (and `Rc`/`Arc` for `#[strong]`) qualify; a hand-rolled struct exposing its *own* `downgrade()` that returns a bespoke weak type (here `TabView` → `WeakTabView`) does **not** — `clone!(#[weak] tab_view, …)` fails with `trait bound TabView: Downgrade is not satisfied`.
2. **Unconditional work outside the guard.** Any statement that must run *even when the widget is gone* — a dialog `.destroy()` teardown, an `action.set_state(v)` that is window-independent, clearing a spent `SourceId` (`timer.replace(None)`), disconnecting a signal — is silently skipped, because the hoisted upgrade returns before reaching it.
3. **Per-branch return values.** A closure that returns different values on different branches when gone (e.g. `Stop` on Shift+Return, `Proceed` otherwise) can't be expressed by `clone!`'s single `#[upgrade_or]` fallback.
4. **Divergent multi-weak semantics.** Two `#[weak]` captures whose failures must be handled *differently* (one aborts, the other only skips a follow-up after the main work) collapse into one all-or-nothing gate.
5. **Nested / reused / non-sink captures.** A weak `.clone()`d into an inner `idle_add_local_once`/`timeout` where the actual upgrade happens (the outer closure never upgrades); a weak passed *as data* to a helper that upgrades internally; or a closure handed to a non-signal sink (`CallbackAction::new`, a stored `Rc<dyn Fn>`, a constructor/method argument) — none are the `downgrade→closure→upgrade` shape `clone!` targets.

**Resolution**: Convert only sites that are a single whole-closure gate. Pick the fallback to match the gone-path return: `()` doing-nothing → plain `#[weak]` (upgrade-or-`Default`); a specific value → `#[upgrade_or] <value>` (e.g. `glib::Propagation::Proceed`, `glib::ControlFlow::Break`); keep non-widget `Rc`/`Arc` as `#[strong]`; `#[weak(rename_to = x)]` preserves the body's variable name. Leave the five shapes above hand-rolled (or, for shape 1, add a `Downgrade`/`Upgrade` impl to the wrapper — a structural change). Bonus: the converted sites now log the gone-branch via `CLONE_MACRO_LOG_DOMAIN` (observability the hand-rolled form lacked).

**Lesson**: `glib::clone!` is the idiomatic replacement for the hand-rolled weak-capture idiom (ScrAP-60), but it encodes exactly one hoisted upgrade-or-fallback per closure — so before converting a site, check three things: (a) does the target implement glib's `Downgrade` (bespoke non-GObject weak wrappers don't)? (b) does any statement run unconditionally *outside* the upgrade guard? (c) is the gone-path a single return value, or does it diverge per-branch / per-capture? If any answer is "no / diverges," keep it hand-rolled. A weak-capture migration that assumes uniformity will compile at most sites and quietly break the few that aren't.

**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63 the self-capture cycle this enforces; GTK4Rs/AP-128 the deferred-idle teardown). #60 (the reference cycle these captures prevent), #152 (the deferred-idle instance).

## 155. A per-render widget whose `Rc` dismiss closure strong-captures its own container, while controllers on that container hold the `Rc` — an uncollectable cycle that strands the subtree every rebuild (unbounded reload leak); plus naming a GTK-internal allocator leak with no debug symbols

**Symptom**: Repeatedly rebuilding a piece of UI (here: the preview rebuilt on every
live-reload) grows process RSS monotonically and roughly linearly — a fixed ~200–350 KiB
per rebuild, **content-independent**, never released across idle settles. No CRITICAL, no
leak-warning. Weak-ref probes on the *obvious* owned objects (the rebuilt view, its buffer,
the wrapping overlay/scroller/adjustment) all show correct finalization (1/N), so the leak
looks "GTK-internal" and un-actionable.

**What was tried**:
- `massif` named the growing retainer as a libglib `GHashTable` (`g_hash_table_insert`)
  surviving every app object's finalize → looked like a toolkit-internal leak.
- Getting GTK/glib debug symbols to name the frame: `apt install …-dbgsym` — the distro's
  ddebs only carried the *release* build (4.6.2), not the installed *update* (4.6.9);
  `debuginfod.ubuntu.com` — connects fine (DNS+TCP+TLS <1 s) but **never delivers a payload**
  for the 4.6.9 build-ids, stringing the client along ~72 min/attempt with a progress
  spinner and **0 bytes written**. BOTH official symbol sources are dead ends for a
  jammy-updates library. (Don't repeat either — see Lesson 3.)
- Disabling the IBus IM module (a classic content-independent per-widget leaker) — no effect.

**Root cause**: An `Rc<dyn Fn()>` "hide/dismiss" helper strong-captured the very widget it
hides (`let w = w.clone()` instead of `w.downgrade()`). That same `Rc` was then captured by
handler closures owned by event controllers/gestures added to *that same widget* (a focus
`connect_leave`; an Escape `GtkEventControllerKey`). The result is a reference cycle that
never crosses a weak edge:
`widget → (add_controller) controller → handler closure → Rc closure → widget`.
GTK object refcounts never reach zero, so when the parent subtree is replaced on the next
render the widget — and its entire internal machinery (for a `GtkEntry`: the inner
`GtkText`'s `GtkGestureClick` / `GtkGestureDrag` / `GtkEventControllerKey`, each owning a
`GHashTable`) — is stranded. A fixed set of these widgets is built per render regardless of
document content, so the leak is a fixed per-render constant — exactly the observed
content-independent ~200 KiB.

**Resolution**: Make the dismiss closure capture its container **weakly** (`downgrade()` +
an `upgrade()`-guarded body; a no-op once the widget is gone is precisely "nothing to
hide"). One weak edge breaks *every* cycle routed through the shared `Rc`. Proof without any
debug symbols: an `LD_PRELOAD` interposer over `g_hash_table_new*` / `ref` / `unref` /
`destroy`, plus `g_object_new*` (variadic `g_object_new` forwarded through
`g_object_new_valist`) and `g_type_create_instance`, tags every table with the **GType**
under construction and records its creation backtrace; return addresses resolve to
`lib+offset` via `dladdr` and app frames via `addr2line` on the unstripped debug binary. It
named `GtkGestureClick`/`GtkGestureDrag`/`GtkEventControllerKey` with **`destroyed=0`**,
scaling one-set-per-render (10 reloads→10, 40→40), and pointed straight at the app callsite.
After the fix the same interposer shows `created==destroyed`, LIVE=0, and the RSS slope
drops from **+246 KiB/reload to ~0**. Guard it with a deterministic test: capture a weak ref
to the per-render widget, drive a few reloads, assert it finalizes — while holding **no**
strong ref to the old subtree yourself (a `sw`/`overlay` local kept alive masks the leak and
gives a false failure).

**Lesson**:
1. A dismiss/close/toggle helper that lives in an `Rc` and is invoked by controllers *on the
   widget it acts upon* must capture that widget **weakly**. Otherwise it closes a
   widget↔controller↔`Rc` cycle that no `WeakRef` guards and no `dispose` unwinds; finalize
   never runs. (Cf. ScrAP-60 the weak-capture idiom this enforces; ScrAP-51 / ScrAP-152 the
   widget-owned-closure leak family.)
2. "The owned objects all finalize, so the leak must be toolkit-internal" is a **false**
   conclusion when a *sibling* object in the same rebuilt subtree is the one stranded. Probe
   *every* widget in the rebuilt subtree (including in-overlay cards / entries), not just its
   root, before blaming the toolkit.
3. When distro debug symbols are unavailable — an `-updates` library whose `dbgsym` and
   `debuginfod` **both** miss (confirmed for Ubuntu jammy libgtk-4 / libglib **4.6.9**) — you
   don't need them. An `LD_PRELOAD` interposer that tags allocations by GType + creation
   backtrace and resolves via `dladdr`/`addr2line` names a GTK-internal allocator leak by
   type and callsite. Cross-run signatures aren't comparable (ASLR randomizes absolute
   frames) — key on `lib+offset`, and confirm a leak by *scaling* the reload count, not by
   absolute counts (one-time init noise like a `GtkSourceLanguage` manager loading ~169
   `.lang` specs is flat across counts; the real leak grows with reloads).

**See**: gtk4-rs skill → threading-async-and-memory — this lesson is encoded there as
**GTK4Rs/AP-140** (the shared-`Rc` dismiss-closure cycle) and the symbol-free
leak-attribution method as **GTK4Rs/AP-141** (placed as the mirror of the skill's GTK4Rs/AP-59:
"no frame is yours → ignore" vs. "it IS yours but symbol-less → name it this way"). Kin:
GTK4Rs/AP-60 the weak-capture idiom this enforces; ScrAP-152 / GTK4Rs/AP-51 the
widget-owned-closure teardown family.

## 156. Reading a `GtkTextView` selection's anchor y from a wall-clock debounce after a scroll — the read lands before validation, so an on-viewport selection is suppressed

**Symptom**: A popover that a preview text selection is supposed to raise (here: the
"Annotate" action popover over a preview selection) **fails to appear on the first
selection after a scroll**. Reliable repro: scroll the document, then select some text →
no popover. Clear the selection and reselect *without* scrolling → it appears. The failure
looks intermittent at first (a preceding save or annotation-add "sometimes" triggers it),
but the underlying trigger is any scroll/re-render that moved the viewport just before the
select.

**What was tried**:
- Suspecting the on-viewport bounds guard was mis-computing → it was correct; the *inputs*
  were stale.
- Suspecting the selection signal never fired → it did; the popover decision ran and chose
  to *hide*.
- Hypothesis (refuted by the researcher against gtk-4.6.9 source): "the first
  `iter_location` read forces validation of that line, which is why the *second* selection
  works." **False** — `gtk_text_view_get_iter_location` → `get_line_display(…, FALSE)` builds
  a Pango display but does **not** write the btree line height, does **not** set the valid
  flag, and does **not** emit the layout `CHANGED` that drives the yoffset correction. A
  forced read would NOT have fixed it — it would have shipped a flake.
- **Moving the read onto a bounded, stability-checked `add_tick_callback`** (re-read
  widget-y each frame, act on two equal frames / an 8-frame cap) — **insufficient,
  live-confirmed broken** on the operator's real KDE/X11 with a large (400-section) doc
  (GTK4Rs/AP-56). A `GtkFrameClock` tick fires at the **UPDATE** phase, which runs **before**
  paint — so two consecutive ticks sample the *same pre-validation estimate* and "converge"
  stably-**wrong**, hiding a genuinely-visible selection. Passed headless (GTK4Rs/AP-78 masks
  the timing). Retracted researcher tip: reading `get_visible_rect().y` instead does **not**
  help — it returns `priv->yoffset` **verbatim**, the same lagging offset, so it is
  algebraically identical to the widget-y check.

**Root cause**: **Only a PAINT validates `GtkTextView` geometry.** Every read API —
`iter_location`, `buffer_to_window_coords`, `get_visible_rect`, `get_line_at_y` — returns a
**lagging `priv->yoffset`** plus a **cached, possibly-estimated `find_line_top`** with *zero*
validation side-effect; `gtk_text_view_paint` is the only path that runs
`flush_first_validate` and asserts `onscreen_validated`, then draws each line at
`find_line_top − yoffset` (the identical transform the reads use). So a read agrees with the
painted position **iff it happens after a paint of the current scroll state**; before that,
after a scroll into a not-yet-painted band, `widget_y(selection)` is an estimate that can
land off-viewport, and the on-viewport guard hides a selection that is really on screen. The
un-scrolled reselect works only because a paint (the first selection's `queue_draw`/`popdown`,
gesture churn) validated the band in between. Both the original **wall-clock timeout** and the
**bare tick callback** read too early (timeout: before any paint; tick: at UPDATE, before
paint) — the same defect at two stages.

**Resolution**: Two independent halves. (1) **The decision is SHOW, not a pixel gate.** A
pointer-drag selection is on-screen *by construction*, and a keyboard caret the view keeps
visible; the only legitimate HIDE is a selection scrolled **away**, which the scroll
handler's `value-changed` → `popdown` already covers. So a fresh selection never gets an
absolute-pixel *hide* gate. (2) **Read geometry only where it is validated — the frame
clock's `after-paint` phase.** `queue_draw()` to guarantee a paint, then a **one-shot**
`widget.frame_clock().connect_after_paint(…)` that reads the anchor (now validated),
positions, and pops up — disconnecting on its first fire (one validated frame is enough; no
stability race, no cap to tune). A generation token cancels if a newer selection supersedes
before the paint; the one in-flight handler is disconnected on re-arm so a never-firing one
can't dangle; nothing strong-captures the view (GTK4Rs/AP-63). Fallback: if the view has no
frame clock (unmapped), attempt the guarded direct decision (the realize gate no-ops it).
**Live-verified** on the operator's real KDE/X11 (kwin, GSK cairo): on the first selection
after a scroll the after-paint handler fires once and the on-viewport read succeeds → the
popover shows and stays (the exact case the bare-tick fix left blank). Diagnostic aside: a
synthetic pointer-drag that auto-scrolls trips the scroll handler's `popdown`, so verify
with a selection that doesn't reach the auto-scroll margin, or the correct fix looks like a
failure.

**Lesson**: A `GtkTextView` geometry read (`iter_location`, `buffer_to_window_coords`,
`get_visible_rect`, `line_yrange`) is only trustworthy **after a paint of the current scroll
state** — the reads never force validation, only paint does. So when a read gates a visible
decision (place/show a popover or overlay after a scroll), take it in the frame clock's
**`after-paint`** phase (`queue_draw` + one-shot `connect_after_paint`), **not** a wall-clock
timeout (fires before any paint) and **not** a bare `add_tick_callback` (fires at UPDATE,
*before* paint — two ticks read the same stale estimate and converge wrong). And don't reach
for `get_visible_rect` as a "more authoritative" offset — it *is* `priv->yoffset` verbatim.
Better still, sidestep the read: for a decision that is really "is this on-screen selection
worth showing," a pointer-anchored surface is on-screen by construction — default to SHOW and
keep suppression only on the scroll path. Testing corollary: this stale-read *timing* cannot
be reproduced deterministically headless (GTK4Rs/AP-78) — settling the scroll to pick an
on-viewport selection also validates it — so the headless test guards *delivery* and the
operator's **live display is the real gate** (GTK4Rs/AP-56); the bare-tick fix passed headless
and still shipped broken until the live run. Researcher-confirmed against gtk-4.6.9
(`gtktextview.c` paint/`flush_first_validate` :5803-5817, `get_visible_rect` :3116-3120;
frame-clock UPDATE-before-paint ordering).

**See**: gtk4-rs skill → **GTK4Rs/AP-142** (textview-layout-and-drawing) encodes the core
lesson and adds a bidirectional correction to **GTK4Rs/AP-97**, whose "bounded
`add_tick_callback` poll" prescription is unsafe for a *validated geometry* read (a tick
samples pre-paint) — the distinction being that GTK4Rs/AP-97's paint-written MAP doesn't exist until
painted (safe to tick-poll) whereas a geometry NUMBER is always present but pre-paint. The
synthetic-drag auto-scroll false-negative in the Resolution aside is **GTK4Rs/AP-144**
(ui-testing-interaction).

## 157. Collapsing a large `GtkTreeListModel` while the `GtkListView` is scrolled to the bottom strands a stale far-end row

**Symptom**: "Collapse all" on a deeply nested outline (observed on a 310-heading document:
one `#`, twenty `##`, 289 `###`) leaves the outline showing **exactly one wrong row — the
document's *last* heading**, a leaf rendered with no expander, from which the tree cannot be
re-opened. The expected result is the single depth-0 root row.

**What was tried**:
- Suspecting the classic "collapsing shifts flat positions under a positional walk" hazard →
  ruled out: the collapse-all collects every depth-0 row into a `Vec` *first*, then calls
  `set_expanded(false)`, so no index shifts under the walk.
- Asserting the **model** state after collapse in a `#[gtk::test]` at the exact 310-heading
  scale → the model collapses **correctly** to a single root row (`[doc_index 0]`). So the
  bug is not in the model mutation, and a model-only assertion passes while the display is
  wrong (GTK4Rs/AP-78 shape).
- Suspecting the deferred scroll-spy/selection re-sync that runs on the collapse's
  `items-changed` → ruled out: the stale far-end row is present **immediately** after the
  collapse, before any deferred spy runs.

**Root cause**: `GtkListView` keeps a scroll-stability **anchor** row across a model change,
chosen from the scroll position at `items-changed` time. When the outline is scrolled to the
bottom, that anchor is a deep `###` row. Collapsing the single root **destroys** that deep
anchor row (an `autoexpand=false` `GtkTreeListModel` frees a collapsed node's whole
descendant subtree — ScrAP-84), and GtkListView 4.6 does not recover from losing
its anchor to the removal: it strands the stale far-end leaf widget materialised (the
scroller's vadjustment auto-resets to 0, but the wrong row stays painted). A headless probe
walking the ListView's materialised child rows confirms it shows the last heading while the
model contains only the root.

**Resolution**: Re-anchor the ListView to the **top** before collapsing — reset the outline
scroller's vadjustment to 0 (`vadjustment().set_value(0.0)`) at the start of collapse-all, so
the anchor is the surviving root row and the collapse removes only rows *below* it. Measured
synchronous enough that the collapse in the same turn reads the reset position. `scroll_to`
(GtkListView, 4.12+) would be the direct API but is unavailable on the 4.6 target
(GTK4Rs/AP-114 — an above-target wrapper compiles and fails at runtime), so drive the
adjustment directly. Guard with a gtk-integration test that *scrolls the realized ListView
to the bottom* before collapsing and asserts the materialised rows (not just the model)
contain no far-end leaf; deleting the reset line reproduces the stale row.

**Lesson**: Removing the row a `GtkListView` is currently anchored on — via a model
collapse/refresh that destroys the scrolled-to region — can strand a stale materialised row
that no longer exists in the model (the model is correct; the *view* is wrong). Assert the
**materialised rows**, not only the model, or the bug hides (GTK4Rs/AP-78 / GTK4Rs/AP-56). Before a
bulk model shrink that may destroy the current anchor, **re-anchor the view to a row that
will survive** (reset the adjustment to the top for a collapse-to-root). Core-GTK
`GtkListView` behaviour, not project-specific.

(model-level collapse-all).
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-143; kin GTK4Rs/AP-111).

## 158. A content-less list item still emits a full item (and task marker) — an unconditional per-item gutter decoration draws a stray marker
> *Non-core (pulldown-cmark/CommonMark) — parser event-stream behaviour + rendering logic, not a GTK lesson. Do not fold into the gtk4-rs skill. Sibling of #66/#75/#147 (pulldown event quirks).*

**Symptom**: an empty task-list item `- [ ]` on its own line drew a checkbox in the preview gutter despite having no content. The same shape affected the other kinds: a content-less bullet (`- `) or number (`1. `) also recorded a marker the gutter would draw (a lone `- ` in an otherwise empty document paints a stray dot on the empty line). Markers should render only when the item has content.

**Root cause**: two compounding facts, both non-obvious.
- The renderer pushed a `ListMarker` at **every** `Tag::Item`, unconditionally — nothing gated on the item actually producing content. pulldown-cmark emits `Start(Item)` … `End(Item)` for a content-less item too, so an empty item recorded a marker, and the gutter draw (which iterates every recorded marker whose first line is on-screen) drew it.
- Two pulldown-cmark task-marker quirks sit underneath: `- [ ]` **without** a trailing newline or space is **not** a task at all — pulldown emits literal `Text("[")`, `Text(" ")`, `Text("]")`; only `- [ ]\n` (or `- [ ] `) emits `TaskListMarker(false)`. And that `TaskListMarker` fires for a content-less task item, upgrading the empty item's marker to a `Task` checkbox.

**Scribobulate**: at `TagEnd::Item`, treat the item as empty when the walk inserted **no buffer content** for it (`end_offset() == item_start`) and drop the marker it pushed. The empty item's marker is guaranteed to be `list_markers.last()` — an empty item inserts no text, so it can hold no *surviving* nested item's marker after its own (a non-empty descendant would have advanced the offset; an empty descendant already dropped its own) — guarded by `first_line == item_start`. The ordered counter still advances across a dropped empty item (source-faithful). Verified headlessly: the gutter draws only recorded markers, so `list_markers.is_empty()` for each empty variant is sufficient proof (no live display needed).

**Lesson**: a per-item — or per-block — decoration must gate on the item having produced content, not on the parser having emitted an item. CommonMark parsers emit a complete item (and even a task marker) for a content-less list item, so "the parser didn't give me an item" is the wrong emptiness test; "the render walk produced no output for it" is the right one. And a task marker is doubly a trap: the *same* `- [ ]` source is literal text or a `TaskListMarker` depending only on trailing whitespace the author didn't think about.

**See**: TDD 2.4b (empty items draw no marker); renderer `TagEnd::Item`; sibling pulldown quirks #66/#75/#147.

**Family**: parser event-stream blind spots — ScrAP-147 (element grouping vs child tags), ScrAP-158 (item emitted ≠ content produced), ScrAP-194 (a shared helper's RAW line hides the container prefix), ScrAP-195 (a second tokeniser's syntax is invisible to the first's events), ScrAP-196 (a symptom-keyed fallback swallows the next cause). Distinct mechanisms, one theme: **the event stream does not say what you assume** — kept whole rather than merged, because the case table that would hold them loses the mechanism that made each expensive.

## 159. Centering a gutter marker on `line_yrange`'s height centers it over ALL of a soft-wrapped item's rows, not the first
**Symptom**: shrinking the preview window until a list item soft-wrapped left the marker (numeric, bullet, or task checkbox) centered vertically against the *whole* multi-line item, floating to the middle row, instead of staying top-aligned on the item's first line. A single-line item looked correct; only wrapping exposed it. More pronounced after zooming in — larger rows make an item wrap at a wider window, and the vertical drift scales with row count.
**Scribobulate**: clamp the logical-line height to the first display row before centering: `h' = min(h, gap + single_line_h)`, where `single_line_h` is one row's text height from a fresh Pango layout in the view's own CSS-zoomed font (`view.create_pango_layout("0").pixel_size().1` — cache-free, zoom-correct for free, the same font the ordered numeral draws in) and `gap` is the item's first-line `pixels_above_lines` (`px(list_item_gap)`, one shared definition). A single-row item is byte-identical: its `line_yrange` height already equals `gap + single_line_h` exactly (same integers GTK laid the row out with; `pixels_inside_wrap = 0`), so the `min` is a no-op. Both the drawn marker and the checkbox hit-column derive from the clamped `(y, h)`, so they stay in lock-step. The clamp is a pure function unit-tested headlessly (wrapped 3-row item centers on row 1 at cy=118, not the whole-block midline 134); pixel confirmation on the live view is the final gate (ScrAP-56).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-145); gutter `first_display_line` / `draw_list_marker`; sibling marker-gutter lessons ScrAP-157/ScrAP-158; the `iter_location`-avoidance rationale is ScrAP-22.

## 160. syntect's bundled default syntax set has no TypeScript/TSX/TOML — a fence in one of those languages silently falls back to plain text and renders as one flat colour
> *Non-core (syntect). Not folded into the core GTK skill.*

**Symptom**: a ` ```typescript ` (also `tsx`, `toml`, `kotlin`, `swift`, `dart`) fenced code block in the preview shows as a flat, single-colour block — every token the same ink, i.e. "all gray" — while ` ```js `, ` ```rust `, ` ```python ` highlight normally. The Markdown source is correct; the info string is a standard, widely-recognised tag.

**Root cause**: syntect's `SyntaxSet::load_defaults_newlines()` (its bundled set, derived from Sublime's default packages) does **not** include a TypeScript grammar — nor TSX/TOML/Kotlin/Swift/Dart. The emitter (`renderer/emit.rs::insert_code_block`) resolves the fence via `ss.find_syntax_by_token(lang).unwrap_or_else(|| ss.find_syntax_plain_text())`. For an omitted language `find_syntax_by_token` returns `None`, so it lands on **plain text**, which assigns every line one scope → one foreground colour → a uniformly-coloured block. The fallback is **silent** — no warning, no error — so it is invisibly wrong for exactly the languages syntect happens to omit, and only for those. Verified empirically against **syntect 5.3.0**: tokens `typescript`/`ts`/`tsx`/`toml` → `None`; `javascript`/`js`/`rust`/`python` → `Some(_)`.

**Scribobulate**: build the engine's `SyntaxSet` from **`two_face::syntax::extra_newlines()`** instead of `SyntaxSet::load_defaults_newlines()` (`renderer::syntect()`). `two-face` embeds bat's vetted syntax dump, a **superset** of syntect's defaults: it keeps every bundled grammar (js/rust/python still resolve) and adds the omitted ones (`typescript`→"TypeScript", `ts`→"TypeScript", `tsx`→"TypeScriptReact", `toml`→"TOML"). Two feature traps: (1) use the **`_newlines`** variant — the emitter feeds lines *with* their trailing `\n` (matching the old `load_defaults_newlines`); the no-newline variant would mis-tokenise line-anchored patterns. (2) depend on `two-face` with `--no-default-features --features syntect-fancy`, **not** its default `syntect-onig` feature — the default drags in the `onig`/`onig_sys`/`cc` C toolchain and contradicts this crate's deliberate `regex-fancy` (pure-Rust) syntect choice (Cargo.toml). Guarded by GTK-free `renderer::syntax_coverage_tests` (every common fence token resolves to a non-plain-text grammar; a real TypeScript snippet yields >1 distinct foreground colour). Reverting to `load_defaults_newlines()` fails both.

**Lesson**: a highlight engine that resolves an unknown language by silently falling back to plain text turns "unsupported grammar" into "looks rendered but isn't" — the failure is invisible and per-language. When a code-fence highlighter looks monochrome for *one* language but not others, suspect the syntax set's coverage before the theme or the emitter. syntect's defaults are narrower than they appear (no TS/TSX/TOML/Kotlin/Swift/Dart); `two-face` (bat's assets) is the maintained way to close the gap without hand-sourcing `.sublime-syntax` files whose cross-syntax `include`s may not resolve. **Citations**: syntect 5.3.0 `dumps/` default syntax list (probed: TypeScript absent); `two-face` `syntax::extra_newlines` / feature matrix (`syntect-fancy` vs `syntect-onig`).

**Licensing/attribution obligation (don't skip this half)**: a crate that *bundles third-party asset data* (here `two-face` compiles bat's grammar definitions into the binary) silently pulls those assets' **own** upstream licenses into anything you distribute — not just the crate's crate-level license. `two-face` is MIT OR Apache-2.0, but the embedded grammars are MIT (majority) + Apache-2.0 + BSD-2-Clause (incl. FreeBSD variant) + BSD-3-Clause — all permissive, all **needs-attribution**, none copyleft/source-available. A distributed binary must therefore reproduce those copyright + license notices. `two-face` gives you the exact required text via `two_face::acknowledgement::listing().to_md()` (or the pinned `generated/acknowledgements_full.md`); Scribobulate ships it verbatim as `THIRD-PARTY-LICENSES.md` and surfaces a summary in About ▸ Credits (`add_credit_section`, plain text — a `<url>` there mis-parses as `mailto:`, ScrAP-40). General principle: whenever a dependency embeds data assets (syntaxes, themes, fonts, icons, dictionaries), audit the *assets'* licenses separately from the crate's, and prefer a crate that exposes its acknowledgement text programmatically. Confirmed against the `open-source-license` skill's compatibility matrix + binary-distribution checklist (all-permissive→Apache-2.0 outbound = compatible, no conflicts).

## 161. A CSS `margin-*` silently ADDS to a code-set `gtk_widget_set_margin_*` on the same axis — the stylesheet can never reduce the inset, so `margin: 0` still stops short of the edge
**Symptom**: restyling the tab strip so the ACTIVE tab's opaque background covers the strip's 1px bottom rule — the classic "the selected tab is part of the page below it" notebook idiom — the stylesheet gave dormant tabs a small bottom margin and the active tab `margin-bottom: 0`. Every tab still stopped several pixels clear of the strip's bottom edge and the rule ran unbroken beneath all of them, active included. The stylesheet looked like it was being ignored, but it wasn't: the *relative* 3px difference between dormant and active tabs was applied exactly as written. Only the absolute floor was wrong.
**Scribobulate**: delete the vertical widget margins from the handle's construction and express the whole vertical inset in CSS, with a comment at the deletion site recording *why* the axis is stylesheet-only. Horizontal margins deliberately stay in Rust — the strip's layout module measures handle widths for its hit-test and scroll arithmetic, so that axis wants its single supplier on the code side. The shipped design then needs no bottom inset at all: every tab reaches the strip's bottom edge and *continues* the baseline rule with its own 1px bottom border on the same pixel row, and the ACTIVE tab turns that border **transparent** (not `none`, which would shift its label 1px by shrinking the box) so its own fill covers the row and the rule breaks under it alone. Confirmed by sampling pixel columns: dormant border and bare-rail rule land on an identical row; the active tab's fill runs straight through it. Verified in BOTH theme variants — the `shade()`-derived rail/dormant/active ladder stays monotonically recessed→lifted in dark *and* light, which a `@theme_base_color` rail would not have (it is the brightest surface on a light theme). Two adjacent facts were established by the same measurement and are worth keeping: **(a)** CSS margins *are* honoured on children that a custom `LayoutManager` allocates by calling `size_allocate` directly with an explicit rectangle — the widget applies its own box arithmetic inside whatever rectangle it is handed, so a custom layout does not opt children out of the stylesheet; **(b)** GTK paints a node's **inset `box-shadow` before its children**, so a child with an opaque background can cover a rule its parent drew along an inner edge — which is what makes the broken-baseline idiom expressible in plain CSS on a custom widget with no notebook involved.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-149).

## 162. `GtkTextView` reading position drifts toward the top under repeated horizontal resize — the re-wrap re-validation clamp, and the one width-changing path with no re-anchor hook
**Symptom**: with the preview scrolled into the document, dragging the window's width smaller creeps the reading position **upward toward the start of the file**. The creep is **cumulative**: a single one-shot resize barely moves it, but an interactive drag (many incremental width steps) accumulates the drift. Auto-reload preserves the reading position perfectly across the same document; a bare resize does not — the question that opened the investigation was "why doesn't resize behave like reload?". Diagnostic tell: only **narrowing** drifts; **widening** holds exactly.
**Scribobulate**: the view already tracks the user's **settled top buffer LINE** continuously (maintained for zoom re-anchoring, ScrAP-65). On a genuine width change in the preview's `size_allocate`, re-anchor to that line through the existing **coalesced, deferred, weak-captured, `is_realized`-gated `scroll_to_mark`** path (the outline/find scroll path) — never a one-shot adjustment `set_value`, which the clamp would immediately re-reset. Two guards make it precise: (1) key on the **raw allocation width**, not the content column — zoom changes the margins (hence content width) at the *same* allocation width and owns its own restore, so keying on content would double-drive it; (2) skip the first allocation (no prior line to preserve, and it must not fight the initial fresh-render restore). Keying on the width alone also makes the hook **cause-agnostic**: the same re-anchor covers a split-pane divider drag and a sidebar toggle — *any* change to the preview's allocated width — not only a window resize (CAM rows 8/9 fall out of row 7's fix for free). A geometry event that changes only *height* (an `Automatic` h-scrollbar appearing) doesn't re-wrap, so it correctly triggers nothing. Because `scroll_to_buffer_offset` only *schedules* an idle, it is safe to call from the size-allocate path (ScrAP-22/29 — no synchronous validation there). The **reload path is hardened** in the same spirit: it now captures from the tracked reading **line** rather than the live vadjustment `value`, so a reload arriving mid-resize (during the transient clamp) can't re-anchor the fresh document to the top. When the old preview widget is replaced (Preview-mode reload), the old view's `unrealize` cancels the pending resize idle (ScrAP-152), and the deferred body's cross-buffer mark guard (ScrAP-104) covers the in-place (Split) path — so the resize↔reload interaction is crash-safe *because* both go through the tracked line.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-153); `sdd/CAM.md` Reading-Position Preservation CAM (row 7 — geometry change); the preview view's `size_allocate` raw-width re-anchor + `reanchor_to_reading_line`, its continuous `reading_line` tracker, and the reload capture that reads `reading_line()` not the live value; the deferred-work meta-pattern in this file ("Lazy height/layout validation" family and the "Preserve a reading position across a re-render" cheat-sheet row — now also *across a resize*).

## 163. Switching a `GtkLabel` to `set_markup` silently makes every interpolated string a Pango-markup injection/breakage surface — an un-escaped filename metacharacter renders the label EMPTY, with no crash
> *Non-core (Pango markup) — sibling of ScrAP-4/115/117. Not folded into the core GTK skill.*

**Symptom**: adding a coloured "⚠" deleted-backing badge to a tab required a per-glyph colour, which a plain-text `GtkLabel` (`set_label`) can't express — so the tab label was converted to Pango markup (`set_markup`) with the "⚠" wrapped in a `<span foreground="#e5a50a">`. The badge itself renders fine. The trap is elsewhere and latent: the label also interpolates the document's **filename**, and a filename may legitimately contain `&`, `<`, or `>` (e.g. `A&B.md`, `<draft>.md`). Under `set_markup` such a name is now parsed as markup — GTK emits a `Pango`/`Gtk-WARNING` ("Failed to set text from markup…") and the label renders **empty or truncated**, with **no crash and no compile error**. A one-line `set_label`→`set_markup` change quietly reclassified every runtime string in that label from inert text to markup.

**Root cause**: `gtk_label_set_markup` runs the string through `pango_parse_markup`, which treats `&`/`<`/`>` as entity/tag syntax. `set_label` does not — the two entry points look interchangeable (both "set the label's text") but have opposite escaping contracts. There is no type-level distinction (`&str` either way), so the compiler can't flag the un-escaped interpolation; the failure only appears at runtime, only for names containing a metacharacter, and only as a soft warning + a blank badge — exactly the "invisible, per-input, silent-fallback" failure shape this register keeps meeting (cf. ScrAP-160's silent syntax fallback).

**Scribobulate**: the tab strip's label is now markup, so the single funnel that composes it (`window/tabs/documents.rs::tab_display_markup`) escapes the filename with `glib::markup_escape_text` **before** interpolation, and the pure label formula (`winstate::decisions::tab_label_markup`) takes the **already-escaped** name plus a caller-supplied `warn_color` and only assembles the markup — so it stays display-free and unit-testable (it decides badge ORDERING — ⚠ before ⟳ before name before •, and "which yellow" is not its concern). The label widget starts life empty (`Label::new(Some(""))`) and is only ever populated through this one `set_markup` funnel, so there is no second, un-escaped write path. The badge colour is a fixed app constant (`#e5a50a`, Adwaita "yellow 5") rather than a reading-theme key, because the tab strip wears the desktop GTK theme, not the preview's reading theme (TECH "the reading theme is preview-only").

**Lesson**: `set_label` and `set_markup` are **not** drop-in swaps — converting a label to markup silently makes every interpolated runtime string an escaping obligation, and the penalty for forgetting is a **blank/garbled label + a soft warning**, never a crash, so a happy-path test with an ASCII filename passes while `A&B.md` breaks in the field. When a label must carry *any* styled fragment, funnel its construction through **one** builder, escape every non-literal fragment there with `glib::markup_escape_text`, and keep the styling colour/attributes as parameters so the string-assembly core stays pure. General rule for GTK text APIs: confirm whether a setter's contract is plain-text or markup before interpolating user- or filesystem-derived data into it. Pango-markup sibling of ScrAP-4 (`<a href>` in labels), ScrAP-115 (highlighting inside an existing markup string), and ScrAP-117 (same-string `set_markup` is a no-op).

**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-154); `winstate/decisions.rs::tab_label_markup` (pure, escaped-name + colour param) and its unit tests; `window/tabs/documents.rs::tab_display_markup` (the escaping funnel + badge colour constant); `widgets/tab/{view,bar}.rs::set_tab_markup`/`set_markup` (the single `set_markup` write path); TDD 15.22.

---

## 164. Committing a test fixture whose filename is itself the invalid input breaks checkout on other platforms
**Symptom**: a committed regression fixture, `tests/fixtures/report:draft.md`, was deliberately named with a colon to exercise ScrAP-151 (a colon in a path is a local file, not a URL scheme) — a link to it in `doc-links.md` was clicked in a manual test to prove it *navigates*, not gets refused as a scheme. On Linux everything worked. On **Windows** the whole repository became **un-cloneable**: `git clone`/checkout fails to write the working-tree file because NTFS reads `:` as the separator for an alternate data stream, so the name is illegal. The failure is at checkout, before a single line of app code — or of the test that fixture serves — ever runs.
**Scribobulate**: `src/links.rs` tests (`scheme_of`/`is_allowed_url` string literals = the cross-platform guard; the `#[cfg(unix)]` `resolve_image` colon temp-file test = the run-time-file precedent); TDD §19.7a (now marked unit-verified, fixture-free); `tests/fixtures/doc-links.md` + `tests/MANUAL-TEST.md` (colon case removed / reframed).
**See**: general-engineering-principles (GEP-29).

## 165. Clearing an env var the wrong way gives a false confirmation
**Symptom**: gvsbuild's `gettext` step failed with `'create-lists.bat' is not recognized as an internal or external command`, repeated, then `create-lists-msvc.mak(94) : fatal error U1052: file 'gettext-runtime-objs.mak' not found`. It reads unmistakably as a corrupt or incomplete source tree. The file was present the whole time. `vswhere.exe` failed the same way in the same run.
**Scribobulate**: `packaging/windows/README.md` ("Two things that will bite you"), which states the non-working alternative explicitly so nobody re-derives it.
**See**: general-engineering-principles (GEP-30).

## 166. Never diagnose a hung test suite from a parallel run
**Symptom**: `cargo test --features gtk-integration-tests` never completed on Windows. Killed after ~10 minutes, it had printed **no test names at all** and emitted a flood of one warning. "Zero test names" was reported as evidence that it hung *at or before the first test*, and both a reviewing agent and this one reasoned from that: the leading hypothesis became a main-loop or renderer stall during startup, and a plausible, well-argued case was built on it.
**Scribobulate**: `scripts/pipeline.steps` carries `--test-threads=1` on `cmd.windows integration`, with the reason recorded at the step — a serialised run prints test names as it goes, so a wedge names the body it wedged on instead of producing the silence that invited the wrong diagnosis. Scoped to Windows, which is where a wedging GTK suite is most likely; `#[gtktest::test]` already serialises GTK bodies, so no platform needs it to pass.
> *This line previously read "`sdd/ISSUES.md` entry P". That issue was fixed and deleted, as issues are meant to be, and the pointer dangled from that moment — SDD principle 6's exact failure, committed inside the register that exists to hold what outlives an issue. Replaced with the durable mitigation rather than a second ephemeral pointer.*
**See**: general-engineering-principles (GEP-31).

## 167. An `Option`-returning lookup whose `None` is also a legitimate answer will fail silently forever
**Symptom**: on Windows the app never saved or restored window geometry, tabs or view state, and never read the user's `config.toml` or their theme overrides. Every launch started from defaults. There was no error, no log line, and nothing in the UI. It survived an entire platform port — build, packaging, installer, CI — unnoticed, and was found only by chasing an unrelated question about window geometry.
**Scribobulate**: `src/config.rs` (`config_home_fallback`), `src/session.rs` (`state_home_fallback`, `state_directory_resolves_without_any_xdg_override`); `sdd/TECH.md` § platform notes.
**See**: general-engineering-principles (GEP-42).

## 168. A popover's layout pass resizes the TOPLEVEL — from GTK's stale remembered size — collapsing a natively-maximized window
**Symptom**: maximize the window with the **title bar's own** maximize button, then open any popover — a menubar menu, a `GtkDropDown`, a right-click `GtkPopoverMenu` — and the window snaps down to whatever size it had before it was maximized. It does **not** come back when the popover closes, the title bar still shows the *restore* glyph, `IsZoomed` still returns true, and the screen keeps a band of stale pixels where the window used to be. Measured: 4112×2128 → **1872×1052 with `WS_MAXIMIZE` still set**. Typing, clicking and every other interaction leave the geometry alone — only a popover does it, which makes it read as a menu bug and sends you into the menubar code, where there is nothing wrong.
**Scribobulate**: `platform::win32::track_maximized_size` — while the window is maximized, and only then, keep GTK's remembered size equal to the size the OS actually gave it (`surface`'s `layout` signal → `set_default_size`). The fallback path then computes the size the window already has, `needs_resize` stays false, and **no resize happens at all**: no clamp, no `present`, no flicker, one comparison per layout pass. Nothing restores the remembered size on unmaximize — `should_remember_size()` becomes true the moment the state clears, so GTK resumes maintaining it from the first layout pass afterwards, and the *OS* (`WINDOWPLACEMENT`), not GTK, holds the rectangle a restore returns to. That last fact is why this is safe on Win32 and would **not** be on X11/Wayland, where GTK's remembered size *is* the restore target — hence `#[cfg(windows)]`, not a portable "fix". The decision core is the pure `remembered_size_while_maximized`, unit-tested for both directions (the inverted form — writing while *un*maximized — is the real regression risk: it would make restore land on the maximized size).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-158); TDD 7.0d / MANUAL-TEST 7.0d (the running-window check, incl. the unmaximized control); POLICY § Architecture rules (why the native frame was adopted, and that `src/platform/win32/frame.rs` also owns native-frame repairs needing no OS call). Sibling of ScrAP-167 — both are the same porting shape: something the toolkit did for us until the platform's own conventions took the job.

## 169. A pruned `-symbolic` icon name degrades to a legacy raster instead of failing — and `has_icon` is *stricter* than the render path, so the audit scores it green
**Symptom**: on a host carrying a recent Adwaita (first measured on GTK 4.22.4 / adwaita-icon-theme 50) a batch of toolbar/menu icons rendered the broken-image placeholder. An audit over the app's whole icon-name table using `IconTheme::has_icon` reported 16 names missing. Installing the icon theme the platform lacked closed 14 of them, leaving 2 — but the *running app* only ever showed one placeholder. One name (`emblem-synchronizing-symbolic`) was reported missing by `has_icon` while its button rendered perfectly normal art.
**Scribobulate**: `tests/icon_resolution.rs` (the render-and-report audit, plus `--render`); `data/resources.gresource.xml` + `data/icons/scalable/emblems/` (the bundled replacements); `src/icons.rs` (the name table the audit walks).
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-174).

## 170. Symbolic icon art drawn with strokes silently changes shape — the SVG rasterizer you preview in is not the renderer that ships it
**Symptom**: while drawing replacement `*-symbolic` icons, the working assumption was the usual one — GTK recolours symbolic icons, so a stroked outline would come out in the wrong colour and vanish on a dark theme. Wrong in a more interesting way: previewing the art through a plain SVG rasterizer and through GTK's own icon pipeline produced **different geometry** from the identical file.
**Scribobulate**: `data/icons/scalable/emblems/*.svg` (both bundled icons are fills-only, with the mechanism recorded in each file's header); `tests/icon_resolution.rs --render` (renders through the shipping path).
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-170).

## 171. Every `#[gtk::test]` aborts on macOS before its body runs — the harness dispatches onto a worker thread, and GTK there requires the main one
**Symptom**: running the GTK integration suite on macOS, every test fails immediately with `assertion left != right failed: Attempted to initialize GTK on OSX from non-main thread`, followed by `GTK has not been initialized. Call gtk::init first.` from whatever the body touched. No test body executes. The same suite is fully green on Linux under Xvfb.
**Scribobulate**: `tests/icon_resolution.rs` + its `[[test]] harness = false` declaration in `Cargo.toml` (the main-thread gate, plus the `#[path]` sharing trick); `src/icons.rs`, whose own `#[gtk::test]` was retired as redundant once one target covered both platforms.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-159).

## 172. A synthesized-click UI-automation tool can be silently broken, making a real bug look unfixable across several attempts
**Symptom**: reproducing and then fixing a "clicking this button does nothing" bug, using an OS-level UI-automation tool (the one to hand — `xdotool`, `osascript`/System Events, AutoHotkey are all the same shape) to both reproduce the click and verify each fix attempt, since no human was driving the mouse in that session. Two different, independently-reasoned code changes were applied in turn; both were reported as "no change — button still doesn't respond" by the automation. The bug read as unusually stubborn.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-12).

## 173. Freezing a drag icon with `current_image()` AFTER dimming the source widget captures a blank — `queue_draw` has already cleared the render node
**Symptom**: a tab drag works correctly end to end — the drop lands, the tab rehomes, no warning is logged, nothing errors — but **no drag icon follows the pointer**. The only sign a drag is in flight is the source widget's own dimming, which is easy to miss. Because the feature "works", the defect reads as a platform/backend deficiency: it was first filed as a suspected macOS/Quartz gap, on the reasoning that GTK's *default* drag-icon synthesis might simply be unimplemented there.
**Scribobulate**: `src/widgets/tab/bar.rs` (`begin_drag_visuals`, the fused capture-then-dim, plus the `drag_icon_freeze_must_be_taken_before_the_handle_is_dimmed` regression guard — which asserts the correct order yields a render node *and*, as a deliberate mutation, that the wrong order yields none, so a refactor cannot make it vacuous); `src/widgets/tab/view.rs` (the delegating wrapper); `src/window/tabs/dnd.rs` (`connect_drag_begin`, now one call). Contract: **TDD 7.9a** and its MANUAL-TEST counterpart — added because 7.9 covered only the drop-target highlight, so nothing asserted the drag icon itself and the bug had no rubric to fail.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-156).

## 174. A single-instance guarantee that lives in a backend, not an API, fails silently where the backend is absent — and the platform's *other* launch path will falsely confirm it works
**Symptom**: on macOS every launch of the app spawned an independent process with its own window — two live-reload monitors on one file, two windows that could each save over the other — while the identical code single-instanced correctly on Linux. Nothing errored. `GApplication` was built with `HANDLES_OPEN`, `g_application_register()` succeeded, and `is_remote()` reported false, which is *exactly* what a legitimate first launch looks like.
**Scribobulate**: `src/platform/mac/single_instance.rs` (whole module, `#[cfg]`-gated at its declaration in `platform/mod.rs` per the `src/platform/win32/` precedent); `scribobulate::run` in `src/lib.rs` (`elect` between `setup_app` and `run_with_args`, skipped under `--new-instance`; it sat in `src/main.rs` until the lib/bin split reduced that file to argument-free delegation). Contract: **TDD 8.1/8.2/8.5** unchanged — the point is that the *behaviour* did not need a platform clause — plus **TDD 8.7** for the crash-recovery property the new primitive introduces, and MANUAL-TEST **8.2m**, which forces both macOS launch paths to be exercised separately.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-157).

## 175. A defect whose CONSEQUENCE is platform-dependent while the defect itself is not — the platform that never triggers it never tests for it, and a guard written on the triggering platform's symptom is permanently green where the bug actually lives
**Symptom**: a tab drag-and-drop integration test hung forever on Windows and on macOS — 28 MB of identical `Gtk-WARNING` in 30 s on GDK-Win32, 12.1 GB / 274 million lines in five minutes on Quartz — and **passed in 0.50 s on Linux**. Two 4.22.4 backends wedged and 4.6.9 did not, so the whole investigation was framed as "what does GTK 4.22 do that 4.6 doesn't", across three seats, for hours. It was tracked as a Windows issue and a macOS issue. The Linux suite was 624 tests green and was twice cited — by the Linux side itself — as cross-platform reassurance.
**Scribobulate**: `src/window/tabs/dnd.rs` — the `detach_overlay_from` call in `move_tab_to_new_window`, and `move_tab_to_new_window_detaches_the_source_windows_format_overlay` (the guard, with the "green for the wrong reason" argument in its doc comment).
**See**: general-engineering-principles (GEP-16).

## 180. A `set_parent`'d child left on a `GtkTextView` at dispose is an INFINITE loop, not a warning — and the suite that stayed green was the one never disposing anything
**Symptom**: one test body in the GTK suite never returned. Not slow — unbounded: **274,758,040** lines of `Gtk-WARNING **: GtkPopover is not a child of GtkSourceView` and **12.1 GB** of stderr in about five minutes, killed by hand before it filled the disk. A `sample` of the process put 100% of samples in one stack, all inside teardown:
**Scribobulate**: `src/saferizer/persistent_popover.rs` (`teardown` = popdown → unparent, the correct shape) and its callers in `src/window/mod.rs`, `src/window/tabs/switch.rs`. **No runner-side guard exists in this branch**: the wall-clock watchdog and log-volume cap that bounded this were part of a GTK test harness that was built, verified, and then reverted on proportionality grounds — so the numbers above are the only record of what an unbounded case costs, which is why they are quoted here rather than cited.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-80).

## 181. A suite that has never RUN on a platform is full of assertions that only look portable
**Symptom**: the first execution of a GTK integration suite on a platform it had never run on (macOS, where the binding's own test attribute dispatches bodies onto a worker thread that GTK there forbids) failed 7 of 134 bodies. Six were nothing to do with macOS *behaviour* — they were assertions that had quietly encoded the Linux environment, and had been green for as long as they had only ever been run there. Three reported "the document never opened", which points at the feature under test and not at the fixture.
**Scribobulate**: neither fix lives in this branch. Both are platform-neutral — a `canonical_tempdir` helper for the temp-dir case, a `#[cfg]`'d `PRIMARY_LABEL` constant for the accelerator one — and were conveyed to the shared branch rather than carried here, per the rule that a platform branch holds no platform-neutral change. The full form of each is in this entry deliberately, so the lesson does not depend on that handover having happened.
**See**: general-engineering-principles (GEP-18).

## 182. A readiness probe stronger than the behaviour it gates fails on its own terms — `has_focus()` needs an ACTIVE toplevel, `notify::focus-widget` does not

**Symptom**: one GTK test failed on macOS with `timed out waiting for: editor to take focus` after the full 5 s — but only sometimes. Three failures out of three in isolation, a failure in one full-suite run, a pass in another, then five clean runs later the same day with no code change. Green on Linux throughout. The obvious reading is a platform focus defect, and it is wrong.

**What was tried, and the hypothesis it killed**: the first theory was that macOS never granted the window key status, because it decides keyboard focus at the *application* level and the run was launched from a terminal that stays frontmost. A probe printing `window.is_active()` alongside the focus state was supposed to confirm it. **It disproved it instead** — and did something more useful:

```
PROBE t0        active=false  is_focus=true  has_focus=false
PROBE after 1 non-blocking iteration:  active=true  is_focus=true  has_focus=true
```

⚠ **The probe also made the test pass**, every time, which is worth flagging on its own: the extra pumping it did before the wait was enough to change the outcome. A probe that perturbs the thing it measures is still useful — but only for what it reads at `t0`, never for whether the run then passed.

**Root cause — two different questions, one of which nothing under test was asking.** `gtk_widget_has_focus()` asks whether the widget holds the **global input focus**, which requires its toplevel to be **active**. `gtk_widget_is_focus()` — equivalently, walking `gtk_window_get_focus()` — asks whether it is the **focus widget within its window**, which does not. The action under test was gated on `notify::focus-widget`, and its handler walked `gtk_window_get_focus()`. So at the instant `grab_focus()` returned, the asserted state was **already correct**, and the test then spent 5 s waiting for an activation the assertion never needed.

On X11 under Xvfb that gap is invisible: the window is active as soon as it maps, so the two predicates are true at the same moment and the stricter one costs nothing. On macOS activation is a separate, application-level, environment-dependent event — hence the intermittency, and hence "it depends on run position", which is the shape that makes this look like a platform defect rather than a test defect.

**This sharpens ScrAP-139's testability note rather than repeating it.** That entry established that a focus hazard needs a **MAPPED** toplevel, not a window manager. There is a third state: **mapped ≠ active**. `grab_focus`, focus-in and the focus-widget notify all fire on a merely mapped window; only `has_focus()` additionally waits for activation. A test that maps and pumps has satisfied ScrAP-139 and can still hang on this.

**Resolution**: make the readiness probe walk **the same accessor the code under test walks** — here, the existing helper that traverses `gtk_window_get_focus()`, which three of the same test's five waits already used for an unrelated reason (entries delegate focus to an internal `GtkText`, so the wrapper's own `has_focus()` never becomes true). The inconsistency between the three entry waits and the two editor waits *was* the defect; the entry sites had been right by accident.

**Verified rather than assumed to be non-vacuous.** The fix makes the probe's condition true before the loop starts, so "it passes now" proves nothing on its own — the guard was therefore **mutation-tested in both directions**: forcing the gate's predicate to `false` fails the test on *"select-all must stand down while the find entry holds focus"*, and forcing it to `true` fails on *"select-all must stay enabled while the editor itself holds focus"* — the latter being precisely the assertion whose wait was changed. Both fail on the behavioural assertion, not on a readiness timeout, which is what distinguishes a real guard from a probe that merely stopped complaining.

**Scribobulate**: `src/window/editoractions.rs` — `focus_is_within` (whose doc comment now carries the rule), and the three waits realigned onto it.

**Lesson**: a readiness probe is not a free assertion — it is a second, independent contract the test imposes, and if it is **stricter than the behaviour under test** it can fail on its own terms while the code is perfectly correct. Choose the probe by asking *what does the code under test actually observe*, and poll **that**, through the same accessor if possible; anything stronger is a source of false failures that will present as flakiness, and will present as the *platform's* fault on whichever platform decouples the two conditions. Two smells: a probe polling a *stricter cousin* of the thing the production code reads (`has_focus` vs the focus widget, "fully rendered" vs "allocated", "connected" vs "queued"), and — the loud one — **a test whose failure message names a readiness step rather than an assertion**. The second is nearly diagnostic on its own: a timeout waiting for a precondition means the precondition is in question, not the behaviour.

**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-119).

## 183. A mutation that fails on an earlier precondition proves nothing about the guard under test
**Symptom**: a guard was strengthened, and mutation-testing was used to prove it now discriminates — the discipline this project already applies (ScrAP-78). The mutation was applied, the test went **red**, and that was taken as proof. It was not. The mutation had broken an *earlier* `assert!` in the same test — a precondition establishing the state the real assertion needed — so the assertion under test never ran at all. The run's red/green signal is identical either way.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-11).

## 184. Four green checks, none of them the outcome — the plumbing was verified and the user-visible result was not
**Symptom**: a feature shipped with four passing checks that each independently confirmed a link in its chain — the value mapping's polarity, the live OS read cross-checked against an independent tool, the toolkit setting moving the resolved palette, and an end-to-end assertion that the module's entry point left the toolkit agreeing with the OS. Every one passed, including under mutation. The feature did not work: the window still rendered in the old appearance, which was the entire user-visible point.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-10).

## 185. An idle queued from a native OS callback is not dispatched for seconds — and deferring the *read* answers the event with the wrong value

**Symptom**: an OS-level appearance change (macOS light↔dark) was observed correctly — the native callback fired, on the main thread, every time — yet the app never reacted. No error, no warning, no failed call. Instrumenting both ends produced a timeline that made no sense at first reading:

```
callback FIRED (main=1), os_dark=true      <- the flip to DARK
... 8 seconds, nothing ...
idle ran, os_dark=false                    <- reads the setting AFTER the flip BACK
```

The handler did fire; it simply ran long after the event, and by then the value it went to look up had changed back. The app then correctly applied "light", which was already the current state, so the change gate suppressed the write and there was nothing at all in the log.

**What was tried**:
- Assumed the observer registration was broken, because a minimal C program using the same API worked. It was not: an isolation probe registering two observers for the same notification — one with the name string kept alive, one released immediately after registration — showed **both** firing. The name's lifetime is not the variable, and a plausible ownership diagnosis was wrong.
- Assumed the callback was being delivered on a worker thread and the marshal was therefore load-bearing. It was not: measured on the main thread every time.
- Added an unrelated one-second repeating timeout while chasing something else. **The bug vanished.** That accident was the diagnosis: with any other source waking the loop regularly, the idle drains promptly and the code appears correct.

**Root cause**: two independent facts compounding. First, an idle queued from inside a native run-loop callback is not serviced promptly by the GLib main context on this backend — an otherwise-idle app can go many seconds before iterating (measured 8 s and 13 s; a running `GMainLoop` in a test still left it undispatched after 1.3 s). Second, and far more damaging, the deferred closure did not carry the event's value — it **re-read the OS setting when it eventually ran**. So the delay was not merely a late repaint: for any change that goes there and back, the deferred read observes the *opposite* of what triggered it, and the app converges on exactly the wrong state while every individual step looks right.

**Resolution**: read at the event, apply at the event. The value is sampled synchronously in the native callback, and the work is done there too — the callback was measured to be on the main thread, so touching the toolkit is legitimate. The boundary is sealed with `catch_unwind`, because a Rust panic may not unwind across an `extern "C"` frame and would abort the process rather than surface an error. The off-main-thread branch is retained but now marshals the **captured value**, so even if it is ever reached it cannot reintroduce the staleness.

**Lesson**: "marshal work to the main loop" is the right reflex and it has a sharp edge — *what* you defer decides whether the deferral is safe. Deferring the **application** of an already-captured value is sound; deferring the **observation** silently rebinds the question to whenever the loop gets round to it, which for a toggling input is arbitrary and can be exactly inverted. Sample at the edge, defer the effect. Two corollaries worth carrying: an idle scheduled from outside GLib's own dispatch may not be prompt at all where a foreign run loop owns the thread, so never treat `idle_add` as "very soon"; and when adding unrelated instrumentation makes a bug disappear, that is not a Heisenbug to be waved off — the instrumentation is a *source*, and what it woke up is the finding.

**See**: gtk4-rs skill → deferred-work-and-ordering (GTK4Rs/AP-185).

## 187. A byte range captured at build time and applied at click time is a bet, not a coordinate

**Symptom**: clicking **Remove** on an annotation card in a document with unsaved
edits deleted the wrong text. Reported by the operator as "it removes the wrong
text and clobbers other content". Reproduced against the mutation core directly:

```
captured against:  alpha {==beta==}{>>note<<} gamma  → "alpha beta gamma"   ✓
seven characters typed at the top, then Remove clicked on that same open card:
                   EDITED alpha {==beta==}{>>note<<} gamma
                                                  → "EDITEDpha note<<} gamma"
```

The annotated word is gone, three characters of an unrelated word are gone, and
the markup is left half-open so the next scan parses a different document. The
sibling **Edit** path was corrupting identically (`{==betrevised{>>note<<}`) and
nobody had noticed. If the document had *shrunk* past the captured range instead,
the same call panicked — `start byte index 26 is out of bounds` — and a panic
inside a GTK signal handler aborts the process, taking the unsaved work with it.

**What was tried**:
- Reading the code first suggested a *race*: the split-mode live re-render
  re-scans the source on a 300 ms debounce, so the window looked narrow and
  timing-dependent. That framing was wrong and would have produced a
  timing-shaped fix. The re-render is only the *second* clock; the first is the
  user typing, which has no bound at all.
- Blaming the popover's staying open. Related, and genuinely a contributor —
  sibling defects had left the card without outside-click or scroll dismissal, so
  it could now sit open across arbitrary editing — but dismissing it sooner only
  narrows the window. It is not a fix, because the re-render invalidates the
  captured range with no user interaction whatsoever.
- Validating the range and refusing when it no longer fits. Necessary, and
  adopted as the floor — but **not sufficient on its own**: it converts "your
  document is silently wrong" into "the button silently does nothing", which is a
  second bug wearing the first one's clothes.

**Root cause**: the mutation was expressed as *"delete bytes 6..32"* rather than
*"delete this annotation"*. A range is only meaningful against the exact string it
was computed from, and this one was carried across time in a closure — captured
when the card's content was built, applied when the button was clicked, with
nothing in between re-establishing that the two strings were the same. The
mutation primitives compounded it by slicing raw (`&source[a..b]`), which is
partial: it panics rather than declining.

**Resolution**: never carry a range across time on its own — carry it together
with **the text that occupied it**, which is what makes it re-findable. A small
display-free type owns the rule: resolve to the captured offset if the text is
still exactly there (the common case, one comparison, and it also disambiguates
several identical constructs that have not moved); otherwise take the occurrence
**nearest** the captured offset, since an edit elsewhere shifts a construct by
that edit's size; otherwise refuse. Sub-ranges (the kept claim, the comment body)
are stored **relative** to the construct so the pair travels as one unit and
cannot be resolved against different positions. The capture happens at the single
point where the ranges are produced, so range and text are incapable of
disagreeing. Underneath that, both mutation primitives were made total — `get`
rather than `[]` — so an unresolvable range is a clean no-op instead of a splice
or an abort.

Both halves are mutation-tested, which mattered more than usual here: the two
defenses cover each other, so the end-to-end guard passes with *either one*
removed and only fails when both are. Each defense therefore needed a guard that
fails when that defense alone is taken away — the end-to-end tests pin the
resolution (reverting it reproduces the exact corruption string above), and
direct unit tests pin the totality (reverting it reproduces the exact panic).

**Lesson**: an index into mutable content is a *reference*, and a bare integer is
the one form of reference that cannot be checked. The moment such an index outlives
the instant it was computed — stored in a closure, a widget's state, a queued
message, a row model — it needs an identity that can be re-established, not just a
number that can be re-used. Ask of any captured offset: *what happens if the thing
it points into changes before this runs?* If the answer is "it points somewhere
else now", the code is one edit away from destroying data, and no amount of
narrowing the window fixes it — only re-resolution does.

Two corollaries worth carrying:

- **Make the primitive total at the boundary where partiality is reachable.**
  `&s[a..b]` is fine for ranges you just computed and a landmine for ranges you
  were handed. Where the caller's range comes from another point in time,
  `Option` is not defensive clutter — it is the honest type, and it converts an
  aborting process into a no-op.
- **Safety and function are separate requirements, and fixing only the first is a
  trap.** "Refuse when unsure" makes the corruption stop and makes the feature
  useless; the nearest-occurrence rule is what keeps Remove *working* after the
  user types. A guard that makes a command silently inert will be reported as a
  new bug, and rightly.

## 188. "It broke when I removed X, so X was providing it" — a temporal correlation dressed as a mechanism
**Symptom**: an annotation card stopped following the document when the view
scrolled — it stayed pinned where it was shown while the text moved out from
under it, ending up anchored to an unrelated line. The operator reported it as
"annotations no longer scroll with the document". Two sibling defects (no
outside-click dismissal, a stale anchor when switching annotations) appeared in
the same window of changes, and all three traced to one commit: the card had
been switched from `autohide` to non-autohide to fix a separate, real defect
where a mouse click on the card's own buttons was misrouted by the seat grab.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-15).

## 189. A GTK doc comment promised scroll-tracking the code never implemented — and the same API silently feeds the view's minimum size

> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkTextView children / size negotiation). Researcher-sourced and MEASURED against the installed 4.6.9, with the upstream fix commit identified.*

**Symptom**: three annotation-card defects all reduced to "the card is positioned in a coordinate space that expires", so the attractive fix was to re-home the card *inside the document* — placed in **buffer** coordinates via `gtk_text_view_add_overlay`, letting GTK move it as the view scrolls and making the whole class of stale-rectangle bugs impossible. The API documents exactly that: *"@child will scroll with the text view."*

**What was tried**:
- Reading the documentation and the API shape, which agree: the coordinates are buffer coordinates and there is a `move_overlay` to update them. Everything says this works.
- A probe built against the actually-installed GTK pinned a child at buffer y=500 and scrolled: the child reported y=500, 500, 500. A control child added at a `GtkTextChildAnchor` in the same run moved correctly, so the measurement was sound.
- Forcing the issue with a `move_overlay` at unchanged coordinates: still no movement. With a `queue_allocate` alone: still none. Only both together move it.

**Root cause**: two independent short-circuits, either of which alone is enough to pin the child. (a) A scroll re-allocates the view only when it has *anchored* children — overlay children live in a different list entirely, so a scroll usually queues nothing but a redraw. (b) Even when an allocation is queued, the routine that writes the child's offset ends in `queue_draw` rather than `queue_allocate`, so the container never gets `alloc_needed`, the allocation is skipped by the standard "nothing changed" early-out, and the only code that would move the child never runs. Both were fixed upstream, together, in **4.19.1** (first stable 4.20.0) — so **no shipping stable release before 4.20 tracks the scroll**.

And there is a second, worse problem that no version fixes: **an overlay child's minimum size propagates into the view's own minimum size.** The view measures its overlay container, and that container takes the *maximum* over every overlay. A ~280 px floating card therefore raises the text view's minimum width by ~280 px, which under a marginal `Automatic` horizontal policy re-arms the scrollbar — and with it the ScrAP-139 → ScrAP-56 layout-churn chain that ends in a stuck-blank view. There is no opt-out (unlike a `GtkOverlay` child, whose `measure` property defaults to *off*).

**Resolution**: keep the card as a popover attached with `set_parent`, which is in **neither** the anchored-children nor the overlay list and therefore contributes nothing to the view's size request — and re-derive its anchor from the annotation on every present and every scroll, rather than moving the card into a coordinate space that would carry the tracking for us. This is also what the adjacent source-view library does for every one of its buffer-anchored interactive popups.

**Lesson**: a documented promise about a widget's own behaviour is a **hypothesis to measure, not a contract** — and this one had been false in every stable release for years, which is exactly how long a plausible-sounding doc comment can survive unchallenged. Two further generalisations worth carrying: a positioning API that looks like it only *places* a child may also make that child **participate in the parent's size negotiation**, so check the measure path before adopting one in a layout you have already had to fight; and when a defect is fixed in a version *above your floor but below your ceiling*, adopting the API buys you **divergent behaviour across your own platform matrix**, which is worse than a bug you can reproduce everywhere.

**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-189).

## 190. A `show`-vfunc invariant has a hole exactly on the re-present path — showing an already-visible widget never runs it

> *Core GTK4 + a general lesson about where to site an invariant. Strong candidate for the gtk4-rs skill.*

**Symptom**: a reused popover was re-pointed at a fresh anchor whenever it was presented, with the recompute placed on the widget's `show` vfunc so that *no* route to being on screen could skip it. Opening the card from a click worked. Opening it from a programmatic navigation worked. The one case the defect was actually about — activating a **different** annotation while the card was already up — silently kept the old anchor.

**What was tried**:
- Placing the recompute on the `show` vfunc, copying the upstream library that solves this exact problem. This reads as airtight: every presentation goes through `show`.
- Testing it by opening the card from closed, repeatedly, at different scroll offsets. All green.
- The hole only surfaced when a regression test was written with the card **already visible**, which is the state the issue describes.

**Root cause**: `set_visible(true)` on a widget that is already visible is a no-op — it does not run the `show` vfunc. A reused surface exists precisely so it can be re-pointed *without* a hide/show cycle (setting the anchor on a visible popover re-presents it immediately), so the busiest path through the code is the one that never touches the vfunc. The invariant was sited on the one route that path skips.

**Resolution**: enforce at two points — the vfunc for genuine transitions into visibility, and the explicit "present" method unconditionally. Also clear any "we already pointed there" idempotence cache at presentation: that cache records what *you* last wrote, not what the widget currently holds, so anything writing the property from outside leaves it stale and the skip then suppresses the very recompute a presentation must perform. Keep the skip only on the high-frequency scroll path, where it is actually load-bearing.

**Lesson**: **an invariant is only as good as the narrowest path it sits on.** Before siting one on a lifecycle vfunc, enumerate the states the object can already be in when the operation is requested — a no-op fast path in the framework is invisible in the call graph but is exactly where a "cannot be skipped" guarantee gets skipped. Corollary for the test: a guard written for a *transition* must be exercised from the *steady state* too, or it certifies only the case that was never broken.

**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-190).

## 191. A "pre-warm" of a reused widget becomes a teardown of a live session the moment that widget owns state

> *General GTK/lifecycle lesson; the mechanism (a first-`map` idle) is framework-agnostic.*

**Symptom**: a card opened by a programmatic navigation vanished a few frames later, with no user input and no error. The `closed` signal fired with the session flag still set — something had dismissed it, and nothing in the dismissal paths had run.

**What was tried**:
- Auditing every call site that dismisses the card. None of them fired.
- Suspecting the framework of popping the surface down on its own (a re-layout, a failed present, a lost grab) and reasoning about which internal path could do it — plausible, unfalsifiable, and wrong.
- Capturing a **backtrace from inside the `closed` handler**, which named the culprit in one line.

**Root cause**: a one-shot warm-up existed to absorb a first-realization cost — it pops the *persistent, reused* instance up and straight down once, from an idle queued at the view's first map. That was inert while the widget was a bare surface with no state of its own. Once the widget became a stateful card with a session, the warm-up's `popdown()` was no longer a no-op: it ended a real session. The implicit justification — "this runs at first map, long before the user can interact" — is an assumption about *idle ordering*, and it is false whenever something opens the card in the same turn the view first maps, which a programmatic navigation does.

**Resolution**: the warm-up no-ops when the instance is already in use. (Returning after consuming the one-shot flag is right, not a leak: if a session is open the widget has already been realized and presented, so the cost the warm-up exists to absorb has already been paid.)

**Lesson**: **a warm-up that drives the shared instance is indistinguishable, to that instance, from real use** — the moment the instance gains state, the warm-up's teardown is a real teardown. Re-audit every "harmless" priming/warming step when the thing it primes acquires state. And when something is dismissed and no dismissal path in your code ran, **backtrace the signal** rather than theorising about the framework: the answer here was one frame deep and the theorising was heading somewhere else entirely.

**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-191).

## 192. `popdown()` is not animated, `closed` fires from `hide` — so your own transient hide trips your own "is it still open?" backstop

> *Core GTK4 (GtkPopover semantics on 4.6.9). Corrects a premise this project had carried in code comments since ScrAP-112.*

**Symptom**: a card that hides itself while its anchor is scrolled off the viewport — intending to come back when the anchor returns — never came back. Its session flag had been cleared by its own hide.

**What was tried**:
- Wrapping the "this hide is mine" flag tightly around the `set_visible(false)` call, on the assumption that the `closed` signal is emitted synchronously from inside it. This is the natural shape and it did not hold up.
- Reaching for the long-standing project belief that *"`popdown` is animated, so `is_visible()` lags during the close animation"*, which had justified keeping a separate explicit open flag. Source-checked against 4.6.9: **false**. `gtk_popover_popdown()` is `gtk_widget_hide()` plus a cascade that **early-returns for a non-autohide popover**; there is no transition, no tick callback, and `is_visible()` flips immediately. For a non-autohide popover `popdown()` and `set_visible(false)` are the same operation.

**Root cause**: `closed` is emitted from the **`hide` vfunc**, so *every* hide emits it — including a deliberate, temporary one. A `closed` handler used as a backstop for "the session ended" therefore fires on the widget's own transient hide and ends a session that was only being paused.

**Resolution**: keep the session flag distinct from visibility (still correct, for a better reason than the animation myth — a card can legitimately be *open but off screen*), and mark a self-initiated hide with a **sticky** flag: set when hiding, cleared when shown again or genuinely dismissed. That needs no assumption about when the signal arrives.

**Lesson**: a framework signal named for an *outcome* ("closed") is usually wired to a *mechanism* (the hide vfunc), and cannot distinguish your intent from the user's — so never infer intent from it; carry the intent explicitly, and make the marker outlive the call rather than bracket it, so the guard holds whenever the signal arrives. Separately: **audit inherited premises when you touch the code they justify.** "Popdown is animated" was load-bearing for real complexity here and was never true on this version; a belief that only ever appears in comments is one nobody re-tests.

**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-192).

## 193. Driving/screenshotting a GTK4/Quartz app from an agent on macOS: no established recipe, and every gap in one reads as an app defect until isolated

> *macOS platform + agent-driving mechanics, not GTK internals. Strong candidate for the gtk4-rs skill's automated-UI-testing module, which is currently written entirely for Xvfb+xdotool and has no macOS content at all.*

**Symptom**: verifying the annotation-card fix (ScrAP-187 onward) required a live "manual pass at the physical machine" per POLICY.md's macOS verification rule — the automated `gtk-integration-tests` suite is structurally unable to run on macOS at all (gtk4-rs dispatches its test bodies onto a worker thread, which GTK's macOS backend forbids), so a real window had to be driven and screenshotted by hand. Every attempt produced a plausible-looking failure that was chased as a possible bug in the app before turning out to be a gap in the driving mechanism itself:

- Two clicks at carefully-reasoned toolbar coordinates via `osascript`/System Events `click at {x,y}` produced no visible change at all, reading as "clicks aren't reaching the app."
- Mid-session, `screencapture` started failing outright (`could not create image from rect`) and a click landed on `loginwindow` instead of the app.
- After recovering, a further "no effect" click was chased for several rounds — at one point an annotation card opened showing text that had never been typed by the session — before the actual cause surfaced.

**What was tried**:
- Assumed the "no visible change" clicks meant delivery was failing. It wasn't: the identical mechanism worked perfectly once the target coordinates were right. The two failed clicks were simply aimed at the wrong pixel — a coordinate-math error, not a click that didn't arrive.
- Diagnosed the screen as genuinely **locked** (`ioreg -n Root -d1 | grep CGSSessionScreenIsLocked` → `Yes`) — root cause: macOS's own idle/display-sleep timer (as short as 2–3 minutes) had fired. An agent driving the app purely through CLI/AppleScript calls generates **no real HID input**, so the OS reads the session as an idle human at a keyboard and locks it exactly as it would for a person who walked away — a hazard specific to headless-style agent driving on a real desktop session, absent on a disposable Xvfb display (which has no idle timer at all).
- After the "foreign text in a card" scare, traced it to the click having landed on the agent's own **controlling Terminal window**, not Scribobulate — something (harness- or OS-level) re-raises the controlling terminal to frontmost **between tool-call turns**, so a click issued in a fresh shell invocation without re-asserting the target app's frontmost status can silently land on whatever regained focus in the interim. The "foreign text" itself was the terminal's own transcript, rendered into the screenshot — not app state at all.
- Tried enumerating the app's UI as macOS accessibility (`AXUIElement`) objects, to click by stable reference instead of raw coordinates — the platform-idiomatic approach, and immune to all of the above. GTK's Quartz backend exposes only the **native window-chrome** buttons (close/minimize/fullscreen) to `NSAccessibility`; none of the app's own widgets — toolbar buttons, list rows, popover content — appear in the tree at all (`entire contents of window` returns three `AXButton`s, full stop). AX-based automation is not viable for a GTK4/Quartz app; raw screen coordinates are the only path.
- Looked for a scroll-wheel primitive in both `System Events` and `cliclick` (a Homebrew CLI click-driver) — neither has one.
- **Recurred in a later session on the same task**, after this entry already existed: two more clicks were computed by reading pixel coordinates directly off a displayed screenshot thumbnail and using them as absolute screen coordinates, skipping the thumbnail-to-native and Retina conversions entirely. Both misses landed on the agent's own terminal window (parked just past the app window's edge), which duly became frontmost and was mistaken, again, for the terminal-focus-theft hazard above — that diagnosis felt cheap to reach for precisely *because* it was already a known, named failure mode. It cost real time before a direct frontmost-process query around the click (querying immediately before and immediately after) showed the window was correctly focused right up to the click and only changed afterward — consistent with the click itself landing outside the app, not with focus drifting away beforehand.

**Root cause**: none of this was a defect in the application. It was the absence of any established recipe for driving a GTK4/Quartz app on macOS — the gtk4-rs skill's entire automated-UI-testing module assumes Linux (`xdotool`, ImageMagick `import`, a disposable `Xvfb` session with no idle timer, no competing terminal window, and a WM that hands over accessible widget geometry for free via `xwininfo`). Every one of the four symptoms above is a distinct macOS-specific gap that Linux driving never has to cross:

1. A live desktop session has an idle/lock timer that a synthetic-input-only agent trips.
2. The agent's own controlling terminal is a real, focusable window that can regain frontmost status independently of the target app.
3. GTK4's Quartz backend does not populate `NSAccessibility` for its own widgets.
4. Neither of macOS's two common CLI input-automation tools (`System Events`, `cliclick`) exposes a scroll-wheel event.

**Resolution** — the recipe assembled, piece by piece:
- **Prevent the idle lock** for the session's duration: run `caffeinate -disu` in the background before starting any live-driving work. If a session goes stale mid-task, check `CGSSessionScreenIsLocked` before assuming a click/screenshot failure is an app or delivery bug.
- **Reassert frontmost status immediately before every click**, in the same shell invocation as the click itself — `osascript -e 'tell application "System Events" to set frontmost of process "<name>" to true'` — never rely on an earlier turn's activation having survived into the current one.
- **Calibrate before committing to a click, every time.** Compute the candidate coordinate from a `screencapture -R<winX>,<winY>,<winW>,<winH>` crop (window geometry read once via `System Events`' `position of window 1` / `size of window 1`, in points), converting for the display's backing-store scale (a Retina screenshot is pixels at 2x; window-position/click coordinates are points) — then *move the cursor only* (`cliclick m:x,y`, no click) and capture again with `screencapture -C` (cursor drawn in) to confirm the cursor sits exactly on the intended target before clicking. This single discipline eliminated nearly every "nothing happened" in this session — all of them were coordinate misses, not failures of the click mechanism.
- **Synthesize scroll via JXA**, not `System Events`/`cliclick` (neither has the primitive): `osascript -l JavaScript -e 'ObjC.import("CoreGraphics"); var e = $.CGEventCreateScrollWheelEvent($(), $.kCGScrollEventUnitLine, 1, delta); $.CGEventPost($.kCGHIDEventTap, e);'`, with the cursor positioned over the target widget first — GTK, like most toolkits, routes wheel events to whatever is under the pointer, not the focus widget.
- Prefer `cliclick` (Homebrew) over raw `osascript`/System Events as the primary driver for click/type/key sequences — same underlying event-posting mechanism, but a purpose-built CLI whose move-only mode is what makes the calibration step above practical.

**Lesson**: a GTK4/Quartz app on macOS has no native accessibility surface to automate against — unlike a well-behaved Cocoa app, where AX-element references would sidestep coordinate math entirely. Calibrated screen-coordinate clicking, verified against a cursor-visible screenshot *before* every click, is not a fallback technique here, it is the only viable path — and skipping the calibration step is exactly what manufactures the appearance of "the app isn't responding" or "a widget is misbehaving" when the real defect is a few points of arithmetic. Two further hazards are specific to *agent-driven* macOS sessions and have nothing to do with GTK at all: the OS's own idle/lock timer cannot distinguish a synthetic-input agent from an unattended human and will lock the session out from under it, and a controlling terminal window is a real, independently-focusable surface that can silently steal the frontmost slot between tool-call turns — both need an explicit, repeated countermeasure (`caffeinate` for the former, reasserting frontmost at the point of every click for the latter), not a one-time fix at session start. None of this generalizes from the Linux/Xvfb driving loop; it has to be learned once, on this platform, before "the app is broken" can be trusted as a diagnosis rather than a byproduct of the harness.

**Diagnostic ordering, learned on the recurrence**: a coordinate-conversion slip and terminal-focus-theft produce the *same* observable — a click that silently lands on the wrong window — and once a named failure mode exists, it is the cheaper-*feeling* explanation to reach for, not the cheaper one to rule out. Recomputing the arithmetic is strictly faster than investigating a focus-theft race, and checking `frontmost of process` immediately before and immediately after the click is a direct, few-second test that distinguishes them (unchanged-then-changed points at the click itself landing elsewhere; already-different beforehand points at real theft). Check the arithmetic — and confirm frontmost held right up to the click — before spending time on the rarer, harder-to-fix explanation, however familiar it's become.

**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-163).

## 194. A shared per-line helper that hands out a RAW line makes every block transform blind to the container prefix — one rule, four copies to get wrong
> *Non-core (CommonMark/Markdown transform logic + enforcement discipline) — no GTK internals involved. Do not fold into the gtk4-rs skill. The enforcement half is the skill's GTK4Rs/AP-108/GTK4Rs/AP-130 ladder applied to a pure module.*

**Symptom**: every block formatting command corrupted a blockquoted line. Heading 3 on `> Heading` produced `### > Heading`; Bulleted/Numbered/Task List on `> item` produced `- > item` / `1. > item` / `- [ ] > item`. None of those is a heading or a list item in any CommonMark renderer — each is a paragraph whose text begins with the marker, and the quote is lost as well, so the preview showed the literal characters. The less obvious half: **toggle-off broke the same way**. The heading detector required the `#` run at offset 0, so `> ## Title` was not recognised as an H2 at all — pressing Heading 2 prefixed *again* (`## > ## Title`), compounding on every press. The list toggles failed identically (`> - item` was not seen as a bullet). Code Block was the same defect in a different shape: it fenced at column 0 (`` ``` `` / `> code` / `` ``` ``), ending the quote and swallowing the `> ` as code text.

**Root cause**: the block formatters all opened by resolving a whole-line span through one shared helper (`text::block_span`) and then reading `span.text` and splitting it on `'\n'` — i.e. the shared layer handed each transform the **raw line**, prefix and content fused. Nothing in it separated a line's *container prefix* (`> `, `>> `, `> > `) from its *content*, so the container prefix sat at the front of what every transform treated as the line's body: prepend a marker and it lands in front of the `> `; test for a marker at offset 0 and the `> ` hides it. The blind spot was not four independent bugs but **one** missing distinction inherited four times — and the parser that captures a line's exact `>` nesting verbatim already existed in the same module, written for Enter-continuation. It was simply never consulted by the transforms.

**Scribobulate**: the split happens once, in the shared block-span layer, and the raw form is sealed off.
- `BlockSpan::lines()` yields `BlockLine { prefix, content }`, splitting on the one existing `quote_prefix_len` parser (`>` runs each with an optional single space; ASCII, so the byte split is char-safe). `BlockSpan::first_line()` gives the toggle-off detectors the first line's *content*.
- Two mirror seams, each exposing only the half its callers own: `text::map_content` rewrites content and re-attaches each line's own prefix verbatim (Heading, Bulleted, Numbered, Task — so a marker can only land *inside* the container, and the detectors see the marker behind a `> `), and `text::map_prefix` rewrites the prefix and leaves content alone (Quote, the one command that *owns* the container: toggle-on adds a level, toggle-off peels exactly one). Code Block changes the line count so it cannot use `map_content`, but it still reads through `lines()`, so its fences take the block's container prefix (`> ``` `) and a fenced block already inside a quote is still recognised for unwrap.
- **`BlockSpan::text` is now module-private** — the enforcement rung that matters. No formatter, present or future, can reach the fused line and re-acquire the blind spot; container-awareness is inherited rather than remembered.
- The container prefix is read **narrowly**: blockquote nesting only. A list marker is deliberately not a container, so Heading on `- item` still yields `## - item` (unchanged behaviour, and a case nobody has asked to change).
- Guarded by unit tests per formatter, all display-free; every one was mutation-tested by neutralising the split (`split_at(0 * len)`), which fails 10 of them — the pre-fix behaviour, reproduced on demand.

**Lesson**: when a family of transforms shares a helper, **what the helper hands back defines the blind spot they all inherit** — a raw line is a fused pair (container, content) and every consumer that treats it as content is wrong in the same way. Fixing it per formatter is the same defect written N more times, and the next formatter added starts out broken; fixing it at the seam fixes the ones not yet written. Two corollaries. First, the *detector* half is as affected as the *emitter* half and is easier to forget: a toggle that keys on "marker at offset 0" silently stops toggling inside any container, which presents as "the command adds a second prefix" rather than as a detection bug. Second, splitting at the seam is only half the fix — until the fused form is unreachable (module-private), the seam is a convention, and a convention is what the next author bypasses. Climb to the strongest rung available: here privacy was free, because every caller already lived in the module.

**See**: TDD 10.20 (block commands inside a blockquote); MANUAL-TEST 10.20; `format::text` module docs ("The container-prefix seam"); the enforcement ladder is the gtk4-rs skill's GTK4Rs/AP-108/GTK4Rs/AP-130.

**Family**: parser event-stream blind spots — ScrAP-147 (element grouping vs child tags), ScrAP-158 (item emitted ≠ content produced), ScrAP-194 (a shared helper's RAW line hides the container prefix), ScrAP-195 (a second tokeniser's syntax is invisible to the first's events), ScrAP-196 (a symptom-keyed fallback swallows the next cause). Distinct mechanisms, one theme: **the event stream does not say what you assume** — kept whole rather than merged, because the case table that would hold them loses the mechanism that made each expensive.

## 195. A decision driven off one parser's event stream cannot see the constructs a second tokeniser owns
> *Non-core (pulldown-cmark/CommonMark + this crate's own inline scanner) — no GTK internals involved. Do not fold into the gtk4-rs skill. Sibling of #66/#75/#147/#158 (pulldown event-stream quirks); the "one definition, two consumers" half is kin to #194.*

**Symptom**: annotating a *partial* selection inside `==highlight==`, `~~strike~~`, `^sup^` or `~sub~` spliced the CriticMarkup **between** the delimiters — `a ==m{==ar==}{>>note<<}k== b` — markup that parses as neither construct. The identical selection inside `**bold**`, a `` `code span` `` or a `[link](url)` was correctly widened to wrap the construct whole. Measured, reading back the balanced source range for a selection of the marked characters:

| Source | Selection | Balanced to (before) | (after) |
|---|---|---|---|
| `a **bold** b` | `bol` | `**bold**` ✅ | unchanged |
| `` a `code` b `` | `od` | `` `code` `` ✅ | unchanged |
| `a ==mark== b` | `ar` | `ar` ⛔ | `==mark==` |
| `a ~~strike~~ b` | `rik` | `rik` ⛔ | `~~strike~~` |
| `a ^sup^ b` | `u` | `u` ⛔ | `^sup^` |
| `a ~sub~ b` | `u` | `u` ⛔ | `~sub~` |

**Root cause**: `copymap::balance_source_span` decided what to swallow by walking pulldown-cmark's event stream and matching `Code`, `Emphasis`, `Strong`, `Strikethrough`, `Link`, `Image`. The four failing constructs are **not pulldown constructs here**: pulldown has no highlight/mark option at all, and its caret/tilde flanking rules never match the tight Pandoc forms authors type, so this crate tokenises all four itself in `renderer::scan_scripts` (ScrAP-66/ScrAP-75). They therefore arrive at the balancer as ordinary `Event::Text` with **no event to match** — nothing errors, nothing is unhandled, the walk simply finds nothing to balance. Two tokenisers, one consulted.

Two things made the gap hard to see in review. The match arm **listed `Tag::Strikethrough`**, which reads as coverage — and is dead code: `md_options()` never enables pulldown's strikethrough (measured: `a ~~b~~ c` yields a single `Text` event), so that arm cannot fire. And the *preview* path, which carries the same contract, was **already correct** — it resolves against the copymap, which reinstates the stripped markers as inline nodes and so never consults pulldown for extent (measured, not assumed: all four balance correctly there). One contract, two implementations, one blind.

**Scribobulate**: make the second tokeniser span-shaped and consult it.
- `renderer::scan_script_spans(text) -> Vec<ScriptSpan { outer, inner, script }>` is now the primitive — the **single definition** of what these four constructs are — returning the whole-construct (`outer`, delimiters included) and content (`inner`) byte ranges. The renderer's string-shaped `scan_scripts` is *derived* from it, so the renderer and the balancer cannot drift apart.
- `balance_source_span` keeps its pulldown pass (minus the dead `Tag::Strikethrough` arm) and adds a second pass over each `Text` event's **source slice** — verbatim, because pulldown emits escapes and entities as their own events, so a span found within it is a source range once shifted by the event start. Both passes feed one `swallow` closure and one fixpoint loop, so a selection running from inside `==mark==` into `**bold**` swallows both.
- Guards: the four constructs added to the `ww_*` editor-path family, a both-tokenisers straddle case, an over-reach negative (prose `a == b` / `2^10` must stay char-precise — the scanner's tight-flanking rules gate the balancer too), and the measured preview-path behaviour pinned as a test so its correctness stops being an assumption. Every positive guard was mutation-tested by disabling the new pass; the negative one by making the pass swallow whole `Text` runs.

**Lesson**: when a project parses *some* of its syntax with a library and *some* itself, every decision derived from the library's output inherits a blind spot exactly the shape of the syntax you own — and it fails silently, because your constructs are indistinguishable from prose in that stream. The fix is not to add your constructs to the library's match arm (they can never appear there); it is to give your own tokeniser the same *shape* of output the decision needs (spans, not strings), derive the rendering form from it, and run both passes into one union. Two corollaries worth as much as the fix: a match arm for an event the configuration can never emit is worse than a missing one — it is what makes the hole look covered, so audit arms against the options actually enabled; and where the same contract is implemented twice (preview vs editor here), measure both before trusting either — the correct sibling gave no hint the other was broken, and "it must be fine, the other path does this" is how the gap survived shipping.

**See**: TDD 17.33 / 17.18; CAM Document Rendering row 3 (widened to name both tokenisers and both paths); `renderer::scan_script_spans`; `copymap::balance_source_span`; sibling pulldown quirks #66/#75/#147/#158. **Scope**: "the preview path was already correct" is about span *balancing* only — the claim-*painting* half of the same feature was wrong on both paths, which is #196.

**Family**: parser event-stream blind spots — ScrAP-147 (element grouping vs child tags), ScrAP-158 (item emitted ≠ content produced), ScrAP-194 (a shared helper's RAW line hides the container prefix), ScrAP-195 (a second tokeniser's syntax is invisible to the first's events), ScrAP-196 (a symptom-keyed fallback swallows the next cause). Distinct mechanisms, one theme: **the event stream does not say what you assume** — kept whole rather than merged, because the case table that would hold them loses the mechanism that made each expensive.

## 196. A fallback keyed on a symptom, not a cause, silently swallows the next cause that shares the symptom
> *Non-core (this crate's renderer/annotation offset mapping) — no GTK internals involved. Do not fold into the gtk4-rs skill. Immediate sibling of #195 (same feature, opposite half: #195 is which SPAN gets annotated, this is which CHARACTERS get painted) and a second instance of #194's duplicated-rule hazard.*

**Symptom**: found on the live display while verifying #194/#195, not by any test. An annotation's amber claim highlight covered the **whole** text run instead of the claim, on any line holding one of the four in-crate constructs. Measured (real KDE/X11, split mode, `a ==mark== b`); the third row is the one that matters, because the claim is a single plain character nowhere near the construct:

| Annotated claim | Source produced | Amber wash (before) | (after) |
|---|---|---|---|
| `Head` in `> Heading` (no construct on the line) | `> {==Head==}{>>c<<}ing` | `Head` ✅ | unchanged |
| the whole `==mark==` | `a {====mark====}{>>c<<} b` | `a mark b` ⛔ | `mark` |
| just the trailing `b` | `a ==mark== {==b==}{>>c<<}` | `a mark b` ⛔ | `b` |

**Root cause**: the cleaned-source→buffer mapper decided per content event, and gave up — tagging `(before, after)`, the whole event — whenever `buf_len != cleaned[s..e].chars().count()`. That branch is correct and necessary for a **synthesised** run: smart punctuation (`--`→`–`, `...`→`…`) and entities substitute characters, so there is no per-character correspondence and tagging half a synthesised glyph would be worse than tagging the run. But the branch was keyed on the *symptom* those runs happen to present — "the lengths differ" — rather than on the cause. A **marker-stripped** run presents the identical symptom (`a ==mark== b` is 12 chars of cleaned source, `a mark b` is 8 in the buffer) while being mappable to the character, because the marker positions are known exactly — `renderer::scan_script_spans` returns them. So a perfectly mappable case inherited "unmappable" treatment, and did so *conservatively*: over-painting looks deliberate, warns nobody, and is invisible to every test that only uses 1:1 prose.

**Scribobulate**: test the cause instead. `annotate::kept_chars` counts the chars of a run that actually reach the buffer (construct delimiters dropped, everything else 1:1) from the scanner's spans; the precise path runs whenever that count equals the event's buffer length — which **subsumes** the old 1:1 case (no constructs ⇒ `spans` empty ⇒ kept == run chars), so the fix *deleted* a special case rather than adding one. The whole-event fallback now covers exactly what it was written for. Equality is a sound test for "fully accounted for" because every transform in this pipeline only ever *removes* characters, so a synthesised substitution in the same run cannot make the counts agree by coincidence — it falls back rather than mis-mapping (guarded by a test).

The two character-identical copies of this mapper — `preview::build::highlight_tag_ranges` (body buffer) and `annotate::map_cleaned_highlight_to_local` (table cells) — were folded into one, the body path becoming a three-line typed adapter. Both carried the defect; fixing either alone would have left the other wrong, and nothing would have said so.

**Lesson**: when you write a defensive fallback, key it on the **cause** you are defending against, not on the observable that led you to it. A symptom-keyed guard is a permanent trap door: every later cause that happens to present the same way falls through it silently, and because a conservative fallback *degrades* rather than fails, no test, log, or warning marks the moment it starts being wrong. Ask "what do I actually need in order to do this precisely, and can I get it?" — here the answer was already in the codebase.

Two method lessons from the discovery, both cheap and both nearly skipped:
- **Attribute a surprise by discriminator before believing you caused it.** The wrong wash surfaced in the same session, on the same feature, minutes after changing how annotation spans are chosen — an overwhelming prior that it was a regression. The discriminator was one keystroke sequence: annotate a lone plain character, which no span-balancing widens. It reproduced, proving the defect predated the change. Without it the honest-looking next move is to revert a correct fix.
- **A defect you can fix now does not belong in the debt register.** It was written up as an ISSUES entry first, on the reasoning that it was a *third* problem and the operator was away. But the register exists for what is *not* being fixed; parking a fixable defect there converts a 40-minute fix into permanent reading for every future agent, and grows the pile the register is meant to shrink. Fix it and let the entry never exist.

**See**: TDD 17.18 (claim extent, including the marker-stripped case); MANUAL-TEST 17.39; `annotate::kept_chars` / `map_cleaned_highlight_to_local`; siblings #194 (one rule, N copies) and #195 (two tokenisers, one consulted).

**Family**: parser event-stream blind spots — ScrAP-147 (element grouping vs child tags), ScrAP-158 (item emitted ≠ content produced), ScrAP-194 (a shared helper's RAW line hides the container prefix), ScrAP-195 (a second tokeniser's syntax is invisible to the first's events), ScrAP-196 (a symptom-keyed fallback swallows the next cause). Distinct mechanisms, one theme: **the event stream does not say what you assume** — kept whole rather than merged, because the case table that would hold them loses the mechanism that made each expensive.

## 197. A `#[path]`-included module's children resolve against the attribute's directory, not the module's own name
**Symptom**: relocating a `harness = false` second crate root away from beside `lib.rs` (to `src/platform/mac/`, reached via `#[path = "../../gtk_suite_probe.rs"] mod gtk_suite_probe;`) failed to compile one of the modules it re-declared: `error[E0583]: file not found for module 'tests' … help: create "src/platform/mac/../../copymap.rs/tests.rs"` — `copymap.rs`'s own `mod tests;`, unrelated to the relocation, broke as a side effect of it.
**Scribobulate**: the second crate root sits beside `lib.rs` in `src/` (`src/gtk_suite.rs`), not under a relocated `#[path]`, so `mod` declarations resolve exactly as `lib.rs` resolves them.
**See**: general-engineering-principles (GEP-34).

## 198. `pub use` cannot widen `pub(crate)` visibility — there is no test-façade shortcut around it
**Symptom**: assumed a small `pub use crate::some_internal::Thing;` façade module, linked from an ordinary `tests/*.rs` integration test, could expose just enough of a `pub(crate)`-everywhere crate for a targeted test. Compiling it produced `error[E0364]: 'Thing' is only public within the crate, and cannot be re-exported outside`.
**Scribobulate**: `src/gtk_suite.rs` is compiled as part of the crate (`[[test]] harness = false`, sharing `lib.rs`'s module tree) rather than as an external façade re-exporting internals.
**See**: general-engineering-principles (GEP-34).

## 199. Treating an `insert-text` of `"\n"` as "the user pressed Enter" — a paste is many `insert-text`s, one of them a bare newline, and acting on it is undefined behaviour
> *Core GTK — fold into the `gtk4-rs` skill (sharpens GTK4Rs/AP-73, the buffer-edit-transform pattern, which recommends this hook without stating the paste hazard). Verified against GTK 4.6.9 source by the researcher; findings doc: `researcher-findings-insert-text-reentrancy-insert-range-paste-list-continuation.md`.*

**Symptom**: cut or copy a run of Markdown list lines in the editor, paste it elsewhere, and the **last line silently disappears** from what lands. Size-independent (a 3-line block loses a line just as a 40-line one does), and apparently **non-deterministic** — the same selection pastes correctly on a second try. Pasting after a lone `"- "` line was worse: only the first line arrived. The X11 clipboard was verified to hold the full text throughout (`xclip -o | wc -c`, 29 124 bytes exact, INCR included), and both app copy paths (editor `emit_copy_clipboard`, preview `copymap::resolve`) were verified char-exact — the loss was entirely on the **paste** side.

**Root cause**: the Enter conveniences hooked `GtkTextBuffer::insert-text` and treated an inserted `"\n"` as an Enter keypress. That proxy is not merely fragile, it is **guaranteed** to misfire. `gtk_text_buffer_paste_clipboard` hardcodes the rich path (`gdk_clipboard_read_value_async` for `GTK_TYPE_TEXT_BUFFER`, `gtktextbuffer.c:3982`) into `insert_range_not_inside_self` (`:1616-1664`), which walks the copied buffer with `gtk_text_iter_forward_to_tag_toggle` and emits **one `insert-text` per tag-delimited run** (`:1394`). A source line ending inside a syntax-highlight tag — `` `code` ``, `**bold**` — therefore contributes its trailing newline as a **bare `"\n"` run of its own**; no content-based test can distinguish it, because it genuinely *is* exactly `"\n"`. The apparent non-determinism was GtkSourceView's lazy highlighting: an un-highlighted region has no tag toggles, arrives as ONE run, and pastes fine. (A *foreign* app's `text/plain` also arrives as a `GtkTextBuffer`, via the deserializer at `:425-429`, but untagged — hence one run.)

Acting on it is **undefined behaviour**, not just a wrong decision. The `location` iter is passed `G_SIGNAL_TYPE_STATIC_SCOPE` (`:581`) — **not a copy** — so it is `insert_range`'s own destination cursor, and the signal's documented contract (`:564-567`) requires a before-handler not to invalidate it. The hook invalidated it by inserting *and* suppressed the default handler that would have revalidated it. Downstream, `_gtk_text_btree_insert` reads the stale `GtkTextLine*` with no validation while `gtk_text_iter_get_offset` returns the invalid-iterator sentinel `0` — which is exactly the "every remaining run inserts at offset 0 and vanishes" seen in the instrumented trace. GTK's only guard is the advisory `Invalid text buffer iterator` warning, *after* the damage (26 of them in the repro run, 0 after the fix). On other data the same state corrupts the B-tree instead of merely losing a tail.

**Resolution**: decide where the keystroke is, so that paste, middle-click PRIMARY paste, drag-and-drop and every programmatic `insert_range` cannot reach the decision **at all** — `src/window/editbar/newline.rs` now runs the whole edit from a CAPTURE-phase `GtkEventControllerKey` on the view (GtkTextView commits Return from its own *bubble*-phase controller, `gtktextview.c:5447-5478`, so capture is ahead of it), owning the Enter end to end and returning `Propagation::Stop`; the `insert-text` hook is gone. Guard-shaped fixes were tried and rejected on measurement, not taste: a "was a key pressed?" token leaves the same UB one keystroke-shaped path away (Ctrl+Z replaying a deleted newline is a keystroke too), and an "are we pasting?" flag cannot see the cases it must exclude — middle-click PRIMARY paste calls `gtk_text_buffer_paste_clipboard` **directly** without emitting `GtkTextView::paste-clipboard` (`gtktextview.c:5597-5607`, measured here: the signal fired once in a whole session, for Ctrl+V only), and a failed clipboard read returns before `end-user-action` (`:3767-3769`), arming such a flag for the rest of the session. An RAII guard is not even available: measured, the `paste-clipboard` handler **returns before the first run is inserted**, so a scope guard drops before the window it was meant to cover.

GtkSourceView's `GtkSourceIndenter` is the sanctioned home for this and was implemented first — it is keystroke-only by construction — but it is **unusable from gtk-rs at sourceview5 0.10**: the subclass trampoline takes the caller's transfer-none `GtkTextIter*` with `from_glib_full` and frees GtkSourceView's own iterator on drop (`subclass/indenter.rs:107-110`). Measured: a single Enter SIGSEGVs with an **empty** `indent()` body. Regression guard: `pasting_highlighted_list_lines_keeps_every_line`, whose `ensure_highlight` call is load-bearing — without it the region copies as one run and the test passes on the broken code.

**Lesson**: **a signal payload describes the edit, never its provenance.** When a decision depends on *who asked* — the user typed this, versus the toolkit is replaying it — no test on the payload can recover that, and the closer the payload test gets to working the more dangerous it is, because it fails only on the inputs that carry structure. Move the decision to the layer that owns the intent (the key event) rather than guarding the layer that doesn't; a guard on a signal-level heuristic is still a heuristic. Two corollaries with teeth: mutating a buffer from inside a before-phase `insert-text` handler is only safe when nothing *else* is mid-mutation, and you cannot tell from inside the handler — GTK hands you its own cursor and trusts the contract; and an intermittent-looking corruption bug in a lazily-highlighted view should have "has the region been tagged yet?" as an early hypothesis, since tag toggles are what split a paste into runs.

## 200. `GtkSourceIndenter` is unusable from gtk-rs — the subclass trampoline frees the caller's `GtkTextIter`
> *Non-core (GtkSourceView + gtk-rs binding defect, not core GTK) — do NOT fold into the gtk4-rs skill; carried alongside #199, which is where the search that found it started. Measured on `sourceview5` 0.10.0 / GtkSourceView 5.4.1.*

**Symptom**: implementing `GtkSourceIndenter` — the sanctioned, keystroke-only home for auto-indent behaviour (`is_trigger(view, location, state, keyval)` + `indent(view, iter)`, `GTK_SOURCE_AVAILABLE_IN_ALL`) — SIGSEGVs the app on the **first Enter**, with no warning, no panic, and an empty stderr.

**Root cause**: the binding's subclass trampoline takes the caller's **transfer-none** `GtkTextIter*` with `from_glib_full`, so the Rust wrapper owns it and frees GtkSourceView's own iterator when it drops (`sourceview5-0.10.0/src/subclass/indenter.rs:107-110`):

```rust
let mut iter = from_glib_full(iterptr);      // ← takes ownership of a borrowed iter
imp.indent(&from_glib_borrow(view), &mut iter);
*iterptr = *iter.to_glib_full();             // ← leaks a copy; `iter` then drops → free()
```

Use-after-free in the caller. The related `IndenterExt::indent` is also commented out as `Unimplemented` in the auto bindings (`auto/indenter.rs:24`), so the interface cannot even be driven from a test — the mutable-`TextIter` signature is under-supported binding-wide.

**Resolution**: don't use the interface from Rust at this version. Do by hand what it would have done, from the same place: a `PropagationPhase::Capture` `GtkEventControllerKey` on the view (GtkSourceView installs its own capture-phase key controller for exactly this purpose, `gtksourceview.c:1442-1443`), mirroring what GtkTextView does around a committed Return — `reset_im_context()` before mutating, `scroll_mark_onscreen` after. That is what `src/window/editbar/newline.rs` ships. Accept the one thing the interface would have given for free: GtkSourceView routes through `gtk_text_view_im_context_filter_keypress` and uses an insertion counter to confirm a keystroke really committed rather than being absorbed into a compose sequence (`:4031-4090`); a hand-rolled controller does not, so a complex input method is the residual gap.

**Lesson**: the discriminator that saves the hour is **re-run the crash with an empty vfunc body**. A segfault inside a freshly written subclass reads as "my code is wrong" and invites a long bisect of one's own logic; if it still crashes doing *nothing*, the binding is the defect and the correct move is to route around the interface rather than keep debugging inside it. More generally, "the sanctioned API for this exists" is a fact about the C library, and a binding's presence in the crate is not evidence that its ownership annotations are right — check `transfer` on any vfunc that hands you a pointer you did not allocate.

## 201. A custom `harness = false` runner that ignores libtest's `--skip` turns a carve-out into a selection — silently, and green
**Symptom**: `packaging/windows/pipeline.ps1`'s step 5 ran `cargo test --features gtk-integration-tests -- --test-threads=1 --skip <known-failing-name>`, printed a pass, and exited 0 — having run **1 case of 149**: precisely the one it meant to omit, and nothing else. Every other case in the suite silently did not run on Windows.
**Scribobulate**: `src/gtk_suite.rs::parse_args` — an explicit `VALUE_FLAGS` list whose values are consumed before filtering, and `--skip`/`--skip=` honoured as a repeatable exclusion; guarded by its own `parse_args` unit tests.
**See**: general-engineering-principles (GEP-32).

## 202. A gate in front of a paint-carried dispatch: the settled state is the one that queues no paint

> *Core GTK4 (frame clock / adjustment / snapshot ordering). Strong candidate for the gtk4-rs skill; an instance of the deferred-work meta-pattern above, and the mirror image of ScrAP-125 — that entry is about reading paint-populated state too EARLY, this one about refusing to act on it and losing the event.*

**Symptom**: a programmatic navigation scrolls to an off-screen target and opens a popover on it, in two phases — a per-frame loop that re-aims the scroll as lazy line-height validation grows the scrollable extent, and a queued open-request that the *paint* fires the instant the target's hit-box exists. On one platform the document came to rest ~160 px short of the position the loop was aiming at. The target really was visible and the popover really did open; the view had simply stopped moving. The loop had exited through its "the request is gone, someone else owns the adjustment now" branch: the target became visible *before* the extent finished growing, so the dispatch happened against a partial aim and nothing re-aimed afterwards. **Visible is a weaker condition than converged, and the loop was treating dispatch as a reason to stop aiming as well as a reason to stop waiting.**

**What was tried**:
- **Resume re-aiming after the dispatch** — the obvious fix, and wrong for the reason the loop exits in the first place: the opened popover's re-pin guard now writes that same adjustment, and a second writer fights it over one slot. Rejected on the code, not by experiment.
- **Gate the dispatch on the loop reporting a settled extent.** This fixed the failing test and broke a different one, whose *precondition* began failing because the popover never opened at all. Instrumentation ruled out the obvious causes — the loop did settle, the gate did open — and named the real one: **nothing painted afterwards.** Settling means the extent stopped changing, which usually also means the loop's final write was a write of the value already held, and that queues no draw. The paint the dispatch rides on never happened, and the earlier paints that *would* have carried it had been refused by the new gate.
- **Adding an explicit repaint when the gate opens** closes that loop and does work. But it leaves the design resting on a further tick of the frame clock, and under a non-blocking pump the clock can go idle after a single tick — a gate whose opening is only observable on a *later* frame can simply never open.

**Root cause**: the fix filtered an event stream without supplying a replacement event. The completion signal was a paint; the gate refused every early paint; and the state transition that opened the gate was itself a no-op write that generated no paint. Worse, the gate's condition was *"the system has stopped changing"* — and a system that has stopped changing is, by construction, the one that emits nothing. The two halves are individually reasonable and jointly a deadlock.

**Resolution**: gate on a **local predicate about the state already in front of you**, not on a flag another loop must set on some future tick. Recompute the very aim the loop applies — the target's current best-known position, clamped to the scrollable range — and ask whether the adjustment is there yet. That is true or false about *this* frame, needs no extra tick to become decidable, and while it is false the request simply stays armed and the loop keeps aiming and keeps painting. Keep the clamp in the predicate: where the target cannot reach the top of the viewport the correct landing *is* the end of the document, so the goal stays satisfiable rather than unreachable. A wall-clock deadline still bounds the whole thing.

**Lesson**: **before putting a gate in front of a deferred request, ask what will re-provoke the event that request rides on once the gate opens.** A gate only ever removes deliveries; if the delivery mechanism is an event the system emits while working, the gate has to be paired with something that emits one more. Two specific alarms: a gate whose condition is *stability* (settled, converged, idle, quiesced) is asking to be woken by the absence of activity, which nothing sends; and an assignment of the value a property already holds is a no-op in most toolkits — it fires no change notification and schedules no redraw — so the "final" write in a converging loop is exactly the one that produces nothing. Prefer a gate that **reads current state and answers now** over one that **waits for a flag**: the first is a question about the present, the second is a bet on a future frame the framework never promised you.

**See**: gtk4-rs skill → deferred-work-and-ordering (GTK4Rs/AP-202).

## 203. Restoring `SIG_DFL` and re-raising inside a fatal-signal handler exits *normally* with status 139 — the signal is blocked for the handler's own duration
**Symptom**: the new SIGSEGV handler wrote a complete, correct crash report and the process died — but the forked-child regression test failed with *"child exited normally (status 35584)"*. `WIFEXITED` was true, `WIFSIGNALED` false, and the exit status was **139** — exactly what a shell reports for a signal-killed process (`128 + SIGSEGV`), so every casual observation of the crash agreed with the intended behaviour.
**Scribobulate**: `src/forensics/signal.rs::die` — the signal is unblocked between restoring the default disposition and re-raising it, so the process dies by the signal rather than exiting normally with its status.
**See**: general-engineering-principles (GEP-35).

## 204. Resolving a kernel segfault `ip` against `nm` output — the kernel's VMA base is the executable *segment*, not the ELF load base
**Symptom**: a kernel log line
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-36).

## 205. Predicting one platform's rendering from another's at the same toolkit version — the distributor's theme decides, not the version number

**Symptom**: a defect was reported against the Windows build by reasoning, correctly at every step, from the macOS build: GTK 4.20 deprecated `gtk-application-prefer-dark-theme` and moved theme-variant selection to `@media (prefers-color-scheme: …)` driven by `gtk-interface-color-scheme`, in which `UNSUPPORTED`/`DEFAULT`/`LIGHT` all evaluate light. The backend sets the new setting on neither platform. Both ship the same GTK version, past that threshold. Therefore Windows — like macOS before ScrAP-184 — must write the palette correctly and paint the window light.

Every premise was verifiable and verified. The conclusion was false.

**What was tried**:
- Reading the settings back. Useless in both directions — this is exactly the intermediate state ScrAP-184 records as staying correct while the window paints wrong.
- Comparing version numbers and the presence of the property. Both matched the prediction: the platform reports the same version, and the new property is present and sitting at `UNSUPPORTED`. Neither fact discriminates, because neither is where the behaviour lives.
- Rendering the window through the software renderer and sampling a pixel — the ScrAP-184 technique — on the platform in question. This settled it in one run: the legacy property alone paints correctly, both directions.
- Inverting the write as a control. Necessary: without it a passing sample is indistinguishable from a test that cannot fail, and the ambient desktop setting is not something a test may change. The control failed with the expected two colours, which is what makes the pass mean anything.

**Root cause**: whether a deprecated theme-variant property still works is decided by the **theme's `gtk.css`**, a data file each distributor builds and ships, not by the toolkit's version number. One vendor's default theme still routes the legacy property to a dark variant; another's has already been rewritten to the media-query form, where it is inert. The two are the same library version and disagree, so nothing derivable from the version — or from the presence and value of the new property — predicts the rendered result. The deprecation announces that support *may* end, not that it *has*.

**Resolution**: write both channels from a single shared writer that neither platform owns, so the version rule has one definition rather than one per platform — and give **each** platform its own pixel assertion rather than extending one platform's conclusion to the other. The shared write stands on the deprecation (the platform is one theme update from the other behaviour, with no signal in between), not on a defect; the test is what converts that from an assumption into something that fails loudly.

**Lesson**: **a version number is not a behaviour contract when the behaviour lives in a data file the distributor ships alongside the library.** Same-version builds may legitimately differ, so a cross-platform inference — however sound its premises — is a hypothesis about the other platform, never a finding on it. Test on the platform you are claiming about, assert the observable output rather than the state that produces it, and pair any environment-dependent assertion with a deliberately inverted control, because a check that cannot fail passes identically to one that cannot break. When measurement contradicts a prediction, the prediction's *rationale* has to be rewritten too: prose that still asserts the predicted failure is worse than none, since it reads as independent confirmation of it.

**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-205).

## 206. A reference gate whose pattern demands a file extension the codebase's citations never write — clean, green, and blind to every dangler of that shape
**Symptom**: `scripts/lint-references.sh` exited **0 with all six checks PASS** while **21 dangling plan pointers** sat in the tree — three retired plans cited 8, 12 and 1 times across `src/`, plus one in `sdd/TDD.md`. The plans had been retired and their files deleted; every pointer resolved to nothing. The commit that retired them recorded the sweep as complete, and `AGENTS.md` instructs that this check "is the only thing that can tell you the sweep is complete."
**Scribobulate**: `scripts/lint-references.sh` check 6a — the plan pattern matches the bare `PLAN.<topic>` citation form as well as the full filename, `.md` listed first so a whole filename still matches under both regex engines.
**See**: general-engineering-principles (GEP-2).

## 207. Two ports of one gate that share a pattern but not a file ENUMERATION — the parity claim is false, and the platform nobody runs is the lenient one
**Symptom**: `sdd/POLICY.md` stated that `lint-references.sh` and its PowerShell twin "share one pattern and one `--self-test`/`-SelfTest` corpus, string-for-string, so **neither can drift into being the lenient one**." The claim was false at the time it was written. A dangling link in `.agents/`, `docs/` or `THIRD-PARTY-LICENSES.md` **failed the Linux gate and passed the Windows one**.
**Scribobulate**: `scripts/lint-references.scan` — one enumeration definition that both the shell and PowerShell gates read rather than restate, exposed by `--list-scan`/`-ListScan`, with `maxdepth` as a hard tripwire rather than a filter.
**See**: general-engineering-principles (GEP-3).

## 208. A proc macro that moves the annotated item's attributes onto the generated BODY instead of the harness item — `#[ignore]` silently does nothing
**Symptom**: `#[gtktest::test]` + `#[ignore]` on a test body ran the body anyway, under **both** harnesses. No warning, no error; the author's quarantine was discarded in silence.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-33).

## 209. A guard test whose setup prevents the resource from ever existing cannot observe the leak it guards — it passes with the fix deleted
**Symptom**: `write_atomic` leaked its temp file on every failure path except a failed rename — `write_all` and `sync_all` returned through `?`, abandoning a randomised `.scribtmp` sibling next to the user's document on every full-disk or quota failure. The fix (a drop guard, so cleanup is the default and success the exception) came with a regression test that made the parent directory read-only, called `write_atomic`, and asserted no temp file was left. It passed. **It also passed with the guard's `remove_file` commented out.**
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-1).

## 210. Windows PowerShell converting a value on your behalf instead of failing — the call site reads correctly in every instance
**Symptom**: at least seven defects across the Windows gates, packaging scripts and documentation, each of which presented as something other than what it was.
**Scribobulate**: `packaging/windows/pipeline.ps1` — every conversion pinned rather than defaulted: `-Encoding` on both halves of a round-trip, quoting around native arguments containing braces, `$m.Success` tested before a capture is read, and `$LASTEXITCODE` checked deliberately.
**See**: general-engineering-principles (GEP-28).

## 211. A verification whose result nothing consumes — it reported the mismatch, and the corrupted payload was applied one line later
**Symptom**: a patch carrying a permanent register entry was applied to the tree despite failing its integrity check. The check ran, computed the right answer, and printed `got: 8354…` beside `want: 21a6…` — and `git apply` executed immediately afterwards regardless, because the two were written as sequential commands rather than as a condition and its consequence:
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-6).

## 212. `#[cfg(unix)]` on a test and "skipped on Windows" are indistinguishable in the report, and only one of them is true
**Symptom**: a documented behavioural rubric — containment must be decided on a symlink's *resolved* target — had a passing test, a checked-in fixture, and a manual checklist step. On one platform it had no coverage whatsoever, and every artifact reported success.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-4).

## 213. An artifact that describes what you meant to do, shipped beside what you actually did, and never reconciled
**Symptom**: a commit landed whose own message stated, in as many words, that one file was "deliberately NOT in this commit". The commit contained that file. Nothing failed, nothing warned, and every gate was green — the claim and the contents were simply never compared, because nothing in the toolchain compares them.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-21).

## 214. `backtrace_symbols`'s BSD twin is not the safe half of the pair — the async-signal-safety argument you inherited is about a different hazard
**Symptom**: none — this is a landmine marker rather than an incident, and it is recorded now precisely because the day it fires is the day nobody will re-derive it. A cross-platform review reported the fatal-signal handler's backtrace call as a live hazard on every platform. Measured on the macOS seat, it is not: the backtrace writer is gated to Linux/glibc, and every other target reaches a stub that records "(unavailable on this platform)" and calls nothing. The exposure below is INFERRED, not reproduced: it is what happens the day someone adds the missing platform arm.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-37).

## 215. Verifying a behaviour-preserving refactor with hand-written expectations tests your belief about the code, not the change you made
**Symptom**: a hot predicate was rewritten from a per-item backwards scan into a single bit carried forward — a change whose entire claim is that it computes the *same* answer more cheaply. The regression test written for it, asserting the transformed function's output on a handful of inputs chosen by hand, **failed on its first case**. The rewrite was correct. The expectation was wrong.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-8).

## 216. A gate that checks a citation EXISTS cannot see one that points at the wrong real thing
**Symptom**: a cross-register citation in a code comment named an entry about an unrelated subject. The comment described one lesson; the number beside it resolved to another. The reference lint passed. It had passed every run since the citation was introduced, and the citation had been introduced *by the sweep that existed to fix exactly this class of ambiguity*.
**Scribobulate**: `lint-references` check 8 plus this file's citation convention — the two legal forms are single unique tokens and the ambiguous bare form is illegal, so a citation's register is decided by its text rather than by when it was written.
**See**: general-engineering-principles (GEP-24).

## 217. A negative result is worthless without a positive control — "it was prevented" and "I cannot see it" produce identical output
**Symptom**: a containment property — a document-supplied URI must never reach the system launcher — rested on one hop nobody could settle by reading the code, because it is a claim about a toolkit's signal-emission semantics rather than about the application. The obvious experiment is to arm the guarded path, trigger it, and check that nothing launched. Run that way it returns "nothing launched" whether the guard works, whether the trigger never fired, whether the observation window closed too early, or whether the detector was watching the wrong thing.
**Scribobulate**: the positive control accompanying every negative result in `packaging/windows/pipeline.ps1`'s verification steps — the same probe re-run with the guard removed, required to show the effect.
**See**: general-engineering-principles (GEP-12).

## 218. Confidence ratchets across a relay — the hedge is dropped by whoever summarises, and nobody does anything wrong
**Symptom**: a note recording that one directory had not been reached by any reviewer was relayed one hop and arrived as a work assignment to give that directory a full review. It was neither — the directory turned out to be covered at five points by findings already in hand, four of which belonged to consolidations that had been deliberately deferred. The relay was caught only because the receiving side asked the originator to re-check their own claim, which they did by grepping their sources instead of trusting their summary.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-19).

## 219. A remedy that lives inside one consumer reaches the consumers that already knew about it
**Symptom**: a recorded remedy — replace a compile-time platform exclusion on a test with a platform-appropriate implementation plus a **runtime** skip that can be printed, counted and grepped (#212) — had been written, reviewed, and landed. A later audit found it applied at **two of five** eligible sites. The other three still carried the exact construct it replaced, and were therefore still deleted rather than skipped on the platform nobody could check. No one had disagreed with the remedy, argued against it, or granted an exception.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-25).

## 220. A regression guard built from the instance you fixed has coverage exactly equal to the fix
**Symptom**: a super-linear scan was found in a parser, fixed, and shipped with a growth-ratio regression test that asserted the exponent rather than a wall-clock bound — careful work, the right property, machine-independent. An independent audit then found the **same** quadratic, in the **same** function, still live under a different delimiter of the same family. The new test passed with the survivor fully intact.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-5).

## 221. A comment explaining why a test asserts less than its name promises is where a false premise hides
**Symptom**: a test named for refusing an over-limit input wrote a **one-byte** file, asserted it was **accepted**, and then asserted the wording of a refusal it constructed by hand. The branch it existed to cover was never executed by anything. It sat in the module written specifically to be the single home of that policy, in the round that minted the register entry about vacuous assertions.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-9).

## 222. Two gates, each correct, enforcing opposite things — and neither can see the other
**Symptom**: the policy document instructed developers to use an attribute that the repository's own lint gate rejects outright. A reader who followed the written rule broke the build; a reader who satisfied the gate was contradicting the written rule. Both artifacts were confident, both were internally consistent, and they had been disagreeing for as long as the gate had existed.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-26).

## 223. Write a finding as a testable proposition, not as a conclusion — a conclusion recruits agreement, a proposition recruits a measurement
**Symptom**: a reviewer reported that a governance property could not be established — single-seat authorship of a shared document was, they said, unverifiable because the history had been squashed. The claim was accepted, in writing, twice. It was also wrong: the pre-squash tips survived in the reflog, and recovering them took four commands. The recovery then found an actual violation of the governance rule in question, which nobody had been looking for and which no other artifact would ever have surfaced.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-20).

## 224. A squash makes single-seat authorship unprovable — on a deadline nobody is watching
**Symptom**: a governance rule said an agent must not autonomously add a rule to the policy document. Whether that rule had been kept could not be answered, because the commits that would answer it had been squashed — three seats' work collapsed into one commit per review round, authored and dated as a single act. A claim that one seat had touched only one section of the document was accepted twice, in writing, because nothing available could check it.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-22).

## 225. Four denial-of-service paths in four subsystems were one omission: nobody had said the project had an opinion about input size
**Symptom**: a review round found four unrelated ways an ordinary-looking document could hang or kill the application — an unbounded recursion that aborted the process from a ~1.1 KiB file, two super-linear scans that froze the UI for minutes on a few megabytes, and an unbounded read. They sat in four subsystems, written by different hands at different times, none of them careless.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-27).

## 226. The check your self-test does not cover is the one that ships broken — and a single-file corpus cannot falsify a multi-file bug
**Symptom**: a lint check was added, mutation-tested against both real defects it existed to catch, run green, cross-checked in a second language, and shipped. It contained two bugs. Every reported line number was correct and every reported **filename** was wrong — always the *next* file in the list, so only the last entry in the set read correctly. Worse, the same root cause silently swallowed real hits: a later file mentioning the search term suppressed an earlier file's finding entirely, turning the check into a false negative.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-3).

## 227. A two-axis hazard gated on one axis — the seam passes, and the assertion it exists to prevent fires through it

**Symptom**: `gdk_monitor_get_geometry: assertion 'GDK_IS_MONITOR (monitor)' failed`, three times, with the popover content allocated a NEGATIVE height — the precise ScrAP-26 failure the `saferizer::popover_anchor` seam was built to make unreachable, arriving *through* the seam. Reproduced from ordinary document content: the editor is `WrapMode::Word` (`window/tabs/lifecycle.rs`), so a long unbroken token overflows horizontally; selecting that line gives `x0=2 x1=36002`, midpoint **18002** in a 600 px view. `ViewportRect::at` returned `Some`, `pin_above` pointed there.

**Where Scribobulate implements the fix**: `src/saferizer/popover_anchor.rs` — a `Viewport` value carrying **both** extents, one `axis_visible` predicate applied to each, `CARET_SLIVER_W` shared between the gate and `pin_above` so the rectangle checked is the rectangle pointed at, and `saturating_add` in the predicate. Callers construct it with `Viewport::of(widget)`. Guards: `the_x_axis_is_gated_exactly_as_the_y_axis_is`, `a_horizontally_overflowing_selection_midpoint_does_not_anchor` (the measured reproduction as a predicate case), `an_overflowing_anchor_coordinate_saturates_rather_than_wrapping`. MUTATION-TESTED: restoring the y-only conjunct fails 5 of 6 bodies in the module.

**Root cause, and the part worth carrying**: the seam was *correctly* built — `ViewportRect` has no public fields, no derives, one `impl`, no back door, and its clamp is total. A reviewer checking whether it could be forged would find nothing wrong, and QA's audit said exactly that. **Unforgeability is a claim about CONSTRUCTION; it says nothing about COVERAGE, and at a call site the two are indistinguishable** — the call site sees a type that means "proven on-viewport" and stops checking, which is the seam working as designed. So a coverage gap in a trusted seam is strictly worse than no seam: every site that would have hand-checked has stopped.

The false premise was *in the seam's own doc comment* — "only the y is gated and clamped, because only the y can leave the viewport under scrolling". It is true of vertical scrolling and false of horizontal overflow, and it had been read and approved for two rounds because it reads like a statement of the constraint rather than an assumption about it. Sibling of #220: the guard's inputs were derived from the scroll case that motivated the fix, so the population it ranged over excluded the only inputs that discriminate.

**The corrective, generalised**: when a hazard has N axes, take the extent as ONE value covering all N (`Viewport`), so a gate that silently omits one is not expressible — the same move as passing a struct instead of two adjacent `i32`s, and for the same reason. Gate the shape you HAND to the toolkit, not the shape you derived it from. And treat a doc comment that explains why a check is narrower than the hazard as an unverified claim inside the seam (#221).

*Core GTK4/gtk-rs — sharpens GTK4Rs/AP-26 and belongs in the `gtk4-rs` skill.*

## 228. A property implemented on one branch, documented as a property of the whole function

> *Non-core (invariant-placement discipline, not GTK) — do NOT fold into the gtk4-rs skill.*

**Symptom**: fixing an unrelated format ambiguity made `the_marker_forgets_reports_that_no_longer_exist` fail — a test that had passed since it was written, over code the fix did not touch.

**Root cause**: `announce_unread_report`'s comment claimed the seen-marker was "pruned to reports that still EXIST, which is what keeps a set-valued marker bounded". The pruning lived in `seen_set`, and only in its **legacy-watermark** branch, where it is structural — a watermark is *evaluated against* the present set, so filtering by it is how that branch works at all. The set branches returned their lines unpruned. This was invisible because the steady state — one crash, one line — took the watermark branch, so the bound was being delivered as a side effect of the format **ambiguity that was itself the defect under repair** (QA round 5, M-2). Disambiguating routed the steady state to the set branch and the marker began growing without limit.

**Where Scribobulate implements the fix**: `src/forensics/report.rs` — `seen_set` applies one `extant` predicate on every branch and owns the bound in its own doc comment; the writer's comment now points at it rather than restating it.

**The lesson**: the tell is a comment at the **writer** asserting a bound the **reader** implements. Nothing connects them, so the claim is unfalsifiable at the site that makes it and unmotivated at the site that satisfies it. State and implement an invariant in one place. And when repairing a discriminator, ask explicitly which properties were riding on the branch you are about to stop taking — a fix that changes which path the common case follows silently re-tests every property that path was carrying. Kin to #222 (a rule filed where it was discovered rather than where it is needed) and #226 (apparatus sharing a blind spot with the code).

## 229. A seam named for a guarantee it delivers on one platform — and the permission model that lives in the directory, not the file
**Symptom**: TDD 21.12 ("a crash report is readable only by the user who ran the application") was satisfied by `private_options()` — an `OpenOptions` constructor applying `0600`, created precisely so a mode would not have to be remembered at N call sites. It was believed to hold everywhere. It held on neither platform completely.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-38).

## 230. A clippy method ban does not cover the builder property of the same name

> *Non-core (Rust tooling / enforcement discipline, with a gtk-rs builder-idiom trigger) — do NOT fold into the gtk4-rs skill as a GTK lesson. The transferable half is about what a `disallowed-methods` matcher can and cannot see; the GTK part is only the idiom that makes the gap likely.*

**Symptom**: `WidgetExt::set_tooltip_text` was banned in `clippy.toml` so that every control had to be routed through `src/a11y.rs`, which sets the accessible name and the tooltip together. `cargo clippy --all-targets -- -D warnings` passed. Three toolbar controls nonetheless carried a tooltip and **no accessible name** — the exact defect the ban existed to make impossible — because they were built as `gtk::MenuButton::builder().tooltip_text("Documents")…build()`.

**What was tried**: nothing failed, which is the point. The ban was written, verified to fire (the four in-module calls needed `#[allow]`), and the whole tree converted. `-D warnings` was green on a tree that still contained the defect. The gap was found only when a separate tree-walk integration test — added for coverage, not because the ban was doubted — walked a live window and reported three unnamed controls.

**Root cause**: `disallowed-methods` matches a **path**. `gtk4::prelude::WidgetExt::set_tooltip_text` and `gtk4::builders::MenuButtonBuilder::tooltip_text` are different paths to the same effect, and the builder form is generated per widget type, so a complete ban would need one entry per builder in the crate rather than one entry per contract. The builder idiom is also *more* likely exactly where the risk is highest: a widget that needs three or more properties at construction is the one a developer reaches for `builder()` on.

**Resolution**: treat the ban and a runtime assertion as **one mechanism with two halves**, not as a primary gate plus a nice-to-have.

- The ban stops the common spelling at write time, where the feedback is cheapest.
- A live assertion over the **built object** — not over the source text — closes every spelling at once, because it tests the effect rather than the call. Here: walk a real window's widget tree and assert every icon-only control and label-less field has an accessible name (`gtk_test_accessible_has_property`, present at the 4.6 floor). It is the half that actually found the defect.

The general form: **an enforcement tool's coverage is a property of its matcher, not of its intent.** Before trusting a ban to make a contract unforgeable, ask what other spellings reach the same effect — a builder, a `.set_property("tooltip-text", …)` string, a UI-file attribute — and put the assertion where they converge. Kin to #219 (the enforcement ladder: this is why the top rung is not the last word).

**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-130).

## 231. Retiring an ambiguous citation form by LEGALISING it instead of banning it — and a completeness claim with no predicate
**Symptom**: the convention paragraph said the legacy citation forms "were swept out of `src/`, `tests/`, and this file's cross-register citations, so the convention now holds tree-wide." Nothing contradicted it — every gate was green, and `ScrAP-N` was in use everywhere one looked. **443 bare `AP-N` citations were in the tree at that moment** (310 in `src/`, 127 in `sdd/`, 6 in `tests/`), plus one in `data/` that no one had thought to look in.
**Scribobulate**: `scripts/lint-references.{sh,ps1}` check 8 (the per-site migration rule and the audit's limits are documented at the check, next to the gate that enforces the form); the citation-convention paragraphs at the top of this file and at "Numbering reconciliation"; POLICY step 9. Retires the citations-registration plan (its analysis lives on in git history).
**See**: general-engineering-principles (GEP-24).

## 232. `g_file_replace_contents` is atomic only under the right flags — and its one remaining fallback deletes the previous file before failing
**Symptom**: none yet — this is a trap found at **design time**, before a line of it was written, which is the only reason it is cheap. The design under review was a crash-recovery swap file: a dirty editor buffer snapshotted periodically to the state directory, whose entire purpose is that the copy on disk is trustworthy after an unclean exit. `g_file_replace_contents_async` looks made for it: one call, "replace", asynchronous, and a `G_FILE_CREATE_PRIVATE` flag that appears to answer the permissions question. Adopting it on that reading would have produced a mechanism that is *usually* atomic, *sometimes* silently truncating, *never* durable across power loss, and — in one failure mode — actively destructive of the very snapshot it exists to preserve.
**Scribobulate**: `src/window/swap.rs` owns the promote — co-located `<name>.swap.tmp` opened with `replace_async` (`PRIVATE`), renamed into place only after a complete write; three tests pin it, one characterising the cancelled-close GLib branch. Format: SCHEMA.md § "Crash-recovery swap file". Contract: TDD §22.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-167); researcher findings — `~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-replace-contents-atomicity-durability-threading.md` (GLib 2.72.4, diffed to 2.89.3). Kin: #229 (a seam named for a guarantee it delivers conditionally).

## 233. Delegating a delimited format's unforgeable-terminator invariant to a third-party serialiser's escaping

**Symptom**: A frontmatter-style file format — a magic line, a TOML metadata block, a
bare `+++` terminator, then a verbatim payload — silently truncated its payload when one
metadata value (a filesystem path) contained a newline. The round-trip test written
specifically to pin the hazard failed on its first run; without it the format would have
shipped, and the loss would have surfaced only for a user who both crashed *and* had an
unusual filename, i.e. in the one code path whose entire purpose is to be trustworthy
after a crash.

**What was tried**:
- The design document reasoned it through and concluded no work was needed: *TOML basic
  strings escape `\n`, so a correct serialiser upholds the invariant automatically.* The
  reasoning is sound about the TOML **specification** and simply does not describe the
  **crate**. It was written down as a settled point with a test noted as "worth having" —
  which is the shape to distrust: a hazard identified precisely, then closed by
  attributing the guarantee to somebody else's code.
- A `debug_assert!` in the encoder. Wrong twice over: it compiles out of the release
  build that ships, and it treats the condition as an assumption to document rather than
  an input to validate.

**Root cause**: The `toml` crate (0.8) chooses between basic, literal and **multi-line**
string forms by an internal heuristic, and for any value containing a newline it selects
a multi-line basic string — whose defining purpose is to reproduce those newlines
verbatim:

```text
path = """
/tmp/evil
+++
not-the-body.md"""
```

That is a forged closing fence sitting inside the header. The parser stops at the first
`+++` on its own line, as designed, and everything after it — the user's actual unsaved
work — is discarded. The serialiser is not misbehaving; it never promised to preserve a
line-oriented property of some enclosing format it knows nothing about. The defect is
the assumption that it had.

The general shape: **a delimited format's terminator is unforgeable only if something
guarantees no value can reproduce it. A length-prefixed format gets that for free; a
delimited one has to enforce it.** Delegating that enforcement to a library's escaping
choices makes the format's core safety property a function of a dependency's internal
heuristic — unversioned, undocumented as a contract, and free to change in a patch
release.

**Resolution**: Enforce it twice — by construction, then by verification.

1. **Construction**: escape line breaks out of every externally-derived string *before*
   it reaches the serialiser (`\` → `\\`, LF → `\n`, CR → `\r`), reversing on the way
   back. A value with no raw newline cannot select a multi-line form, whatever the
   heuristic decides. Apply it to *every* string field through one explicit
   field-by-field transform, so adding a field does not compile until it has been
   considered — not to "the ones that could contain a newline", which is a judgement the
   next author re-makes from scratch.
2. **Verification**: after serialising, check that no header line equals the terminator
   and return a real `Err` if one does. With (1) in place this is unreachable — which is
   exactly the point. Its job is to convert a future change in a dependency's escaping
   behaviour into a loud failure instead of a truncated document.

Both halves were mutation-tested: neutering the escaping fails three tests, including one
that asserts the *mechanism* (no header line forges the fence) and not only the
*consequence* (the round trip survives). The mechanism assertion is the one that keeps
holding if the splitting logic is later made accidentally tolerant.

**Lesson**: **When you write down that a hazard is handled *by someone else's code*, that
sentence is a hypothesis with a test attached, not a conclusion.** The tell is a design
note that identifies a risk precisely and then discharges it by appeal to an upstream
guarantee — the precision of the analysis lends unearned credibility to the hand-off at
the end of it. Two specifics worth carrying:

- **A specification is not an implementation.** "TOML escapes newlines in basic strings"
  is true of TOML and says nothing about which string form a given serialiser picks.
  Every "format X handles this" claim needs re-reading as "the library I am calling
  handles this, in the version I have pinned" — and that is a five-minute experiment,
  not a five-minute argument. (Kin to GTK4Rs/AP-162: a documented standard's name in an
  API's vocabulary is not a contract that the API implements it.)
- **Enforce an invariant on the side you control.** The input to a serialiser is yours;
  its output formatting is not. Escaping before the call is a property of your code that
  no dependency upgrade can revoke, while any argument of the form "it will encode this
  safely" is a standing bet on a component that never agreed to the terms.

**Scribobulate**: `src/swapfile/codec.rs` — `to_wire`/`from_wire` (construction) and the
`encode` fence check (verification); the invariant is stated in the module doc. Sibling
of #232, which came from the same feature: both are cases of a convenience API's
advertised behaviour being narrower than its name.

---

## 234. Asserting one of a feature's two representations, and reading the green suite as evidence about both
**Symptom**: A crash-recovery pass restored a document's unsaved content correctly — the
editor pane showed exactly the pre-crash text, marked unsaved, and every one of 856 tests
passed. On a live display the *preview* pane showed the pre-crash **file** instead: the
recovered work was invisible in the application's default view mode. Nothing warned, no
test failed, and the two panes disagreed with each other.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-10).

## 235. Wiring a startup feature into one framework entry point and assuming it covers launch

**Symptom**: Crash recovery worked perfectly when the application was launched bare, and
did nothing at all when launched with a file argument — no notice, no recovered tab, and
that run's log never mentioned the snapshot. The snapshot itself survived untouched, so a
later bare launch still recovered it; the *offer* simply never appeared at the moment the
user expected it.

**What was tried**: nothing, which is the point. It was never observed locally. Every
automated test reached the feature through the code path that worked, and manual
verification used a bare launch and session restore. The defect was found by a peer seat
on another platform running the same feature's smoke test, and it turned out to be in
shared code rather than in that platform's port.

**Root cause**: the toolkit's application object dispatches a launch to **different
handlers depending on its arguments** — GApplication calls `activate` for a bare launch
and `open` for one carrying files. The recovery pass had exactly one call site, in
`activate`. So the feature was not degraded on the `open` route, it was **absent** from
it, silently and totally.

Two things made it easy to miss and worth naming:

- **The entry point you develop against is the one that works.** A developer iterating on
  a startup feature launches it the simplest way, repeatedly. That single habit decides
  which dispatch path gets exercised hundreds of times and which gets zero.
- **The unexercised route was the one that mattered most.** Double-clicking a document, a
  desktop-file association, `xdg-open`, `open -a`, a shell invocation with a path — the
  overwhelmingly common way a user opens the document they were working on. For a recovery
  feature specifically, that is precisely the launch where the offer needs to appear.

**Resolution**: capture the cold-start signal at each handler's entry — *before* anything
creates a window — and route every entry point through **one gated helper whose doc
comment enumerates its callers and states that a missing one is a total failure for that
route**. Then assert the *effect* through the real handler (build the application with the
production flags and wiring, invoke `open` with a file, and check the work came back)
rather than asserting that some function was called: a later refactor should be free to
move the call and must not be free to drop it. Mutation-tested by deleting the new call
site.

**Lesson**: **enumerate a framework's entry points as a set before wiring anything
startup-scoped, and treat *it works when I launch it* as a claim about exactly one dispatch
path.** Frameworks routinely fan a single user action out across several callbacks chosen
by argument shape, environment, or platform convention — `activate` vs `open`, a
URI-scheme handler, a service/daemon mode, the re-activation of an already-running
instance. Coverage of one is evidence about one. The corollary for reviewers: when a change
adds behaviour *at startup*, the first question is **which startups?** Kin to #234 (a
suite is evidence about the surfaces its assertions name) — same failure of scope, one
about assertion targets, this one about dispatch paths.

---

**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-235).

## 236. A screen-coordinate capture is not a window capture

> *Verification tooling / Windows harness mechanics, not GTK. Non-core: do NOT fold into
> the `gtk4-rs` skill. Authored by the Windows seat; allocated and landed by the register
> writer, edited for format and not for argument.*

**Symptom** — MEASURED. A UI harness captured a specific application window by taking its
rectangle from `GetWindowRect(hwnd)` and calling `System.Drawing.Graphics.CopyFromScreen`
over that rectangle. The PNG came back **well-formed, correctly sized, and showing a
completely different application** — the agent's own session window, which happened to be
in front at those coordinates. Nothing errored. The capture succeeded, the file was valid,
and the image was confidently, entirely wrong.

**What was tried, and why the failure stayed invisible**: the same call had worked all
session. Every earlier capture followed an `AppActivate` that had brought the target to
the foreground, so the coordinates and the pixels happened to agree. The bug appeared the
first time a window was captured that had deliberately **not** been activated — a
background second instance during a two-instance test. The mechanism was load-bearing and
undetectable for exactly as long as focus coincided with intent.

**Root cause**: `CopyFromScreen` reads the **desktop framebuffer at those coordinates**
and knows nothing about the window whose rect supplied them. `GetWindowRect` answers
*where is this window*, not *what does this window look like*. Composing the two reads as
a window capture while being a screen capture with extra steps — so any occluding window
is photographed instead, and the intended target need not even be visible.

**Resolution**: activate, settle, then capture — and gate the capture on the activation's
**result**, not merely the typing:

```powershell
$ok = (New-Object -ComObject WScript.Shell).AppActivate($pid)
if (-not $ok) { <abort — do not capture, do not type> }
Start-Sleep -Milliseconds 900
# GetWindowRect + CopyFromScreen only now
```

`AppActivate` returns a boolean, and treating it as fire-and-forget is what lets both the
keystroke path *and* the capture path silently address the wrong window.

**A second measured instance says the boolean is necessary and not sufficient** (Windows
seat again, during the ScrAP-292 verification): the activation succeeded, and the
operator's own Git client came forward **during the 900 ms settle**, so the capture
photographed that instead — same well-formed, confidently-wrong artefact, from a harness
already obeying the rule above. A check at activation time answers *did I bring it
forward*, which stops being true the moment anything else asks for focus; the capture
needs *is it forward NOW*. So re-assert it at the capture itself — `GetForegroundWindow()`
must equal the process's `MainWindowHandle`, else refuse to capture — and note the tell
that caught it: the log evidence and the screenshot disagreed. On a shared, live desktop,
**verify a precondition at the moment it is depended on, not at the moment it is
established** — every gap between the two belongs to whoever else is using the machine. Capture the
**window rect only**, never the full desktop: on a real operator's machine a full-screen
grab also photographs their unrelated applications, so this is a privacy property as well
as a correctness one.

**Lesson**: a verification harness that reads the right coordinates from the wrong layer
produces **a plausible artefact of the wrong subject, which is strictly worse than an
error** — an error stops you; a wrong screenshot gets believed and reasoned from. The
general rule for any capture or observation step: **the thing that supplies the
coordinates and the thing that supplies the pixels must be the same object**, or something
must guarantee they still agree at the moment of capture. This is #193's shape one level
down — there a harness *gap* read as an application defect; here the harness *succeeds*
and misidentifies its subject. Applies to any screen-capture-based verification, including
X11 `import` if it is ever pointed at a rectangle rather than a window id.

---
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-236).

## 237. A `cfg`-gated gate proves nothing about the branches it did not compile
**Symptom**: `cargo clippy --all-targets --features gtk-integration-tests -- -D warnings`
— build-pipeline step 2, mandatory, run on every change for the life of the project — had
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-4).

## 238. Activating on a click gesture's `released` alone — the release that ends a drag is not a click

> *Core GTK4 event-controller semantics (`GtkGestureClick`), plus the enforcement shape.
> Reported by the operator against the preview pane; found to affect three affordances.*

**Symptom**: swipe-selecting a passage of rendered text, the pointer happens to come to
rest over a hyperlink. On the mouse-up the link **activates** — the browser opens, or the
pane scrolls away to a fragment — although the reader never clicked it; they were
selecting. Measured on the pre-fix release build under Xvfb: a drag begun on blank space
right of a link and released on the link scrolled the pane to that link's target, pixel-
identical to a deliberate click on it.

**Root cause**: `GtkGestureClick` emits `pressed` and `released` as two independent
signals and **pairs them for nobody**. A handler on `released` therefore answers "was the
pointer over X when the button came up", which is not the question — every drag ends with
a release somewhere, and in a text pane dragging *is* the ordinary gesture. The affordance
had no notion of where its own click began.

The widget GTK ships for this does the bookkeeping in its own C:
`gtk_label_click_gesture_pressed` records the link under the press and sets `link_clicked`
only if there was one; `gtk_label_click_gesture_released` activates only when that flag is
set **and** `selection_anchor == selection_end` (`gtklabel.c`, 4.6.9). Two operands — and
the second is the one a reader stops before, having already found the confirmation they
were looking for. It is load-bearing: without it, a press and release inside a *single*
link is indistinguishable from a click, which is exactly what selecting a long link
caption to copy it looks like. That second half was measured too: on the pre-fix build a
swipe from one end of a link's caption to the other navigated.

**What was tried** (and why the obvious guard was dropped):

- *Track the press and require the same target* — necessary, and the whole of the reported
  case, but it leaves the swipe-within-one-link half live.
- *Copy GTK's second operand literally — refuse when the buffer has a selection.* Rejected
  after reading `gtktextview.c`: a press landing **inside** an existing selection is
  deliberately **not** resolved at press time (it may begin a drag-and-drop of the selected
  text) and is settled only when the drag gesture ends. So "is something selected right
  now", read at release, is not the same predicate in a `GtkTextView` that it is in a
  `GtkLabel`, and a guard built on it would refuse legitimate clicks.
- *Bound the pointer's travel instead* — press-to-release distance against the desktop's
  own `gtk-dnd-drag-threshold`, which is GTK's click-versus-drag boundary everywhere else.
  Independent of any widget's internal selection timing, and it expresses the actual rule:
  a click is a press and a release in the same place, on the same thing.

**Resolution**: one seam (`saferizer::ClickActivation`) that owns **both** connections —
callers hand it a hit-test returning the target's *identity* and an activation, and never
write either signal handler. Identity, not a boolean: two occurrences of one URL are two
links, and a hit-test answering `bool` cannot express "the same one". Options for the two
axes that legitimately vary (claim-the-sequence-on-press; a release slop for targets a few
pixels wide; a travel bound) default to the strict reading. `GestureClick::connect_released`
is then banned in `clippy.toml` with the seam's single `#[allow]` inside it, so the
release-only shape does not compile — the ScrAP-116/ScrAP-129 enforcement ladder, taken to
its top rung, because the alternative is a rule every future affordance has to remember.

Verified by driving the release build under Xvfb against the pre-fix binary as a control:
drag-onto-link, press-on-link-A/release-on-link-B and swipe-within-one-link all select and
**do not** navigate, while a plain click still does; a drag released over a task checkbox
no longer toggles it and one released over a margin comment marker no longer opens its
card, with plain clicks on both still working.

**A second GTK fact, measured while pricing the one case this does not cover**: a
`GtkGestureClick` whose sequence has been **claimed by another gesture** emits `pressed`
and then *nothing* — `gtk_gesture_click_end` gates the release emission on
`state != GTK_EVENT_SEQUENCE_DENIED` (`gtkgestureclick.c`), and a denial is not a
cancellation, so `cancel` does not fire either. There is no signal at all to hang a
fallback on. `GtkTextView` uses exactly this to reserve a press that lands **inside** a
selection for a drag-and-drop of the selected text (`gtk_text_view_click_gesture_pressed`,
"a special case to start DnD", unconditional for a non-touch single press), which is why a
first click on an affordance under a selection does nothing anywhere in this app. So: a
click affordance in a `GtkTextView` is not merely un-paired, it is **pre-emptible without
notice** — budget for the press whose release never arrives, and do not build a mechanism
that assumes every `pressed` is eventually resolved.

**Lesson**: **a "click" is a pairing, and no toolkit hands it to you from one signal.**
When a framework exposes press and release separately, the handler you write on release
alone is not answering "did the user click this" — it is answering "where did some
gesture happen to end", and those coincide only when nothing was dragged. Two consequences
worth carrying: (1) when a toolkit ships a widget that does the same job, read *its*
predicate before writing your own — and read **all** of its operands, since the one you
stop before is reliably the one covering the case you had not thought of; (2) three
affordances in this app had drifted to three different answers to one question (the
checkbox tracked its press, the link and the marker did not), which is what an unwritten
rule looks like from the outside — the fix is not to correct the copies but to remove the
possibility of having them. Sibling of #79: both are a gesture firing for an event that
was never a click on that thing.

**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-169).

## 239. `git stash pop` restores the source, not the binary — a control run that silently drives the old build
**Symptom**: a fix verified green by unit tests, two mutation-tested integration tests
and one driven Xvfb scenario appeared to **fail** the moment a second scenario was added.
Worse, it failed *identically* on the pre-change binary and the fixed one — the notice
stayed on screen in both columns — which reads as "the fix does not work", and, since the
first scenario had already passed, as "the fix works for one path and not the other". Four
drive cycles went into hunting a defect in the shipped code, ending at an
`eprintln!`-instrumented build which proved the timer fired and the retraction ran.
**Scribobulate**: `tests/MANUAL-TEST.md` §1.7 — the control build is copied and named explicitly before the fix is applied, rather than assumed to still be on disk.
**See**: general-engineering-principles (GEP-14).

## 240. A detector that enumerates the VOCABULARY of a free-text citation is defeated by a synonym
**Symptom**: an illegal cross-file issue citation — the register's name, the word `item`,
and an entry letter, in a doc comment — sat in `src/` through a clean
`scripts/lint-references.sh` run. Check 1 exists for exactly that citation form and
reported PASS. Worse, the citation was **born dangling**: the very commit carrying it
rewrote the entry it named, and that entry disappears entirely once its last platform
lands. (This write-up names no entry letter for the same reason — and the gate flagged the
draft that did, which is the rule enforcing itself on its own documentation.)
**Scribobulate**: `scripts/lint-references.sh` check 1 — the pattern matches the *shape* of a reference rather than enumerating the connecting nouns a citation might use.
**See**: general-engineering-principles (GEP-2).

## 241. A process NAME is not an identity — pid reuse defeats every liveness probe, on every platform
**Symptom**: none yet — this is a limitation recorded before it bites, which is the only
useful time to record one. The crash-recovery scan skips a snapshot whose `owner_pid` is a
live instance of this app, so two instances never fight over each other's unsaved work.
"Live instance" is answered by reading the process's executable name: `/proc/<pid>/comm` on
Linux, `proc_pidpath` on macOS, `QueryFullProcessImageNameW` on Windows.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-17).

## 242. `clippy --all-targets` WITHOUT the feature flag reports dead-code errors in files you never touched
**Symptom**: `cargo clippy --all-targets -- -D warnings` fails with two dead-code errors —
`a11y::has_name` and `PreviewFindCache::builds` — in modules the current change never
touched. It reads as "your change broke two unrelated files", which is the most expensive
possible framing: it sends you to read code that is not the problem.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-4).

## 243. GLib's I/O thread pool is one process-wide pool of ten — moving I/O off the main thread makes it contend with the crash-recovery writer
**Symptom**: moving document reads and writes off the GTK main thread with `gio::spawn_blocking` cured the freeze and broke nothing visible — but the crash-recovery snapshot writer's `replace_async` goes through the SAME pool, so a slow or unresponsive filesystem now delays the one mechanism that protects unsaved work, at exactly the moment the filesystem is misbehaving.
**Scribobulate**: `src/docio/pool.rs` bounds this project's own use at the source — `MAX_CONCURRENT = 4` admitted operations, a FIFO of waiters, a `Slot` released on `Drop`; its module doc carries the measurements and `sdd/TECH.md` § Concurrency model the consequence. Unit-tested headlessly (a plain future over a `thread_local`, so it needs no display), and those tests caught a real double-release in the hand-off path. The per-document in-flight gate beside it (#244, `winstate::WriteGate`) is REQUIRED rather than belt-and-braces, because completion order is not dispatch order. Not taken, and recorded because it is the right fix under a different rule: a dedicated thread for the snapshot writer, immune to all of this and the only option that holds when document I/O hangs unboundedly (NFS), declined against POLICY's "the application owns no threads" — if that rule is ever revisited, this is the case that should reopen it.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-243). Findings: `~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-task-thread-pool-sharing-starvation.md` (GLib 2.72, rig `_src/gio-taskpool-starvation/`). Kin: #244.
## 244. Making a window-scoped operation async turns "which tab is active?" into two different questions
**Symptom.** `save_window` resolved its target with `state(window)` — the window's
active tab. That was exact for as long as the write was synchronous, because
nothing could change which tab was active in the middle of a call. Once the guard
read and the write were dispatched to a thread pool, the main loop ran between
them, and the *same expression* asked before and after answered about two different
documents: the guard could check tab A's file for an external change and then
authorise a write of tab B's buffer. The conflict protection (C2) was defeated
without a symptom.
**Where Scribobulate implements the fix.** `src/window/save.rs` — every step takes
an explicit `Rc<TabState>`, resolved once when the user acts and carried through
the read, the dialog and the write. `save_window` additionally splits its
completion into **tab-scoped** work (`sync_tab_swap`, `badge_tab_label` — must
target the captured tab) and **window-scoped** work (`refresh_dirty_status`, the
toast — must target whatever is on screen). `src/window/reload.rs` does the same
with an explicit active-vs-background split after its read.
**See**: general-engineering-principles (GEP-43).

## 245. An Xvfb UI drive can deliver nothing and look exactly like one that delivered everything

**Symptom.** A driven verification run completes, the screenshots show a healthy
application, the log shows no errors — and not one of the inputs the script sent
ever reached the app. The run "passes" and proves nothing.

**Measured** (Xvfb + openbox, GTK 4.6.9 on X11, release binary, this machine):

| Channel | Result |
|---|---|
| `xdotool windowactivate --sync $WIN` | succeeded |
| `xdotool getactivewindow` / `getwindowfocus` | both returned the app's window |
| `xdotool key ctrl+n` (add a tab) | **no effect** |
| `xdotool key alt+3` (change view mode) | **no effect** |
| `xdotool type 'TEXT'` | **no effect** |
| `xdotool mousemove <abs> click 1` on a link | **worked** — opened the target as a new tab |
| …on a task checkbox | **worked** — toggled it and dirtied the tab |
| …on a toolbar button | **worked** — saved the document |

So the **pointer** half of XTEST reached the application and the **keyboard** half did
not, under a configuration where every diagnostic said focus was correct. No tool
reported an error. The cause is not established here and is deliberately not guessed
at; what is established is that the two channels can diverge and that neither
`getwindowfocus` nor a screenshot can tell you.

**Cost.** Three full drive cycles. The first script also *hung* rather than failing,
because the window lookup silently returned an empty string and `import -window ""`
blocks — a second instance of the same shape one layer down.

**The rule.** **Assert a positive control on every input channel before trusting a
drive**: send one keystroke and one click whose effect you can check independently
(a tab count, a log line, a file on disk), and only then run the real scenario. Until
that exists, "the drive ran and found no bug" is unfalsifiable — it is equally
consistent with a working application and with an input pipe that goes nowhere. This
is #217's positive-control rule and #239's "a control that cannot differ has stopped
being a control", aimed at the *harness's input path* rather than at the binary.

**Two practicalities that each cost a cycle here:**
- **With no window manager at all, nothing ever takes focus**, so keys are dropped for
  a completely different reason. Adding a WM fixes that one and can leave the above
  intact — which is exactly why fixing the first cause reads as fixing the problem.
- **`xdotool mousemove` takes SCREEN coordinates; a screenshot is window-relative.**
  Read `xwininfo -id <win>` "Absolute upper-left" and add the offset. A click at the
  un-offset coordinate lands on empty desktop and reports success.

**Corollary worth keeping**: when the keyboard channel is unavailable, a GTK app is
still fully drivable by pointer if it has toolbar buttons and in-document affordances
— the save path here was verified end to end without a single keystroke. Prefer that
over abandoning live verification.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-245).

## 246. GDK-Win32 refuses an empty window title and substitutes a literal "."
**Symptom.** On Windows, every modal confirmation the application raises shows a
lone `.` beside the app icon in its title bar, and the same `.` in the taskbar and
in Alt+Tab. Nothing in the application sets that string, and no warning is emitted.
**Scribobulate.** `window::save::confirm_dialog` sets `winstate::APP_NAME`; the one
place all three modal confirmations (close prompt, overwrite warning, save error)
are built. Pinned by `a_modal_confirmation_carries_a_window_title`, which diffs the
window's modal transients across the call — a document window already owns another
modal transient (the Keyboard Shortcuts help window), so "the modal transient" is
not a well-formed question.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-246).

## 247. "No handler is registered for this scheme" is not a safety property
**Symptom.** A GTK test suite raised a modal Windows dialog that **outlived the test
process**, held foreground focus, and could not be dismissed by any of the usual
means — then silently swallowed the keystrokes of an unrelated driven UI run an hour
later.
**Scribobulate.** `renderer::end`'s `INERT_URI`, plus
`the_probe_uri_is_one_gtk_refuses_to_launch`, which asserts
`glib::Uri::is_valid(INERT_URI, UriFlags::NONE).is_err()` on every platform — the
enforcement mechanism for the claim, per POLICY § Typed GTK seams.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-247).

## 248. A randomly-minted identity correlates only with the mechanism that persisted it

**Symptom.** After an unclean exit, reopening the lost document *by name* — an
Explorer double-click, `scribobulate notes.md`, a desktop-file association —
produced **two tabs of one file**: the one the user asked for, clean, and the
recovered one beside it. No error, no warning, nothing in the log to distinguish
it from correct behaviour.

**The mechanism.** Each snapshot records the `DocId` of the tab that wrote it: 128
random bits minted when the tab is created and persisted in `session.toml`. So the
id correlates a snapshot with a tab **session restore** rebuilt, and with nothing
else. A tab created any other way gets a fresh id, and the recovery pass's
"the session never restored this document" branch — a legitimate branch, and the
one that makes the header authoritative rather than advisory — then opens a tab of
its own.

The two facts that make this expensive rather than obvious:

- **The failing route is the ordinary one.** On Windows especially, the normal way
  to open a document is to double-click it. "Crash, then reopen the file" is the
  central user sequence for this whole feature, and it was the one path the
  correlation could not serve.
- **Every automated test relaunched bare.** A bare launch restores from the session
  file, which is exactly the case where the id *does* correlate. The suite was not
  weak; it exercised the one entry point under which the defect is invisible —
  #234's shape, aimed at entry points rather than assertions.

**The rule.** *When an identity is minted per object rather than derived from the
object, enumerate every way that object can be re-created before relying on the
identity to find it again.* Where the enumeration has gaps, give the lookup a
second key derived from the thing itself — here the canonical backing path, routed
through the project's existing "is this the same file?" helper so that `..`,
symlinks and Windows' choice of letter case are all handled in one place rather
than by a fresh string comparison. Keep it a *fallback*: an id is exact, a path is
merely suggestive.

**Measured second-order trap, in the fix itself.** The adopting tab takes on the
snapshot's id (so the re-armed snapshot lands on the file it was read from instead
of orphaning it). A "tabs already recovered into" set recorded under the id the tab
had **when it was chosen** therefore stops matching a moment later — and the next
snapshot naming that same path finds an apparently-unclaimed tab and overwrites the
work just recovered into it. That is a data-loss bug introduced by a
duplicate-tab fix, i.e. strictly worse than what it replaced. It was caught by the
test written for the two-snapshots-for-one-path case, which existed only because
the boundary was stated before it was coded.

The general form: **a set keyed on a mutable identity is a set that silently stops
containing its members.** Record every identity the member will answer to, or key
on something that cannot change.

**Scribobulate.** `swapfile::recovery::disposition`'s `tab_at_same_path` parameter
(the decision) and `window::swaprecovery::tab_id_at_same_path` (the filesystem half,
kept out of the display-free core). Contract is TDD 22.17, with 22.16 as the
boundary. Kin #235 — the same defect one level up, where the route never reached the
recovery code at all rather than reaching it and missing — and #241, another lookup
whose key answers a weaker question than it appears to.

**See**: general-engineering-principles (GEP-44) carries the generalised minted-identity / mutable-key lesson. **The body above stays here in full** — the disposition mechanics and their TDD contract are this project's, not the general principle's.

## 249. A capability whose backend is a HELPER EXECUTABLE is a packaging obligation, and the dev tree cannot fail the test
**Symptom.** A user reported that Scribobulate opened a **separate process** for
every Markdown file double-clicked in Explorer — two windows, two live-reload
monitors, two buffers that could each save over the other. It did not reproduce on
any developer machine. Nothing errored: `g_application_register()` succeeded,
`is_remote()` reported `false`, and every launch elected itself primary, which is
byte-for-byte what a legitimate first launch looks like — #174's signature exactly,
on a platform #174 had explicitly cleared.
**Scribobulate.** `packaging\windows\stage.ps1` `$helpers` (hard-fails like the DLL
list, so a future gvsbuild layout change is a build error rather than a silent
regression); the corrected claims in [TECH.md](TECH.md) (platform table + the
single-instance architecture bullet) and `tests/MANUAL-TEST.md` (§A *Launch & instance
identity*, the `-n` note); new manual item **8.2s**, which re-runs 8.1/8.2 against the
staged tree with a scrubbed `PATH` and asserts the `gdbus` daemon as a transport check
rather than trusting the outcome. Contract unchanged: **TDD 8.1/8.2** — the point is
that the *behaviour* never needed a Windows clause, only the *package* did.
**See**: general-engineering-principles (GEP-39).

## 250. A widget swapped in for one feature's sake moves its text out of every text-walker's reach

**Symptom.** The find bar reports "No matches" for a word the reader can see on the
page. In a table, `| [Handbook](…) |` — a cell that is *nothing but* a link — is
never found; the same word written as `see [Handbook](…) again`, in the cell beside
it, is found normally. Nothing errors, and which cells find can see is unguessable
from the document.

**The mechanism.** Two correct decisions, taken years apart, with nothing joining
them.

- A cell whose whole content is one link renders as a `GtkLinkButton`, not a
  `GtkLabel` with Pango `<a href>` markup — that is #4, and the reasons (hover
  cursor, activate on *release*) still hold. A button keeps its caption in a label
  **inside** itself.
- Find-in-preview reaches cell text by walking each table widget's **direct
  children** and downcasting to `GtkLabel`, because a cell's text lives in a label
  and not in the `GtkTextBuffer` (#36), so `forward_search` cannot see it.

A link cell is therefore not a `GtkLabel` child, and the walk skips it — not by
decision but by shape. What made it durable is that the code had *written the
exclusion down as intentional*: "pure-link cells are deliberately excluded — they
are not selectable labels, so they participate in neither the match count nor
navigation, keeping the count exactly equal to what find can navigate to." Every
clause of that was true **of the walk as it stood**, and none of it was a reason not
to reach the caption: a caption label takes the same Pango highlight overlay, and
find's scroll resolves a row by transforming the label into its `ScribTableWidget`
ancestor (#109), which is an ancestor of a caption label too.

**The rule.** *A downcast to a concrete widget type is a **structural** predicate
standing in for a **semantic** question* — here "does this cell have text?" — *and
it silently answers "no" for every shape nobody thought of.* When a walk asks a
semantic question, enumerate the widget shapes that can carry that content, and make
the choice of shape and the read-back of its content **one seam**, so a future shape
cannot be introduced without a way to read it. And treat a "we deliberately skip X"
comment sitting next to a structural test as a claim to re-derive rather than
inherit: it is exactly where a true statement about the *implementation* gets
mistaken for a statement about the *requirement*.

**The same predicate in a parameter, where it is cheaper to kill.** The walk above
has to downcast — the shapes are genuinely heterogeneous and only discoverable at
runtime — but the identical trap appears in a far more tractable place: a helper
that declares `&gtk::Widget` and opens by downcasting its own argument. `preview::
scroll`'s seven entry points did exactly that, each returning silently when the
argument was not a `ScrolledWindow`. Every caller passed the right widget, so
nothing was broken; but the preview pane is a `GtkOverlay` *wrapping* the scroller,
so the natural mistake — handing in "the preview" rather than "the preview's
scroller" — compiled, ran, and did nothing, and the symptom would have been a
reading position that quietly failed to restore. `window::reload` had already hit
it once and carried a comment warning the next author off. **Distinguish the two
forms:** where the downcast interrogates a value the function *received*, the
semantic question is answerable at compile time and the corrective is to narrow the
parameter (`&gtk::ScrolledWindow`), which is strictly stronger than enumerating
shapes — the wrong argument stops building instead of stopping silently, and the
warning comment can be deleted rather than maintained. Only a downcast that
interrogates something *discovered* (a child, a pick, a tree walk) needs the
enumerate-every-shape treatment. Rule of thumb: **an untyped `&gtk::Widget`
parameter that is immediately downcast is a type that has not been written down
yet.**

**Trap inside the fix.** The caption must be installed as **escaped Pango markup**,
not plain text. Find's cell path forces an anchored child to re-snapshot by toggling
a transient no-attr `<span>` wrapper around the label's own markup (#37/#117); on a
plain-text label that reinterprets the caption as markup, so a caption containing
`&` or `<` fails `pango_parse_markup` and the label renders **empty** (#163) — a fix
that repairs find and silently blanks captions, with no crash and no compile error.
One `set_markup` of an escaped string, at the seam, is the whole of it.

**Measured** (GTK 4.6.9, Xvfb): a fixture with one pure-link cell and one mixed cell,
both captioned "Handbook", found **1 of 2** before the fix and **2 of 2** after;
restoring the direct-children-only walk returns it to 1, so the guard is not
vacuous.

**This was a CAM violation, and the more useful finding is why the CAM did not catch
it.** Document Rendering CAM row 8 (find still matches the document's text) read
against row 2 (correct inside every container markup, table cells named first) is
exactly this obligation, written down and unmet. It did not fire because **row 8 was
written on 2026-07-13, the pure-link cell shipped in the initial commit (2026-06-23),
and its find path a week after that** — a CAM is consulted when a change lands, so a
row added later applies to future changes and to nothing else, while the matrix reads
as though the whole codebase satisfies it. Six weeks of a rule that was true on paper
and unenforced in the one place it mattered, closed by a user report rather than by
any gate. The general form is now a governing rule in [CAM.md](CAM.md): *adding a row
obliges a back-sweep of the features already in that category, or an explicit record
that no sweep was done.*

The sweep row 8 implies was then run, and it is bounded: the preview anchors exactly
four kinds of widget — the table, an image overlay, a broken-image icon and the
horizontal-rule separator — and **only the table carries text**, so table cells are
the whole of the out-of-buffer exposure and all three cell shapes (plain, mixed,
pure-link) are now targets. Every other rendered string is buffer text and always was.

**Scribobulate.** `widgets::table::linkcell` — `link_cell_button` (the only sanctioned
way to build a link cell; `gtk4::LinkButton::with_label`/`::new` are banned in
`clippy.toml`, the seam and the two GTK-emission probes in `renderer::end` carrying
the only allows) and its twin `link_cell_caption`, consumed by
`preview::cells::cell_search_targets`. Contract is TDD 11.9; manual check §11.9.
Kin #36 (cell text is not in the buffer at all) and #235 (a capability wired into one
of several shapes of the same thing).

## 251. A distribution GTK has its entire introspection surface compiled out, and the three channels fail in three different ways — one of them by reporting a number that means "healthy"
**Symptom.** Every toolkit-side instrument a profiling or leak-hunting plan would
reach for produces nothing on this host, and only one of them says so clearly. The
open CPU-spin issue's recommended first instrument — running once under
`GTK_DEBUG=geometry` to name the subtree that re-`queue_resize`s each frame — cannot
run here as written, and an agent who grepped the resulting log for geometry output
would read the empty result as *"no widget re-queues resize"*.
**Scribobulate**: the introspection-surface probe used when verifying against a distribution GTK build — the channel that reports a number is the one that lies, so the probe asserts on the transport rather than the returned value.
**See**: gtk4-rs skill → ui-testing-debugging (GTK4Rs/AP-251, the UI-driving half); general-engineering-principles (GEP-13, the prove-it-emits half). **Both** — each carries only half this lesson.

## 252. A drive step routed through an app command inherits that command's own enablement gate — and a disabled `GAction` swallows the step in silence

**Symptom.** Halfway through a live drive, the step that *positions* the subject
(here: put the caret on a given line, via the app's own Go To Line) stops taking
effect. Nothing errors: `xdotool` reports success, no dialog appears, the log is
clean. Every assertion after it is then evaluated against the *previous* position —
and reads as a result rather than as a failure.

**Measured** (Xvfb + openbox, GTK 4.6.9 / X11, release build, verifying Edit ▸ Copy
Link Location). The drive alternated two steps: place the caret with `Ctrl+G` +
a line number, then click the toolbar button under test. The click moves focus to
the toolbar; `win.go-to-line` is gated on **editor focus** (`editbar/focusgate.rs`),
so from the second iteration onward `Ctrl+G` activated a *disabled* action —
`g_simple_action_activate` returns without emitting `activate` when the action is
disabled, so there is nothing to observe and nothing to log.

**Why it is worse than a dropped input.** The stale-caret run produced the *correct
answer for the previous line*: the negative case ("caret in prose ⇒ the clipboard
must not change") reported the URL from the link line, which is exactly what a
**broken gate** would also produce. The false reading was indistinguishable from a
genuine defect, in the direction that manufactures a bug report about working code.
It was caught only by cropping the footer's Ln/Col indicator out of a screenshot and
seeing `Ln 5` where the script believed `Ln 3`.

**The rule — two halves, both cheap:**

1. **Verify a setup step by its own observable, never by its exit status.** A drive
   asserts the *behaviour*; the state it established first is an assumption, and an
   unasserted assumption is where a run silently changes subject. This app hands out
   the observables for free — the Ln/Col indicator, the window title, the match
   count — and reading one costs a crop.
2. **Prefer a setup primitive with no enablement gate.** Clicking into the text at
   coordinates moves the caret unconditionally; the same caret move via a focus-gated
   command is only available in the states that command is available in — which are
   not the states the test is exploring. A harness step must not depend on the
   subsystem under test being in a particular state.

**Second-order, and the reason this shape survives review:** two assertions sharing
one stale precondition **agree with each other**, so a self-consistent pair of
results is not evidence that either was measured. The corroboration is an artefact of
the shared premise.

**A second instance, same session, different mechanism — which is why the rule is
about the *class* and not about Go To Line.** Chaining the pointer move and the press
into ONE `xdotool mousemove --window … mousedown 3` invocation delivered a press the
app's own gesture handler **never ran for**: no handler call at all (proved by a
temporary `[🐛DEBUG]` line in the handler — three lines for four clicks), while a
context menu still appeared on screen. The screenshot therefore showed a real menu
built for the *previous* pointer position, and the feature under test read as broken
for one link and working for its neighbour — "which occurrences it works on looks
arbitrary" being the same reader-facing symptom #250 produces from a genuine defect.
Splitting the move from the press with a ~0.3 s settle made it deterministic (three
consecutive runs, identical coordinates, correct result). The cheap general form:
**an input that is delivered is not an input that was handled** — when a drive step's
effect is invisible, instrument the handler rather than re-reading the screenshot.

**A third instance, and the one that shows the class has a TIME axis — a probe can be
correct for years and be converted to wrong by an APP change that never touches it.**
Measured on Windows (Win10 19045, Windows PowerShell 5.1 / .NET Framework, no GTK
involved): `System.Diagnostics.Process` **caches** `MainWindowTitle` on first access,
so a driver holding one process object across an action re-reads the pre-action title —
silently, no error, no staleness indicator — while `GetWindowTextW` (ground truth) and a
**freshly constructed** `Get-Process` object both return the new caption, and
`$p.Refresh()` clears the cache. **The trap needs a read BEFORE the change and a re-read
on the SAME object** — a held object whose *first* read happens after the change answers
correctly — which is exactly why a harness can hold one for years and never see it,
until someone adds an earlier read. The trap is not "a cached property exists": it is
that the harness's correctness rested on an **undocumented app invariant** — "the window
title only changes when the tab set changes" — which held until the title started
naming the *active* document, at which point a plain tab switch retitles the window and
a held probe grades the previous tab (measured on the app itself, not just a control
subject: held-then-switch-then-re-read returned the pre-switch document twice). Nothing
in the harness was edited on the day it became wrong. Rule: **a cached OS probe is a bet on how often the value changes; a
feature that makes it change more often converts the probe from correct to wrong, and
the diff that broke it does not touch the probe.** So read such a value through a fresh
object on every read (the piped `Get-Process … | Select MainWindowTitle` form is safe
precisely because it constructs one per call — never "optimise" it into a held
variable), and when a change makes an observable change more often, ask what was
sampling it.

*Non-core (verification tooling / test-harness design — not a GTK widget contract).
The transferable half is "assert the setup, not its delivery", plus the GTK-specific
fact that activating a disabled `GAction` is a silent no-op with no signal to hook.*
Kin #245 (the same silence one layer down — the input *channel* delivering nothing
while every diagnostic says otherwise), #217 (positive controls), #239 (a control
that cannot differ has stopped being a control).

**Scribobulate**: no application code — but the third instance's corrective IS carried in this tree, as the fresh-`Get-Process` rule in `tests/MANUAL-TEST.md` §A.3's Windows launch step.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-252).

## 253. The `org.gtk.Actions` probe answers about the operator's app when addressed by the well-known name — a `--new-instance` app must be probed by its UNIQUE name

**Symptom.** You verify a newly-added action's registration or `enabled` bit the way
the plan prescribes — over `org.gtk.Actions`, because sensitivity frequently has no
pixel signal at all (GTK4Rs/AP-67) — and D-Bus replies, crisply, that your action
does not exist. The obvious conclusion is that the registration is broken.

**Measured** (GTK 4.6.9 / X11, private Xvfb, release build, verifying the new
`win.nav-back` / `win.nav-forward`):

```
gdbus call --session --dest com.extollit.scribobulate \
  --object-path /com/extollit/scribobulate/window/1 \
  --method org.gtk.Actions.Describe nav-back
→ GDBus.Error:org.freedesktop.DBus.Error.InvalidArgs:
  The named action ('nav-back') does not exist.
```

…while the same build, driven through the keyboard and the toolbar on the same
display seconds later, navigated correctly on both actions.

**Cause.** The manual plan mandates `-n`/`--new-instance` so a relaunch cannot be
forwarded to a stale primary (ScrAP-43). `-n` means `ApplicationFlags::NON_UNIQUE`,
and a non-unique `GApplication` **owns no bus name**. The well-known name in the
`--dest` argument therefore does not resolve to the process under test; it resolves
to whatever process currently holds that name — on a shared session bus, the
operator's own installed copy (`pgrep -ax scribobulate` showed
`/home/…/.local/bin/scribobulate` alongside the test binary). The reply was truthful
about *that* app, which is running an older build.

**Why it is expensive rather than merely wrong.** The answer is specific,
well-formed, and points at the code you just wrote, so it reads as a finding rather
than as a mis-addressed query — the ScrAP-252 shape (a confident answer about the
wrong subject). The available tell is easy to miss and requires already suspecting
the problem: `org.gtk.Actions.List` at `window/1` **and** `window/2` both answered,
for a launch that had built exactly one window.

**The corrective — and the first version of this entry got it wrong, which is worth
recording because the error is the more expensive of the two.** That version
concluded the probe was *unavailable* for a `-n` instance and prescribed functional
verification instead. It reached that from a single failed well-known-name query
without trying the other address. **A `NON_UNIQUE` `GApplication` still opens a
session-bus connection and still exports its action groups — it just owns no
well-known name.** So resolve its UNIQUE name from the PID you launched and address
that:

```bash
BUS=$(busctl --user list --no-pager | awk -v p=$APP_PID '$2==p {print $1}')   # → :1.NNN
gdbus call --session --dest "$BUS" \
  --object-path /com/extollit/scribobulate/window/1 \
  --method org.gtk.Actions.Describe nav-back
```

MEASURED end-to-end against a `-n` instance (Xvfb, release): `Describe nav-back`
returned `(false, …)` on a fresh window, `Activate next-tab` drove a tab switch, and
a re-`Describe` returned `(true, …)` for `nav-back` with `(false, …)` for
`nav-forward` — then `Activate nav-back` flipped the pair the other way. Both halves
of GTK4Rs/AP-67 (read the state, drive the action) therefore work for exactly the
launch the manual plan mandates, with no pointer and no pixels.

**The rule.**
- **A well-known bus name is a ROLE, not your process.** Bind the probe to the
  identity you launched — the unique name derived from the PID — and never to a name
  another process can hold. This is ScrAP-131's "scope it by the PID you launched"
  applied to a third axis: process, window, *and* bus identity are three separate
  scopings, each of which must be done deliberately.
- **Functional verification is the fallback, not the answer:** invoke the command and
  observe whether anything happened. A disabled `GAction` is a silent no-op
  (ScrAP-252), so "pressed it, nothing changed" *is* the reading of `enabled == false`.
  Reach for it where there is no session bus at all.
- **`dbus-run-session` is not needed for this** — it isolates *forwarding*, which is
  a different concern, and costs the cold-portal delay of ScrAP-138.

**Second-order, and the reason the wrong version is recorded rather than quietly
overwritten:** "the tool cannot see my process" and "I addressed the tool wrongly"
predict the same single observation, and the first is the one that ends the
investigation. The corrected reading arrived only because a peer's own knowledge base
already carried the unique-name route — i.e. from *outside* the failing observation,
which is precisely where it had to come from. Before concluding a capability is
absent, enumerate the ways of asking for it.

**Confirmed in passing, and worth stating because it is what sends people to D-Bus
in the first place:** a toolbar `GtkButton` driven by `set_action_name` renders
**pixel-identical** enabled and disabled — `compare -metric AE` over the chevron's
crop returned **0** across a transition proven real by the button's behaviour
(GTK4Rs/AP-67, re-measured). A *menu item* for the same action does grey visibly, so
the menu is the surface to screenshot when a pixel is wanted.

*Non-core (verification tooling / harness design). The GTK-specific halves are
`NON_UNIQUE` ⇒ no bus name, and the pixel-identity of a disabled action-button.*
Kin ScrAP-43 and ScrAP-131 (process, window and bus-name isolation are three
separate things, each of which must be scoped), ScrAP-252 (a self-consistent wrong
answer), ScrAP-247 (a claim about the host rather than about your own dependency).

**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-253).

## 254. An invariant held by two sufficient mechanisms is mutation-proof one at a time — so the mutation test calls each of them dead code
**Symptom.** You neuter a guard to prove the test that covers it fails, per the
project's own mutation discipline — and the suite stays green. Two readings are
available and both are wrong: "the guard is dead code, delete it", and (if you
never ran the mutation) "mutation-checked" in a doc comment.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-11).

## 255. A construct whose glyphs are buffered at its `End` event is not opaque — it is char-precise in a coordinate space nobody wrote down

> *Non-core (this project's copymap + renderer offset model) — do NOT fold into the
> gtk4-rs skill. Sibling of ScrAP-73 (the block-granular copy this register was
> opened with) and ScrAP-97 (branch behaviour set from the construct KIND).*

**Symptom.** Selecting a couple of words inside a rendered code block and choosing
Copy — from the context menu, the Edit menu, or Ctrl+C, all one `win.copy` action —
put the **entire fenced block, fences included** on the clipboard. Every other
construct had been char-precise since ScrAP-73; the code block was the last one
that had not, and it is the construct users copy *from* most.

**Root cause.** The copymap captures each render event's live buffer range as
`(before, after)` around that event's processing. That is exact for every
construct whose interior events insert their own glyphs — and a code block's do
not: `Renderer` *accumulates* the body while the `Text` events go by (inserting
nothing, so each captured range is **zero-width**) and flushes the whole block in
one syntect-highlighted insertion at `TagEnd::CodeBlock`. The block's glyphs are
therefore attributed to the `End` event, and the interior looks like it has no
buffer coordinates at all. It was consequently filed with images and tables as
`Node::Opaque` — "owns buffer glyphs with no reconstructable interior".

**The misread is the lesson.** An image or a table is opaque for a *structural*
reason: it is one `U+FFFC` anchor standing in for a widget whose text is not in
this buffer at all. A code block shares none of that — its body **is**
buffer text, character for character; only the *bookkeeping* attributed it
elsewhere. "No per-event buffer range" and "no reconstructable interior" are
different claims, and one was read as the other.

**Resolution.** `copymap::code_block_node` lays the interior events' source runs
out across the `End` event's buffer range, in order, producing one leaf per run —
and **proves the layout before trusting it**: the flushed char count must equal
the body's, mirroring `insert_code_block`'s own rule (trailing blank lines
trimmed, one `\n` per line). Where it does not reconcile — or where any
non-`Text` event appears inside — the block falls back to exactly the opaque node
it used to be. Coarse copy, never wrong copy, on the same principle as the
`one_to_one` degradation an escape or an entity already gets.

**Two traps in the fix, both worth more than the fix.**

1. **The tree has a second consumer with the opposite requirement.** `wrap_span`
   resolves where a CriticMarkup annotation may be placed. Making the block
   divisible for *copy* would silently have made it divisible for *annotate*, and
   `{==` landing inside a fence renders as literal code — a regression in a
   different feature, from a change that reads as local. The two properties
   ("char-precise for copy", "indivisible as a construct") are independent.
2. **A closing fence closes nothing unless it begins a line.** A selection that
   crosses into the block and stops mid-line reconstructs the fence right after
   the last selected character — ```` let a``` ```` — which CommonMark reads as
   more code, so the paste swallows everything after it. Well-formedness (TDD
   2.8e) is a property of the *emitted* text, not of having emitted a delimiter.

Both were caught because the branch flags were re-examined rather than reused: the
node carried `line_marker: bool` + `inline: bool`, and a code block is neither
(nor plainly both). Replacing them with one `BranchKind` — `Container`,
`LineMarker`, `Paired`, `Fence` — made each site ask a semantic question and made
`Fence`'s extra rule expressible. **A boolean pair that has no name for the case in
front of you is the design telling you it is an enum.**

**Now pinned by**: `copymap/tests.rs`'s code-block block (within/whole-line/whole-body,
both crossings, indented, quoted, in a list, trailing-blank degradation, empty
block, `wrap_span` whole-block) plus `preview::build`'s
`code_block_is_char_precise_live`, which drives the REAL syntect flush the unit
sim can only mirror. All five guards mutation-tested: restoring the opaque set,
neutering the fence newline, dropping `Fence` from `paired()`, and disabling
either fallback condition each fail a specific test.

## 256. A gate's threshold is copied by hand out of a multi-metric report — so maintaining the gate is how you break it
**Symptom.** A change that *added* tests and *raised* scoped coverage was followed
by the coverage gate exiting non-zero. The obvious reading — the one the exit code
invites — is "this change tanked coverage", and it is wrong: coverage had gone up.
The floor had been set to a number the run can never reach.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: general-engineering-principles (GEP-7).

## 257. `Trying to snapshot GtkGizmo … without a current allocation` is GTK's own scrollbar trough — the one benign member of a warning family whose other members are real bugs

> *Core GTK4 — landed in the gtk4-rs skill as **GTK4Rs/AP-257** (its name/count triage for
> this warning, `references/ui-testing-debugging.md`); kept here because the identification
> TECHNIQUE and the filter rule were derived on this project and are cheap to lose.*

**Symptom.** One `Trying to snapshot GtkGizmo 0x… without a current allocation`
reaching WARN at the default log level, once per session, with no visual defect —
inside a warning family whose other members are genuine, ugly bugs (a pane stuck
blank until resized; a first-shown stack page; content overflowing at window open).
Same message text, opposite severity.

**What was tried.**
- Chasing it as one of ours. Four reproduction attempts were spent on a trigger
  recorded from a wrong first reading ("the first content-height change"), which
  never fires in the plain editor.
- Reaching for a debugger and then for distribution debug symbols — a dead end on
  this platform in both directions (see the debuginfod/ddebs entry), and unnecessary.

**Root cause.** The warning's `%s` is `gtk_widget_get_name()`, which returns
`priv->name` **or falls back to the GType name**. `GtkGizmo` is GTK's *private*
generic leaf widget, used internally for CSS-styled sub-nodes. So the literal string
`GtkGizmo` is a proof of provenance: **an application widget can never print as
one** — an instance of your own subclass prints its own GType. Specifically it is a
`GtkRange`'s **trough**: `gtk_range` queues an allocation on the trough
unconditionally whenever its adjustment emits `changed`, and if that lands after the
frame's layout pass the trough carries `alloc_needed` into the same frame's
snapshot, where GTK warns and early-returns. The early return leaves the previous
`render_node` intact, so GTK re-appends the **previous** frame's paint — stale, not
blank — and re-snapshots once the allocation lands. Self-healing in one frame, which
is exactly why this member of the family has no visual signature.

**Resolution.** Two techniques, both cheap and both reusable:

- **Identify a GTK-internal widget with a symbol-free parent walk.** Resolve the
  warned pointer to a widget in the log-writer function and walk `get_parent`
  upward, printing each GType. `gtk_widget_get_parent` is an *exported* symbol, so
  this needs no debug symbols, no DWARF, and no debugger — the ladder it prints
  (`GtkGizmo → GtkRange → GtkScrollbar → GtkScrolledWindow → …`) settles the identity
  outright, because a range parents its trough on the range and its slider on the
  trough. Prefer this to elimination-by-pointer-arithmetic, which was the slow road
  to the same answer.
- **If you demote it in a log filter, pin the type.** Match
  `"Trying to snapshot GtkGizmo "`, never a wildcard over the type. The wildcard
  form silences the identical message emitted for one of *your* widgets, which is a
  real defect, and it silences it in exactly the situation where you most need the
  line.

**Lesson.** **A diagnostic's severity can depend entirely on one interpolated field,
so read the field before triaging the message.** Here the whole family shares a
sentence and splits on a single `%s`: GTK's own private widget = benign and
self-healing; your GType = a bug that leaves a pane blank. Two corollaries worth
carrying: a message that prints a *type* is telling you its provenance, and where a
toolkit exports an accessor, provenance questions are answerable **from inside the
running process** — reach for an in-process walk before you reach for symbols you may
not be able to obtain.

**Open, and deliberately not asserted either way**: on this project the warning
stopped reproducing entirely (eight headless runs across four document sizes, two
view modes, typing and zoom — zero). Whether that is a fix or a shifted timing window
is unknown, and one configuration could not be driven at all because the process dies
there for an unrelated reason. Do not read "it no longer reproduces" as "the
mechanism above is retired" — the mechanism is source-confirmed and unconditional in
`gtk_range`.

## 258. Replacing a live `GtkTextView`'s buffer is a use-after-free, not a swap — the layout's line-display cache survives `set_buffer` and dangles

> *Core GTK4, and the deepest one this project has found. **Send to the gtk4-rs skill**
> (GtkTextView / buffer lifecycle). Supersedes nothing: ScrAP-104 (a persisted
> `GtkTextMark` re-resolved across a swap) and ScrAP-105 (an `iter_location` right after
> a swap) are the same underlying defect seen through two narrower windows, and both
> guards stay — this entry is the root, and it removes the swap entirely.*

**Symptom**: the application dies outright — usually `SIGSEGV` reading a small offset off
a garbage base, occasionally the glib fatal `Gtk-ERROR gtk_text_btree_line_number
couldn't find line` → `SIGTRAP` — a second or so after an edit, with a backtrace that is
**all GTK**: `g_sequence_insert_sorted` under either `gtk_text_view_get_cursor_locations`
(reached from a `value-changed`) or the widget-tree snapshot descent. Nothing of the
application's own is on the stack above `g_application_run`. It presented for weeks as an
untriggerable "occasional SIGSEGV", because reproducing it needs an edit to land inside a
window whose width is the *previous* render's validation time.

**What was tried**, in order, before the cause was known:
- **Reading it as a size problem.** A 41,785-line document died every time; an 886-line
  one never did. Recorded as "needs a large document" — **wrong**, and the wrong frame
  cost two rounds of investigation. Holding the document constant and varying only the
  pause before typing showed 3 s and 5 s fatal, 6 s and 15 s safe; holding the pause at
  ~1 s made 21,001- and 42,001-line documents die too. Size only sets how wide the
  window is.
- **Suspecting the split scroll-sync's `set_value`** (it appears in the first backtrace).
  A probe build with the sync disabled died anyway, at a *different* GTK call site. The
  sync selects the executioner, not the cause.
- **Auditing our own display-cache-inserting reads.** They had already been moved off
  `iter_location` onto the cache-free `line_yrange` when ScrAP-105 was written. Correct,
  and irrelevant: every faulting call site is GTK's own.
- **Hoping for an upstream fix.** There is none — see below.

**Root cause** (researcher, against the GTK 4.6.9 C source, with a standalone reproducer):
`gtk_text_view_set_buffer` keeps the same `GtkTextLayout`, and `gtk_text_layout_set_buffer`
never touches that layout's line-display cache. The only cleanup is indirect, through
btree teardown, and it is conditional:

```c
ld = _gtk_text_line_remove_data (line, view_id);   /* gtktextbtree.c, node_remove_view */
if (ld)                                            /* <== the defect */
  gtk_text_layout_free_line_data (view->layout, line, ld);
```

An entry is dropped **iff** the line owns a `GtkTextLineData` for this view. The two
populations are not the same set:

- `GtkTextLineData` is created in exactly one place, `gtk_text_layout_wrap()`, called only
  from btree **validation**.
- A `GtkTextLineDisplay` is cached by every non-`size_only` reader — snapshot,
  `get_cursor_locations`, `get_iter_location` — and `gtk_text_layout_wrap` itself asks
  `size_only=TRUE`, which the cache deliberately does not store.

So **validating a line does not cache it, and caching a line does not validate it.** Any
paint or geometry read touching a line the incremental validator has not reached yet
leaves an entry `set_buffer` will not clean; `GtkTextLineDisplay::line` is a raw,
unrefcounted `GtkTextLine *`, so the moment the old buffer finalizes those entries dangle,
and the next `g_sequence_insert_sorted` from anywhere runs its comparator over freed
memory. The two fatal arms are one bug: if the recycled junk ends the sibling walk with
NULL you land on the `g_error` instead of a segfault.

**Not a version to upgrade past.** The `if (ld)` guard is present and unchanged on
gtk-4-8, 4-10, 4-12, 4-14, 4-16, 4-18 and `origin/main` (4.23.2), and
`gtk_text_layout_set_buffer` still does not touch the cache on `main`.

**Resolution**: **render into the view's own buffer instead of replacing it.** The preview
re-render now clears the live buffer and refills it, so the buffer object never dies and
no cached display can outlive it; the clearing delete also invalidates the old content's
entries on GTK's own delete path, which carries no line-data condition. Three consequences
worth knowing before doing this elsewhere:

1. **Detach anchored children with `GtkTextView::remove`, never a bare `unparent`.** The
   view bookkeeps anchored children, and deleting an anchor's `U+FFFC` makes GTK call
   `gtk_text_view_remove` itself — which faults on a child already unparented behind its
   back. This is the one behavioural requirement rendering in place adds; a swap never
   deletes the old buffer's anchors, so it never exercised the path.
2. **Empty the tag table too.** A `GtkTextTagTable` rejects duplicate names, and the tags
   carry theme colours and zoom-scaled metrics that must be rebuilt.
3. **Anything keyed on buffer identity silently stops invalidating.** The preview find
   cache used "is this a different buffer object" as a proxy for "is this a different
   render". With the buffer stable it would serve hits indexing content that is gone → a
   monotonic render generation on the view, bumped in the render's shared choke point, is
   both the replacement and the more honest key.

**Verification, and one negative result worth keeping**: the guard is a **state** assertion
— re-render, then assert the buffer is the same object — mutation-checked by restoring the
swap. A test that re-enacted the *crash* (cache a display on an unvalidated far line,
re-render, force a cache insert) was written, measured, and **deleted**: it PASSED against
the fatal code, because a dangling `GtkTextLine *` only faults once the freed memory is
recycled, which a short headless body does not reliably do. It would have been a guard
reporting "protected" while protecting nothing (ScrAP-87). The crash itself is covered by
a `tests/MANUAL-TEST.md` check that types *inside* the validation window.

**Lesson**: **a "swap" that keeps a machine attached to both sides is not an atomic
exchange — find what the machine cached about the side you are discarding.** GTK's own
API named this `set_buffer` and cleaned up *most* of what a buffer owns, which is exactly
what makes the residue invisible; the failure needed a *timing* window to appear, so it
read as random for weeks. Two transferable habits fall out. First, **when a defect's
observed trigger is a quantity (document size, item count), test whether it is really a
duration** — holding the quantity fixed and varying only the delay is a cheap experiment
that reframed this entire investigation, and the size framing had already been written
into the register as fact. Second, **replacing an object a live widget holds is the
expensive operation; mutating the one it already has is usually both safer and cheaper** —
here it also deleted a class of re-attachment bookkeeping (every handler on the buffer
stays connected, because the buffer stays).

**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-258).

## 259. A rendering feature built for one of a construct's widget shapes leaves the others inert, and the reader sees one capability behaving at random

> *Core GTK4 (GtkLabel link markup, `activate-link` emission, CSS node naming) with a
> Pango-markup escaping half. **Send the core half to the gtk4-rs skill**; the
> double-escaped tooltip title is a Pango/GtkLabel-contract sibling of ScrAP-163 and
> ScrAP-4. **Corrects ScrAP-4** — see the closing note.*

**Symptom**: in a rendered Markdown table, some links are links and some are not, with no
difference a reader can name. In the reported document, every link in the *Context*
table — `| [#6429](url) |`, a cell that is nothing but a link — was coloured, underlined
and clickable, while every link in the *Progress tracker* table — `| ☑ [#6378](url) |`,
the shape every progress/status table is made of — rendered as ordinary body-coloured
prose: no colour, no underline, no pointer cursor, no hover tooltip, no activation, and a
greyed-out Copy Link Location on right-click. Nothing failed anywhere: no GTK warning, no
log line, no test failure (992 green).

**What was tried** — nothing, and that is the finding worth recording. The defect was
resolved by *reading* rather than by iterating, and three written artefacts each said it
was already handled:

- The table widget's module doc stated "cells stay real, selectable `GtkLabel`s **with
  working `<a href>` links**". No cell had ever carried an `<a href>`. It was a statement
  of design *intent* that had frozen into a statement of *fact* — the same
  inherit-don't-re-derive failure ScrAP-250 recorded, recurring in the file it was
  written about.
- The Copy Link Location seam carried a careful paragraph explaining that a mixed cell is
  "not a link on screen at all", marked MEASURED, ending "if mixed-cell links ever become
  real links, this is the function that grows the second branch". Accurate, and it had
  turned an unimplemented case into a documented property.
- `sdd/CAM.md` Document Rendering **row 2** (a rendering feature must be correct inside
  every container markup — table cells) and **row 11** (interaction parity for a target
  inside a table cell) name this obligation exactly, and were unmet.

**Root cause**: the renderer decides a cell's widget shape from its *content*, and only
one shape ever learned about links. A cell whose entire content is one link becomes a
`GtkLinkButton` (ScrAP-4, ScrAP-250); every other cell becomes a `GtkLabel` built from
accumulated Pango markup, and the markup accumulator emitted `<b>`, `<i>`, `<s>`, `<sup>`,
`<sub>` and highlight spans but simply **dropped the href**, appending the link's caption
as escaped text. The link's URL was recorded (to decide the cell shape) and then thrown
away. The same asymmetry ran one level deeper: the shape that *did* work called the
external-URL gate directly rather than the document's link policy, so a `#fragment` or a
relative `./other.md` was dead inside a table and live in a paragraph.

**Resolution**: emit the link into the cell's markup exactly as the inline-format tags are
emitted (open at the link's start, close at its end, so it composes with bold/italic and
with the tight `==`/`~~`/`^`/`~` constructs the crate scans itself), and give **both** cell
shapes one activation seam that routes to the same policy every body link already used.
Four things had to be measured rather than assumed, each of which fails silently:

1. **`<a href>` is `GtkLabel` markup, not Pango markup.** `GtkLabel` runs its own
   `GMarkupParser` first, lifting out the `a` elements and recording each `href`/`title`
   (`gtklabel.c:3376`, `parse_uri_markup` `:3542`), and hands only what remains to
   `pango_parse_markup`. Validating the emitted fragment against Pango asserts against the
   wrong parser and fails on correct markup.
2. **The `title` must be escaped twice.** It is what produces the URL tooltip a body link
   has. Pango unescapes the attribute value when it parses the markup, and
   `gtk_label_query_tooltip` then hands *that* to `gtk_tooltip_set_markup`, which parses it
   **again** (`:1757`). A single escape leaves any URL containing `&` — `?a=1&b=2`, half
   the URLs in a real document — failing to parse as tooltip markup, so no tooltip appears
   and nothing says why. The `href` is escaped once; nothing parses it twice.
3. **`GtkLabel` has the same bypassable default handler as `GtkLinkButton`.**
   `gtk_label_activate_link` calls `gtk_show_uri` with the raw href (`:2081`), so the URL
   scheme gate is bypassed unless the app's handler returns `Propagation::Stop`. That is a
   claim about a *different widget's* signal, so it needs its own measured guard — an
   argument by analogy from the button's is not one. Its oracle is different too: a label
   has no `:visited` property, but its default handler declines outright outside a
   `GtkWindow` (`:2087`), and the signal uses the boolean-handled accumulator (`:2274`),
   so on an unparented label a `true` from emission can only have come from a handler that
   halted it.
4. **Colour *and* underline must be stated by the app, for both shapes.** The body's link
   `GtkTextTag` sets both explicitly; the cells are outside the buffer, so no tag reaches
   them (ScrAP-36) and CSS is their only path. Left to the desktop theme, the measured
   result was the worst one: Breeze-dark underlines `button.link` but not a label's `link`
   node, so the two cell shapes in one table disagreed with each other *and* with the body.
   The nodes are `label → link` (`gtklabel.c:3447`) and `button.link`
   (`gtklinkbutton.c:223`/`:364`).

**Measured** (GTK 4.6.9, gtk-rs 0.10, X11 + Xvfb, with a pre-fix positive control and a
mutation check): pre-fix the mixed cell renders inert text and the guard fails on the
missing href; post-fix a body link, a mixed-cell link and a pure-link cell are
indistinguishable on screen, all three hover-tooltip their URL, and all three
fragment-links scroll to the identical position.

**Lesson**: **when one construct renders through more than one widget shape, the feature
is only as complete as its rarest shape — and the shapes fail silently, because each one
is correct code doing what it was written to do.** The reader experiences that as a
capability behaving at random, which is worse than a capability that is missing: a missing
one is reported, an arbitrary one is worked around. Three habits fall out. **Enumerate the
shapes before writing the feature, not after the bug report** — the shape split is usually
visible in the renderer's own branch and is the natural test matrix (this is ScrAP-250's
read-back seam completed by its write-back twin: that entry made every cell's *text*
reachable, this one makes every cell's *link* work, and both were the same missing
question — "which shapes can carry this?"). **Give the shapes one activation seam rather
than one handler each**, so the policy cannot fork per shape and a later shape inherits it
by construction. And **treat a doc comment that asserts a capability as a claim to
re-derive**: two comments here described this feature as working and one described its
absence as deliberate, so every written artefact agreed with the code and none agreed with
the screen. The check that beat all three was looking at the pixels.

**Corrects ScrAP-4.** That entry's stated reasons — "Pango `<a href>` in a `GtkLabel` has
no pointer cursor on hover and activates on button-*press*" — are **false at 4.6.9**, and
they were the argument that made a link inside a mixed cell look not worth having.
`gtk_label_update_cursor` sets `"pointer"` whenever the pointer is over a link
(`gtklabel.c:737`), and `gtk_label_click_gesture_released` emits `activate-link` on
**release**, under three operands including "no selection was made"
(`:4400`) — the same complete-click discipline ScrAP-238 arrived at independently. ScrAP-4's
*conclusion* survives on other grounds (a whole-cell link is a better button: it is
focusable, it carries its URL as a property, and it gets a frame-less button's padding for
free), but its *reasons* did not survive being read. Generalised: **an anti-pattern entry
is evidence about the version it was measured on, and a reason that was never measured is
the part that rots first** — re-check the mechanism before letting an old entry veto a
feature.

**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-239).

---

## 260. A `GtkTextView` scroll aimed past the lazily-validated frontier is parked and never re-issued — and the validation idle cancels the one scroll it does animate
**Symptom**: Ctrl+Home / Ctrl+End in a large document stop part-way and pressing again gets further — the caret lands correctly every time, only the viewport does not follow. Presses needed scale with size (8 000 lines fine, 14 000 three, 20 000+ never); nothing logged, nothing failing, suite green.
**Scribobulate**: `src/farscroll.rs` owns the re-issue — an idle *below* GTK's permanently-ready priority-125 validate source, which is therefore an exact "the layout is valid now" event GTK does not otherwise expose, bounded by a `g_timeout_add` keyed on STALLED PROGRESS rather than elapsed time (a legitimately huge document takes as long as it takes, and a fixed deadline would pre-empt the correct answer). `window/tabs/lifecycle.rs`'s `build_tab_editor` — the one place every editor view is built — installs it, so a view cannot be constructed without it; the re-issue's licence to act is the caret still being where the keystroke put it, so a later navigation retires a pending one with no generation bookkeeping. `saferizer::scrollpos` owns the writes (`jump`, and `reconfigure` for the mirror-image failure: `configure`/`clamp_page` write `priv->value` with no `end_updating`, so a running animation's next frame silently overwrites them), all backed by `clippy.toml` bans on `TextViewExt::scroll_to_mark` and `AdjustmentExt::set_value`/`configure`/`clamp_page` so neither half can be re-entered by a new call site without the ban naming the route. It also covers the app as its OWN second source: any adjustment write destroys that view's `first_validate_idle`, so a split-pane sync projection into one pane orphans a scroll the other pane had queued — the reported symptom, arriving from our own code. Verified on the operator's real session against a deliberately rebuilt pre-fix binary (never a stash — ScrAP-239): 200 000 lines, the control still at line 1 after 120 s, the fixed build at line 199 959 by 40 s.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-260) for the mechanism, the refuted pin-to-the-frontier design (a livelock: 20 000 lines took 98 ms unpinned and 1494 ms pinned), the private-`is_animating` corollary and the "turns are not time" testing note. Upstream GNOME/gtk #7507 / #2205 / #5065 are filed and STALLED — **do not file again**; this needs an application-side answer for the foreseeable life of the project. Kin: #82, #13 (same lazy-validation cause on a FRESH view, where this is a warm one), #87.
## 261. A derived-state hook installed at the producer misses the rebuild shape the producer also has

**Symptom**: a hook that keeps state consistent with the rendered document fires for
every re-render *except* the one the feature exists for. The headless test — which
drives the in-place re-render — passes; on the live display the same scenario leaves
the state stale, silently, with no warning and no log line.

**What was tried**:
- Hooking the point where the render stores its heading map into the per-render data.
  This looks like the precise choke point (it is where the value changes) and is where
  a test naturally drives it, so it reads as correct and is green.
- Reasoning that the fresh-render path could be skipped because a fresh render means a
  new tab with no state pointing at it yet. False: a fresh render is *also* how an
  existing tab is rebuilt.

**Root cause**: "the preview was rebuilt" is **two code paths, not one**. Preview mode's
external reload rebuilds by a wholesale render into a brand-new scroller/view widget;
split mode re-renders the existing view in place. A hook installed on the in-place path
is absent from the wholesale one, and nothing types, lints or tests the difference — the
producer's shape is invisible from the hook's own site.

**Resolution**: move the hook off the producer and put it **immediately in front of its
consumer** — here, reconcile the history against the live heading set inside the function
that computes the two actions' sensitivity, so the reconciliation and the value it
protects are computed in the same call and cannot disagree. Path coverage then stops
mattering: neither rebuild shape has to remember anything. Once both callers were in one
module the helper was demoted to module-private, so reconciling from a *third* render
site does not compile (ScrAP-219's ladder).

**Lesson**: when a producer has more than one code shape, a hook on the producer is a
latent regression — the next shape added will not have it, and a test written against the
shape you are looking at will not notice. Prefer siting derived state in front of the
**consumer** that must not be stale: there is one consumer, its correctness is checkable
locally, and the producer count becomes irrelevant. Corollary for verification: a headless
test that drives *a* rebuild proves nothing about the rebuild the user actually triggers —
enumerate the shapes before believing the green.

**Cost**: the defect shipped through a full green gate set (fmt, clippy, 808 unit, 1034
headless, 227 main-thread, and a mutation check that killed its mutant) and was caught
only by driving the real app on the operator's display. Second occurrence of this file's
two-rebuild-shape hazard — Severity: High

**Scribobulate**: `window/navhistory.rs`'s `reconcile_nav_history_headings` is private and
called from `refresh_nav_history_actions` (before it reads `nav_can`) and from `traverse`
(before it steps); `preview/render.rs` deliberately calls nothing.

**See**: kin #52 (the same two rebuild shapes, reached through a stale *signal* rather
than a missing hook — different root cause, same architectural fact); TDD 23.14.

---

## 262. A restore seam's "nothing to do at the boundary" shortcut is a claim about its first caller, and the second caller loses a real destination

**Symptom**: Back and Forward do nothing at all for a jump *within* one document —
the commonest shape of the feature, following a table of contents at the head of the
file — while the same commands work for every other navigation. Nothing is logged,
the buttons light and grey correctly, and the recorded history is provably right
(`nav-back` reports itself available, the entry names the right place). Only the
viewport never moves. Both directions look broken at once, which misdirects the
diagnosis to the recording half: Back scrolls nowhere, so Forward is then asked to
return to the section the reader never left, and *it* looks broken too.

**What was tried**: reading the recording path first — the two choke points, the
slug that gets recorded, the reconciliation that degrades stale headings, the
suppression depth — on the reasoning that "neither direction works" means the
history is empty or wrong. All of it was correct, and every one of the feature's
twenty-one automated tests was green.

**Root cause**: the traversal restores a recorded reading line through
`preview::restore_preview_scroll_to_line`, which opened with `if line <= 0 { return; }`,
documented as *"no-op for `line <= 0` (already at/above the top)"*. That parenthesis
is true of the seam's original caller and only of it: a zoom re-render restores a
view to where it already is, so restoring line 0 is genuinely redundant there. Back
and Forward then reused the same seam to *travel* to a recorded place, where line 0
is not a redundant refresh but a destination — "take the reader back to the top" —
and the guard swallowed it. And it swallowed exactly the case the feature exists
for: a reader following a TOC link is, by definition, at the top of the document, so
the departure stamped onto the entry they leave is line **0** essentially every time.
The seam had absorbed a precondition that belonged to a caller, and nothing at the
new call site could see it — the parameter is an `i32` either way.

**Why the tests all passed**: every existing body parked the reader at a non-zero
line, and the shared fixture's own comment explains why — it carries "enough filler
that a heading's line and the reader's departure line are far apart (an assertion
that passed for both would prove nothing)". That reasoning is sound and it
systematically excluded the boundary: choosing a *non-degenerate* input so the
assertion discriminates is the same choice as never testing the degenerate one, and
here the degenerate one was the majority of real use.

**Resolution**: `restore_preview_scroll_to_line` now clamps a negative line (a bad
value) and *scrolls* to 0 (a position) like any other line, and its doc comment
states the contract as a destination rather than as a refresh, names the sibling
`restore_preview_scroll_to_line_fresh` whose brand-new view genuinely is at the top,
and says the two are not interchangeable. Regression guard:
`window/navhistory/record.rs`'s
`back_from_a_link_followed_at_the_top_of_the_document_returns_to_the_top` —
mutation-checked, the restored early return fails it (`16` against an expected `0`)
and leaves all twenty-one other navigation tests green, which is precisely the
evidence that the existing suite could not have caught this.

**Lesson**: an early-return that means "there is nothing to do here" is a statement
about the caller's *context*, not about the argument's value, and a shared seam has
no access to context — so the second caller inherits a silent, invisible drop. Before
adding a caller to a restore/refresh helper, read its no-op guards as preconditions
and ask whether they hold for the new call; before writing one, prefer a guard that
rejects an impossible *value* (a negative line) over one that assumes a *situation*
(the view is already there). The testing corollary is the sharper half: when a
fixture is deliberately chosen to be non-degenerate so an assertion can discriminate,
that same choice deletes the boundary case from the suite — write the boundary case
as its own body, because for a feature triggered from a document's own table of
contents the boundary *is* the common path.

**Cost**: one full read of the recording half, the pure history core and the
reconciliation before the traversal was suspected at all — the "both directions are
dead" symptom argues for the shared upstream (recording) and against the shared
downstream (the restore seam), and it argues wrongly. Shipped through the whole gate
set and the feature's own twenty-one tests — Severity: High

**Scribobulate**: `preview/scroll.rs`'s `restore_preview_scroll_to_line`
(no `line <= 0` return; negatives clamped), reached from
`window/navhistory/traverse.rs`'s `restore_place` for a `NavSpot::Line`, and from
`window/zoom.rs` for the zoom re-render it was originally written for.

**See**: kin #261 (the sibling defect in the same feature, found the same way — a
correct-looking mechanism whose *other* path nobody enumerated); TDD 23.12; MANUAL-TEST §23.

---

## 263. `line_at_y` on a not-yet-allocated `GtkTextView` reports the buffer's LAST line, so a viewport read taken in the turn a view is built in is maximally wrong
**Symptom**: a `doc.md#section` link opens the target scrolled to the right heading, and one Back throws the reader to the END of the document just opened — silently, because the recorded history honestly names the last line: the measurement was the bug, not the recording.
**Scribobulate**: `saferizer/viewport.rs` gates `ViewportTopIter::of` / `ViewportRange::of` on `visible_rect().height() > 0`, answering with the buffer start when there is no layout; `codeview`'s `reading_line` and `preview/scroll.rs`'s `preview_top_line` route through that seam instead of hand-rolling `vadjustment().value()` + `line_at_y`. Guards: `the_top_of_an_unallocated_view_is_the_start_of_the_buffer_not_its_end` (carries the raw pre-fix call as an in-body control, without which a correct `0` is indistinguishable from a view that simply had not scrolled) and `a_fragment_link_to_another_document_walks_back_across_both_stops`. Audited: the seven `line_at_y` sites outside the seam are test bodies on mapped views or already gated, and no clippy ban was added because every current use is legitimate — it would be all false positives.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-263). Findings: tests/reports/gtk4skiller-brief-ScrAP-263.md (local, gitignored — the woven brief, incl. the measurement tables).

---

## 264. A focused anchored child swallows its host `GtkTextView`'s navigation key bindings, and the document silently refuses to move
**Symptom**: the reader clicks a table cell in the preview and from that moment Ctrl+Home, Ctrl+End, Home, End, ← and → do nothing at all — no scroll, no warning, no log line — while ↑, ↓, PageUp and PageDown keep working, so the pane looks half-dead rather than broken; clicking any prose puts it right, which reads as "intermittent".
**Scribobulate**: `codeview::navkeys::wire_document_navigation_keys`, wired from `CodePreviewView::new` (the one place a preview pane is built), redirects a navigation key to the view with `emit_move_cursor` from a **capture-phase** `GtkEventControllerKey` — the only phase that still sees the key — when `keynav::FocusSite` says focus sits on an anchored child; `keynav` owns both decisions display-free (the movement table mirrors `gtktextview.c`'s own bindings; `FocusSite` excludes the pane itself, a `set_parent`ed popover by `GtkNative`, and any `GtkEditable`). Guards: `keynav`'s 14 unit tests, `ctrl_home_from_a_focused_table_cell_moves_the_document` (mutation-checked: neutering the redirect fails it on the unmoved adjustment) and its four siblings — including `a_key_in_a_popover_parented_to_the_pane_is_left_alone`, which pins the live case the widget-tree test alone gets wrong (the annotation card's `CommentEntry` IS a descendant of the view) and is held by two independently sufficient gates, measured: dropping either alone leaves all five green, dropping both fails it (ScrAP-254 shape); MANUAL-TEST §9 item 9.33b; TDD 9.33. Shift is deliberately not redirected, so in-cell keyboard selection survives (verified live).
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-264, woven 2026-08-09; it sits beside **GTK4Rs/AP-53**, which already owns the general propagation rule — a focused composite's own class-level keybinding beats an ancestor's bubble-phase controller however propagation is configured — so the skill entry spends itself on the corrective, the gates and the diagnostic signature rather than restating the mechanism). Root cause: a selectable `GtkLabel` is focusable and carries its own `move-cursor` class key bindings for the horizontal and buffer-ends keys, so it consumes them in its target phase and they never bubble to the host view; measured GTK 4.6.9/X11 with capture+bubble controllers on the view — the view's capture phase sees the key, its bubble phase never does, `move-cursor` never fires, and a `GtkLinkButton` cell (the other cell shape) swallows nothing. Findings: tests/reports/gtk4skiller-brief-ScrAP-264.md (local, gitignored); the mechanism and the measurement tables are also carried permanently by the two modules' own rustdoc.

## 265. A test that arms a process-global fatal-signal handler and never disarms it re-points the rest of the suite — and displaces the runtime's own stack-overflow guard, so a later overflow stops naming itself
**Symptom**: a required gate (here the coverage step) dies by `signal: 11, SIGSEGV` about one run in three, and dies **articulately** — stderr carries a complete, plausible application crash report with an identity header, fault address, instruction pointer and backtrace. Every reader's first conclusion is that the application crashed. It did not: the report's breadcrumbs are a *test fixture's* two lines, the process that faulted is the test binary, and the fault landed long after the test that armed the handler had already reported `ok`. Re-running the gate "fixes" it, so the whole thing reads as an application flake.
**Scribobulate**: `forensics::signal::tests::ArmedHandler` — an RAII guard that takes the install lock, snapshots every fatal disposition in `FATAL_SIGNALS` (four when this was written, five since ScrAP-268) **and** the calling thread's alternate signal stack, arms, and restores both on drop. Production `install` keeps no uninstall on purpose: a process wants the handler for its whole life, and an uninstall is only a window in which it can die unreported — so the remedy belongs to the test harness, not to the module's public surface. Guard: `the_fatal_handler_does_not_outlive_the_test_that_armed_it`, which asserts the armed state as well as the disarmed one (ScrAP-217 — "no report was written" and "this harness cannot write reports at all" are the same output) and asserts the two restorations *separately* (ScrAP-254 — restoring the disposition alone is sufficient to make the report assertion pass, so the alternate stack, which is the half the language runtime needs back, carries its own).
**Root cause, measured** — 2026-08-09, glibc/Linux, the same overflow probe run both ways: arming takes SIGSEGV away from Rust's stack-overflow handler and `sigaltstack` away from that thread's guard stack. **Left armed**, a stack overflow on an unrelated thread ends the whole process with `(signal: 11, SIGSEGV)` and writes the crash report described above. **Disarmed**, the identical overflow prints `thread '<unknown>' has overflowed its stack` / `fatal runtime error: stack overflow` and aborts on SIGABRT. So the handler never *caused* the fault; it replaced a diagnosis that names itself with one that confidently names something else.
**The transferable half, which is bigger than signals**: state a test writes into the *process* is not scoped to that test, and it fails somewhere else, later, intermittently — the shape that makes a gate look flaky rather than broken. It is worst when the hijacked state is **diagnostic** machinery, because then the failure is not silent but eloquent: it manufactures authoritative-looking evidence about the wrong subject, and the more trustworthy the reporting mechanism is, the further the investigation goes in the wrong direction before anyone checks whether the breadcrumbs belong to the process that died. The reflex worth keeping: read a crash report's identifying detail (here, two fixture strings that exist nowhere but one test) *before* believing its subject.

## 266. A focused popover that is its own `GtkNative` is an application-keyboard dead zone
**Symptom**: while the annotation comment card holds the focus, NO application accelerator fires — not the command that opened it, and not an unrelated `F8` pane toggle either — yet the card's own Escape and Tab-to-Edit work, and the identical keystroke works the instant Escape returns focus to the view. Discriminator: press a COMPLETELY UNRELATED accelerator in the same state; if that is dead too, the keystroke was never delivered and the command's own handler is the wrong subject.
**Scribobulate**: `codeview::card::app_accelerator_controller` — a BUBBLE-phase `GtkShortcutController` on the card carrying one `GtkNamedAction` shortcut per binding from `app::accelerator_bindings()`, the same enumeration `register_accelerators` binds from (a hand-listed set here would be a second copy of the accel SSOT). Bubble, not capture, so a focused comment entry keeps the keys it binds itself (ScrAP-137 one level down). Guard: `the_card_re_offers_every_application_accelerator_locally`, asserting the offered set against that table and the phase separately; both halves mutation-tested.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-266) — which also records this as the other face of GTK4Rs/AP-264's `GtkNative` gate. Mechanism INFERRED, not source-traced; the corrective is measured and does not depend on it.

## 267. A `GtkSingleSelection` built `.model(…).autoselect(false)` opens with a phantom selection
**Symptom**: a list shows its first row highlighted the first time it is displayed although the user has activated nothing — and unrecoverably so where the selection means "the item you last went to": nothing recorded it, so it vanishes at the next rebuild and the highlight appears to move on its own.
**Scribobulate**: `annotations_view::build_annotations_content` builds `.autoselect(false).can_unselect(true).model(&store)` — the flag before the data it governs. Guards: `a_freshly_built_list_selects_nothing` (mutation-tested by swapping the two builder calls back) paired with `a_restored_selection_lands_on_the_annotation_it_names`, so "nothing is selected" cannot be satisfied by a build that has stopped honouring restored selections at all.
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-267).

## 268. A GLib **fatal** log message dies of `SIGTRAP`, not `abort()`, so a crash handler that takes the classic four signals reports nothing for the whole `g_error` class
**Symptom**: nothing — which is the entire problem. A death that the crash-forensics kit exists to describe leaves no report, no breadcrumb flush, not even a signal line, and an absent report is indistinguishable from a clean exit or from "the kit is broken". The suite is green, the rubric is satisfied, and the implementation matches its contract exactly; the gap is in the *enumeration* both sides inherited from one scope decision.
**Root cause, MEASURED** (GLib 2.72.4, Linux/x86-64, this machine): GLib's fatal path does not reach `abort()`. `_g_log_abort` computes `debugger_present = TRUE` unconditionally on every non-Windows target ("assume GDB is attached") and therefore executes `G_BREAKPOINT()` — `int $03` on x86 — instead of `g_abort()`. **The class is every FATAL log message, not the `g_error` family alone** — re-measured after the skill maintainer asked how wide it goes, because the narrower claim was the one I had in my head. Exit codes, driven through real processes, all with the default writer: `g_error(…)` → **133** (128 + `SIGTRAP`); `g_warning` promoted by `G_DEBUG=fatal-warnings` → **133**; `g_critical` promoted by `G_DEBUG=fatal-criticals` → **133**; `g_message` promoted programmatically by `g_log_set_always_fatal` → **133**; and each of those on the legacy *and* the structured log path alike. The lone `abort()` is `g_assert_not_reached()` → **134** (`SIGABRT`), which is not a log promotion at all: the assertion path calls the same function with `breakpoint = FALSE`. So the exception is precisely the case everyone has in mind. Under `gdb` the trap stops in `g_log_structured_array` (`g_logv → g_log_default_handler → g_log_structured_array`).
**The trap inside the trap**: the family everyone quotes as the example — `g_assert_not_reached` / "code should not be reached", which GTK uses freely — is the one that *was* already covered, and the family that actually killed this application (`g_error("gtk_text_btree_line_number couldn't find line")`, ScrAP-258) is the one that was not. Reasoning from the loud, familiar assertion message therefore concludes "we handle this" and stops. `G_DEBUG=fatal-warnings`, the flag CI runs under, converts every routine warning into this same unreported death.
**Scribobulate**: `forensics::signal::FATAL_SIGNALS` gains `SIGTRAP` beside `SIGSEGV`/`SIGBUS`/`SIGILL`/`SIGABRT`, and `signal_name` names it. Two guards, both mutation-tested and neither implying the other: `every_enumerated_fatal_signal_is_reported_and_still_kills_the_process` drives a real death per entry and checks each report against a **literal** name table (an earlier draft asked `signal_name` what to expect and stayed green while reports said `signal: unknown (5)` — GTK4Rs/AP-160's self-agreement shape); `a_glib_fatal_message_dies_by_a_signal_this_handler_takes` re-executes the test binary as a doomed child, provokes a real `G_LOG_LEVEL_ERROR` through `glib::ffi::g_log`, and asserts the death signal *and* the resulting report — the only one of the two that holds the premise about somebody else's library. A `Command` child rather than a `fork`, because a forked child of the multi-threaded test process may not allocate and `g_log` does, so the failure mode there would be a hang.
**Found while verifying the fix on the running app, and worth as much as the fix: a custom `g_log_set_writer_func` silently disables EVERY promoted fatality on the structured path** — `G_DEBUG=fatal-warnings`, `G_DEBUG=fatal-criticals` and a programmatic `g_log_set_always_fatal` are defused identically (all three measured). The live check written for this change told the tester to provoke a warning under `G_DEBUG=fatal-warnings`; driven for real (Xvfb, release build), the app logged four `Gtk-WARNING`s and carried on. MEASURED with a four-way C probe: for **structured** logs — which GTK4's own diagnostics are — `g_log_always_fatal` is consulted inside `g_log_writer_default`, so replacing that writer removes the check entirely (survives, exit 0); the **legacy** `g_log`/`g_warning` path decides fatality in `g_logv` instead and still dies (133) with the same writer installed. `g_error` is immune to this in both modes (fatality is enforced after the writer returns) — which is the operand this fix depends on, so it was measured rather than assumed. Consequence for any GTK app with a log bridge: the `G_DEBUG` flag arms your *test* binaries (no bridge) and quietly does nothing to your *application*, and nothing anywhere says so. Note the DIRECTION, which is the uncomfortable half: the legacy handler that this advice steers away from is the one that *preserves* promotion, because `g_logv` decides fatality upstream of the writer — so the recommended option is the one that loses it.
**Taking `SIGTRAP` costs no debugging session** (MEASURED, gdb 12.1): a debugger intercepts the trap through ptrace and reports `SIGTRAP … Pass to program: No`, so it still stops at the faulting frame and the handler never runs under it. The obvious objection — "you have just stolen the debugger's signal" — is false, and worth measuring rather than arguing, since it is the reason not to do this.
**The transferable half**: (1) *"fatal" is a word in a library's vocabulary, not a statement about how the process dies* — before enumerating the signals a crash handler takes, measure how each dependency's fatal path actually exits, because the plausible enumeration (the hardware faults plus `abort`) is complete only for the deaths you thought of; (2) an enumeration copied into both a rubric and its implementation is one decision wearing two hats, so a gap in it is certified rather than caught, and closing it is always two edits — TDD 21.4's **Given** listed the same four signals the `const` did, and *widening* the claim later repeated the miss exactly once more: the re-measurement swept the code, this entry, POLICY and the manual check, and left the rubric still saying "any warning `fatal-warnings` promotes"; (3) the diagnosis costs minutes when the question is put to the machine (`gcc` + `g_error` + `echo $?`) and hours when it is put to memory.
**See**: routed to the `gtk4-rs` skill (app-lifecycle-and-env) as a GTK/GLib-stack lesson — any GTK4 application that installs a fatal-signal handler has this hole.


## 269. Two sufficient monitor-cancel mechanisms made the rename guard pass with its own fix deleted — and a freshly attached `GFileMonitor` is not yet watching

**Symptom**: two false results in one sitting, in opposite directions, both from the
same feature's tests. (a) The regression guard for "renaming a document must not look
like a deletion" **passed with the fix deleted** — remove the `cancel_monitor()` call
the whole choreography is built on and the suite stays green. (b) A sibling test
asserting the failure path's re-attached monitor still works **failed against correct
code**, and failed in a way that read as "the monitor only half works": a *write* to
the watched file was missed while a *delete* issued twenty seconds later was caught.

**Root cause — (a), the false PASS.** The rename path cancels the tab's monitor before
renaming. But `app::attach_file_backing` *also* cancels whatever monitor the tab was
holding when it installs the new one, so the invariant has **two** sufficient
mechanisms and the ScrAP-254 shape applies: mutating either alone leaves the suite
green. What made it hide rather than merely be redundant is a timing accident — in the
test the rename completes so fast that the completion callback runs before GLib's
inotify worker has dispatched the rename's events, so the second mechanism suppresses
them and the reader sees no difference. In production it is the other way round (the
researcher MEASURED the three events arriving *before* the completion, using a 60 ms
stand-in for the async round trip), so the pre-rename cancel is genuinely load-bearing
and merely **invisible to a fast test**. A behavioural guard cannot discriminate here
at all; the fix is a guard on the **state** — hold the filesystem call open with the
existing `docio::slow_io` injection, and assert that while the rename provably has not
happened yet (the old filename still exists) the old monitor is already cancelled.
That one fails on the mutation and nothing else does.

**Root cause — (b), the false FAILURE.** A `GFileMonitor` object exists the instant it
is constructed, but GLib establishes the underlying inotify watch on its **private
worker thread** (`inotify-kernel.c` attaches its source to `g_get_worker_context()`),
so a change made in the same main-loop turn as the attach races that setup and is
simply never seen. "The tab's monitor slot is populated" is not "the watch is live".
The delete appeared to work only because it happened after a 20-second `settle`
timeout had already elapsed. The kin is GTK4Rs/AP-261 — turns are not time — reaching a
new mechanism: here the wait is for a *worker thread's* setup, so no number of
main-loop iterations substitutes for wall clock.

**What was tried**: asserting the reader-facing symptom (no deleted-backing state after
a rename) — passes with the fix deleted, and is the guard the plan originally specified.
Injecting `slow_io` to separate the events from the completion — does not work in this
direction, because the injected delay sleeps on the pool thread *before* the blocking
call, so it delays the rename itself rather than the gap between the rename and its
completion.

**Scribobulate**: `window/rename.rs` cancels the monitor before the rename and
re-attaches on **every** path including failure;
`the_old_monitor_is_cancelled_before_the_rename_touches_the_filesystem` is the state
guard (mutation-checked: removing the cancel fails it and only it), and
`a_rename_does_not_look_like_a_deletion` keeps the behavioural contract with its
non-optional in-test control (delete the renamed file, assert the notice *does*
appear — without it the test passes against a dead monitor, ScrAP-209's shape). The
second mutation is the positive half: neutering `expect_self_delete` must **not** break
either test, which is what proves the cancel rather than the save path's flag is
carrying the invariant. Every integration test that writes to a watched file pumps for
~300 ms after attaching first. Contract: TDD §24; checks: `tests/MANUAL-TEST.md` §24.

**The macOS half is now MEASURED — one correction and one confirmation, and the split
between them is the useful part** (mac seat, macOS 26.6.1 / GTK 4.22.4 / GLib 2.88.2,
raw probe, three identical runs). **Corrected:** a rename of a watched file emits
`DELETED` **twice**, not the single `DELETED` that was recorded — reproducible, not
noise. **Confirmed:** the inode-following claim was right, and is no longer an
inference — an uncancelled monitor reported a post-rename write to `Notes.md` as
`CHANGED` **under the old `notes.md` path**. That is this entry's choreography being
vindicated from the other side — it is precisely what a stale monitor does when nobody
cancels it, and it is *actively wrong* rather than merely inert, which is what Linux and
Windows path-watching would give you.

**What to trust in a source-traced event claim — SOURCE-READ, and it replaces a rule of
mine that was over-fitted.** From one correction I generalised "source-tracing gets
event *semantics* right and *counts* wrong". The researcher refuted it with a
counter-example from the same investigation: they predicted **exactly three** inotify
events for a rename from source, before running anything, and it measured exactly three.
Counts are not structurally unreadable. Every event that reaches you is queued by one of
exactly **nine** `queue_event()` sites in `glocalfilemonitor.c` (@2.72.4 lines 266, 289,
291, 307, 319, 327, 328, 551, 560) and the amplifiers are all visible there. Two things,
and only two, are genuinely unreadable:
1. **`CHANGED`/`CHANGES_DONE_HINT` counts are rate-limit coalesced** — `DEFAULT_RATE_LIMIT`
   is **800 ms** (`glocalfilemonitor.c:33`) — so they are timing-dependent *by design*.
   Lifecycle events (CREATED/DELETED/the RENAMED expansion) bypass the limiter.
2. **Multiplicity that crosses the kernel boundary**, which is outside GLib entirely.
⇒ **Assert on the event SET and on lifecycle counts; assert `CHANGED` only as `>= 1`;
treat any count crossing the kernel boundary as a range until measured on that platform.**
**Worked example, and the researcher applied it against their own claim:** the Win32
three-event expansion is conditional on `NextEntryOffset != 0`
(`gwin32fsmonitorutils.c:70-88`) — i.e. on both records landing in **one**
`ReadDirectoryChangesW` buffer, which is the OS's batching and therefore unreadable
from GLib. Split across two callbacks, the `else` branch runs instead and reaches the
same **set** by a different path. So the Windows row is **set-verified,
count-unverified** — `{DELETED, CREATED, CHANGES_DONE_HINT}` holds; "exactly three"
does not, and a busier directory could still split it even after one measurement says
three.
**The `DELETED` ×2 is now MEASURED to the kernel, and the answer moves it out of case 2.**
A raw kqueue probe (mac seat, `EVFILT_VNODE`, GIO bypassed entirely, three identical
runs) reports the rename as **one** kevent: `fflags=0x21` = `NOTE_DELETE|NOTE_RENAME`,
with nothing left queued behind it. So the kernel emits **once**; the doubling is
entirely GLib's — `gkqueuefilemonitor.c` maps that single struct through a deliberately
**non-exclusive `if`-cascade** (its own comment: *"since kqueue can return multiple
events in a single kevent struct, we must use 'if' instead of 'else if'"*, @2.88.0
:377-380) and **two** arms emit `DELETED`, `NOTE_DELETE` (:381) and `NOTE_RENAME`
(:426). Both fire off the one struct. The version-skew rival was separately *disproved*
by reading the file at five tags (identical, 621 lines).
**Which sharpens the rule rather than breaking it.** The doubling was inside the
readable nine-site envelope all along — what was unreadable was never the record
*count* but **which flag bits the OS sets in one record**, and one `if`-cascade turns
that unknown into a multiplier. Keep the distinction: *how many notifications* and
*how many bits per notification* are both kernel-side unknowns, and only the second was
in play here.

**See**: the GIO-side mechanisms this rests on (cancel is a barrier at *emission*, not
a queue drain; the per-backend event sets; path-vs-inode) are researcher-established
and routed to the `gtk4-rs` skill — findings doc
`~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-rename-refusal-and-filemonitor-cancel-barrier.md`
(Linux/GLib 2.72.4 MEASURED; macOS/Windows SOURCE-TRACED). Kin: ScrAP-254 (the two-mechanism
mutation trap this is an instance of), ScrAP-209 (a guard whose setup prevents the thing
it guards from existing), ScrAP-54 (the self-delete guard, still load-bearing on the save
path and deliberately not reused here).

## 270. Asking GIO what a file is called after you renamed it, and being told what you asked for
**Symptom**: a rename succeeds, and then everything keyed on the new path — tab, title, and above all the `GFileMonitor` being re-attached — addresses a name no directory entry holds, on any volume that stores a spelling other than the one it was handed; the monitor watches nothing and reports no error, so the only visible symptom is that live reload quietly stopped.
**Scribobulate**: `docio::rename::stored_spelling` enumerates the parent and matches on `id::file` — which entry *is* this file, not which looks like it — and is best-effort throughout, every failure keeping the requested spelling, because a rename that SUCCEEDED must never be reported as failed because a follow-up READ did. Guards: `the_reported_path_is_the_one_the_directory_actually_holds` (asserts against a real `read_dir`, so it is meaningful on all three platforms), `a_name_the_directory_does_not_hold_resolves_to_the_one_it_does`, `a_name_the_directory_does_hold_is_left_exactly_as_asked`. Contract: TDD §24.13. The NFD cause is MEASURED end-to-end on a purpose-built HFS+ volume including the live-reload leg — but the branch is *routinely* executed by a case-insensitive filesystem standing in for the unreachable cause, which is the coverage that costs nobody a special volume and survives that seat going away.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-270), which holds the mechanism, the nine-tag source trace, the Windows backslash/typed-case riders and the cause-vs-mechanism testing lesson. Kin: ScrAP-271, ScrAP-269.

## 271. Matching a directory entry by `id::file`, which identifies the FILE and not the entry
**Symptom**: a rename onto an ordinary, unambiguous name silently renames the tab onto some *other* filename in the same directory — and only sometimes, because it depends on `readdir` order, which ext4 hashes rather than sorts.
**Scribobulate**: `docio::rename::stored_spelling` decides only after the whole enumeration — the requested spelling returns `None` on sight, an identity match is held and answered with only if that spelling never appears. Guard: `a_hard_link_beside_the_document_does_not_steal_its_name`, which links four aliases so no single `readdir` ordering can make it pass by luck; it failed against the pre-fix code with `Some("0notes.md")`.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-271). Kin: ScrAP-270 (the fix this was found inside).

## 272. A plan obligation written as a property of an artefact, which reads as done once the artefact exists
**Symptom**: a feature ships having satisfied its plan line by line, and the gap is found later by a different seat reading the same plan. Nothing was forgotten and nothing was disputed — the sentence was implemented, and the sentence was not the obligation.

**The instance.** The two-step case-only rename mints its intermediate name as `<document>.rename-<pid>-<seq>` so that a crash between the two steps leaves debris that can be told apart from a user's own files. The plan said, and the implementation's own comment repeated, *"a crash between them leaves the temp name behind — which is why it is recognisably `<name>.rename-*`"*. That is a true statement about a **string**, it was fully implemented, and **nothing anywhere recognised it**. The consequence is not a damaged file but an invisible one: the reader reports the document missing and offers to create a blank one over it, while the user's text survives under a name nothing in the app points at.

**Root cause.** The obligation was phrased as a property the artefact *has* (`recognisable`) rather than as a thing some code *does* (`recognise`). An adjective has no caller, no test and no reviewer question attached to it, so once the name is minted every reading of the plan checks out — including a careful one, including the author's, including a CAM sweep, because there is no row for "the counterpart of a thing we named". The word `recognisably` is doing the work of a whole feature and is grammatically indistinguishable from a comment about formatting.

**Why the usual gates cannot see it.** Coverage is full (every line of the minting code runs), the mutation tests pass (deleting the naming scheme breaks tests, so the naming *is* load-bearing), clippy and the reference lint are silent, and TDD §24 was complete against the plan as written. The only artefact that could have caught it is a rubric for the recogniser, and the rubric set was derived from the same plan.

**The corrective, which is a reading habit rather than a check.** When a plan or a comment asserts that something is *recognisable*, *recoverable*, *resumable*, *reversible*, *detectable*, *auditable* — any `-able` about a state the system can be left in — treat the adjective as naming a **second deliverable** and ask who its reader is. If the answer is "a human with a file manager", say so explicitly and ratify it; if there is no answer, the obligation is unfinished. Same test for the passive voice around a leftover: *"leaves the temp name behind"* has no subject picking it up. Note the shape of the near-miss that makes this worth writing down: the plan was unusually good — it anticipated the crash window, it specified the debris format, it used the right word — and being *nearly* complete is what made the remainder invisible.

**Scribobulate**: `docio::rename::recover_rename_orphan` is the missing recogniser, called from `docio::read_document_blocking` — the module's only door, so one placement covers Open, session restore, link navigation and crash recovery, and ordered *ahead* of the admission check so a recovered file is stat'd and size-capped like any other rather than admitted for having been absent when the check ran. Deliberately narrow, because it moves a file the user did not ask it to move: only when the path is absent (guarded inside the function, not only at the call site that already checks — a guard a caller has to remember is one refactor from a silent overwrite), only on exactly one match against the full `<document>.rename-<digits>-<digits>` shape rather than the `.rename-` infix a real document could carry, and only back to the pre-rename name the orphan encodes — the rename is not replayed, since a crash is no evidence the reader still wants it. Guards: `only_this_apps_rename_debris_is_recognised_as_an_orphan` (round-trips against the real producer so matcher and minter cannot drift), `a_document_stranded_by_a_crash_mid_rename_is_put_back`, `an_orphan_beside_a_present_document_is_left_alone`, `an_ambiguous_pair_of_orphans_recovers_neither`, and — separately, at the door rather than at the function — `a_document_stranded_mid_rename_is_recovered_by_the_reader` paired with `an_absence_with_no_orphan_is_still_a_new_document`, because the recovery is worth nothing if the reader concludes `Missing` before reaching it and that ordering is exactly what a later edit could invert without any unit test of the recovery noticing. Contract: TDD §24.14; checks: `tests/MANUAL-TEST.md` §24.

**Ratified: the recovery is silent to the reader** (logged only), which TDD §24.14 states and this records the reasoning for. The app is completing its own abandoned two-step transaction on its own marked debris, and the reader's world afterwards is exactly the pre-rename one — the document under the name they know — so there is nothing to accept and nothing to discard, and a notice would describe an internal mechanism rather than anything about their document. It differs from crash-recovery of *unsaved changes* (TDD §22) precisely there: that restores a buffer which disagrees with the disk, and so must be both announced and refusable. Revisit the moment recovery can hand back a document the reader did not last see. The one residue accepted with it: two orphans recover neither and log a warning, so a second crash leaves the content only under names the app does not show — visible in a file manager, not in the app.


## 273. A runtime skip announcement shredded by libtest's own progress output — and one shred read `SKIPPED [rubric]: ok`
**Symptom**: the pipeline's skip report prints a line that is a *different* test's name welded to half of a skip reason, or — twice in four measured runs of the real command — a line whose reason has been replaced outright:

```text
test copymap::tests::within_link_caption_excludes_brackets_and_url ... SKIPPED [TDD 24.13 stored spelling]: ok
SKIPPED [TDD 24.13 stored spelling]: test copymap::tests::within_heading_excludes_the_hash ... ok/tmp/.tmpVUbh7F
```

**Root cause, MEASURED** (this machine, `cargo test -- --nocapture`, the pipeline's own step-4b command): `--nocapture` makes libtest write its progress as `test <name> ... ` before a test runs and `ok\n` after, on the same stderr the test itself writes to, from as many threads as it runs tests on. A **formatted** `eprintln!` reaches stderr as one write *per format fragment* — `"SKIPPED [{limb}]: {why}…"` is prefix, arg, arg, suffix — so another thread's `ok` lands inside the announcement. A **literal-only** `eprintln!` is a single write and was never affected, which is exactly why this survived: every site whose skip had a *reason worth interpolating* was the vulnerable kind, and every site that had been safe all along was safe by accident of having nothing to say.

**Why it is worse than a garbled line.** The consequence is not noise, it is inversion. POLICY mandates this announcement precisely so a test that quietly did not run cannot be mistaken for one that passed — a runtime skip is the remedy for `#[cfg(platform)]` deleting a test (ScrAP-212). A reader greps `SKIPPED [`, and the mechanism hands them **`SKIPPED [TDD 24.13 stored spelling]: ok`**. The one line in the build whose entire job is to say *this rubric went unverified* renders as a pass, in the report a human reads to find out what went unverified. Note also that it is **intermittent and load-dependent** — it needs enough concurrent tests to hit the window, so it does not reproduce on a focused `--lib` run (6/6 clean) and does reproduce on the full one (2/4 corrupt), which is the shape that reads as "a flake in the output" rather than as a defect.

**Scribobulate**: `testsymlink::skipped` builds the whole line — newline included — and emits it with a single `std::io::stderr().lock().write_all()`; a sub-`PIPE_BUF` write is atomic on the pipe the pipeline reads through. Verified 6/6 clean on the same command that produced 2/4 corrupt. Every skip site routes through that helper rather than its own `eprintln!`.
**The residual, stated because the fix's limit is not where you would guess.** libtest's `test <name> ... ` prefix can still *share* the line — an atomic write cannot un-write what another thread emitted a moment earlier, and a post-fix run shows exactly that. What it can no longer do is get *inside* the announcement, so the rubric and the reason always arrive whole and the grep always matches. The distinction is the whole point of the fix: a line with a foreign prefix is untidy, a line whose reason has been replaced by `ok` is a lie. Do not "finish the job" by trying to suppress the prefix — that would mean fighting libtest for the stream, and the property that matters is already held.

**The second lesson, which is the same lesson the helper's own module header already taught and I re-learned anyway.** `skipped()` is documented there as "the general primitive… hoisted, because a remedy only reachable from the symlink helper is one the next `#[cfg(unix)]`-shaped test will not find" — and I wrote a fresh local `eprintln!` for a case-sensitivity skip without ever finding it, because the module is called `testsymlink` and my test had nothing to do with symlinks. **A shared remedy is discoverable by the name of the module it lives in, not by the doc comment inside it**, and a name that describes the *first* consumer quietly re-privatises it for every later one. That the module predicted this failure in prose and still suffered it is the point: prose inside a file cannot be read by someone who has no reason to open the file. The standing candidate is renaming the module for what `skipped` is rather than for what `symlink_or_skip` is; not done unilaterally, because it touches `lib.rs`, `gtk_suite.rs` and eight call sites.

**See**: project-specific tooling; the fix + rationale live in a code comment at `src/testsymlink.rs::skipped`. Kin: ScrAP-212 (the runtime-skip remedy this protects), ScrAP-270 (whose skip line was the one observed shredded).

## 274. A provenance tally that counts measurements instead of outcomes, and so reports the opposite of its evidence
**Symptom**: a summary line says measurement "corrected the source-traced record twice", a peer register builds a confidence policy on it, and the underlying evidence is one correction and two confirmations — i.e. the source-traced record has a *winning* record and the tally said it was losing.

**Root cause.** Upgrading a claim from SOURCE-TRACED to MEASURED is one event with two possible outcomes — the measurement **overturned** the claim, or it **confirmed** it — and the label `MEASURED` records only that the upgrade happened. Counting upgrades therefore counts *occasions on which source-tracing was checked*, while reading like *occasions on which source-tracing failed*. Here three claims were measured across two seats (kqueue event count: **corrected**; kqueue inode-following: **confirmed**; NTFS case-only rename: **confirmed**), and compressing them into "twice" inverted the conclusion a reader would draw. It survived because both descriptions were true in isolation — "no longer inferred" and "corrects the record" can each be said of the same upgrade, and only the tally makes them incompatible.

**Why it propagates rather than sits still.** A tally is the one part of a provenance record that other people *reason from* rather than look up. The skill maintainer marked a remaining source-only row **provisional** on the strength of this one — sound reasoning from a wrong premise — and the error would have shipped into a second register as a confidence policy. Provenance labels are load-bearing precisely because they are trusted without re-derivation; a wrong tally is therefore worse than a wrong claim, because a claim is checked by whoever uses it and a tally is not.

**The corrective.** Tally **outcomes, not events**: say "one correction, two confirmations", never "measured three times". And when a correction does land, characterise *what kind* of thing was wrong rather than lowering confidence uniformly — a blanket "provisional" predicts nothing and gives a reader nothing to do.

**Then this entry did the same thing again, one level up, which is the part worth keeping.** Having corrected the tally, I replaced it with a *characterisation* — "source-tracing gets every semantic right and gets counts wrong" — and shipped that to the skill maintainer, who wove it in. It is **also over-fitted**, and the researcher refuted it from the same investigation: they had predicted **exactly three** inotify events from source before running anything, and measured exactly three. A count, read off the source, right first time. So the second attempt was a nicer-sounding rule built on the same one data point as the first, and it propagated *further* than the tally did because it read as insight rather than as arithmetic. The replacement (ScrAP-269, SOURCE-READ) is narrower and names a mechanism rather than a tendency: counts are readable up to the **kernel boundary**; `CHANGED`/`CHANGES_DONE_HINT` are rate-limit coalesced at 800 ms *by design*; multiplicity the OS produces is what GLib cannot tell you. **And then a third time, which is what identifies the actual failure mode.** Writing this feature up I claimed the mechanism-substitute test "kept the branch executed for **months** before any HFS+ volume existed" — and sent that to the skill maintainer, who carried it. One `git log` falsifies it: the substitute landed at 12:27 and the HFS+ measurement ran the same evening. **Eight hours.** So the three are a set — a tally, a count, and a duration — and the common factor is not bad luck but *writing a quantity that was never measured*, each time in a sentence whose surrounding argument was sound. A number inherited from the shape of an argument rather than from a source is the thing to catch, and it is catchable: every one of the three took under a minute to check, and none of them was checked because none of them felt like a claim.
**Which is the second property, and it makes this a REVIEW blind spot and not only an authoring one** (skill maintainer's framing, and better than mine): *a number reads as an observation even when it is an estimate.* "Corrects the record twice", "`DELETED` once", "months" — none of them *look* like assertions needing support the way "source-tracing is unreliable here" does. They look like readings. That is why all three cleared two careful readers rather than one, with the reviewer holding the means to check two of them.
⇒ **The corrective is structural, not vigilance: quantities need provenance labels as much as mechanisms do.** This register is fastidious about MEASURED / SOURCE-READ / INFERRED for behaviours and was entirely silent about them for numbers, which is precisely the gap all three fell through. Label a count the way you would label a claim, or do not write the count. **The lesson is that a small sample does not stop being a small sample because you drew a more sophisticated conclusion from it** — and the tell is the same both times: a rule whose evidence is one event, phrased as a property of a whole practice. Ask for the counter-example before shipping it; the researcher's took one sentence to produce.

**And the thing that licenses the caution without borrowing confidence.** The tempting argument is asymmetric cost — over-trusting ships a bug, under-trusting sends someone to measure, so round toward caution. That treats a provenance label as a lever for producing behaviour rather than as a claim about what is known, and a register whose labels drift toward whatever produces good behaviour has labels that mean nothing when the stakes are real. **Asymmetric cost licenses naming the specific thing to go and measure — which costs nothing in accuracy — and nothing more.**

**Scribobulate**: ScrAP-269's macOS paragraph now splits the correction from the confirmation explicitly and states the semantics/multiplicity distinction; the rename feature's platform-gap record had its monitor-event-count row reopened for Windows after being closed wholesale on the strength of a *macOS* measurement — macOS closing is not evidence about Windows, and the two rows only looked like one row because they had been traced together.
**A fifth instance, from outside this project, sharpens what the check has to be aimed at.** Reviewing the ScrAP-290 weave, the `gtk4-rs` maintainer wrote that the tab strip's ~240 px drift was invisible because *every child is wrong by the same amount*. It is not: the displacement accumulates left to right (measured deltas 0, 0, 20, 40 … 240), because a slot is a running total over the children to its left — the very mechanism stated two sentences above it. Their own post-mortem is the reusable part and is better than the original framing here: the error was not inferring a quantity from a mechanism, which is often all that is available, but inferring the WRONG quantity from the RIGHT one, because the sentence was checked against whether it explained the SYMPTOM (why nobody reports it) rather than whether it followed from the CAUSE. **A plausible explanation of the symptom is not evidence about the mechanism**, and it is more dangerous than a bare unsupported number because it recruits the reader's agreement on the way past. Two corroborations landed with it: the sentence was catchable only because it was LABELLED inferred (they could see it was unsupported while still not seeing it was inconsistent), and in the very edit correcting it they typed "unreported for months", a duration nobody measured, caught before commit. The discipline is the check, not the care — care is what produced the sentence.
**See**: project-specific provenance discipline; no external home. Kin: ScrAP-269 and ScrAP-270 (whose provenance this governs), GTK4Rs/AP-141 (source-read vs inferred, the labels this miscounts).

## 275. A `GFileMonitor` created while its parent DIRECTORY is absent is permanently dead on Windows, and self-heals everywhere else
**Symptom**: live reload never works for one document, on Windows only, with no error, no warning, and a perfectly valid non-null `GFileMonitor` to inspect — while the same code on Linux and macOS starts working within seconds, so it is invisible to every developer not sitting at a Windows box.
**Scribobulate**: not reachable today — every site that attaches a monitor (`app::attach_file_backing`, the rename re-attach, crash-orphan recovery) does so for a document whose directory exists. Recorded because the reachable version is one ordinary feature away: a monitor armed on a not-yet-created path, or a document on a removable volume. If that is ever built, Windows needs its own rescan; GIO will neither provide one nor say so.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-275), which holds the three-layer source trace, the per-backend self-heal behaviour, and the two transferable halves (a well-formed question aimed at the wrong level; an API that cannot report failure is not one that does not fail).

## 290. A custom widget that caches child positions derived from child sizes, and re-derives them on nothing
**Symptom**: a tab opened into an already-overflowing tab strip is drawn on top of its left-hand neighbour, two labels superimposed, and stays there through a switch away and back and two window resizes — while every other tab looks evenly spaced.
**Scribobulate**: `widgets/tab/bar.rs`'s `with_entry_width_change` funnel (both width-changing setters fused to a retarget), `widgets/tab/ops.rs`'s `handle_width_changed` + `add_tab`'s re-derive-and-settle backstop, `widgets/tab/layout.rs`'s pure `target_positions`/`any_unsettled`; guarded by three per-mechanism `#[gtktest::test]`s in `widgets::tab::bar` (each mutation-tested, singly and paired) and MANUAL-TEST 7.20 / TDD 7.20.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-290), which holds the measurement, the accumulating-displacement mechanism, the repaint-bug misdiagnosis signature and the GTK4Rs/AP-254 two-sufficient-mechanisms note. Canonical text is there, not in this file's git history — it was stubbed at mint time.

## 291. Every `GtkAdjustment` write is clamped, so revealing something added in the same turn scrolls short by exactly its own width
**Symptom**: a tab appended to an overflowing strip is made active but never becomes visible — it sits clipped just past the right-hand edge for good, while every other way of scrolling reveals it perfectly.
**Scribobulate**: `widgets/tab/ops.rs`'s `scroll_into_view` republishes the range (`scrollpos::reconfigure` over `layout::scroll_extent(content_extent(), viewport)`) before writing a position, with `content_extent()` the single definition `size_allocate` also reads; guarded by `switching_to_a_just_added_tab_scrolls_it_fully_into_view` (mutation-tested) and MANUAL-TEST 7.20 / TDD 7.20.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-291), which holds the measurement and reframes that module's clamp family — lazy validation is the family's trigger, not the clamp's precondition. Canonical text is there, not in this file's git history — it was stubbed at mint time.

## 292. A `GFile` built from an `https://` URI resolves only where a GVfs backend claims the scheme
**Symptom**: every remote image in a document stays a broken-image placeholder on macOS with the safety toggle ON — while the identical document renders them on Linux *and* on Windows (three platforms, three different answers: `gvfsd-http`, GLib's in-process `GWinHttpVfs`, and nothing at all) — and the placeholder's tooltip says the bytes would not decode, because the error that says no request was ever attempted is swallowed by an `Option`.
**Scribobulate**: `imagefetch.rs` owns the fetch (an explicit HTTP GET, bounded by connect/global timeouts and `limits::MAX_REMOTE_IMAGE_BYTES`) and `renderer::start::load_remote_texture` decodes it with `GdkTexture::from_bytes`, logging fetch and decode failures separately at `warn`; the transport is replaced for **every** platform rather than behind a `platform/mac/` seam, since the substitute is portable — which makes it a *replacement of working behaviour* on the two platforms that had a backend, so the client verifies against the machine's trust store and honours the Windows proxy settings (the replaced routes both did), or the macOS fix would have landed as a corporate-desktop regression elsewhere. Guarded by `imagefetch`'s cap/scheme unit tests and MANUAL-TEST 14.2a / TDD 14.2, which drives it with GVfs disabled (`GIO_USE_VFS=local`) so the platform gap is reproduced on Linux rather than assumed.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-292), which holds the measurement, the daemon-not-library mechanism, and the general lesson about a toolkit API whose capability is supplied by a separate desktop component. Canonical text is there, not in this file's git history — it was stubbed at mint time.

## 293. Sizing a drawn affordance from one font's row height and fitting it to a container laid out in another
**Symptom**: a code block's copy button appeared on every multi-line block and on **no** one-line block — no warning, no log line, and the happy-path test (a multi-line block) green. The fit rule refused a button whenever the card was shorter than the button plus its corner inset, and a one-line card is 39 px against a 42 px ideal at the default configuration, because the button's size derived from the **body** row (18 px) while the card is laid out in the `code-block` tag's **monospace** row, which is shorter.
**Scribobulate**: `affordance::copy_button_rect` yields instead of refusing — it keeps the derived size, collapses the vertical inset to centre the button in whatever the card has, shrinks to the container only as a last resort, and floors at one text row. Chosen over "measure the tagged row instead" deliberately: the yielding rule is correct whether or not the reading of `create_pango_layout`'s font source is right, so the corrective does not rest on the weakest evidence in the diagnosis. The true extent was in hand all along — the card rectangle comes from `line_yrange` and already carries the tag's real row height and padding.
**See**: gtk4-rs skill → textview-layout-and-drawing, as a rider on GTK4Rs/AP-145, which recommends the very call that produces the wrong number ("the view's own CSS-zoomed font") and is correct where it stands, for a list item whose text *is* the body font. The skill holds the general form: you are computing a fit against a container you have already measured.

## 294. Letting a coverage ratchet be satisfied by widening the exclusion instead of testing the code
**Symptom**: a change whose only new logic was display-free and fully unit-tested still failed build-pipeline step 6. The feature's GTK wiring had landed in `preview/interactions.rs` — in scope, and 0% covered like every preview wiring file — while its decidable half sat in `codeview/`, which the scope regex excludes wholesale. Net: ~28 uncovered lines added, ~0 covered, and the scoped total 0.08 pt under the floor.
**Scribobulate**: the pure geometry and the shared point-in-rectangle hit test moved out of the excluded `codeview/` tree into `affordance.rs`, where the gate counts them and their tests; `FLOOR` rose 77.72 → 77.75 in the same change, with the reason recorded beside it. The rejected alternative is the point of this entry — adding `preview/*.rs` to `IGNORE` would have raised the number by roughly 900 lines of hidden code and satisfied every gate. **A ratchet measures what it is pointed at, so the only honest way to move it is to point it at more code, never at less.** POLICY already names extraction as the mechanism by which the floor rises; this is the worked instance, and the tell is that the failing number appeared in a change that added no untested logic at all.
**See**: project-specific (process/tooling; the routing rule keeps these here). POLICY § Build pipeline step 6 for the rule, `scripts/coverage.sh` for the floor, the scope and the per-module rationale.

## 295. A PID-qualified AppleScript process reference decays to name resolution once stored
**Symptom**: driving the app on macOS, `first process whose unix id is <pid>` used **inline** inside a `tell` block works; the identical filter **stored in a variable and reused** fails on the same kind of target, and the error's own text has dropped the PID qualifier — `Can't get menu bar item "View" of menu bar 1 of application process "scribobulate"` against the inline form's `... of process 1 whose unix id = 88574`. Separately, that bare-name resolution picks a **fixed** process among same-named duplicates (first-launched, as observed) regardless of which is frontmost, which is how a driven click reached the operator's own live window instead of the test instance.
**Scribobulate**: no code — a harness rule for `tests/MANUAL-TEST.md` §A.2. Re-derive the PID-qualified reference **inside every `tell` block**; never bind it to a variable and reuse it. This is GTK4Rs/AP-131 and ScrAP-253 one layer up: those say identify your instance by PID rather than by name, and this says a PID-qualified *reference* is not the same thing as a PID-qualified *lookup* — something else re-resolves it at use time.
**Scope, honestly.** MEASURED: the inline form succeeding and the stored form failing on plain-text targets, on a fresh PID, with an emoji-name confound ruled out by re-running. NOT established: whether the stored form, had it resolved, would have hit the **wrong** process or merely errored — it was only ever seen to error. Deliberately not chased, because the corrective is identical under both answers, which is the same reason the fit rule in #293 was chosen over the better-measured one.
**Also from the same episode, and the more transferable half**: the first report of this described a difference in **error wording** as a difference in **outcome** — two lookups failed, by two routes, with different messages, and the difference in message was read as a difference in behaviour. It was caught by the reporting seat and re-run clean. A difference in an error message is not a difference in behaviour until something rules out the alternatives.
**See**: project-specific (agent tooling; the routing rule keeps these here). Also routed to the `gtk4-rs` skill as a rider on GTK4Rs/AP-163, whose macOS drive path already carries non-GTK mechanics of exactly this kind.

## 296. A derived screen coordinate is only as trustworthy as the derivation behind it
**Symptom**: a UI-driving seat replaced estimated coordinates with *derived* ones — grow along the code-block card's fill to find its extent, then look for the button inside the card's right end — and got four consecutive readings of `card h=8, NO BUTTON` for blocks that plainly had one, on a fix that was about to be reported as a platform-specific regression. The locator grew the card's extent **along the pointer's own column**, and code text interrupts the fill on every row, so it stopped a few pixels in. What caught it was opening the capture and looking at it.
**Scribobulate**: `tests/MANUAL-TEST.md` §1 and §A.3 — derive the coordinate rather than estimating it, **and give the derivation its own sanity check**: grow along a column the content cannot interrupt, and confirm the derived rectangle against the image before any assertion rests on it.
**Why it earns an entry of its own beside ScrAP-252.** 252 says a setup step that silently fails makes the next assertion grade the previous state; the corrective everyone takes from it is "derive, don't estimate". This is the failure mode *of that corrective*: a derivation is code, code has assumptions, and a wrong one yields numbers that are **confident, repeatable and reproducible across runs** — which is precisely the signature people read as a real defect rather than an instrument fault. An estimated coordinate is obviously a guess and gets checked; a derived one carries unearned authority. **Deriving the coordinate is necessary and not sufficient.**
**The check that worked is the cheapest one available**: look at the picture. Both instances this pattern produced in one session — an estimated coordinate that missed by 107 px, and a derived one from a broken derivation — were settled in seconds by viewing the capture instead of reasoning about the numbers, after minutes of reasoning had pointed the wrong way. A driven UI test that never looks at its own screenshots is one bad locator away from a fabricated bug report (ScrAP-236 is the same lesson about *what* was captured; this one is about *where you decided to look in it*).
**See**: project-specific (agent tooling; the routing rule keeps these here). Kin ScrAP-252, ScrAP-236.

## 297. Finalizing a cancelled `GFileMonitor` after the main context has dispatched corrupts the process heap (Windows)
**Symptom**: a GTK/Rust test process on Windows dies `0xC0000374` STATUS_HEAP_CORRUPTION with **no Rust panic, no GLib warning and no failing assertion** — every test reports pass, and the process dies mid-body of whichever test finalizes a cancelled monitor. It reads as accumulated corruption surfacing at teardown, and it invites blaming whichever test happens to sort last. Both readings are wrong, and the deleted issue entry asserted both — it was neither deterministic (same binary and arguments, dying then passing) nor the last test to run (the 254 cases sorting after it pass).
**Root cause** (MEASURED by the Windows seat; gvsbuild GLib/GIO **2.88.1**, GTK 4.22.4, MSVC, Win10 19045): finalizing a **cancelled** `GFileMonitor` after the main context has **dispatched** aborts inside `g_object_unref`. Reduced to a loop containing no application code — `monitor_file(NONE, Cancellable::NONE)` → `cancel()` → `MainContext::default().iteration(false)` → `drop`. Ingredient matrix, 400 iterations per row with `HeapValidate` after every finalize: cancel **plus a dispatch between cancel and finalize** dies on the first iteration; cancel with **no** dispatch before finalize is clean; cancel plus a wall-clock **sleep** only is clean; cancel with the dispatch **after** the finalize is clean (2×400); no cancel at all is clean either way. A rename of the watched file and a `connect_changed` handler both drop out entirely. So the cancel is necessary, and the trigger is main-context **dispatch**, not elapsed time — GTK4Rs/AP-261's "which clock" question, whose answer here is *neither*.
**Scribobulate — safe by construction, not by a gate.** Both cancel sites (`window::rename`'s `cancel_monitor`, `app::open`'s `attach_file_backing`) release the last reference in one uninterrupted stretch with no main-loop turn between cancel and drop, which is the measured-clean row. Nothing enforces that: **any future change that parks a monitor reference across a turn and then drops it reintroduces a silent process kill on Windows only** — invisible to the Linux and macOS seats, and not a test failure but a dead process. The corrective for the one exposed site is `std::mem::forget` on the monitor in the rename test that interrogates `is_cancelled()` after the rename is dispatched — the only place in the tree holding a second reference to a cancelled monitor across a pump, and it does so for a good reason. Leaking one GObject per run keeps the strong assertion rather than weakening it to something that finalizes safely; the mutation check confirms it does not blunt the guard (with `cancel_monitor` removed the test still fails, on its own assertion, and now fails *as a test failure* instead of aborting the process).
**Now enforced by `saferizer::DocMonitor` (not `Clone`; `cancel_and_release` consumes it), with `gio::prelude::FileMonitorExt::cancel` and `FileExt::monitor_file` banned in `clippy.toml`** — so the ordering is unrepresentable rather than merely correct today. The observing test holds an `ObservationHandle`, whose reference is never released *by construction* rather than by a `mem::forget` a later reader could tidy away.
**Two traps in the diagnosis, both worth more than the fix.** (1) Bisection reported halves of 33 and 56 cases clean and their union of 89 dying, which reads as a cumulative heap-pressure threshold and is a **false lead** — it is nondeterminism, not accumulation, and the same binary alternates outcomes on identical arguments. (2) The first `HeapValidate` probe made the crash **vanish** (261/261 green) and was nearly filed as "instrumentation perturbs it, unfixable". It was Rust's **block-buffered** stdout when piped: the `println!` naming the last-passing case died in the buffer with the process, so the visible last line named the wrong moment, and flushing before the probe moved the apparent death site by a whole test. That is ScrAP-236's lesson at a different sensor — **the artefact was real and the locator was wrong**.
**Scope, honestly.** NOT established: the GIO defect's own mechanism (double free vs use-after-free vs a stale pointer into the freed `GSource`) — the trigger is in hand, no GLib source was read. No page heap, no Application Verifier, no debugger, no ASan; the box is not elevated, so `HKLM` IFEO heap instrumentation was unavailable and the finding rests on black-box `HeapValidate` plus the matrix. The reproducer uses only GIO calls but ran inside a `gtk::init()`ed process, so "needs no GTK" is untested. Linux and macOS are unmeasured; the corrective is inert there. And the versions are nowhere near this project's 2.72.4 / GTK 4.6 floor, so this can only be a claim about 2.88.1 — GTK4Rs/AP-272's asymmetry: a floor claim can be falsified from here, never confirmed.
**See**: gtk4-rs skill → app-lifecycle-and-env (glib/gio are in the skill's scope; routed for weaving, stub to follow once a number is allocated there). Kin ScrAP-269 (the same API, from the delivery side: cancel does not drain the queue, and a freshly attached monitor is not yet watching), GTK4Rs/AP-262 (a toolkit abort with no Rust panic is not a bad test), ScrAP-236.

## 298. A TIGHT list item's content arrives as bare inline events with no `Tag::Paragraph` wrapper
> *Non-core (pulldown-cmark/CommonMark) — parser event-stream behaviour, not a GTK lesson. Do not fold into the gtk4-rs skill. Sibling of #147/#66/#75 (the same register of pulldown emission surprises).*

**Symptom**: an exported document breaks its lines after almost every token inside a numbered or bulleted list — `POLICY.md`, then a line break, then the next four words, then a break, then a comma on its own line. Only *inside* list items; the same prose at top level is fine.
**Root cause**: pulldown-cmark wraps a **loose** list item's content in `Tag::Paragraph` and a **tight** item's in nothing at all — the inline events arrive directly inside `Tag::Item`. A consumer that reaches for "no inline container is open, so start a paragraph" therefore starts a *new* paragraph for every inline event the item contains: one for the text run, one for the inline code, one for the link, one for the soft break. Plain prose survives because a whole text run is one `Text` event; the moment an item contains a second inline of any kind, it shatters.

**Why it survived the tests, which is the transferable half.** Every list fixture in the suite used single-word items (`- one`, `- two`). For an item whose content is **one** inline event the broken path emits exactly one paragraph — byte-identical to correct. The defect is invisible below two inline runs per item, so a fixture has to contain inline code, a link, emphasis or a soft break *inside a list item* to detect it at all. It took exporting a real document (`sdd/PLAN.preview-decoration.md`, whose every numbered item cites a filename in backticks) for the operator to see it. **A fixture that cannot distinguish the broken output from the correct one is not coverage**, and "the list tests pass" read as though it were.

**Scribobulate**: `src/export/walk.rs` carries an implicit-paragraph frame — `Open::ImplicitParagraph`, opened lazily by `Builder::push_inline` the first time an inline arrives with an empty inline stack, and closed by `Builder::flush_implicit` into the enclosing block frame. **Both halves are load-bearing and the second is the subtle one**: the flush fires only at **block** boundaries, gated by `is_block_start`/`is_block_end`. Flushing on every `Start`/`End` was the first attempt and it re-split the item at every link, because a link is an inline construct whose edges are not paragraph boundaries — it belongs *inside* the paragraph. Those two predicates are written as "not one of the inline set" rather than by enumerating block tags, so a construct pulldown adds later defaults to *block* and closes a paragraph, which is the safe direction. `flush_implicit` also runs before `Event::Rule` and in `Builder::finish`, so nothing is left unflushed. Regression guards: four tests in `export::doc::export_doc_tests` (a tight item with several inlines; a tight item containing a link; a **loose** item keeping each of its paragraphs, which confirms loose was never affected; a tight item with a sublist flushing its own text first), `tests/MANUAL-TEST.md` §25.3a, and TDD 25.3, whose predicate now states outright that **the fixture must give an item two or more inline runs**.

**Scope, honestly.** The preview's own renderer (`renderer::Renderer`) does not exhibit this — it never had the "no container, so open a paragraph" fallback, so it was never at risk, and the bug was confined to the export pipeline from the day it was written. This entry exists because the export was the *second* consumer of that event stream and hit a property of the stream that the first consumer had simply never had occasion to encounter; a **third** consumer would hit it again. Measured on pulldown-cmark 0.13 with this project's `md_options` allowlist; the tight/loose distinction is CommonMark's, so the behaviour is not expected to be version-specific, but only 0.13 was tested.
**See**: project-specific; the fix and its rationale live in code comments at `src/export/walk.rs` (`Open::ImplicitParagraph`, `Builder::flush_implicit`, `is_block_start`). Kin #147 (raw HTML emitted per-line and wrapped in `Tag::HtmlBlock` — the same class of "the parser's framing is not the framing you assumed"), #66/#75 (pulldown flanking and fragmentation).

## 299. A suite-ordering defect that is deterministic on one platform and invisible in the canonical platform's full suite
> *Core half — GLib `MainContext` ownership across libtest threads — routes to the `gtk4-rs` skill under the glib/gio rule; the tooling/process remainder (which platform is a witness, and what a green gate proves) stays here in full. Same precedent as #36 and #124. Authored by the Windows seat; allocated and formatted by the Linux seat, reasoning and MEASURED/INFERRED labels unchanged.*

**Symptom**: a test fails deterministically on Windows while the full Linux suite is green — and the *same defect reproduces on Linux* the moment two tests are run as a pair. The test in question waits on a GLib-delivered signal (a `GFileMonitor::changed`) and burns its whole 5 s deadline, while running alone it passes in 0.05 s. Because it presents as a timeout, the natural "fix" is to wait longer, which never works.
**Root cause (core half, MEASURED)**: a `#[gtktest::test]` body run earlier in the same binary leaves the default `GMainContext` **held by another thread**; a later plain `#[test]` runs on a different libtest thread, so `MainContext::default().iteration(false)` cannot acquire it, dispatches nothing, and the event stays queued. Confirmed by `MainContext::acquire()` — `Err("Failed to acquire ownership of main context, already acquired by another thread")` with a GTK body first, `Ok` alone. **Root cause (project half, MEASURED)**: nothing about either *platform's behaviour* differs — only the test **order** does, and no gate asserts order. Whole-suite ordering happens to leave the default context free on Linux and does not on Windows.
**Why this is a project lesson and not only a skill one**: POLICY holds **Linux as the gate for GTK-level regressions**, and here Linux is the platform that structurally cannot see it (1236/1236 green, 65 s), while Windows sees it every run. "It's green on the gate platform" is precisely the reassurance that hides it.
**Reproducer, and it is the load-bearing part** — one GTK body earlier in the binary is sufficient:
```
cargo test --features gtk-integration-tests --lib -- saferizer::buffer_mark saferizer::file_monitor   # FAILS, burns 5 s
cargo test --features gtk-integration-tests --lib saferizer::file_monitor                            # PASSES, 0.06 s
cargo test --features gtk-integration-tests --lib saferizer:: -- --skip gtk_integration_tests        # PASSES, 21 tests
```
**Two natural hypotheses that are WRONG**, each predicting the same observation: (1) *fresh-watch timing* (GTK4Rs/AP-269) — refuted, the test already uses wall clock, repeated writes and a 5 s deadline per GTK4Rs/AP-261, and in isolation the first write lands in 0.05–0.06 s; the timing discipline was never the defect. (2) *the `--test-threads=1` pin* in Windows's contract — refuted, it fails identically in parallel 3/3 and under Linux's own unpinned step-5 command shape. A third guess, that the sibling test in the same module was the polluter, is also wrong: the two file-monitor tests alone pass together in 0.06 s.
**Resolution**: give the test a context it owns — `MainContext::new()` entered via `with_thread_default` **before** the monitor is attached, so the source lands on that context. Not "pump harder" and not a longer deadline: `iteration(false)` on a context you do not own dispatches nothing, and no amount of waiting fixes a pump that is not pumping.
**Corrective, as a rule**: when a test waits on anything the main loop delivers, **own the context explicitly rather than borrowing the default** — the default context's ownership is a property of whoever ran before you, which is not something a test may assume. And when a suite is green on the gate platform and red elsewhere, **minimise to a pair before concluding "platform-specific"**: the two-test pairing cost minutes and turned a Windows bug report into a tree-wide defect.
**The witness corollary, and it generalises past this bug**: a step declared NOT APPLICABLE on a platform correctly excuses that platform *and* removes it as a witness. Step 6 is contract-declared not-applicable on Windows (unix-only code not compiled) and permanent-N/A on macOS — which is exactly how a `FLOOR` raised above the tree's actual scoped coverage stayed red on the only platform that runs it, unnoticed, in the same week as this. **Same shape, different step: the excuse and the blindness are the same declaration.**
**Family — the false-PASS cluster, on a NEW axis: suite ORDERING.** Kin by origin: GTK4Rs/AP-78 (from the setup), GTK4Rs/AP-168 (from the assertion target), GTK4Rs/AP-254 (from a masking second mechanism), GTK4Rs/AP-160 (from the environment), GTK4Rs/AP-272 (from the build configuration). Unlike GTK4Rs/AP-160's, the blind platform here is the **canonical** one. Kin here: ScrAP-265 (a test leaving process-global state armed re-points the rest of the suite — and GTK initialisation *is* process-global state that cannot be restored, which POLICY already concedes) and ScrAP-212.
**Scribobulate**: `src/saferizer/file_monitor.rs` — the `changed`-adapter test enters its own `MainContext` before attaching. **The honest limit of that guard**: it pins the *fix*, not the *class*. Nothing in the tree asserts that no other test borrows the default context, and no claim is made that anything does.
**Measurement provenance**: MEASURED Windows 10 19045 / GTK 4.22.4 gvsbuild / MSVC (deterministic 2/2 full runs, 3/3 pairings) and MEASURED Linux / GTK 4.6 / Xvfb (same pairing, same shape, full suite green). The mechanism was INFERRED on both until probed on Linux with `MainContext::acquire()` (glib 0.21); it is now MEASURED there. Still not source-traced.
**The instrument trap, which cost two runs and nearly produced a false refutation.** `g_main_context_is_owner()` is the obvious probe and it is **useless here**: it answers *"does **this** thread own it"*, which is `false` in the working case and the broken case alike — a confident, well-formed, non-discriminating answer, the P-3 shape. `acquire()` is the discriminator. **And the experiment must be serialized**: under parallel libtest the probe ran *before* the polluting body and returned `Ok` both times, reading exactly like a refutation of the mechanism. The result only appears under `--test-threads=1`. Note the apparent contradiction, because a summary will flatten it — the thread pin is **not the cause** (refuted above) but **is required to observe the cause**.
**Non-core (tooling/process remainder — which platform witnesses a defect, and what a green gate proves) — do NOT fold into the gtk4-rs core skill.**
**See**: gtk4-rs skill → threading-async-and-memory (**GTK4Rs/AP-293**) for the mechanism, and its ui-testing-verification module for the transferable "green is a property of the invocation, not the platform" form beside GTK4Rs/AP-160. Note the numbers do not correspond — this register's spaces are unrelated to the skill's, as the header says and as ScrAP-88 ↔ GTK4Rs/AP-79 demonstrates.
