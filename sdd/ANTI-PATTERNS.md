# Anti-Patterns

Scribobulate's register of costly dead ends. It is a **project index, not an essay collection**: each entry is a few lines — the trap, where *this* tree implements the fix, and a pointer to the reusable home of the full lesson. The full essays live in this file's git history (`git show f725e67:sdd/ANTI-PATTERNS.md` is the last long-form revision). Read the table of contents, then only the entries whose titles match the task (SDD principle 7).

**Citation convention.** An entry here is `ScrAP-N` (bare `#N` only inside this file); a `gtk4-rs` skill entry is `GTK4Rs/AP-N`, one of its techniques `GTK4Rs/T-N`; a `general-engineering-principles` entry is `GEP-N`. A bare `AP-N` or `T-N` is illegal anywhere in the tree (`cargo xtask lint-references` check 8). Skills are named, never pathed — they may not be installed on every machine. When both registers hold a lesson, cite `ScrAP-N`; it is always resolvable.

**Routing rule — applied when an entry is MINTED, never in a later migration.**
1. About gtk4-rs itself (gtk4, glib, gio, gdk)? → weave it into the `gtk4-rs` skill; leave a stub here (`**Scribobulate**` + `**See**` lines only).
2. General engineering discipline that survives deleting every Scribobulate noun? → route it to `general-engineering-principles`, cited `GEP-N`; leave a one-line `**Routed**` tombstone here. No ScrAP number is needed for provenance.
3. Neither — Scribobulate internals, or a non-gtk4-rs dependency (Pango, GtkSourceView, pulldown-cmark, librsvg, syntect, serde/toml, the toolchain)? → it stays here, **in ≤ 6 lines**: Symptom · Root cause · Resolution · Lesson · Scribobulate · See. Extend an existing entry rather than minting a sibling for the same root cause. Route a Pango lesson on whose API *contract* it is about, and raise it before routing.

**Numbers are frozen** (check 9): never renumbered, never reused; a retired entry keeps its `## N.` heading as a landing spot. Reserved gaps — do not fill: **176–179** (Windows port; holder gone, held pending operator resolution), **186** (`feat/spelling`, inbound), **276–289** (unmerged branches). **Next free number: 338**+ — check this table and announce the range you claim; never derive it from the highest heading below.

**Growth** is gated in bytes (check 11). The ratchet only tightens; consolidate in the change that trips it.

**Disposition** (`Disp`): `A` gtk4-rs · `B` general-engineering-principles · `C` resident · `D` dead landing spot.

| # | Anti-pattern | Disp |
|---|--------------|------|
| 1 | Rendering a document viewer with a GPU-compositing UI stack | A |
| 2 | Assuming "disable hardware acceleration" makes a web engine render on the CPU | A |
| 3 | Using environment variables to prevent GTK from crashing on a large XCompose file | A |
| 4 | Using Pango `<a href>` markup in GtkLabel for standalone link widgets | C |
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
| 41 | `FileChooserNative`/transient-dialog lifetime is backend- and widget-type-dependent — including WHEN you may tear one down | A |
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
| 132 | A guard test whose INPUT SET is not the thing it polices — a wrong scope filter, or a hand-maintained mirror — passes forever | B |
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
| 248 | A randomly-minted identity correlates only with the mechanism that persisted it | B |
| 249 | A capability whose backend is a HELPER EXECUTABLE is a packaging obligation, and the dev tree cannot fail the test | B |
| 250 | A widget swapped in for one feature's sake moves its text out of every text-walker's reach | C |
| 251 | A distribution GTK has its entire introspection surface compiled out, and the three channels fail in three different ways — one of them by reporting a number that means "healthy" | A |
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
| 264 | A focused anchored child swallows its host `GtkTextView`'s navigation key bindings, and the document silently refuses to move | A |
| 265 | A test that arms a process-global fatal-signal handler and never disarms it re-points the rest of the suite — and displaces the runtime's own stack-overflow guard, so a later overflow stops naming itself | B |
| 266 | A focused popover that is its own `GtkNative` is an application-keyboard dead zone | A |
| 267 | A `GtkSingleSelection` built `.model(…).autoselect(false)` opens with a phantom selection | A |
| 268 | A GLib **fatal** log message dies of `SIGTRAP`, not `abort()`, so a crash handler that takes the classic four signals reports nothing for the whole `g_error` class | A |
| 269 | Two sufficient monitor-cancel mechanisms made the rename guard pass with its own fix deleted — and a freshly attached `GFileMonitor` is not yet watching | A |
| 270 | Asking GIO what a file is called after you renamed it, and being told what you asked for | A |
| 271 | Matching a directory entry by `id::file`, which identifies the FILE and not the entry | A |
| 272 | A plan obligation written as a property of an artefact, which reads as done once the artefact exists | B |
| 273 | A runtime skip announcement shredded by libtest's own progress output — and one shred read `SKIPPED [rubric]: ok` | B |
| 274 | A provenance tally that counts measurements instead of outcomes, and so reports the opposite of its evidence | B |
| 275 | A `GFileMonitor` created while its parent DIRECTORY is absent is permanently dead on Windows, and self-heals everywhere else | A |
| 276 | A parity artefact written to the console instead of the success stream — the documented diff produced an empty file, and the self-test rebuilt the list rather than calling the printer | B |
| 277 | A test whose subject IS a by-design CRITICAL blocks a process-wide fatal-criticals switch — and "do not weaken the test" was the wrong reading of the trade | A |
| 278 | A filename or file-existence predicate standing in for a semantic question — three measured cases in one night, each confidently wrong | A |
| 279 | `gvsbuild --configuration release` compiles GTK's assertions OUT, so the development box cannot enforce the contracts CI enforces | A |
| 280 | Provisioning for a machine you cannot inspect — installing a tool the image already had, and discovering one path component while pinning its sibling | B |
| 281 | A corpus that exercises the PATTERN cannot see a bug in the FLAG on the call site that consumes it | B |
| 282 | An operation counter is a complexity oracle only for operations you control | C |
| 283 | A false premise can file a measurable fact as unmeasurable, and the two protect each other | B |
| 284 | A dialog raised over a natively full-screen macOS window seizes a Space of its own and leaves the parent black | A |
| 285 | *(merged into 245)* A drive tool's zero exit is a claim about the tool, never about delivery | B |
| 286 | A reconciliation agreed in the room and never written into the artefact reopens on the next read | B |
| 287 | A scope claim is only as wide as the thing it was measured over — a runtime survey answered an attribution question | B |
| 288 | `$PSScriptRoot` is empty while `param()` defaults bind under `powershell -File`, and correct everywhere else | B |
| 289 | An HTTP 200 is a claim about the transaction, not about the document — four fetched licence texts were anti-bot pages | B |
| 290 | A custom widget that caches child positions derived from child sizes, and re-derives them on nothing | A |
| 291 | Every `GtkAdjustment` write is clamped, so revealing something added in the same turn scrolls short by exactly its own width | A |
| 292 | A `GFile` built from an `https://` URI resolves only where a GVfs backend claims the scheme | A |
| 293 | Sizing a drawn affordance from one font's row height and fitting it to a container laid out in another | A |
| 294 | Letting a coverage ratchet be satisfied by widening the exclusion instead of testing the code | C |
| 295 | A PID-qualified AppleScript process reference decays to name resolution once stored | B |
| 296 | A derived screen coordinate is only as trustworthy as the derivation behind it | B |
| 297 | Finalizing a cancelled `GFileMonitor` after the main context has dispatched corrupts the process heap (Windows) | A |
| 298 | A TIGHT list item's content arrives as bare inline events with no `Tag::Paragraph` wrapper | C |
| 299 | A suite-ordering defect that is deterministic on one platform and invisible in the canonical platform's full suite | B |
| 300 | A driven UI step that misses its target does not fail — it acts somewhere else, and a loop that does nothing produces a perfectly stable measurement | B |
| 301 | A save chooser returns a foreign extension unchanged, so an export default derived from the document's filename overwrites the reader's source | A |
| 302 | Both of `GtkPrintOperation`'s completion signals misreport, in both directions | A |
| 303 | On the preview route `render_page` ends the page — do not also call `show_page` | A |
| 304 | cairo's Windows colour-glyph path wraps colour glyphs in a Type3 `d0` font, and one 2017 extractor mishandles it | B |
| 305 | A `#[gtk::test]` body and a plain `#[test]` calling `gtk::init()` cannot share a binary | B |
| 306 | A chooser's `set_current_folder` is best-effort and its failure is unobservable | C |
| 307 | macOS embeds a colour emoji as a bare Image XObject, so its text is absent by construction | B |
| 308 | A font's Unicode flag does not predict whether its text extracts | B |
| 309 | A cancelled export destroys the destination, and the wreckage is a valid file | A |
| 310 | An extraction failure is not evidence about appearance | B |
| 311 | Which of two same-key bindings wins is a property of the BACKEND, not of the toolkit | C |
| 312 | Repairing the editor buffer when the defect is in the source string — the preview never reads the buffer | C |
| 313 | `GtkTextBufferContent` refs the buffer and never unrefs it — a whole document leaks per select-then-deselect | A |
| 314 | Instantiating ANY `sourceview::Buffer` subclass corrupts the heap — and the backtrace is innocent | C |
| 315 | Laying a table out with tab characters — a tab ladder cannot express a column | C |
| 316 | A repair handler on `insert-text` is also a handler on the UNDO machinery, and the divergence it causes is silent | C |
| 317 | A counter that stops being able to SEE its subject reports the intervention as a success | B |
| 318 | Silencing a whole log DOMAIN to arm a gate also silences the defects that domain is the only signal for | A |
| 319 | A portability gate whose verdict depends on which `grep` is first on PATH — and the seat that should catch the bug is the seat that hides it | B |
| 320 | The unresolvable pointer — a reference whose target the reader cannot dereference, mistaken for a delivery | B |
| 321 | The spurious kill — a mutation run that scores its own breakage as detection, and certifies coverage that does not exist | B |
| 322 | A control is a property of a CLAIM, not of a probe — and one control makes every other claim feel covered | B |
| 323 | `Clipboard::formats()` answers a question about the PROCESS, not about the content | A |
| 324 | A compiled-in asset resolved against a runtime directory is absent everywhere that directory isn't | C |
| 325 | A whole-struct `{:?}` in a completeness digest degenerates the guard into a restatement of what the producer already guarantees | B |
| 326 | A `const`-evaluated constructor scores ZERO in llvm-cov, so code exercised at every build reads as dead | B |
| 327 | `TextTag::property_value("*-rgba")` formats a POINTER under `Debug`, not the colour | A |
| 328 | A gdk-pixbuf dimension probe must `set_size(0, 0)`; any non-zero size still allocates the bomb | A |
| 329 | A gate read through a pipe reports the pipe's last stage, not the gate | t |
| 330 | A seam that exists is not a seam that is called | B |
| 331 | A vocabulary rename that reaches a selector | B |
| 332 | Re-styling a background view in place, and trusting a headless run to prove it | A |
| 333 | A repeating pattern anchored to a viewport-CLAMPED extent is pinned to the screen, not the document | A |
| 334 | A repaint failure that also happens in an unrelated application is upstream, and the platform seam it invites is the wrong response | C |
| 335 | A generated stylesheet GTK refuses loads silently, and the guard that could see it was scoped to one property | A |
| 336 | A `GtkPaned` child dragged to nothing still reports its full natural height, so `height()` cannot answer "was it crushed away" | A |
| 337 | A precondition implicit in a whole script is reported by whichever line violates it first, at that line's layer, after everything before it has run | B |

---

## 1. Rendering a document viewer with a GPU-compositing UI stack
**Scribobulate**: the founding decision — native GTK4 + Cairo **software** rendering (no GPU compositor); see PRODUCT.md / TECH.md.
**See**: gtk4-rs skill → architecture-and-rendering (GTK4Rs/AP-1).

## 2. Assuming "disable hardware acceleration" makes a web engine render on the CPU
**Scribobulate**: WebKit rejected outright; native widgets only.
**See**: gtk4-rs skill → architecture-and-rendering (GTK4Rs/AP-2).

## 3. Using environment variables to prevent GTK from crashing on a large XCompose file
**Scribobulate**: a dedicated startup workaround function, run **before** GTK init.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-3).

## 4. Using Pango `<a href>` markup in GtkLabel for standalone link widgets
**Symptom**: a link rendered via Pango `<a href>` in a `GtkLabel` was believed to style and activate but with no pointer cursor on hover and activation on button-*press* rather than *release*.
**Root cause**: *(as recorded — since disproved)*: `GtkLabel` was thought to handle `<a href>` without `GtkLinkButton`'s full interaction model. **At 4.6.9 it does both**: `gtk_label_update_cursor` sets `"pointer"` over an active link (`gtklabel.c:737`) and `gtk_label_click_gesture_released` emits `activate-link` on **release** (`:4400`). See #259.
**Resolution**: for a cell that IS a single link, use `GtkLinkButton` (`has_frame = false`) — now on its real merits (focusable, carries the URL as a property, frame-less button padding), not on an interaction deficit that does not exist.

## 5. Reading GtkTextBuffer text to track trailing newlines when child anchors are present
**Scribobulate**: an explicit trailing-newline counter maintained alongside rendering (with reset sites), never buffer-content inspection.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-5); #74 (the get_text-vs-get_slice offset-basis distinction).

## 6. Using a horizontal rule to indicate a blockquote
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 7. Placing the blockquote `DrawingArea` in an outer overlay outside the `ScrolledWindow`
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 8. Redirecting `XDG_CONFIG_HOME` without carrying `mimeapps.list`
**Scribobulate**: the `mimeapps.list` symlink created inside the startup XCompose workaround.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-8).

## 9. Duplicating action logic across context menu, main menu, and keyboard shortcut
**Symptom**: the main-menu Copy stayed enabled regardless of selection — the surfaces drifted out of sync.
**Scribobulate**: one `win.copy` `SimpleAction`; every surface (menu / toolbar / context menu / accelerator) binds to it by name — POLICY single-source-of-truth.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-9).

## 10. Walking the widget tree to re-discover anchor-embedded GtkLabel widgets
**Symptom**: a recursive `find_selectable_labels()` tree-walk to re-find anchored cell labels is fragile and breaks across re-renders.
**Scribobulate**: a qdata handoff from the render pass to the copy-action wiring step.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-10).

## 11. Expecting `g_menu_item_set_icon()` to render icons in a GTK4 menu bar
**Scribobulate**: text-only menu items; icons live on the toolbar buttons.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-11).

## 12. Using untyped GLib qdata as the canonical store for per-window state
**Scribobulate**: a typed per-window state registry (`thread_local`, keyed by window pointer), plus pure decision fns.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-12).

## 13. Restoring GtkTextView scroll via a GTK adjustment (on `changed`, or `set_value` after `set_buffer`) before the layout has validated
**Scribobulate**: never restore via the adjustment; the buffer-offset scroll helper uses a persistent left-gravity mark + `scroll_to_mark` on a coalesced idle. (Taxonomy: Resistant — stays prose.)
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-13 / §13, §14).

## 14. Restoring `GtkTextView` scroll via adjustment manipulation after `set_buffer`
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 15. Using `iter_at_location` to get the iter at the top of a `GtkTextView` viewport
**Scribobulate**: top iter from `visible_rect().y()` + `line_at_y`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-15, §15).

## 16. Mirroring split-pane scroll synchronously inside `value-changed`
**Scribobulate**: a coalesced once-per-frame scroll-sync projection (GtkSourceMap pattern), on `value-changed` AND `notify::upper`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-16, §16); findings: scroll-sync-validation-coalescing.md.

## 17. Parsing a "new instance" CLI switch after the GApplication has registered
**Scribobulate**: decide `NON_UNIQUE` (same app-id) **before** the GApplication registers, at the application's construction site.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-17).

## 18. Bridging glib↔Rust `log` with the wrong handler (stack overflow / dropped Gtk-CRITICAL)
**Scribobulate**: a single one-direction writer bridge (`forward` + `init`) wired first in `main()`; POLICY.md §Logging.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-18).

## 19. Trying to override a `GtkDropDown`'s empty-state "(None)" caption
**Scribobulate**: the heading control is a `GtkMenuButton` with a `(Hn)` caption, not a `GtkDropDown`.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-19).

## 20. Gating a widget-scoped action on raw per-widget focus
**Scribobulate**: a window-level `connect_focus_widget_notify` + `is_ancestor` *sticky* gate (transient surfaces — toolbar, menus, popovers, find bar — leave it untouched); `set_focus_on_click(false)` on the Format buttons, wired via a dedicated focus-gate setup function.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-20).

## 21. Indicating a text block (code block / blockquote) with the wrong GTK primitive instead of self-drawing it
**Scribobulate**: block chrome is self-drawn in the view's own draw/snapshot layer — code-block backgrounds on the BelowText layer (text inset past the rect by the code-block tag's margins), and the blockquote accent bar over the quote's buffer range. Blockquotes and code are buffer text, never widgets.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-21, §6/§21).

## 22. Forcing `GtkTextView` layout validation inside the snapshot/draw or size-allocate path
**Scribobulate**: the preview view measures block extents visible-only, clamped to the viewport, in the draw/snapshot path — never a full-content measure.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-22).

## 23. Embedding height-for-width block content as a widget at a `GtkTextChildAnchor`
**Scribobulate**: renders 1-D content as buffer text with self-drawn chrome. FOUR widget kinds anchor at a `GtkTextChildAnchor`, but only TWO carry the width-dependent height-for-width contract: tables (`ScribTableWidget`) and images (`GtkPicture`) are constant-size widgets whose width is cached and re-bounded from t…
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-23); findings: custom-anchored-table-widget-contract.md.

## 23a. Bounding an anchored child to the content column while it sits at an INDENTED margin
**Symptom**: on a table-heavy document a *marginal* horizontal overflow appears in the preview — `hadjustment.upper` exceeds `page_size` by a fixed handful of px (~27px in the field case), summoning the Automatic h-scrollbar whose appear/disappear re-arms the ScrAP-22/ScrAP-23 width↔height-for-width churn → the…
**Root cause**: ScrAP-23's width-bounding (`CodePreviewView::size_allocate`) bounds every anchored child to `content − 1`, where `content = width − view.left_margin − view.right_margin` — i.e. as if the child began at the view's content EDGE.
**Resolution**: give every anchored bounded child an **inset** = the horizontal margin its enclosing block steals, and bound it to `content − 1 − inset`. `Renderer::block_inset()` (`renderer/emit.rs`) computes it from the renderer's live list depth + blockquote state — list = left-only `depth*list_step`, blockquote…
**See**: extends #23 (this is the indent axis #23's "reserving cursor slack so they never overflow into a scrollbar" did not cover); gtk4-rs skill → textview-anchored-and-integration.

## 24. Relying on the system theme to paint a `GtkTreeExpander`'s disclosure chevron
**Scribobulate**: the outline view supplies its own chevron CSS.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-24).

## 25. Assuming the `.heading` / `.title-N` typographic CSS classes require libadwaita
**Scribobulate**: the "Outline" sidebar caption uses the `.heading` class with plain GTK CSS — no libadwaita dependency.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-25).

## 26. Pointing a `GtkPopover` at an anchor rect outside the visible viewport
**Scribobulate**: the caret-format overlay's positioning logic guards/clamps the anchor into the visible viewport before `set_pointing_to`.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-26).

## 27. Searching find-next from the caret after `select_range` (re-finds the current match)
**Symptom**: "Find next" sticks on the current match while "find previous" works — the asymmetry is the tell.
**Root cause**: `select_range(ins, bound)` parks the caret (`cursor-position`) at the match **start**; `sc.forward(caret)` returns the first match at-or-after the caret — the same match, forever. Backward happens to advance, so only *next* looks broken.
**Resolution**: step from the **far edge of the current selection in the direction of travel** — forward from `selection_bounds().end`, backward from `.start`; fall back to the caret only with no selection. Same rule for a plain `TextIter::forward_search` loop.

## 28. Tracking a selectable `GtkLabel`'s selection via a `notify::cursor-position`
**Scribobulate**: a copy-enabled recomputation reads buffer + the anchored-label handoff fresh, driven by the buffer `has-selection` and the **primary clipboard** `changed` signal (handler id tracked via qdata, disconnected before re-adding to avoid accumulation).
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-28); threading-async-and-memory.

## 29. Re-laying-out an anchored child via `queue_resize` after `parent_size_allocate` in the same `size_allocate`
**Scribobulate**: the view's `size_allocate` override derives the bound from the `width` argument and applies it to the anchored children's bound-width **before** chaining up, so children re-validate in the same pass.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-29).

## 30. Dismissing/unparenting a popover from inside a descendant's click handler
**Scribobulate**: the context-menu dismiss path defers the popdown to `glib::idle_add_local_once`, out of the event dispatch.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-30); findings: popover-teardown-in-handler.md.

## 31. Resolving an untrusted document's local image `src` against the CWD (or with a lexical-only containment check)
**Routed**: GEP-46 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: the contained-image resolver joins the document directory with the source path, then `dunce::canonicalize`s it (resolves `..` **and** symlinks), admitting the result only if it `starts_with` the canonicalized document directory — **component-wise `Path::starts_with`**, never a string prefix.

## 32. Anchoring a `GtkPicture` in a `GtkTextView` without a nonzero width request
**Scribobulate**: the image-rendering path sets a definite `set_size_request(seed_w, seed_h)`; the view's `size_allocate` re-clamps it to the live column on each real width change — `w = min(natural, content)`, `h = max_h·w/max_w` (aspect preserved, `max-width: 100%`, no upscaling past natural).
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-58); findings: researcher-findings-anchored-picture-blank.md.

## 33. Testing a rebuilt single-instance GApplication while a stale primary is still running
**Scribobulate**: launch with `--new-instance` / `-n` (ScrAP-17) when verifying a change interactively, or quit the running primary first; headless smoke tests already use `-n`.
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-43).

## 34. Remote image loading blocks the main thread; "refused" ≠ "unresolvable" in a multi-outcome resolution enum
**Scribobulate**: 34a accepted for the opt-in "Show Unsafe Images" path (the image-tag rendering site); 34b — the containment gate reports its *reason* (`Containment::Inside`/`Escapes`/`Absent`) through **one** routine both the image and link resolvers call, so `Refused` means a file that is really there and outside,…
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-44); app-lifecycle-and-env (GTK4Rs/AP-57 / GTK4Rs/AP-34b enum split).

## 36. Letting the editor `GtkSourceSearchContext` `notify::occurrences-count` overwrite the preview buffer's `forward_search` count in preview mode
**Symptom**: in preview mode the find count shows more matches than navigation can reach — the editor source context counts `| cell |` markdown the preview buffer can't navigate to (table cell text lives in `GtkLabel` child widgets, never in the buffer's btree).
**Root cause**: `set_search_text()` triggers `notify::occurrences-count` on the *editor* context, whose handler ran unconditionally and overwrote the correct preview (body-only) count with the inflated editor count.
**Scribobulate**: gate the handler with an early return when the active view mode is preview; reach cell text via a dedicated preview-hits builder.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-46, cell-highlight + two-step scroll); findings: researcher-findings-textview-search-anchored-cell-text.md.

## 37. `GtkTextView` never repaints an anchored child when a descendant's Pango background is REMOVED
**Scribobulate**: the preview highlight-apply path paints a **match-only** highlight (no base) and forces the anchored child to re-snapshot on every add/recolour/removal via a dedicated cell-repaint forcer — the #117 transient no-attr `<span>`-wrapper markup toggle.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-45); #117 (the forced-repaint primitive).

## 35. Reading `st.source` for a programmatic preview re-render in split mode
**Symptom**: in split mode a programmatic preview re-render (zoom/toggle/theme) uses stale content — just-typed editor text vanishes until a mode round-trip.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the split-mode preview re-render site.

## 38. Driving derived UI state from a delta-only signal, missing lifecycle boundary events
**Scribobulate**: the find bar re-runs its search-changed logic on reveal, the outline scroll-spy fires an explicit deferred initial scroll in every mode, and the preview find-highlight re-sync now runs at **every** preview-rebuild boundary — tab switch (`tabs/switch.rs`, pre-existing), theme sweep (`re_render_all_wi…
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-47); POLICY Document Rendering CAM row 8.

## 39. Specifying GNOME-specific icon names absent from non-GNOME themes
**Scribobulate**: the split-arrangement buttons and the view-command table use icon names confirmed present in `breeze-dark/actions/symbolic/` as well as Adwaita.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-48); inverse hazard (bundled name PRESENT in the host theme → theme overrides your bundle) — see #85.

## 40. `GtkAboutDialog` `authors` entries with `<url>` format open `mailto:`
**Scribobulate**: the About-dialog action — removed the `<url>` from `authors`; use `.website()` + `.website_label()` instead.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-50).

## 41. `FileChooserNative`/transient-dialog lifetime is backend- and widget-type-dependent — including WHEN you may tear one down
**Scribobulate**: split by type — `gtk::Window` dialogs keep a weak self-ref (toplevel list pins them) while any `NativeDialog` keeps ONE strong ref in an external `Rc<RefCell<Option<…>>>` holder (`saferizer::native_dialog::NativeDialogHolder`), dropped in `connect_response` after `.destroy()`; that ordering is also…
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-41, and GTK4Rs/AP-304 for the Quartz ownership defect that makes the visible-teardown path costly — unfixed upstream in every GTK 4.x, inherited b…

## 42. Predictable, reused path under the shared temp dir for a config-redirect workaround (security)
**Routed**: GEP-47 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: the temp-dir helper prefers `$XDG_RUNTIME_DIR` (0700) and makes a PID+timestamp dir with exclusive no-clobber semantics (`DirBuilder::mode(0o700).create`, fails on `AlreadyExists`).

## 43. Relying on `GtkNotebook`'s `create-window` signal for "drag a tab to the desktop to spawn a new window" on Wayland
**Scribobulate**: the custom tab widget that superseded `GtkNotebook` (#50) reimplements the drag-to-desktop path portably — a `GtkDragSource`/`GtkDropTarget` pair whose cancel handler treats `DragCancelReason::NoTarget` as "spawn a new window hosting this tab" (`window/tabs/dnd.rs`), plus a first-class portable **"M…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-49).

## 44. Using a `<Shift>` + digit/punctuation `GtkApplication` accelerator
**Scribobulate**: the view- and format-command tables drop `<Shift>` and use another modifier on the same physical key (`<Alt><Shift>2` → `<Alt>2`).
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-51).

## 45. A `GtkNotebook` with `show-tabs` false cannot be a cross-window tab-drag drop target
**Retired**: merged/superseded — see the entry named in the title's successor; number kept as a landing spot.

## 46. An idempotent signal-rewire check keyed only on widget identity misses a stale closure after a cross-window reparent
**Scribobulate**: the per-tab scroll-spy connection state adds a third field — the bound window pointer — alongside the `ScrolledWindow` + `SignalHandlerId`. Superseded for scroll-spy specifically by the ScrAP-57 dynamic host-window resolution pattern; the window-pointer field remains a harmless belt-and-suspenders f…
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-52); ScrAP-57 (the host_window resolution pattern this was migrated to).

## 47. `gtk_window_present()` from a D-Bus `open` handler doesn't raise+focus a tokenless (bare-terminal) launch — legitimate WM behavior, not a bug
**Scribobulate**: no code change — a bare shell launch carries no `DESKTOP_STARTUP_ID`, so user-time is 0 and WM focus-stealing-prevention correctly declines. TDD 8.2 tests the tokened path.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-54); #129 (the launch-path sibling — same literal `0`, opposite meaning, decided by which GDK path consumes it).

## 48. Adding an ancestor `GtkEventControllerKey` for Escape doesn't catch Escape while a `GtkSearchEntry` descendant has focus
**Scribobulate**: `GtkSearchEntry`'s own class keybinding (`Escape` → `stop-search`) consumes it first; the find-bar wiring shares one close closure across the button, the ancestor controller, and `connect_stop_search`.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-53).

## 49. `cargo valgrind` on a GTK4 app reports hundreds of toolkit-internal "leak"/uninitialised-value errors that are NOT application bugs — but valgrind still catches real app UAFs here, so triage by stack, don't dismiss wholesale
**Scribobulate**: for that ~700-error live-window run, grepping every stack for a `scribobulate::` frame found zero — GTK4's by-design OS-reclaims-at-exit retained memory absent a `gtk.supp`/`glib.supp` suppression file.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-59).

## 50. `GtkNotebook`'s native cross-window tab-detach DnD is unsafe on GTK 4.6.9 — a NULL deref inside GTK's own `dnd_finished_cb`, not (only) a freed source notebook
**Scribobulate**: stop using native detach — `set_group_name(None)` + `set_tab_detachable(false)`; reimplement cross-window move with a Shift-gated custom `GtkDragSource`/`GtkDropTarget`.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-60, incl. meta-lessons); findings: researcher-findings-notebook-dnd-null-detached-tab-CORRECTION.md.

## 51. A `GtkSourceSearchContext` `occurrences-count` handler that strong-captures its own context is a permanent self-reference leak
**Symptom**: no crash/warning — steady unbounded growth, one `GtkSourceSearchContext` (plus its `SearchSettings` and the buffer's tag table) leaked per closed tab.
**Root cause**: a signal connected ON the context, whose closure captures a strong `.clone()` of that same context — a GObject self-reference cycle (refcount-only, no cycle collector). The buffer↔context relationship itself is weak both ways in GtkSourceView source.
**Resolution**: read the emitter from the signal's own first parameter (`move |sc, _| …`), capturing nothing.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63, the general self-capture kernel); findings: researcher-findings-searchcontext-self-capture-signal-cycle.md.

## 52. Swapping in a brand-new `GtkScrolledWindow`/`GtkAdjustment` on external auto-reload without re-wiring the scroll-spy signal bound to the old one
**Scribobulate**: the external-reload path's Preview branch rebuilds a fresh `ScrolledWindow` (new adjustment) via the preview render step, orphaning the old listener; fixed by re-wiring the scroll-spy after refreshing the outline.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-55); controllers-and-bindings (GTK4Rs/AP-52).

## 53. Holding a `RefCell` `Ref` alive across a GTK setter that synchronously re-enters and borrows the same cell aborts the process
**Scribobulate**: the active-tab-changed handler set the find entry's text directly from a borrowed `RefCell` value — the temporary `Ref` lived across `set_text`, which synchronously emits `search_changed` → `borrow_mut()` on the same cell. Fix: clone out first, then `set_text`.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-61).

## 54. `write_atomic`'s crash-safe write-temp-then-rename makes GIO's `GFileMonitor` report every save as a deletion
**Scribobulate**: the atomic-write helper's `rename()` changes the inode; a `FileMonitorFlags::NONE` monitor reports a plain `Deleted`. Fixed with a dedicated self-delete guard (armed before the rename, consumed once on `Deleted`, cleared on `Changed`/`Created`), reset at monitor (re)creation.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-62).

## 55. Caching "which slot is active" as a raw `Vec` index across a drag-reorder that moves entries within the same `Vec`
**Scribobulate**: the custom tab-bar widget tracks the active slot as a `Cell<Option<usize>>` separately from the widget-identity-bound `.active` CSS class; the reorder handler snapshots the previously-active entry's OWN identity (a widget pointer) before mutating the order and re-derives the active slot from that id…
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-74).

## 56. Toggling a sibling widget's visibility from inside a container's own `size_allocate`
**Scribobulate**: chevrons are hidden by GEOMETRY (`size_allocate`d out of the clip region), never `:visible` or a degenerate size; the tab-view container's construction sets `stack.set_hhomogeneous(false)`/`vhomogeneous(false)` (bubbling `queue_resize`, clean pre-layout schedule) with `transition-type=NONE`, and the…
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-104).

## 57. A signal handler that captures its host `ApplicationWindow` by weak ref silently stops working after the widget subtree is re-homed to another window (split scroll-sync)
**Scribobulate**: the split scroll-sync, outline scroll-spy, and caret-format overlay handlers resolve the host window from the pane widget's live tree root AT EMISSION TIME through the shared `window::host_window()` seam (`tabs/lifecycle.rs`: `widget.root()?.dynamic_cast::<ApplicationWindow>()`), never a captured we…
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-52, dynamic `root()` resolution scope); related project entries #46, #52, #55.

## 58. Reparenting a reused `GtkSourceView` across view-mode containers re-fires its gutter's never-unbound `vadjustment` binding → a use-after-free
**Symptom**: switching view mode (Preview ↔ Edit ↔ Split) emits six `g_object_unref: assertion 'G_IS_OBJECT (object)' failed` per switch, only on the 2nd+ switch — a genuine read-after-free.
**Root cause**: the reused `GtkSourceView`'s gutter binds `view."vadjustment"` with `G_BINDING_SYNC_CREATE`, but `connect_view` never stores the returned `GBinding` (an upstream defect, unchanged in `main`), so it's never explicitly unbound. Rebuilding the mode container REPARENTS the reused view; every reparent re-runs `notify::vadjustment`, re-firing the binding against a tree mid-teardown.
**Resolution**: a custom `GtkWidget` container subclass mounts the editor's `GtkScrolledWindow` **once** and NEVER reassigns its child slot again; mode/orientation/order are pure layout parameters (`set_child_visible`, allocation order), never a `set_child` call.
**See**: gtk4-rs skill → state-and-subclassing (custom container that holds reused children as layout parameters). Findings: researcher-findings-gtksourceview-reparent-gutter-vadjustment-binding-unref.md.

## 59. Mounting a scrolling pane in a `GtkBox` without `vexpand` collapses it to its natural height (a lazily-validating `GtkTextView` then paints only ~2 lines)
**Scribobulate**: the split-view container's construction sets `hexpand`/`vexpand` on the persistent widget itself (expand flags don't transfer when consolidating several individually-expanding widgets into one container).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-105).

## 60. A closure owned by a widget's own machinery that strong-captures self (or an ancestor) is an uncollectable cycle — on window close it strands the entire descendant subtree
**Scribobulate**: the window-chrome build step's `content_paned.connect_map(move |_| …)` had captured a strong clone of the SAME paned; fixed by using the handler's own emitter argument (`move |paned| …`) instead. Two secondary self-cycles in the custom tab-bar/tab-view widgets fixed the same way (weak-captured).
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63).

## 61. Building N `GtkMenuButton` menu-models in a synchronous startup burst forces N×items accelerator-label font resolutions → a multi-second UI freeze
**Scribobulate**: exactly ONE shared caret-format overlay per window, re-parented onto the active tab's editor per switch — one heading-menu materialization ever, independent of tab count.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-106); app-lifecycle-and-env. Findings: researcher-findings-popover-set-parent-superlinear-startup-freeze.md.

## 62. A custom tab/stack widget leaves its active-index model unset for the default-visible first page
**Symptom**: moving/closing the FIRST tab of a window (never explicitly switched to) left the source window with a blank content pane and the moved tab's stale outline — a "phantom tab".
**Root cause**: the custom tab-bar/tab-view widget tracks the active tab in its own `Cell<Option<usize>>`, set only by its switch-to-index method — but a window's INITIAL page is shown by the `GtkStack`'s default (first child visible) and never travels through that path.
**Scribobulate**: a dedicated first-page-active marker, called once right after appending the first page, sets the active slot to `Some(0)` WITHOUT firing a switch callback.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-75); related #55 (GTK4Rs/AP-74), #56.

## 63. A shared app-level menubar model can't carry per-window content — a per-window submenu needs a self-built `GtkPopoverMenuBar` + selection-as-action-state + deferred `GMenu` mutation
**Scribobulate**: each `ApplicationWindow` self-builds its own `GtkPopoverMenuBar::from_model` (drops `app.set_menubar()`); "which tab is active" is a stateful radio `win.select-tab` action (a switch mutates NO menu content); `Documents` rebuilds are coalesced behind a dirty flag into a single `idle_add_local`.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-76); app-lifecycle-and-env (`set_menubar` D-Bus export, F10 self-registration). Findings: researcher-findings-per-window-menubar-documents-submenu.md.

## 64. A process-global CSS provider with an unscoped selector collides across windows (last-loaded wins)
**Scribobulate**: each window's rule is scoped to a per-window CSS class; a cross-window tab move ALSO re-renders the arriving tab's pixel geometry at the destination zoom (a dedicated tab-arrival wiring step) — the CSS half self-heals via tree-matching, the imperative half needed an explicit re-sync.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-77); app-lifecycle-and-env (CSS-provider display lifecycle).

## 65. Preserving a `GtkTextView` reading position across a re-render drifts, and can wedge input dead
**Scribobulate**: restore anchors to a buffer LINE via a persistent mark + deferred `scroll_to_mark`, followed (for the generic editor restore) by a non-animating `set_value` clamp; the preview view caches the reading line only while a `user_scrolling` flag is set, immune to mid-animation reads during rapid zoom.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-14; kin GTK4Rs/AP-115, GTK4Rs/AP-153).

## 66. Relying on pulldown-cmark's native superscript/subscript for tight `E=mc^2^` / `H~2~O`
**Symptom**: tight, Pandoc-style superscript/subscript (`E=mc^2^`, `H~2~O`) never rendered — the literal `^`/`~` showed instead; a multi-tilde line also lost its SECOND subscript once native superscript was disabled.
**Root cause**: pulldown-cmark recognises `^`/`~`/`~~` with CommonMark FLANKING-delimiter rules (like emphasis) — the inverse of Pandoc's TIGHT rule; and any enabled tilde feature FRAGMENTS a paragraph across multiple `Text` events at a stray unpaired marker, defeating a per-event scanner.
**Resolution**: the Markdown-options setup disables `ENABLE_SUPERSCRIPT`/`SUBSCRIPT`/`STRIKETHROUGH` at every parse site; a dedicated scan step tokenises `^x^`/`~x~`/`~~x~~` ourselves with tight Pandoc semantics on clean, unfragmented text.
**See**: pulldown-cmark 0.13 `Options`; CommonMark §6.2 flanking rules; Pandoc superscript/subscript extension. Empirically bisected.

## 67. Screenshotting an open GTK4 `GtkPopoverMenu` under kwin/X11 to verify a menu
**Scribobulate**: verify the menu's EFFECT on a capturable surface (the main window, or the action/persisted state) instead of trying to screenshot the popover; also banked that `xdotool windowmove` sets the frame origin, so client-area clicks need the decoration-height offset added.
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-67, the D-Bus GAction-state screenshot alternative). Observed live on GTK 4.6, kwin_x11.

## 68. Deferring a `set_visible(false)`→`measure()` read as if it were the GtkTextView lazy-validation family
**Scribobulate**: the toolbar min-width update takes no deferral at all — a plain `queue_resize` is enough.
**See**: gtk4-rs skill → textview-layout-and-drawing (**GTK4Rs/T-2**, the hide→measure synchronous-cache-invalidation contrast; `T-n` is the skill's technique register, numbered and frozen like its `AP-n`).

## 69. Putting mnemonic `_` markers in a command label shared across menu + tooltip + context-menu surfaces
**Scribobulate**: mnemonics injected ONLY at menu-build time (a dedicated mnemonics table + helper function), reused by the context menus; the shared command label stays literal so toolbar tooltips are unaffected. Dedicated well-formedness/uniqueness guard tests catch drift.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-68). Findings: researcher-findings-popovermenubar-mnemonics.md.

## 70. Getting bare-letter access keys (with a visible underline) in a plain `GtkPopover` via mnemonics / use-underline
**Scribobulate**: a dedicated access-markup/access-shortcut helper builds a `GtkShortcutController` (Capture/Local phase) with one `KeyvalTrigger(keyval, NO modifiers)` per row, gated on `is_sensitive()`; the underline is drawn manually with Pango `<u>` markup.
**See**: gtk4-rs skill → controllers-and-bindings (**GTK4Rs/T-1**, bare-letter access keys via ShortcutController + KeyvalTrigger). Findings: researcher-findings-plain-popover-access-keys.md.

## 71. Nesting a submenu as a child `GtkPopover` inside a plain autohide `GtkPopover` context menu
**Scribobulate**: the context-menu implementation uses a single-surface `GtkStack` (`main`/submenu pages, `SlideLeftRight`) mirroring what `GtkPopoverMenu` itself does for submenus, omitting its spurious-scrollbar-causing `ScrolledWindow` wrap; access keys are page-gated (#70's controller, same physical key means dif…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-69). Findings: researcher-findings-plain-popover-nested-submenu.md.

## 72. Gating/targeting a multi-pane action on the first-found view instead of the focused pane
**Scribobulate**: a dedicated focused-text-view resolver tracks a sticky focused pane (updated by `focus-widget-notify`, ignoring transient popovers/find-bar exactly like ScrAP-20), falling back to the single view otherwise.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-70; kin GTK4Rs/AP-20).

## 73. Reconstructing character-precise copied Markdown from sparse parser waypoints, and mis-reading pulldown-cmark offset semantics
**Symptom**: copying a *partial* preview selection returned the WHOLE enclosing block's Markdown source (four letters of a heading → the entire `# Heading` line).
**Root cause**: the sparse waypoint map records source offsets only at pulldown event boundaries and snaps outward — block-granular by construction. A block's Start/End range includes the TRAILING newline, an escaped char's `Text` token DROPS the backslash, and an entity tokenises apart from its rendered char.
**Resolution**: the copy-map builder constructs a buffer-annotated construct TREE in the same render pass that fills the buffer, reconstructing delimiters from source only when a selection crosses a construct's content boundary; leaf runs interpolate char-precisely.
**Lesson**: to reconstruct *balanced* source from a rendered selection you need a construct tree annotated with real render offsets, not a flat source-offset map.

## 74. Aligning char offsets with `GtkTextBuffer::get_text()` — it omits anchored children
**Symptom**: a debug assertion (and any offset-indexed logic) drifted between the buffer's character offsets and a `buf.text()` string — by one char per anchored child.
**Root cause**: `gtk_text_buffer_get_text()`/`get_iter_text()` silently OMIT anchored children, but `char_count()`, `iter.offset()`, `slice()`/`get_slice()`, and `selection_bounds()` all count each as one `U+FFFC`. A `get_text`-derived char array and any iter/`char_count`-derived offset diverge silently, only on documents that HAVE anchors.
**Scribobulate**: the copymap drift guard and the copy path both use `buf.slice()`, never `text()`, when correlating with iter/char_count offsets.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-5).

## 75. A hard tab in a GFM table breaks table recognition; normalise tabs — but length-preservingly
**Symptom**: a table pasted from a spreadsheet, cells separated by hard TABS, rendered as a literal paragraph (`---` even turned into an em-dash via smart punctuation) — the byte-identical table with spaces parsed fine.
**Root cause**: a GFM delimiter row's grammar admits only `-`, `:`, `|`, spaces — a tab is a CommonMark/GFM-conformant rejection, not a pulldown bug.
**Resolution**: a dedicated tab-normalization step replaces a hard tab with ONE space — LENGTH- and POSITION-preserving (so copymap/scroll-sync byte offsets never drift) — exempting leading indentation and verbatim code regions (found via a structural pre-parse).
**See**: pulldown-cmark 0.13 `ENABLE_TABLES`; GFM spec §Tables; CommonMark §2.2/§6.2.

## 76. A paragraph-attribute `GtkTextTag` applied as one continuous range over a multi-paragraph region drops the attribute on toggle-free middle lines
**Scribobulate**: a dedicated per-line tag-application step tags each logical line's CONTENT ONLY, leaving every terminating `\n` untagged — the untagged gaps prevent coalescing, so every line gets its own toggle.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-72). Findings: researcher-findings-textview-blockquote-left-margin-multipara.md.

## 77. UI-testing a formatter over the selectable read-only Preview pane
**Symptom**: a formatter click over a visibly-selected Preview pane silently no-opped for every command — a selection in a selectable READ-ONLY view looks identical to an editor selection, but the format action is correctly disabled there.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: `src/window/editbar/focusgate.rs` — the window focus-widget gate (`connect_focus_widget_notify` + `is_ancestor`) that keys `win.format` on editor focus, not on selection presence; a read-only-preview…

## 78. `Options::all()` (or any enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of degrading to literal text
**Symptom**: math (`$E=mc^2$`) rendered as nothing, footnote refs (`[^1]`) vanished, and YAML/`+++` frontmatter leaked into the body as a stray paragraph — silent content loss, no warning.
**Root cause**: `Options::all()` turns on EVERY pulldown-cmark extension, including ones the renderer has no handler for; the dispatcher's catch-all silently drops standalone events and leaks a container's inner `Text`.
**Resolution**: the Markdown-options setup is an explicit ALLOWLIST of only the extensions actually handled (`TABLES | TASKLISTS | SMART_PUNCTUATION | HEADING_ATTRIBUTES | GFM`); anything else degrades to literal `Text` rather than vanishing.

## 79. A container-level `GtkGestureClick` also fires on presses that land on a child `GtkButton` — a bar-wide "activate" gesture activates a tab even when the press was on its × close button
**Scribobulate**: a dedicated close-button hit-test resolves the real target with `WidgetExt::pick()` and bails early when it (or an ancestor) carries the close-button CSS class.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-109).

## 80. Tracking a "reading line" only from a wheel `EventControllerScroll` misses scrollbar-drag and keyboard scrolling — the re-anchor goes stale
**Scribobulate**: a dedicated scroll-position-tracking wiring step also hooks a `GtkGestureClick` on the scrollbar and a scroll-key `GtkEventControllerKey`; every programmatic scroll resets the `user_scrolling` flag to false at its start, so a rapid-zoom burst's own animation frames stay excluded (burst-safe).
**See**: gtk4-rs skill → controllers-and-bindings / textview-scrolling-and-adjustments (input-source wiring companion, sibling of the GTK4Rs/AP-14 family / #65).

## 81. Persisting "all windows" from each window's `close-request` + a sequential-close quit loses every window but the last
**Scribobulate**: a dedicated quit-all-windows routine snapshots the full window set ONCE up front, freezes session-save via a `thread_local` latch for the close sequence, and thaws it on a cancelled prompt / backed-out Save-As.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-113).

## 82. A one-shot `scroll_to_mark` restoring a FAR reading position onto a freshly-rebuilt GtkTextView lands near the top
**Scribobulate**: a dedicated fresh-view scroll-restore routine drives a PROGRESSIVE non-animating `set_value(line_yrange(mark).y)` off `notify::upper` until `line_at_y` converges; the one-shot `scroll_to_mark` is reserved for warm views (outline-nav, zoom, small docs).
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-115). Findings: researcher-findings-textview-far-scroll-fresh-unvalidated.md.

## 83. `GtkShortcutsWindow`'s programmatic `add_section`/`add_group`/`add_shortcut` API is GTK 4.14+ — on 4.6 you must build it from Builder XML
**Scribobulate**: a dedicated shortcuts-window builder generates a `GtkBuilder` interface XML from the command tables (the stable `Buildable` path since GTK 4.0) and fetches the object; `set_help_overlay` (`GtkApplicationWindowExt`, present on 4.6) wires it.
**See**: gtk4-rs skill → versioning-and-features (GTK4Rs/AP-114).

## 84. `GtkTreeListModel` `autoexpand=true` makes true recursive Collapse all impossible; build `autoexpand=false` + explicit expand pass. Collapse DESTROYS the subtree (it does not cache expanded flags)
**Scribobulate**: model built `autoexpand=false` + an explicit forward-walk expand-all pass at build time for the default-open TOC; Collapse-all need only collapse the depth-0 roots (destroying each wipes everything below).
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-111).

## 85. A bundled (gresource) `*-symbolic` icon is only a fallback — a host theme that ships the same name overrides it
**Scribobulate**: the outline Expand/Collapse buttons keep standard icon names (`expand-all-symbolic`) so KDE/breeze supplies its native chevrons; the bundled SVGs are the Adwaita/headless fallback only.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-102).

## 86. Probing a broader Markdown marker before a narrower one that embeds it mis-parses the input — test narrowest-first
**Symptom**: a GFM task item (`- [ ] foo`) auto-continued on Enter as if a plain bullet, leaving the checkbox dangling — everything looked right for plain bullets/numbered lists, so the bug only showed on the newest marker type.
**Root cause**: a task marker (`- [ ] `) is a bullet marker plus more; in an `if let … else if let …` chain, the FIRST-tested broader parser (bullet) matched the shared prefix and short-circuited before the narrower task parser ever ran.
**Resolution**: order the parser chain NARROWEST-first — the task-marker parser before the bullet-marker parser before the ordered-marker parser; give the narrow parser its own detector for the discriminating part (the checkbox).

## 87. A `#[gtk::test]` that maps + pumps to full allocation BEFORE calling the code under test validates line heights first, masking an unvalidated-heights bug
**Scribobulate**: scope each automated test to what it CAN decide deterministically (mark placement, no-panic, no re-arming ScrAP-22); mark the true regression guard as a load-bearing MANUAL integration test; always mutation-test a regression guard (flip the fix, confirm the test fails) before trusting it.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-78).

## 88. Bounding a blocking `MainContext::iteration(true)` pump loop with a between-iterations wall-clock check instead of a timeout SOURCE — it can hang forever on an idle display
**Scribobulate**: install a real `glib::timeout_add_local_once` SOURCE before the loop as the watchdog — a ready timeout is dispatchable work, so `iteration(true)` is GUARANTEED to return by the deadline; remove the source on a converged (normal) exit.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-79).

## 89. Gating a programmatic `GtkSingleSelection` change with a transient "we're setting it" bool — it re-emits `selected-item` after the setter returns, escaping the bool
**Scribobulate**: TWO complementary per-tab guards, because the bool alone can't cover the async echoes — `outline_spy_selecting` (a transient `Cell<bool>`, `winstate/tab.rs`) catches the synchronous `notify::selected-item` inside `set_selected`, and `outline_spy_doc: Cell<Option<usize>>` (the doc **index** the spy c…
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-112).

## 90. A `GtkPopover` attached with `set_parent()` is NOT auto-unparented — the parent widget's `dispose()` must unparent it, or teardown floods "GtkPopover is not a child of …"
**Scribobulate**: THREE mechanisms, now with a unified handle for the hazardous case. (1) The view-parented persistent popovers (codeview marker popover + selection overlay, window format overlay) are owned by `saferizer::PersistentPopover` (Wave 7): its `teardown()` runs `popdown()`→`unparent()` in the one safe orde…
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-80).

## 91. An always-on scrollbar with default (overlay) scrolling floats over the `GtkTextView`'s right margin, stealing clicks meant for margin-drawn affordances
**Scribobulate**: the preview-rendering setup builds the preview scroller with `.overlay_scrolling(false)`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-81).

## 92. A mutation path that edits the buffer but leans on a MODE-GATED live-preview refresh leaves the preview stale
**Symptom**: creating/editing/removing an annotation in preview-only mode wrote correct source but left the preview's highlights/markers/popover text stale until a manual reload.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the mode-agnostic annotation re-render site.

## 93. Anchoring positions by pulldown-cmark source offset against ALL events maps onto a block-structure event whose range spans the whole block
**Symptom**: a CriticMarkup comment marker/highlight placed by mapping a cleaned-source offset to a buffer position landed on the BLANK-LINE separator above its paragraph instead of the paragraph itself.
**Root cause**: pulldown's offset iterator reports the source range of the ENTIRE BLOCK for a block Start/End event, even though this renderer emits only the block separator there — an all-events offset lookup resolves a paragraph interior onto that misleading range.
**Resolution**: the preview-build offset-anchoring map is restricted to CONTENT events only (`Text`/`Code`/`Break`), excluding `Start`/`End` block-structure events.

## 94. A signal handler connected to a `GtkTextView`'s BUFFER is silently dropped when `set_buffer` swaps the buffer — re-wire buffer-dependent handlers on the new buffer
**Scribobulate**: the annotation-overlay wiring re-invokes its selection-connect closure from `view.connect_notify_local("buffer", …)` — a VIEW-level hook that survives every swap.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-82, GtkTextView buffer-signal lifecycle).

## 95. A shown `GtkPopover` does not grow its surface when its child grows — pre-size it (homogeneous `GtkStack`), don't re-present it
**Scribobulate**: the live instance is the two-page context-menu popover (`window/contextmenu.rs`) — a single `GtkStack` built once with both pages present, relying on `GtkStack`'s *implicit* `hhomogeneous=true` default (it only overrides `vhomogeneous`/`interpolate-size`), so the popover pops up at the widest page's…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-86).

## 96. Committing an action that rebuilds the widget subtree synchronously inside a `GtkButton` `clicked` handler breaks active-state accounting
**Scribobulate**: the annotation commit paths defer the rebuild with `glib::idle_add_local_once` so the gesture unwinds first.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-30 — rebuilding a widget inside its own event emission; defer with `idle_add_local_once`).

## 97. Inferring "inline vs block" from non-empty source delimiter bytes engulfs whole paragraphs
**Symptom**: annotating a single plain word in a paragraph highlighted the ENTIRE paragraph.
**Root cause**: `wrap_span` inferred "inline" from whether a node's source open/close delimiter byte-ranges were non-empty — a paragraph ALSO has non-empty trailing "close" bytes, so it was mis-flagged inline and taken whole.
**Resolution**: the copy-map's branch-node representation carries an explicit kind set from the CONSTRUCT KIND at build time, never inferred from byte shape (originally an `inline: bool`; now the `BranchKind` enum — see ScrAP-255 for why the boolean pair became one enum).

## 98. A `GtkPopover` hosting a typing entry is unwinnable on X11 (autohide steals focus via its seat grab; non-autohide can drop clicks) — host a typing entry as an in-surface `GtkOverlay` child instead
**Scribobulate**: the preview-rendering setup wraps the preview `ScrolledWindow` in a `GtkOverlay`; the comment entry lives there (a dedicated in-surface overlay component); the Annotate action button stays a non-autohide popover.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-83).

## 99. A translucent text-tag highlight is painted over by a later opaque-background tag — GTK text-tag backgrounds don't composite; the highest-priority tag wins
**Scribobulate**: the tag-table setup raises the annotation-highlight tag to `table.size()-1` (top priority) after all other tags are added.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-84).

## 100. Measuring a widget while it is `visible=false` returns 0 — center an overlay child off a hidden measure and it collapses to a left-edge anchor
**Scribobulate**: the annotation-overlay wiring shows the entry card BEFORE positioning it (measurement needs VISIBILITY, not allocation).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-85).

## 101. UI-test tooling: kwin-on-Xvfb won't deliver a synthetic `xdotool` click to a non-autohide `GtkPopover` surface — verify such flows via a keyboard-triggerable action, not a synthetic popover click
**Scribobulate**: the `win.annotate` GAction is the keyboard-triggerable path used to test entry-card positioning headlessly (also how ScrAP-102 was found and verified).
**See**: gtk4-rs skill → ui-testing-interaction (GTK4Rs/AP-175); gtk4-rs skill → automated-UI-testing (ui-testing-interaction module).

## 102. Positioning a widget via `set_margin_*` then re-measuring it double-counts the margin — GTK folds a widget's own margins into `preferred_size()`/`measure()`
**Scribobulate**: the card-positioning routine zeroes `margin_start`/`margin_top` BEFORE measuring, then applies the freshly-computed margins.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-87).

## 103. Refreshing a `GtkTextView` via `set_buffer` for a change that leaves the rendered text identical repaints the whole document and jumps the scroll
**Scribobulate**: a dedicated in-place annotation-refresh path re-tags + re-markers the LIVE buffer in place (no `set_buffer`) whenever the freshly-parsed text is structurally identical to what's on screen, falling back to a full re-render only if it isn't.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-90).

## 104. A persisted `GtkTextMark` re-resolved after a `set_buffer` swap is a cross-buffer footgun that aborts with `gtk_text_btree_line_number couldn't find line`
**Scribobulate**: every persisted-mark resolution site guards `mark.buffer().as_ref() == Some(&view.buffer())` before resolving; mutation-tested (removing the guard reproduces the exact crash).
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-89).

## 105. `iter_location` (any line-DISPLAY-caching geometry read) right after a `set_buffer` swap, before re-allocation, aborts with `gtk_text_btree_line_number couldn't find line`
**Scribobulate**: root fix — the reload-from-disk path sets a loading guard flag around the editor-load step; defense in depth — the scroll-sync and preview draw/snapshot paths read the cache-free `line_yrange` instead of `iter_location` on a possibly-just-swapped view.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-89; kin GTK4Rs/AP-258).

## 106. A selectable `GtkLabel` in a popover auto-selects all its text on open — the popover focuses it, and a selectable label selects-all on focus-in
**Scribobulate**: the marker-popover builder drops `set_selectable(true)` on the comment label.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-107).

## 107. A menu-activated action that synchronously raises a focus-grabbing in-surface widget has its focus stolen by the menu popover's pop-down focus-restore — defer the raise to idle
**Scribobulate**: the Annotate action's registration defers the raise via `glib::idle_add_local_once` so the pop-down + focus-restore settle first.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-116).

## 108. `GtkTextBuffer::redo()`/`undo()` leaves no undo barrier — the next edit merges into the redone action's group, so one later Undo reverts two edits
**Scribobulate**: **originally this entry claimed the flush happened "before each discrete edit" — it did not; it was at ONE of the three routines** (the annotation splice), while every format command (`editbar/edit.rs`) and smart-newline continuation (`editbar/newline.rs`) lacked it, so the double-revert was live on…
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-110, the GtkTextBuffer/GtkTextHistory undo-barrier lesson).

## 109. Mapping GtkTextView buffer coords ↔ an anchored-child cell's interior under incremental allocation
**Scribobulate**: a dedicated cell-row-geometry routine computes table-top buffer-Y from `line_yrange(iter_at_child_anchor(table_anchor))` (cache-free) PLUS `translate_coordinates(cell → table widget)` (a local, placeholder-immune subtree transform) — recomputed every frame in the draw/snapshot layer, no cache.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-91).

## 110. Driving selection-dependent UI for a selectable-`GtkLabel` cell (a selection island) — buffer signals never fire; use the primary clipboard, wired on the live view
**Scribobulate**: the annotation-overlay wiring connects `view.primary_clipboard().connect_changed` PER-RENDER (guaranteed-live view), disconnected in `dispose`; cell selections clear on a genuine buffer-cursor placement (otherwise sticky).
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-28).

## 111. The in-place buffer-tag refresh can't repaint an anchored-child cell decoration — reconcile the cell labels in place, unconditionally
**Symptom**: creating a cell annotation didn't show its amber highlight; removing the last cell annotation didn't clear it — both fixed only by an unrelated full re-render.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the in-place annotation-refresh site (the anchored-child cell-label reconciliation).

## 112. `GDK_IS_SURFACE` criticals are a stale TOOLTIP timer over an unrealized grabbing popover — reuse popovers, don't destroy per use
**Scribobulate**: the interactive/grabbing popovers OVER THE `GtkTextView` — the codeview marker popover and the selection-action/format overlay — are REUSED (created + `set_parent`'d once, only `popup()`/`popdown()`, content rebuilt per use), now behind `saferizer::PersistentPopover`; these are the surfaces where th…
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-117). Findings: researcher-findings-popover-tooltip-surface-assertion.md.

## 113. The first popup of a view-parented popover forces a one-shot table revalidation that scrolls the view and drops the click — pre-warm it
**Scribobulate**: a dedicated popover pre-warm routine pre-warms the persistent popover once at first `map`, at scroll 0 (absorbs first-validation churn); the marker-popover open routine holds the saved vadj value across the popup's settle via a `value-changed` re-pin guard, wall-clock-bounded (`REPIN_GUARD_US`, 1.5…
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-118).

## 114. An in-place live-buffer edit that skips the canonical source-of-truth vanishes on the next fresh render
**Symptom**: an annotation created in preview-only mode vanished on a mode switch, then reappeared on the next toggle.
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: project-specific; the fix + rationale live in a code comment at the mode-switch source-flush site.

## 115. Highlighting a char range in an existing Pango-markup string via `find` wraps the wrong (first) occurrence
**Symptom**: annotating a word inside a formatted table cell highlighted a DIFFERENT (the FIRST) occurrence of the same word elsewhere in the cell.
**Root cause**: the highlight was injected via `result.find(escaped_slice)` — a TEXT search returning the first occurrence, not the annotated char position; a range crossing an inline-format boundary also isn't one contiguous substring.
**Resolution**: a dedicated char-range markup-wrapping routine walks the markup tracking the PLAIN-char index (tags=0, entities=1) and opens/closes the span POSITIONALLY, closing before and reopening after every existing tag to preserve well-nesting.

## 116. Activating a nested-submenu item in a `GtkPopoverMenuBar` leaves a sibling top-level menu popped open — the bar clears its open menu through only one channel (a top-level popover's `unmap`)
**Scribobulate**: `window::actions::dismiss_stray_menubar_popovers` `popdown()`s any still-mapped top-level popover on idle after a nested-submenu action (public-API only — safe against the ScrAP-63 UAF).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-108). Findings: researcher-findings-popovermenubar-submenu-stays-open.md.

## 117. Clearing a `GtkLabel` `set_attributes` overlay in place needs a transient markup-STRING change to repaint — a same-string `set_markup` is a no-op
**Scribobulate**: the preview-highlight clear routine does `set_attributes(None)` then a transient no-attribute `<span>` wrapper `set_markup` followed by reverting to the clean markup — two genuine string changes, zero visual difference.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-92's toggle technique, companion).

## 118. A list-item hanging indent (`left-margin` + negative first-line `indent`) is unreliable across paragraphs; the durable fix is to DROP the hanging indent (draw the marker in a gutter, uniform margin)
**Scribobulate**: DROP the hanging indent entirely — draw the marker in a left GUTTER (the draw/snapshot layer, out of the buffer) with a uniform per-level `left-margin` and `indent=0`, applied per logical line.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-95; kin GTK4Rs/AP-72).

## 119. A `GtkPaned` with the default narrow handle silently swallows presses in a strip at a child pane's edge
**Scribobulate**: `paned.set_wide_handle(true)` (inset 0, hit-area = handle widget only); probe CAPTURE delivery per ancestor level to diagnose an edge dead-zone, never assume coordinates.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-93).

## 120. `WidgetExt::color()` is gated behind the gtk-rs `v4_10` feature — the compile error never mentions it
**Scribobulate**: use `widget.style_context().color()` (GTK 4.0; deprecated only at `v4_10`, so no warning below 4.10) on 4.6–4.12 targets.
**See**: gtk4-rs skill → versioning-and-features (GTK4Rs/AP-94).

## 121. Two `GtkTextTag`s that both set `left-margin` on a line (a list item inside a blockquote) do not compose
**Scribobulate**: make the per-depth list tag ACCUMULATIVE with an indent RELATIVE to its container (adds onto the view default OR the blockquote's margin); enforce exactly one depth tag per line (an inner tag's `TagEnd` fires before its outer, so the deepest lands first — avoids double-stacking a nested list's own m…
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-96).

## 122. Translating a stripped-then-parsed document's ranges back to original coordinates instead of per-position translation silently swallows the stripped bytes (the range-merge gotcha)
**Symptom**: an inline annotation adjacent to other CriticMarkup in the same paragraph (`the earth is {==flat==}{>>cite?<<} ok`) produced downstream text that silently OMITTED the stripped delimiter bytes whenever a translated RANGE was used.
**Root cause**: with delimiters stripped from the parsed ("cleaned") text before parsing, pulldown-cmark emits the surrounding prose as ONE `Text` event whose CLEANED range maps to a NON-CONTIGUOUS original region (the deleted bytes sit in the middle); a range has two endpoints and cannot express that hole, so translating it once (range-in, range-out) silently drops the gap.
**Resolution**: keep render maps in CLEANED coordinates and translate PER-POSITION at the point of use (a dedicated cleaned-to-original translation step), never as a range; identity-translate when the shift table is empty so unannotated documents keep byte-identical behaviour.
**See**: the CriticMarkup cleaned↔original shift-table mapping (verified against pulldown-cmark 0.13.4).

## 123. A coverage ratchet's floor recorded as stale prose drifts from the real (climbing) figure, silently loosening the gate
**Symptom**: POLICY's coverage prose cited "~66.48% lines" while the real figure had climbed to 67.83% over several unrelated cycles — nothing alerted, since a ratchet only fires on a DROP.
**Root cause**: the figure lived in three places (the coverage-gate script's floor constant, POLICY's inline snippet, prose) with no single source of truth; a contributing misread nearly set the floor from `cargo-llvm-cov`'s eye-catching FIRST (Regions) column instead of the gated LINES column.
**Resolution**: ratchet the floor and the stated figure together whenever coverage rises; verify a ratchet change by the gate's EXIT CODE, never the printed percentage.

## 124. A test suite gated behind a Cargo feature is invisible to every gate that does not enable it — it rots until it stops compiling, and every line it covers reads as 0%
**Scribobulate**: put the feature in the pipeline — `clippy --features gtk-integration-tests` and a dedicated `xvfb-run -a cargo test --features gtk-integration-tests` step (a display is not an excuse).
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-98, the core "green ≠ covered for a gated suite" lesson).

## 125. Scheduling work that depends on paint-populated state via `idle_add_local_once` reads the previous frame's state and silently no-ops
**Scribobulate**: never bound this on a FRAME COUNT — the thing being waited on is wall-clock (GTK's scroll animation is a fixed 200 ms, `ANIMATION_DURATION` gtkscrolledwindow.c:196), while a tick fires per frame at a refresh rate the app does not control; 45 ticks is 750 ms at 60 Hz but 187 ms at 240 Hz — *shorter t…
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-97).

## 126. Styling a `GtkTextView`'s background via `textview { background-color }` alone works on Default but is defeated by the user's system theme
**Scribobulate**: the preview CSS theming step styles BOTH nodes — `textview { color; font-family }` for the widget node (also read by GTK's own caret/color paths) + `textview > text { background-color }` for the fill.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-100).

## 127. Reaching for CSS selector specificity to arbitrate between two `GtkCssProvider`s is a category error
**Scribobulate**: never let two providers own the same property — give each provider a DISJOINT set of properties so they compose without arbitration (in Scribobulate the zoom provider owns CSS `font-size` exclusively, while the theme owns Pango *scale* — a tag attribute GTK multiplies onto the CSS base, a different…
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-101).

## 128. `g_get_user_config_dir()`'s process-global lazy cache makes a mid-startup `XDG_CONFIG_HOME` redirect and an honest config-dir read mutually exclusive
**Scribobulate**: snapshot CONFIG from `std::env` BEFORE the redirect and never call `glib::user_config_dir()` anywhere in the app or its dependencies; mitigate each known reader individually — symlink `mimeapps.list` (deriving the desktop-specific filename set from live `XDG_CURRENT_DESKTOP`, not hardcoding) and sym…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-173; kin GTK4Rs/AP-3).

## 129. `g_app_info_launch_default_for_uri(uri, NULL, …)` silently emits no activation token, so a WM's focus-stealing prevention refuses to raise the handler
**Scribobulate**: `gtk_show_uri_full(None, uri, 0, None, cb)` builds the launch context automatically — `None` as parent is DELIBERATE (a parent buys no extra token, only a `PARENT_WINDOW_ID` at the cost of an `gtk_window_export_handle` unexport warning on EVERY call on 4.6 X11, fixed upstream in 4.8 but never backpo…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-99).

## 130. A hand-authored SVG that renders fine in Inkscape can be invalid XML that librsvg (and GTK) rejects outright
**Symptom**: `sdd/system-overview.svg` rendered perfectly in Inkscape but the app itself showed a broken-image placeholder for its own architecture diagram.
**Root cause**: three `<text>` elements carried a DUPLICATE `class` attribute — a fatal XML well-formedness error; Inkscape's libxml2 recovery mode silently keeps the first occurrence and continues, while librsvg parses strictly and fails the WHOLE document with no partial render.
**Resolution**: `xmllint --noout file.svg` is the gate for any hand-authored/generated SVG, run BEFORE ever trusting a render; confirm in the actual (strict) consumer, never the lenient authoring tool.

## 131. A refactor that REDEFINES what an existing field means keeps compiling at every call site, and silently changes behaviour
**Routed**: GEP-40 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: the preview palette no longer carries a page-lightness field; a comment records why, and anything outside the preview probes the desktop's lightness through a dedicated helper. TDD 18.7.

## 132. A guard test whose INPUT SET is not the thing it polices — a wrong scope filter, or a hand-maintained mirror — passes forever
**Routed**: GEP-1 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `app::mnemonics::menu_access_keys_unique_per_popover` now DERIVES its popover grouping from `app::menubar::build_top_level_menus` — the same models `build_menubar` ships — instead of mirroring them, and pins its own non-vacuity plus the dynamic-popover exemptions; deriving found the collision on its…

## 133. A hard-coded Xvfb display lets one crashed run orphan a server that silently serves stale windows to every run after it
**Scribobulate**: the capture stage of `scripts/gen-splash.sh` (display allocation + `SPAWNED_PIDS` / `trap … EXIT`).
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-131).

## 134. Bounding a wait on a FRAME COUNT when the thing waited on is measured in WALL-CLOCK
**Scribobulate**: `Deadline` + `NAV_BUDGET_US` / `REPIN_GUARD_US` in `src/codeview/markers.rs`. The two defective sites were a `MAX_FRAMES = 45` ("~0.75s at 60Hz") navigation poll and a `ticks >= 48` ("~0.8s @60fps") re-pin guard.
**See**: gtk4-rs skill → deferred-work-and-ordering (GTK4Rs/AP-122).

## 135. `GtkText` writes PRIMARY on every selection change — and a widget claiming PRIMARY CLEARS the previous owner's selection
**Scribobulate**: `preview/annotate/overlay.rs`, inside `schedule`'s debounce timer — the code's own comment already *claimed* the contract ("the selection it was anchored to changed") but never **checked** it; it inferred it from "a signal fired".
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-120).

## 136. Seeding live UI state from the persisted-session snapshot
**Routed**: GEP-41 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `session::LiveChrome` + `update_live_chrome` (a `thread_local`; GTK is single-threaded), read by `window/mod.rs`'s `build_window` — **since retired.** The live app-wide cache was a correct fix to the *read* staleness and a wrong answer to the underlying question: the state was never app-wide.

## 137. A window `GAction` accelerator BEATS a focused `GtkText`'s own keybinding — and *disabling* the action is what hands the key back
**Scribobulate**: `focus_in_text_entry` (`window/actions.rs`) gating `win.select-all` (`window/editoractions.rs`). It began life as `focus_in_annotation_card`, a CSS-class-ancestor check scoped to one card, and had to be widened to a type check once the mechanism was understood — the find and replace entries had the…
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-121).

## 138. Polling a `GtkEntry`'s own `has_focus()` in a test spins forever — focus lands on its internal `GtkText` delegate
**Scribobulate**: the readiness probes in `window/editoractions.rs`'s select-all standdown rubric, and `focus_in_text_entry` (`window/actions.rs`), which is keyed on `gtk::Text` for exactly this reason.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-119).

## 139. A `GtkText`/`GtkEntry` selects ALL its text on focus-in, silently undoing a caret set BEFORE `grab_focus` — and the hazard IS guardable headlessly, if the toplevel is MAPPED
**Scribobulate**: `preview/annotate/overlay.rs` and `window/editor_annotate.rs` both `set_position(-1)` after `grab_focus`; the editor card's ordering guard uses a `raise_card_over_mapped` helper for exactly this reason.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-124); #106 (same GTK select-on-focus behaviour, `GtkLabel` in a popover), #138 (the wrapper's `has_focus()` is not the delegate's).

## 140. A security gate answering a DIFFERENT question than the one being asked
**Routed**: GEP-45 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `links.rs` (`is_allowed_url` / `resolve_doc_link` / `scheme_of`) + `window/linknav.rs` (the dispatcher).

## 141. A "this will misbehave" theory read from a construction site, never executed
**Routed**: GEP-15 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 142. A capture-phase ancestor gesture cannot pre-empt a child's gesture and hand it back cleanly — "one similar event will be emulated" preserves event COHERENCE, not gesture STATE
**Scribobulate**: probed for a table-cell selection-promotion design; the routes above are why that design is not viable, and why table cells remain selection islands.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-171).

## 143. A PERMANENT register entry citing an EPHEMERAL artifact (an ISSUES entry, a PLAN file)
**Routed**: GEP-23 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: #142 was filed citing a plan file that was deleted ~20 minutes later, when the probe killed the design and its findings were folded into the issue register. The entry now inlines the gesture trace and the reduced repro and cites nothing ephemeral.

## 144. `unparent()` on an OPEN GtkPopover does not emit `closed` — it skips the close path entirely
**Scribobulate**: **`popdown()` THEN `unparent()`.** Both. In order.
**See**: gtk4-rs skill.

## 145. Two registers numbering their entries with the SAME prefix — every cross-citation is wrong-but-plausible
**Routed**: GEP-24 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: this register's `ScrAP-N` citation prefix and the citation-convention paragraph at the top of this file; `lint-references` check 8, which makes the ambiguous bare form illegal rather than defaulted.

## 146. Assuming `GdkTexture::from_file` ignores installed gdk-pixbuf loaders, and adding a manual `Pixbuf` fallback
**Scribobulate**: no manual fallback — load via `Texture::from_file` and let its built-in chain handle native + registered-pixbuf formats. If a format won't render despite an installed loader, fix the **registration** (regenerate `loaders.cache`, point `GDK_PIXBUF_MODULE_FILE` at the right one), don't route around `G…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-66; kin GTK4Rs/AP-34).

## 147. Raw-HTML `<picture>`/`<img>` silently dropped — block HTML is emitted per-line, wrapped in `Tag::HtmlBlock`
**Symptom**: three related failures. (a) A GitHub-style `<picture>…</picture>` hero (WebP `<source>` + GIF `<img>` fallback) renders as NOTHING in-app, even though it displays fine on GitHub — the app's own README hero was invisible when the app opened its own README.
**Root cause**: - (a) The renderer's pulldown-cmark event loop drops `Event::Html`/`Event::InlineHtml` via a catch-all `_ => {}` (sanitize-by-omission — correct for untrusted HTML). pulldown-cmark 0.13 emits a **block** HTML construct **line-by-line** — one `Event::Html` per source line — **wrapped** in `Event::Start(Tag::HtmlBlock)` … `Event::End(TagEnd::HtmlBlock)`.
**Lesson**: when a renderer mirrors an HTML element's semantics, honour the element's **grouping/scoping**, not just the presence of the child tags — and remember the *same* logical construct reaches you as **either a block or inline events** depending on formatting the author didn't think about, so grouping st…
**Scribobulate**: a pure, unit-tested scanner turns a fragment into an ordered tag stream (`PictureOpen` / `PictureClose` / `Candidate(src)`); the renderer replays that stream against a `<picture>` grouping state carried **on the `Renderer`, across events** (`feed_html`/`picture_open`).
**See**: TDD 2.23; TECH.md § Rendering (the rich-images work — `<picture>`/`<img>` + WebP fallback — was retired into this entry + #146).

## 148. Splicing at an offset mapped OUT of a delimiter-stripped coordinate space
**Symptom**: making a preview annotation from a selection that (a) spans more than one block AND (b) ends part-way through an existing `{==highlight==}{>>comment<<}` did **nothing the user could see** — no new comment chip appeared — and the reviewer's typed comment vanished.
**Root cause**: **an offset translated out of a delimiter-stripped ("cleaned") coordinate space is safe to READ from but not safe to INSERT AT.** A cross-block selection's end is mapped cleaned→original through the shift table (`cleaned_to_original`), and any cleaned offset that falls within a kept highlight's *content* maps to a byte **strictly inside** the original `{==…==}` — the `{==` and `==}` were deleted i…
**Resolution**: **before splicing at an anchor that came out of a stripped space, snap it to a boundary the stripped space could not see.** A point comment must land cleanly *outside* any construct — it never extends one (extending is the intra-block highlight path's job; a cross-block selection was deliberately ne…
**Lesson**: **a coordinate is only as trustworthy as the space it was measured in.** An offset from a projection that *deleted* structure (a cleaned/stripped/normalised view) can be dereferenced for reading but must be re-validated against the *full* text before it is used as an insertion or deletion point — th…
**Scribobulate**: `annotate::point_comment_anchor` (pure, unit-tested in `annotate/mutate.rs`), applied at the single commit choke point `window::annotate::apply_annotation_edit`'s `Point` arm — which **both** the preview sink and the editor Create card route through, so neither call site can forget the guard.
**See**: TDD 17.44 (the cross-block point-comment contract + the deliberate no-extend decision); #143 (an ANTI-PATTERNS entry must be self-contained — this one inlines the mechanism rather than citing the now-…

## 149. Two overlapping async scroll-drivers over one adjustment, neither cancelled by a newer navigation
**Scribobulate**: `codeview::CodePreviewView` `nav_generation`; bumped in `open_marker_popover_at`; checked in `converge_and_scroll_to_offset`'s tick and in `open_marker_popover`'s re-pin `value-changed` handler + disconnect-tick. TDD 20.16.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-172).

## 150. A self-drawn decoration re-adding padding that the line's own tags already put inside its `line_yrange`
**Scribobulate**: `codeview::mod` `snapshot_layer(BelowText)` code-block + blockquote loops; `codeview::geometry::span_card_y_extent`; the `code-block-top`/`code-block-bottom` tags in `tags.rs`. The abutting-`\n` construct comes from `start.rs`'s loose-list-paragraph branch (one `newline()`, not `block_sep`).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-127).

## 151. Detecting a URL scheme with "the text before the first colon" (`split_once(':')`)
**Routed**: GEP-46 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `links::scheme_of` (now the single source, shared by `is_allowed_url`, the doc-link gate, and `resolve_image` — the last had inlined its own `split_once(':')`).

## 152. A deferred idle closure that strong-captures a widget fires against it after teardown — and the reflexive guards each miss
**Scribobulate**: `codeview::geometry`'s `scroll_to_buffer_offset` and `scroll_to_cell_offset` (both idles + the nested inner refine idle, all sharing one `scroll_idle` slot); the cancel lives in `CodePreviewView`'s `WidgetImpl::unrealize`.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-128).

## 153. A `#[gtk::test]` integration suite renders on the default GskGLRenderer, not the renderer `main()` selects — and its GL texture cache SIGABRTs at teardown under a headless display
**Scribobulate**: `.cargo/config.toml` `[env] GSK_RENDERER = "cairo"`, mirroring `main.rs`'s in-process override; documented in POLICY § Architecture rules. This entry retires a former known-issue register entry — it inlines the mechanism rather than citing that ephemeral ID (#143).
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-129).

## 154. Migrating a hand-rolled weak capture to `glib::clone!` is not a blind find/replace — its single hoisted upgrade changes behaviour at several site shapes
**Scribobulate**: Convert only sites that are a single whole-closure gate. Pick the fallback to match the gone-path return: `()` doing-nothing → plain `#[weak]` (upgrade-or-`Default`); a specific value → `#[upgrade_or] <value>` (e.g.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63 the self-capture cycle this enforces; GTK4Rs/AP-128 the deferred-idle teardown).

## 155. A per-render widget whose `Rc` dismiss closure strong-captures its own container, while controllers on that container hold the `Rc` — an uncollectable cycle that strands the subtree every rebuild (unbounded reload leak); plus naming a GTK-internal allocator leak with no debug symbols
**Scribobulate**: Make the dismiss closure capture its container **weakly** (`downgrade()` + an `upgrade()`-guarded body; a no-op once the widget is gone is precisely "nothing to hide"). One weak edge breaks *every* cycle routed through the shared `Rc`.
**See**: gtk4-rs skill → threading-async-and-memory — this lesson is encoded there as

## 156. Reading a `GtkTextView` selection's anchor y from a wall-clock debounce after a scroll — the read lands before validation, so an on-viewport selection is suppressed
**Scribobulate**: Two independent halves. (1) **The decision is SHOW, not a pixel gate.** A pointer-drag selection is on-screen *by construction*, and a keyboard caret the view keeps visible; the only legitimate HIDE is a selection scrolled **away**, which the scroll handler's `value-changed` → `popdown` already cove…
**See**: gtk4-rs skill → **GTK4Rs/AP-142** (textview-layout-and-drawing) encodes the core lesson and adds a bidirectional correction to **GTK4Rs/AP-97**, whose "bounded `add_tick_callback` poll" prescription i…

## 157. Collapsing a large `GtkTreeListModel` while the `GtkListView` is scrolled to the bottom strands a stale far-end row
**Scribobulate**: Re-anchor the ListView to the **top** before collapsing — reset the outline scroller's vadjustment to 0 (`vadjustment().set_value(0.0)`) at the start of collapse-all, so the anchor is the surviving root row and the collapse removes only rows *below* it.
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-143; kin GTK4Rs/AP-111).

## 158. A content-less list item still emits a full item (and task marker) — an unconditional per-item gutter decoration draws a stray marker
**Symptom**: an empty task-list item `- [ ]` on its own line drew a checkbox in the preview gutter despite having no content. The same shape affected the other kinds: a content-less bullet (`- `) or number (`1.
**Root cause**: two compounding facts, both non-obvious. - The renderer pushed a `ListMarker` at **every** `Tag::Item`, unconditionally — nothing gated on the item actually producing content. pulldown-cmark emits `Start(Item)` … `End(Item)` for a content-less item too, so an empty item recorded a marker, and the gutter draw (which iterates every recorded marker whose first line is on-screen) drew it.
**Lesson**: a per-item — or per-block — decoration must gate on the item having produced content, not on the parser having emitted an item. CommonMark parsers emit a complete item (and even a task marker) for a content-less list item, so "the parser didn't give me an item" is the wrong emptiness test; "the rend…
**Scribobulate**: at `TagEnd::Item`, treat the item as empty when the walk inserted **no buffer content** for it (`end_offset() == item_start`) and drop the marker it pushed.
**See**: TDD 2.4b (empty items draw no marker); renderer `TagEnd::Item`; sibling pulldown quirks #66/#75/#147.

## 159. Centering a gutter marker on `line_yrange`'s height centers it over ALL of a soft-wrapped item's rows, not the first
**Scribobulate**: clamp the logical-line height to the first display row before centering: `h' = min(h, gap + single_line_h)`, where `single_line_h` is one row's text height from a fresh Pango layout in the view's own CSS-zoomed font (`view.create_pango_layout("0").pixel_size().1` — cache-free, zoom-correct for free,…
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-145); gutter `first_display_line` / `draw_list_marker`; sibling marker-gutter lessons ScrAP-157/ScrAP-158; the `iter_location`-avoidance rational…

## 160. syntect's bundled default syntax set has no TypeScript/TSX/TOML — a fence in one of those languages silently falls back to plain text and renders as one flat colour
**Symptom**: a ` ```typescript ` (also `tsx`, `toml`, `kotlin`, `swift`, `dart`) fenced code block in the preview shows as a flat, single-colour block — every token the same ink, i.e. "all gray" — while ` ```js `, ` ```rust `, ` ```python ` highlight normally.
**Root cause**: syntect's `SyntaxSet::load_defaults_newlines()` (its bundled set, derived from Sublime's default packages) does **not** include a TypeScript grammar — nor TSX/TOML/Kotlin/Swift/Dart. The emitter (`renderer/emit.rs::insert_code_block`) resolves the fence via `ss.find_syntax_by_token(lang).unwrap_or_else(|| ss.find_syntax_plain_text())`.
**Lesson**: a highlight engine that resolves an unknown language by silently falling back to plain text turns "unsupported grammar" into "looks rendered but isn't" — the failure is invisible and per-language.
**Scribobulate**: build the engine's `SyntaxSet` from **`two_face::syntax::extra_newlines()`** instead of `SyntaxSet::load_defaults_newlines()` (`renderer::syntect()`). `two-face` embeds bat's vetted syntax dump, a **superset** of syntect's defaults: it keeps every bundled grammar (js/rust/python still resolve) and a…

## 161. A CSS `margin-*` silently ADDS to a code-set `gtk_widget_set_margin_*` on the same axis — the stylesheet can never reduce the inset, so `margin: 0` still stops short of the edge
**Scribobulate**: delete the vertical widget margins from the handle's construction and express the whole vertical inset in CSS, with a comment at the deletion site recording *why* the axis is stylesheet-only.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-149).

## 162. `GtkTextView` reading position drifts toward the top under repeated horizontal resize — the re-wrap re-validation clamp, and the one width-changing path with no re-anchor hook
**Scribobulate**: the view already tracks the user's **settled top buffer LINE** continuously (maintained for zoom re-anchoring, ScrAP-65). On a genuine width change in the preview's `size_allocate`, re-anchor to that line through the existing **coalesced, deferred, weak-captured, `is_realized`-gated `scroll_to_mark`…
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-153); `sdd/CAM.md` Reading-Position Preservation CAM (row 7 — geometry change); the preview view's `size_allocate` raw-width re-anchor + `…

## 163. Switching a `GtkLabel` to `set_markup` silently makes every interpolated string a Pango-markup injection/breakage surface — an un-escaped filename metacharacter renders the label EMPTY, with no crash
**Symptom**: adding a coloured "⚠" deleted-backing badge to a tab required a per-glyph colour, which a plain-text `GtkLabel` (`set_label`) can't express — so the tab label was converted to Pango markup (`set_markup`) with the "⚠" wrapped in a `<span foreground="#e5a50a">`. The badge itself renders fine.
**Root cause**: `gtk_label_set_markup` runs the string through `pango_parse_markup`, which treats `&`/`<`/`>` as entity/tag syntax. `set_label` does not — the two entry points look interchangeable (both "set the label's text") but have opposite escaping contracts.
**Lesson**: `set_label` and `set_markup` are **not** drop-in swaps — converting a label to markup silently makes every interpolated runtime string an escaping obligation, and the penalty for forgetting is a **blank/garbled label + a soft warning**, never a crash, so a happy-path test with an ASCII filename pass…
**Scribobulate**: the tab strip's label is now markup, so the single funnel that composes it (`window/tabs/documents.rs::tab_display_markup`) escapes the filename with `glib::markup_escape_text` **before** interpolation, and the pure label formula (`winstate::decisions::tab_label_markup`) takes the **already-escaped*…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-154); `winstate/decisions.rs::tab_label_markup` (pure, escaped-name + colour param) and its unit tests; `window/tabs/documents.rs::tab_display_markup`…

## 164. Committing a test fixture whose filename is itself the invalid input breaks checkout on other platforms
**Routed**: GEP-29 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/links.rs` tests (`scheme_of`/`is_allowed_url` string literals = the cross-platform guard; the `#[cfg(unix)]` `resolve_image` colon temp-file test = the run-time-file precedent); TDD §19.7a (now marked unit-verified, fixture-free); `tests/fixtures/doc-links.md` + `tests/MANUAL-TEST.md` (colon ca…

## 165. Clearing an env var the wrong way gives a false confirmation
**Routed**: GEP-30 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `packaging/windows/README.md` ("Two things that will bite you"), which states the non-working alternative explicitly so nobody re-derives it.

## 166. Never diagnose a hung test suite from a parallel run
**Routed**: GEP-31 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `scripts/pipeline.steps` carries `--test-threads=1` on `cmd.windows integration`, with the reason recorded at the step — a serialised run prints test names as it goes, so a wedge names the body it wedged on instead of producing the silence that invited the wrong diagnosis.

## 167. An `Option`-returning lookup whose `None` is also a legitimate answer will fail silently forever
**Routed**: GEP-42 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/config.rs` (`config_home_fallback`), `src/session.rs` (`state_home_fallback`, `state_directory_resolves_without_any_xdg_override`); `sdd/TECH.md` § platform notes.

## 168. A popover's layout pass resizes the TOPLEVEL — from GTK's stale remembered size — collapsing a natively-maximized window
**Scribobulate**: `platform::win32::track_maximized_size` — while the window is maximized, and only then, keep GTK's remembered size equal to the size the OS actually gave it (`surface`'s `layout` signal → `set_default_size`).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-158); TDD 7.0d / MANUAL-TEST 7.0d (the running-window check, incl.

## 169. A pruned `-symbolic` icon name degrades to a legacy raster instead of failing — and `has_icon` is *stricter* than the render path, so the audit scores it green
**Scribobulate**: `tests/icon_resolution.rs` (the render-and-report audit, plus `--render`); `data/resources.gresource.xml` + `data/icons/scalable/emblems/` (the bundled replacements); `src/icons.rs` (the name table the audit walks).
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-174).

## 170. Symbolic icon art drawn with strokes silently changes shape — the SVG rasterizer you preview in is not the renderer that ships it
**Scribobulate**: `data/icons/scalable/emblems/*.svg` (both bundled icons are fills-only, with the mechanism recorded in each file's header); `tests/icon_resolution.rs --render` (renders through the shipping path).
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-170).

## 171. Every `#[gtk::test]` aborts on macOS before its body runs — the harness dispatches onto a worker thread, and GTK there requires the main one
**Scribobulate**: `tests/icon_resolution.rs` + its `[[test]] harness = false` declaration in `Cargo.toml` (the main-thread gate, plus the `#[path]` sharing trick); `src/icons.rs`, whose own `#[gtk::test]` was retired as redundant once one target covered both platforms.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-159).

## 172. A synthesized-click UI-automation tool can be silently broken, making a real bug look unfixable across several attempts
**Routed**: GEP-12 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 173. Freezing a drag icon with `current_image()` AFTER dimming the source widget captures a blank — `queue_draw` has already cleared the render node
**Scribobulate**: `src/widgets/tab/bar.rs` (`begin_drag_visuals`, the fused capture-then-dim, plus the `drag_icon_freeze_must_be_taken_before_the_handle_is_dimmed` regression guard — which asserts the correct order yields a render node *and*, as a deliberate mutation, that the wrong order yields none, so a refactor c…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-156).

## 174. A single-instance guarantee that lives in a backend, not an API, fails silently where the backend is absent — and the platform's *other* launch path will falsely confirm it works
**Scribobulate**: `src/platform/mac/single_instance.rs` (whole module, `#[cfg]`-gated at its declaration in `platform/mod.rs` per the `src/platform/win32/` precedent); `scribobulate::run` in `src/lib.rs` (`elect` between `setup_app` and `run_with_args`, skipped under `--new-instance`; it sat in `src/main.rs` until th…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-157).

## 175. A defect whose CONSEQUENCE is platform-dependent while the defect itself is not — the platform that never triggers it never tests for it, and a guard written on the triggering platform's symptom is permanently green where the bug actually lives
**Routed**: GEP-16 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/window/tabs/dnd.rs` — the `detach_overlay_from` call in `move_tab_to_new_window`, and `move_tab_to_new_window_detaches_the_source_windows_format_overlay` (the guard, with the "green for the wrong reason" argument in its doc comment).

## 180. A `set_parent`'d child left on a `GtkTextView` at dispose is an INFINITE loop, not a warning — and the suite that stayed green was the one never disposing anything
**Scribobulate**: `src/saferizer/persistent_popover.rs` (`teardown` = popdown → unparent, the correct shape) and its callers in `src/window/mod.rs`, `src/window/tabs/switch.rs`.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-80).

## 181. A suite that has never RUN on a platform is full of assertions that only look portable
**Routed**: GEP-18 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: neither fix lives in this branch. Both are platform-neutral — a `canonical_tempdir` helper for the temp-dir case, a `#[cfg]`'d `PRIMARY_LABEL` constant for the accelerator one — and were conveyed to the shared branch rather than carried here, per the rule that a platform branch holds no platform-neu…

## 182. A readiness probe stronger than the behaviour it gates fails on its own terms — `has_focus()` needs an ACTIVE toplevel, `notify::focus-widget` does not
**Scribobulate**: `src/window/editoractions.rs` — `focus_is_within` (whose doc comment now carries the rule), and the three waits realigned onto it.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-119).

## 183. A mutation that fails on an earlier precondition proves nothing about the guard under test
**Routed**: GEP-11 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 184. Four green checks, none of them the outcome — the plumbing was verified and the user-visible result was not
**Routed**: GEP-10 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 185. An idle queued from a native OS callback is not dispatched for seconds — and deferring the *read* answers the event with the wrong value
**Scribobulate**: read at the event, apply at the event. The value is sampled synchronously in the native callback, and the work is done there too — the callback was measured to be on the main thread, so touching the toolkit is legitimate.
**See**: gtk4-rs skill → deferred-work-and-ordering (GTK4Rs/AP-185).

## 187. A byte range captured at build time and applied at click time is a bet, not a coordinate
**Symptom**: clicking **Remove** on an annotation card in a document with unsaved edits deleted the wrong text. Reported by the operator as "it removes the wrong text and clobbers other content". Reproduced against the mutation core directly:
**Root cause**: the mutation was expressed as *"delete bytes 6..32"* rather than *"delete this annotation"*. A range is only meaningful against the exact string it was computed from, and this one was carried across time in a closure — captured when the card's content was built, applied when the button was clicked, with nothing in between re-establishing that the two strings were the same.
**Resolution**: never carry a range across time on its own — carry it together with **the text that occupied it**, which is what makes it re-findable. A small display-free type owns the rule: resolve to the captured offset if the text is still exactly there (the common case, one comparison, and it also disambiguate…
**Lesson**: an index into mutable content is a *reference*, and a bare integer is the one form of reference that cannot be checked. The moment such an index outlives the instant it was computed — stored in a closure, a widget's state, a queued message, a row model — it needs an identity that can be re-establish…

## 188. "It broke when I removed X, so X was providing it" — a temporal correlation dressed as a mechanism
**Routed**: GEP-15 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 189. A GTK doc comment promised scroll-tracking the code never implemented — and the same API silently feeds the view's minimum size
**Scribobulate**: keep the card as a popover attached with `set_parent`, which is in **neither** the anchored-children nor the overlay list and therefore contributes nothing to the view's size request — and re-derive its anchor from the annotation on every present and every scroll, rather than moving the card into a…
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-189).

## 190. A `show`-vfunc invariant has a hole exactly on the re-present path — showing an already-visible widget never runs it
**Scribobulate**: enforce at two points — the vfunc for genuine transitions into visibility, and the explicit "present" method unconditionally. Also clear any "we already pointed there" idempotence cache at presentation: that cache records what *you* last wrote, not what the widget currently holds, so anything writin…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-190).

## 191. A "pre-warm" of a reused widget becomes a teardown of a live session the moment that widget owns state
**Scribobulate**: the warm-up no-ops when the instance is already in use. (Returning after consuming the one-shot flag is right, not a leak: if a session is open the widget has already been realized and presented, so the cost the warm-up exists to absorb has already been paid.)
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-191).

## 192. `popdown()` is not animated, `closed` fires from `hide` — so your own transient hide trips your own "is it still open?" backstop
**Scribobulate**: keep the session flag distinct from visibility (still correct, for a better reason than the animation myth — a card can legitimately be *open but off screen*), and mark a self-initiated hide with a **sticky** flag: set when hiding, cleared when shown again or genuinely dismissed.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-192).

## 193. Driving/screenshotting a GTK4/Quartz app from an agent on macOS: no established recipe, and every gap in one reads as an app defect until isolated
**Scribobulate**: — the recipe assembled, piece by piece: - **Prevent the idle lock** for the session's duration: run `caffeinate -disu` in the background before starting any live-driving work.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-163).

## 194. A shared per-line helper that hands out a RAW line makes every block transform blind to the container prefix — one rule, four copies to get wrong
**Symptom**: every block formatting command corrupted a blockquoted line. Heading 3 on `> Heading` produced `### > Heading`; Bulleted/Numbered/Task List on `> item` produced `- > item` / `1. > item` / `- [ ] > item`.
**Root cause**: the block formatters all opened by resolving a whole-line span through one shared helper (`text::block_span`) and then reading `span.text` and splitting it on `'\n'` — i.e. the shared layer handed each transform the **raw line**, prefix and content fused.
**Lesson**: when a family of transforms shares a helper, **what the helper hands back defines the blind spot they all inherit** — a raw line is a fused pair (container, content) and every consumer that treats it as content is wrong in the same way.
**Scribobulate**: the split happens once, in the shared block-span layer, and the raw form is sealed off. - `BlockSpan::lines()` yields `BlockLine { prefix, content }`, splitting on the one existing `quote_prefix_len` parser (`>` runs each with an optional single space; ASCII, so the byte split is char-safe).
**See**: TDD 10.20 (block commands inside a blockquote); MANUAL-TEST 10.20; `format::text` module docs ("The container-prefix seam"); the enforcement ladder is the gtk4-rs skill's GTK4Rs/AP-108/GTK4Rs/AP-130.

## 195. A decision driven off one parser's event stream cannot see the constructs a second tokeniser owns
**Symptom**: annotating a *partial* selection inside `==highlight==`, `~~strike~~`, `^sup^` or `~sub~` spliced the CriticMarkup **between** the delimiters — `a ==m{==ar==}{>>note<<}k== b` — markup that parses as neither construct.
**Root cause**: `copymap::balance_source_span` decided what to swallow by walking pulldown-cmark's event stream and matching `Code`, `Emphasis`, `Strong`, `Strikethrough`, `Link`, `Image`. The four failing constructs are **not pulldown constructs here**: pulldown has no highlight/mark option at all, and its caret/tilde flanking rules never match the tight Pandoc forms authors type, so this crate tokenises all fou…
**Lesson**: when a project parses *some* of its syntax with a library and *some* itself, every decision derived from the library's output inherits a blind spot exactly the shape of the syntax you own — and it fails silently, because your constructs are indistinguishable from prose in that stream.
**Scribobulate**: make the second tokeniser span-shaped and consult it. - `renderer::scan_script_spans(text) -> Vec<ScriptSpan { outer, inner, script }>` is now the primitive — the **single definition** of what these four constructs are — returning the whole-construct (`outer`, delimiters included) and content (`inne…
**See**: TDD 17.33 / 17.18; CAM Document Rendering row 3 (widened to name both tokenisers and both paths); `renderer::scan_script_spans`; `copymap::balance_source_span`; sibling pulldown quirks #66/#75/#147/#1…

## 196. A fallback keyed on a symptom, not a cause, silently swallows the next cause that shares the symptom
**Symptom**: found on the live display while verifying #194/#195, not by any test. An annotation's amber claim highlight covered the **whole** text run instead of the claim, on any line holding one of the four in-crate constructs.
**Root cause**: the cleaned-source→buffer mapper decided per content event, and gave up — tagging `(before, after)`, the whole event — whenever `buf_len != cleaned[s..e].chars().count()`. That branch is correct and necessary for a **synthesised** run: smart punctuation (`--`→`–`, `...`→`…`) and entities substitute characters, so there is no per-character correspondence and tagging half a synthesised glyph would b…
**Lesson**: when you write a defensive fallback, key it on the **cause** you are defending against, not on the observable that led you to it. A symptom-keyed guard is a permanent trap door: every later cause that happens to present the same way falls through it silently, and because a conservative fallback *deg…
**Scribobulate**: test the cause instead. `annotate::kept_chars` counts the chars of a run that actually reach the buffer (construct delimiters dropped, everything else 1:1) from the scanner's spans; the precise path runs whenever that count equals the event's buffer length — which **subsumes** the old 1:1 case (no c…
**See**: TDD 17.18 (claim extent, including the marker-stripped case); MANUAL-TEST 17.39; `annotate::kept_chars` / `map_cleaned_highlight_to_local`; siblings #194 (one rule, N copies) and #195 (two tokenisers,…

## 197. A `#[path]`-included module's children resolve against the attribute's directory, not the module's own name
**Routed**: GEP-34 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: the second crate root sits beside `lib.rs` in `src/` (`src/gtk_suite.rs`), not under a relocated `#[path]`, so `mod` declarations resolve exactly as `lib.rs` resolves them.

## 198. `pub use` cannot widen `pub(crate)` visibility — there is no test-façade shortcut around it
**Routed**: GEP-34 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/gtk_suite.rs` is compiled as part of the crate (`[[test]] harness = false`, sharing `lib.rs`'s module tree) rather than as an external façade re-exporting internals.

## 199. Treating an `insert-text` of `"\n"` as "the user pressed Enter" — a paste is many `insert-text`s, one of them a bare newline, and acting on it is undefined behaviour
**Scribobulate**: decide where the keystroke is, so that paste, middle-click PRIMARY paste, drag-and-drop and every programmatic `insert_range` cannot reach the decision **at all** — `src/window/editbar/newline.rs` now runs the whole edit from a CAPTURE-phase `GtkEventControllerKey` on the view (GtkTextView commits R…
**See**: gtk4-rs skill.

## 200. `GtkSourceIndenter` is unusable from gtk-rs — the subclass trampoline frees the caller's `GtkTextIter`
**Symptom**: implementing `GtkSourceIndenter` — the sanctioned, keystroke-only home for auto-indent behaviour (`is_trigger(view, location, state, keyval)` + `indent(view, iter)`, `GTK_SOURCE_AVAILABLE_IN_ALL`) — SIGSEGVs the app on the **first Enter**, with no warning, no panic, and an empty stderr.
**Root cause**: the binding's subclass trampoline takes the caller's **transfer-none** `GtkTextIter*` with `from_glib_full`, so the Rust wrapper owns it and frees GtkSourceView's own iterator when it drops (`sourceview5-0.10.0/src/subclass/indenter.rs:107-110`):
**Resolution**: don't use the interface from Rust at this version. Do by hand what it would have done, from the same place: a `PropagationPhase::Capture` `GtkEventControllerKey` on the view (GtkSourceView installs its own capture-phase key controller for exactly this purpose, `gtksourceview.c:1442-1443`), mirroring…
**Lesson**: the discriminator that saves the hour is **re-run the crash with an empty vfunc body**. A segfault inside a freshly written subclass reads as "my code is wrong" and invites a long bisect of one's own logic; if it still crashes doing *nothing*, the binding is the defect and the correct move is to rou…

## 201. A custom `harness = false` runner that ignores libtest's `--skip` turns a carve-out into a selection — silently, and green
**Routed**: GEP-32 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/gtk_suite.rs::parse_args` — an explicit `VALUE_FLAGS` list whose values are consumed before filtering, and `--skip`/`--skip=` honoured as a repeatable exclusion; guarded by its own `parse_args` unit tests.

## 202. A gate in front of a paint-carried dispatch: the settled state is the one that queues no paint
**Scribobulate**: gate on a **local predicate about the state already in front of you**, not on a flag another loop must set on some future tick. Recompute the very aim the loop applies — the target's current best-known position, clamped to the scrollable range — and ask whether the adjustment is there yet.
**See**: gtk4-rs skill → deferred-work-and-ordering (GTK4Rs/AP-202).

## 203. Restoring `SIG_DFL` and re-raising inside a fatal-signal handler exits *normally* with status 139 — the signal is blocked for the handler's own duration
**Routed**: GEP-35 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/forensics/signal.rs::die` — the signal is unblocked between restoring the default disposition and re-raising it, so the process dies by the signal rather than exiting normally with its status.

## 204. Resolving a kernel segfault `ip` against `nm` output — the kernel's VMA base is the executable *segment*, not the ELF load base
**Routed**: GEP-36 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 205. Predicting one platform's rendering from another's at the same toolkit version — the distributor's theme decides, not the version number
**Scribobulate**: write both channels from a single shared writer that neither platform owns, so the version rule has one definition rather than one per platform — and give **each** platform its own pixel assertion rather than extending one platform's conclusion to the other.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-205).

## 206. A reference gate whose pattern demands a file extension the codebase's citations never write — clean, green, and blind to every dangler of that shape
**Routed**: GEP-2 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `cargo xtask lint-references` check 6a — the plan pattern matches the bare `PLAN.<topic>` citation form as well as the full filename, `.md` listed first so a whole filename still matches under both regex engines.

## 207. Two ports of one gate that share a pattern but not a file ENUMERATION — the parity claim is false, and the platform nobody runs is the lenient one
**Routed**: GEP-3 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `scripts/lint-references.scan` — one enumeration definition the gate reads rather than restates, with `maxdepth` as a hard tripwire rather than a filter.

## 208. A proc macro that moves the annotated item's attributes onto the generated BODY instead of the harness item — `#[ignore]` silently does nothing
**Routed**: GEP-33 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 209. A guard test whose setup prevents the resource from ever existing cannot observe the leak it guards — it passes with the fix deleted
**Routed**: GEP-1 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 210. Windows PowerShell converting a value on your behalf instead of failing — the call site reads correctly in every instance
**Routed**: GEP-28 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `packaging/windows/pipeline.ps1` — every conversion pinned rather than defaulted: `-Encoding` on both halves of a round-trip, quoting around native arguments containing braces, `$m.Success` tested before a capture is read, and `$LASTEXITCODE` checked deliberately.

## 211. A verification whose result nothing consumes — it reported the mismatch, and the corrupted payload was applied one line later
**Routed**: GEP-6 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 212. `#[cfg(unix)]` on a test and "skipped on Windows" are indistinguishable in the report, and only one of them is true
**Routed**: GEP-4 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 213. An artifact that describes what you meant to do, shipped beside what you actually did, and never reconciled
**Routed**: GEP-21 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 214. `backtrace_symbols`'s BSD twin is not the safe half of the pair — the async-signal-safety argument you inherited is about a different hazard
**Routed**: GEP-37 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 215. Verifying a behaviour-preserving refactor with hand-written expectations tests your belief about the code, not the change you made
**Routed**: GEP-8 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 216. A gate that checks a citation EXISTS cannot see one that points at the wrong real thing
**Routed**: GEP-24 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `lint-references` check 8 plus this file's citation convention — the two legal forms are single unique tokens and the ambiguous bare form is illegal, so a citation's register is decided by its text rather than by when it was written.

## 217. A negative result is worthless without a positive control — "it was prevented" and "I cannot see it" produce identical output
**Routed**: GEP-12 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: the positive control accompanying every negative result in `packaging/windows/pipeline.ps1`'s verification steps — the same probe re-run with the guard removed, required to show the effect.

## 218. Confidence ratchets across a relay — the hedge is dropped by whoever summarises, and nobody does anything wrong
**Routed**: GEP-19 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 219. A remedy that lives inside one consumer reaches the consumers that already knew about it
**Routed**: GEP-25 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 220. A regression guard built from the instance you fixed has coverage exactly equal to the fix
**Routed**: GEP-5 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 221. A comment explaining why a test asserts less than its name promises is where a false premise hides
**Routed**: GEP-9 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 222. Two gates, each correct, enforcing opposite things — and neither can see the other
**Routed**: GEP-26 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 223. Write a finding as a testable proposition, not as a conclusion — a conclusion recruits agreement, a proposition recruits a measurement
**Routed**: GEP-20 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 224. A squash makes single-seat authorship unprovable — on a deadline nobody is watching
**Routed**: GEP-22 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 225. Four denial-of-service paths in four subsystems were one omission: nobody had said the project had an opinion about input size
**Routed**: GEP-27 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 226. The check your self-test does not cover is the one that ships broken — and a single-file corpus cannot falsify a multi-file bug
**Routed**: GEP-3 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 227. A two-axis hazard gated on one axis — the seam passes, and the assertion it exists to prevent fires through it
**Scribobulate**: `src/saferizer/popover_anchor.rs` — a `Viewport` value carrying **both** extents, one `axis_visible` predicate applied to each, `CARET_SLIVER_W` shared between the gate and `pin_above` so the rectangle checked is the rectangle pointed at, and `saturating_add` in the predicate.
**See**: gtk4-rs skill.

## 228. A property implemented on one branch, documented as a property of the whole function
**Symptom**: fixing an unrelated format ambiguity made `the_marker_forgets_reports_that_no_longer_exist` fail — a test that had passed since it was written, over code the fix did not touch.
**Root cause**: `announce_unread_report`'s comment claimed the seen-marker was "pruned to reports that still EXIST, which is what keeps a set-valued marker bounded". The pruning lived in `seen_set`, and only in its **legacy-watermark** branch, where it is structural — a watermark is *evaluated against* the present set, so filtering by it is how that branch works at all.
**Scribobulate**: `src/forensics/report.rs` — `seen_set` applies one `extant` predicate on every branch and owns the bound in its own doc comment; the writer's comment now points at it rather than restating it.

## 229. A seam named for a guarantee it delivers on one platform — and the permission model that lives in the directory, not the file
**Routed**: GEP-38 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 230. A clippy method ban does not cover the builder property of the same name
**Scribobulate**: treat the ban and a runtime assertion as **one mechanism with two halves**, not as a primary gate plus a nice-to-have.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-130).

## 231. Retiring an ambiguous citation form by LEGALISING it instead of banning it — and a completeness claim with no predicate
**Routed**: GEP-24 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `cargo xtask lint-references` check 8 (the per-site migration rule and the audit's limits are documented at the check, next to the gate that enforces the form); the citation-convention paragraphs at the top of this file and at "Numbering reconciliation"; POLICY step 9.

## 232. `g_file_replace_contents` is atomic only under the right flags — and its one remaining fallback deletes the previous file before failing
**Scribobulate**: `src/window/swap.rs` owns the promote — co-located `<name>.swap.tmp` opened with `replace_async` (`PRIVATE`), renamed into place only after a complete write; three tests pin it, one characterising the cancelled-close GLib branch. Format: SCHEMA.md § "Crash-recovery swap file". Contract: TDD §22.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-167); researcher findings — `~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-replace-contents-atomicity-durability-threading.md`…

## 233. Delegating a delimited format's unforgeable-terminator invariant to a third-party serialiser's escaping
**Symptom**: A frontmatter-style file format — a magic line, a TOML metadata block, a bare `+++` terminator, then a verbatim payload — silently truncated its payload when one metadata value (a filesystem path) contained a newline.
**Root cause**: The `toml` crate (0.8) chooses between basic, literal and **multi-line** string forms by an internal heuristic, and for any value containing a newline it selects a multi-line basic string — whose defining purpose is to reproduce those newlines verbatim:
**Resolution**: Enforce it twice — by construction, then by verification.
**Lesson**: **When you write down that a hazard is handled *by someone else's code*, that sentence is a hypothesis with a test attached, not a conclusion.** The tell is a design note that identifies a risk precisely and then discharges it by appeal to an upstream guarantee — the precision of the analysis lends…
**Scribobulate**: `src/swapfile/codec.rs` — `to_wire`/`from_wire` (construction) and the `encode` fence check (verification); the invariant is stated in the module doc. Sibling of #232, which came from the same feature: both are cases of a convenience API's advertised behaviour being narrower than its name.

## 234. Asserting one of a feature's two representations, and reading the green suite as evidence about both
**Routed**: GEP-10 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 235. Wiring a startup feature into one framework entry point and assuming it covers launch
**Scribobulate**: capture the cold-start signal at each handler's entry — *before* anything creates a window — and route every entry point through **one gated helper whose doc comment enumerates its callers and states that a missing one is a total failure for that route**.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-235).

## 236. A screen-coordinate capture is not a window capture
**Scribobulate**: activate, settle, then capture — and gate the capture on the activation's
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-236).

## 237. A `cfg`-gated gate proves nothing about the branches it did not compile
**Routed**: GEP-4 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 238. Activating on a click gesture's `released` alone — the release that ends a drag is not a click
**Scribobulate**: one seam (`saferizer::ClickActivation`) that owns **both** connections — callers hand it a hit-test returning the target's *identity* and an activation, and never write either signal handler.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-169).

## 239. `git stash pop` restores the source, not the binary — a control run that silently drives the old build
**Routed**: GEP-14 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `tests/MANUAL-TEST.md` §1.7 — the control build is copied and named explicitly before the fix is applied, rather than assumed to still be on disk.

## 240. A detector that enumerates the VOCABULARY of a free-text citation is defeated by a synonym
**Routed**: GEP-2 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `cargo xtask lint-references` check 1 — the pattern matches the *shape* of a reference rather than enumerating the connecting nouns a citation might use.

## 241. A process NAME is not an identity — pid reuse defeats every liveness probe, on every platform
**Routed**: GEP-17 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 242. `clippy --all-targets` WITHOUT the feature flag reports dead-code errors in files you never touched
**Routed**: GEP-4 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 243. GLib's I/O thread pool is one process-wide pool of ten — moving I/O off the main thread makes it contend with the crash-recovery writer
**Scribobulate**: `src/docio/pool.rs` bounds this project's own use at the source — `MAX_CONCURRENT = 4` admitted operations, a FIFO of waiters, a `Slot` released on `Drop`; its module doc carries the measurements and `sdd/TECH.md` § Concurrency model the consequence.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-243). Findings: `~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-task-thread-pool-sharing-starvation.md` (GLib 2.72, rig `_src/g…

## 244. Making a window-scoped operation async turns "which tab is active?" into two different questions
**Routed**: GEP-43 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/window/save.rs` — every step takes an explicit `Rc<TabState>`, resolved once when the user acts and carried through the read, the dialog and the write.

## 245. An Xvfb UI drive can deliver nothing and look exactly like one that delivered everything
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-245).

## 246. GDK-Win32 refuses an empty window title and substitutes a literal "."
**Scribobulate**: `window::save::confirm_dialog` sets `winstate::APP_NAME`; the one place all three modal confirmations (close prompt, overwrite warning, save error) are built.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-246).

## 247. "No handler is registered for this scheme" is not a safety property
**Scribobulate**: `renderer::end`'s `INERT_URI`, plus `the_probe_uri_is_one_gtk_refuses_to_launch`, which asserts `glib::Uri::is_valid(INERT_URI, UriFlags::NONE).is_err()` on every platform — the enforcement mechanism for the claim, per POLICY § Typed GTK seams.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-247).

## 248. A randomly-minted identity correlates only with the mechanism that persisted it
**Routed**: GEP-44 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `swapfile::recovery::disposition`'s `tab_at_same_path` parameter (the decision) and `window::swaprecovery::tab_id_at_same_path` (the filesystem half, kept out of the display-free core). Contract is TDD 22.17, with 22.16 as the boundary.

## 249. A capability whose backend is a HELPER EXECUTABLE is a packaging obligation, and the dev tree cannot fail the test
**Routed**: GEP-39 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `packaging\windows\stage.ps1` `$helpers` (hard-fails like the DLL list, so a future gvsbuild layout change is a build error rather than a silent regression); the corrected claims in [TECH.md](TECH.md) (platform table + the single-instance architecture bullet) and `tests/MANUAL-TEST.md` (§A *Launch &…

## 250. A widget swapped in for one feature's sake moves its text out of every text-walker's reach
**Symptom**: The find bar reports "No matches" for a word the reader can see on the page. In a table, `| [Handbook](…) |` — a cell that is *nothing but* a link — is never found; the same word written as `see [Handbook](…) again`, in the cell beside it, is found normally.
**Scribobulate**: `widgets::table::linkcell` — `link_cell_button` (the only sanctioned way to build a link cell; `gtk4::LinkButton::with_label`/`::new` are banned in `clippy.toml`, the seam and the two GTK-emission probes in `renderer::end` carrying the only allows) and its twin `link_cell_caption`, consumed by `prev…

## 251. A distribution GTK has its entire introspection surface compiled out, and the three channels fail in three different ways — one of them by reporting a number that means "healthy"
**Scribobulate**: the introspection-surface probe used when verifying against a distribution GTK build — the channel that reports a number is the one that lies, so the probe asserts on the transport rather than the returned value.
**See**: gtk4-rs skill → ui-testing-debugging (GTK4Rs/AP-251, the UI-driving half); general-engineering-principles (GEP-13, the prove-it-emits half). **Both** — each carries only half this lesson.

## 252. A drive step routed through an app command inherits that command's own enablement gate — and a disabled `GAction` swallows the step in silence
**Scribobulate**: no application code — but the third instance's corrective IS carried in this tree, as the fresh-`Get-Process` rule in `tests/MANUAL-TEST.md` §A.3's Windows launch step.
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-252).

## 253. The `org.gtk.Actions` probe answers about the operator's app when addressed by the well-known name — a `--new-instance` app must be probed by its UNIQUE name
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)
**See**: gtk4-rs skill → ui-testing (GTK4Rs/AP-253).

## 254. An invariant held by two sufficient mechanisms is mutation-proof one at a time — so the mutation test calls each of them dead code
**Routed**: GEP-11 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 255. A construct whose glyphs are buffered at its `End` event is not opaque — it is char-precise in a coordinate space nobody wrote down
**Symptom**: Selecting a couple of words inside a rendered code block and choosing Copy — from the context menu, the Edit menu, or Ctrl+C, all one `win.copy` action — put the **entire fenced block, fences included** on the clipboard.
**Root cause**: The copymap captures each render event's live buffer range as `(before, after)` around that event's processing. That is exact for every construct whose interior events insert their own glyphs — and a code block's do not: `Renderer` *accumulates* the body while the `Text` events go by (inserting nothing, so each captured range is **zero-width**) and flushes the whole block in one syntect-highlighte…
**Resolution**: `copymap::code_block_node` lays the interior events' source runs out across the `End` event's buffer range, in order, producing one leaf per run — and **proves the layout before trusting it**: the flushed char count must equal the body's, mirroring `insert_code_block`'s own rule (trailing blank line…

## 256. A gate's threshold is copied by hand out of a multi-metric report — so maintaining the gate is how you break it
**Routed**: GEP-7 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — a discipline lesson with no implementation in this tree. (Stated, not omitted: an absent field and a dropped one look identical.)

## 257. `Trying to snapshot GtkGizmo … without a current allocation` is GTK's own scrollbar trough — the one benign member of a warning family whose other members are real bugs
**Scribobulate**: Two techniques, both cheap and both reusable:
**See**: gtk4-rs skill.

## 258. Replacing a live `GtkTextView`'s buffer is a use-after-free, not a swap — the layout's line-display cache survives `set_buffer` and dangles
**Scribobulate**: **render into the view's own buffer instead of replacing it.** The preview re-render now clears the live buffer and refills it, so the buffer object never dies and no cached display can outlive it; the clearing delete also invalidates the old content's entries on GTK's own delete path, which carries…
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-258).

## 259. A rendering feature built for one of a construct's widget shapes leaves the others inert, and the reader sees one capability behaving at random
**Scribobulate**: emit the link into the cell's markup exactly as the inline-format tags are emitted (open at the link's start, close at its end, so it composes with bold/italic and with the tight `==`/`~~`/`^`/`~` constructs the crate scans itself), and give **both** cell shapes one activation seam that routes to th…
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-239).

## 260. A `GtkTextView` scroll aimed past the lazily-validated frontier is parked and never re-issued — and the validation idle cancels the one scroll it does animate
**Scribobulate**: `src/farscroll.rs` owns the re-issue — an idle *below* GTK's permanently-ready priority-125 validate source, which is therefore an exact "the layout is valid now" event GTK does not otherwise expose, bounded by a `g_timeout_add` keyed on STALLED PROGRESS rather than elapsed time (a legitimately huge…
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-260) for the mechanism, the refuted pin-to-the-frontier design (a livelock: 20 000 lines took 98 ms unpinned and 1494 ms pinned), the priv…

## 261. A derived-state hook installed at the producer misses the rebuild shape the producer also has
**Symptom**: a hook that keeps state consistent with the rendered document fires for every re-render *except* the one the feature exists for. The headless test — which drives the in-place re-render — passes; on the live display the same scenario leaves the state stale, silently, with no warning and no log line.
**Root cause**: "the preview was rebuilt" is **two code paths, not one**. Preview mode's external reload rebuilds by a wholesale render into a brand-new scroller/view widget; split mode re-renders the existing view in place. A hook installed on the in-place path is absent from the wholesale one, and nothing types, lints or tests the difference — the producer's shape is invisible from the hook's own site.
**Resolution**: move the hook off the producer and put it **immediately in front of its consumer** — here, reconcile the history against the live heading set inside the function that computes the two actions' sensitivity, so the reconciliation and the value it protects are computed in the same call and cannot disag…
**Lesson**: when a producer has more than one code shape, a hook on the producer is a latent regression — the next shape added will not have it, and a test written against the shape you are looking at will not notice. Prefer siting derived state in front of the
**Scribobulate**: `window/navhistory.rs`'s `reconcile_nav_history_headings` is private and called from `refresh_nav_history_actions` (before it reads `nav_can`) and from `traverse` (before it steps); `preview/render.rs` deliberately calls nothing.
**See**: kin #52 (the same two rebuild shapes, reached through a stale *signal* rather than a missing hook — different root cause, same architectural fact); TDD 23.14.

## 262. A restore seam's "nothing to do at the boundary" shortcut is a claim about its first caller, and the second caller loses a real destination
**Routed**: GEP-53 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `preview/scroll.rs`'s `restore_preview_scroll_to_line` (no `line <= 0` return; negatives clamped), reached from `window/navhistory/traverse.rs`'s `restore_place` for a `NavSpot::Line`, and from `window/zoom.rs` for the zoom re-render it was originally written for.

## 263. `line_at_y` on a not-yet-allocated `GtkTextView` reports the buffer's LAST line, so a viewport read taken in the turn a view is built in is maximally wrong
**Scribobulate**: `saferizer/viewport.rs` gates `ViewportTopIter::of` / `ViewportRange::of` on `visible_rect().height() > 0`, answering with the buffer start when there is no layout; `codeview`'s `reading_line` and `preview/scroll.rs`'s `preview_top_line` route through that seam instead of hand-rolling `vadjustment()…
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-263). Findings: tests/reports/gtk4skiller-brief-ScrAP-263.md (local, gitignored — the woven brief, incl. the measurement tables).

## 264. A focused anchored child swallows its host `GtkTextView`'s navigation key bindings, and the document silently refuses to move
**Scribobulate**: `codeview::navkeys::wire_document_navigation_keys`, wired from `CodePreviewView::new` (the one place a preview pane is built), redirects a navigation key to the view with `emit_move_cursor` from a **capture-phase** `GtkEventControllerKey` — the only phase that still sees the key — when `keynav::Focu…
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-264, woven 2026-08-09; it sits beside **GTK4Rs/AP-53**, which already owns the general propagation rule — a focused composite's own class-level keyb…

## 265. A test that arms a process-global fatal-signal handler and never disarms it re-points the rest of the suite — and displaces the runtime's own stack-overflow guard, so a later overflow stops naming itself
**Routed**: GEP-54 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `forensics::signal::tests::ArmedHandler` — an RAII guard that takes the install lock, snapshots every fatal disposition in `FATAL_SIGNALS` (four when this was written, five since ScrAP-268) **and** the calling thread's alternate signal stack, arms, and restores both on drop.

## 266. A focused popover that is its own `GtkNative` is an application-keyboard dead zone
**Scribobulate**: `codeview::card::app_accelerator_controller` — a BUBBLE-phase `GtkShortcutController` on the card carrying one `GtkNamedAction` shortcut per binding from `app::accelerator_bindings()`, the same enumeration `register_accelerators` binds from (a hand-listed set here would be a second copy of the accel…
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-266) — which also records this as the other face of GTK4Rs/AP-264's `GtkNative` gate.

## 267. A `GtkSingleSelection` built `.model(…).autoselect(false)` opens with a phantom selection
**Scribobulate**: `annotations_view::build_annotations_content` builds `.autoselect(false).can_unselect(true).model(&store)` — the flag before the data it governs. Guards: `a_freshly_built_list_selects_nothing` (mutation-tested by swapping the two builder calls back) paired with `a_restored_selection_lands_on_the_ann…
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-267).

## 268. A GLib **fatal** log message dies of `SIGTRAP`, not `abort()`, so a crash handler that takes the classic four signals reports nothing for the whole `g_error` class
**Scribobulate**: `forensics::signal::FATAL_SIGNALS` gains `SIGTRAP` beside `SIGSEGV`/`SIGBUS`/`SIGILL`/`SIGABRT`, and `signal_name` names it. Two guards, both mutation-tested and neither implying the other: `every_enumerated_fatal_signal_is_reported_and_still_kills_the_process` drives a real death per entry and chec…
**See**: routed to the `gtk4-rs` skill (app-lifecycle-and-env) as a GTK/GLib-stack lesson — any GTK4 application that installs a fatal-signal handler has this hole.

## 269. Two sufficient monitor-cancel mechanisms made the rename guard pass with its own fix deleted — and a freshly attached `GFileMonitor` is not yet watching
**Scribobulate**: `window/rename.rs` cancels the monitor before the rename and re-attaches on **every** path including failure; `the_old_monitor_is_cancelled_before_the_rename_touches_the_filesystem` is the state guard (mutation-checked: removing the cancel fails it and only it), and `a_rename_does_not_look_like_a_de…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-269), which carries the GIO source trace, the per-backend event sets and counts, and the pre-2.84 `cancelled`-property defect.

## 270. Asking GIO what a file is called after you renamed it, and being told what you asked for
**Scribobulate**: `docio::rename::stored_spelling` enumerates the parent and matches on `id::file` — which entry *is* this file, not which looks like it — and is best-effort throughout, every failure keeping the requested spelling, because a rename that SUCCEEDED must never be reported as failed because a follow-up R…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-270), which holds the mechanism, the nine-tag source trace, the Windows backslash/typed-case riders and the cause-vs-mechanism testing lesson.

## 271. Matching a directory entry by `id::file`, which identifies the FILE and not the entry
**Scribobulate**: `docio::rename::stored_spelling` decides only after the whole enumeration — the requested spelling returns `None` on sight, an identity match is held and answered with only if that spelling never appears.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-271). Kin: ScrAP-270 (the fix this was found inside).

## 272. A plan obligation written as a property of an artefact, which reads as done once the artefact exists
**Routed**: GEP-67 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `docio::rename::recover_rename_orphan` is the missing recogniser, called from `docio::read_document_blocking` — the module's only door, so one placement covers Open, session restore, link navigation and crash recovery, and ordered *ahead* of the admission check so a recovered file is stat'd and size…

## 273. A runtime skip announcement shredded by libtest's own progress output — and one shred read `SKIPPED [rubric]: ok`
**Routed**: GEP-25 (module-name half); the platform half in its platform entry — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `testsymlink::skipped` builds the whole line — newline included — and emits it with a single `std::io::stderr().lock().write_all()`; a sub-`PIPE_BUF` write is atomic on the pipe the pipeline reads through. Verified 6/6 clean on the same command that produced 2/4 corrupt.

## 274. A provenance tally that counts measurements instead of outcomes, and so reports the opposite of its evidence
**Routed**: folded into GEP-20 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: ScrAP-269's macOS paragraph now splits the correction from the confirmation explicitly and states the semantics/multiplicity distinction; the rename feature's platform-gap record had its monitor-event-count row reopened for Windows after being closed wholesale on the strength of a *macOS* measuremen…

## 275. A `GFileMonitor` created while its parent DIRECTORY is absent is permanently dead on Windows, and self-heals everywhere else
**Scribobulate**: not reachable today — every site that attaches a monitor (`app::attach_file_backing`, the rename re-attach, crash-orphan recovery) does so for a document whose directory exists.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-275), which holds the three-layer source trace, the per-backend self-heal behaviour, and the two transferable halves (a well-formed question aimed at t…
---

## 276. A parity artefact written to the console instead of the success stream — the documented diff produced an empty file, and the self-test rebuilt the list rather than calling the printer
**Symptom**: `-ListSteps > file` exits 0, prints the list to the log, and writes a zero-byte file — while `-SelfTest` passed on the same runner seconds earlier.
**Scribobulate**: `packaging/windows/pipeline.ps1` (`Write-StepList`, `Invoke-SelfTest`), compared by `scripts/pipeline-parity.sh` from `.github/workflows/pipeline.yml`.
**See**: general-engineering-principles — the self-test half (GEP-3), the success-signal half (GEP-52).

## 277. A test whose subject IS a by-design CRITICAL blocks a process-wide fatal-criticals switch — and "do not weaken the test" was the wrong reading of the trade
**Scribobulate**: `saferizer::popover_anchor`'s round-trip now PARENTS the popover, which strengthened the assertion rather than narrowing it — `pointing_to`'s fallback is a zeroed rect when parentless and the parent's own bounds when parented, so the seam is now proven to discard a BELIEVABLE rectangle rather than an obviously-empty one. The stateless-action defect it surfaced (`change_action_state` on `win.find-replace`) is fixed at the call site. `G_DEBUG=fatal-criticals` is armed in `scripts/run-integration.sh` with no allow-list, which is what the corrected reading bought.
**See**: gtk4-rs skill → ui-testing-debugging (GTK4Rs/AP-319), which holds both abort cases, the `gtk_popover_get_pointing_to` NULL-parent fallback mechanism, and the "price the edit" argument including the premise this entry originally got wrong.

## 278. A filename or file-existence predicate standing in for a semantic question — six measured cases, each confidently wrong

**Symptom**: a check answers with total confidence and is wrong in whichever direction costs
more. A cross-reference gate reported two dangling documents that were Rust field accesses. A
licence audit reported four projects as shipping no licence text when one ships eleven. A
dependency probe reported seven runtime contracts as missing from a machine that resolves all
of them.

**Root cause**: each predicate is a *structural* stand-in for a *semantic* question, and the
substitution is invisible at the call site.
- `\bPLAN\.[A-Za-z0-9_-]+` asks "is this a document path?" — case-insensitively it says yes to `plan.switch_to`, a field access.
- `^(licence|license|copying|copyright|notice)` asks "does this project ship a licence?" — it says no to a directory of SPDX-named texts (`LGPL-2.1-or-later.txt`).
- `Test-Path api-ms-win-crt-heap-l1-1-0.dll` asks "does this dependency resolve?" — it says no for an **API set contract**, which is not a file and resolves through the loader's schema.

**Resolution**: match the instrument to the question. Case-sensitivity where the pattern is
case-bearing; **enumerate the directory, never the filename pattern**; ask the loader, not the
filesystem, whether a dependency resolves. Where a semantic question has no cheap structural
proxy, pay for the semantic answer — reading a running process's module list cost one command.

**THE ESCALATION: the wrong answer becomes a false LEGAL claim.** A "the licence file exists"
condition passed for all three of these in the gvsbuild prefix. `pcre2/COPYING` is four lines
saying to read a `LICENCE` file that is not shipped. `cairo/COPYING` is a summary pointing at
two files, neither present. `gettext/COPYING` is GPL-3.0 — the licence of the gettext *tools*,
while the shipped DLL is libintl, LGPL-2.1; staging it would have attached a GPL-3 notice to a
component not under it. That last does not under-attribute, it makes a confident false
statement *about the product*, which a downstream redistributor acts on. The gate now requires
each row to declare a **string that must occur in its licence text**, testing identity rather
than presence. Note the ordering trap: "vendor a licence for every project shipping none" and
"check each vendored file exists" are both reasonable, and neither sees a file that exists, is
named correctly, and says the wrong thing.

**A fourth case that ran through three proxies, the last being a careful inference from
complete evidence that was still wrong.** `share/icons/hicolor` was first reconciled by
**counting** — right, agreed by two seats, and blind to IDENTITY: the SVGs were
GtkSourceView's completion-provider set, not Adwaita artwork. (Two people agreeing on the
measured half is not corroboration of the inferred half; it is why nobody re-examines it.)
With the artwork then identified, the licence was inferred as LGPL-2.1 because the SVGs carry
no per-file header and no licence file sits beside them in the prefix — both true. Upstream's
`data/icons/meson.build` installs the icon subdirectory *only*, so the `data/icons/COPYING`
that governs them (CC-BY-SA-3.0, explicitly not the code licence) is never installed. The
evidence was complete about the installed tree; the question was about the work. **GEP-50**
carries the rule that generalises past licensing — *a derived artefact is a filtered view, so
absence in it is evidence about the filter.* What stays here is why we walked in anyway: we
held the narrow version ("an empty directory is evidence about the packaging") and still
missed the deeper one, because a *populated* directory lacking a COPYING reads as informative
where an empty one reads as suspicious.

**Lesson**: a predicate over a NAME or over EXISTENCE is a proxy, and every proxy has a domain
where it silently inverts. The tell is that it never returns "I don't know" — it returns a
clean answer of the wrong kind. Ask what it would say about the case you have *not* got. —
Severity: High

## 279. `gvsbuild --configuration release` compiles GTK's assertions OUT, so the development box cannot enforce the contracts CI enforces

**Symptom**: a test aborts in CI and cannot be reproduced on the Windows development machine under any condition — single case, full suite, same GTK version number. The abort is a fatal GLib assertion inside GTK itself (`gsk_renderer_dispose: assertion failed: (!priv->is_realized)`, exit `0xC0000409`), so it reads as a platform or timing difference rather than a build difference.

**What was tried**: ruling out the GTK version (identical, 4.22.4, checked against the upstream pin); the application's own code path; teardown ordering across the full suite; and "assertions wholesale compiled out", which was inferred from a one-sided string grep and then **retracted as unsound** — a single-binary grep cannot distinguish "this assertion is present" from "some string sharing those bytes is present".

**Root cause**: GTK's own meson adds `-DG_DISABLE_ASSERT` when `debug=false` and `optimization ∈ {2,3,s}` — i.e. under `--buildtype release`. `gvsbuild`'s **default** is `debug-optimized` (meson `debugoptimized`, assertions live), which is what its published release archives are built with; an explicit `--configuration release` gets a real release buildtype and every `g_assert` in GTK/GSK vanishes at compile time. **Both install to a directory named `release`**, so nothing on disk distinguishes them.

**Resolution**: test against the artefact CI consumes rather than one that resembles it — unpack the published archive to a separate prefix and point the prefix variable at it per-invocation, leaving the working toolchain untouched. Confirm by the paired literals (`!priv->is_realized` and the enclosing function name present in one binary, absent in the other) and by a positive control that *aborts* where it previously exited 0.

**Lesson**: **a version number is not a build.** Where a toolchain offers a configuration that removes runtime checking, "green on the development machine" is systematically weaker evidence than it reads for every assertion-backed contract in the dependency — and the weakness is invisible from that machine, because the check that would reveal it is the check that was compiled out. Ask what your build *enforces*, not what it *is*.

**Where Scribobulate implements the fix**: the Windows seat's standing practice, recorded in `packaging/windows/README.md`; `.github/workflows/pipeline.yml` pins the published archive by release tag.

**Scope**: MEASURED on GTK 4.22.4 / gvsbuild 2026.8.0 / MSVC / Windows 10 Pro 19045, both binaries on one machine; the meson logic is SOURCED, not measured across versions. **Ubuntu's distro GTK compiles assertions IN** (`libgtk-4-1` 4.6.9+ds-0ubuntu0.22.04.2 carries both literals). That is single-binary and so sound only as a **positive** — a PRESENT is real evidence, since the stringified expression plus `G_STRFUNC` is only produced by an expanded `g_assert`, while an ABSENT elsewhere would need the two-binary pairing first. So the Linux seat structurally could have caught this class and the Windows development box structurally could not, which is why the same suite being green on three machines was only ever proving this about one of them. MSYS2 and Homebrew remain unanswered.

**See**: routed to the `gtk4-rs` skill — any GTK project with a locally built Windows toolchain has this exposure. Sibling there is GTK4Rs/AP-160 ("a green suite is evidence only about the environments it has run in"); this is the sharper child, not environments but **build configurations behind an identical version number**, and where GTK4Rs/AP-160's gap is visible from the machine you stand on, this one is definitionally not.

**Cost**: a CI failure irreproducible locally, resolved only by diffing two binaries; plus a retracted intermediate conclusion. The standing consequence is larger than the one bug — every `g_assert`-backed GTK contract was unenforced on the machine the project is developed on. — Severity: High

---

## 280. Provisioning for a machine you cannot inspect — installing a tool the image already had, and discovering one path component while pinning its sibling
**Symptom**: Two consecutive packaging failures on a hosted runner, both in provisioning, neither reproducible on any development machine.
**Scribobulate**: `packaging/windows/package.ps1` — the installer-compiler lookup takes the first match (its comment records the doubled-path invocation), and the redistributable directory is selected by content rather than by name; the workflow probes before installing and announces which branch it took.
**See**: general-engineering-principles (GEP-55).

## 281. A corpus that exercises the PATTERN cannot see a bug in the FLAG on the call site that consumes it
**Symptom**: A gate's self-test stays green through a mutation battery aimed at the defect, and adding plainly discriminating cases does not change it.
**Scribobulate**: `xtask/src/lint/` — case sensitivity at the MATCH SITE, not in the pattern, pinned by a corpus case in `xtask/src/lint/corpus.rs` that routes through the same call the check itself makes. Found while the gate was two hand-synced shell ports; the ports are retired, the trap is not — a corpus that feeds a pattern directly still cannot see a flag on the call site that consumes it.
**See**: general-engineering-principles (GEP-3).

## 282. An operation counter is a complexity oracle only for operations you control

**Symptom**: a wall-clock growth-ratio guard flakes on shared CI. The obvious repair — count operations instead of time, machine-independent by construction — was built, and the resulting guard could not fail.

**Root cause**: two, and the second is the general one. The test corpus made the counted structure trivially small, so a linear and a logarithmic lookup performed identically. More fundamentally, the regression being guarded did its work **inside the standard library** (`str::find` scanning the source), which no counter in the project can reach: a reintroduction ticks the counter exactly as often as correct code, leaving the ratio linear while the run takes minutes.

**Resolution**: keep the **absolute** ceiling, which has wide headroom, has never flaked, and is what actually catches the regression; remove the wall-clock **ratio**, whose signal and noise envelope overlap; and address the flake by raising the SAMPLE COUNT on noisy machines rather than the threshold, since the estimator is the minimum of N draws and additive noise only needs more draws to find the same floor.

**Lesson**: "count operations, not time" is right only where the operations that scale are **yours**. Before replacing a timing oracle with a counting one, ask where the work in the regression actually happens — if it is behind an API you do not instrument, the counter measures call frequency, which was never in question. A guard that cannot fail is worse than the flaky one it replaced.

**Cost**: an implementation, a mutation test, and a revert — cheap, and only because the mutation was run. Recorded because the reasoning is persuasive enough to be re-attempted. — Severity: Low

---

## 283. A false premise can file a measurable fact as unmeasurable, and the two protect each other
**Symptom**: A document states that something "remains unmeasured" — a claim that was never true — and it survives review because the sentence explaining *why* it cannot be measured sits a few lines above it and is itself false.
**Scribobulate**: `packaging/windows/README.md`'s long-paths section, where both coupled sentences are corrected together and each says the other must be edited with it, and the matching claim in `.github/workflows/pipeline.yml`.
**See**: general-engineering-principles (GEP-56).

## 284. A dialog raised over a natively full-screen macOS window seizes a Space of its own and leaves the parent black

**Symptom**: with a window in macOS native full screen, opening About or the unsaved-changes Save/Discard/Cancel prompt makes the **dialog** go full screen into a Space of its own instead of floating over its parent; dismissing it returns to an unpainted black parent, and Escape never reaches the dialog. Reproduces **only from inside the `.app` bundle**, never from a bare `cargo run` binary — so the cheapest way to test it is the way that cannot see it.

**Root cause**: GDK attaches a transient child via `-[NSWindow addChildWindow:ordered:]` from inside `gtk_window_realize`, and AppKit runs a genuine enter-full-screen transition on the *child* when the parent already occupies a full-screen Space. GDK tags every toplevel `NSWindowCollectionBehaviorFullScreenPrimary` unconditionally.

**Resolution**: hold `transient-for` aside until the window has realized, tag the `NSWindow` `FullScreenAuxiliary` from the `realize` handler, then hand the parent back. Armed for every toplevel at startup rather than per dialog, so no future dialog site can forget it.

**Lesson**: a window-system relationship established by the toolkit *at realize time* can mean something different when the parent is in a platform mode the toolkit does not model. Where a backend tags every toplevel with one collection behaviour unconditionally, "transient" is not the whole contract.

**Where Scribobulate implements the fix**: `src/platform/mac/fullscreen.rs`. Guards: `the_transient_parent_is_withheld_until_the_window_has_realized` (mutation-checked in both directions, asserting both halves as **states** rather than as schedulings), the `is_secondary`/auxiliary unit tests, and `tests/MANUAL-TEST.md` §7.19m — which must be run **from the bundle**, since a bare binary cannot reproduce it (TDD 7.19).

**See**: gtk4-rs skill → widgets-and-composites. Measured and written up by the macOS seat.

**Cost**: a user-visible defect on every modal over a full-screen window, invisible to the development path most likely to be used. — Severity: High

---

## 285. Merged into ScrAP-245 — a drive tool's zero exit is a claim about the tool, never about delivery

**Symptom**: `cliclick kp:esc` exits 0, posts an event the application never receives, while clicks and typed text from the same tool land — a working build reads as broken.

**Merged, not deleted.** This is the macOS instance of ScrAP-245's root cause (input channels diverging silently with no tool reporting an error), so it lives there as a second measured case rather than as a numbered essay beside it — one root cause, one entry. The number is retired and kept as a landing spot; it is never reused.

**Scribobulate**: `tests/MANUAL-TEST.md` §A.2's drive loop, cited from §7.19m.

**See**: ScrAP-245, and gtk4-rs skill → ui-testing-debugging (GTK4Rs/AP-245), which took the same fold decision independently.

---

## 286. A reconciliation agreed in the room and never written into the artefact is not a decision — it reopens on the next read, and the seat that measured it pays twice
**Symptom**: A question settled days ago comes back phrased as though it had never been asked, because the artefact still carries the superseded claim that the conversation corrected.
**Scribobulate**: `sdd/PLAN.build-pipeline-ci.md` — the hicolor row now states the 15/17 reconciliation inline, the `BuilderBlocks.ttf` row is marked open with its measurement rather than carrying only a verdict, and "The design, agreed and not yet written" now names the owning seat and says why leaving it unsaid stalled it.
**See**: general-engineering-principles (GEP-48).

## 287. A scope claim is only as wide as the thing it was measured over — "which platform bundles the runtime" answered "which platform owes attribution"
**Symptom**: An obligation recorded as affecting one platform — with a measurement backing it — is in fact live on all three.
**Scribobulate**: `packaging/linux/payload.sh` stages `THIRD-PARTY-LICENSES.md` for all three Linux routes (rpm marks it `%license`; the deb `copyright` gained a `Files: usr/bin/*` stanza naming the grammars' licences); `sdd/PLAN.build-pipeline-ci.md` now separates the two obligations; `tests/MANUAL-TEST.md` §A.1 item 5 asserts all six payload files.
**See**: general-engineering-principles (GEP-49).

## 288. `$PSScriptRoot` is EMPTY while parameter defaults are evaluated under `powershell -File`, and correct everywhere else
**Symptom**: A script's `param()` default resolves against the drive root under one invocation form and correctly under every other, with nothing at the use site to show it. The error names a path that still LOOKS like a path (`the item 'R:\..\..\target\release\...' is outside the base 'R:'`), so it reads as a broken checkout rather than as an unbound variable — which is what makes it expensive rather than merely obscure.
**The precondition is a PAIR, not `-File` alone** — measured with a two-line probe, and worth knowing because it says exactly which scripts are exposed instead of leaving every `-File` call site suspect: `[CmdletBinding()]` **with** `powershell -File` breaks; `[CmdletBinding()]` without `-File` is fine; `-File` without `[CmdletBinding()]` is fine; `&` or a dotted path is fine either way.
**Scribobulate**: fixed in `packaging/windows/stage.ps1` — its `$OutDir`/`$RepoRoot` defaults moved out of `param()` into the body, where `$PSScriptRoot` is populated, as `package.ps1` and `verify-licenses.ps1` already did. It was the one exposed script and nothing in-tree started it that way, so the blast radius was zero and stayed zero: this was found by a seat reading the file, not by a failure, which is the only way a dormant defect of this shape is ever found.
**See**: general-engineering-principles (GEP-51).


---

## 289. An HTTP 200 is a claim about the transaction, not about the document — four fetched licence texts were anti-bot pages
**Symptom**: four of the first nine upstream licence fetches returned HTTP 200 carrying an anti-bot interstitial, all four byte-identical at 4626 bytes, with the client reporting success for every one — while the two requests that failed loudly (406, 404) were the harmless ones.
**Scribobulate**: the Windows licence gate's fourth condition — every row of `packaging/windows/licenses.psd1` declares an `Expect` string that must occur in its licence text, asserted when the text is fetched and re-checked at build time by `packaging/windows/verify-licenses.ps1`. `packaging/windows/licenses/PROVENANCE.md` records the pinned versions and SHA-256s, and `packaging/windows/licenses/.gitattributes` (`* -text`) keeps those hashes true on a CRLF checkout — without it a fresh clone on an `autocrlf` seat rewrites the texts and every recorded hash silently describes bytes that are no longer on disk (measured: 219,581-byte blob vs 223,875 on disk, the delta exactly the line count).
**Case — the guard's SCOPE did not follow what it guards, and a plausible wrong explanation nearly closed over the gap.** That `.gitattributes` covers `packaging/windows/licenses/` only. `LICENSE` and `THIRD-PARTY-LICENSES.md` were staged from the repo root by a *later* commit and were still `text: auto`, so the Windows installer shipped them CRLF while Linux and macOS shipped LF: measured 205,287 B installed against a 201,166 B blob, delta 4,121 = exactly the line count. (`THIRD-PARTY-LICENSES.md` has since stopped being versioned at all — `build.rs` generates it and normalises to LF — which removes its variance by construction rather than by guard. `LICENSE` is still versioned and still varies.) **Harmless here** (no SHA-256 is recorded for these two, and CRLF is arguably right for a file a Windows user opens in Notepad) — but it is only harmless by luck, since the guard was written *because* silent rewriting falsifies recorded bytes, and nothing extends it to files added afterwards. Two rules follow: **a byte count for a text file is only reproducible if it states its line-ending convention**, and when two seats report different sizes for one file, **check the line count against the delta before accepting any causal story.** The first explanation offered here was that a pending edit accounted for the difference — plausible, wrong, and self-ratcheting, because the file was about to grow and the story would have survived by being adjusted rather than falsified.
**Case — the anchor discriminates DOCUMENTS, not REVISIONS of one, and that is one notch coarser than it looks.** The FTL text vendored first was SPDX's re-wrap, not FreeType 2.14.3's own file: same licence, different edition — 5,979 B against 6,743 B, paragraphs unwrapped, section rules stripped, stale `http://www.freetype.org` URL — sitting beside a provenance line asserting 2.14.3. The gate passed it and **would pass it again**, because the declared anchor occurs in both. The earlier catches here were the wrong *document*; this was the right document in the wrong *revision*. **Deliberately not escalated to a hash anchor**: that fails every row on any upstream whitespace change and teaches re-baselining rather than reading, which converts a check that catches real substitutions into a ritual. The limit is documented instead. What caught it was not a check but going to the build tree for a neighbouring file — so **prefer vendoring from the source tree the artefact was built from over a tagged download**: a tag asserts upstream published those bytes under that name; the build tree *is* the bytes the shipped binary was built from.
**See**: general-engineering-principles (GEP-52).

## 290. A custom widget that caches child positions derived from child sizes, and re-derives them on nothing
**Scribobulate**: `widgets/tab/bar.rs`'s `with_entry_width_change` funnel (both width-changing setters fused to a retarget), `widgets/tab/ops.rs`'s `handle_width_changed` + `add_tab`'s re-derive-and-settle backstop, `widgets/tab/layout.rs`'s pure `target_positions`/`any_unsettled`; guarded by three per-mechanism `#[g…
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-290), which holds the measurement, the accumulating-displacement mechanism, the repaint-bug misdiagnosis signature and the GTK4Rs/AP-254 two-suffici…

## 291. Every `GtkAdjustment` write is clamped, so revealing something added in the same turn scrolls short by exactly its own width
**Scribobulate**: `widgets/tab/ops.rs`'s `scroll_into_view` republishes the range (`scrollpos::reconfigure` over `layout::scroll_extent(content_extent(), viewport)`) before writing a position, with `content_extent()` the single definition `size_allocate` also reads; guarded by `switching_to_a_just_added_tab_scrolls_i…
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-291), which holds the measurement and reframes that module's clamp family — lazy validation is the family's trigger, not the clamp's preco…

## 292. A `GFile` built from an `https://` URI resolves only where a GVfs backend claims the scheme
**Scribobulate**: `imagefetch.rs` owns the fetch (an explicit HTTP GET, bounded by connect/global timeouts and `limits::MAX_REMOTE_IMAGE_BYTES`) and `renderer::start::load_remote_texture` decodes it with `GdkTexture::from_bytes`, logging fetch and decode failures separately at `warn`; the transport is replaced for **…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-292), which holds the measurement, the daemon-not-library mechanism, and the general lesson about a toolkit API whose capability is supplied by a separ…

## 293. Sizing a drawn affordance from one font's row height and fitting it to a container laid out in another
**Scribobulate**: `affordance::copy_button_rect` yields instead of refusing — it keeps the derived size, collapses the vertical inset to centre the button in whatever the card has, shrinks to the container only as a last resort, and floors at one text row.
**See**: gtk4-rs skill → textview-layout-and-drawing, as a rider on GTK4Rs/AP-145, which recommends the very call that produces the wrong number ("the view's own CSS-zoomed font") and is correct where it stand…

## 294. Letting a coverage ratchet be satisfied by widening the exclusion instead of testing the code
**Symptom**: a change whose only new logic was display-free and fully unit-tested still failed build-pipeline step 6. The feature's GTK wiring had landed in `preview/interactions.rs` — in scope, and 0% covered like every preview wiring file — while its decidable half sat in `codeview/`, which the scope regex exc…
**Scribobulate**: the pure geometry and the shared point-in-rectangle hit test moved out of the excluded `codeview/` tree into `affordance.rs`, where the gate counts them and their tests; `FLOOR` rose 77.72 → 77.75 in the same change, with the reason recorded beside it.
**See**: project-specific (process/tooling; the routing rule keeps these here). POLICY § Build pipeline step 6 for the rule, `scripts/coverage.sh` for the floor, the scope and the per-module rationale.

## 295. A PID-qualified AppleScript process reference decays to name resolution once stored
**Routed**: folded into GEP-17 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: no code — a harness rule for `tests/MANUAL-TEST.md` §A.2. Re-derive the PID-qualified reference **inside every `tell` block**; never bind it to a variable and reuse it.

## 296. A derived screen coordinate is only as trustworthy as the derivation behind it
**Routed**: folded into GEP-61 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `tests/MANUAL-TEST.md` §1 and §A.3 — derive the coordinate rather than estimating it, **and give the derivation its own sanity check**: grow along a column the content cannot interrupt, and confirm the derived rectangle against the image before any assertion rests on it.

## 297. Finalizing a cancelled `GFileMonitor` after the main context has dispatched corrupts the process heap (Windows)
**Scribobulate**: Both cancel sites (`window::rename`'s `cancel_monitor`, `app::open`'s `attach_file_backing`) release the last reference in one uninterrupted stretch with no main-loop turn between cancel and drop, which is the measured-clean row.
**See**: gtk4-rs skill → app-lifecycle-and-env (glib/gio are in the skill's scope; routed for weaving, stub to follow once a number is allocated there).

## 298. A TIGHT list item's content arrives as bare inline events with no `Tag::Paragraph` wrapper
**Symptom**: an exported document breaks its lines after almost every token inside a numbered or bulleted list — `POLICY.md`, then a line break, then the next four words, then a break, then a comma on its own line. Only *inside* list items; the same prose at top level is fine.
**Root cause**: pulldown-cmark wraps a **loose** list item's content in `Tag::Paragraph` and a **tight** item's in nothing at all — the inline events arrive directly inside `Tag::Item`. A consumer that reaches for "no inline container is open, so start a paragraph" therefore starts a *new* paragraph for every inline event the item contains: one for the text run, one for the inline code, one for the link, one for…
**Scribobulate**: `src/export/walk.rs` carries an implicit-paragraph frame — `Open::ImplicitParagraph`, opened lazily by `Builder::push_inline` the first time an inline arrives with an empty inline stack, and closed by `Builder::flush_implicit` into the enclosing block frame.
**See**: project-specific; the fix and its rationale live in code comments at `src/export/walk.rs` (`Open::ImplicitParagraph`, `Builder::flush_implicit`, `is_block_start`).

## 299. A suite-ordering defect that is deterministic on one platform and invisible in the canonical platform's full suite
**Routed**: folded into GEP-54 (+GEP-4, GEP-16) — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/saferizer/file_monitor.rs` — the `changed`-adapter test enters its own `MainContext` before attaching. **The honest limit of that guard**: it pins the *fix*, not the *class*. Nothing in the tree asserts that no other test borrows the default context, and no claim is made that anything does.

## 300. A driven UI step that misses its target does not fail — it acts somewhere else, and a loop that does nothing produces a perfectly stable measurement
**Routed**: GEP-61 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: none — harness discipline, with no implementation in this tree. (Stated rather than omitted: an absent field and a dropped one look identical.)

## 301. A save chooser returns a foreign extension unchanged, so an export default derived from the document's filename overwrites the reader's source
**Scribobulate**: `export::default_export_name` (`src/export/mod.rs`) is the sole producer, with the guarantee pinned by unit test on every platform (TDD 25.10/25.11).
**See**: gtk4-rs skill → printing-and-export (GTK4Rs/AP-298).

## 302. Both of `GtkPrintOperation`'s completion signals misreport, in both directions
**Scribobulate**: `export::pdf::finish(result, drawn, expected)` — a pure function inside the coverage gate, so it is settled by unit test on every platform rather than by a driven run (TDD 25.20).
**See**: gtk4-rs skill → printing-and-export (GTK4Rs/AP-294).

## 303. On the preview route `render_page` ends the page — do not also call `show_page`
**Scribobulate**: not on the shipped path — the PDF sink uses `run(Export)`, and this was measured while evaluating the preview route as an alternative. Retained because "we already draw this, just draw it somewhere else" is the first idea everyone has.
**See**: gtk4-rs skill → printing-and-export (GTK4Rs/AP-296).

## 304. cairo's Windows colour-glyph path wraps colour glyphs in a Type3 `d0` font, and one 2017 extractor mishandles it
**Routed**: folded into GEP-63/GEP-49; toolkit half in gtk4-rs — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: accepted as an out-of-scope limit by operator ruling, on a hands-on Acrobat test (TDD 25.19).

## 305. A `#[gtk::test]` body and a plain `#[test]` calling `gtk::init()` cannot share a binary
**Routed**: folded into GEP-54 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: not hit on the shipped tree — this project writes `#[gtktest::test]` everywhere and `cargo xtask lint-references` check 5 enforces it. Measured while evaluating whether the export path was reachable from a test.

## 306. A chooser's `set_current_folder` is best-effort and its failure is unobservable
**Symptom**: on Windows `set_current_folder` returns `Ok(())` and `current_folder()` reads back `None` **whether or not** the folder was honoured.
**Root cause**: the setter cannot report what the native dialog did with it, and the getter is not the check it looks like — it is **non-discriminating**, returning the same answer in both cases. Worse than an API with no check at all, because one appears to exist.
**Resolution**: nothing downstream may assume the dialog opened where it was asked. Treat the initial folder as a courtesy, never as state.
**Scribobulate**: `window::export::choose_destination` sets it through a discarded `let _ =` with the reason in a comment beside the call.
**See**: gtk4-rs skill → printing-and-export (GTK4Rs/AP-299).

## 307. macOS embeds a colour emoji as a bare Image XObject, so its text is absent by construction
**Routed**: folded into GEP-63/GEP-49; toolkit half in gtk4-rs — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: accepted as the stricter of the two platform limits (TDD 25.18b), asserted as measured behaviour to catch a *change*, not as an aspiration.

## 308. A font's Unicode flag does not predict whether its text extracts
**Routed**: GEP-52 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: TDD 25.18's method note forbids gating on font metadata, and the §25 checks assert the round-trip per line.

## 309. A cancelled export destroys the destination, and the wreckage is a valid file
**Scribobulate**: `atomic_io::AtomicPublish` holds the create-private-temp → write → publish sequence; `export_pdf` stages into it and promotes only on `Ok(Apply)` with `drawn == expected` (TDD 25.21).
**See**: gtk4-rs skill.

## 310. An extraction failure is not evidence about appearance
**Routed**: GEP-60 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: TDD 25.11a makes the pairing a rubric rather than a habit.

## 311. Which of two same-key bindings wins is a property of the BACKEND, not of the toolkit
**Symptom**: a fix, a TDD rubric and a module doc all state as settled fact that `GtkSourceView`'s `move-words` class keybinding beats a window `GAction` accelerator declared on the same keystroke while the view holds focus. On Quartz that is measured and true.
**Root cause**: **not established.** It wants the per-backend shortcut-controller phase; nobody has source-traced it. The Windows seat's reading is that this is GTK4Rs/AP-121's shape (a window accelerator at capture/global beating a focused widget's bubble-phase class binding) — **INFERRED**, and it does not explain why Quartz differs.
**Resolution**: **state the winner of a keybinding contest together with the backend it was measured on.** A sentence like *"the class binding wins that contest"* reads as a toolkit property and will be inherited as one; the next seat then either ports a fix nobody needs or skips one somebody does.
**Scribobulate**: TDD §4.13 and §23.6 and `macwordnav`'s module doc now scope the claim to Quartz and record the two contrary legs. `macwordnav` itself is unaffected and stays macOS-only — it pre-empts a binding that only wins there.
**See**: gtk4-rs skill → GTK4Rs/AP-121 (the shape this is INFERRED to belong to); routed to that skill's maintainer with the discrimination method and the control traps.

## 312. Repairing the editor buffer when the defect is in the source string — the preview never reads the buffer
**Symptom**: a document whose lines are separated by a bare `\r` renders in the preview as **one enormous heading** containing the whole file, and in the outline as one entry — while the **editor pane beside it shows the lines correctly** and the footer reports the right `Ln`/`Col`.
**Root cause**: GTK and pulldown-cmark disagree totally about whether a bare `\r` is a line ending. MEASURED on both platforms — Quartz/4.22.4 and X11/4.6.9: the buffer reports `line_count = 6` and highlights the heading, while the byte-identical string parses to one `Start(Heading(H1))` swallowing the document.
**Resolution**: repair at the **ingress doors**, never at a parse site. (a) `docio`'s readers, beside `without_bom`, at **all three** — the save guard and crash recovery each compare on-disk content against an in-memory baseline, so repairing one side of a comparison turns every save into a spurious "changed on dis…
**Lesson**: , five, and the first four are the same shape — *check the thing that PRODUCES the thing in front of you*. (1) **When two views of a document disagree, fix the input they SHARE**, not the one whose output is wrong; a green suite over a fix sited one layer too late is the expected outcome.
**Scribobulate**: `src/lineendings.rs` owns the rule and the display-free repair; `docio`'s three readers and `window::actions::load_into_editor` apply it, and that half is landed and sound.

## 313. `GtkTextBufferContent` refs the buffer and never unrefs it — a whole document leaks per select-then-deselect
**Scribobulate**: none — GTK-internal, unavoidable from application code; there is no PRIMARY provider in this tree any more (removed in `4b97c84`).
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-318); the refcount table and the negative tracker search live there.

## 314. Instantiating ANY `sourceview::Buffer` subclass corrupts the heap — and the backtrace is innocent
**Symptom**: `cargo test --features gtk-integration-tests --lib` SIGSEGVs in `g_slice_alloc`, reached from `g_main_context_dispatch` on `gtk4::test_synced`'s thread. The `--test gtk_suite` main-thread harness passes. The same tests pass when run **alone**, at any thread count. Only a FULL `--lib` run crashes.
**Lesson**: **when a crash lands in an allocator, the bug is somewhere you have already been.** Stop reading the stack and start deleting variables — and prefer an arm that makes the suspect *exist but do nothing* (E) over one that removes it entirely (A), because only the first distinguishes "this code is wron…
**Scribobulate**: it forecloses the mechanism that would have made ScrAP-312's clipboard repair impossible-to-break, so that repair ships as a marked fence instead — the two conditions it depends on are named at `lineendings::wire_paste_normalization`.

## 315. Laying a table out with tab characters — a tab ladder cannot express a column
**Symptom**: an exported PDF's tables look ragged and border-less, the same column starting at a different x on almost every row. All the text is present and correctly ordered, so it reads as a styling omission rather than a missing feature.
**Root cause**: `export/pdf.rs`'s `table()` joined each row's cells with `\t` and emitted one ordinary Pango paragraph per row. A tab advances to the next stop in a **fixed ladder** (24pt here), so a cell one character too wide shoves its neighbour a whole stop right.
**Resolution**: a measured column grid — `export/pdftable.rs`, display-free and unit-tested, decides widths from per-column max-content and min-content measurements; `pdf.rs` measures and inks.
**Lesson**: ScrAP-75 records that a hard TAB inside a GFM table breaks table recognition, and the fix normalises tabs away on the way IN. The export sink then chose tabs as its column mechanism on the way OUT.
**Scribobulate**: `src/export/pdftable.rs` (99.5% covered) + `export/pdf.rs`'s table path. Guards: `export::pdftable::tests`, `export::pdf::pdf_layout_tests`' table set — chiefly `every_row_of_a_table_shares_one_column_grid` — and `the_shared_rule_agrees_with_the_previews_own_fit_columns`, which runs identical inputs…

## 316. A repair handler on `insert-text` is also a handler on the UNDO machinery, and the divergence it causes is silent
**Symptom**: an undo puts back **different bytes than were deleted**, and nothing anywhere says so — no warning, no critical, no failing assertion. The history's own model still believes the original bytes were restored, so a later redo compounds it rather than exposing it.
**Root cause**: `gtk_text_buffer_history_insert` reaches the buffer through the **public** `gtk_text_buffer_insert`, so a handler that rewrites inserted text is on the replay path as much as on the paste path — it was never opted out of.
**Resolution**: Do not bracket: the no-lone-CR invariant outranks byte-exact undo of a sequence no buffer may legally hold, and the two only ever differ on a buffer that already violates the invariant.
**Lesson**: This was filed as not-currently-reachable on the strength of two lines appearing in the right order inside one function, with a comment saying the order was load-bearing.
**Scribobulate**: `src/lineendings.rs` — `new_editor_buffer` (the choke point, the only route to an armed buffer) and the private `wire_paste_normalization`; `window::tabs::lifecycle`'s `build_tab_editor` is its single production caller.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-303), landed as skill commit `8fd296b` and read back from the installed copy on this host rather than taken on report; attribution there is split th…

## 317. A counter that stops being able to SEE its subject reports the intervention as a success
**Routed**: GEP-68 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `probes/native-chooser-rss.m` grew `--track-dealloc` (a `DeallocSpy` associated object) precisely to catch this, and `--track-dealloc` against `--reap` is the reproduction.

## 318. Silencing a whole log DOMAIN to arm a gate also silences the defects that domain is the only signal for
**Scribobulate**: `scripts/pipeline.steps` arms `G_DEBUG=fatal-criticals` on the **Linux and Windows** step-5 commands — not Linux alone, which is what this line said when it was written and what the contract had already outgrown at that commit — and declares its absence on macOS explicitly with `disarm.macos integra…
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-305); sibling trap ScrAP-268 (a custom log writer silently disarms promoted fatality).

## 319. A portability gate whose verdict depends on which `grep` is first on PATH — and the seat that should catch the bug is the seat that hides it
**Routed**: folded into GEP-57 (+GEP-1) — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: the check is now two POSIX-ERE stages behind `win_illegal_path` (character rule case-sensitive with the control range built by the shell; reserved device names as their own `grep -i` pass, since ERE has no inline flag) plus the `--self-test` corpus it shipped without — 18 cases, mirrored string-for-…

## 320. The unresolvable pointer — a reference whose target the reader cannot dereference, mistaken for a delivery
**Routed**: folded into GEP-23 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: five incidents in one session, across two agents. The load-bearing one for generality is **this seat's**, not the researcher's: `docs/` is gitignored (`.gitignore:16`), and both platform seats were dispatched their QA work by being pointed at `docs/code-review.md`, which reaches neither clone.

## 321. The spurious kill — a mutation run that scores its own breakage as detection, and certifies coverage that does not exist
**Routed**: folded into GEP-11 (+GEP-2, GEP-3, GEP-64) — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: three instances, one seat, one session, on Windows PowerShell 5.1. (1) A check-12 predicate harness wrote mutants to a scratch directory while the script resolves its scan set from `$PSScriptRoot`, so every mutant died on a missing `.scan` and five of six clauses reported "killed" — the BASELINE die…

## 322. A control is a property of a CLAIM, not of a probe — and one control makes every other claim feel covered
**Routed**: folded into GEP-12 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `probes/textview-primary-overwrite.c` carries all five modes and the retraction in its own header, so the corrected reading is where the wrong one was.

## 323. `Clipboard::formats()` answers a question about the PROCESS, not about the content
**Scribobulate**: assert `clipboard.content()`'s provider formats, never the clipboard's — `clipboard::a_preview_selection_still_publishes_gtks_default_rich_content_to_primary` does, and carries the reasoning at the assertion (mutation-checked: publishing plain text over the same selection makes it report `gchararray…
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-306), placed beside GTK4Rs/AP-285, which is the same deserializer union on the DROP side.

## 324. A compiled-in asset resolved against a runtime directory is absent everywhere that directory isn't
**Symptom**: a shipped theme's sprite renders as its flat fallback on a fresh install, a developer build run with no user config, and a macOS bundle — no warning, no crash, green suite throughout. The reference was never invalid; it was simply never resolved.
**Root cause**: two ways a resource can be *named* — compiled into the binary, or read from disk — but only one way it was *resolved*: against a themes-file's own directory, a step that only the disk-file case ever ran.
**See**: kin to ScrAP-317/319/320/321 (this register's own family of checks/mechanisms that cannot go red for the right reason) — here the mechanism that went silently right was a *resolution step*, not a test…

## 325. A whole-struct `{:?}` in a completeness digest degenerates the guard into a restatement of what the producer already guarantees
**Routed**: folded into GEP-10 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/theme/tests/sinks.rs` — `decoration_digest` enumerates the metrics the preview's paint pass scales and nothing else, with the measurement above stated in the function's own comment so the next agent does not "simplify" it back; everything typographic is proven through the tag digest beside it,…

## 326. A `const`-evaluated constructor scores ZERO in llvm-cov, so code exercised at every build reads as dead
**Routed**: folded into GEP-4 (+GEP-7) — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `src/theme/keys.rs`'s registry is the const-evaluated table; its runtime-construction test asserts the registry's shape as well as restoring the measurement, and `scripts/coverage.sh`'s FLOOR was re-armed upward (81.45) rather than relaxed. Commit `4b17fde`.

## 327. `TextTag::property_value("*-rgba")` formats a POINTER under `Debug`, not the colour
**Scribobulate**: `src/theme/tests/sinks.rs`'s tag digest reads colours typed as `property::<Option<gdk::RGBA>>` and formats the RGBA; the `Debug` spelling is confined to the non-colour properties beside it, where it is faithful.
**See**: gtk4-rs skill → ui-testing-verification (GTK4Rs/AP-310). Woven at mint time — the canonical text is skill-side and was never committed in full here.

## 328. A gdk-pixbuf dimension probe must `set_size(0, 0)`; any non-zero size still allocates the bomb
**Scribobulate**: `src/sprite.rs`'s decoded-pixel cap probes with `set_size(0, 0)` in its `size-prepared` handler and carries the measured RSS figures and the failing spelling in its rustdoc, because the correct and incorrect forms are indistinguishable from the gate's own verdict.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-311, beside GTK4Rs/AP-66's loader-chain entry). Woven at mint time — the canonical text is skill-side and was never committed in full here.

## 329. A gate read through a pipe reports the pipe's last stage, not the gate
**Symptom**: a coverage ratchet was set from a measurement, re-run to confirm, and reported green twice — while actually failing. Two independent mechanisms had to line up, and both fail in the green direction.
**Root cause**: The gate was invoked as `scripts/coverage.sh | tail -2; echo $?`. In a POSIX shell `$?` after a pipeline is the exit status of its LAST command, so the `0` printed was `tail`'s, and `tail` succeeds whatever the gate decided. The output looked right because `tail` faithfully showed the gate's own summary lines; only the VERDICT was substituted.
**Resolution**: invoke a gate directly and read its own exit status; where a pipeline is genuinely wanted, `set -o pipefail` first. Read a coverage floor from the column the gate reads, never the column the summary leads with.
**BOUNDARY, measured later and worth reading before generalising this**: "read its own exit status" assumes the tool is HONEST about it, and some are not. `codesign --force --deep --sign -` printed `bundle format unrecognized, invalid, or unsuitable`, produced no `_CodeSignature` at all, and RETURNED ZERO — so a guard written as `if ! codesign …`, authored precisely to stop a broken signature shipping, waved it through. **The boundary is narrower than "this tool is dishonest", and the narrow form is the useful one: it is the ACTING verb that lies, not the tool.** `codesign --sign` returns 0 having done nothing; `codesign --verify --deep --strict` exits non-zero correctly. So the repair does not require distrusting exit codes generally, or finding a different tool — it is to follow the acting verb with the SAME tool's verifying verb, and to assert the artefact exists. Measured while establishing this: reading `$?` after piping `--verify` to `head` reported 0 when verify had in fact failed, so this entry's own original lesson bit during the investigation of its boundary. **This case BREAKS the resolution above rather than illustrating it**, which is the reason the boundary is written here at all: in every other instance the tool is honest about the layer it reported on and the fix is to read the right layer — but here the layer that failed IS the layer that returned 0, so there is no correct status to read. The general rule is GEP-52; what is local is that this entry and ScrAP-123 both read as "the exit status is authoritative", and it is not universally.
**Scribobulate**: `scripts/coverage.sh`'s header carries the column warning and its instances, and the floor is set from the Lines column and verified by running the script directly. **The value is deliberately not repeated here** — POLICY step 6 makes the script its only home, and the copy that used to sit in this line had already gone stale.
**See**: project tooling — cargo-llvm-cov and shell invocation. Kin — ScrAP-321's family (a green that means nothing), and ScrAP-326 beside it, which is the other way a coverage number misleads: 326 is a real…

## 330. A seam that exists is not a seam that is called
**Routed**: GEP-67 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `CssSafeFontStack::pango_family` (`src/theme/value.rs`) is the sole projection, and the PDF sink's layout specs carry that type rather than `String`. The guard asserts the face Pango *resolved* in the artefact, and separately asserts its own fixture came back quoted so it cannot rot into the weaker…

## 331. A vocabulary rename that reaches a selector
**Routed**: folded into GEP-10 (+GEP-40) — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: the table-cell link selectors (`src/preview/css.rs`) name GTK's own `link` class on `GtkLinkButton`, not this project's `link_color` key; the constant carries a warning saying so, because the two are one blanket rename apart.

## 332. Re-styling a background view in place, and trusting a headless run to prove it
**Scribobulate**: `app::setup::re_render_all_windows` re-renders only the **active** tab in place; every other preview-visible tab has its preview released and `needs_render` set, then `start_deferred_prerender_pump` warms them.
**See**: gtk4-rs skill.

## 333. A repeating pattern anchored to a viewport-CLAMPED extent is pinned to the screen, not the document
**Scribobulate**: `widgets::tile_texture` anchors the grid at `(rect.x(), 0.0)` for every caller (`codeview::quotes::draw_accent_bar`, `codeview::bands::draw`, `widgets::rule`) instead of taking a per-site `TileOrigin` whose two answers were both wrong and whose own docs asserted the opposite of what each did; guarde…
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-315).

## 334. A repaint failure that also happens in an unrelated application is upstream, and the platform seam it invites is the wrong response
**Symptom**: on a KDE/X11 desktop, toggling the system dark↔light theme leaves parts of the application drawn in the previous scheme until something forces a repaint.
**Scribobulate**: NOT fixed, and deliberately not investigated further (operator, 2026-08-28). The remedy it invites is a new `src/platform/linux/` portal seam to observe the desktop's appearance signal directly, which is a real module with a real maintenance cost, built to work around somebody else's bug.
**See**: kin to the `Upstream` scope rule in `sdd/ISSUES.md`'s header, which exists for the same reason — an upstream defect is not work waiting to be scheduled here.

## 335. A generated stylesheet GTK refuses loads silently, and the guard that could see it was scoped to one property
**Scribobulate**: the whole declaration is spliced after the preceding semicolon, never into a value (`preview::css::theme_css`). Every builtin theme's sheet is now loaded into a real `CssProvider` with `connect_parsing_error` armed first, and any error fails the test naming the theme and quoting GTK's own words (`pr…
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-316); kin — ScrAP-331 (the same stylesheet, the same blind text assertions, the other failure axis), ScrAP-132.

## 336. A `GtkPaned` child dragged to nothing still reports its full natural height, so `height()` cannot answer "was it crushed away"
**Scribobulate**: `window::gtk_integration_tests::the_divider_cannot_crush_a_sidebar_section_away` asserts the paned's `position()` against the sidebar's own height, at both extremes and for both sections (TDD 20.21).
**See**: gtk4-rs skill.

## 337. A precondition implicit in a whole script is reported by whichever line violates it first, at that line's layer, after everything before it has run
**Routed**: GEP-69 — the lesson lives in the `general-engineering-principles` skill; essay in git history (f725e67).
**Scribobulate**: `install.sh` at the repo root is a `uname -s` router holding no install logic, dispatching to `packaging/linux/install.sh` and `packaging/macos/install.sh`; each body ALSO guards itself, because it stays directly runnable and a direct run must not be the lenient path. Found by the macOS seat: the pre-router script spent a full `cargo build --release` and then died on `install -Dm755` with exit 71 and a message naming a path, because BSD install has no `-D`.
