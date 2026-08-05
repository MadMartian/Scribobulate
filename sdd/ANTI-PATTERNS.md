# Anti-Patterns

Lessons from building Scribobulate's native GTK4/Rust rendering stack. This file is now a compact **project index**: per entry — *symptom* · *where Scribobulate implements the fix* · *pointer into the `gtk4-rs` skill* (+ findings doc). The full transferable lesson, dead ends, and GTK source tracing live in the **`gtk4-rs` skill** — the standing GTK4/Rust anti-pattern knowledge base this project was originally built alongside and which is highly recommended (though not required) when working here. It is referred to **by name only, never by filesystem path**: the skill may not be installed on every machine this repository lives on, so a path would rot. AGENTS.md carries what it is and where to find it; the original self-contained essays remain in this file's **git history**.

**Citation convention.** Two anti-pattern registers are in play and their numbering spaces are unrelated. An entry in **this** file is cited **`ScrAP-N`**; an entry in the `gtk4-rs` skill is cited **`GTK4Rs/AP-N`**. **A bare `AP-N` is illegal anywhere in the tree** — not "it means the skill", illegal — and `lint-references` **check 8** fails on one. Both legal forms are deliberately SINGLE TOKENS with no space: a two-word form is split by any Markdown or `rustfmt` wrap, and two such citations were already broken that way when the gate was written. `ScrAP-` and `GTK4Rs/` are both unique, so a citation resolves to the same register whether it appears in a code comment, in `sdd/`, or inside this file. A bare `#N` inside **this file's own body** is the local shorthand for an entry here (never a skill entry); write `ScrAP-N` in full everywhere outside this file. When a lesson is held by BOTH registers, cite `ScrAP-N`: this register is always resolvable, whereas the skill may not be installed on the machine — which is also why a `GTK4Rs/AP-N` is checked for FORM only and its correctness rests on a human audit of a greppable list (#231). The retired forms — a bare `AP-N` meaning a project entry, `ANTI-PATTERNS #N`, and the two-word `skill AP-N` — are gone from the tree as of 2026-08-01, this time measured rather than asserted (#231 records why the first sweep's identical claim was false).

> **Non-core (delineation rule):** lessons belonging to a separate library or a process/tooling
> concern — **Pango**, **GtkSourceView**, **pulldown-cmark/CommonMark**, **librsvg**,
> **testing/CI process**, **GLib/XDG caching** — are NOT folded into the core skill: **ScrAP-4**
> (Pango), **ScrAP-27** (GtkSourceView), **#36** (GtkSourceView; #36's core-GTK half went to the
> skill as GTK4Rs/AP-46), **#51**, **#58** (GtkSourceView), **#66**, **#73**, **#75**, **#78**, **#86**,
> **#93**, **#97** (pulldown-cmark/CommonMark/copymap), **#115** (Pango markup manipulation),
> **#122** (pulldown-cmark), **#123** (tooling/process), **#128** (GLib/XDG dir caching + app
> lifecycle), **#130** (librsvg/docs tooling), **#147** (pulldown-cmark/CommonMark block-HTML
> event stream), **#160** (syntect default-syntax coverage), **#163** (Pango markup escaping), **#164** (testing/CI + cross-platform filesystem),
> **#194** (CommonMark/Markdown transform logic + enforcement discipline),
> **#195** (pulldown-cmark + this crate's own inline scanner),
> **#196** (this crate's renderer/annotation offset mapping), **#206**/**#207** (reference-gate tooling + cross-platform gate parity), **#208** (Rust proc-macro/test-harness design), **#209** (testing discipline), **#214** (macOS/dyld dynamic-loader semantics + Rust/libc), **#215** (testing discipline), **#216** (register/citation tooling), **#217** (experiment design), **#218** (multi-agent process), **#219** (testing/tooling discipline), **#220** (testing discipline), **#221** (testing discipline), **#222** (documentation/tooling governance), **#223** (review/claim discipline), **#224** (version control / project governance), **#225** (input-cost policy / threat modelling), **#226** (testing/gate discipline), **#228** (invariant-placement discipline), **#229** (filesystem permissions / cross-platform security model), **#230** (Rust tooling / enforcement discipline), **#233** (serde/`toml` crate + file-format design), **#234** (testing discipline), **#236** (verification tooling / Windows harness), **#237** (testing/CI process + cross-platform build discipline), **#239** (verification tooling / Cargo + git build-artefact semantics), **#240** (reference-gate tooling + review discipline), **#241** (crash-recovery design limitation / OS process identity), **#242** (tooling/process), **#245** (verification tooling / X11 input injection), **#251** (verification tooling / distribution build configuration), **#252** (verification tooling / test-harness design), **#243** (GLib/GIO task-pool internals — not a GTK widget contract, though it binds any gtk-rs app that moves I/O off the main thread) — and, following the same precedent as #36,
> **#124**'s core-GTK half went to the skill as **GTK4Rs/AP-98**; its tooling/process remainder
> (which pipeline step compiles a gated suite) stays project-specific. Their stubs stay a
> little fuller (root cause + citations inline), full essay in git history. Edited only by
> the Scribobulate maintainer agent.

| #   | Anti-pattern |
|-----|-------------|
| 1   | Rendering a document viewer with a GPU-compositing UI stack |
| 2   | Assuming "disable hardware acceleration" makes a web engine render on the CPU |
| 3   | Using environment variables to prevent GTK from crashing on a large XCompose file |
| 4   | Using Pango `<a href>` markup in GtkLabel for standalone link widgets |
| 5   | Reading GtkTextBuffer text to track trailing newlines when child anchors are present |
| 6   | Using a horizontal rule to indicate a blockquote |
| 7   | Placing the blockquote `DrawingArea` in an outer overlay outside the `ScrolledWindow` |
| 8   | Redirecting `XDG_CONFIG_HOME` without carrying `mimeapps.list` |
| 9   | Duplicating action logic across context menu, main menu, and keyboard shortcut |
| 10  | Walking the widget tree to re-discover anchor-embedded GtkLabel widgets |
| 11  | Expecting `g_menu_item_set_icon()` to render icons in a GTK4 menu bar |
| 12  | Using untyped GLib qdata as the canonical store for per-window state |
| 13  | Acting on a GTK adjustment's `changed` signal before `page_size > 0` |
| 14  | Restoring `GtkTextView` scroll via adjustment manipulation after `set_buffer` |
| 15  | Using `iter_at_location` to get the iter at the top of a `GtkTextView` viewport |
| 16  | Mirroring split-pane scroll synchronously inside `value-changed` |
| 17  | Parsing a "new instance" CLI switch after the GApplication has registered |
| 18  | Bridging glib↔Rust `log` with the wrong handler (stack overflow / dropped Gtk-CRITICAL) |
| 19  | Trying to override a `GtkDropDown`'s empty-state "(None)" caption |
| 20  | Gating a widget-scoped action on raw per-widget focus |
| 21  | Padding a `GtkTextView` block background with `paragraph-background` + margins |
| 22  | Forcing `GtkTextView` layout validation inside the snapshot/draw or size-allocate path |
| 23  | Embedding height-for-width block content as a widget at a `GtkTextChildAnchor` |
| 23a | Bounding an anchored child to the content column while it sits at an INDENTED (list/blockquote) margin → over-wide by the indent → spurious Automatic h-scrollbar → ScrAP-22/ScrAP-23 churn |
| 24  | Relying on the system theme to paint a `GtkTreeExpander`'s disclosure chevron |
| 25  | Assuming the `.heading` / `.title-N` typographic CSS classes require libadwaita |
| 26  | Pointing a `GtkPopover` at an anchor rect outside the visible viewport |
| 27  | Searching find-next from the caret after `select_range` (re-finds the current match) |
| 28  | Tracking a selectable `GtkLabel`'s selection via a `notify::cursor-position` |
| 29  | Re-laying-out an anchored child via `queue_resize` after `parent_size_allocate` in the same `size_allocate` |
| 30  | Dismissing/unparenting a popover from inside a descendant's click handler |
| 31  | Resolving an untrusted document's local image `src` against the CWD (or with a lexical-only containment check) |
| 32  | Anchoring a `GtkPicture` in a `GtkTextView` without a nonzero width request |
| 33  | Testing a rebuilt single-instance GApplication while a stale primary is still running |
| 34  | Remote image loading blocks the main thread; "refused" ≠ "unresolvable" in a multi-outcome resolution enum |
| 35  | Reading `st.source` for a programmatic preview re-render in split mode |
| 36  | Letting the editor `GtkSourceSearchContext` `notify::occurrences-count` overwrite the preview buffer's `forward_search` count in preview mode |
| 37  | `GtkTextView` never repaints an anchored child when a descendant's Pango background is REMOVED |
| 38  | Driving derived UI state from a delta-only signal, missing lifecycle boundary events |
| 39  | Specifying GNOME-specific icon names absent from non-GNOME themes |
| 40  | `GtkAboutDialog` `authors` entries with `<url>` format open `mailto:` |
| 41  | `FileChooserNative`/transient-dialog lifetime is backend- and widget-type-dependent |
| 42  | Predictable, reused path under the shared temp dir for a config-redirect workaround (security) |
| 43  | Relying on `GtkNotebook`'s `create-window` signal for "drag a tab to the desktop to spawn a new window" on Wayland |
| 44  | Using a `<Shift>` + digit/punctuation `GtkApplication` accelerator |
| 45  | A `GtkNotebook` with `show-tabs` false cannot be a cross-window tab-drag drop target |
| 46  | An idempotent signal-rewire check keyed only on widget identity misses a stale closure after a cross-window reparent |
| 47  | `gtk_window_present()` from a D-Bus `open` handler doesn't raise+focus a tokenless (bare-terminal) launch — legitimate WM behavior, not a bug |
| 48  | Adding an ancestor `GtkEventControllerKey` for Escape doesn't catch Escape while a `GtkSearchEntry` descendant has focus — nor while focus is inside a **popover**, whose key events are bounded at the GtkNative just as its pointer events are (and the source predicts otherwise) → put a popover's Escape on the popover |
| 49  | `cargo valgrind` on a GTK4 app reports hundreds of "leak" and uninitialised-value errors that are toolkit-internal, not application bugs |
| 50  | `GtkNotebook`'s native cross-window tab-detach DnD is unsafe on GTK 4.6.9 — a NULL deref inside GTK's own `dnd_finished_cb`, not (only) a freed source notebook |
| 51  | A `GtkSourceSearchContext` `occurrences-count` handler that strong-captures its own context is a permanent self-reference leak |
| 52  | Swapping in a brand-new `GtkScrolledWindow`/`GtkAdjustment` on external auto-reload without re-wiring the scroll-spy signal bound to the old one |
| 53  | Holding a `RefCell` `Ref` alive across a GTK setter that synchronously re-enters and borrows the same cell aborts the process |
| 54  | `write_atomic`'s crash-safe write-temp-then-rename makes GIO's `GFileMonitor` report every save as a deletion |
| 55  | Caching "which slot is active" as a raw `Vec` index across a drag-reorder that moves entries within the same `Vec` |
| 56  | Toggling a sibling widget's visibility from inside a container's own `size_allocate` |
| 57  | A signal handler that captures its host `ApplicationWindow` by weak ref silently stops working after the widget subtree is re-homed to another window (split scroll-sync) |
| 58  | Reparenting a reused `GtkSourceView` across view-mode containers re-fires its gutter's never-unbound `vadjustment` binding → a use-after-free |
| 59  | Mounting a scrolling pane in a `GtkBox` without `vexpand` collapses it to its natural height (a lazily-validating `GtkTextView` then paints only ~2 lines) |
| 60  | A closure owned by a widget's own machinery that strong-captures self (or an ancestor) is an uncollectable cycle — on window close it strands the entire descendant subtree |
| 61  | Building N `GtkMenuButton` menu-models in a synchronous startup burst forces N×items accelerator-label font resolutions → a multi-second UI freeze |
| 62  | A custom tab/stack widget leaves its active-index model unset for the default-visible first page |
| 63  | A shared app-level menubar model can't carry per-window content — a per-window submenu needs a self-built `GtkPopoverMenuBar` + selection-as-action-state + deferred `GMenu` mutation |
| 64  | A process-global CSS provider with an unscoped selector collides across windows (last-loaded wins) |
| 65  | Preserving a `GtkTextView` reading position across a re-render drifts, and can wedge input dead |
| 66  | Relying on pulldown-cmark's native superscript/subscript for tight `E=mc^2^` / `H~2~O` |
| 67  | Screenshotting an open GTK4 `GtkPopoverMenu` under kwin/X11 to verify a menu |
| 68  | Deferring a `set_visible(false)`→`measure()` read as if it were the GtkTextView lazy-validation family |
| 69  | Putting mnemonic `_` markers in a command label shared across menu + tooltip + context-menu surfaces |
| 70  | Getting bare-letter access keys (with a visible underline) in a plain `GtkPopover` via mnemonics / use-underline |
| 71  | Nesting a submenu as a child `GtkPopover` inside a plain autohide `GtkPopover` context menu |
| 72  | Gating/targeting a multi-pane action on the first-found view instead of the focused pane |
| 73  | Reconstructing character-precise copied Markdown from sparse parser waypoints, and mis-reading pulldown-cmark offset semantics |
| 74  | Aligning char offsets with `GtkTextBuffer::get_text()` — it omits anchored children |
| 75  | A hard tab in a GFM table breaks table recognition; normalise tabs — but length-preservingly |
| 76  | A paragraph-attribute `GtkTextTag` applied as one continuous range over a multi-paragraph region drops the attribute on toggle-free middle lines |
| 77  | UI-testing a formatter over the selectable read-only Preview pane |
| 78  | `Options::all()` (or any enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of degrading to literal text |
| 79  | A container-level `GtkGestureClick` also fires on presses that land on a child `GtkButton` — a bar-wide "activate" gesture activates a tab even when the press was on its × close button |
| 80  | Tracking a "reading line" only from a wheel `EventControllerScroll` misses scrollbar-drag and keyboard scrolling — the re-anchor goes stale |
| 81  | Persisting "all windows" from each window's `close-request` + a sequential-close quit loses every window but the last |
| 82  | A one-shot `scroll_to_mark` restoring a FAR reading position onto a freshly-rebuilt GtkTextView lands near the top |
| 83  | `GtkShortcutsWindow`'s programmatic `add_section`/`add_group`/`add_shortcut` API is GTK 4.14+ — on 4.6 you must build it from Builder XML |
| 84  | `GtkTreeListModel` `autoexpand=true` makes true recursive Collapse all impossible; build `autoexpand=false` + explicit expand pass. Collapse DESTROYS the subtree (it does not cache expanded flags) |
| 85  | A bundled (gresource) `*-symbolic` icon is only a fallback — a host theme that ships the same name overrides it |
| 86  | Probing a broader Markdown marker before a narrower one that embeds it mis-parses the input — test narrowest-first |
| 87  | A `#[gtk::test]` that maps + pumps to full allocation BEFORE calling the code under test validates line heights first, masking an unvalidated-heights bug |
| 88  | Bounding a blocking `MainContext::iteration(true)` pump loop with a between-iterations wall-clock check instead of a timeout SOURCE — it can hang forever on an idle display |
| 89  | Gating a programmatic `GtkSingleSelection` change with a transient "we're setting it" bool — it re-emits `selected-item` after the setter returns, escaping the bool |
| 90  | A `GtkPopover` attached with `set_parent()` is NOT auto-unparented — the parent widget's `dispose()` must unparent it, or teardown floods "GtkPopover is not a child of …" |
| 91  | An always-on scrollbar with default (overlay) scrolling floats over the `GtkTextView`'s right margin, stealing clicks meant for margin-drawn affordances |
| 92  | A mutation path that edits the buffer but leans on a MODE-GATED live-preview refresh leaves the preview stale |
| 93  | Anchoring positions by pulldown-cmark source offset against ALL events maps onto a block-structure event whose range spans the whole block |
| 94  | A signal handler connected to a `GtkTextView`'s BUFFER is silently dropped when `set_buffer` swaps the buffer — re-wire buffer-dependent handlers on the new buffer |
| 95  | A shown `GtkPopover` does not grow its surface when its child grows — pre-size it (homogeneous `GtkStack`), don't re-present it |
| 96  | Committing an action that rebuilds the widget subtree synchronously inside a `GtkButton` `clicked` handler breaks active-state accounting |
| 97  | Inferring "inline vs block" from non-empty source delimiter bytes engulfs whole paragraphs |
| 98  | A `GtkPopover` hosting a typing entry is unwinnable on X11 (autohide steals focus via its seat grab; non-autohide can drop clicks) — host a typing entry as an in-surface `GtkOverlay` child instead |
| 99  | A translucent text-tag highlight is painted over by a later opaque-background tag — GTK text-tag backgrounds don't composite; the highest-priority tag wins |
| 100 | Measuring a widget while it is `visible=false` returns 0 — center an overlay child off a hidden measure and it collapses to a left-edge anchor |
| 101 | UI-test tooling: kwin-on-Xvfb won't deliver a synthetic `xdotool` click to a non-autohide `GtkPopover` surface — verify such flows via a keyboard-triggerable action, not a synthetic popover click |
| 102 | Positioning a widget via `set_margin_*` then re-measuring it double-counts the margin — GTK folds a widget's own margins into `preferred_size()`/`measure()` |
| 103 | Refreshing a `GtkTextView` via `set_buffer` for a change that leaves the rendered text identical repaints the whole document and jumps the scroll |
| 104 | A persisted `GtkTextMark` re-resolved after a `set_buffer` swap is a cross-buffer footgun that aborts with `gtk_text_btree_line_number couldn't find line` |
| 105 | `iter_location` (any line-DISPLAY-caching geometry read) right after a `set_buffer` swap, before re-allocation, aborts with `gtk_text_btree_line_number couldn't find line` |
| 106 | A selectable `GtkLabel` in a popover auto-selects all its text on open — the popover focuses it, and a selectable label selects-all on focus-in |
| 107 | A menu-activated action that synchronously raises a focus-grabbing in-surface widget has its focus stolen by the menu popover's pop-down focus-restore — defer the raise to idle |
| 108 | `GtkTextBuffer::redo()`/`undo()` leaves no undo barrier — the next edit merges into the redone action's group, so one later Undo reverts two edits |
| 109 | Mapping GtkTextView buffer coords ↔ an anchored-child cell's interior under incremental allocation |
| 110 | Driving selection-dependent UI for a selectable-`GtkLabel` cell (a selection island) — buffer signals never fire; use the primary clipboard, wired on the live view |
| 111 | The in-place buffer-tag refresh can't repaint an anchored-child cell decoration — reconcile the cell labels in place, unconditionally |
| 112 | `GDK_IS_SURFACE` criticals are a stale TOOLTIP timer over an unrealized grabbing popover — reuse popovers, don't destroy per use |
| 113 | The first popup of a view-parented popover forces a one-shot table revalidation that scrolls the view and drops the click — pre-warm it |
| 114 | An in-place live-buffer edit that skips the canonical source-of-truth vanishes on the next fresh render |
| 115 | Highlighting a char range in an existing Pango-markup string via `find` wraps the wrong (first) occurrence |
| 116 | Activating a nested-submenu item in a `GtkPopoverMenuBar` leaves a sibling top-level menu popped open — the bar clears its open menu through only one channel (a top-level popover's `unmap`) |
| 117 | Clearing a `GtkLabel` `set_attributes` overlay in place needs a transient markup-STRING change to repaint — a same-string `set_markup` is a no-op |
| 118 | A list-item hanging indent (`left-margin` + negative first-line `indent`) is unreliable across paragraphs; the durable fix is to DROP the hanging indent (draw the marker in a gutter, uniform margin) |
| 119 | A `GtkPaned` with the default narrow handle silently swallows presses in a strip at a child pane's edge |
| 120 | `WidgetExt::color()` is gated behind the gtk-rs `v4_10` feature — the compile error never mentions it |
| 121 | Two `GtkTextTag`s that both set `left-margin` on a line (a list item inside a blockquote) do not compose |
| 122 | Translating a stripped-then-parsed document's ranges back to original coordinates instead of per-position translation silently swallows the stripped bytes (the range-merge gotcha) |
| 123 | A coverage ratchet's floor recorded as stale prose drifts from the real (climbing) figure, silently loosening the gate |
| 124 | A test suite gated behind a Cargo feature the build pipeline never enables rots invisibly until it doesn't compile |
| 125 | Scheduling work that depends on paint-populated state via `idle_add_local_once` reads the previous frame's state and silently no-ops |
| 126 | Styling a `GtkTextView`'s background via `textview { background-color }` alone works on Default but is defeated by the user's system theme |
| 127 | Reaching for CSS selector specificity to arbitrate between two `GtkCssProvider`s is a category error |
| 128 | `g_get_user_config_dir()`'s process-global lazy cache makes a mid-startup `XDG_CONFIG_HOME` redirect and an honest config-dir read mutually exclusive |
| 129 | `g_app_info_launch_default_for_uri(uri, NULL, …)` silently emits no activation token, so a WM's focus-stealing prevention refuses to raise the handler |
| 130 | A hand-authored SVG that renders fine in Inkscape can be invalid XML that librsvg (and GTK) rejects outright |
| 131 | A refactor that REDEFINES what an existing field means keeps compiling at every call site, and silently changes behaviour |
| 132 | A source-scanning guard test whose scope filter is wrong scans nothing and passes forever |
| 133 | A hard-coded Xvfb display lets one crashed run orphan a server that silently serves stale windows to every run after it |
| 134 | Bounding a wait on a FRAME COUNT when the thing waited on is measured in WALL-CLOCK |
| 135 | `GtkText` writes PRIMARY on every selection change — and a widget claiming PRIMARY CLEARS the previous owner's selection |
| 136 | Seeding live UI state from the persisted-session snapshot |
| 137 | A window `GAction` accelerator BEATS a focused `GtkText`'s own keybinding — and *disabling* the action is what hands the key back |
| 138 | Polling a `GtkEntry`'s own `has_focus()` in a test spins forever — focus lands on its internal `GtkText` delegate |
| 139 | A `GtkText`/`GtkEntry` selects ALL its text on focus-in, silently undoing a caret set BEFORE `grab_focus` — and the hazard IS guardable headlessly, if the toplevel is MAPPED |
| 140 | A security gate answering a DIFFERENT question than the one being asked |
| 141 | A "this will misbehave" theory read from a construction site, never executed |
| 142 | A capture-phase ancestor gesture cannot pre-empt a child's gesture and hand it back cleanly — "one similar event will be emulated" preserves event COHERENCE, not gesture STATE |
| 143 | A PERMANENT register entry citing an EPHEMERAL artifact (an ISSUES entry, a PLAN file) |
| 144 | `unparent()` on an OPEN GtkPopover does not emit `closed` — it skips the close path entirely |
| 145 | Two registers numbering their entries with the SAME prefix — every cross-citation is wrong-but-plausible |
| 146 | Assuming `GdkTexture::from_file` ignores installed gdk-pixbuf loaders and adding a manual `Pixbuf` fallback (it already falls back to `gdk_pixbuf_new_from_stream`; a format failing with the loader installed is a cache-REGISTRATION gap, and `Pixbuf::from_file` is *less* capable) |
| 147 | Rendering raw-HTML `<picture>`/`<img>`: block HTML is per-line in `Tag::HtmlBlock`, a single-line `<picture>` is inline events, and grouping must be `<picture>`-scoped across events |
| 148 | Splicing at an offset mapped OUT of a delimiter-stripped coordinate space — safe to read, not safe to insert at: it can land inside a construct invisible in the stripped space |
| 149 | Two overlapping async scroll-drivers over ONE `GtkAdjustment`, neither cancelled when a newer navigation starts — a stop-condition keyed on a shared slot being EMPTY misses a new op that REPLACES it → a monotonic generation token |
| 150 | A self-drawn decoration that ADDS its own padding on top of the tag-supplied `pixels_above/below_lines` already inside a line's `line_yrange` — double-counting that also bleeds the rect onto the next line when no blank separator absorbs it |
| 151 | Detecting a URL scheme with `split_once(':')` — the text before the first colon *anywhere* — misclassifies every local path that contains a colon (Windows drive letters, colons in filenames) as a URL and refuses it |
| 152 | A deferred `idle_add_local_once` closure that **strong-captures a widget** pins it alive as an *unrooted zombie* past `window.destroy()` and fires against it (SIGSEGV) — and the reflexive guards each miss: `WeakRef::upgrade()` is liveness-only (Some for the zombie), `dispose` never runs under the strong cycle, and glib's `SourceId::remove()` *panics* on an already-fired id |
| 153 | A `#[gtk::test]` integration suite renders on GTK's **default GskGLRenderer** (not whatever `main()` selects — the test bodies bypass `main`), and under a headless Xvfb display the GL texture-cache SIGABRTs at teardown on a zero-size-surface→NULL-texture; force the app's shipped Cairo renderer for the harness via `.cargo/config.toml [env]`, not per-test |
| 154 | Migrating a hand-rolled `downgrade()`→closure→`upgrade()` weak capture to `glib::clone!(#[weak] …)` is NOT a blind find/replace — `clone!` hoists ONE upgrade-or-fallback to the closure top, so it silently changes behaviour where work runs *outside* the guard, where the gone-path return diverges per-branch or per-capture, or where the target is a non-GObject wrapper that lacks glib's `Downgrade` |
| 155 | A per-render widget whose `Rc` dismiss closure strong-captures its own container, while controllers *on that container* hold the `Rc` — an uncollectable widget↔controller↔`Rc` cycle that strands the whole subtree every rebuild (unbounded reload leak); plus how to name a GTK-internal allocator leak with NO debug symbols |
| 156 | Reading a `GtkTextView` selection's widget-y for a popover anchor from inside a WALL-CLOCK debounce (`timeout_add`) after a scroll — the read lands before the frame clock's `validate_onscreen`, so `buffer_to_window_coords` returns a stale (off-viewport) y and an on-viewport guard suppresses the popover; `iter_location` does NOT force the validation → move the read + guard onto a bounded, stability-checked `add_tick_callback` |
| 157 | Collapsing a large `GtkTreeListModel` (Collapse-all) while the `GtkListView` is SCROLLED TO THE BOTTOM strands a stale far-end leaf row painted with no expander — GtkListView 4.6 picks a scroll-stability anchor from the scroll position at `items-changed` time, and when the collapse destroys that deep anchor row it never recovers → re-anchor the ListView to the top (reset the vadjustment) BEFORE collapsing |
| 158 | A content-less list item (`- `, `1. `, `- [ ]`) still emits a full `Item` (and `TaskListMarker`), so an unconditional per-item gutter marker draws a stray bullet/number/checkbox for an empty item → gate the marker on the item having produced buffer content |
| 159 | Centering a gutter marker (bullet/number/checkbox) on `GtkTextView::line_yrange`'s height silently centers it over ALL of a soft-wrapped item's display rows — so a wrapped item's marker floats to the MIDDLE row instead of staying top-aligned on the first (worse at higher zoom, which grows rows and provokes the wrap) → `line_yrange` is the whole LOGICAL line; clamp its height to one display row (`min(h, gap + single_line_h)`, `single_line_h` from a cache-free Pango layout in the view's own zoomed font) before centering |
| 161 | A CSS `margin-*` that appears to do nothing (or a `margin: 0` that still won't reach its container's edge) because the widget ALSO carries a code-set `gtk_widget_set_margin_*` on the same axis — widget margins and CSS margins are INDEPENDENT and CUMULATIVE, so the stylesheet can only ADD to the code's inset, never reduce it → pick one supplier per axis and delete the other |
| 160 | A ` ```typescript `/`tsx`/`toml` fenced code block renders as a flat, single-colour ("all-gray") block while ` ```js`/`rust`/`python` highlight fine — syntect's bundled default syntax set (`SyntaxSet::load_defaults_newlines()`) ships NO TypeScript/TSX/TOML grammar, so `find_syntax_by_token` returns `None` and the emitter's SILENT `find_syntax_plain_text()` fallback gives every line one scope → one colour → load `two_face::syntax::extra_newlines()` (fancy-regex feature, not the onig default) instead |
| 162 | A `GtkTextView`'s reading position drifts toward the top of the document under repeated horizontal window resize — a width change re-wraps the text and lazy line-height re-validation transiently underestimates the vadjustment `upper`, so `value ≤ upper − page_size` clamps `value` toward 0; the drift is CUMULATIVE across a drag's many size-allocate passes and appears only when NARROWING (more wrapping → taller doc), and unlike a buffer swap a bare geometry resize has NO re-anchor hook (reload/zoom/theme/view-mode all capture-and-restore; resize doesn't) → track the user's settled top buffer LINE and re-anchor via the deferred/coalesced `scroll_to_mark` path on a RAW-width change only (never a one-shot `set_value`, which the clamp re-resets; never `content` width, which zoom's margin change would double-drive) |
| 163 | Switching a `GtkLabel` from `set_label` (plain) to `set_markup` — done here so a tab's "⚠ deleted-backing" badge glyph can be coloured with a `<span foreground>` — silently turns every interpolated string into Pango markup: an un-escaped filename metacharacter (`&`, `<`, `>`) makes GTK reject the string with a `Pango`/`Gtk-WARNING` and render the label EMPTY or wrong, with no crash and no compile error → escape every runtime-supplied fragment with `glib::markup_escape_text` at the ONE funnel that feeds the label, and keep the badge colour a caller parameter so the pure label formula stays display-free/testable |
| 164 | Committing a test fixture whose FILENAME is itself the invalid input it exercises (a colon, a Windows-reserved name) — here `tests/fixtures/report:draft.md`, deliberately colon-named to prove ScrAP-151 — breaks `git clone`/checkout on Windows (NTFS reads `:` as an alternate-data-stream separator) BEFORE any app code runs, and unlike a runtime temp file a tracked file can't be `#[cfg(unix)]`-gated or `.gitattributes`-excluded → exercise the invalid-name logic in CODE (in-memory string literals for pure classification, cross-platform; a platform-gated runtime temp file only where the OS permits the name), never in a committed filename; sweep the class with `git ls-files | grep -nE '[<>:\"\|?*]\|[ .]$'` + the reserved-basename check |
| 165 | Clearing an inherited environment variable with `[Environment]::SetEnvironmentVariable(name, $null)` leaves it **defined-but-empty**, which `cmd.exe` still honours — and `$env:VAR` reads empty either way, so the check that "confirms" the fix cannot distinguish success from failure. Cost two failed GTK builds whose symptom (`'create-lists.bat' is not recognized` -> `U1052: file not found`) reads as a corrupt source tree. |
| 166 | Diagnosing a hung test suite from a **parallel** run: libtest buffers output per thread, so a suite that wedges on the 400th test prints *no test names at all* — which reads as "hangs before the first test" and points every subsequent hypothesis at process startup. Re-running with `--test-threads=1` converts "no output" into a named culprit. |
| 167 | A directory lookup that returns `Option` and legitimately yields `None` for "the user has not configured anything" is **indistinguishable from "this platform is unsupported"** — so a Unix-only XDG/`HOME` chain silently disabled config *and* session persistence for an entire platform port, with no error, no log, and an app that started, rendered, and simply forgot. |
| 168 | Opening **any** popover collapses a natively-maximized window to its pre-maximize size while the OS still reports it maximized — a popup shares the toplevel's frame clock, and GDK runs `compute_size` for *every* surface on that clock, so the toplevel is sized on a layout pass it never requested. With no configured size it falls back to GtkWindow's **remembered** size, which `should_remember_size()` deliberately freezes at the pre-maximize value. The clamp that would hide this only unlocks when the **app** calls `gtk_window_maximize()` — which the OS's own maximize button never does. |
| 169 | A `-symbolic` icon name PRUNED from a newer icon theme doesn't fail loudly — GTK's lookup expands `X-symbolic` into a candidate CHAIN (`…-ltr`, `X-symbolic`, `X-ltr`, **`X`**, `image-missing`), so a leftover legacy RASTER of the base name quietly wins: it renders, but is non-symbolic and stops tracking the theme foreground. `has_icon` tests the EXACT name only, so it is *stricter* than the render path and unreliable in BOTH directions (false alarm where the render succeeds; silent green where it succeeds WRONGLY) → audit by RENDERING and reporting the resolved source file, bundle the pruned names as real symbolic SVGs, and pin the removal version from the upstream changelog rather than inferring a range |
| 170 | Symbolic icon art drawn with STROKES silently changes SHAPE — GTK wraps a symbolic SVG in a generated stylesheet whose first rule is `rect,circle,path { fill: <fg> !important; }`, which outranks an authored `fill="none"`, so a stroked outline FILLS IN (a hollow ring rasterizes as a SOLID DISC through GTK). Confirmed on 4.22.4 AND 4.6.9; the stroke-dropped-unless-`class="foreground-stroke"` behaviour is 4.22+ ONLY (inert on 4.6.9, so not portable) → draw fills-only and verify through the GTK icon pipeline, never a bare `rsvg-convert`/Inkscape preview, which validates the wrong artifact |
| 171 | Every `#[gtk::test]` in the suite aborts on macOS before its body runs ("Attempted to initialize GTK on OSX from non-main thread") — gtk4-rs's `test_synced` dispatches test bodies onto a `glib::ThreadPool` worker, and GTK on macOS requires init on the process main thread; `--test-threads=1` does NOT help, because libtest's serial mode doesn't change which thread the pool uses → the HARNESS is the problem, so drop it: a `[[test]] harness = false` target runs its own `main()` on the main thread and stays an ordinary `cargo test` target on both platforms. Do NOT reach for `examples/` — it also owns `main()`, but `cargo test` never RUNS an example, so the check quietly stops being a gate |
| 172 | A synthesized-click UI-automation tool can itself be silently broken, so a "still broken" result is AMBIGUOUS between *the fix is wrong* and *the test's own input never arrived* — indistinguishable from inside the test, and the reason a real bug read as unfixable across several attempts → before trusting a NEGATIVE from click/keystroke automation (especially before a second or third fix attempt), run a POSITIVE CONTROL: drive an unrelated, definitely-working control with the same tool in the same session and confirm it fires |
| 173 | `GtkWidgetPaintable::current_image()` taken in the SAME main-loop turn as a `set_opacity` (or any other `queue_draw`) on that widget returns an EMPTY paintable, not a dimmed one — `queue_draw` clears the cached `render_node` walking to the root, and "freezing" does NOT rescue a late capture: it faithfully freezes the blank. A drag icon set from it is invisible on every backend while the drag itself keeps working, so nothing errors, no warning prints, and both orderings read as reasonable in review → capture the image BEFORE the state change, and FUSE the two into one call so the order cannot be got wrong at the call site |
| 174 | A cross-platform guarantee whose PORTABILITY lives in a backend, not an API — `GApplication`'s single-instance activation — degrades SILENTLY where the backend is absent: on macOS `g_application_register()` still succeeds, still returns `is_remote() == false`, and every launch simply elects itself primary, which is byte-for-byte what "no other instance is running" looks like. Worse, the platform has a SECOND, independent reuse path (LaunchServices hands a document to a running *bundled* app without exec'ing anything, and GTK/Quartz routes it into `open`), so verifying via Finder / `open -a` returns a confident green light for a mechanism your code never ran → verify a portability claim by proving the BACKEND exists, not by exercising the API; and when a platform offers more than one launch path, treat each as a separate contract with its own test |
| 175 | A defect whose CONSEQUENCE is platform-dependent while the defect itself is NOT — the platform that never triggers it never tests for it, so its green suite is uninformative rather than reassuring (here: shared code left a `set_parent`ed popover on a moved editor; two backends wedged forever, Linux passed because it never disposes that editor, and the stale parenting was then MEASURED present on Linux too) → before accepting a "platform-specific defect" framing, measure the suspected bad STATE on the passing platform; and write the regression guard against that state, never against the triggering platform's symptom (flood/hang/timeout), which is permanently green where the bug actually lives |
| 180 | A `gtk_widget_set_parent`-attached child left parented on a `GtkTextView` at dispose is not a leak or a warning — it is an **unbounded loop**, and the suite that reported green was simply never disposing the widget: dispose drains with `while ((child = gtk_widget_get_first_child(view))) gtk_text_view_remove(view, child);` and `gtk_text_view_remove` **warns and returns without unparenting** any child lacking the `quark_text_view_child` qdata, so `get_first_child` hands back the same widget forever (measured: 274,758,040 warning lines / 12.1 GB of stderr in five minutes). Code identical in 4.6.9 and 4.22.4 and GTK leaves none of its OWN children behind on either, so a stuck child is yours — what differs between a platform that wedges and one that does not is **whether anything ever disposes the view at all**: the suite that passed leaves the widget alive and still parented after the test believes it destroyed it, i.e. **it does not exercise disposal, so it reports green on every bug in this class** → prove teardown HAPPENS (enumerate children/parent after the destroy) before reading a green suite as evidence about it; never hang a `set_parent`'d child's detach on a `destroy` handler reached through a binding you are still holding (`gtk_window_destroy()` emits `destroy` synchronously ONLY when the caller holds no reference of its own — on BOTH versions); and give any GTK runner a wall-clock cap AND a log-volume cap, because this failure mode consumes rather than fails |
| 181 | A test suite that has never RUN on a platform accumulates platform-shaped assertions that read as portable and are not — two measured here the first time the macOS suite executed: `tempfile::tempdir()` returns a `/var/…` path that macOS reaches through the `/private/var` symlink, so every comparison against a path the app had RESOLVED failed (and failed as "the document never opened", pointing at the feature rather than the fixture); and `gtk_accelerator_get_label` renders `<Primary>` as `Ctrl` on X11/Wayland but as the `⌃` glyph on Quartz, which three assertions had pinned as a literal → canonicalise a temp dir ONCE at creation rather than at each comparison (a no-op where nothing is symlinked, so no `#[cfg]`), and pin a platform-rendered string in ONE `#[cfg]`'d constant rather than deriving it from the same call under test, which would agree with itself |
| 182 | A test's **readiness probe** can be strictly stronger than the behaviour it gates, and then it is the probe that fails — `has_focus()` asks whether a widget holds the GLOBAL input focus, which additionally requires the **toplevel to be active**, while the code under test (an action gated on `notify::focus-widget`) only ever consults the window's **focus widget**. Invisible on X11/Xvfb, where a window is active as soon as it maps; NOT guaranteed on macOS, where key-window status is granted at the *application* level. Measured the instant `grab_focus()` returns: `is_focus = true`, `has_focus = false`, `window.is_active = false` — the asserted state is already correct while the probe waits 5 s for an activation that has nothing to do with it → make the readiness probe walk **the same accessor the code under test walks**; a probe that can time out on a condition the assertion does not need is a false failure generator, and its intermittency will read as a platform defect |
| 183 | A mutation test whose mutation trips an EARLIER precondition proves nothing about the guard you were checking — the run goes red, which looks like success, but the assertion under test never executed; verify WHICH assertion fired, not merely that something did |
| 184 | A chain of green checks that each assert a LINK and none assert the OUTCOME — detection verified four ways (polarity, live OS read, the property moving the resolved palette, the end-to-end write) while the window still painted light, because `lookup_color` following a setting does not mean the pixels follow it → for any user-VISIBLE contract, one check must assert the thing the user sees, or the suite is measuring its own plumbing |
| 185 | An idle queued from a NATIVE OS callback (CoreFoundation/CFRunLoop) is not dispatched for many SECONDS on an otherwise-idle GTK app — and because the deferred closure RE-READS the OS setting at dispatch time rather than event time, a there-and-back change is answered with the OPPOSITE value, silently and with no error → read the event's value AT the event; defer the APPLICATION of a captured value, never the observation |
| 187 | A document offset captured when a UI affordance is BUILT and applied when it is CLICKED is not a coordinate, it is a bet on the document not having changed — and the payout is destructive: the surgery lands wherever those bytes now point, deleting unrelated text and leaving the markup half-open, or panicking out of bounds (a panic in a GTK handler ABORTS the process). Two clocks move underneath it, the user's typing and any debounced re-render, so it rots with no interaction at all → carry the RANGE together with THE TEXT THAT OCCUPIED IT and re-resolve at apply time (exact-offset fast path, else nearest occurrence, else refuse); make every mutation primitive TOTAL (`get`, not `[]`) so an unresolvable range is a no-op rather than a splice. Validation alone is not enough — it converts corruption into "the button silently does nothing" |
| 188 | When a behaviour disappears after you removed a mechanism, the inference "the mechanism was providing it" is a GUESS wearing a causal chain's clothes — and it points the fix at *restoring* something that may never have existed. Here, three annotation-card defects appeared right after `autohide` was switched off, and the card no longer following the scrolling document read as obviously grab-related; the 4.6.9 source says `check_autohide` handles **button and touch only, never scroll**, and GTK implements no anchor-tracking anywhere — so the "lost" behaviour was never lost, it was never implemented, and the honest reading is that removing the grab merely let the document scroll *at all* while a card was open → before restoring a behaviour, confirm the mechanism ever provided it; "it worked before" locates a change in TIME, not in CAUSE |
| 189 | GTK's own doc comment can promise a behaviour the code does not implement: `gtk_text_view_add_overlay` says the child "will scroll with the text view" and before GTK 4.19.1 it does not move at all (two independent short-circuits — a scroll only re-allocates when *anchored* children exist, and the child's offset setter ends in `queue_draw` not `queue_allocate`). Worse for a viewer with a marginal `Automatic` h-scrollbar, an overlay child's minimum size FEEDS the view's own minimum (`gtk_text_view_measure` measures `center_child`, which maxes over every overlay), so a wide floating card re-arms the ScrAP-139 → ScrAP-56 churn-blank chain with no opt-out; and the fix landing in 4.19.1 means the *same code behaves differently* on a 4.6 floor and a 4.22 build → prefer a `set_parent` popover, which is in neither child list and so never touches the size request; and treat a documented promise about a widget's own behaviour as a hypothesis to measure, not a contract |
| 190 | An invariant sited on a widget's `show` vfunc has a hole exactly where it is most needed: `set_visible(true)` on an ALREADY-VISIBLE widget is a no-op that never runs the vfunc, so a "recompute before presenting" guarantee silently does not fire on the re-present path — which is the path a reused, re-pointed surface exists for. Every open-from-closed test passes while the case the bug is about goes unguarded → put the recompute on BOTH the vfunc (transitions) and the explicit present method (unconditional), and write the regression test with the widget already visible |
| 191 | A one-shot "pre-warm" that pops a REUSED widget up and straight down to absorb a first-realization cost is harmless only while that widget is stateless; the moment it owns session state, the warm-up's teardown is a teardown of a live session. Ours ran from a deferred first-`map` idle and silently dismissed a card a programmatic navigation had opened in the same turn — "the user cannot have interacted yet" is an assumption about idle ordering, not a fact → a warm-up must no-op when the instance is already in use, and any reasoning of the form "this runs too early to collide" needs the collision checked, not asserted |
| 192 | `GtkPopover::popdown()` is NOT animated on 4.6.9 — it is `gtk_widget_hide()` plus a cascade that early-returns for a non-autohide popover — so `is_visible()` flips synchronously and `popdown()` is exactly `set_visible(false)`. The corollary is the trap: `closed` is emitted from the **`hide` vfunc**, so ANY hide emits it, including a deliberate transient one — a `closed` handler used as an "is it still open?" backstop fires on your own hide and ends the session you were only pausing → keep the session flag distinct from visibility and mark a self-initiated hide with a STICKY flag (a flag scoped tightly around the `set_visible` call assumes synchronous emission, which is an assumption you do not need to make) |
| 193 | Driving/screenshotting a GTK4/Quartz app on macOS from an agent has no established recipe (the skill's whole automated-UI-testing module is Xvfb+xdotool) and every gap reads as an app defect until isolated: the OS's own idle timer locks the screen out from under a purely-synthetic-input session (no real HID activity ever resets it) — mitigate with `caffeinate -disu` for the session's duration, confirm with `CGSSessionScreenIsLocked`; the controlling terminal can silently reclaim frontmost status BETWEEN tool-call turns, so a click lands on the wrong window with no error — reassert `set frontmost of process <app>` in the SAME invocation as every click, not once at the start; GTK's Quartz backend exposes NO widgets to `NSAccessibility` (only native window-chrome — close/minimize/fullscreen), so AX-element clicking is not viable, raw coordinates are the only path; neither `System Events` nor `cliclick` has a scroll-wheel command (synthesize one via JXA `CGEventCreateScrollWheelEvent`+`CGEventPost`) → calibrate every click BEFORE committing to it — move the cursor only (`cliclick m:x,y`) and capture with `screencapture -C` to confirm pixel alignment against a Retina-2x-corrected, window-relative coordinate; nearly every "nothing happened" in this session was a coordinate miss, not a delivery failure |
| 195 | A decision driven off ONE parser's event stream is blind to every construct a SECOND tokeniser owns — and the blindness is silent, because the missing constructs arrive as ordinary `Text`. Here the annotation balancer (which widens a selection so `{==…==}` can never land between a delimiter and its partner) matched pulldown-cmark's `Code`/`Emphasis`/`Strong`/`Link`/`Image` events, while `==highlight==`, `~~strike~~`, `^sup^` and `~sub~` are tokenised **by this crate** (`scan_scripts`, ScrAP-66/ScrAP-75, because pulldown has no highlight option and its flanking rules never match the tight Pandoc forms) → annotating a partial selection inside one spliced `a ==m{==ar==}{>>note<<}k== b`, which parses as neither construct. The match arm *listed* `Tag::Strikethrough`, which reads as coverage and is DEAD (`md_options()` never enables it — measured: the events for `a ~~b~~ c` are one `Text`), so the register of handled constructs looked complete. Sibling paths do NOT share the defect symmetrically: the preview path resolves against the copymap, which models the stripped markers as inline nodes, and was already correct — one contract, two implementations, one of them blind (measure both; do not infer either from the other) → make the second tokeniser SPAN-shaped (`scan_script_spans` returning outer/inner ranges) so the renderer's string form is derived from it, then have every consumer that reasons about construct extent run BOTH passes into one union; and treat a match arm whose event the options can never produce as a defect in its own right, since it is what makes the gap invisible in review |
| 196 | A fallback branch keyed on a SYMPTOM rather than a cause silently absorbs every future cause that presents the same way — and does it conservatively, so nothing complains. The annotation claim-highlight mapper tagged a whole content run whenever the run's buffer length differed from its cleaned-source length, a guard written for genuinely unmappable SYNTHESISED runs (smart punctuation `--`→`–`, entities); a **marker-stripped** run (`a ==mark== b` → `a mark b`, ScrAP-66's in-crate constructs) trips the identical symptom while being mappable to the character, because the scanner knows exactly where the markers were → any annotation ANYWHERE on such a line washed the entire run amber, including one on a plain word nowhere near the construct. Test the CAUSE (can I account for the missing chars? — `kept_chars` via `scan_script_spans`, which also subsumes the 1:1 case, deleting the special case rather than adding one), and keep the fallback for the cause it was written for. Two method lessons from the discovery, both cheap: a wrong-looking pixel found while verifying an UNRELATED fix must be attributed by a DISCRIMINATOR before you believe you caused it (annotating a lone plain character — which no span-balancing touches — proved this predated the change; assuming authorship would have sent me reverting a correct fix), and a defect you *can* fix now does not belong in the debt register, which exists for what you are NOT fixing |
| 194 | A shared per-line helper that hands out a RAW line makes every block transform built on it blind to the line's **container prefix** — Heading/Bulleted/Numbered/Task each prepended their marker in front of a blockquote's `> ` (`### > Heading`, a paragraph starting with `###` that also loses its quote), and the *same* blind spot broke toggle-OFF (a detector requiring the marker at offset 0 never recognises `> ## Title`, so each press compounded `## > ## Title`). One rule, four independent copies to get wrong, and the parser that captures the prefix already existed for Enter-continuation — nothing in the shared layer consulted it → split each line into (container prefix, content) ONCE at the shared seam, transform only the half you own (content for the marker commands, prefix for Quote), re-attach verbatim; then **seal** the raw form (make the span's `text` module-private) so the next transform added cannot reacquire the blind spot — a per-formatter prefix check is the same defect written four more times |
| 197 | A `#[path]`-included module's children resolve against the attribute's directory, not the module's own name — only a `mod.rs`-named file escapes it, so relocating a second crate root away from beside the real one is a trap, not a convenience |
| 198 | `pub use` cannot widen `pub(crate)` visibility for a test façade — E0364, and there is no shortcut around it; a test that needs crate internals must be compiled AS the crate (a `harness = false` root sharing the module tree), never through a thin re-exporting shim |
| 199 | Treating an `insert-text` of `"\n"` as "the user pressed Enter": a same-app paste is a rich-text `insert_range` that emits ONE `insert-text` per tag-delimited run, so a line ending in a highlighted span hands its newline over as a BARE `"\n"` — indistinguishable by payload, and acting on it is UB (the `location` iter is `STATIC_SCOPE`, i.e. `insert_range`'s own cursor) → the paste's tail silently vanished. Decide where the keystroke is (capture-phase key controller), don't guard the signal; `GtkSourceIndenter` would be the sanctioned home but its gtk-rs 0.10 trampoline frees the caller's iter (SIGSEGV on the first Enter) |
| 200 | `GtkSourceIndenter` (the sanctioned keystroke-only home for auto-indent) is unusable from gtk-rs at sourceview5 0.10 — the subclass trampoline takes the caller's transfer-none `GtkTextIter*` with `from_glib_full` and frees GtkSourceView's own iterator on drop → SIGSEGV on the first Enter, **with an empty `indent()` body** (which is the discriminator: an empty vfunc that still crashes indicts the binding, not your logic) |
| 201 | A `harness = false` runner that filters argv by pattern (drop `--*`, treat the rest as name filters) mis-reads an unknown flag's VALUE as a positive filter — `--skip <name>` inverted into "run ONLY that case", so the Windows pipeline's step 5 ran 1 case of 149 and reported success. Parse value-taking flags explicitly; print `running N of M` and treat an unexpected N as a harness failure, because a test-SELECTION bug is always green |
| 202 | Gating a deferred request that rides on a PAINT behind "has the system settled?" removes deliveries without adding any — the settling itself is the no-op (a `set_value` to the value already held queues no draw), so the paint that would have carried the dispatch never comes and the request hangs armed forever; gate on a predicate about the state **already in front of you**, decidable this frame, never on a flag some other loop must set on a future tick |
| 203 | A fatal-signal handler that restores `SIG_DFL` and re-`raise`s does **not** kill the process: entering a handler installed without `SA_NODEFER` blocks that signal for the handler's duration, so the raise only marks it *pending* and control falls through to the next line — the process leaves by a **normal exit whose status is 139**, the very number a shell prints for a segfault, so every symptom still looks like a crash while `WIFSIGNALED` is false and the kernel logs no `segfault at …` line. Unblock with `pthread_sigmask(SIG_UNBLOCK)` before the raise. Only a real signal driven through a forked child catches it; code review and the report contents both read as correct |
| 204 | Resolving a kernel `segfault at … ip <IP> … in <lib>[<VMA>+<size>]` line by looking `IP − VMA` up in `nm` output names the **wrong function**, plausibly and silently: the kernel's `<VMA>` is the start of the mapping holding the IP — the library's *executable* segment — not its ELF load base, so the offset must first be re-based onto that segment's page-aligned `PT_LOAD` vaddr (`readelf -lW`). Then check the *size* of the symbol you land in: past its end means the real frame is a `static` function with no dynamic symbol, and that is as far as naming goes without symbols |
| 205 | Carrying a toolkit behaviour across platforms because the **version numbers match** — two builds of the identical GTK version disagree about whether a *deprecated* property still works, because the behaviour lives in the theme's `gtk.css` that each distributor ships, not in the library. Predicting one platform's rendering from another's at the same version produces a confident, wrong defect report; only a pixel assertion on the platform in question settles it, and only an inverted control proves that assertion can fail |
| 206 | A reference gate whose pattern requires a `.md` extension the tree's citations never write — code cites a plan by SECTION (`PLAN.<topic> D3`), which names the document, not the file — so the gate exited 0 with all checks PASS over **21** live danglers, including in the commit that declared the sweep complete. A PASS means "no match", which is indistinguishable from "cannot match" |
| 207 | Two ports of one gate sharing a pattern but not a file ENUMERATION: the shell side swept `find … .` (repo-wide, incl. gitignored generated trees) and the PowerShell side seven named dirs, so `.agents/`, `docs/` and `THIRD-PARTY-LICENSES.md` failed one gate and passed the other — while POLICY asserted "neither can drift into being the lenient one". A gate is its pattern **and** its input set; where two platforms cannot test each other, ship a `--list-scan` comparison artifact, not a promise |
| 208 | A proc macro that splits one annotated item into several and passes the original's outer attributes through with the body tokens puts `#[ignore]` on a plain helper fn, where it is inert — the real libtest item gets none, so a quarantined test runs anyway under both harnesses, silently. Route each attribute to the item that reaches its interpreter (compiler / harness / body) |
| 209 | A cleanup-path guard test whose setup makes the resource impossible to create passes with the fix deleted — a read-only parent fails `create_new`, so "no temp file left behind" was true because none ever existed. Provoke the failure AFTER the resource exists (rename onto a directory), and mutation-test error-path guards specifically |
| 210 | Windows PowerShell converting a value on your behalf instead of failing — an empty `-Raw` read becoming `$null`, a non-matching capture group becoming `''`, an unquoted `{…}` argument becoming base64 `-encodedCommand`, a native command's stderr becoming a terminating error under `Stop`, a file's bytes becoming CP1252 or UTF-8-with-BOM. **At least** seven instances, one mechanism, every one found incidentally rather than by search, and the call site reads correctly in all of them. Pin the conversion explicitly (`-Encoding`, quoting, `$LASTEXITCODE`); when a rendering cannot be trusted, re-encode the bytes and read those |
| 211 | A verification whose result nothing consumes — a hash compared and PRINTED beside the `git apply` rather than gating it, so the check reported a mismatch and the corrupted patch was applied one line later. Distinct from a check that cannot fail: this one DID fail, into a void, and its output still reads as verification. The comparison must be the thing that DECIDES (`[ "$got" = "$want" ] \|\| exit 1`), never a line printed next to the thing that decides |
| 212 | `#[cfg(unix)]` on a test does not skip it on Windows — it DELETES it, and no harness has a column where "never compiled" differs from "passed". Paired with a version-control symlink the checkout could not materialise (so the fixture degraded into an ordinary file the app correctly navigates to), one rubric had zero coverage on a platform while every artifact reported success. Prefer a runtime skip that can be printed and counted; treat a fixture existing *in the form intended* as a precondition to check |
| 213 | A commit message, changelog or doc comment is an assertion about state that NO TOOL CHECKS — "this commit deliberately excludes X" shipped in a commit containing X, and the message was written before the staging it describes. Intent and result are separate artifacts published together and never reconciled; verify the DESTINATION STATE (`git show --stat`, a blob hash, a tree hash) rather than trusting that the plan was executed |
| 214 | `backtrace_symbols`' BSD twin is not the safe half of the pair — on macOS BOTH it and `backtrace_symbols_fd` resolve each frame through `dladdr`, i.e. dyld's loaded-image walk, which is not documented async-signal-safe; a fatal-signal handler that calls it deadlocks if the signal lands while any thread holds a dyld lock (an ordinary lazy bind is enough). The existing safety argument — `backtrace_symbols_fd` avoids the `malloc` that `backtrace_symbols` does — is CORRECT and about the string-formatting half only, so it reads as settling a question it never addressed; Apple's own reporter runs OUT-OF-PROCESS precisely to keep symbolication out of a dying process. Write raw frame addresses + the module map and symbolicate offline. A same-named POSIX API on a new platform needs its OWN safety argument, not the old one's |
| 215 | A refactor claiming to PRESERVE behaviour, verified with hand-written expectations about the new code's output — which encode what you BELIEVE the function does, and are checked by a suite that will happily agree with a wrong belief. Written after a rewrite of a hot predicate: the hand-written expectation failed, the rewrite was correct, and the expectation was wrong about the LANGUAGE (a document starting with a tab is an indented code block, so a second mechanism downstream suppressed the effect being asserted). Keep the OLD expression verbatim as an oracle and compare the two at every input of a corpus — a differential test encodes what the code DID, is immune to a second mechanism sitting downstream, and tests the thing that actually changed |
| 216 | A gate that checks a citation EXISTS when the property that matters is that it CORRESPONDS — structurally incapable of seeing the defect, not merely blind to one instance. A prefix-normalising sweep rewrote a cross-register citation without re-resolving its NUMBER, silently re-pointing it at a real entry about an unrelated subject; the reference lint passed, because the entry named is defined, and a citation naming the wrong real thing is not a dangling reference. It hid among three CORRECT citations of the same number in the same file. Ask what a gate's PASS actually asserts: "the target exists" and "the target is the right one" are different questions and only the first is cheap |
| 217 | A negative result — "nothing happened, so it is contained" — is worthless without a POSITIVE CONTROL proving the detector fires at all, because "the effect was prevented" and "I cannot observe this effect" produce identical output. The counterpart to #209: 209 says an assertion that cannot fail is worthless; this says an observation that cannot succeed is worse, because it reads as evidence. Run the same probe with the guard REMOVED and require the effect to appear; where the effect is asynchronous, wait for it (a synchronous post-check reads zero even on the control); where it is ambient, key detection on an identifier nothing else could produce. Best of all, prefer an oracle with no side effect to control for — a synchronous state change the guarded machinery sets is strictly better than watching for a launch |
| 218 | Confidence RATCHETS across a relay: each hop drops the qualifier that made the previous hop honest, and nobody in the chain does anything wrong. A coverage NOTE ("no reviewer reached this directory") became a work ASSIGNMENT one hop later, and an assignment carries an implicit "this is worth your day" the note never claimed. The hedge is dropped precisely because it lives in a trailing caveat paragraph, which is the first thing a summary discards. Label an unmeasured claim INLINE, on the sentence making it, so the hedge travels with the claim into whatever brief is written from it; and when converting someone else's claim into work for a third party, carry the label or go back and ask for one |
| 219 | A remedy written INSIDE ONE CONSUMER is one the next consumer will not find — it reached 2 of 5 eligible sites not because anyone disagreed with it but because it lived in a test module nothing else could import, so the other three kept the shape it replaced. Nobody notices, because the sites that adopted it look like the whole population. Hoisting the remedy to where every consumer can reach it is not tidying: it converts a discipline into a DEFAULT, and only a default survives a tired author at 6pm. "We wrote a helper" and "we put the helper where it is easier to use than to avoid" are different lessons; the second is the one that holds. Discoverability rung of the enforcement ladder, below a clippy ban and a module-privacy seal but above pure convention — and the one to reach for when the risky call is legitimate elsewhere and cannot be banned |
| 220 | A regression guard whose inputs were copied from the reproduction that motivated the FIX has coverage exactly equal to the fix — it can confirm, never discover. A super-linear scan was fixed and guarded with a correct, discriminating, machine-independent growth-ratio test; an identical quadratic survived in the SAME function under a different delimiter of the same family, and the new guard passed with it fully intact. Two parameters had been frozen to a literal and either alone was fatal: the INSTANCE (one of five delimiters — the one just fixed) and the input SHAPE (the survivor sat past a guard clause the test's input never satisfied). Ask what POPULATION the defect is one member of, then derive the guard's inputs from a structure the production code also consumes, crossed with one shape per guard clause on the path |
| 221 | A test comment explaining why it asserts less than its NAME promises is the strongest defence against review a gap can have — it converts the gap into a documented decision, and nobody re-derives a constraint someone else has already reasoned about. "Real oversized files are impractical to create" justified a refusal test that wrote ONE byte, asserted ACCEPTANCE, and checked a hand-constructed refusal's wording; the premise was false and two lines cheap to falsify (a sparse file via `set_len`). Treat "we cannot test this because X" as an unverified claim inside the test. Two greps fall out: a test whose name describes a behaviour its body never provokes, and any comment beginning "real … are impractical/too slow/not possible to" |
| 222 | TWO GATES, EACH CORRECT, ENFORCING OPPOSITE THINGS — a policy document prescribing the exact attribute the repo's lint REJECTS, so obeying the written rule breaks the build and satisfying the gate contradicts the document. Nothing is broken: the check's pattern matches, its assertion can fail, it checks the right property. The stale artifact is the WRITTEN RULE, and a lint's input set is source, so nothing in the toolchain reads it. Compounded by the correct rule being present in the same file under a PLATFORM-SCOPED heading — filed where it was discovered, not where it is needed (#219 one level up, applied to a document). Gate the PRESCRIPTIVE documents on "names a banned construct without naming its replacement", discriminating by PARAGRAPH not line proximity; scope to documents a reader could ACT on (the historical register legitimately names the ban 13 times); define the document set in the shared cross-platform contract, never as a literal list per script; make an empty set a hard error or both ports agree vacuously. Closing: a gate beats a document on FALSIFIABILITY (it can be mutation-tested; prose cannot) not on observability — but that is a reason to distrust the DOCUMENT, not to trust the GATE, because a check can itself be an unratified artifact winning purely because it executes. "It runs" establishes neither correctness nor authority. Sub-lesson worth its own name: a rule filed under the heading of the investigation that produced it can be present, correct and UNREACHABLE — not stale, just never opened by the reader who needs it |
| 223 | A finding written as a CONCLUSION recruits agreement or dispute — both cheap, neither producing information; the same belief written as a bounded TESTABLE PROPOSITION recruits a measurement. "Attribution is unverifiable FROM THE COMMIT RECORD" got tested and falsified (the reflog held the pre-squash tips), and the falsification exposed a governance violation nobody was hunting; "attribution is lost" would have left nothing to attack. Inverts the usual advice: **an unmeasured claim written as a testable proposition is more useful than a measured one written as a conclusion**, because the first recruits a second measurer and the second forecloses the exchange while sounding more rigorous. State the search space in the sentence — "not recoverable FROM Y", "grepped Z for P and found none", "holds ON THE PLATFORM I RAN IT ON". Read a finding back and ask what a reader can DO with it |
| 224 | A SQUASH destroys ATTRIBUTION (not history — the tree is exact; "who wrote this line" is what goes), and the only artifact that restores it is the squashing machine's REFLOG: local, never pushed, and pruning on a 90-day timer that looks identical to present until it is gone. So a governance rule of the form "an agent may not autonomously add a POLICY rule" is unenforceable with no error, no failing check and no visible gap — it bites hardest on exactly the documents whose rules govern who may change them. Decide deliberately: tag the pre-squash tips, or accept the loss AND WRITE THE CONSEQUENCE INTO PROSE WHILE THE EVIDENCE EXISTS. Meta-lesson: "this cannot be determined" is a claim about a SEARCH, and the commit record was the wrong place to stop |
| 225 | FOUR denial-of-service paths in FOUR subsystems by different hands were ONE omission seen four times: the project had a developed opinion about what a document may REFERENCE (URLs, image paths, traversal) and none at all about what it may COST — the same threat model's other half, never written down. The failure mode is not bad decisions but the reasonable assumption that someone UPSTREAM decided. Count a cluster before fixing it individually: one instance is a bug, four is a missing DECISION, and fixing four without making it leaves the fifth author where the first four were. File the halves separately — the a priori RULE in POLICY (one line: every read goes through the one admission test), the VALUES in code as SSOT with their measurement recorded, the a posteriori LESSON here. Two conditions each necessary and neither sufficient (size vs file TYPE — a FIFO reports length 0, a type check alone admits 40 GiB) must be one function, because callers offered two constants will use one |
| 226 | THE CHECK YOUR SELF-TEST DOES NOT COVER is the one that ships broken. A lint check was mutation-tested against both defects it existed to catch, run green, cross-checked in a second language — and shipped with every reported FILENAME wrong (always the NEXT file; only the last entry read correctly) plus a silent false negative. All three verifications shared one assumption with the code: that a run is ONE FILE. The mutation test planted one defect at a time and asserted only that the check FAILED, never WHICH file it named; the cross-check ran one file, and both bugs live at the boundary BETWEEN files; the corpus had no entry for this check at all. When apparatus and implementation share a blind spot, agreement is the blind spot reporting itself twice. Fix: capture identifying context at DETECTION time not report time; make the self-test corpus cover every check and be MULTI-FILE, asserting which file and line; define the program once so the self-test exercises what the gate runs. Corollary: testing effort is evidence about the SPACE TESTED, not about correctness — here the never-executed port was right and the mutation-tested one was wrong, the exact reverse of the author's prediction |
| 227 | A ScrAP-26-style on-viewport safety gate that checks ONE AXIS of a two-axis hazard: it passes, the call site stops looking because a proven-safe TYPE now vouches for the value, and the exact assertion the seam exists to prevent fires THROUGH it on the other axis. The y-only reasoning was written into the seam's own doc comment ("only the y can leave the viewport under scrolling") and was false — a `WrapMode::Word` view overflows HORIZONTALLY on a long unbroken token, putting the selection midpoint 18002 px into a 600 px view. An unforgeable-by-construction type is a claim about CONSTRUCTION, never about COVERAGE, and the two read identically at a call site. Pass the extent as ONE two-axis value so a one-axis gate stops being expressible; gate the rectangle you actually POINT AT (the caret sliver), not the one you derived it from; saturate the extent arithmetic, since the coordinate is document-derived and the whole point is that content can put it absurdly far out |
| 228 | A property IMPLEMENTED ON ONE BRANCH of a discriminator and DOCUMENTED as a property of the whole function survives exactly until the discriminator moves. The seen-marker's "pruned to reports that still exist" bound was real only in the legacy-watermark branch — where it is structural, since a watermark must be evaluated against the present set — while the set branches returned their lines unpruned. Nobody noticed because the steady state (one crash, one line) took the watermark branch, so the bound was being delivered by the very format AMBIGUITY that was the defect under repair. Removing the ambiguity routed the steady state to the other branch and the marker began growing without limit. The tell is a comment at the WRITER naming a bound the READER implements: state and implement an invariant in one place, and when repairing a discriminator, ask which properties were riding on the branch you are about to stop taking |
| 229 | A SEAM NAMED FOR A GUARANTEE IT DELIVERS ON ONE PLATFORM. `private_options()`'s doc said "owner-only (0600) on unix, platform default elsewhere" — which reads as graceful degradation and is an unimplemented half; a seam honest about being a no-op invites a reviewer to check whether the no-op is acceptable, one named `private_options` invites them to assume it is handled. Windows cannot express it at all (`OpenOptionsExt` has NO security-descriptor hook, `lpSecurityAttributes=NULL` composes the DACL purely from the parent's inheritable ACEs, and there is no umask analogue), so the guarantee is ENTIRELY a property of the parent DIRECTORY — measured safe under `%LOCALAPPDATA%` (`D:P` protected) and VIOLATED with `XDG_STATE_HOME` on a second volume (`Authenticated Users:(I)(M)`). Its Linux twin was live on the DEFAULT path: reports correctly `0600` inside a `0775` directory holding a `0664` `session.toml` that lists every open document — a private file in a traversable directory still advertises its own name. Fix the DIRECTORY, not the file: it is the one seam both platforms share (POSIX traversal, Windows ACE inheritance), it covers the files that never went through the per-file seam at all, and it is the only place a writer that must NOT be private (an atomic-save that also writes user documents) does not have to be special-cased. Also: a platform-conditional permission model needs a platform-conditional RUBRIC, or the gate deletes the requirement |
| 230 | A `clippy.toml` `disallowed-methods` ban reaches the METHOD and not the BUILDER PROPERTY of the same name — `set_tooltip_text` is banned, `MenuButton::builder().tooltip_text(…)` compiles clean. Three sites sat inside the ban's blast radius and were never flagged; only a live tree-walk assertion found them. The ban is a path match, and a builder property is a different path (a generated setter on `…Builder`), so the "strongest rung of the enforcement ladder" silently drops every construction site that uses the builder idiom — which is exactly the idiom a codebase reaches for when a widget needs three or more properties at construction. Nothing warns: the call site reads correctly, the property IS set, and the paired obligation the ban existed to force is simply absent. So a ban is a claim about one SPELLING of a call, never about the effect — pair every `disallowed-methods` entry with a runtime assertion over the built object (here: walk a live window and assert the accessible name exists), and treat the two as one mechanism with two halves rather than as a primary gate plus a nice-to-have. Kin to #219 (the ladder itself) and to the general lesson that coverage of an ENFORCEMENT tool is a property of its matcher, not of its intent |
| 231 | RETIRING AN AMBIGUOUS CITATION FORM BY LEGALISING IT INSTEAD OF BANNING IT, and then claiming the sweep "holds tree-wide" without stating a predicate anyone could re-run. The bare `AP-N` form meant *this* register historically and the `gtk4-rs` skill currently, so its correct and incorrect uses were textually identical — 443 were still in the tree when the convention said none were, and the sweep that produced the convention **rewrote prefixes without re-resolving numbers**, re-pointing correct skill citations at unrelated local entries (`bar.rs`'s `ScrAP-79` on a pump-loop comment) where the reference lint cannot see it: proving a cited entry EXISTS cannot distinguish "the right entry" from "a real entry about something else" → make the ambiguous form ILLEGAL and gate it (check 8), spell the other register as one whitespace-free token (`GTK4Rs/AP-N` — a two-word form is split by any wrap, and two were already broken that way), migrate PER SITE by re-deriving each number against the lesson, and treat "both registers agree at this number" as the only licence for a bulk edit |
| 232 | `g_file_replace_contents[_async]` IS NOT UNCONDITIONALLY ATOMIC, and its documented-sounding "replace" hides a path that **deletes your previous good file before failing**. Four routes reach `ftruncate(fd, 0)` (`glocalfileoutputstream.c:1030-1041`, `:1230`): a **symlink** target, a **hard-linked** target (`st_nlink > 1`), an fchown/fchmod mismatch on the temp — all three closed by `REPLACE_DESTINATION` — and **`g_mkstemp_full` failing** (unwritable dir / ENOSPC / RO mount → `goto fallback_strategy`, `:1046`), which that flag does NOT close and which, *with the flag set*, `g_unlink`s the destination before reopening (`:1192-1211`). So the flag that buys atomicity converts the remaining failure from "torn file" into "**previous file gone**" — strictly worse, because a torn file is detectable and a deleted one is indistinguishable from "there was nothing there". ⚠ NARROWED before any probe: the trigger is **not** an unwritable directory (`unlink` needs the same directory write permission `mkstemp` was denied, so that case should fail safe) but any failure stopping `mkstemp` yet not `unlink` — **ENOSPC/EDQUOT/EMFILE** — which is why a startup writability check is NOT a mitigation. Durability is likewise partial and unflaggable: the temp is fsynced **only when the destination already existed** (`sync_on_close` set solely on the EEXIST branch, `:1305`; fsync at `:328`) and the **parent directory is NEVER fsynced** — exactly one `fsync` in the whole file, identical in 2.72.4 and 2.89.3 — so a first-write-of-session is unsynced and a power loss can lose the newest rename (a SIGSEGV/OOM/`kill -9` cannot: page cache survives). Also: without `REPLACE_DESTINATION` the temp is created `PRIVATE` (0600, at `open(2)` via `g_mkstemp_full` — genuinely no world-readable window) and then **fchmod'd back to the original file's mode** (`:1048-1058`), so a file that ever acquires a laxer mode keeps it forever; with the flag, the temp's mode wins and privacy self-heals. The good news is real and worth recording too — the async is **genuinely thread-pooled**, not main-loop-chunked (`g_file_real_replace_async` → `g_task_run_in_thread`; via-threads because `GLocalFileOutputStream` neither overrides `write_async` nor implements `GPollableOutputStream`), the callback returns on the thread-default main context, and gio-rs's `replace_contents_async` **takes ownership with no copy and hands the buffer back** (so a 64 MiB allocation is recyclable; `replace_contents_bytes_async` is an un-bound TODO in 0.21.5). GIO does **not** serialise concurrent replaces of the same file — order them yourself. Lesson: **a GIO convenience call's guarantees are a function of its FLAGS, and the fallback path is where the flag you set changes what failure costs you** — enumerate the fallbacks before adopting it, and never infer "atomic" from the verb in the function name — ⛔ **SUPERSEDED IN PART, MEASURED**: route C is NOT the hazard (it cannot fail for the reason that sent it there — `g_close(fd)` precedes the unlink and hands back the denied descriptor); the real data-loss path is an **ordinary disk-full during the write**, because `gfile.c` ignores the close result and close is where the rename lives, so a truncated temp is promoted over the previous good file (measured: 33 bytes → 0). `GError` cannot distinguish the cases. Mitigation: drive the stream yourself and close with an **already-cancelled** `GCancellable` on write error (unlinks the temp instead of renaming). Also corrects this entry's own size-sampling measurement: a probe establishes a property only over the interval it samples, and the damage here happens at CLOSE, after the window ends — LATER MEASURED: reproduces on **real ext4** (via `RLIMIT_FSIZE`, not a tmpfs artefact), and on the **async** path the rename lands on a worker thread AFTER your error callback returns — so an error handler that rewrites the previous snapshot from memory RACES it, and a probe asserting immediately in that callback reports a false INTACT |
| 233 | Delegating a delimited format's unforgeable-terminator invariant to a third-party serialiser's escaping — the `toml` crate emits a MULTI-LINE string for any value containing a newline, so a path with an embedded newline forges the `+++` closing fence and silently truncates the payload. The design note that closed this hazard by reasoning ("TOML basic strings escape `\n`, so a correct serialiser upholds it automatically") is true of the SPEC and false of the CRATE; the test written to pin it FAILED on first run → enforce on the side you control: escape line breaks out of every externally-derived string BEFORE the serialiser sees it (one explicit field-by-field transform, so a new field does not compile until it has been considered), then VERIFY the serialised header carries no bare terminator and return a real `Err` — unreachable by construction, which is exactly why it must not be a `debug_assert` (compiled out of the shipped build). Mutation-tested; assert the MECHANISM (no header line forges the fence) as well as the CONSEQUENCE (the round trip survives). Lesson: a design note that discharges a precisely-identified hazard by appeal to an upstream guarantee is a hypothesis with a test attached, not a conclusion |
| 234 | A TEST SUITE THAT ASSERTS ONE OF A FEATURE'S TWO REPRESENTATIONS IS EVIDENCE ABOUT THAT ONE ONLY. A recovery wrote the editor buffer but not the text every DERIVED view renders from, so the editor was right and the preview showed pre-crash content — in the application's DEFAULT mode, i.e. the feature silently doing nothing for most users. 856 tests passed, because every assertion had been written against `editor_text()`: the half that worked. The suite was not weak, it was **aimed**, and a suite's greenness is a claim scoped to the surfaces its assertions name → when state has a source and a projection, assert the PROJECTION (or both, and that they agree); enumerate a feature's representations before writing its first test; and treat a live run as non-optional for anything with a rendered surface, because it is the only check that cannot be aimed at the wrong half. Kin to #87 (masking) and ScrAP-56 (live gate); the CAM's Derived-view matrix is the write-time form of the same obligation |
| 235 | A FEATURE WIRED INTO ONE OF A FRAMEWORK’S SEVERAL ENTRY POINTS IS ABSENT FROM THE OTHERS, TOTALLY AND SILENTLY. Crash recovery ran only from GApplication’s `activate`; a launch carrying a file argument dispatches to `open`, which never called it — so `app notes.md`, a desktop-file association, `xdg-open` and an Explorer double-click all skipped the recovery offer. **The ordinary way a user reopens the document they just lost was the one route that did not offer it back.** Not partial degradation: total, for that route, with no error and no log line. Hidden because the author’s own testing (and every automated test) launched bare or via session restore — the entry point you develop against is the one that works → enumerate a framework’s entry points as a SET before wiring anything startup-scoped (`activate`/`open`/re-activation/URI handler/service mode), route them through ONE gated helper whose doc names every caller, and assert the EFFECT through the real handler rather than that a function was called. Found cross-platform by a peer seat, in shared code, not in a port. Kin to #234 — same failure of scope, that one about assertion targets, this one about dispatch paths |
| 236 | A SCREEN-COORDINATE CAPTURE IS NOT A WINDOW CAPTURE. `GetWindowRect(hwnd)` + `CopyFromScreen` over that rect reads the DESKTOP FRAMEBUFFER at those coordinates and knows nothing about the window that supplied them — so an unfocused/occluded target is silently photographed as whatever is in front of it. Well-formed PNG, correct size, wrong application, no error. Invisible all session because every earlier capture happened to follow an `AppActivate`, so coordinates and pixels coincided by luck → activate, settle, THEN capture, and gate the capture on `AppActivate`'s BOOLEAN result (it is not fire-and-forget); capture the window rect only, never the desktop (also a privacy property on an operator's real machine). Rule: **the thing supplying the coordinates and the thing supplying the pixels must be the same object.** A plausible artefact of the WRONG SUBJECT is strictly worse than an error — an error stops you, a wrong screenshot gets believed. #193's shape one level down (there a harness gap read as an app defect; here the harness succeeds and misidentifies its subject) |
| 237 | A `cfg`-GATED GATE PROVES NOTHING ABOUT THE BRANCHES IT DID NOT COMPILE. `clippy --all-targets -- -D warnings` — mandatory, run on every change for the project's life — had NEVER passed on Windows: 7 pre-existing errors, every one in a Windows-only compiled path (an unused `mut` whose only mutation is in a `#[cfg(unix)]` arm, a constant dead once its POSIX-mode uses vanish, five `return`s unneeded once the unix arm compiles out). The canonical-platform convention hid it: the platform running the gate most compiles the least platform-specific code. **A green gate reads as evidence of absence** — nothing reports "7 checks not performed" → run a `cfg`-selecting gate on each `cfg` that selects code, and treat a clean run as evidence about that platform ONLY. Fix mechanically, never `#[allow]`. Kin to #207 (a gate is its pattern AND its set) one dimension across — there the set was an enumeration you could diff, here the compiler chooses it and there is no list; kin to #234, the same failure of scope aimed at assertions rather than gates |
| 238 | ACTIVATING ON `GtkGestureClick`'s `released` ALONE IS NOT "ACTIVATE ON CLICK" — it fires for the release that ENDS A DRAG THAT BEGAN SOMEWHERE ELSE, so an ordinary swipe-selection whose pointer happens to stop over a link opens it (MEASURED, 4.6.9: the reader selects, the document navigates away). GTK gives no pairing: `pressed` and `released` are independent signals, and the WIDGET GTK ships for this does the bookkeeping itself — `gtk_label_click_gesture_pressed` sets `link_clicked` only when the PRESS landed on a link, and released requires that flag **AND `selection_anchor == selection_end`**, i.e. two operands, the second easy to stop reading before. So the same-target rule alone leaves half the defect live: a swipe *within* one long link caption presses and releases on the same link (also MEASURED navigating) → require BOTH, comparing the press's target IDENTITY to the release's (a hit-test answering `bool` cannot express it — two occurrences of one URL are two links) and bounding the pointer's TRAVEL by the desktop's `gtk-dnd-drag-threshold`. Do not gate on "is something selected now" instead: `gtktextview.c` deliberately holds a press that lands INSIDE a selection for a possible DnD and resolves it only at drag end, so that state answers a different question at release time. Enforcement, because a per-site rewrite is the thing that rots (ScrAP-116/ScrAP-129 ladder): one seam owning BOTH connections + a `clippy.toml` ban on the raw `connect_released`, whose sole `#[allow]` is inside it — the release-only shape then does not compile. The three affordances in this app had drifted to three different answers (checkbox correct, link and marker not), which is what a rule with no home looks like. Also measured here: a gesture whose sequence another gesture CLAIMS emits `pressed` and then NOTHING — released is gated on `state != DENIED` (`gtkgestureclick.c`) and a denial is not a cancellation, so no `cancel` either; `GtkTextView` claims exactly this way for a press inside a selection ("a special case to start DnD"), so a press whose release never arrives is a normal state to budget for. Sibling of #79 (a container gesture firing on a child's press): both are "the gesture fired for an event that was never a click on this thing" |
| 239 | `git stash pop` RESTORES THE SOURCE, NOT THE BINARY — a `target/release/<app>` invoked BY PATH after a pop is still the PRE-change build (a pop touches the working tree, never `target/`; only `cargo run`/`cargo build` rebuilds), so a two-binary regression comparison silently drives ONE binary twice. Presents as the FIX FAILING — and failing *identically* in both columns, which reads as "the fix doesn't work" rather than "these are the same program" (cost 4 drive cycles + an instrumented build to unmask). Diagnostic tell: **symmetry** — when control and treatment behave identically, suspect they ARE identical (build/path/cache) before suspecting the treatment; a control that cannot differ has stopped being a control (#217 inverted). Fix: `cp target/release/<app> /tmp/pre` to name the control, `cargo build --release` immediately after the pop, compare two DISTINCTLY-NAMED binaries — never one path that means different things at different times. Kin #132 (an identifier chosen by convention is not one you own — there a PID, here a build-output path), #223 |
| 240 | A DETECTOR THAT ENUMERATES THE **VOCABULARY** OF A FREE-TEXT CITATION IS DEFEATED BY A SYNONYM. `lint-references` check 1 matched the `…entry <id>` spelling and passed clean over `…item <id>` — an illegal, BORN-DANGLING issue citation in `src/` (the very commit carrying it rewrote the entry it named) — because the pattern enumerated the connecting noun the author happened to write. Third instance of one shape in this script (cf. #207's 21 danglers; check 1's own first version missing three forms) → match the SHAPE (`([a-z]+ )?`, any lowercase connector) and let the discriminating token (`[A-Z]\b` + case-sensitivity) do the work; a synonym list is always one entry short and fails by reporting PASS. SECOND-ORDER, costlier: the review named ONE instance, the fix removed it and introduced TWO MORE of the same rule — both catchable by the OLD pattern — because the fixing seat's "full pipeline" (fmt/clippy/every test target) omitted steps 6 and 9, the only ones that can see a reference defect. **Name the class, not the instance, and put a gate on it** — a review finding is discharged by whoever fixes the line, not the category; and a per-platform pipeline minus the steps unique to a defect class is a SUBSET claim whose green run reads identically to a complete one (#237's shape, aimed at which STEPS ran rather than which BRANCHES compiled). Kin #219 |
| 241 | A process NAME IS NOT AN IDENTITY — the crash-recovery liveness probe compares a recorded `owner_pid` against the running process's executable name (`/proc/<pid>/comm`, `proc_pidpath`, `QueryFullProcessImageNameW`), and a RECYCLED pid now running ANOTHER instance of this app answers **live** on all three → the scan skips a snapshot whose real owner is dead and silently declines to offer the user's unsaved work back. **NOT platform-specific**, which is the finding: it surfaced on Windows (small, eagerly-recycled pid pool) but Linux and macOS have the identical hole and merely reach it less often — so "a Windows problem, fix it in the Windows arm" is the mis-scoping to avoid, and a fix built there leaves the other two exposed while looking complete (kin #175, consequence-vs-mechanism inverted). Fix = record the owner's process START TIME beside `owner_pid` in `SwapHeader` and require both (a recycled pid has a later start time); DELIBERATELY NOT IMPLEMENTED — a three-platform on-disk schema change against an unobserved hole, documented so the next reasoner starts from "the probe is identity-blind" and so it rides along free if the schema is revised anyway. Rule: a probe comparing a REUSABLE handle plus a NON-UNIQUE attribute answers a weaker question than it appears to, and the gap is untestable (it needs a race against the OS allocator) → ask what would have to be true for the answer to be WRONG |
| 242 | `cargo clippy --all-targets -- -D warnings` **without** `--features gtk-integration-tests` reports dead-code errors (`a11y::has_name`, `PreviewFindCache::builds`) in modules your change never touched — both are used ONLY from feature-gated code, so without the feature the callers do not compile and clippy is right; the INVOCATION is wrong, not the tree. Reads as "your change broke two unrelated files", the most expensive framing, since it sends you to read code that is not the problem → run step 2 exactly as written; diagnostic reflex is to check the invocation before the code, and `git status` on the named files settles it in one command. Lesson: **a rule stated only as its RATIONALE is not usable at the moment it is needed** — POLICY already said the flag "is not optional", but in terms of what it protects (ScrAP-124's rotting suite), never what it LOOKS LIKE when forgotten, which is the form the knowledge is needed in. Document a failure's APPEARANCE beside its cause; the symptom is the index the lesson is looked up by. Kin #237 (a `cfg`-selected input set producing a confident wrong-looking result — there by hiding checks, here by inventing findings) |
| 243 | GLib's I/O THREAD POOL IS **ONE PROCESS-WIDE POOL OF TEN**, so moving document reads/writes off the main thread with `gio::spawn_blocking` puts them in the same queue as the crash-recovery snapshot writer's `replace_async` — `GLocalFile` overrides no async `GFile` vfuncs and `GLocalFileOutputStream` is not pollable, so BOTH fall through to `g_task_run_in_thread` (`gtask.c:643` `G_TASK_POOL_SIZE 10`, `:2195`). It is **NOT starvation** — the pool grows (`:629-646`, ONE thread per compounding wait, 100 ms base ×1.03/running task, on GLib's worker thread so it survives a wedged main loop) — it is **unbounded latency inflation, which for a writer that protects unsaved work is the same problem**: a snapshot that is 8 s late is 8 s of work a crash still loses. MEASURED (GLib 2.72, N tasks blocked on an empty pipe, then a 64 KiB snapshot write): 9 blocked → **0.2 ms**; **10 → 206.6 ms**; 15 → 686 ms; 20 → 1.36 s; 50 → 8.37 s — **the cliff is exactly the base pool size**, so the ninth costs nothing and the tenth costs 200 ms. No lever exists downstream: there is no public API to give one writer its own pool, and `io_priority` is compared only AFTER `blocking_other_task` (`:2199` sorts the queue), which is set solely for tasks queued from inside a pool thread (`:1534`) — none of an app's own are → **bound your own concurrency at the source** (here 4, leaving ≥6 base threads), and let the excess wait in-process where waiting is free. Two corollaries worth keeping: the two-stage `replace_async`→`write_all_async` shape does NOT double the penalty (15→567 ms, 30→2.87 s), so nobody should "optimise" it back to `replace_contents`, which destroys the previous file on a write failure (#232); and `g_task_start_task_thread` has exactly two exits, both `g_thread_pool_push` (`:1516`, `:1536`), with `g_assert` on pool creation (`:2197`) — the func can NEVER run inline on the calling thread, so the freeze cannot come back by that route. Rule: **moving work off the main thread does not make it free — it makes it contend**, and the thing it contends with is whatever else the runtime put in the same pool, which is not visible from the API you called |
| 244 | MAKING A WINDOW-SCOPED OPERATION ASYNC TURNS "WHICH TAB IS ACTIVE?" INTO TWO DIFFERENT QUESTIONS. Save was `state(window)` — exact while the write was synchronous, because nothing could change the active tab mid-call. With the read and the write dispatched to a thread pool, the main loop runs in between, and the SAME expression asked before and after answers about two documents: the guard read checks tab A's file for an external change and the write then commits tab B's buffer (C2 defeated silently — the conflict prompt that should have appeared was about a different file). Same shape, three more sites: a modal "Overwrite?" the user answers after switching tabs; a completion that refreshes the on-screen chrome for the wrong document; a per-tab crash-recovery snapshot retired via the WINDOW-scoped choke point, so a saved document keeps a snapshot and the next crash offers already-saved work back as "unsaved" → **resolve ambient context ONCE, at the moment the user acts, and carry it** (an explicit `Rc<TabState>` parameter, not a re-lookup), and split every completion into tab-scoped work (which must target the captured tab) and window-scoped work (which must target whatever is on screen). MEASURED TRAP in guarding it: the obvious regression test PASSES against the broken code, because a test that saves and asserts leaves the same tab active throughout — the guard must FORCE the divergence (`spawn_local` does not poll until the loop iterates, so switching tabs synchronously right after issuing the save is deterministic), or it pins nothing; this one survived its first mutation run and had to be rebuilt. Kin #46 (don't cache a reparent-able context) one step across: there the cached value went stale across a MOVE, here a freshly-read one is stale across an AWAIT — and the tell is identical, an expression whose answer is a property of *now* being used to describe *then* |
| 245 | AN Xvfb UI DRIVE CAN DELIVER **NOTHING** AND LOOK EXACTLY LIKE ONE THAT DELIVERED EVERYTHING. MEASURED here (Xvfb + openbox, GTK 4.6.9/X11): `xdotool windowactivate --sync` succeeded and BOTH `getactivewindow` and `getwindowfocus` returned the app's window, yet **every** `xdotool key`/`type` was silently dropped — Ctrl+N added no tab, `Alt+3` did not change view mode, typed text never appeared — while **`xdotool mousemove`+`click` on the same window worked perfectly** (a link click opened a tab, a task checkbox toggled, a toolbar button saved). So the pointer half of XTEST reached the app and the keyboard half did not, with no error from any tool and a screenshot that looks like a healthy app because it IS one. Cost 3 full drive cycles before a deliberate probe (press a key with an unmistakable, checkable effect) unmasked it; the first cycle's script also HUNG, because `import -window ""` blocks when the window lookup silently returned empty → **before trusting any drive, assert a POSITIVE CONTROL on each input channel you intend to use** — one keystroke and one click whose effect you verify — and treat "the drive ran and found no bug" as unfalsifiable until you have. Two further practicalities that cost time here: with no WM at all nothing ever takes focus, so keys are dropped for a *different* reason and adding a WM fixes only that one; and `xdotool mousemove` takes SCREEN coordinates while screenshots are window-relative, so read `xwininfo -id <win>` "Absolute upper-left" and add the offset — a click at the un-offset coordinate lands on empty desktop and, again, reports success. Same family as #239 (a control that cannot differ has stopped being a control) and #217 (positive controls), aimed at the input channel rather than the binary under test |
| 246 | GDK-WIN32 REFUSES AN EMPTY WINDOW TITLE AND SUBSTITUTES A LITERAL `"."` — `gdk_win32_surface_set_title` (gtk-4.22.4 `gdk/win32/gdksurface-win32.c:1238`, MEASURED from source) does `if (!title[0]) title = "."` before `SetWindowTextW`. So every window GTK leaves untitled — and `GtkMessageDialog` leaves itself untitled BY DESIGN, because GNOME's HIG asks for it — wears a lone period in its caption and in the taskbar on the native Win32 frame. Invisible to the Linux and macOS seats not because the code differs but because **an empty caption is a legitimate rendering everywhere else**: CSD simply draws nothing, so the same widget tree is correct there and wrong here, and no warning is emitted on any platform → set a title on every window you construct, at the shared construction site rather than per call, and do it UNCONDITIONALLY (a `cfg(windows)` caption forks behaviour and puts platform code outside `platform/<os>/`; the visible cost elsewhere is a header label where there was blank space, which is a design choice to state, not a defect). The generalisable half: **a toolkit default chosen for one platform's design language becomes a defect on a platform whose window manager cannot express it** — audit every "we deliberately leave this empty/unset" against each backend's substitution behaviour, because a substitution is silent by construction. Kin #237 (a green gate on the canonical platform is evidence about that platform only), aimed at rendering rather than compilation |
| 248 | A RANDOMLY-MINTED IDENTITY CORRELATES ONLY WITH THE MECHANISM THAT PERSISTED IT, AND FAILS AS A DUPLICATE RATHER THAN AS AN ERROR. Crash recovery matched each snapshot to a tab by `DocId` — 128 random bits minted per tab and persisted in `session.toml` — so it correlated with **session restore** and with nothing else. The ordinary way a user reopens the document they just lost is to open the FILE again (Explorer double-click, `app notes.md`, a desktop association), which mints a FRESH id; the snapshot then matched nothing, fell through to its "the session never restored this" branch, and opened a **second tab for the same path** — one clean, one carrying the work, indistinguishable in the strip. Not an error, not a log line: a plausible extra tab, which is why it survived a suite in which every recovery test relaunches *bare* (the one route where identity does correlate) → when an identity is minted per object rather than derived from the thing itself, enumerate **every** way the object can be re-created and give the lookup a second, content-derived key (here the canonical path, through the project's existing "is this the same file?" rule, so `..`/symlinks/Windows casing are handled once). MEASURED SECOND-ORDER TRAP in the fix: the adopting tab takes on the snapshot's id, so a "already claimed" set recorded under the id the tab had **when it was chosen** stops matching a moment later — the next snapshot for that path finds an unclaimed-looking tab and overwrites the work just recovered into it. Record both identities, and treat *any* set keyed on a mutable identity as suspect. Kin #235 (a feature wired into one of several entry points — same defect, one level up: there the route never reached the code, here it reached it and the correlation quietly missed) and #241 (a lookup whose key answers a weaker question than it appears to) |
| 249 | A CAPABILITY WHOSE BACKEND IS A HELPER EXECUTABLE IS A PACKAGING OBLIGATION, AND THE DEVELOPMENT TREE IS STRUCTURALLY INCAPABLE OF FAILING ITS TEST. Every Explorer double-click opened a **separate process** in the installed Windows app — two windows, two live-reload monitors, two buffers able to save over each other — and reproduced on no developer machine, with `register()` Ok and `is_remote()` false, i.e. #174's exact silent signature on the platform #174 had cleared. Cause: GIO has **no Win32 single-instance backend** (four places in the tree said it did, inheriting the very belief #174 recorded as the macOS error); uniqueness rides a D-Bus session bus on Windows too, and GLib autolaunches that bus by **spawning `gdbus.exe`** from beside the loaded GLib DLL — a file `stage.ps1` never shipped, because its dependency manifest was a rigorous list of *libraries* and the missing thing was an *executable* nothing links against. The test could not catch it: single-instance items run `target\release\...` from a shell with gvsbuild's `bin` on `PATH`, so they pass whether or not the redistributable contains the helper — a green suite that was evidence about `PATH` → ask what a backend physically **is**, and when the answer is a file, treat the capability as a packaging obligation; a backend can exist in your build environment and be absent from your product. MEASURED: staged tree 2 processes, +`gdbus.exe` 1, two staged copies at different paths 1 (so install location was a red herring); the daemon runs from the install's own `bin\` and self-terminates when the last client drops, so it cannot wedge an uninstall. Corollaries: a manifest listing only libraries loses helper executables silently (no loader error names them), and at least one item per capability must run against the **assembled** artefact from a `PATH` scrubbed of the build environment. Kin #174 (prove the backend exists — this is the case it missed), #239 (verifying against the right artefact) and #234 (a suite exercising only the entry point under which the defect is invisible) |
| 250 | A WIDGET SWAPPED IN FOR ONE FEATURE'S SAKE MOVES ITS TEXT OUT OF EVERY TEXT-WALKER'S REACH, AND THE WALKERS FAIL SILENTLY. A table cell that is nothing but a link is a `GtkLinkButton` rather than a `GtkLabel` (#4 — Pango `<a href>` has no hover cursor and fires on press), which puts its caption in a label INSIDE the button. Find-in-preview enumerates cell text by downcasting each of a table's DIRECT children to `GtkLabel` (cell text is in labels, not the buffer — #36), so a link cell matched nothing: the reader sees "Handbook" on the page and the find bar says "No matches", while the same word in a MIXED cell (still a label) matches — the inconsistency reads as random, and the code recorded the exclusion as deliberate ("not selectable labels… keeping the count equal to what find can navigate to"), which is a real property of the OLD walk and no reason at all not to reach the text. Neither widget choice was wrong; nothing connected them, and a container-child type test is a **structural** predicate standing in for a **semantic** question ("does this cell have text?") → make the swap-in and the read-back ONE seam (`link_cell_button`/`link_cell_caption` in `widgets::table::linkcell`) and ban the raw constructor so the next link widget cannot be built without a way to read its text back; when a walk answers a semantic question by downcasting to a concrete type, enumerate every widget shape that can carry that content, and treat "we deliberately skip X" written next to a structural test as a claim to re-derive, not inherit. TRAP in the fix: the caption label must be installed as ESCAPED MARKUP, because find's cell path forces an anchored-child repaint by toggling a `<span>` wrapper around the label's own markup (#37/#117) and a plain-text caption containing `&`/`<` would then fail `pango_parse_markup` and render EMPTY (#163) — a fix that repairs find and silently blanks captions. MEASURED (4.6.9/Xvfb): pre-fix 1 of 2 link captions found (mixed cell only), post-fix 2, and the mutation (restore the direct-children walk) returns it to 1. Kin #36 (cell text is not in the buffer) and #235 (a capability wired into one of several shapes of the same thing). CAM: this is Document Rendering row 8 x row 2, written down and unmet — the row POSTDATES the code (row 2026-07-13, cell 2026-06-23), so it applied to future changes and to nothing already shipped; adding a row now obliges a back-sweep (CAM.md governing rules) |
| 251 | A DISTRIBUTION GTK HAS ITS ENTIRE INTROSPECTION SURFACE COMPILED OUT, AND THE THREE CHANNELS FAIL IN THREE DIFFERENT WAYS — ONE OF THEM BY REPORTING A NUMBER THAT MEANS "HEALTHY". MEASURED on Ubuntu jammy GTK 4.6.9 / GLib 2.72.4: every informational `GTK_DEBUG`/`GDK_DEBUG`/`GSK_DEBUG` key reports `[unavailable]` (only `interactive` survives), `g_type_get_instance_count()` is exported and always returns **0** with no diagnostic, and neither `libgtk-4` nor `libglib-2.0` links `libsysprof-capture`, so the toolkit emits no profiler marks at all → a profiling or leak-hunting plan that names any of them is unrunnable here, and the instance-count one produces a false all-clear. Plan around app-owned instrumentation plus process-external sampling; verify a channel EMITS before trusting its silence |
| 247 | "NO HANDLER IS REGISTERED FOR THIS SCHEME" IS NOT A SAFETY PROPERTY — IT IS A CLAIM ABOUT EVERY MACHINE THE TEST WILL EVER RUN ON. A control test deliberately let `GtkLinkButton`'s default `activate-link` handler run, using `x-scribobulate-no-such-handler:///probe` on the reasoning that an unhandled scheme "fails to launch instead of opening something". True on Linux (a `g_warning`, nothing else). **False on Windows**: the shell answers an UNREGISTERED scheme with a modal "You'll need a new app to open this" chooser, hosted by `SystemSettings`/`ApplicationFrameHost` — a DIFFERENT PROCESS, so it outlives the test binary, is absent from your own window list, holds foreground focus (silently swallowing keystrokes aimed at any later driven UI run — cost a full cycle before the focus theft was traced back to it), ignores `WM_CLOSE` and Escape because it is a UWP surface, and presents OK beside an already-ticked "Always use this app", i.e. one stray click from writing a scheme handler into `HKCU`. **A test suite reached out of its process and could have reconfigured the host.** → make the probe an **invalid URI**, not an unclaimed one: `gtk_uri_launcher_launch` runs `g_uri_is_valid` and returns an error before `gtk_show_uri_full` (gtk-4.22.4 `gtk/gtkurilauncher.c:333`, MEASURED from source; `gtk_link_button_set_visited(…, TRUE)` still runs unconditionally afterwards, so a `visited`-based oracle is unaffected), and assert that invalidity in the suite so the property cannot rot back into a comment. The rule: **prefer a safety claim about your own dependency over one about the host** — the first is checkable in-repo on every run, the second is unfalsifiable and fails on exactly the machine you did not test on. Second-order, and the reason this sat undetected: the Linux failure mode is *silence*, so the seat that could most easily have found it is the one structurally unable to. Kin #237/#242 (a platform-shaped hole in what a gate can see) |
| 252 | A DRIVE STEP ROUTED THROUGH AN APP COMMAND INHERITS THAT COMMAND'S OWN ENABLEMENT GATE, AND A DISABLED `GAction` SWALLOWS IT IN SILENCE. A live drive alternated "place the caret with the app's own Go To Line" and "click the toolbar button under test"; the click moved focus to the toolbar, `win.go-to-line` is gated on EDITOR focus, and `g_simple_action_activate` returns without emitting when an action is disabled — so from the second iteration the caret never moved, no dialog appeared, nothing logged, and every later assertion was evaluated against the PREVIOUS position. Worse than a dropped input: the stale run produced the CORRECT ANSWER FOR THE OLD LINE, which is exactly what a broken gate would produce, so the false reading was indistinguishable from a real defect — caught only by cropping the footer Ln/Col indicator and seeing `Ln 5` where the script believed `Ln 3` → verify a setup step by its OWN observable (Ln/Col, title, match count), never by its exit status, and prefer a setup primitive with NO enablement gate (a click into the text moves the caret unconditionally) — a harness step must not depend on the subsystem under test being in a particular state. Second-order: two assertions sharing one stale precondition AGREE WITH EACH OTHER, so a self-consistent pair of results is not evidence either was measured. SECOND INSTANCE, same session, different mechanism: chaining `mousemove`+`mousedown` in ONE `xdotool` invocation delivered a press the app's gesture handler NEVER RAN FOR (proved by instrumenting the handler — 3 calls for 4 clicks) while a context menu still appeared, so the capture showed a menu built for the PREVIOUS pointer position and the feature read as broken on one link and fine on its neighbour; separate the move from the press with a settle. An input that is DELIVERED is not an input that was HANDLED — instrument the handler rather than re-reading the screenshot. Kin #245 (the same silence one layer down, at the input channel), #217, #239 |


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

**Symptom**: a link rendered via Pango `<a href>` in a `GtkLabel` styles and activates, but with no pointer cursor on hover and activation on button-*press* rather than *release*.
**Root cause**: `GtkLabel` handles `<a href>` without `GtkLinkButton`'s full interaction model (no hover cursor; `activate-link` wired to press).
**Resolution**: for a cell that IS a single link, use `GtkLinkButton` (`has_frame = false`). Pango `<a href>` stays correct for an *inline* link inside mixed-content label text (bold/italic/link interleaved), where `connect_activate_link` is the open hook.

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
**Now enforced by**: `saferizer::QdataKey<T>` — the remaining untyped-qdata keys are typed consts (render data, labels, anchor widgets, cell copymap, copy-primary handler).
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
**Now enforced by**: `saferizer::ViewportTopIter::of`/`top_offset` and `ViewportRange::of` — the one place the viewport-top/range iter is read; 4 duplicated 2-liners retired.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (§15).

## 16. Mirroring split-pane scroll synchronously inside `value-changed`
**Symptom**: mirroring a follower pane synchronously in `value-changed` oscillates wildly during the re-render validation thrash.
**Scribobulate**: a coalesced once-per-frame scroll-sync projection (GtkSourceMap pattern), on `value-changed` AND `notify::upper`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (§16); findings: scroll-sync-validation-coalescing.md.

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
**See**: gtk4-rs skill → textview-layout-and-drawing (§6/§21).

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
**Now enforced by**: `saferizer::popover_anchor`, one seam with two public shapes and no third way in. `ViewportRect::at` is the caret-sliver anchor and `pin_above` its sole sink (guard + clamp + the `width = 1` rect + `Top` placement, all in one place); `on_viewport` is the full-rectangle form for a popover anchored to something with real extent, and it RETURNS the clamped rect rather than a bool. The predicate itself (`anchor_visible`) is now module-private, which is the actual enforcement: the guard and the clamp are two halves of one contract, and every previous bug here was a caller taking one without the other. Three call sites had hand-rolled the pair — the preview selection popover, the editbar format overlay (its own inline guard+clamp+rect), and the annotation card, which asked a bare predicate and then pointed at the UNCLAMPED rectangle, passing the guard and still handing GTK a negative y whenever its chip straddled the top edge. All three now route through the seam and that last one is fixed. Both shapes are unit-tested and mutation-tested. Pointer/click-anchored popovers (context menus) pass click-coordinate rects and are out of scope.
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
**There is NO hole here — a previous edition of this entry claimed one, and it was wrong.** For the record, so nobody rediscovers the retracted claim and "fixes" it: it was asserted (from a source read, never observed) that `gdk_clipboard_set_content` early-returns without emitting on an unchanged provider, and that `GtkLabel` reuses one stable provider, so extending a selection *within* one cell label would silently miss `::changed`. The early-return is real; the conclusion is not. `gtklabel.c:5038` — **one line above** the `set_content` call the claim was reasoned from — calls `gdk_content_provider_content_changed`, which emits **unconditionally** (`gdkclipboard.c:209`, outside the `if (priv->content != content)` guard). So a `GtkLabel` fires `::changed` on **every** selection delta and this corrective is sound as written. See **#135** for the per-widget table (`GtkLabel` and `GtkTextBuffer` call `content_changed`; **`GtkText` never does** — that one really is transitions-only), and for what this near-miss cost.
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
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-57, incl. the unresolvable-vs-refused enum split, GTK4Rs/AP-34b); findings: researcher-findings-image-src-path-containment.md.

## 32. Anchoring a `GtkPicture` in a `GtkTextView` without a nonzero width request
**Symptom**: a Markdown image loads (valid `GdkTexture`) but the anchored `GtkPicture` renders BLANK (height 0) — `can_shrink` reports `min_width = 0`, so `GtkTextView` measures height at `for_size = 0` → 0 (image face of ScrAP-22/ScrAP-23).
**Scribobulate**: the image-rendering path sets a definite `set_size_request(seed_w, seed_h)`; the view's `size_allocate` re-clamps it to the live column on each real width change — `w = min(natural, content)`, `h = max_h·w/max_w` (aspect preserved, `max-width: 100%`, no upscaling past natural).
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-58); findings: researcher-findings-anchored-picture-blank.md.

## 33. Testing a rebuilt single-instance GApplication while a stale primary is still running
**Symptom**: a rebuilt binary appears to have NO effect — it behaves like an older build.
**Scribobulate**: launch with `--new-instance` / `-n` (ScrAP-17) when verifying a change interactively, or quit the running primary first; headless smoke tests already use `-n`.
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-43).

## 34. Remote image loading blocks the main thread; "refused" ≠ "unresolvable" in a multi-outcome resolution enum
**Symptom (34a)**: `Texture::from_file(&gio::File::for_uri(url))` is synchronous even for a remote URI — rendering remote images freezes the UI per fetch. **(34b)**: an untitled buffer showed a broken-image icon (implies "refused") for an unresolvable relative `src` because the gate collapsed both `None` reasons into `Refused`.
**Scribobulate**: 34a accepted for the opt-in "Show Unsafe Images" path (the image-tag rendering site); 34b — the image-resolution routine early-returns `Missing` when the document directory is unset and the source path is relative, reserving `Refused` for a present-but-escaped path.
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
**See**: project-specific; the fix + rationale live in a code comment at the split-mode preview re-render site.

## 38. Driving derived UI state from a delta-only signal, missing lifecycle boundary events
**Symptom**: find-bar highlights absent when the bar is reopened with the same term (no new typing → no `search-changed` to drive them); the outline scroll-spy's `value-changed`-derived highlight similarly stayed stale across a mode/window boundary that preserves scroll position but fires no delta. **The same lesson bit again at THREE more boundaries** (2026-07-19): the preview find-match highlights (`scrib-search-hl` buffer tags + table-cell Pango attrs) were erased by **theme switch**, **view-mode switch** (edit↔split↔preview), and **external reload** — each installs a FRESH preview `GtkTextBuffer`, which carries none of the tags, so the matches vanished until the user next edited the query or stepped a match (which re-runs `search-changed`). The highlight is derived state layered on the buffer; every boundary that rebuilds the buffer must re-apply it, exactly as `refresh_outline`/`refresh_annotations` already were in those same sweeps — find was the overlay left out.
**Scribobulate**: the find bar re-runs its search-changed logic on reveal, the outline scroll-spy fires an explicit deferred initial scroll in every mode, and the preview find-highlight re-sync now runs at **every** preview-rebuild boundary — tab switch (`tabs/switch.rs`, pre-existing), theme sweep (`re_render_all_windows`), mode switch (`viewactions.rs`), and external reload (`reload.rs`).
**Now enforced by**: one shared choke-point helper `window::refresh_preview_find_highlight` (`findbar.rs`) that every boundary calls — no per-site re-implementation to drift (the ScrAP-116 shape). Guarded by three mutation-checked `#[gtk::test]`s (`theme_re_render_…`, `mode_switch_…`, `external_reload_preserves_preview_find_highlights`), each of which fails if its boundary's call is removed. Closes the POLICY Document Rendering CAM row-8 gap (find's own highlights must survive the re-render boundaries, not just match through the markup).
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-47); POLICY Document Rendering CAM row 8.

## 39. Specifying GNOME-specific icon names absent from non-GNOME themes
**Symptom**: toolbar icons render as a ⚠ placeholder — the name is a valid Adwaita icon but Adwaita is not in the active theme's inheritance chain (e.g. `breeze-dark → breeze → hicolor`).
**Scribobulate**: the split-arrangement buttons and the view-command table use icon names confirmed present in `breeze-dark/actions/symbolic/` as well as Adwaita.
**Now enforced by**: `icons::Icon` enum — every fixed icon name is a variant with a `const name()`, and a `#[gtk::test]` asserts each RESOLVES via `has_icon` in the bundled theme (resolution, NOT visible render — ScrAP-169). Landing this test **caught a live bug**: the Show-Unsafe-Images toggle used `image-x-generic-symbolic`, an Adwaita *mimetypes*-category name **absent from Breeze/Breeze-dark**, so it rendered `image-missing` on the operator's real desktop; fixed to `emblem-photos-symbolic` (present in breeze, breeze-dark, Adwaita). Lesson (ScrAP-39 kin): a freedesktop name present in ONE theme's category dir can be absent from the host theme, and a symbolic name's fallback chain does NOT cross into an unrelated theme (Breeze ↛ Adwaita) — verify against the *host* theme, not just Adwaita.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-48); inverse hazard (bundled name PRESENT in the host theme → theme overrides your bundle) — see #85.

## 40. `GtkAboutDialog` `authors` entries with `<url>` format open `mailto:`
**Symptom**: clicking a link in the Credits tab of `GtkAboutDialog` launches the default email client with a `mailto:https://…` URI instead of the browser.
**Scribobulate**: the About-dialog action — removed the `<url>` from `authors`; use `.website()` + `.website_label()` instead.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-50).

## 41. `FileChooserNative`/transient-dialog lifetime is backend- and widget-type-dependent
**Symptom**: a leak fix that uniformly converted every dialog's self-closure from strong `.clone()` to weak `.downgrade()` broke every `FileChooserNative` dialog outright (failed to appear/respond).
**Scribobulate**: split by type — `gtk::Window` dialogs keep a weak self-ref (toplevel list pins them) while any `NativeDialog` keeps ONE strong ref in an external `Rc<RefCell<Option<…>>>` holder, dropped in `connect_response` after `.destroy()`.
**Now enforced by**: `saferizer::NativeDialogHolder::show` — owns the sole external strong ref and drops it after `connect_response`, replacing 3 hand-copied protocol+rationale copies.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-41).

## 42. Predictable, reused path under the shared temp dir for a config-redirect workaround (security)
**Symptom**: a fixed, predictable, world-writable path (`temp_dir().join(...)`) trusted as "not yet created" lets a local attacker pre-plant `settings.ini`, achieving code execution via its `dlopen()`-capable `gtk-modules` key.
**Scribobulate**: the temp-dir helper prefers `$XDG_RUNTIME_DIR` (0700) and makes a PID+timestamp dir with exclusive no-clobber semantics (`DirBuilder::mode(0o700).create`, fails on `AlreadyExists`).
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-8, the predictable-temp-path security subsection).

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
**Second, independent cause — a native boundary, not a keybinding**: an ancestor key controller also never fires while focus is inside a **popover**, because key events originating in a popup surface are bounded at the `GtkNative` exactly as pointer events are. Measured on 4.6.9 (researcher, round 4), and note the source predicts the opposite: `rewrite_event_for_toplevel` in `gtkmain.c` explicitly re-targets key events from a popup surface to the toplevel — but `handle_key_event` restarts propagation from `gtk_root_get_focus(root)`, which is the widget *inside* the popover, so it meets the same bound. Rewriting the surface does not rewrite the propagation origin. **So: never hang Escape (or any key-driven dismissal) for a popover on an ancestor or toplevel controller — put it on the popover itself.** A controller on an ancestor is not a backstop here, it is dead code.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-53).

## 49. `cargo valgrind` on a GTK4 app reports hundreds of toolkit-internal "leak"/uninitialised-value errors that are NOT application bugs — but valgrind still catches real app UAFs here, so triage by stack, don't dismiss wholesale
**Symptom**: `cargo valgrind run` against a live window reports ~700 errors whose stacks bottom out in `gtk_at_context_create`, Pango, Fontconfig, and IM compose code — symbol-less, none naming a `scribobulate::` frame.
**Scribobulate**: for that ~700-error live-window run, grepping every stack for a `scribobulate::` frame found zero — GTK4's by-design OS-reclaims-at-exit retained memory absent a `gtk.supp`/`glib.supp` suppression file. But the technique is triage, not blanket dismissal: valgrind DID pinpoint a real read-after-free on this project — the reused `GtkSourceView` gutter's unbound `vadjustment` binding fired against a tree mid-teardown (#58, "Valgrind proved the read-after-free directly"). A stack that names a `scribobulate::` frame is signal, not noise.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-59).

## 50. `GtkNotebook`'s native cross-window tab-detach DnD is unsafe on GTK 4.6.9 — a NULL deref inside GTK's own `dnd_finished_cb`, not (only) a freed source notebook
**Symptom**: dragging the ONLY tab of window A onto window B (native group-name drag) intermittently `SIGSEGV`s — a live GTK bug: a local drag makes X11's GDK finish synchronously, whose `gtk_notebook_dnd_finished_cb` derefs an already-NULL `detached_tab` in the unguarded `rootwindow_drop` branch.
**Scribobulate**: stop using native detach — `set_group_name(None)` + `set_tab_detachable(false)`; reimplement cross-window move with a Shift-gated custom `GtkDragSource`/`GtkDropTarget`.
**Superseded**: a self-contained custom tab-bar/tab-view widget replaced `GtkNotebook` outright — this crash class can no longer occur by construction.
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
**Root cause chain**: round 1 — flipping a sibling's `:visible` from inside a container's own `size_allocate` mutates a tree GTK is mid-way through allocating (the same "don't mutate the tree during layout" family as #30). Round 2 — `show()`'s `queue_resize(parent)` is unconditional and recursive, so even "my own child, allocated immediately after" propagates `alloc_needed` up through a shared `GtkOverlay`. Round 3 — a 0-width real (CSS-padded) widget allocation goes negative internally. Rounds 4-5 (the true root) — a homogeneous `GtkStack`'s `set_visible_child` issues only a non-bubbling `queue_allocate`, so a lazily-validating `GtkTextView` child's FIRST show can dirty a shared ancestor AFTER the allocation descent has already passed it, leaving the overlay stuck blank until an external resize.
**Scribobulate**: chevrons are hidden by GEOMETRY (`size_allocate`d out of the clip region), never `:visible` or a degenerate size; the tab-view container's construction sets `stack.set_hhomogeneous(false)`/`vhomogeneous(false)` (bubbling `queue_resize`, clean pre-layout schedule) with `transition-type=NONE`, and the content `GtkScrolledWindow` stays vertically decoupled so the resulting reflow is absorbed as a scroll-range change, not a visible jump.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-104); findings: researcher-findings-sibling-visibility-toggle-in-size-allocate-snapshot-warning.md, researcher-findings-gtkoverlay-snapshot-without-allocation-during-tick-animation.md, researcher-findings-textview-first-show-validation-blank-in-gtkstack-page.md.

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
**Root cause**: a `GtkScrolledWindow`'s natural height is small by design; a `GtkBox` packs a child at its natural size unless it sets `vexpand`, so the scroller got a ~2-line allocation and `GtkTextView`'s lazy validation painted only that.
**Scribobulate**: the split-view container's construction sets `hexpand`/`vexpand` on the persistent widget itself (expand flags don't transfer when consolidating several individually-expanding widgets into one container).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-105).

## 60. A closure owned by a widget's own machinery that strong-captures self (or an ancestor) is an uncollectable cycle — on window close it strands the entire descendant subtree
**Symptom**: no crash, no warning — a slow resource leak. Closing a `GtkApplicationWindow` doesn't reclaim its content subtree even though the WINDOW GObject itself finalizes cleanly.
**Root cause (GTK 4.6.9)**: `gtk_window_destroy` never disposes the widget tree — it unrealizes and relies purely on refcount→0. A closure owned by a widget's OWN machinery (a signal handler connected on self, a tick callback, a controller added to self) that strong-captures that same widget is freed only at `dispose`/`finalize`, which the strong ref prevents forever — an uncollectable cycle.
**Scribobulate**: the window-chrome build step's `content_paned.connect_map(move |_| …)` had captured a strong clone of the SAME paned; fixed by using the handler's own emitter argument (`move |paned| …`) instead. Two secondary self-cycles in the custom tab-bar/tab-view widgets fixed the same way (weak-captured).
**Now enforced by**: convention + POLICY (Code style) — widget-owned closures use `glib::clone!(#[weak] …)`, never a strong self-capture; the idiom is now ambient (112 sites adopted, the hand-rolled `downgrade()`/`upgrade()` pattern retired everywhere it converts cleanly). See #154 for the sites that must stay hand-rolled.
**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63, the general self-capture kernel, shared with GTK4Rs/AP-51); findings: researcher-findings-window-subtree-never-finalizes-teardown-leak.md.

## 61. Building N `GtkMenuButton` menu-models in a synchronous startup burst forces N×items accelerator-label font resolutions → a multi-second UI freeze
**Symptom**: opening many documents/tabs at once froze input for several seconds a few frames after first paint, scaling with tab count not document size; `perf` fingerprinted `FcFontSetSort`/fontconfig inside `gtk_accelerator_get_label`.
**Root cause (GTK 4.6.9)**: `gtk_menu_button_set_menu_model` builds the WHOLE `GtkPopoverMenu` eagerly (not lazily on first popup); each accelerated item's label forces a pango/fontconfig font-match on its first layout — N menus × M items multiplied it into one synchronous burst.
**Scribobulate**: exactly ONE shared caret-format overlay per window, re-parented onto the active tab's editor per switch — one heading-menu materialization ever, independent of tab count.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-106); app-lifecycle-and-env. Findings: researcher-findings-popover-set-parent-superlinear-startup-freeze.md.

## 62. A custom tab/stack widget leaves its active-index model unset for the default-visible first page
**Symptom**: moving/closing the FIRST tab of a window (never explicitly switched to) left the source window with a blank content pane and the moved tab's stale outline — a "phantom tab".
**Root cause**: the custom tab-bar/tab-view widget tracks the active tab in its own `Cell<Option<usize>>`, set only by its switch-to-index method — but a window's INITIAL page is shown by the `GtkStack`'s default (first child visible) and never travels through that path.
**Scribobulate**: a dedicated first-page-active marker, called once right after appending the first page, sets the active slot to `Some(0)` WITHOUT firing a switch callback.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-75); related #55 (GTK4Rs/AP-74), #56.

## 63. A shared app-level menubar model can't carry per-window content — a per-window submenu needs a self-built `GtkPopoverMenuBar` + selection-as-action-state + deferred `GMenu` mutation
**Symptom**: a `View ▸ Documents` per-window tab-list submenu can't be built by mutating the app's shared `Documents` `gio::Menu` — every window's menubar renders the SAME model.
**Root cause (GTK 4.6.9)**: `app.set_menubar()` sets ONE `GMenuModel` shared by every window's `GtkPopoverMenuBar`; `GAction` STATE resolves per-window for free, but model CONTENT does not. Mutating a `GMenu` bound to a live menubar from inside a menu item's OWN activation can free the just-clicked button (UAF).
**Scribobulate**: each `ApplicationWindow` self-builds its own `GtkPopoverMenuBar::from_model` (drops `app.set_menubar()`); "which tab is active" is a stateful radio `win.select-tab` action (a switch mutates NO menu content); `Documents` rebuilds are coalesced behind a dirty flag into a single `idle_add_local`.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-76); app-lifecycle-and-env (`set_menubar` D-Bus export, F10 self-registration). Findings: researcher-findings-per-window-menubar-documents-submenu.md.

## 64. A process-global CSS provider with an unscoped selector collides across windows (last-loaded wins)
**Symptom**: with ≥2 windows open, preview zoom shifted content but did not resize text in every window except the last-built one; opening a second window instantly reset the first window's zoomed font.
**Root cause**: `add_provider_for_display` is process-global; two per-window providers using an UNSCOPED selector at equal priority let the LAST-LOADED provider win everywhere. Zoom's other half (per-window pixel margins, in Rust) stayed genuinely per-window, so the two halves desynced.
**Scribobulate**: each window's rule is scoped to a per-window CSS class; a cross-window tab move ALSO re-renders the arriving tab's pixel geometry at the destination zoom (a dedicated tab-arrival wiring step) — the CSS half self-heals via tree-matching, the imperative half needed an explicit re-sync.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-77); app-lifecycle-and-env (CSS-provider display lifecycle).

## 65. Preserving a `GtkTextView` reading position across a re-render drifts, and can wedge input dead
**Symptom**: (a) drift — a preview zoom step nudged the reading position toward the top on repeated/fast zooms; (b) wedge — intermittently, mouse wheel AND PageUp/PageDown both went input-dead after a mode switch right after a zoom.
**Root cause (GTK 4.6.9)**: (a) the restore captured a PIXEL fraction and re-applied it as a LINE fraction, reading `value`/`upper` right after `set_buffer` (biased low, not yet settled). (b) `scroll_to_mark` scrolls by ANIMATION, and `size_allocate` skips refreshing the adjustment WHILE animating — a coincident relayout can freeze a collapsed scroll range.
**Scribobulate**: restore anchors to a buffer LINE via a persistent mark + deferred `scroll_to_mark`, followed (for the generic editor restore) by a non-animating `set_value` clamp; the preview view caches the reading line only while a `user_scrolling` flag is set, immune to mid-animation reads during rapid zoom.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (the deferred-work/GTK4Rs/AP-14 family); findings: researcher-findings-textview-scroll-to-mark-pending-scroll-wedge.md.

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
**See**: gtk4-rs skill → textview-layout-and-drawing (companion note: hide→measure is synchronous cache invalidation, not lazy validation).

## 69. Putting mnemonic `_` markers in a command label shared across menu + tooltip + context-menu surfaces
**Symptom**: injecting `_` into the ONE shared `Cmd.label` (feeding menu, tooltip, and a hand-rolled context menu) leaked a literal underscore into tooltips and mis-set/hid an access key on the manual button.
**Root cause**: `_` is a mnemonic marker only in GTK menu-model label contexts — `set_tooltip_text` and a manual button label don't interpret it.
**Scribobulate**: mnemonics injected ONLY at menu-build time (a dedicated mnemonics table + helper function), reused by the context menus; the shared command label stays literal so toolbar tooltips are unaffected. Dedicated well-formedness/uniqueness guard tests catch drift.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-68). Findings: researcher-findings-popovermenubar-mnemonics.md.

## 70. Getting bare-letter access keys (with a visible underline) in a plain `GtkPopover` via mnemonics / use-underline
**Symptom**: a plain `GtkPopover` context menu (deliberately not `GtkPopoverMenu`) needs a BARE-letter access key with a visible underline; `Button::with_mnemonic`/`use-underline` requires Alt and only shows the underline while Alt is held.
**Root cause**: managed mnemonics default `mnemonics-modifiers = Alt`; `GtkPopoverMenu` gets bare letters only via private calls unavailable to a plain popover, and a label's `_` underline draws only while `mnemonics-visible` is set.
**Scribobulate**: a dedicated access-markup/access-shortcut helper builds a `GtkShortcutController` (Capture/Local phase) with one `KeyvalTrigger(keyval, NO modifiers)` per row, gated on `is_sensitive()`; the underline is drawn manually with Pango `<u>` markup.
**See**: gtk4-rs skill → controllers-and-bindings (companion note: bare-letter access keys, ShortcutController + KeyvalTrigger). Findings: researcher-findings-plain-popover-access-keys.md.

## 71. Nesting a submenu as a child `GtkPopover` inside a plain autohide `GtkPopover` context menu
**Symptom**: a submenu (Change Case ▸) as a child `GtkPopover`/`GtkMenuButton` popover parented to a row runs into the parent autohide popover's grab.
**Root cause**: nested override-redirect popup surfaces + stacked grabs are fragile under a per-invocation `set_parent`/`unparent`-on-`closed` lifecycle; `cascade-popdown` also defaults false.
**Scribobulate**: the context-menu implementation uses a single-surface `GtkStack` (`main`/submenu pages, `SlideLeftRight`) mirroring what `GtkPopoverMenu` itself does for submenus, omitting its spurious-scrollbar-causing `ScrolledWindow` wrap; access keys are page-gated (#70's controller, same physical key means different things per page).
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-69). Findings: researcher-findings-plain-popover-nested-submenu.md.

## 72. Gating/targeting a multi-pane action on the first-found view instead of the focused pane
**Symptom**: in split (two-`GtkTextView`) layout, Copy/Select All always acted on the editor — Copy stayed disabled while the PREVIEW held a selection.
**Root cause**: the shared window action resolved its view with a first-found tree-order lookup — the FIRST `GtkTextView` in tree order — for both enabled-state and activation; with two panes visible that silently ignores which one the user is working in.
**Scribobulate**: a dedicated focused-text-view resolver tracks a sticky focused pane (updated by `focus-widget-notify`, ignoring transient popovers/find-bar exactly like ScrAP-20), falling back to the single view otherwise.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-70, the multi-pane sibling of GTK4Rs/AP-20).

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
> *Core GTK — teach this in the gtk4-rs skill (GtkTextView layout/style cache). Researcher-sourced against GTK 4.6.9.*

**Symptom**: a `blockquote` tag (`left-margin`/`right-margin`) spanning several GtkTextView paragraphs renders the FIRST and LAST paragraphs correctly but drops the margin on toggle-free MIDDLE paragraphs — width-dependent, non-self-healing.
**Root cause (GTK 4.6.9)**: `get_style()`'s one-line style cache returns the PREVIOUS line's style WITHOUT consulting tags whenever the current line carries no tag toggle; only a line with an actual toggle (the first/last paragraph) invalidates it. Abutting same-tag ranges also COALESCE in the btree, so per-paragraph tagging over adjacent ranges is a no-op.
**Scribobulate**: a dedicated per-line tag-application step tags each logical line's CONTENT ONLY, leaving every terminating `\n` untagged — the untagged gaps prevent coalescing, so every line gets its own toggle.
**Refinement (2026-07-18)**: the per-line fix is silently defeated by ANY OTHER application of the *same* tag over the region that tags the terminating `\n`s. A `code-block` margin tag was applied correctly per-line, yet middle lines still lost their margin on uniformly-highlighted blocks — because a *second* loop (per-syntect-token) also applied `code-block`, and those token strings carried their trailing `\n`, so the newlines got tagged and the whole block re-COALESCED into one continuous run before the per-line pass could take effect. The block only rendered correctly by accident, when per-token `fg-*` colour toggles happened to land on every line. **Lesson**: when you apply a margin/paragraph tag per logical line, audit *every other* application of that same tag over the range and ensure none of them tags a terminating `\n` — one stray newline-tagging apply re-coalesces the block and reinstates the middle-line drop. The per-line step is necessary but not sufficient on its own.
**Now enforced by**: `renderer::emit::apply_tag_per_line` is the sole per-line applier; the redundant per-token `code-block` apply was removed (N4 fix, branch `typed-gtk-seams`).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-72). Findings: researcher-findings-textview-blockquote-left-margin-multipara.md.

## 77. UI-testing a formatter over the selectable read-only Preview pane
**Symptom**: a formatter click over a visibly-selected Preview pane silently no-opped for every command — a selection in a selectable READ-ONLY view looks identical to an editor selection, but the format action is correctly disabled there.
**See**: `src/window/editbar/focusgate.rs` — the window focus-widget gate (`connect_focus_widget_notify` + `is_ancestor`) that keys `win.format` on editor focus, not on selection presence; a read-only-preview selection therefore leaves it disabled and the format click no-ops. Sibling of #20 (the sticky focus-gate).

## 78. `Options::all()` (or any enabled-but-unhandled pulldown-cmark extension) silently DROPS constructs instead of degrading to literal text
> *Non-core (pulldown-cmark), full. Same family as #66/#75 — a parser-configuration trap, not a GTK lesson. Keep in-repo; do not fold into the gtk4-rs skill.*

**Symptom**: math (`$E=mc^2$`) rendered as nothing, footnote refs (`[^1]`) vanished, and YAML/`+++` frontmatter leaked into the body as a stray paragraph — silent content loss, no warning.
**Root cause**: `Options::all()` turns on EVERY pulldown-cmark extension, including ones the renderer has no handler for; the dispatcher's catch-all silently drops standalone events and leaks a container's inner `Text`.
**Resolution**: the Markdown-options setup is an explicit ALLOWLIST of only the extensions actually handled (`TABLES | TASKLISTS | SMART_PUNCTUATION | HEADING_ATTRIBUTES | GFM`); anything else degrades to literal `Text` rather than vanishing.

## 79. A container-level `GtkGestureClick` also fires on presses that land on a child `GtkButton` — a bar-wide "activate" gesture activates a tab even when the press was on its × close button
> *Core GTK4. Candidate for the gtk4-rs skill (gestures / event delivery).*

**Symptom**: clicking a BACKGROUND tab's × close button closed the right tab but also silently switched the active document — the closed tab's neighbour became active instead of the previously-active tab staying active.
**Root cause**: the custom tab-bar widget's single container-level `GtkGestureClick` hit-tests every press by x/y and calls its switch-to-index handler; GTK delivers the press in bubble phase, so a press ON the × button still reaches the bar's gesture, which fires BEFORE the button's own `clicked`.
**Scribobulate**: a dedicated close-button hit-test resolves the real target with `WidgetExt::pick()` and bails early when it (or an ancestor) carries the close-button CSS class.
**See**: gtk4-rs skill → controllers-and-bindings (GTK4Rs/AP-109).

## 80. Tracking a "reading line" only from a wheel `EventControllerScroll` misses scrollbar-drag and keyboard scrolling — the re-anchor goes stale
> *Core GTK4. Candidate for the gtk4-rs skill (scroll / adjustment lifecycle). Sibling of #65.*

**Symptom**: after a SCROLLBAR-drag or KEYBOARD scroll (not the mouse wheel), a subsequent zoom/reload re-anchor snapped the viewport back toward the top instead of holding the reading position.
**Root cause**: `user_scrolling` was set only by a wheel `GtkEventControllerScroll`; a scrollbar thumb-drag/trough-click and keyboard nav move the `GtkAdjustment` directly, emitting no scroll event, so `value-changed` never updated the cached reading line.
**Scribobulate**: a dedicated scroll-position-tracking wiring step also hooks a `GtkGestureClick` on the scrollbar and a scroll-key `GtkEventControllerKey`; every programmatic scroll resets the `user_scrolling` flag to false at its start, so a rapid-zoom burst's own animation frames stay excluded (burst-safe).
**See**: gtk4-rs skill → controllers-and-bindings / textview-scrolling-and-adjustments (input-source wiring companion, sibling of the GTK4Rs/AP-14 family / #65).

## 81. Persisting "all windows" from each window's `close-request` + a sequential-close quit loses every window but the last
> *Core GTK4. Candidate for the gtk4-rs skill (application / window lifecycle).*

**Symptom**: quitting with several windows open (a sequential `for w in app.windows() { w.close() }`, needed so `close-request` still fires the unsaved-changes prompt) persisted only the LAST-closed window's session.
**Root cause**: `app.windows()` is a live, SHRINKING view during the close/destroy sequence — each per-window `close-request` snapshot overwrites the file with fewer windows (last-writer-wins); `app.quit()` alone skips `close-request` entirely (silent data loss).
**Scribobulate**: a dedicated quit-all-windows routine snapshots the full window set ONCE up front, freezes session-save via a `thread_local` latch for the close sequence, and thaws it on a cancelled prompt / backed-out Save-As.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-113).

## 82. A one-shot `scroll_to_mark` restoring a FAR reading position onto a freshly-rebuilt GtkTextView lands near the top
> *Core GTK4. Candidate for the gtk4-rs skill (scroll / adjustment / layout-validation lifecycle). Sibling of #65/#80. Researcher-sourced from GTK 4.6.9 C source.*

**Symptom**: an external file reload rebuilds the preview from scratch (fresh `GtkTextView`/adjustment); restoring a FAR reading position on a very large document snapped near the TOP — the same mark + `scroll_to_mark` restore worked fine on a WARM view.
**Root cause (GTK 4.6.9)**: a fresh view's line heights start unvalidated (0); `scroll_to_mark`'s `flush_scroll` validates only a local ±2×height band; the scroll-y is a pure sum of cached (zero) heights above the target; `pending_scroll` is a ONE-SHOT, not self-re-applying.
**Scribobulate**: a dedicated fresh-view scroll-restore routine drives a PROGRESSIVE non-animating `set_value(line_yrange(mark).y)` off `notify::upper` until `line_at_y` converges; the one-shot `scroll_to_mark` is reserved for warm views (outline-nav, zoom, small docs).
**Caveat — zoom's "warm" classification is assumed, not verified.** A zoom `re_render` is itself a `set_buffer` swap (the whole preview is rebuilt), so the new view's line heights start unvalidated there too, exactly the fresh-view precondition this entry is about. "Warm" is classified off *view* maturity (mapped/allocated), not *buffer* maturity, so a far zoom on a very large document could in principle reproduce the land-near-the-top symptom. This has not been reproduced or ruled out — test it before treating zoom as definitely warm, and do not "fix" the zoom path on the assumption that it is broken without first confirming the symptom.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-115). Findings: researcher-findings-textview-far-scroll-fresh-unvalidated.md.

## 83. `GtkShortcutsWindow`'s programmatic `add_section`/`add_group`/`add_shortcut` API is GTK 4.14+ — on 4.6 you must build it from Builder XML
> *Core GTK4. Candidate for the gtk4-rs skill (menus / actions / app-lifecycle — help overlay). Version-availability trap; verified against the 4.6.9 runtime with `nm`.*

**Symptom**: the natural programmatic `add_section`/`add_group`/`add_shortcut` construction compiles cleanly against gtk4-rs 0.10 but references C symbols absent before GTK 4.14 — an undefined-symbol link/load failure on 4.6, not a graceful degradation.
**Root cause**: the gtk4-rs `doc(cfg(v4_14))` marker affects only docs.rs rendering, NOT a real compile gate; `nm -D libgtk-4.so.1` confirms the symbols are absent on 4.6.9.
**Resolution**: a dedicated shortcuts-window builder generates a `GtkBuilder` interface XML from the command tables (the stable `Buildable` path since GTK 4.0) and fetches the object; `set_help_overlay` (`GtkApplicationWindowExt`, present on 4.6) wires it.
**See**: gtk4-rs skill → versioning-and-features (GTK4Rs/AP-114).

## 84. `GtkTreeListModel` `autoexpand=true` makes true recursive Collapse all impossible; build `autoexpand=false` + explicit expand pass. Collapse DESTROYS the subtree (it does not cache expanded flags)
> *Core GTK4. Candidate for the gtk4-rs skill (lists-and-models — tree list). Source-grounded against `gtktreelistmodel.c` (GTK 4.6).*

**Symptom**: with `autoexpand=true`, Collapse-all only reaches "collapse to roots" — re-expanding a single root always springs its ENTIRE subtree open, never just its direct children.
**Root cause**: `autoexpand` recursively expands newly-created/added rows (`init_node`/`items_changed_cb`); `collapse_node` DESTROYS the descendant subtree (frees the child model + node rbtree) rather than caching expanded flags, so under `autoexpand=false` a re-expand always recreates only the DIRECT children, collapsed.
**Scribobulate**: model built `autoexpand=false` + an explicit forward-walk expand-all pass at build time for the default-open TOC; Collapse-all need only collapse the depth-0 roots (destroying each wipes everything below).
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-111).

## 85. A bundled (gresource) `*-symbolic` icon is only a fallback — a host theme that ships the same name overrides it
> *Core GTK4 (GtkIconTheme + gresource icon resolution). The inverse of #39.*

**Symptom**: redrawing a bundled `*-symbolic` SVG changed nothing on a real desktop whose theme (e.g. breeze-dark) ships the same icon name; a headless Adwaita screenshot deceptively showed the new bundled art.
**Root cause**: `add_resource_path` only registers a FALLBACK search location — the active theme chain is searched with higher priority and wins whenever it provides the requested name.
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
> *Core GTK4. Candidate for the gtk4-rs skill (lists-and-models — selection).*

**Symptom**: an outline scroll-spy's programmatic selection change, guarded by a transient bool bracketing the setter call, still spuriously navigated the preview whenever the user expanded/collapsed a tree node.
**Root cause**: `GtkSingleSelection` re-emits `notify::selected-item` OUTSIDE the synchronous setter call — once after the bool resets, and again per `items-changed` batch during an expand/collapse.
**Scribobulate**: TWO complementary per-tab guards, because the bool alone can't cover the async echoes — `outline_spy_selecting` (a transient `Cell<bool>`, `winstate/tab.rs`) catches the synchronous `notify::selected-item` inside `set_selected`, and `outline_spy_doc: Cell<Option<usize>>` (the doc **index** the spy currently owns, matched by equality — not GObject identity) catches the emissions `GtkSingleSelection` fires AFTER the bool resets: deferred, and again per `items-changed` during expand/collapse. The activation handler (`window/outline_nav.rs`) suppresses navigation when either fires; a genuine user click on a different heading matches neither, so navigation still works.
**See**: gtk4-rs skill → lists-and-models (GTK4Rs/AP-112).

## 90. A `GtkPopover` attached with `set_parent()` is NOT auto-unparented — the parent widget's `dispose()` must unparent it, or teardown floods "GtkPopover is not a child of …"
> *Core GTK4. Candidate for the gtk4-rs skill (widgets-and-lifecycle) — submitted to gtk4skiller.*

**Symptom**: a `gtk-integration-tests` teardown (any preview-view dispose) flooded 15+ identical `Gtk-WARNING: GtkPopover is not a child of …`.
**Root cause**: a `GtkPopover` attached via `set_parent()` is a `GtkNative` child; GTK's default widget `dispose` does NOT auto-unparent such children — the parent widget's own `dispose` must release them.
**Scribobulate**: THREE mechanisms, now with a unified handle for the hazardous case. (1) The view-parented persistent popovers (codeview marker popover + selection overlay, window format overlay) are owned by `saferizer::PersistentPopover` (Wave 7): its `teardown()` runs `popdown()`→`unparent()` in the one safe order (#123), guarded against double-unparent, called from the view's `ObjectImpl::dispose` — so "left parented at dispose" (this entry's flood) is unrepresentable through the handle. (2) The two transient context menus (`window/contextmenu.rs`, `window/tabs/contextmenu.rs`) `connect_closed(|p| p.unparent())` per invocation. (3) Custom containers (`SplitView`, tab widgets) unparent non-popover children via `widgets::unparent_all_children` in their own dispose.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-80).

## 91. An always-on scrollbar with default (overlay) scrolling floats over the `GtkTextView`'s right margin, stealing clicks meant for margin-drawn affordances
> *Core GTK4. Candidate for the gtk4-rs skill (scrolling-and-adjustments) — submitted to gtk4skiller.*

**Symptom**: right-margin CriticMarkup marker chips, painted and hit-tested correctly, never opened their popover on click.
**Root cause**: `GtkScrolledWindow` defaults to overlay scrolling — an always-visible bar does NOT reserve its own column, it floats over the child's right edge, exactly where the margin markers draw.
**Scribobulate**: the preview-rendering setup builds the preview scroller with `.overlay_scrolling(false)`.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-81).

## 92. A mutation path that edits the buffer but leans on a MODE-GATED live-preview refresh leaves the preview stale
**Symptom**: creating/editing/removing an annotation in preview-only mode wrote correct source but left the preview's highlights/markers/popover text stale until a manual reload.
**See**: project-specific; the fix + rationale live in a code comment at the mode-agnostic annotation re-render site.

## 93. Anchoring positions by pulldown-cmark source offset against ALL events maps onto a block-structure event whose range spans the whole block
> *Non-core (pulldown-cmark). Its `into_offset_iter()` gives a source *range* per event; a `Start(Paragraph)` can emit only the inter-block separator while its range spans the entire block.*

**Symptom**: a CriticMarkup comment marker/highlight placed by mapping a cleaned-source offset to a buffer position landed on the BLANK-LINE separator above its paragraph instead of the paragraph itself.
**Root cause**: pulldown's offset iterator reports the source range of the ENTIRE BLOCK for a block Start/End event, even though this renderer emits only the block separator there — an all-events offset lookup resolves a paragraph interior onto that misleading range.
**Resolution**: the preview-build offset-anchoring map is restricted to CONTENT events only (`Text`/`Code`/`Break`), excluding `Start`/`End` block-structure events.
**Non-core (pulldown-cmark) — do NOT fold into the gtk4-rs skill.**

## 94. A signal handler connected to a `GtkTextView`'s BUFFER is silently dropped when `set_buffer` swaps the buffer — re-wire buffer-dependent handlers on the new buffer
> *Core GTK4. Candidate for the gtk4-rs skill (textview / signals) — submitted to gtk4skiller.*

**Symptom**: the preview's "select → show Annotate overlay" worked for the FIRST annotation only — after one annotation, further selections drove nothing.
**Root cause**: the overlay's selection detection was wired via `view.buffer().connect_mark_set(...)` — a handler on the BUFFER; the in-place re-render swaps the buffer (`set_buffer`), finalizing the old one and dropping every handler on it.
**Scribobulate**: the annotation-overlay wiring re-invokes its selection-connect closure from `view.connect_notify_local("buffer", …)` — a VIEW-level hook that survives every swap.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GTK4Rs/AP-82, GtkTextView buffer-signal lifecycle).

## 95. A shown `GtkPopover` does not grow its surface when its child grows — pre-size it (homogeneous `GtkStack`), don't re-present it
> *Core GTK4. Candidate for the gtk4-rs skill (popover / sizing) — PENDING submission to gtk4skiller. Correction: an earlier draft also blamed the `popdown();popup()` re-present for focus theft and active-state warnings — those are separate causes (#98 and #96), proven independently. This entry is scoped to the sizing fact only.*

**Symptom**: swapping a popover's narrow button child for a wider comment `GtkEntry` clipped the entry — the popover kept the smaller child's surface width.
**Root cause**: a `GtkPopover`'s surface is sized at `popup()` and is NOT resized in place when its child's size request later grows.
**Scribobulate**: the live instance is the two-page context-menu popover (`window/contextmenu.rs`) — a single `GtkStack` built once with both pages present, relying on `GtkStack`'s *implicit* `hhomogeneous=true` default (it only overrides `vhomogeneous`/`interpolate-size`), so the popover pops up at the widest page's width from the first show and page swaps never resize the surface. (Prior revisions of this entry claimed an annotation-overlay `GtkStack` with an explicit `hhomogeneous=true` — a fabrication: `git log -S 'gtk::Stack' -- src/preview/` = 0 commits, and `hhomogeneous` is set only in the tab widget, to `false`. The annotation comment popover avoids the sizing hazard differently — it never re-presents; see #98.)
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-86).

## 96. Committing an action that rebuilds the widget subtree synchronously inside a `GtkButton` `clicked` handler breaks active-state accounting
> *Core GTK4. Candidate for the gtk4-rs skill (gestures / widget lifecycle) — PENDING submission to gtk4skiller.*

**Symptom**: committing an annotation (create/edit/remove) from a popover's Save button worked but logged ~11 "Broken accounting of active state for widget" lines up the document's ancestor chain.
**Root cause**: the commit synchronously rebuilt the widget subtree (`set_buffer` + render) INSIDE the `GtkButton` `clicked` handler — while the press gesture is still logically active, GTK is mid-way through updating `:active` state on the very ancestor chain the rebuild tore down.
**Scribobulate**: the annotation commit paths defer the rebuild with `glib::idle_add_local_once` so the gesture unwinds first.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-30 — rebuilding a widget inside its own event emission; defer with `idle_add_local_once`).

## 97. Inferring "inline vs block" from non-empty source delimiter bytes engulfs whole paragraphs
> *Non-core (pulldown-cmark / this project's copymap). A construct node's source "open"/"close" byte spans are non-empty for plenty of non-inline constructs.*

**Symptom**: annotating a single plain word in a paragraph highlighted the ENTIRE paragraph.
**Root cause**: `wrap_span` inferred "inline" from whether a node's source open/close delimiter byte-ranges were non-empty — a paragraph ALSO has non-empty trailing "close" bytes, so it was mis-flagged inline and taken whole.
**Resolution**: the copy-map's branch-node representation carries an explicit `inline: bool` set from the CONSTRUCT KIND at build time, never inferred from byte shape.
**Non-core (pulldown-cmark / this project's copymap) — do NOT fold into the gtk4-rs skill.**

## 98. A `GtkPopover` hosting a typing entry is unwinnable on X11 (autohide steals focus via its seat grab; non-autohide can drop clicks) — host a typing entry as an in-surface `GtkOverlay` child instead
> *CORRECTION (2026-07-13, researcher-confirmed vs gtk-4-6 — submitted to gtk4skiller). Supersedes the earlier "make it autohide(true)" resolution for any popover hosting a typing entry.*

**Symptom**: a preview "Annotate" overlay hosting a comment entry was unwinnable on X11 — `autohide(false)` dropped clicks/keys to whatever was beneath (WM-arbitrated, no grab, real WM only, invisible on WM-less Xvfb); `autohide(true)` took a real X11 seat grab whose popover-LOCAL coordinates resolve against the toplevel tree with the popover→window offset DROPPED, stealing focus onto the chrome underneath.
**Root cause (gdk/x11/gdksurface-x11.c:1901, gtkmain.c ~:1337)**: an autohide popover is a `GtkNative` but not a `GtkRoot`; GTK4's input path has no cross-surface offset term under that grab.
**Resolution**: host the TYPING entry as an in-surface `GtkOverlay` child (ordinary `grab_focus()`, no grab, no second origin); keep buttons-only chrome in a non-autohide popover if wanted.
**Scribobulate**: the preview-rendering setup wraps the preview `ScrolledWindow` in a `GtkOverlay`; the comment entry lives there (a dedicated in-surface overlay component); the Annotate action button stays a non-autohide popover.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-83).

## 99. A translucent text-tag highlight is painted over by a later opaque-background tag — GTK text-tag backgrounds don't composite; the highest-priority tag wins
> *Core GTK4. Candidate for the gtk4-rs skill (GtkTextTag / priority) — PENDING submission to gtk4skiller.*

**Symptom**: annotating a claim overlapping inline `code` wrote correct CriticMarkup and correct tag data, but the amber highlight was INVISIBLE — the code's opaque grey background painted over it.
**Root cause**: GTK resolves a character's background from the single HIGHEST-PRIORITY tag that specifies one — it does not composite a translucent higher tag over a lower one; the opaque `code-inline` tag (added later) outranked the translucent highlight tag.
**Scribobulate**: the tag-table setup raises the annotation-highlight tag to `table.size()-1` (top priority) after all other tags are added.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-84).

## 100. Measuring a widget while it is `visible=false` returns 0 — center an overlay child off a hidden measure and it collapses to a left-edge anchor
> *Core GTK4. Candidate for the gtk4-rs skill (widget sizing / GtkOverlay).*

**Symptom**: a comment-entry overlay card meant to CENTER on the selection midpoint sat to the RIGHT of the anchor — centering degenerated to a left-edge anchor.
**Root cause**: `gtk_widget_measure()` early-returns 0 for a non-visible, non-toplevel widget; the card's width was measured BEFORE it was made visible, so `bw=0` and `x = anchor − bw/2` collapsed to `x = anchor`.
**Scribobulate**: the annotation-overlay wiring shows the entry card BEFORE positioning it (measurement needs VISIBILITY, not allocation).
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-85).

## 101. UI-test tooling: kwin-on-Xvfb won't deliver a synthetic `xdotool` click to a non-autohide `GtkPopover` surface — verify such flows via a keyboard-triggerable action, not a synthetic popover click
> *UI-test tooling (non-core; not for the core gtk4-rs skill).*

**Symptom**: a synthetic `xdotool` click squarely on a non-autohide `GtkPopover`'s button, under kwin-on-Xvfb, never activated it — an established (working-on-real-KDE) popover was equally unresponsive under the same harness, proving it wasn't a code bug.
**Resolution**: drive such flows through a keyboard-triggerable `GAction` (accelerator) instead of a synthetic popover click; assert the flow's effect on the main window.
**Scribobulate**: the `win.annotate` GAction is the keyboard-triggerable path used to test entry-card positioning headlessly (also how ScrAP-102 was found and verified).
**See**: gtk4-rs skill → automated-UI-testing (ui-testing-interaction module).

## 102. Positioning a widget via `set_margin_*` then re-measuring it double-counts the margin — GTK folds a widget's own margins into `preferred_size()`/`measure()`
> *Core GTK4. Candidate for the gtk4-rs skill (widget sizing / margins / GtkOverlay).*

**Symptom**: a comment-entry card positioned by `set_margin_start`/`set_margin_top` landed correctly the FIRST show but drifted on every LATER show, jumping to the TOP near the viewport bottom.
**Root cause**: `preferred_size()`/`measure()` FOLDS a widget's own margins into its measured size — each placement pass measured `content + the PRIOR margin`, compounding across shows.
**Scribobulate**: the card-positioning routine zeroes `margin_start`/`margin_top` BEFORE measuring, then applies the freshly-computed margins.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-87).

## 103. Refreshing a `GtkTextView` via `set_buffer` for a change that leaves the rendered text identical repaints the whole document and jumps the scroll
> *Core GTK4. Candidate for the gtk4-rs skill (GtkTextView / rendering).*

**Symptom**: adding/removing a CriticMarkup annotation made the whole preview pane visibly JUMP — a full repaint plus a top-flash-then-restore — even though only decorations (tags/markers) changed, not the rendered text.
**Root cause**: `set_buffer` is a WHOLE-document replacement — GTK resets the view's adjustments to the top and repaints everything, regardless of whether the underlying text actually changed.
**Scribobulate**: a dedicated in-place annotation-refresh path re-tags + re-markers the LIVE buffer in place (no `set_buffer`) whenever the freshly-parsed text is structurally identical to what's on screen, falling back to a full re-render only if it isn't.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-90).

## 104. A persisted `GtkTextMark` re-resolved after a `set_buffer` swap is a cross-buffer footgun that aborts with `gtk_text_btree_line_number couldn't find line`
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkTextView / marks / deferred work).*

**Symptom**: a fatal, real-session-only crash (`gtk_text_btree_line_number couldn't find line` / SIGSEGV) on reload of a document carrying CriticMarkup annotations — never reproduced across an extensive headless Xvfb battery.
**Root cause (GTK 4.6 + gtk4-rs 0.10)**: a `GtkTextMark` persisted across a `set_buffer` swap becomes ORPHANED when its old buffer finalizes; `gtk_text_buffer_get_iter_at_mark` has NO mark∈buffer check and returns an UNINITIALISED iter on a deleted mark, which gtk4-rs surfaces with no `Option`/error.
**Scribobulate**: every persisted-mark resolution site guards `mark.buffer().as_ref() == Some(&view.buffer())` before resolving; mutation-tested (removing the guard reproduces the exact crash).
**Now enforced by**: `saferizer::BufferMark` — a named `{mark, buffer: WeakRef}` struct whose `resolve()`/`scroll_mark()` are membership-gated (return `None` on a foreign buffer); the `WeakRef` also avoids resurrecting the old buffer across a `set_buffer` swap (Wave 2).
**See**: gtk4-rs skill → textview-anchored-and-integration (sibling of GTK4Rs/AP-91, the persisted-mark family).

## 105. `iter_location` (any line-DISPLAY-caching geometry read) right after a `set_buffer` swap, before re-allocation, aborts with `gtk_text_btree_line_number couldn't find line`
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkTextView / scrolling / deferred work). The sibling of #104: same fatal g_error, same freed-`GtkTextLine` root, reached WITHOUT any mark.*

**Symptom**: the same fatal `couldn't find line` crash as #104, reached through GTK's own line-display cache — at three successive call sites in turn (our tick, our paint, then GTK's OWN `parent_snapshot`), real-session-only.
**Root cause**: `iter_location` builds+INSERTS a display into a `GSequence` cache sorted by line number; after a `set_buffer` swap, comparing against a still-cached entry from the freed OLD buffer's line calls `_gtk_text_line_get_number` on a dangling line. Upstream trigger: a redundant `set_text` on reload was missing the `st.loading` guard, letting the live-preview debounce fire a spurious `re_render` mid-settle.
**Scribobulate**: root fix — the reload-from-disk path sets a loading guard flag around the editor-load step; defense in depth — the scroll-sync and preview draw/snapshot paths read the cache-free `line_yrange` instead of `iter_location` on a possibly-just-swapped view.
**See**: gtk4-rs skill → textview-layout-and-drawing (sibling of the GTK4Rs/AP-89/GTK4Rs/AP-91 `set_buffer` cluster).

## 106. A selectable `GtkLabel` in a popover auto-selects all its text on open — the popover focuses it, and a selectable label selects-all on focus-in
> *Core GTK4. Candidate for the gtk4-rs skill (GtkLabel / GtkPopover / focus).*

**Symptom**: clicking a margin annotation marker opened its popover with the comment `GtkLabel` already fully selected, every time.
**Root cause**: `set_selectable(true)` makes a label focusable, and its focus-in handler runs select-all; a `GtkPopover` focuses its first focusable descendant on `popup()`, and the comment label was that descendant.
**Scribobulate**: the marker-popover builder drops `set_selectable(true)` on the comment label.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-107).

## 107. A menu-activated action that synchronously raises a focus-grabbing in-surface widget has its focus stolen by the menu popover's pop-down focus-restore — defer the raise to idle
> *Core GTK4. Strong candidate for the gtk4-rs skill (GAction / GtkPopoverMenu / focus / deferred work).*

**Symptom**: Edit ▸ Annotate did nothing from the MENU (flashed the comment card then it vanished) but worked perfectly from its keyboard accelerator.
**Root cause**: a `GtkPopoverMenu` item activation runs the `GAction` handler while the menu is STILL popping down; on pop-down GTK RESTORES focus to the pre-menu widget, stealing it back from whatever the handler synchronously focused.
**Scribobulate**: the Annotate action's registration defers the raise via `glib::idle_add_local_once` so the pop-down + focus-restore settle first.
**See**: gtk4-rs skill → actions-and-commands (GTK4Rs/AP-116).

## 108. `GtkTextBuffer::redo()`/`undo()` leaves no undo barrier — the next edit merges into the redone action's group, so one later Undo reverts two edits
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkTextBuffer / GtkTextHistory / undo).*

**Symptom**: annotate → Undo → Redo → annotate something ELSE → one later Undo reverted BOTH annotations, not just the last.
**Root cause**: `redo()`/`undo()` leave NO undo barrier afterward; `begin_user_action()` on the NEXT edit is too late — the merge happens as its insert/delete are recorded, before the matching `end_user_action` would set the barrier. The fix is an EMPTY `begin_user_action()`/`end_user_action()` pair flushed BEFORE the edit's own action opens. All three of Scribobulate's discrete-edit routines already wrapped their edit in a user action, so the missing piece — the empty pair flushed *first* — is invisible to anyone grepping for `begin_user_action`; that is why it was omitted at two of three sites and survived.
**Scribobulate**: **originally this entry claimed the flush happened "before each discrete edit" — it did not; it was at ONE of the three routines** (the annotation splice), while every format command (`editbar/edit.rs`) and smart-newline continuation (`editbar/newline.rs`) lacked it, so the double-revert was live on every format command and smart-newline for long after this entry was written. The contract is now a single RAII seam, `window::undo::UndoGroup` — its constructor flushes the barrier and opens the action, its `Drop` closes it — and raw `begin_user_action`/`end_user_action` are banned via `clippy.toml`'s `disallowed-methods`, so a new discrete-edit routine cannot re-introduce the merge (the same construction-contract shape as ScrAP-74's `slice()`-only buffer newtype).
**Meta-lesson worth keeping**: this entry described the fix as if it were universal ("before each discrete edit") when it held at exactly one call site. A note that *states a contract* should say **how many of the eligible sites actually satisfy it**, or a reader audits for the symptom, finds the one guarded site, and concludes the whole class is handled. A contract enforced at 1-of-3 sites reads identically to one enforced everywhere until you count.
**See**: gtk4-rs skill → textview-scrolling-and-adjustments (GtkTextBuffer/GtkTextHistory undo-barrier lesson).

## 109. Mapping GtkTextView buffer coords ↔ an anchored-child cell's interior under incremental allocation
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkTextView / GtkTextChildAnchor / allocation timing).*

**Symptom**: mapping a buffer position to the INTERIOR of an anchored-child table cell via `translate_coordinates(cell → view)` intermittently landed a marker/scroll a whole row too high, self-healing only on a full rebuild.
**Root cause (corrected, gtk4skiller content correction)**: (a) the "parked in snapshot" off-screen placeholder park opens ONLY when a `size_allocate` has intervened since the last paint — in STEADY STATE, a snapshot-time `cell → view` read returns REAL positions, so "always poisoned" is the wrong framing; (b) the OLD `scroll_to_mark`+idle recipe reads the PRE-scroll position at ALL distances because `scroll_to_mark` ANIMATES (gtktextview.c:2665) and the idle fires with `vadj=0.0` before the animation lands.
**Scribobulate**: a dedicated cell-row-geometry routine computes table-top buffer-Y from `line_yrange(iter_at_child_anchor(table_anchor))` (cache-free) PLUS `translate_coordinates(cell → table widget)` (a local, placeholder-immune subtree transform) — recomputed every frame in the draw/snapshot layer, no cache.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-91).

## 110. Driving selection-dependent UI for a selectable-`GtkLabel` cell (a selection island) — buffer signals never fire; use the primary clipboard, wired on the live view
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkLabel / primary clipboard / GAction sensitivity / signal lifecycle). Extends GTK4Rs/AP-28.*

**Symptom**: selecting text INSIDE a table cell drove neither the `win.annotate` action state nor the auto-showing selection overlay, though body-text selection drove both.
**Root cause**: a cell is a selectable `GtkLabel` — a selection island that fires NO buffer signal; the only cross-environment signal is the display-level PRIMARY CLIPBOARD's `changed` (ScrAP-28), and the hook meant to wire it early-returned at startup before the preview pane was mapped.
**Scribobulate**: the annotation-overlay wiring connects `view.primary_clipboard().connect_changed` PER-RENDER (guaranteed-live view), disconnected in `dispose`; cell selections clear on a genuine buffer-cursor placement (otherwise sticky).
**See**: gtk4-rs skill → textview-anchored-and-integration / actions-and-commands (extends GTK4Rs/AP-28's family).

## 111. The in-place buffer-tag refresh can't repaint an anchored-child cell decoration — reconcile the cell labels in place, unconditionally
**Symptom**: creating a cell annotation didn't show its amber highlight; removing the last cell annotation didn't clear it — both fixed only by an unrelated full re-render.
**See**: project-specific; the fix + rationale live in a code comment at the in-place annotation-refresh site (the anchored-child cell-label reconciliation).

## 112. `GDK_IS_SURFACE` criticals are a stale TOOLTIP timer over an unrealized grabbing popover — reuse popovers, don't destroy per use
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkPopover / GtkTooltip / surface lifecycle). Researcher-sourced, gtk-4-6.*

**Symptom**: interacting with popovers over a `GtkTextView` on real X11/kwin repeatedly logged `GDK_IS_SURFACE`-failed assertions — invisible on WM-less Xvfb even under `fatal-criticals`.
**Root cause**: a pending GTK TOOLTIP timer (armed by real motion landing on an autohide popover's grabbing surface, making it `tooltip->native`) fires `gtk_native_get_surface` on a popover that has since been UNREALIZED (destroyed per-use); `GtkPopover` never calls `gtk_tooltip_unset_surface`, so the timer's target goes NULL out from under it.
**Scribobulate**: the interactive/grabbing popovers OVER THE `GtkTextView` — the codeview marker popover and the selection-action/format overlay — are REUSED (created + `set_parent`'d once, only `popup()`/`popdown()`, content rebuilt per use), now behind `saferizer::PersistentPopover`; these are the surfaces where the assertion fired. The two transient CONTEXT menus (`window/contextmenu.rs`, `window/tabs/contextmenu.rs`) are the deliberate exception — `Popover::new()` + `set_parent` + `connect_closed(|p| p.unparent())` per right-click, NOT reused. Read the fix as scoped to the view-parented interactive popovers: the context menus rely on being short-lived, not on reuse.
**See**: gtk4-rs skill → state-and-subclassing (GTK4Rs/AP-117). Findings: researcher-findings-popover-tooltip-surface-assertion.md.

## 113. The first popup of a view-parented popover forces a one-shot table revalidation that scrolls the view and drops the click — pre-warm it
> *Core GTK4. Candidate for the gtk4-rs skill (GtkTextView validation timing / GtkPopover realize / lazy-init). Researcher-sourced, GTK 4.6.9 C-source trace.*

**Symptom**: clicking a marker chip in a tall table visibly SCROLLED the preview (toward the table top) and sometimes dropped the click — only on the FIRST activation of a session, or after annotating then popping up mid-table.
**Root cause (GTK 4.6.9)**: `popup()` of a view-parented popover forces a `size_allocate` whose `validate_onscreen` re-anchors to the first onscreen paragraph; a whole table is ONE paragraph, so a mid-table viewport re-anchors to the table's own top, shifting the clicked chip's hitbox out from under the grab.
**Scribobulate**: a dedicated popover pre-warm routine pre-warms the persistent popover once at first `map`, at scroll 0 (absorbs first-validation churn); the marker-popover open routine holds the saved vadj value across the popup's settle via a `value-changed` re-pin guard, wall-clock-bounded (`REPIN_GUARD_US`, 1.5 s — a `Deadline`, not a tick count; see #125) to span the deferred validation scroll, and disarmed the instant the user scrolls.

**The re-pin guard's saved value must be the POST-scroll one, and it must bracket only the `popup()`.** There is one animation slot per `GtkAdjustment`, and `set_value` beats an in-flight animation outright (it calls `end_updating` — gtkadjustment.c:532). So a guard that bracketed a whole scroll-then-open operation, or that reused a pre-warm's pre-scroll snapshot, re-pins the view straight back to where the navigation started — silently undoing it while every other assertion still passes. Note this is **not** caught by an end-to-end "the navigation worked" test: the guard only acts on `value-changed`, so on a fixture with no deferred validation scroll it never runs and the mutation survives. It needs a test that nudges the adjustment deliberately (`markers.rs` → `the_repin_guard_holds_the_post_scroll_position_not_the_pre_scroll_one`).
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-118).

## 114. An in-place live-buffer edit that skips the canonical source-of-truth vanishes on the next fresh render
**Symptom**: an annotation created in preview-only mode vanished on a mode switch, then reappeared on the next toggle.
**See**: project-specific; the fix + rationale live in a code comment at the mode-switch source-flush site.

## 115. Highlighting a char range in an existing Pango-markup string via `find` wraps the wrong (first) occurrence
> *Non-core (Pango markup manipulation) — kept fuller here, not folded into the core GTK skill (delineation rule). Related to GTK4Rs/AP-45 / #111.*

**Symptom**: annotating a word inside a formatted table cell highlighted a DIFFERENT (the FIRST) occurrence of the same word elsewhere in the cell.
**Root cause**: the highlight was injected via `result.find(escaped_slice)` — a TEXT search returning the first occurrence, not the annotated char position; a range crossing an inline-format boundary also isn't one contiguous substring.
**Resolution**: a dedicated char-range markup-wrapping routine walks the markup tracking the PLAIN-char index (tags=0, entities=1) and opens/closes the span POSITIONALLY, closing before and reopening after every existing tag to preserve well-nesting.
**Non-core (Pango markup manipulation) — do NOT fold into the gtk4-rs skill.**

## 116. Activating a nested-submenu item in a `GtkPopoverMenuBar` leaves a sibling top-level menu popped open — the bar clears its open menu through only one channel (a top-level popover's `unmap`)
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkPopoverMenuBar / GtkPopoverMenu dismiss + keynav). Latent behaviour verified present GTK 4.6.9 → 4.12.5.*

**Symptom**: activating an item in a NESTED submenu (e.g. Format ▸ Heading ▸ Heading 1) fired correctly but left a SIBLING top-level menu (always index N−1) visibly popped open.
**Root cause (GTK 4.6.9 `gtkpopovermenubar.c`)**: the bar clears "which menu is open" through exactly ONE channel — a top-level popover's `unmap` → `set_active_item(NULL)`. A focus-restore during the nested-leaf's teardown reaches the bar's `focus` vfunc mid-cascade, whose `was_popup && changed` branch closes-then-reopens the PREVIOUS sibling, consuming the bar's sole clear channel on the wrong popover.
**Scribobulate**: `window::actions::dismiss_stray_menubar_popovers` `popdown()`s any still-mapped top-level popover on idle after a nested-submenu action (public-API only — safe against the ScrAP-63 UAF). **Now enforced by** *(structurally, Wave 6)*: a family of choke-point constructors — `nested_submenu_action` (`activate`-driven `win.`), `nested_submenu_stateful_action` (`change-state` `win.`, also applies `set_state`), and `nested_submenu_app_stateful_action` (`change-state` `app.`, dismissal routed to the active window) — is the **only** path that wires the dismissal. `dismiss_stray_menubar_popovers` was made **module-private** once all four call sites (`change-case`, `format`, `select-tab`, `preview-theme`) routed through a constructor, so the raw dismissal is no longer callable by hand at all — the opt-in latent regression this entry records is now *unrepresentable* rather than merely discouraged. Ordering nuance: schedule the dismissal **before** running the handler, not after — dismissal only *enqueues* an idle, so if it were enqueued after a handler that defers its own `grab_focus` to idle, the popover's pop-down focus-restore would run last and steal the just-grabbed focus (ScrAP-107 focus-steal).
**Lesson**: a workaround you must re-apply by hand at every call site is a latent regression, not a fix — it re-breaks silently, past a feature's own functional test. Enforce it as a rule or a central choke point, and make it trigger when the risky code is *written* (adding a submenu), not just when the symptom appears.
**See**: gtk4-rs skill → widgets-and-composites (GTK4Rs/AP-108). Findings: researcher-findings-popovermenubar-submenu-stays-open.md.

## 117. Clearing a `GtkLabel` `set_attributes` overlay in place needs a transient markup-STRING change to repaint — a same-string `set_markup` is a no-op
> *Core GTK4. Strong candidate for the gtk4-rs skill (GtkTextView anchored-child repaint / GtkLabel markup vs attributes). Refines #37 / GTK4Rs/AP-45 and #111.*

**Symptom**: clearing a find-bar highlight painted as a `set_attributes` overlay on table-cell `GtkLabel`s left the old highlight ink on screen — a same-string `set_markup` after `set_attributes(None)` was also a no-op.
**Root cause**: two repaint gates both miss it — an anchored child doesn't re-snapshot on ink-REMOVAL (#37 / GTK4Rs/AP-45), and `set_markup` with an UNCHANGED string doesn't force a re-snapshot either.
**Scribobulate**: the preview-highlight clear routine does `set_attributes(None)` then a transient no-attribute `<span>` wrapper `set_markup` followed by reverting to the clean markup — two genuine string changes, zero visual difference.
**See**: gtk4-rs skill → textview-anchored-and-integration (GTK4Rs/AP-92's toggle technique, companion).

## 118. A list-item hanging indent (`left-margin` + negative first-line `indent`) is unreliable across paragraphs; the durable fix is to DROP the hanging indent (draw the marker in a gutter, uniform margin)
> *Core GTK4. Teach in the gtk4-rs skill (GtkTextView paragraph attribute resolution) — a corollary to #76.*

**Symptom**: a list item's hanging indent rendered correctly for single-line/soft-wrapped items but outdented every CONTINUATION paragraph of a multi-paragraph item to the marker column.
**Root cause**: `indent` is a first-line paragraph attribute resolved through the SAME per-line style cache as `left-margin` (#76), but — unlike `left-margin` — it must differ between a marker line and its continuations, and no single/two-tag scheme survives the cache's intermittent resolution.
**Resolution (final)**: DROP the hanging indent entirely — draw the marker in a left GUTTER (the draw/snapshot layer, out of the buffer) with a uniform per-level `left-margin` and `indent=0`, applied per logical line.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-95, corollary to GTK4Rs/AP-72/#76).

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

## 124. A test suite gated behind a Cargo feature the build pipeline never enables rots invisibly until it doesn't compile
> *Core-GTK half in the gtk4-rs skill as GTK4Rs/AP-98 (automated-UI-testing); the tooling/process remainder (which pipeline step compiles this?) stays project-specific — same precedent as #36.*

**Symptom**: Scribobulate's 398 `gtk-integration-tests` (the only suite covering real GTK wiring — real windows, real paint) went invisible to every pipeline gate; a type refactor broke their build while fmt/clippy/build/test/coverage all stayed green through multiple debrief cycles.
**Root cause**: the tests are `#[cfg(all(test, feature = "gtk-integration-tests"))]`; neither plain `cargo test` nor plain `cargo clippy --all-targets` compiles feature-gated code at all — the feature was documented as HOW to run them, never enforced as a pipeline step ("documented" masqueraded as "enforced").
**Resolution**: put the feature in the pipeline — `clippy --features gtk-integration-tests` and a dedicated `xvfb-run -a cargo test --features gtk-integration-tests` step (a display is not an excuse).
**Non-core (tooling/process remainder — which CI step compiles a gated suite) — do NOT fold into the gtk4-rs core skill.** Sibling of #123.
**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-98, the core "green ≠ covered for a gated suite" lesson).

## 125. Scheduling work that depends on paint-populated state via `idle_add_local_once` reads the previous frame's state and silently no-ops
> *Core GTK4. Instance of the deferred-work/ordering family — presents as a race, is really a temporal-ordering hazard.*

**Symptom**: a marker-popover "open next" a11y action scrolled the view to an off-screen marker then silently found no hit-box and never opened a popover — no error, no log; the view visibly scrolled and nothing else happened.
**Root cause**: the marker hit-box cache is populated ONLY by the draw/snapshot layer (an actual paint); `scroll_to_mark` defers its real scroll onto its OWN coalesced idle and ANIMATES over several frames, so the naive idle fires against the PRE-scroll paint's hit-boxes — `idle` answers "after the current main-loop turn", never "after the next paint".
**Resolution**: never bound this on a FRAME COUNT — the thing being waited on is wall-clock (GTK's scroll animation is a fixed 200 ms, `ANIMATION_DURATION` gtkscrolledwindow.c:196), while a tick fires per frame at a refresh rate the app does not control; 45 ticks is 750 ms at 60 Hz but 187 ms at 240 Hz — *shorter than the animation itself* — and wildly over-generous when `gtk-enable-animations` is FALSE (duration 0 ⇒ instant). Prefer, in order: (1) **generate the completion event yourself** where you own the paint — repopulate the state in `snapshot_layer`, then dispatch the waiting work with `idle_add_local_once` (never inline: you are in the draw path, ScrAP-22/ScrAP-30). GTK offers no signal to wait on — `GtkTextView` has none, `GtkTextLayout`'s `changed`/`invalidated` are not public GTK4 API, and `GDK_FRAME_CLOCK_PHASE_AFTER_PAINT` is documented "should not be handled by applications". (2) Failing that, poll the frame clock but bound it with a **wall-clock deadline** (`glib::monotonic_time`, ~1.5–2 s) — an absolute stamp, never an accumulated per-frame delta. Make the give-up branch **observable** (leave the visible side effect applied), never a silent no-op. Verify with a mutation test (reverting to a single idle must make the covering test FAIL). **Landed** as the `Deadline` newtype (`monotonic_time` absolute stamp) + `NAV_BUDGET_US` (2 s) / `REPIN_GUARD_US` (1.5 s) in `src/codeview/markers.rs`, replacing the former `MAX_FRAMES = 45`; the wall-clock-vs-frame-count lesson is #134.
**See**: gtk4-rs skill → textview-layout-and-drawing (GTK4Rs/AP-97).

## 126. Styling a `GtkTextView`'s background via `textview { background-color }` alone works on Default but is defeated by the user's system theme
> *Core GTK4 — teach in the gtk4-rs skill; researcher-sourced from gtk-4-6 @492b44f20c (4.6.9) + empirical probe on Breeze-Dark.*

**Symptom**: a reading theme's sepia background rendered correctly under GTK's Default theme but showed white sans text over an OPAQUE dark background on Breeze-Dark — theme-sensitive, so both source-reading and a Default-theme test passed while the shipped app was broken.
**Root cause**: two backgrounds paint per frame — the widget node over the whole border-box, then the `text` node painted ON TOP by TextView itself; GTK's Default theme sets `textview > text { background-color: transparent }` (letting the widget node show through), but Breeze-Dark sets it OPAQUE, painting over.
**Scribobulate**: the preview CSS theming step styles BOTH nodes — `textview { color; font-family }` for the widget node (also read by GTK's own caret/color paths) + `textview > text { background-color }` for the fill.
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-100).

## 127. Reaching for CSS selector specificity to arbitrate between two `GtkCssProvider`s is a category error
> *Core GTK4 — teach in the gtk4-rs skill; researcher-sourced from gtk-4-6 @492b44f20c (4.6.9).*

**Symptom**: a carefully-scoped selector in provider B did not beat a bare, unscoped selector in provider A at the same priority — GTK's cascade is NOT web CSS.
**Root cause**: GTK resolves a property by scanning providers highest-priority-FIRST, taking the FIRST value found permanently; specificity only breaks ties WITHIN one provider, never across providers — equal priority is decided purely by add order (last-added scanned first). The `font:` shorthand also silently clobbers a sibling provider's `font-size` by expanding to six longhands.
**Resolution**: never let two providers own the same property — give each provider a DISJOINT set of properties so they compose without arbitration (in Scribobulate the zoom provider owns CSS `font-size` exclusively, while the theme owns Pango *scale* — a tag attribute GTK multiplies onto the CSS base, a different lookup slot — enforced by the `the_theme_sheet_never_writes_a_property_zoom_owns` guard test in `src/preview/css.rs`); never emit the `font:` shorthand, which expands to six longhands including `font-size`. (A prior revision's example — "bake a theme's base scale into the zoom provider's `font-size`" — was fabricated: it describes exactly what that guard test forbids.)
**See**: gtk4-rs skill → theming-and-css (GTK4Rs/AP-101).

## 128. `g_get_user_config_dir()`'s process-global lazy cache makes a mid-startup `XDG_CONFIG_HOME` redirect and an honest config-dir read mutually exclusive
> *Non-core (GLib/XDG dir caching + app-lifecycle) — do NOT fold into the gtk4-rs core skill; researcher-sourced from GLib 2.72.4 gutils.c/genviron.c + GTK 4.6.9 gtkimcontextsimple.c, probed both orderings.*

**Symptom**: whichever call resolves the user config dir FIRST wins permanently for the whole process — resolving it early ("just to be safe") silently RE-ARMS the ScrAP-3 XCompose crash; resolving it late (after the redirect) silently loses the user's real `~/.config` overrides for anything reading the cache afterward.
**Root cause**: `g_get_user_config_dir()` caches into a global static on FIRST call (gutils.c:1865-1878, GLib 2.72.4), and GTK's own compose-table read, GIO's MIME/default-app lookup (which additionally probes a `<desktop>-mimeapps.list` FIRST, per lowercased `XDG_CURRENT_DESKTOP` entry), and the GTK-3-path bookmarks file (`gtkbookmarksmanager.c`) all consume that SAME cache — a process-global lazy cache turns "which call happens first" into a permanent, invisible correctness decision.
**Resolution**: snapshot CONFIG from `std::env` BEFORE the redirect and never call `glib::user_config_dir()` anywhere in the app or its dependencies; mitigate each known reader individually — symlink `mimeapps.list` (deriving the desktop-specific filename set from live `XDG_CURRENT_DESKTOP`, not hardcoding) and symlink the `gtk-3.0` DIRECTORY (never a file — a crash-safe write-temp-then-rename writer defeats a file-level symlink).
**Non-core (GLib/XDG dir caching + app-lifecycle) — do NOT fold into the gtk4-rs core skill.**
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-3's redirect, `workaround.rs`).

## 129. `g_app_info_launch_default_for_uri(uri, NULL, …)` silently emits no activation token, so a WM's focus-stealing prevention refuses to raise the handler
> *Core GTK4 — teach in the gtk4-rs skill; researcher-sourced from gtk-4-6 @492b44f20c (4.6.9).*

**Symptom**: clicking a link opened the browser tab BEHIND the app and returned `Ok` — no warning, no log; only reading the child process's ENVIRONMENT (`DESKTOP_STARTUP_ID`) reveals the missing token.
**Root cause**: it is the `GAppLaunchContext`'s `get_startup_notify_id` vfunc that emits `DESKTOP_STARTUP_ID`; a `NULL` context means NOTHING emits a token even though GIO still fork+execs the handler and genuinely succeeds.
**Scribobulate**: `gtk_show_uri_full(None, uri, 0, None, cb)` builds the launch context automatically — `None` as parent is DELIBERATE (a parent buys no extra token, only a `PARENT_WINDOW_ID` at the cost of an `gtk_window_export_handle` unexport warning on EVERY call on 4.6 X11, fixed upstream in 4.8 but never backported). Timestamp `0` here is correct and must not be "fixed" — see #47 for why the launch path substitutes it while the focus path does not.
**Now enforced by**: `links::open_url` is the sole sanctioned route; bare `gtk4::show_uri` is banned (`clippy.toml`) — its one bypass (help URL) now routes through `open_url`.
**See**: gtk4-rs skill → app-lifecycle-and-env (GTK4Rs/AP-99); #47 (the focus-path sibling — same literal `0`, opposite meaning).

## 130. A hand-authored SVG that renders fine in Inkscape can be invalid XML that librsvg (and GTK) rejects outright
> *Non-core (librsvg/docs tooling) — do NOT fold into the gtk4-rs core skill.*

**Symptom**: `sdd/system-overview.svg` rendered perfectly in Inkscape but the app itself showed a broken-image placeholder for its own architecture diagram.
**Root cause**: three `<text>` elements carried a DUPLICATE `class` attribute — a fatal XML well-formedness error; Inkscape's libxml2 recovery mode silently keeps the first occurrence and continues, while librsvg parses strictly and fails the WHOLE document with no partial render.
**Resolution**: `xmllint --noout file.svg` is the gate for any hand-authored/generated SVG, run BEFORE ever trusting a render; confirm in the actual (strict) consumer, never the lenient authoring tool. Corollary: librsvg ignores `prefers-color-scheme: dark`, so light-theme defaults must self-sufficiently fill every box.
**Non-core (librsvg/docs tooling) — do NOT fold into the gtk4-rs core skill.**

## 131. A refactor that REDEFINES what an existing field means keeps compiling at every call site, and silently changes behaviour

**Symptom**: splitting `Palette` for theming redefined `is_dark` from *"the desktop theme is dark"* to *"the rendered page is dark"* — the same name, same `bool`, same struct. Every existing reader compiled untouched. Two of them set the **editor's** GtkSourceView style scheme, so selecting a light reading theme on a dark desktop would have flipped the editor to a light scheme — a pane the theme is explicitly not supposed to touch. Nothing failed; the tests stayed green.

**Root cause**: the type system checks *shape*, not *meaning*. When a refactor changes what a value denotes while its name and type survive, every call site silently re-binds to the new meaning — and the compiler, the tests, and code review all read as "no change here". The danger scales with how *reasonable* the old readers look: `Palette::resolve().is_dark` reads correctly under both meanings.

**Resolution**: when a value's *denotation* changes, do not let the old name survive. Delete the field, force every reader to fail to compile, and give each one an explicitly named replacement (`palette::desktop_is_dark()` for the desktop's lightness; a page-local for the preview's own derivations). Turning a silent semantic change into a compile error is the whole move.

**Scribobulate**: the preview palette no longer carries a page-lightness field; a comment records why, and anything outside the preview probes the desktop's lightness through a dedicated helper. TDD 18.7.

**Lesson**: a refactor that redefines a name is more dangerous than one that removes it, because removal is caught by the compiler and redefinition is caught by nobody. **Rename or delete on a meaning change — never re-point a surviving name.** Generalise: ask "would every existing reader still be *correct* under the new meaning?", and if the answer is no, make it not compile. **Non-core (general refactoring discipline) — do NOT fold into the gtk4-rs core skill.**

## 132. A source-scanning guard test whose scope filter is wrong scans nothing and passes forever

**Symptom**: a regression guard asserting that no module resolves the config dir through `glib::user_config_dir()` (which would silently re-arm the XCompose crash and/or lose the user's theme overrides — #128 family) passed. It also passed with a deliberate violation injected. It was scanning an empty string.

**Root cause**: the test read each module with `include_str!` and truncated at `#[cfg(test)]` to skip the test module. That attribute *also* sits on individual test-only helper fns partway up a file (some modules carry two), so `split("#[cfg(test)]").next()` cut the file off above the very function the test existed to police. A guard that scans nothing is indistinguishable from a guard that finds nothing — both are green.

**Resolution**: split on the test-MODULE header (`"#[cfg(test)]\nmod "`), not the bare attribute. Then **prove the guard fails**: inject the violation, watch it go red, revert, watch it go green. Assembling the banned pattern with `concat!` also stops the test's own source from matching itself (it scans its own file).

**Scribobulate**: this entry describes the now-retired config-dir-resolution scanner (the "no module resolves the config dir through `glib::user_config_dir()`" guard) — its `#[cfg(test)]`-truncation bug, and later its hardcoded-4-file scope (itself this very species), meant it never scanned the sites it policed. That invariant is now enforced compiler-wide by a `clippy.toml` `disallowed-methods` ban on `glib::user_config_dir` (N10) — a gate with no scope to get wrong. The one surviving injection-verified absence-guard is `workaround.rs::the_redirect_touches_config_home_only` (the redirect's `set_var(` lines must name `XDG_CONFIG_HOME` and never the data dirs), whose single-file scope is correct because `set_var` only happens there.

**Lesson**: a test that asserts an ABSENCE can pass for two reasons — the thing is absent, or you looked in the wrong place — and it reports both identically. **Every absence-asserting guard must be shown to fail on a planted violation before it is trusted**; unverified, it is worse than no guard, because it converts an unprotected invariant into one everybody believes is protected. Applies to lint rules, grep-based CI gates, "no forbidden import" checks, and negative snapshot assertions alike. Same family as #123/#124/#130 — the gate that passes while measuring the wrong thing. **Non-core (testing discipline) — do NOT fold into the gtk4-rs core skill.**

## 133. A hard-coded Xvfb display lets one crashed run orphan a server that silently serves stale windows to every run after it

**Symptom**: two sequential headless captures, each pinning a *different* reading theme through its own private session file, both came out in the **same** theme — the frame specified as Terminal rendered Synthwave. Every screenshot was a valid PNG of a real, correctly-rendered Scribobulate window; it was simply the *wrong window*. Suspicion landed on the theme engine (session parsing, theme resolution, the `-n` new-instance flag), all of which were fine. An isolated capture of the same theme rendered perfectly, which made it look intermittent.

**Root cause**: an earlier run had failed at its screenshot step and, under `set -e`, aborted **before** its cleanup lines, orphaning `Xvfb :99`. With the display number hard-coded, every later `Xvfb :99 &` exits immediately ("server already active") — but the launcher never checks: `$!` is a already-dead PID, so the matching `kill` reaps nothing, and the app happily connects to the **pre-existing** server. The window search then finds whatever stale windows are lying around on it. Nothing errors, nothing warns; the capture just reads someone else's screen. The failure is invisible precisely because every individual step "succeeds".

**Resolution**: never hard-code the display — let Xvfb allocate a free one and report it (`Xvfb -displayfd 1 … >file`, then read the number back), so two runs can never collide. Make teardown unconditional: record every spawned PID and reap it from an `EXIT` trap, because `set -e` skips the teardown exactly when a failure has made teardown most necessary. Verified after the fix by asserting on pixels rather than trusting the run: Terminal captured true-black `srgb(0,0,0)`, Synthwave indigo `srgb(26,16,51)`.

**Scribobulate**: the capture stage of `scripts/gen-splash.sh` (display allocation + `SPAWNED_PIDS` / `trap … EXIT`).

**Lesson**: a **fixed global resource id** — an X display, a port, a lock path, a fixed temp dir — converts one crashed run into a booby trap for every run after it, because the second process does not fail, it *silently attaches to the first one's leftovers*. Two rules: **allocate the id dynamically and let the tool tell you which one it got**, and **make cleanup unconditional (a trap), never a trailing line an early exit can skip**. The diagnostic corollary generalises past X11: when a headless capture disagrees with the state you configured, suspect the **harness/environment before the application** — an artifact that is *wrong but internally valid* is the signature of reading the wrong source, not of a broken feature. **Belongs in the `gtk4-rs` skill's automated-UI-testing methodology** (launch/teardown discipline, alongside the existing "only ever kill the PID you launched" rule) — sent to the skill maintainer.

## 134. Bounding a wait on a FRAME COUNT when the thing waited on is measured in WALL-CLOCK
> *Core GTK4. **In the gtk4-rs skill as its GTK4Rs/AP-122** (deferred-work-and-ordering; landed `a619a05`). Instance of the deferred-work/ordering family. Its landing also fixed a latent instance there: the skill's own GTK4Rs/AP-97 example still bounded a wall-clock wait on `MAX_FRAMES = 45` — i.e. it prescribed this very bug — and was re-shaped to a monotonic deadline. When a finding invalidates a TECHNIQUE, sweep for the technique, not just the entry.*

**Symptom**: an `add_tick_callback` poll bounded by `if ticks >= N` works on the dev box and fails on a user's high-refresh panel — **silently**, because the give-up branch does nothing. Nobody reproduces it, because nobody's test box is 240 Hz.

**Root cause**: a frame count converts to wall-clock only by assuming a refresh rate. The app does not control the refresh rate; GTK's own timings are durations (`ANIMATION_DURATION` = 200 ms, distance-independent, gtkscrolledwindow.c:196). The same constant is therefore *both* too small (240 Hz: 45 frames = 187 ms < the 200 ms animation it must cover) *and* too large (`gtk-enable-animations` FALSE ⇒ duration 0 ⇒ the value sets instantly). A constant that is wrong at both ends depending on the environment is a wrong-**shaped** bound, not a mistuned one — do not retune it, re-shape it.

**Resolution**: bound on `glib::monotonic_time()`. Use an **absolute deadline stamp** (`now + budget`, tested with `>=`), never an accumulated per-frame delta — accumulation reintroduces the very frame dependence you are removing, and a clamped dt (correct for *easing* an animation, e.g. a tab-strip `animate_tick`) will **under-count a stall** and push the give-up past its wall-clock intent. Give the shape a name (a `Deadline` newtype) so it is a choke point rather than an open-coded convention two call sites can drift apart on — in this codebase they already had. Make the give-up branch **observable** (leave the visible side effect applied), never a silent no-op.

**Scribobulate**: `Deadline` + `NAV_BUDGET_US` / `REPIN_GUARD_US` in `src/codeview/markers.rs`. The two defective sites were a `MAX_FRAMES = 45` ("~0.75s at 60Hz") navigation poll and a `ticks >= 48` ("~0.8s @60fps") re-pin guard. A sweep confirmed no third site (`preview/scroll.rs` is already wall-clock; `scrollsync.rs` breaks after one tick).

**Lesson**: **match the bound's units to the quantity being waited on.** A tick fires per *frame*; an animation, a timeout, a settle are *durations*. Converting between them requires a refresh rate you do not own, so the conversion is the bug — and it hides, because the dev box's 60 Hz is the one rate at which the arithmetic happens to work. Two corollaries. First, a constant that is simultaneously too small and too large depending on environment is diagnostic: it means the *shape* is wrong, and retuning it just moves which users it fails. Second, **deductive evidence is sufficient here and observation may be unavailable** — 187 < 200 settles it, while a UI drive on a 144 Hz panel (break-even is 225 Hz) passes both before and after the fix and proves nothing. Verify by construction (the counts are gone, the type is the choke point) plus mutation-checked tests, and do not let "we couldn't reproduce it" become "it isn't real". **Belongs in the `gtk4-rs` skill** — sent to the skill maintainer.

## 135. `GtkText` writes PRIMARY on every selection change — and a widget claiming PRIMARY CLEARS the previous owner's selection
> *Core GTK4. **In the gtk4-rs skill as its GTK4Rs/AP-120** (actions-and-commands; landed `a619a05`). Instance of the deferred-work/ordering family's proxy-vs-goal-state class. Researcher-sourced (GTK 4.6.9) + live-verified. Its retraction half also inoculated the skill's GTK4Rs/AP-28 with the per-widget nuance (`GtkLabel`/`GtkTextBuffer` emit on every selection delta via `content_changed`; **`GtkText` does not**) and seeded two evidence-provenance Working Principles there. **Skill-side open edge**: the GTK 4.12 counts, pending a checkout.*
>
> **⚠ Read the per-widget table below before generalising ANY of this.** An earlier draft of this entry stated a `GtkText`-only behaviour as a universal PRIMARY-clipboard rule; it was false for `GtkLabel` and `GtkTextBuffer`, and would have taught the wrong thing about two widgets out of three from the canonical entry. Whether PRIMARY `::changed` tracks a selection **is per-widget, not a property of the clipboard**.

**Symptom**: with an in-surface comment card open and text typed, **Ctrl+A *or* Shift+Home** dismissed the card and silently discarded the text. Two keystrokes — and the obvious explanation (a window-accelerator collision) covered only one of them, which is exactly why a fix built on it would have "worked" and left the other broken.

**Root cause**: `GtkText` writes its selection to the **display-level** PRIMARY clipboard on EVERY selection change — keyboard included, gated only on being realized (`gtk_text_set_selection_bounds` → `gtk_text_update_primary_selection`, gtktext.c:3477). Two consequences, and the second is the one that bites:
1. Any listener on the primary clipboard's `::changed` (the common workaround for tracking a selectable `GtkLabel`) hears **your own** entries — and other **applications**, since PRIMARY is display-wide. It **over-fires**.
2. It also **under-fires — but ONLY for `GtkText`.** Whether a selection *delta* re-emits depends entirely on **whether the widget calls `gdk_content_provider_content_changed`**, which emits unconditionally (`gdkclipboard.c:209`, *outside* the `if (priv->content != content)` guard):

   | Widget | Calls `content_changed`? | `::changed` on an intra-widget selection delta |
   |---|---|---|
   | `GtkLabel` | **YES** — `gtklabel.c:5038` | **fires every time** |
   | `GtkTextBuffer` | **YES** — `gtktextbuffer.c:3797` | **fires every time** |
   | **`GtkText` / `GtkEntry`** | **NEVER** | **ownership transitions only** |

   So for `GtkText` alone, selection→different-selection **by the same widget is silent** — Ctrl+A twice is **one** emission. Do **not** generalise this: `gdk_clipboard_set_content`'s early-return on an unchanged provider is real, but for `GtkLabel`/`GtkTextBuffer` the `content_changed` call one line earlier has **already** emitted, so the early-return never bites. Tracking a selectable `GtkLabel` via PRIMARY `::changed` therefore has **no hole** — do not go hunting for one.
3. **A widget claiming PRIMARY makes the previous owner clear its own selection**, which emits that owner's own selection signals (`mark-set` on a `GtkTextBuffer`). So muting your clipboard listener does **not** contain the blast radius — the perturbation arrives through the *document's* signals too. This is what makes "just block our listener" fail, and it is the consequence that actually bit.

**Resolution**: never treat either signal as "the selection I care about changed". They are **proxies**; name the **goal state** and test it. Record the anchor you actually depend on when you raise the UI, and act only when the live selection is a *different, non-empty* selection — a selection that is merely **gone** is not the user choosing something else. Guard at the **one decision point**, not at each signal: there are more signals than you think, and you cannot enumerate them. Do **not** try to identify the provider from Rust — `GtkText`'s selection content is private and unexposed in gtk4-rs, so a handler cannot ask "was that me?".

**Scribobulate**: `preview/annotate/overlay.rs`, inside `schedule`'s debounce timer — the code's own comment already *claimed* the contract ("the selection it was anchored to changed") but never **checked** it; it inferred it from "a signal fired".

**Probe discipline** (if you ever need to confirm the table above): instrument the **emission COUNT, not "did it fire"**. A boolean probe reports success on a broken build, because *some* emission always arrives. Count them: for a `GtkText`, select (1) · extend within it (**still 1**) · move to another widget (2). For a `GtkLabel`, every one of those increments.

**Lesson**: **when a comment states a condition the code only infers, that gap is the bug.** The inference held while nothing else perturbed the selection, and PRIMARY made that assumption false. Generalisable: a signal that fires when your goal state changes is not the same as a signal that *means* your goal state changed — guard the decision, not the notification, or you will chase mechanisms forever. And when two triggers produce one symptom, **do not stop at the first mechanism that explains one of them**: the second is the falsifier that tells you the first is wrong.

**And the lesson this entry learned about itself, which is the more expensive one.** The under-firing claim above was originally written as a **universal** rule, from a **correct** read of `gtklabel.c:5039` that never asked what ran on the line above. Measured (Xvfb, GTK 4.6.9, counting emissions per step):

| step | observed | meaning |
|---|---|---|
| extend selection **within** one label | **1** | the discriminator — the claim predicted **0** |
| genuinely **identical** re-selection | **0** | `set_content`'s early-return **is real** (`gtklabel.c:5025-5027`) |

**The error was not a hallucinated mechanism — it was a real mechanism that simply is not on the path.** `:5038`'s `content_changed` bypasses the early-return for any real change. That is *why* it felt confirmed: everyone who checked found the early-return exactly where it was said to be.

Four rules fall out, in ascending order of how much they cost us:
1. **A source-read is a prediction, not an observation.** `grep`ping the call you expect proves nothing about the call you didn't look for.
2. **Scope a mechanism to the widget you actually verified.** Three widgets sharing one API do not share one behaviour — and the odd one out (`GtkText`) was the one this bug was about, which is precisely what made over-generalising so tempting.
3. **Relay evidence, not labels.** This went three hops — researcher → planning agent → this register — and each hop passed on the word "confirmed" rather than the derivation, so nobody re-derived it. "Confirmed against the C source" is the exact phrasing that stops the next reader checking. It was caught only because the researcher re-approached the same machinery from the opposite direction and checked *itself*.
4. **Write the probe against the OUTCOME you care about, not the mechanism you found.** A probe testing only "extend within one label → assert silence" would have **passed** had we written it before the retraction — because we'd have built it to confirm the mechanism we'd already spotted. The step that settled this was the one that could **discriminate between the two readings**, plus a control proving the early-return real. A test written to confirm a hypothesis confirms the hypothesis.

A canonical entry teaching a false universal is worse than no entry — see #125, which prescribed the very bug it existed to forbid. **Belongs in the `gtk4-rs` skill** — sent to the skill maintainer **including this correction**, since the skill carries its own copy of the stale claim and a skill teaches every future session.

## 136. Seeding live UI state from the persisted-session snapshot
> *Non-core (Scribobulate architecture) — this is a state-ownership pitfall, NOT a core-GTK lesson. Do not fold into the gtk4-rs skill.*

**Symptom**: a UI preference toggled mid-session is silently ignored by the next window opened — it comes up with the last-**persisted** value instead. Presents as per-path ("pop-out is broken") but is shared by every new-window path. It survives a restart *correctly*, which misdirects diagnosis toward the persistence layer, where nothing is in fact wrong.

**Root cause**: the window factory seeds state by calling `session::load()`. The session file is a **snapshot written only at window close and coordinated quit** — so for the entire life of a running app it lags every toggle. Because all new-window paths funnel through one factory, one `load()` call there makes the staleness universal.

**The trap**: `load()` at build time *looks* right, and greens in the two cases anyone checks by hand — cold start (disk is current) and post-restart (disk was just written). It is wrong only in the window between a toggle and the next persist: exactly the window a user lives in and a test rarely opens.

**Resolution**: keep live UI state in an in-memory source of truth, seeded from disk once on first read and updated by every toggle through a **single mutation choke point** (so a field cannot be added with the seeding forgotten). The factory reads that, never `load()`. **`load()` is for restoring at startup, not for reading current state** — the snapshot answers "what was X when we last shut down?", which is a different question from "what is X now?".

**Scribobulate**: `session::LiveChrome` + `update_live_chrome` (a `thread_local`; GTK is single-threaded), read by `window/mod.rs`'s `build_window` — **since retired.** The live app-wide cache was a correct fix to the *read* staleness and a wrong answer to the underlying question: the state was never app-wide. It is now window-scoped (`session::ChromeSession` inside each `WindowSession`), and the live source of truth is each window's own `win.*` action states — which cannot go stale, because the toggle handlers that write them are the only claimants. `window::read_window_chrome` is the one reader, shared by the seed path (`inherit_from`) and the persist path.

**This entry is VINDICATED by its own retirement, which is why it stays.** The cache was built on the classification "this state is app-wide", and that classification was never checked against the code's *behaviour* — every toggle handler had only ever touched its own window. Storage said app-wide, behaviour said per-window, persistence said "whichever window closed last": three answers to one question. Fixing the read path with a live cache made the write path's disagreement **sharper**, not smaller — the second corollary below, arrived at the hard way, one layer up from where it was first learned. The lesson is not "caches go stale"; it is that a scope error cannot be fixed by any amount of care at the seeding site, because the seeding site is not where the mistake lives.

**Lesson**: **classify a piece of state's scope before writing any seeding code** — app-wide, window-scoped, and tab-scoped have three different inheritance rules, and the seeding code you write is a *consequence* of that choice, not a place to decide it. Reaching for the persisted snapshot to answer "what is the current value of X?" is the smell. Two hard-won corollaries. First, **fixing the read path without the write path leaves the two disagreeing**: seeding from live state while still *persisting* from whichever window happens to close last is a sharper bug than the original, because the halves now diverge — fix both ends or neither. Second, mutation-test it: a guard that asserts only the cold-start path **passes with the bug present** and is worthless (cf. ScrAP-87).

## 137. A window `GAction` accelerator BEATS a focused `GtkText`'s own keybinding — and *disabling* the action is what hands the key back
> *Core GTK4. **In the gtk4-rs skill as its GTK4Rs/AP-121** (actions-and-commands; landed `a619a05`). Researcher-sourced (GTK 4.6.9) + live-verified with real keystrokes.*

**Symptom**: Ctrl+A in an entry inside your window selects the whole **document** instead of the entry's text, and the entry appears to have no select-all at all. Any window accel shadowing a standard text-editing key (Ctrl+A/C/X/Z…) has this against **every** `GtkEntry` in the window.

**Root cause**: GTK adds the application accel controller to the **window** at `GTK_PHASE_CAPTURE` with `GTK_SHORTCUT_SCOPE_GLOBAL` (gtkwindow.c:2855-2859), while `gtk_widget_class_add_binding*` installs widget keybindings at `GTK_PHASE_BUBBLE` (gtkwidget.c:4416). **Capture runs root→target, therefore before bubble** — the window accel wins and the focused widget never sees the key.

**Resolution**: `set_enabled(false)` on the action while the focused widget should own the key, recomputed on `notify::focus-widget` (ScrAP-38) via a window-level focus walk (GTK4Rs/AP-20/#72), never a single widget's `has_focus`. **Disabling does more than withhold the action**: when a shortcut's action activation FAILS, `GtkShortcutController` leaves its return FALSE and propagation **continues** (gtkshortcutcontroller.c:409-422), so the focused widget's own binding then runs. One change both stops the theft and restores the widget's native behaviour — the entry *gains* a select-all it never had.

**Key the predicate on the focused widget's TYPE, not an ancestor CSS class.** `gtk::Text` is the delegate every `GtkEntry`/`GtkSearchEntry` focuses into, and a `GtkTextView` is a different GObject type entirely — so a type check naturally covers every entry in the window while leaving document-wide select-all correct in text views. A class-based check silently covers only the surfaces someone remembered to tag.

**Scribobulate**: `focus_in_text_entry` (`window/actions.rs`) gating `win.select-all` (`window/editoractions.rs`). It began life as `focus_in_annotation_card`, a CSS-class-ancestor check scoped to one card, and had to be widened to a type check once the mechanism was understood — the find and replace entries had the identical bug.

**Lesson**: **scope the fix to the mechanism, not to the report.** The bug arrives as "the annotation card eats my text"; the mechanism is "window accels outrank every focused entry in this window", and a fix cut to the report's shape leaves every other entry broken and *looks* correct. Two corollaries. First, this makes the symptom **state-dependent** — the same keystroke behaves differently depending on the action's enabled state, which is why it reads as intermittent and defeats "I can't reproduce it". Second, when you widen such a predicate, **hunt the comments that named the old mechanism**: a comment saying "this class is what the standdown matches on" survives the rename compiling perfectly and starts lying — and any test asserting the old proxy keeps **passing while guarding nothing**. **Belongs in the `gtk4-rs` skill** — sent to the skill maintainer.

## 138. Polling a `GtkEntry`'s own `has_focus()` in a test spins forever — focus lands on its internal `GtkText` delegate
> *Core GTK4. **In the gtk4-rs skill as its GTK4Rs/AP-119** (ui-testing-verification; landed `a619a05`). Instance of the automated-UI-testing family. Independently rediscovered by another agent within minutes of this filing — convergence, which is signal that it earns its place.*

**Symptom**: a headless test grabs focus into a `GtkEntry`/`GtkSearchEntry`, then pumps the main loop until `entry.has_focus()` — and never returns, timing out or hanging. The widget is visibly focused and behaving correctly; the probe simply never observes it.

**Root cause**: `GtkEntry` and `GtkSearchEntry` are **wrappers**. The real editable is an internal `GtkText` delegate, and that is what actually takes focus — so the wrapper's own `has_focus()` stays FALSE while its child holds focus. The probe is asking the wrong object.

**Resolution**: walk the **focus ancestor chain** from `GtkWindowExt::focus(window)` upward (or test `is::<gtk::Text>()` on the focused widget) instead of asking any wrapper whether it has focus. This is the same walk the production predicate uses to decide whether focus is in a text entry — so test and production agree by construction.

**Scribobulate**: the readiness probes in `window/editoractions.rs`'s select-all standdown rubric, and `focus_in_text_entry` (`window/actions.rs`), which is keyed on `gtk::Text` for exactly this reason.

**Lesson**: **a composite GTK widget's public identity is not its focusable identity**, and the gap only shows in a probe — interactive use looks fine, so this is a test-only trap that reads as "GTK is broken" or "the harness can't do focus". Generalises past focus: when a wrapper delegates behaviour to an internal child, assertions aimed at the wrapper quietly observe the wrong object. Prefer a **tree walk over a widget query** when asking "where is state X right now?". Corollary for pumping loops: a readiness probe that can never become true is indistinguishable from a slow one — bound every pump on a wall-clock deadline (#134) and fail loudly, rather than spinning. **Belongs in the `gtk4-rs` skill's automated-UI-testing methodology** — sent to the skill maintainer.

## 139. A `GtkText`/`GtkEntry` selects ALL its text on focus-in, silently undoing a caret set BEFORE `grab_focus` — and the hazard IS guardable headlessly, if the toplevel is MAPPED
> *Core GTK4. Candidate for the gtk4-rs skill (controllers-and-bindings / automated-UI-testing). Instance of the deferred-work/ordering family. Sibling of #106 (same GTK behaviour, different widget and trigger). Live-verified + mutation-proven both ways.*

**Symptom**: an entry pre-filled with text the user must not lose opens **fully selected**, so their first keystroke silently replaces the whole value. Nothing warns; the pre-fill is visibly present and completely useless. It bit **both** annotation surfaces here, each time in the *first working build of the fix whose entire purpose was to stop silent data loss* — the loss walking back in through the door the fix opened.

**Root cause**: `gtk-entry-select-on-focus` defaults TRUE, so a focused `GtkText` selects all its text on focus-in (gtktext.c:3310-3317). A caret set **before** `grab_focus` is therefore undone by the grab itself. The call order looks arbitrary and is load-bearing.

**Resolution**: set the caret **AFTER** `grab_focus` — `entry.set_position(-1)` for caret-at-end, so typing **appends** and the user amends rather than starts over. Comment the ordering as load-bearing **at the call site**: it is one line away from being "tidied" back into the bug by a future refactor, and nothing about the code's appearance objects.

**Testability — the part that is easy to get wrong, and the reason this entry exists.** This is widely assumed to be live-drive-only. **It is not.** The behaviour needs a **MAPPED toplevel, not a window manager**: `win.present()` + pump to `is_mapped()` makes `grab_focus` fire focus-in and `GtkText` select-all with **no WM running** — measured identically with and without openbox. A plain `#[gtk::test]` under `xvfb-run -a` catches an ordering regression. **A WM is needed for *keystroke delivery* — a different capability**, and conflating the two is what makes this look untestable. The failure mode of getting it wrong is silent: a guard built on an **unmapped** widget **passes vacuously** against the ordering mutation and looks like coverage. **Mutation-test this guard specifically — it is the one that lies.**

**Scribobulate**: `preview/annotate/overlay.rs` and `window/editor_annotate.rs` both `set_position(-1)` after `grab_focus`; the editor card's ordering guard uses a `raise_card_over_mapped` helper for exactly this reason.

**See**: #106 (same GTK select-on-focus behaviour, `GtkLabel` in a popover), #138 (the wrapper's `has_focus()` is not the delegate's).

**Lesson**: **"this needs a real desktop" is a claim to test, not a premise to accept.** The reflex to declare a UI hazard untestable is expensive twice over — it skips an automatable guard *and* it launders the omission as a limitation of the harness. Decompose the capability instead: *mapping*, *focus*, *keystroke delivery*, and *a compositor* are four different requirements, and most "needs a WM" beliefs only need the first. Corollary, and the sharp end: **the more confident you are that a guard can't work, the more certain you must be that it isn't passing vacuously** — this codebase nearly shipped a test that passed against the mutation *with a comment explaining why it couldn't catch it*. **Belongs in the `gtk4-rs` skill's automated-UI-testing methodology** — sent to the skill maintainer.

## 140. A security gate answering a DIFFERENT question than the one being asked
> *Non-core (Scribobulate architecture / security design) — a gate-scoping pitfall, not a core-GTK lesson. Do not fold into the gtk4-rs skill.*

**Symptom**: a relative Markdown link (`[architecture](TECH.md)`) produced no navigation, no error, no dialog — only a WARN log line ("refusing to open URL with disallowed scheme"). Looked like a missing feature. It was a **correct gate being asked the wrong question**.

**Root cause**: one function answered two different questions. *"Is it safe to hand this URI to the OS's default handler"* (external launch — real risk: arbitrary program execution, arbitrary file disclosure via whatever `xdg-open` does with it) and *"is it safe to navigate to this document inside the app"* (internal navigation — hands nothing to any external program) are **different risk profiles with different correct answers for the exact same schemeless input**. A gate written for the first, silently asked the second, produces a correct-but-wrong-context refusal that presents as a bug.

**What was tried and rejected before implementation**: widening the existing scheme allowlist (`http`/`https`/`mailto`) to also admit "no scheme". That would also admit `file:///etc/passwd` and any absolute or `..`-traversal path reached by a schemeless link — defeating the gate's actual purpose at exactly the point the two operations diverge.

**Resolution**: the external-launch gate is **completely unchanged** (still `http`/`https`/`mailto`, still refuses `file://`). A **separate** resolution step handles schemeless local document links, reusing the **existing** image-containment primitive (component-wise `Path::starts_with` after canonicalize — never a string prefix, which would admit a `docs-evil/` sibling as if it were under `docs/`) rather than inventing new containment. One dispatcher decides which gate a click needs **before** either runs, so the question can't be asked of the wrong gate again. Where the two genuinely must agree on a sub-question ("does this string have a URL scheme"), that **one predicate** is factored out and shared explicitly.

**Scribobulate**: `links.rs` (`is_allowed_url` / `resolve_doc_link` / `scheme_of`) + `window/linknav.rs` (the dispatcher).

**Lesson**: **when a trust gate exists for one operation and a new feature needs a related-but-distinct one over the same untrusted input, do not widen the existing allowlist.** Widening conflates two risk profiles precisely where they diverge, and it does it invisibly — the gate still "works", it just now answers for a case it was never reasoned about. Write a second, purpose-built decision; **reuse the containment PRIMITIVE the first gate already proved correct, but keep the DECISION separate per operation**. The tell that you are in this situation: the existing gate's refusal is *correct on its own terms* and yet the user-visible outcome is obviously wrong. That is a scoping error, not a policy error — and reaching for the allowlist is the natural, wrong move.

## 141. A "this will misbehave" theory read from a construction site, never executed
> *Non-core (methodology) — a verification-discipline lesson. Sibling of #135's source-read trap; see also ScrAP-87 (mutation testing).*

**Symptom** (of the *process*, not the app): a plausible, code-grounded bug report that **does not reproduce**. Here: reading a `WindowInit`-style seeding struct showed a per-tab toggle's mirrored `GAction` being seeded from a hardcoded construction-time default, independent of whichever tab lands there, with nothing *obviously* re-syncing it on the transplant path — a live "the checkbox lies about what the tab will do" bug, in a **security** toggle.

**Root cause of the false theory**: the diagnosis was built entirely from the **seeding site**. Tracing the full path showed the tab-strip's "make this page current" call early-returns only when the requested page is **already** current — and the transplant sequence (build a bare window with a throwaway starter tab, remove it, *then* append the real tab) leaves "current page" at **none** in between. So the subsequent focus call is **never** a same-index no-op for the arriving tab, the existing switch-signal handler fires, and it re-syncs every per-tab `GAction` — **self-healing the mirror before it is ever observably wrong**. Later code in the same call chain already corrected it, through a mechanism invisible from where the theory was read.

**How it was caught**: **mutation-testing the fix** (revert it, re-run the regression test) — and the test **still passed**. That is what exposed the theory, not review.

**Resolution**: the explicit resync was **kept** — not as a repair for an observed defect (there isn't one) but as cheap, enforced insurance against a future change to the index bookkeeping reintroducing the same-index no-op. **Both the code comment and the test's own doc comment say so plainly**, so a later reader can't mistake insurance for a fix, or rediscover the false lead. The regression test asserts the **end-state property** ("the destination mirrors the moved tab"), not that any one line produces it.

**Lesson**: **a theory built from reading a construction/seeding site is not evidence the misbehaviour is reachable.** Defaults, `*Init` structs and constructors are seductive to reason from because they are simple and self-contained — which is exactly why they are a bad place to stop: whatever runs *after* construction may already correct them. Trace the whole path, including the "now make this the active thing" call that follows, and **mutation-test before writing the resolution note as fact**. Keeping a defensive fix that the mutation test shows is currently redundant is reasonable — **say so plainly rather than writing up a bug that testing did not confirm**. Compare #135: the same disease (a correct local read generalised into a false claim about the whole path) caught the same way (by executing the discriminating case rather than re-reading the code).

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

## 143. A PERMANENT register entry citing an EPHEMERAL artifact (an ISSUES entry, a PLAN file)
> *Non-core (SDD process) — a documentation-lifecycle lesson, not a GTK one. Do not fold into the gtk4-rs skill. **Feedback owed to the SDD skill**: its principle "never reference an issue from outside ISSUES.md" is one notch too narrow — see below.*

**Symptom**: an ANTI-PATTERNS entry says "see `sdd/PLAN.<topic>.md` for the trace and the repro" — and the plan is gone. The entry does not break loudly; it just **stops resolving**, and the next reader concludes the evidence was never captured and re-derives it. Or worse, the ID has been recycled and the pointer resolves to something unrelated: it lies quietly.

**Root cause**: **the two registers have opposite lifecycles, and citing across that boundary is always wrong.** ISSUES entries and PLAN files are **ephemeral by design** — an issue's success condition is *being deleted*, and a plan's is *being retired into the code*. ANTI-PATTERNS entries are **permanent by design**. A permanent artifact pointing at an ephemeral one has an expiry date built in from the moment it is written.

**A plan is if anything MORE ephemeral than an issue**: it disappears when it *succeeds*, and — as here — also when it *fails*, because a design nobody should build has no reason to keep a design document. Both exits delete it.

**Resolution**: **a permanent entry must be self-contained.** Inline the trace, the measured numbers, the reduced repro — everything a reader needs to act without following a link. Cite only things that are themselves permanent: another ANTI-PATTERNS entry, a source file and line, an upstream C source, a commit sha. **Do not "fix" a dangling plan reference by re-targeting it at the ISSUES entry that absorbed the plan** — that entry is ephemeral too, and you have merely bought a few weeks. Inlining looks like duplication; it is not. It is the entry paying the storage cost of its own permanence.

**Scribobulate**: #142 was filed citing a plan file that was deleted ~20 minutes later, when the probe killed the design and its findings were folded into the issue register. The entry now inlines the gesture trace and the reduced repro and cites nothing ephemeral.

**Lesson**: **classify an artifact's lifecycle before you cite it, exactly as you classify state scope before seeding it** — same discipline, different domain. The tell is one question: *what does this artifact look like when the project is healthy?* An issue register is healthy **empty**; a plan is healthy **deleted**; the lessons register is healthy **growing**. Anything from the growing set may never point into the shrinking sets. Corollary worth its own line, because it is where the rule actually gets broken: **the danger window is when the ephemeral artifact still exists and looks solid.** The plan cited here was real, current, and freshly written when the citation was made — the reference was accurate for twenty minutes. "It's there right now" is precisely the reasoning the rule exists to overrule.

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
> *Non-core (documentation/process) — a register-hygiene lesson, not a GTK one. Do not fold into the gtk4-rs skill. Sibling of #143 (both are "a citation that resolves to the wrong thing rather than to nothing").*

**Symptom**: an entry cites the bare number **98** for a fact that the entry it resolves to does not contain. Nothing dangles, nothing 404s — the number **resolves**, to a real entry, about something else entirely. The reader follows it, finds a plausible-looking lesson that doesn't say what the citing text claimed, and concludes they have misunderstood the *code* rather than that the *citation* is wrong.

**Root cause**: **this project and the `gtk4-rs` skill both number their entries from 1, and both are cited as `AP-<n>`.** They are different registers. the bare number **98** means one thing written here and a different thing written in the skill, and the collision is total across the overlap. Worse, the convention is *positional* and undocumented: **from outside** the register (code comments, ISSUES.md) `AP-N` means *this project's* entry N; **inside** ANTI-PATTERNS.md, `#N` means an entry here and `AP-N` means a **skill** entry. So the same string means different registers depending on which file it is typed in — and this file's own header once violated its own rule (it wrote a bare "**4** (Pango)" for project #4, one line from "went to the skill as" a bare **46** for a skill entry; now written `ScrAP-4` and `GTK4Rs/AP-46`).

**What it actually cost, in one session**:
- This register's **#98** is the popover autohide/seat-grab lesson. **GTK4Rs/AP-98** is the CI/feature-gate lesson — which arrived there as *this register's* **#124**'s core half. So the bare number **98** named both, and #124's own note ("core-GTK half in the skill as GTK4Rs/AP-98") is *correct* while the bare form it used to carry read exactly like the error.
- **#144 was filed citing the bare numbers 90/98/112/117** — reasoning from the code's convention while writing inside the file that uses the opposite one. Three were the right entries under the wrong prefix; the fourth was not an entry at all.
- **A bare 117 was pure fabrication, propagated by copy.** No register's 117 is about popovers (here it is a `GtkLabel` `set_attributes` repaint lesson; in the skill it is "reuse popovers"). The text "117's parenthetical — this class is real-compositor-only" was really **#98's** parenthetical. It survived into an ISSUES entry, two code comments, a test's failure message and a commit message, because **each copy looked exactly as authoritative as the original**.
- Relaying those numbers **to the skill maintainer** — who correctly read `AP-N` as *skill* numbers — produced a confident, precise, entirely wrong claim about their file. They verified instead of complying, which is the only reason it stopped there.

**Resolution**: **never let two registers share a citation prefix.** One of them renames — the newer, the smaller, or the one with fewer inbound links. **Done in two passes, and the first was not enough.** 2026-07-22: this register renamed to `ScrAP-N`, which removes the collision by construction rather than by everyone remembering a positional rule. That pass then claimed the legacy citations "were swept out of `src/`, `tests/`, and this file's cross-register citations, so the convention now holds tree-wide" — **and that claim was false when written**: 443 bare citations were still in the tree (310 in `src/`, 127 in `sdd/`, 6 in `tests/`), because the sweep had legalised the bare form ("it means the skill") instead of retiring it. A form whose correct and incorrect uses are textually identical is not a convention: nothing could tell a deliberate skill citation from one the sweep missed. Worse, the sweep itself **rewrote prefixes without re-resolving numbers**, silently re-pointing correct skill citations at unrelated local entries — `src/widgets/tab/bar.rs` carried a `ScrAP-79` on a pump-loop comment (that is GTK4Rs/AP-79, and #88 here), sitting among three *correct* `ScrAP-79` citations, and the reference lint passed because a cited entry *existing* cannot distinguish "cites the right entry" from "cites a real entry about something else". **2026-08-01, the actual fix**: the bare form is now **illegal**, the skill form is the single token `GTK4Rs/AP-N` (never two words — see the first rule below), and `lint-references` **check 8** fails on any `AP-N` not prefixed `Scr` or `GTK4Rs/`. Making bare illegal does not make skill citations *verifiable* — the skill may not be installed, so nothing here can resolve one — it makes them **enumerable**, which is the whole gain: the un-checkable set stops being unbounded and textually identical to the checkable one, and becomes a greppable list a human can audit deliberately (#217's control-arm principle, one layer up). The rules below remain the durable guidance, and still apply verbatim to any *future* pair of registers:
- **Cite across registers by a PREFIXED, SINGLE-TOKEN id, never by a bare number**: `GTK4Rs/AP-80`, not a bare 80. One token, not two words — a form with a space in it (the retired two-word `skill AP-N`) is split by any Markdown or `rustfmt` wrap, and a citation a grep cannot enumerate cannot be audited. Length is not the cost here; a wrong cross-reference is.
- **A cross-register mapping is a fact to be looked up, not recalled.** Verify the target entry's *title* says what you are citing it for, at the moment you cite it. The number is not self-validating.
- **When a citation and a correction disagree, check before defending.** The skill maintainer's "those aren't the popover ones" was right and this register's four-times-repeated number was wrong; the repetition was evidence of copying, not of correctness.

**Lesson**: **a broken reference is safe; a reference that resolves to the wrong thing is not.** #143's dangling-pointer failure at least *announces itself* — the target is gone. This one is strictly worse: it stays green forever, reads as diligence, and quietly attributes a claim to a source that never made it. The tell is structural rather than textual, so grepping will not find it: **two independently-numbered namespaces sharing a syntax.** Whenever you write a citation, ask which register's numbering you are in and which one your reader will be in — and note that those are different questions with different answers *in the same file*, which is precisely why this survived four copies and a confident hand-off to the one agent equipped to catch it.

## 146. Assuming `GdkTexture::from_file` ignores installed gdk-pixbuf loaders, and adding a manual `Pixbuf` fallback
**Symptom**: a WebP renders as the broken-image marker even with `webp-pixbuf-loader` installed. The intuitive read — "`GdkTexture` has a fixed native loader set and won't use gdk-pixbuf's runtime loaders" — leads you to add a manual `Pixbuf::from_file` → `Texture::for_pixbuf` fallback.
**Root cause**: **that read is FALSE on GTK 4.6.x** (measured, not assumed). `gdk_texture_new_from_file` → `gdk_texture_new_from_bytes` tries the native set (PNG/JPEG/TIFF), then on an unsupported format FALLS BACK to `gdk_texture_new_from_bytes_pixbuf` → `gdk_pixbuf_new_from_stream` (`gdktexture.c`, 4.6.9) — so `Texture::from_file` **does** consult gdk-pixbuf's installed runtime loaders. A format failing *with the loader installed* is therefore a loader-**registration** problem: the loader is missing from the `loaders.cache` the PROCESS actually reads (stale cache / `GDK_PIXBUF_MODULE_FILE` / sandbox — GTK4Rs/AP-66), even when it is on disk (here the CLI `gdk-pixbuf-query-loaders` printed nothing while the process's cache held the webp entry ×5). And the "fallback" is *less capable*: on 4.6.9, `Pixbuf::from_file` on an animated WebP errors **"Cannot create WebP decoder"**, while `Texture::from_file` and `Pixbuf::from_stream` (the path `Texture` uses internally) both decode it.
**Resolution**: no manual fallback — load via `Texture::from_file` and let its built-in chain handle native + registered-pixbuf formats. If a format won't render despite an installed loader, fix the **registration** (regenerate `loaders.cache`, point `GDK_PIXBUF_MODULE_FILE` at the right one), don't route around `GdkTexture`. **Scope: verified GTK 4.6.9 only** — a later GTK that dropped the internal pixbuf fallback would make a manual fallback version-gated; re-verify before assuming.
**Lesson**: verify which layer already does the work *before* adding a "fallback" — the toolkit call you think is limited may already do exactly what you're about to reimplement, and your reimplementation can be strictly worse (`Pixbuf::from_file` vs the `from_stream` path `Texture` uses). A capability that "doesn't work" is often a registration/config gap, not a missing feature. This entry **retracts** its own first draft (which asserted `GdkTexture::from_file` never consults runtime loaders) — a source-plausible hypothesis that a ten-minute execution probe disproved. Verified: probe on 4.6.9 → `Texture::from_file`=Ok, `Pixbuf::from_file`="Cannot create WebP decoder", `Pixbuf::from_stream`=Ok.
**See**: GTK4Rs/AP-66 (loader-cache registration), GTK4Rs/AP-34a (remote texture sync); #147 (the `<picture>` feature this load path serves).

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

**What was tried**:
- Adding a `nav_generation` token to the converge-scroll tick alone fixed *one* fighter but the desync persisted — proof there were **two** independent drivers, not one.
- Live driving on a real compositor to reproduce (headless Xvfb leaves the window unmapped, so this class needs a real session — cf. #56).

**Root cause**: A programmatic marker navigation runs an `add_tick_callback` that re-aims the vadjustment at its target every frame until convergence, and the card it opens installs a ScrAP-113 re-pin guard that restores *its* saved scroll on every `value-changed` for ~1.5 s. Neither was cancelled when a **new** navigation began, so two (or more) drivers drove the one `GtkAdjustment` at once and the last writer per frame won:
- The converge tick's only stop-condition was `pending_marker_open.is_none()`. But a new navigation **replaces** that slot with its own request (it does not clear it), so the *old* tick never observed `None` and kept pulling the scroll back to its stale target.
- The prior card's re-pin guard kept forcing the scroll to that card's position, and `user_scrolling` could not catch it because a programmatic navigation deliberately sets `user_scrolling = false`.

**Resolution**: A monotonic `nav_generation: Cell<u64>`, bumped at the start of every navigation. Each async driver — the converge tick and the re-pin guard — **captures the generation at the moment it starts** and self-cancels the instant `nav_generation` no longer equals its captured value. Starting a new navigation therefore cancels *all* of an older one's still-running drivers, so exactly one navigation owns the adjustment at a time.

**Scribobulate**: `codeview::CodePreviewView` `nav_generation`; bumped in `open_marker_popover_at`; checked in `converge_and_scroll_to_offset`'s tick and in `open_marker_popover`'s re-pin `value-changed` handler + disconnect-tick. TDD 20.16. Verified on the operator's live session (the timing behaviour is not observable headlessly — #56).

**Lesson**: When several asynchronous drivers can act on **one shared resource** (here a `GtkAdjustment`) across **overlapping** operations, each driver must be able to detect that a *newer* operation has superseded it — and a cancellation predicate keyed on a shared slot being **empty** is the wrong test, because a new operation that **replaces** the slot's contents (rather than clearing it) never makes the old driver see empty. Key cancellation on **supersession**, not emptiness: stamp each operation with a monotonic generation, capture it in every async driver that operation spawns, and have each driver stop the moment the current generation moves past its own. This is the async, wall-clock analogue of #74's stale-index/identity lesson: don't ask "is the slot empty?", ask "am I still the current one?".

---

## 150. A self-drawn decoration re-adding padding that the line's own tags already put inside its `line_yrange`

> *Core GTK4. Candidate for the gtk4-rs skill (GtkTextView `snapshot_layer` self-drawn decorations / `GtkTextTag` `pixels_above/below_lines` / `line_yrange`). Corollary of #21 (self-draw a code block's box because `paragraph-background` can't pad) and a sibling of #84's "who supplies the pixels" confusion.*

**Symptom**: In the rendered preview, a fenced code block's colored card **overlapped the text of the line immediately below it** — the following line's glyphs were painted over the card's bottom edge. Only reproduced in one narrow construct: a **loose (hard-broken) continuation paragraph wedged directly under a code block inside a nested list item** (e.g. a bold `**OR**` line between two fenced blocks of an ordered sub-list). Every ordinary code block looked perfect. Source and editor pane were unaffected — a preview-only overlap.

**What was tried** (the issue was recorded as a lead with the mechanism UNCONFIRMED; it was NOT guessed):
- Two candidate causes were written down and *partitioned by measurement* before any code changed: (a) the code-block box's painted region extending past its own span onto the following line, vs (b) the loose paragraph's line geometry being placed at a `y` that fails to clear the block.
- A headless (Xvfb) `#[gtk::test]` mapped the real preview view at full allocation and dumped, in buffer-Y, each line's `line_yrange` and the card's computed extent. That measurement decided it: card bottom `= line_bottom_y(last) + pad = 112 + 12 = 124`, while the following "OR" line occupied `[112, 130]` — the card reached **12 px into** the next line. Cause (a), quantified.

**Root cause**: The card's vertical inner padding was being supplied **twice**. A code block's first/last lines carry `code-block-top`/`code-block-bottom` tags whose `pixels_above_lines`/`pixels_below_lines` (= the pad) **expand those lines' own `line_yrange`** — so the padding is *already inside* the block's line-range extent (the measured code line was 42 px tall vs a normal 18 px line: 18 + 12 + 12). The `snapshot_layer` self-draw then drew the card at `[line_top_y(start) − pad, line_bottom_y(last) + pad]`, adding the **same** pad a second time. That made the vertical padding 24 px (vs the 12 px horizontal, itself supplied once by the `code-block` tag's `left/right_margin`) and, critically, pushed the card's bottom edge **past the block's last line**. It stayed invisible for every ordinary block because a `block_sep()` blank line sits after them and absorbed the bleed; a loose continuation paragraph inside a list item is separated from the preceding code block by only **one** `\n` (not a `block_sep`), so its real text abutted the card and the extra pad landed on it. (The raw `pad` was also un-zoomed — `config.block_padding as f32` — while the tag pad was `px()`-scaled, a latent zoom mismatch riding along.)

**Resolution**: Draw the card to **exactly** the block's own line-range extent — `[line_top_y(start), line_bottom_y(last_content)]`, no `±pad`. The 12 px inner padding is left entirely to the `code-block-top/bottom` tags (already inside `line_yrange`, and correctly `px()`-scaled), matching how the horizontal padding is already a tag-only concern. This also made the code-block card consistent with the blockquote accent bar drawn a few lines away, which had **always** used the pad-free `[line_top_y, line_bottom_y]` extent — the bar was the working reference the card should have followed. The two loops were unified onto one shared `span_card_y_extent(view, buffer, span, vis_start, vis_end, vtop, vbot)` helper (viewport-clamped per #22), and the regression test calls that **same** helper on the real mapped view's visible range — not an independent recomputation — so it exercises the actual paint formula (#78). Mutation-verified both ways: reintroducing `+ pad` in the shared helper makes the test fail (`card bottom 124 must not reach past next line's top 112`); removing it passes.

**Scribobulate**: `codeview::mod` `snapshot_layer(BelowText)` code-block + blockquote loops; `codeview::geometry::span_card_y_extent`; the `code-block-top`/`code-block-bottom` tags in `tags.rs`. The abutting-`\n` construct comes from `start.rs`'s loose-list-paragraph branch (one `newline()`, not `block_sep`).

**Lesson**: When you self-draw a decoration behind text (#21) and *also* give the text tag-based vertical padding (`pixels_above/below_lines`), remember that **`line_yrange` already includes that tag padding** — the line is physically taller, with the pad inside it. Drawing the backing rect to `[line_top_y(first), line_bottom_y(last)]` therefore *already* wraps the glyphs with the tag's padding; adding a further pad to the rect double-counts it and, on the trailing edge, pushes the rect **outside the span it belongs to**. A decoration must never paint past its own content's line extent: whatever sits on the next line (a blank separator today, real text tomorrow) will be covered. Two tells that this class of bug is present but hidden: a decoration that only misbehaves when *no blank line* follows it (its bleed was being absorbed), and a nearby sibling decoration doing the "same" job with a *different*, simpler extent formula — that sibling is usually the correct one. Decide "who supplies the padding — the tag or the draw?" once, per axis, and let exactly one do it.

---

## 151. Detecting a URL scheme with "the text before the first colon" (`split_once(':')`)

> *Non-core (URL/URI parsing per **RFC 3986 §3.1**, not GTK) — do NOT fold into the gtk4-rs skill. Language/protocol gotcha, transferable to any code that must tell a URL from a local path.*

**Symptom**: A Markdown-referenced **local file whose path contains a colon** silently failed to render or navigate. Images (`![](assets/notes:v2.png)`, `![](C:\pics\x.png)`) drew the broken-image placeholder with alt text suppressed; document links (`[x](report:draft.md)`) went inert. The source and the file were perfectly valid; the reference was simply refused before it ever reached the filesystem. Absolute *or* relative, on Linux as well as Windows — any colon anywhere in the path triggered it.

**Root cause**: The scheme detector was `url.split_once(':').map(|(scheme, _)| scheme)` — it returned **everything before the first colon, wherever that colon fell**, with no check that the prefix is a valid scheme token, that the colon precedes any path separator, or that a hierarchical URL's `//` follows. So a path became a bogus "scheme": `C:\…` → scheme `"C"`, `assets/notes:v2.png` → scheme `"assets/notes"`, `report:draft.md` → scheme `"report"`. None matched the `http`/`https`/`mailto` allowlist, so each fell through to the "unknown scheme → refuse" branch — the security gate for `file://`/`smb://` was mis-firing on ordinary files. (The naive split *looked* correct: it passes every test built from normal `http://…` URLs and normal colon-free paths; only a colon-in-path exercises the gap — and the same broken predicate was **duplicated** at a second call site, so fixing one resolver alone would have left the other wrong.)

**Resolution**: Detect a scheme per RFC 3986 §3.1 — a token `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` immediately followed by `:` — and accept it as a scheme **only** when it is either *hierarchical* (`scheme://…`) or the single bare-colon scheme the app actually launches (`mailto:`). Everything else with a colon is treated as the local path it is and goes through the normal resolve + containment gate. Both call sites were routed through the one shared detector so they can't drift. Security is preserved by **fall-through, not by enumeration**: a dangerous bare-colon scheme (`javascript:`, `data:`) has no `//` and isn't `mailto`, so it is not recognised as launchable — it falls through to local resolution, fails to resolve, and is rendered inert; a hierarchical `file://`/`smb://` still resolves to a scheme and is refused. Mutation-proven: reverting to the naive split makes the colon-in-name resolution test and the `scheme_of` classification test both fail.

**Scribobulate**: `links::scheme_of` (now the single source, shared by `is_allowed_url`, the doc-link gate, and `resolve_image` — the last had inlined its own `split_once(':')`).

**Lesson**: "The part before the first colon" is not a URL scheme — it is a **string operation wearing a parser's clothes**. A colon is a legal character in a filesystem path (a Unix filename, a Windows drive letter `C:`, a relative segment), so splitting on it conflates two namespaces. When you must decide "URL or local path?", parse the scheme to the actual grammar (RFC 3986: leading `ALPHA`, then `ALPHA/DIGIT/+/-/.`, terminated by `:`) and require the disambiguating `//` for hierarchical schemes — or gate the special bare-colon schemes (`mailto:`) by an explicit, tiny allowlist. And when a predicate this load-bearing (it is *both* a resolution gate and a security gate) is copy-pasted to a second site, unify it: two copies of a subtly-wrong check are two bugs, and they drift apart the moment one is fixed.

---

## 152. A deferred idle closure that strong-captures a widget fires against it after teardown — and the reflexive guards each miss

**Symptom**: A `glib::idle_add_local_once` closure that defers a `GtkTextView` scroll (`scroll_to_mark` on the next idle, to let lazy line-height validation settle — the standard #22 deferral) captured the view with a strong `self.clone()`. If the view's window is destroyed in the single idle-priority tick between the scroll being *scheduled* and the idle *firing*, the idle still runs — against a widget whose toplevel is gone. Deterministic, reproduced 4/4 in-suite and again under Xvfb by a mutation test: a real scroll requested, then `window.destroy()` with no main-loop pump, then a later pump. The exact chain (GTK 4.6.9):
```
Gtk-WARNING: Calling gtk_widget_realize() on a widget that isn't inside a toplevel window …
Gdk-CRITICAL: gdk_surface_new_popup: assertion 'GDK_IS_SURFACE (parent)' failed
→ SIGSEGV (signal 11)
```

**What was tried** (three reflexive fixes, each insufficient *alone* — this is the load-bearing part):
- **`WeakRef::upgrade()` + early-return** (the usual `clone!(#[weak])` idiom). **Does NOT fix it.** `upgrade()` tests *liveness only* — it returns `Some` whenever the object is not-yet-*finalized*. But `window.destroy()` **unrealizes the widget subtree synchronously without finalizing it** (it relies on refcount→0), and a strong-capturing idle *pins the view alive* — so at crash time the view is **alive-but-unrooted**, `upgrade()` returns `Some`, and it crashes anyway. WeakRef's real job here is **de-pinning** (a weak-capturing pending source retains nothing, so the view *can* finalize and a later `upgrade()` then correctly returns `None`); it is *necessary but not the crash guard*.
- **Cancel the source from `dispose`.** Wrong hook: the strong capture forms a reference cycle (`source → closure → strong view`), so the view never reaches refcount 0 and `dispose` **can never run** while the source is pending.
- **`SourceId::remove()` on the stored id, assuming a fired source removes as a no-op.** In glib-rs 0.21.x `SourceId::remove()` is `result_from_gboolean!(g_source_remove(id), …).unwrap()` — removing an **already-fired / non-existent** id returns `FALSE` and the `.unwrap()` **panics** (plus a `g_critical`, fatal under `G_DEBUG=fatal-criticals`), and numeric source ids are **recycled** (a stale id can name an unrelated new source).

**Root cause**: The precondition the crash violates is **rooted/realized**, a *hierarchy/phase* state — not *liveness* and not *mapped*. `scroll_to_mark` on an unrealized view drives a popover/native realize whose parent surface is derived from `gtk_native_get_surface(gtk_widget_get_native(parent))`, which is NULL once the subtree is torn down → `gtk_popover_realize` (gtkpopover.c:944-947) hands NULL to `gdk_surface_new_popup` (gdksurface.c:885, `GDK_IS_SURFACE(parent)` assert) → NULL surface deref → SIGSEGV. `WeakRef::upgrade()` (liveness) does not gate this because the zombie is still alive; `dispose` can't cancel it because the strong cycle blocks finalize; a raw `remove()` is unsafe because a one-shot source may already have fired.

**Resolution**: A **combined**, three-facet fix — each addresses a distinct facet and together they are airtight:
1. **Capture a `WeakRef`, not a strong clone, in every deferred idle closure** (including nested inner idles). This removes the pin so no zombie is created and the view can finalize.
2. **After `upgrade()`, gate on the hierarchy state** — `if !view.is_realized() { return; }` (equivalently `root().is_some()`). This is the actual crash guard. Prefer `is_realized()` over the stricter `is_mapped()`: a *realized-but-not-yet-mapped* view (e.g. a freshly opened tab whose deferred scroll is legitimate) is still realized, so `is_realized()` keeps that scroll while still rejecting the torn-down view; `is_mapped()` would wrongly drop it.
3. **Cancel the stored `SourceId` in `WidgetImpl::unrealize`** (the #60 `remove_tick_callback`-in-`unrealize` mirror). `unrealize` fires *synchronously* inside `window.destroy()` — before the `DEFAULT_IDLE`-priority source can dispatch — and `gtk_widget_dispose` also calls `unrealize`, so the one hook covers both teardown paths (unlike `dispose`). Make it **idempotent** (`Option::take` — `unrealize` fires on every realize/unrealize cycle) and **uphold the invariant "slot holds `Some(id)` ⟺ that source is still pending"**: the closure clears its slot to `None` *as its first act on firing*, so neither `unrealize` nor the coalescing re-target ever calls the panicking `remove()` on a fired id. Chain up to `parent_unrealize()`.

Proven by a pre-fix-crash / post-fix-clean regression test plus a mapped-path test showing a live scroll still lands (#22-safe); mutation-reverting the fix reproduces the exact SIGSEGV chain above.

**Scribobulate**: `codeview::geometry`'s `scroll_to_buffer_offset` and `scroll_to_cell_offset` (both idles + the nested inner refine idle, all sharing one `scroll_idle` slot); the cancel lives in `CodePreviewView`'s `WidgetImpl::unrealize`. The file's tick-callbacks already avoided the pin by taking the view from the callback's emitter arg — an `idle_add_local_once` has no emitter arg, which is why these two needed the explicit `WeakRef`.

**Lesson**: A deferred callback that strong-captures the widget it acts on is a #60 reference cycle wearing a scroll's clothes — and "just make it a weak ref" is a *half*-fix. Two invariants to carry: **(1) `upgrade()` proves the object is not finalized, nothing more** — it says nothing about realized/rooted/mapped, so after upgrading a widget you deferred work against, gate on the *hierarchy/phase* precondition the work actually needs (`is_realized`/`root`) before touching it (cf. gtk4-rs Issue #819: "disposed-but-not-finalized widgets … calling anything might crash"). **(2) The teardown hook for a widget-owned deferred source is `unrealize`, not `dispose`** — `unrealize` runs synchronously at `destroy()` and is reached by `dispose` too, whereas `dispose` is unreachable while a strong capture pins the object. And know your binding's `SourceId::remove()` contract: in glib-rs 0.21.x it *panics* on an already-fired or recycled id, so cancellation is only safe under a strict "slot ⟺ pending" invariant maintained by clearing the slot the instant the closure fires.

**See**: gtk4-rs skill → threading-async-and-memory (GTK4Rs/AP-63 / this project's #60 kin) + textview-scrolling-and-adjustments (the #22 deferral this guards). Findings: `researcher-findings-idle-once-strong-capture-widget-teardown-crash.md` (verbatim GTK 4.6.9 / glib-rs 0.21.5 locators).

---

## 153. A `#[gtk::test]` integration suite renders on the default GskGLRenderer, not the renderer `main()` selects — and its GL texture cache SIGABRTs at teardown under a headless display
> *Core GTK4 + automated-UI-testing methodology. Candidate for the gtk4-rs skill's ui-testing module (GSK renderer selection in a headless `#[gtk::test]` harness; GTK4Rs/AP-56 / GTK4Rs/AP-98 kin). Verified GTK 4.6.9 / gtk4-rs 0.10, Xvfb.*

**Symptom**: The full `cargo test --features gtk-integration-tests` suite intermittently aborts (SIGABRT, exit 134) at **process teardown** under Xvfb — roughly 2–3 in 10–30 full runs; re-running passes, and every test passes in isolation. Historically also reported as a SIGSEGV, but that turned out to be a *separate* mechanism (#152, since fixed). The remaining abort's signature:
```
Gdk-CRITICAL: gdk_monitor_get_geometry: assertion 'GDK_IS_MONITOR (monitor)' failed
Gdk-CRITICAL: gdk_texture_new_for_surface: assertion 'cairo_image_surface_get_width (surface) > 0' failed
Gsk-CRITICAL: gsk_gl_driver_load_texture: assertion 'GDK_IS_TEXTURE (texture)' failed
Gsk:ERROR: ../../../gsk/gl/gskgldriver.c:713: gsk_gl_driver_cache_texture: assertion failed: (texture_id > 0)  → abort
```

**What was tried**:
- Blaming a deferred idle surviving `window.destroy()`. That *was* a real, distinct bug (#152) — but it produced a **popover-realize SIGSEGV**, a different signature, and a 30+-run sweep still reproduced the abort after it was fixed. Ruled out.
- A `set_parent`ed popover surviving `dispose` (#80/#144) as the source of the `GDK_IS_MONITOR` criticals — a plausible secondary suspect, but not the abort.
- The decisive step was to stop theorising about *teardown order* and identify the **renderer**: reading `NativeExt::renderer(&window).type().name()` on a realized test window printed `GskGLRenderer` by default, whereas production is always `GskCairoRenderer`.

**Root cause**: `#[gtk::test]` bodies never run `main.rs`, and `main.rs` is the *only* place the app sets `GSK_RENDERER=cairo`. So the harness realizes surfaces under GTK's **default renderer** — `GskGLRenderer` on this X11 system. Under a headless Xvfb display the monitor geometry is unavailable (`GDK_IS_MONITOR` criticals), so a surface can be measured 0×0; `gdk_texture_new_for_surface` refuses a zero-width surface and returns NULL; the GL renderer's texture cache then asserts `GDK_IS_TEXTURE` / `texture_id > 0` and aborts. It is intermittent because it depends on teardown/finalize timing feeding that bad texture into the GL cache at process exit. The Cairo software renderer has no GL driver and no texture cache, so the whole path is *structurally unreachable* under it — which is exactly why production (ScrAP-1/ScrAP-2, always Cairo) never saw it.

**Resolution**: Pin the renderer the app actually ships — `GSK_RENDERER=cairo` — for the entire test harness via `.cargo/config.toml`'s `[env]` table. Cargo applies it to every process it spawns (test binaries and `cargo run`), so it is **one enforced choke point** no individual test has to remember. Setting it per-test would be the ScrAP-116 shape: a forgettable per-call-site workaround whose omission the test's own assertions can't catch (the test still passes; only the teardown-crash immunity silently regresses). Measured on this machine: default GL renderer → **3 SIGABRT in 26 full runs** with the exact signature above; config forcing Cairo → **0 aborts in 20 runs**, `renderer_type == GskCairoRenderer`, and only the benign, non-fatal, renderer-independent `GDK_IS_MONITOR` criticals remained.

**Scribobulate**: `.cargo/config.toml` `[env] GSK_RENDERER = "cairo"`, mirroring `main.rs`'s in-process override; documented in POLICY § Architecture rules. This entry retires a former known-issue register entry — it inlines the mechanism rather than citing that ephemeral ID (#143).

**Lesson**: A gtk-rs `#[gtk::test]` suite renders on GTK's **default** GSK renderer, *not* whatever your `main()` chooses — the test bodies bypass `main()`, so any renderer override there is invisible to them. If the app deliberately ships on the Cairo software renderer, its integration tests must too; otherwise the harness exercises a rendering stack the product never ships and inherits GL-driver failure modes the app is immune to (here, a headless-display zero-size-surface → NULL-texture → GL-texture-cache abort at teardown). Pin the renderer **once at the harness boundary** — `.cargo/config.toml`'s `[env]` table, inherited by every `cargo test`/`cargo run` — never per-test. Corollary to GTK4Rs/AP-56: a headless (Xvfb) environment differs from a real session in more than the window manager — the *renderer* and *monitor geometry* differ too, and either can manufacture a crash with nothing to do with the code under test.

**See**: gtk4-rs skill → automated-UI-testing (GTK4Rs/AP-56 "a clean Xvfb run doesn't prove a GPU/compositor-dependent result"; GTK4Rs/AP-98 "put the feature-gated suite in CI"); architecture-and-rendering (GTK4Rs/AP-1/GTK4Rs/AP-2, the 0-VRAM Cairo stack). #152 (the separate, since-fixed teardown SIGSEGV once conflated with this abort).

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

**See**: gtk4-rs skill → **GTK4Rs/AP-143** (lists-and-models), paired with GTK4Rs/AP-111
(model-level collapse-all).

## 158. A content-less list item still emits a full item (and task marker) — an unconditional per-item gutter decoration draws a stray marker
> *Non-core (pulldown-cmark/CommonMark) — parser event-stream behaviour + rendering logic, not a GTK lesson. Do not fold into the gtk4-rs skill. Sibling of #66/#75/#147 (pulldown event quirks).*

**Symptom**: an empty task-list item `- [ ]` on its own line drew a checkbox in the preview gutter despite having no content. The same shape affected the other kinds: a content-less bullet (`- `) or number (`1. `) also recorded a marker the gutter would draw (a lone `- ` in an otherwise empty document paints a stray dot on the empty line). Markers should render only when the item has content.

**Root cause**: two compounding facts, both non-obvious.
- The renderer pushed a `ListMarker` at **every** `Tag::Item`, unconditionally — nothing gated on the item actually producing content. pulldown-cmark emits `Start(Item)` … `End(Item)` for a content-less item too, so an empty item recorded a marker, and the gutter draw (which iterates every recorded marker whose first line is on-screen) drew it.
- Two pulldown-cmark task-marker quirks sit underneath: `- [ ]` **without** a trailing newline or space is **not** a task at all — pulldown emits literal `Text("[")`, `Text(" ")`, `Text("]")`; only `- [ ]\n` (or `- [ ] `) emits `TaskListMarker(false)`. And that `TaskListMarker` fires for a content-less task item, upgrading the empty item's marker to a `Task` checkbox.

**Scribobulate**: at `TagEnd::Item`, treat the item as empty when the walk inserted **no buffer content** for it (`end_offset() == item_start`) and drop the marker it pushed. The empty item's marker is guaranteed to be `list_markers.last()` — an empty item inserts no text, so it can hold no *surviving* nested item's marker after its own (a non-empty descendant would have advanced the offset; an empty descendant already dropped its own) — guarded by `first_line == item_start`. The ordered counter still advances across a dropped empty item (source-faithful). Verified headlessly: the gutter draws only recorded markers, so `list_markers.is_empty()` for each empty variant is sufficient proof (no live display needed).

**Lesson**: a per-item — or per-block — decoration must gate on the item having produced content, not on the parser having emitted an item. CommonMark parsers emit a complete item (and even a task marker) for a content-less list item, so "the parser didn't give me an item" is the wrong emptiness test; "the render walk produced no output for it" is the right one. And a task marker is doubly a trap: the *same* `- [ ]` source is literal text or a `TaskListMarker` depending only on trailing whitespace the author didn't think about.

**See**: TDD 2.4b (empty items draw no marker); renderer `TagEnd::Item`; sibling pulldown quirks #66/#75/#147.

## 159. Centering a gutter marker on `line_yrange`'s height centers it over ALL of a soft-wrapped item's rows, not the first
> *Core GTK4 (`GtkTextView` geometry) — landed in the `gtk4-rs` skill as GTK4Rs/AP-145.*

**Symptom**: shrinking the preview window until a list item soft-wrapped left the marker (numeric, bullet, or task checkbox) centered vertically against the *whole* multi-line item, floating to the middle row, instead of staying top-aligned on the item's first line. A single-line item looked correct; only wrapping exposed it. More pronounced after zooming in — larger rows make an item wrap at a wider window, and the vertical drift scales with row count.

**Root cause**: the gutter paints each marker at the item's first line, reading `(y, h)` from `GtkTextView::line_yrange` (chosen precisely because it's cache-free and doesn't validate a line mid-snapshot, unlike `iter_location` — ScrAP-22). But `line_yrange` returns the extent of the whole **logical** line (the paragraph), which spans **every soft-wrapped display row**. The marker was centered on `text_top + text_h/2` where `text_h = h − gap` — so for an item wrapped to N rows, `text_h ≈ N · row_height` and the center landed on the middle row. Correct-looking for N=1, wrong for N>1. `line_yrange` has no per-display-row variant, and the deliberate avoidance of `iter_location` (which *could* give a single character's display-row rect) rules out the obvious workaround.

**Scribobulate**: clamp the logical-line height to the first display row before centering: `h' = min(h, gap + single_line_h)`, where `single_line_h` is one row's text height from a fresh Pango layout in the view's own CSS-zoomed font (`view.create_pango_layout("0").pixel_size().1` — cache-free, zoom-correct for free, the same font the ordered numeral draws in) and `gap` is the item's first-line `pixels_above_lines` (`px(list_item_gap)`, one shared definition). A single-row item is byte-identical: its `line_yrange` height already equals `gap + single_line_h` exactly (same integers GTK laid the row out with; `pixels_inside_wrap = 0`), so the `min` is a no-op. Both the drawn marker and the checkbox hit-column derive from the clamped `(y, h)`, so they stay in lock-step. The clamp is a pure function unit-tested headlessly (wrapped 3-row item centers on row 1 at cy=118, not the whole-block midline 134); pixel confirmation on the live view is the final gate (ScrAP-56).

**Lesson**: `GtkTextView::line_yrange` (and `line_at_y`) operate on **logical lines** — a soft-wrapped paragraph is one logical line whose height covers all its display rows. Any per-first-line chrome (a gutter marker, a drop cap, a fold arrow) centered on that height drifts down as the paragraph wraps. There is no cache-free "display-row height" primitive, but a Pango layout of one glyph in the view's font gives the row height without touching `iter_location`'s validation path — clamp `min(logical_h, gap + row_h)` and single-row cases stay exact while wrapped ones pin to the first row. General principle: when a geometry API is documented per *logical* line, confirm whether "line" means paragraph or display row before deriving a position from its height.

**See**: gutter `first_display_line` / `draw_list_marker`; sibling marker-gutter lessons ScrAP-157/ScrAP-158; the `iter_location`-avoidance rationale is ScrAP-22.

## 160. syntect's bundled default syntax set has no TypeScript/TSX/TOML — a fence in one of those languages silently falls back to plain text and renders as one flat colour
> *Non-core (syntect). Not folded into the core GTK skill.*

**Symptom**: a ` ```typescript ` (also `tsx`, `toml`, `kotlin`, `swift`, `dart`) fenced code block in the preview shows as a flat, single-colour block — every token the same ink, i.e. "all gray" — while ` ```js `, ` ```rust `, ` ```python ` highlight normally. The Markdown source is correct; the info string is a standard, widely-recognised tag.

**Root cause**: syntect's `SyntaxSet::load_defaults_newlines()` (its bundled set, derived from Sublime's default packages) does **not** include a TypeScript grammar — nor TSX/TOML/Kotlin/Swift/Dart. The emitter (`renderer/emit.rs::insert_code_block`) resolves the fence via `ss.find_syntax_by_token(lang).unwrap_or_else(|| ss.find_syntax_plain_text())`. For an omitted language `find_syntax_by_token` returns `None`, so it lands on **plain text**, which assigns every line one scope → one foreground colour → a uniformly-coloured block. The fallback is **silent** — no warning, no error — so it is invisibly wrong for exactly the languages syntect happens to omit, and only for those. Verified empirically against **syntect 5.3.0**: tokens `typescript`/`ts`/`tsx`/`toml` → `None`; `javascript`/`js`/`rust`/`python` → `Some(_)`.

**Scribobulate**: build the engine's `SyntaxSet` from **`two_face::syntax::extra_newlines()`** instead of `SyntaxSet::load_defaults_newlines()` (`renderer::syntect()`). `two-face` embeds bat's vetted syntax dump, a **superset** of syntect's defaults: it keeps every bundled grammar (js/rust/python still resolve) and adds the omitted ones (`typescript`→"TypeScript", `ts`→"TypeScript", `tsx`→"TypeScriptReact", `toml`→"TOML"). Two feature traps: (1) use the **`_newlines`** variant — the emitter feeds lines *with* their trailing `\n` (matching the old `load_defaults_newlines`); the no-newline variant would mis-tokenise line-anchored patterns. (2) depend on `two-face` with `--no-default-features --features syntect-fancy`, **not** its default `syntect-onig` feature — the default drags in the `onig`/`onig_sys`/`cc` C toolchain and contradicts this crate's deliberate `regex-fancy` (pure-Rust) syntect choice (Cargo.toml). Guarded by GTK-free `renderer::syntax_coverage_tests` (every common fence token resolves to a non-plain-text grammar; a real TypeScript snippet yields >1 distinct foreground colour). Reverting to `load_defaults_newlines()` fails both.

**Lesson**: a highlight engine that resolves an unknown language by silently falling back to plain text turns "unsupported grammar" into "looks rendered but isn't" — the failure is invisible and per-language. When a code-fence highlighter looks monochrome for *one* language but not others, suspect the syntax set's coverage before the theme or the emitter. syntect's defaults are narrower than they appear (no TS/TSX/TOML/Kotlin/Swift/Dart); `two-face` (bat's assets) is the maintained way to close the gap without hand-sourcing `.sublime-syntax` files whose cross-syntax `include`s may not resolve. **Citations**: syntect 5.3.0 `dumps/` default syntax list (probed: TypeScript absent); `two-face` `syntax::extra_newlines` / feature matrix (`syntect-fancy` vs `syntect-onig`).

**Licensing/attribution obligation (don't skip this half)**: a crate that *bundles third-party asset data* (here `two-face` compiles bat's grammar definitions into the binary) silently pulls those assets' **own** upstream licenses into anything you distribute — not just the crate's crate-level license. `two-face` is MIT OR Apache-2.0, but the embedded grammars are MIT (majority) + Apache-2.0 + BSD-2-Clause (incl. FreeBSD variant) + BSD-3-Clause — all permissive, all **needs-attribution**, none copyleft/source-available. A distributed binary must therefore reproduce those copyright + license notices. `two-face` gives you the exact required text via `two_face::acknowledgement::listing().to_md()` (or the pinned `generated/acknowledgements_full.md`); Scribobulate ships it verbatim as `THIRD-PARTY-LICENSES.md` and surfaces a summary in About ▸ Credits (`add_credit_section`, plain text — a `<url>` there mis-parses as `mailto:`, ScrAP-40). General principle: whenever a dependency embeds data assets (syntaxes, themes, fonts, icons, dictionaries), audit the *assets'* licenses separately from the crate's, and prefer a crate that exposes its acknowledgement text programmatically. Confirmed against the `open-source-license` skill's compatibility matrix + binary-distribution checklist (all-permissive→Apache-2.0 outbound = compatible, no conflicts).

## 161. A CSS `margin-*` silently ADDS to a code-set `gtk_widget_set_margin_*` on the same axis — the stylesheet can never reduce the inset, so `margin: 0` still stops short of the edge

**Symptom**: restyling the tab strip so the ACTIVE tab's opaque background covers the strip's 1px bottom rule — the classic "the selected tab is part of the page below it" notebook idiom — the stylesheet gave dormant tabs a small bottom margin and the active tab `margin-bottom: 0`. Every tab still stopped several pixels clear of the strip's bottom edge and the rule ran unbroken beneath all of them, active included. The stylesheet looked like it was being ignored, but it wasn't: the *relative* 3px difference between dormant and active tabs was applied exactly as written. Only the absolute floor was wrong.

**Root cause**: the tab handle's construction code already set a vertical widget margin (`set_margin_top`/`set_margin_bottom`). A **GtkWidget margin and a CSS margin are separate mechanisms applied at different stages**: the widget margin is subtracted inside `gtk_widget_allocate`, and the CSS box is then laid out inside the rectangle that remains — so the two **add**, and no selector or specificity can arbitrate between them. The arithmetic is exact and diagnostic: with the strip's bottom edge at y=114, the active tab (widget 4 + CSS 0) ended at 110 and dormant tabs (widget 4 + CSS 3) at 107. A CSS margin therefore has a hard floor at the code-set value; `margin-bottom: 0` reads as "no *additional* inset", not "no inset". The tempting next move — a negative CSS margin to cancel the code's — appears to work while silently hard-coding the code-side constant into the stylesheet, so the two drift apart at the next refactor.

**Scribobulate**: delete the vertical widget margins from the handle's construction and express the whole vertical inset in CSS, with a comment at the deletion site recording *why* the axis is stylesheet-only. Horizontal margins deliberately stay in Rust — the strip's layout module measures handle widths for its hit-test and scroll arithmetic, so that axis wants its single supplier on the code side. The shipped design then needs no bottom inset at all: every tab reaches the strip's bottom edge and *continues* the baseline rule with its own 1px bottom border on the same pixel row, and the ACTIVE tab turns that border **transparent** (not `none`, which would shift its label 1px by shrinking the box) so its own fill covers the row and the rule breaks under it alone. Confirmed by sampling pixel columns: dormant border and bare-rail rule land on an identical row; the active tab's fill runs straight through it. Verified in BOTH theme variants — the `shade()`-derived rail/dormant/active ladder stays monotonically recessed→lifted in dark *and* light, which a `@theme_base_color` rail would not have (it is the brightest surface on a light theme). Two adjacent facts were established by the same measurement and are worth keeping: **(a)** CSS margins *are* honoured on children that a custom `LayoutManager` allocates by calling `size_allocate` directly with an explicit rectangle — the widget applies its own box arithmetic inside whatever rectangle it is handed, so a custom layout does not opt children out of the stylesheet; **(b)** GTK paints a node's **inset `box-shadow` before its children**, so a child with an opaque background can cover a rule its parent drew along an inner edge — which is what makes the broken-baseline idiom expressible in plain CSS on a custom widget with no notebook involved.

**Lesson**: **one axis, one margin supplier.** Whenever a visual inset can be specified in two places — a widget property in code and a `margin` in a stylesheet — they compose additively rather than competing, and the code side wins by being a floor the stylesheet cannot lower. So a CSS margin change that "does nothing", or a `0` that refuses to reach an edge, should send you looking for a code-set margin on that axis *before* you reach for a negative value to compensate. Prefer the stylesheet as the supplier for any inset that must vary by widget STATE (only CSS can express "except when active"), and keep the code side only for insets that some other code path must measure. Kin to the "one axis, one pad-supplier" rule for self-drawn block decorations in the `gtk4-rs` skill (GTK4Rs/AP-127): the same failure shape — two independent sources of the same spacing, silently summed — recurs wherever GTK offers both a property and a CSS route to one visual quantity.

**See**: the tab handle's construction (vertical margins removed, rationale in situ) and the tab-strip rules in the app's static CSS; GTK4Rs/AP-127 (one axis, one pad-supplier) and GTK4Rs/AP-101 (GTK's cascade is not web CSS).

## 162. `GtkTextView` reading position drifts toward the top under repeated horizontal resize — the re-wrap re-validation clamp, and the one width-changing path with no re-anchor hook
> *Core GTK4 (`GtkTextView` scroll / `size_allocate` / adjustment clamp) — a new member of the lazy-validation-clamp family (ScrAP-13/14/15/22/65); realises the Reading-Position Preservation CAM's geometry row. NOT yet in the `gtk4-rs` skill — route to the `gtk4skiller` agent.*

**Symptom**: with the preview scrolled into the document, dragging the window's width smaller creeps the reading position **upward toward the start of the file**. The creep is **cumulative**: a single one-shot resize barely moves it, but an interactive drag (many incremental width steps) accumulates the drift. Auto-reload preserves the reading position perfectly across the same document; a bare resize does not — the question that opened the investigation was "why doesn't resize behave like reload?". Diagnostic tell: only **narrowing** drifts; **widening** holds exactly.

**Root cause**: a width change re-wraps the text, so `GtkTextView` re-validates line heights lazily off the frame clock (no "done" signal). While it re-validates, the vadjustment's `upper` is a **transient underestimate** (only the validated span is counted). Since `value ≤ upper − page_size` always holds, a momentarily-smaller `upper` **clamps `value` toward 0** → the viewport creeps up; `upper` grows back as validation catches up but `value` was already clamped and nothing restores it. Each incremental size-allocate pass of a drag applies another clamp, so the drift **accumulates across the drag** rather than jumping in one step. **Narrowing** increases total height (more wrapped rows) and triggers the clamp; **widening** shrinks total height and never clamps `value` toward 0 (GTK preserves `yoffset`), hence the asymmetry. The deeper cause is structural: **every *other* width-changing path re-anchors** — reload, zoom, theme switch, and view-mode switch each capture a reading anchor before the rebuild and restore it after — but a bare **geometry resize has no equivalent re-anchor hook**, so GTK is left to preserve scroll across the re-wrap on its own, which it does poorly. Same lazy-height-validation family as ScrAP-13/14/15/22 and the animation/anchor discipline of ScrAP-65.

**Scribobulate**: the view already tracks the user's **settled top buffer LINE** continuously (maintained for zoom re-anchoring, ScrAP-65). On a genuine width change in the preview's `size_allocate`, re-anchor to that line through the existing **coalesced, deferred, weak-captured, `is_realized`-gated `scroll_to_mark`** path (the outline/find scroll path) — never a one-shot adjustment `set_value`, which the clamp would immediately re-reset. Two guards make it precise: (1) key on the **raw allocation width**, not the content column — zoom changes the margins (hence content width) at the *same* allocation width and owns its own restore, so keying on content would double-drive it; (2) skip the first allocation (no prior line to preserve, and it must not fight the initial fresh-render restore). Keying on the width alone also makes the hook **cause-agnostic**: the same re-anchor covers a split-pane divider drag and a sidebar toggle — *any* change to the preview's allocated width — not only a window resize (CAM rows 8/9 fall out of row 7's fix for free). A geometry event that changes only *height* (an `Automatic` h-scrollbar appearing) doesn't re-wrap, so it correctly triggers nothing. Because `scroll_to_buffer_offset` only *schedules* an idle, it is safe to call from the size-allocate path (ScrAP-22/29 — no synchronous validation there). The **reload path is hardened** in the same spirit: it now captures from the tracked reading **line** rather than the live vadjustment `value`, so a reload arriving mid-resize (during the transient clamp) can't re-anchor the fresh document to the top. When the old preview widget is replaced (Preview-mode reload), the old view's `unrealize` cancels the pending resize idle (ScrAP-152), and the deferred body's cross-buffer mark guard (ScrAP-104) covers the in-place (Split) path — so the resize↔reload interaction is crash-safe *because* both go through the tracked line.

**Verification note (worth keeping — it shaped the confidence)**: the drift is **cumulative and interactive-drag-driven**, so it *under-reproduces* under synthetic drives. A single `xdotool windowsize` narrow — on Xvfb **or** the real KWin session — moved the viewport only ~2–3 sections, never a dramatic jump; the full symptom needs a genuine border-drag. A headless `#[gtk::test]` that maps and pumps to full allocation settles the very validation race and yields a **false PASS** (the `gtk4-rs` skill's GTK4Rs/AP-78 full-allocation masking). What established the fix was a **mutation test** (re-anchor disabled → drift reappears; enabled → holds exactly) run at three fidelities: Xvfb+openbox (no-fix 085→083 vs with-fix held), synthetic `:0`/KWin (no-fix 097→094 vs held), and — decisively — a human hand-drag on the operator's real `:0`/KWin session (2400→1130px, text visibly re-wrapped, viewport held at the same section). The widen-vs-narrow asymmetry is the cheapest reproduction check.

**Lesson**: a **geometry change (width) is a first-class scroll-perturbing event**, distinct from a buffer swap — and it is the one width-changing path GTK will *not* re-anchor for you. Preserve a reading position across **any** re-layout (re-wrap *or* rebuild) by capturing a buffer-**LINE** anchor and restoring it after validation through **one** choke point — never a pixel fraction (mixes tall/short lines → drifts) and never a one-shot `set_value` (the lazy-validation `upper` clamp re-resets it). Resize-driven drift is **cumulative** across a drag's many size-allocate passes and appears only in the height-**increasing** direction (narrowing), so a single-step or headless test under-reproduces it — mutation-test the guard and confirm on a real compositor with an interactive drag (GTK4Rs/AP-56 verify-on-real-session / GTK4Rs/AP-78 mutation-test-the-guard). New member of the lazy-height-validation family (ScrAP-13/14/15/22/65).

**See**: `sdd/CAM.md` Reading-Position Preservation CAM (row 7 — geometry change); the preview view's `size_allocate` raw-width re-anchor + `reanchor_to_reading_line`, its continuous `reading_line` tracker, and the reload capture that reads `reading_line()` not the live value; the deferred-work meta-pattern in this file ("Lazy height/layout validation" family and the "Preserve a reading position across a re-render" cheat-sheet row — now also *across a resize*).

## 163. Switching a `GtkLabel` to `set_markup` silently makes every interpolated string a Pango-markup injection/breakage surface — an un-escaped filename metacharacter renders the label EMPTY, with no crash
> *Non-core (Pango markup) — sibling of ScrAP-4/115/117. Not folded into the core GTK skill.*

**Symptom**: adding a coloured "⚠" deleted-backing badge to a tab required a per-glyph colour, which a plain-text `GtkLabel` (`set_label`) can't express — so the tab label was converted to Pango markup (`set_markup`) with the "⚠" wrapped in a `<span foreground="#e5a50a">`. The badge itself renders fine. The trap is elsewhere and latent: the label also interpolates the document's **filename**, and a filename may legitimately contain `&`, `<`, or `>` (e.g. `A&B.md`, `<draft>.md`). Under `set_markup` such a name is now parsed as markup — GTK emits a `Pango`/`Gtk-WARNING` ("Failed to set text from markup…") and the label renders **empty or truncated**, with **no crash and no compile error**. A one-line `set_label`→`set_markup` change quietly reclassified every runtime string in that label from inert text to markup.

**Root cause**: `gtk_label_set_markup` runs the string through `pango_parse_markup`, which treats `&`/`<`/`>` as entity/tag syntax. `set_label` does not — the two entry points look interchangeable (both "set the label's text") but have opposite escaping contracts. There is no type-level distinction (`&str` either way), so the compiler can't flag the un-escaped interpolation; the failure only appears at runtime, only for names containing a metacharacter, and only as a soft warning + a blank badge — exactly the "invisible, per-input, silent-fallback" failure shape this register keeps meeting (cf. ScrAP-160's silent syntax fallback).

**Scribobulate**: the tab strip's label is now markup, so the single funnel that composes it (`window/tabs/documents.rs::tab_display_markup`) escapes the filename with `glib::markup_escape_text` **before** interpolation, and the pure label formula (`winstate::decisions::tab_label_markup`) takes the **already-escaped** name plus a caller-supplied `warn_color` and only assembles the markup — so it stays display-free and unit-testable (it decides badge ORDERING — ⚠ before ⟳ before name before •, and "which yellow" is not its concern). The label widget starts life empty (`Label::new(Some(""))`) and is only ever populated through this one `set_markup` funnel, so there is no second, un-escaped write path. The badge colour is a fixed app constant (`#e5a50a`, Adwaita "yellow 5") rather than a reading-theme key, because the tab strip wears the desktop GTK theme, not the preview's reading theme (TECH "the reading theme is preview-only").

**Lesson**: `set_label` and `set_markup` are **not** drop-in swaps — converting a label to markup silently makes every interpolated runtime string an escaping obligation, and the penalty for forgetting is a **blank/garbled label + a soft warning**, never a crash, so a happy-path test with an ASCII filename passes while `A&B.md` breaks in the field. When a label must carry *any* styled fragment, funnel its construction through **one** builder, escape every non-literal fragment there with `glib::markup_escape_text`, and keep the styling colour/attributes as parameters so the string-assembly core stays pure. General rule for GTK text APIs: confirm whether a setter's contract is plain-text or markup before interpolating user- or filesystem-derived data into it. Pango-markup sibling of ScrAP-4 (`<a href>` in labels), ScrAP-115 (highlighting inside an existing markup string), and ScrAP-117 (same-string `set_markup` is a no-op).

**See**: `winstate/decisions.rs::tab_label_markup` (pure, escaped-name + colour param) and its unit tests; `window/tabs/documents.rs::tab_display_markup` (the escaping funnel + badge colour constant); `widgets/tab/{view,bar}.rs::set_tab_markup`/`set_markup` (the single `set_markup` write path); TDD 15.22.

---

## 164. Committing a test fixture whose filename is itself the invalid input breaks checkout on other platforms
> *Non-core (testing/CI + cross-platform filesystem, not GTK) — do NOT fold into the gtk4-rs skill. Transferable to any repository whose test suite runs on more than one OS.*

**Symptom**: a committed regression fixture, `tests/fixtures/report:draft.md`, was deliberately named with a colon to exercise ScrAP-151 (a colon in a path is a local file, not a URL scheme) — a link to it in `doc-links.md` was clicked in a manual test to prove it *navigates*, not gets refused as a scheme. On Linux everything worked. On **Windows** the whole repository became **un-cloneable**: `git clone`/checkout fails to write the working-tree file because NTFS reads `:` as the separator for an alternate data stream, so the name is illegal. The failure is at checkout, before a single line of app code — or of the test that fixture serves — ever runs.

**Root cause**: the fixture encoded the invalid input *in the filename of a tracked file*. A tracked file is materialised in every checkout on every platform, so its name must be legal on the **intersection** of all target platforms' filename rules — the most restrictive wins, and Windows forbids `< > : " | ? *`, a trailing dot or space, and the reserved basenames `CON`/`PRN`/`AUX`/`NUL`/`COM1‑9`/`LPT1‑9`. Crucially, a committed file has **no per-platform escape hatch**: the equivalent unit test that *creates* a colon-named file in a temp dir at run time is already `#[cfg(unix)]`-gated and simply doesn't run on Windows, but there is no analogue for a tracked file — `.gitattributes`, sparse-checkout, and `.gitignore` can't rename it or make it conditional, and it is present the instant you clone. The name being tested *looked* like inert test data; it was actually a path constraint on every consumer of the repo.

**Resolution**: retire the fixture and move the invariant into code, split by what each half actually needs. The load-bearing half is *classification* — "a colon is not a URL scheme" — which is pure string logic, so it is unit-tested cross-platform with in-memory **string literals** (`links::scheme_of("report:draft.md") == None`, `!is_allowed_url("report:draft.md")`): no file, runs on Windows too. The *navigation* half needed no colon-specific fixture at all: the doc-link resolver never sees a scheme (the caller strips it first via `scheme_of`), so a colon path resolves exactly like any other sibling, already covered by the non-colon `resolve_doc_link` tests. The one place a real colon-named file is still made — the `resolve_image` colon test — creates it in a temp dir at run time and is `#[cfg(unix)]`-gated, so it never touches a Windows checkout. The manual suite drops the fixture and points §19.7a at the unit tests. Swept the whole class, not just the reported name: `git ls-files -z | tr '\0' '\n' | grep -nE '[<>:"|?*]|[ .]$'` plus the reserved-basename check found no others.

**Scribobulate**: `src/links.rs` tests (`scheme_of`/`is_allowed_url` string literals = the cross-platform guard; the `#[cfg(unix)]` `resolve_image` colon temp-file test = the run-time-file precedent); TDD §19.7a (now marked unit-verified, fixture-free); `tests/fixtures/doc-links.md` + `tests/MANUAL-TEST.md` (colon case removed / reframed).

**Lesson**: a test fixture's **filename is a path, not free-form test data** — every checkout on every target OS must be able to write it, so it inherits the strictest platform's naming rules, not the author's. When the thing under test *is* an illegal or edge-case name (a colon, a reserved word, a trailing dot, an over-long component), put the offending string in **code** — a literal for pure logic, or a run-time temp file guarded by `#[cfg(target_os)]`/a skip when a real file is unavoidable — **never** in a committed filename, which can't be platform-gated and fails at `git clone` before any assertion. And when you do hit one, sweep for the class (`git ls-files | grep -nE '[<>:"|?*]|[ .]$'` + reserved basenames), because the same reflex that created one usually created more.

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
> **The next free number is 197.** Do not derive it from the highest entry below —
> unmerged branches hold ranges that are invisible from this file, which is exactly
> how a collision happens. Check the table, **announce the range you are claiming**,
> and never fill a reserved gap.

## 165. Clearing an env var the wrong way gives a false confirmation
> *Non-core (tooling/CI + Windows shell semantics, not GTK) — do NOT fold into the gtk4-rs skill. Transferable to any Windows build driven from PowerShell.*

**Symptom**: gvsbuild's `gettext` step failed with `'create-lists.bat' is not recognized as an internal or external command`, repeated, then `create-lists-msvc.mak(94) : fatal error U1052: file 'gettext-runtime-objs.mak' not found`. It reads unmistakably as a corrupt or incomplete source tree. The file was present the whole time. `vswhere.exe` failed the same way in the same run.

**Root cause**: two layers. The outer one is `NoDefaultCurrentDirectoryInExePath` — when that variable is **defined**, `cmd.exe` stops searching the working directory for an executable, and gettext's `Makefile.vc` invokes `create-lists.bat` by bare name. Hardened, sandboxed and CI environments set it. The inner one, which cost the extra builds, is that the obvious way to clear it **does not work**: `[Environment]::SetEnvironmentVariable('NoDefaultCurrentDirectoryInExePath', $null)` leaves the variable *defined with an empty value*, and `cmd.exe` tests **whether it is defined**, not what it contains. Only `Remove-Item Env:NAME` actually deletes it. **Both forms make `$env:VAR` print empty**, so the natural verification step reports success in both cases — the fix appears applied, the build fails identically, and the failure looks inexplicable rather than unfixed.

**Resolution**: clear it with `Remove-Item Env:NoDefaultCurrentDirectoryInExePath -ErrorAction SilentlyContinue` before invoking the build, and **verify with a behavioural probe, never with `$env:`** — drop a `.bat` in a scratch directory and check whether `cmd /c` finds it by bare name.

**This recurs, and the scope it recurs at is the point**: measured again on a second Windows box (2026-07-29), the variable was **defined in the inherited process environment while empty at both Machine and User scope** — i.e. it comes from the agent/CI session, not from anything configured on the machine, so a clean box is no defence and inspecting the machine's own settings will show nothing wrong. Any script that drives gvsbuild must clear it itself.

**Scribobulate**: `packaging/windows/README.md` ("Two things that will bite you"), which states the non-working alternative explicitly so nobody re-derives it.

**Lesson**: when a fix is applied by *mutating an environment*, verify it by **observing the behaviour it was meant to change**, not by reading the value back. A read-back shares the same failure mode as the write — here, two different mutations produce an identical, apparently-correct reading, so the check has no power to detect the broken one. The general shape: *a verification that cannot fail when the fix fails is not a verification.* Prefer "does the thing now work?" over "does the variable look right?".

## 166. Never diagnose a hung test suite from a parallel run
> *Non-core (testing/CI process, not GTK) — do NOT fold into the gtk4-rs skill. Transferable to any Rust project using libtest.*

**Symptom**: `cargo test --features gtk-integration-tests` never completed on Windows. Killed after ~10 minutes, it had printed **no test names at all** and emitted a flood of one warning. "Zero test names" was reported as evidence that it hung *at or before the first test*, and both a reviewing agent and this one reasoned from that: the leading hypothesis became a main-loop or renderer stall during startup, and a plausible, well-argued case was built on it.

**Root cause**: the evidence was an artefact. **libtest buffers each test's output per thread under parallel execution and only flushes it when that test completes**, so a suite that wedges partway through prints nothing at all — indistinguishable from one that never started. Re-run with `--test-threads=1`, the same suite ran *hundreds* of tests, printed names throughout, and wedged on one named test. Every hypothesis built on "hangs before the first test" was answering a question that never applied.

**Resolution**: bisect serially. `--test-threads=1` named the culprit in one run; running that test alone under a hard OS timeout, with checkpoint prints across its call chain, then showed the test body *completing* and the process failing to exit afterwards — a livelock in teardown, not a blocked call, which also invalidated "attach a debugger and look at the stuck frame". *(The culprit turned out to be an app-side popover left `set_parent`ed on an editor handed to another window, wedging `gtk_text_view_dispose`'s drain loop — see ScrAP-90 and ScrAP-175. It is fixed; the lesson here is the serial-bisect **method**, which is what found it and is unaffected by the fix.)*

**Scribobulate**: `sdd/ISSUES.md` entry P (records the corrected diagnosis and the eliminated hypotheses as do-not-revisit).

**Lesson**: **absence of output is not evidence of absence of progress.** Before theorising about *where* a suite hangs, re-run it serially so the harness is forced to tell you how far it got — one cheap run converts "no information" into a named culprit and retires whole families of hypothesis. More generally: when a symptom is *silence*, first ask what could suppress the signal, before asking what could stop the work. Two capable reviewers reasoned carefully and at length from a measurement artefact, which no amount of reasoning could have corrected.

## 167. An `Option`-returning lookup whose `None` is also a legitimate answer will fail silently forever
> *Non-core (cross-platform/XDG conventions, not GTK) — do NOT fold into the gtk4-rs skill. Transferable to any project with platform-specific user directories.*

**Symptom**: on Windows the app never saved or restored window geometry, tabs or view state, and never read the user's `config.toml` or their theme overrides. Every launch started from defaults. There was no error, no log line, and nothing in the UI. It survived an entire platform port — build, packaging, installer, CI — unnoticed, and was found only by chasing an unrelated question about window geometry.

**Root cause**: both `config::user_config_dir()` and `session::session_path()` resolved their base directory through `XDG_*` with a fallback to `HOME` — POSIX conventions Windows sets neither of. Each returned `None`, and every caller degraded to doing nothing. The *silence* is the real defect: `None` from these lookups is **also the correct answer for "the user simply has no config file"**, a state the app must handle gracefully, so the unsupported-platform case was indistinguishable from the ordinary one. Nothing in a build or test run can notice, because the app behaves exactly as an unconfigured app should. Note too that a `#[cfg(windows)]` test asserting on a path would **not** have caught it — the failure was that no path was produced at all — and every existing session test went through a helper that sets `XDG_STATE_HOME`, masking it precisely.

**Resolution**: keep `XDG_*` first on every platform (a test helper sets it at runtime and needs a live read) and branch only the *fallback* — `HOME`-relative on unix, `%APPDATA%` for config and `%LOCALAPPDATA%` for session on Windows. Both sites now `log::warn!` **once** when no directory resolves. Added `state_directory_resolves_without_any_xdg_override`, which removes the override and asserts a path is still produced — the shape that would actually have caught this.

**Scribobulate**: `src/config.rs` (`config_home_fallback`), `src/session.rs` (`state_home_fallback`, `state_directory_resolves_without_any_xdg_override`); `sdd/TECH.md` § platform notes.

**Lesson**: when a fallible lookup has an outcome that is **both a normal state and a total failure**, the two must be distinguishable — log the failure, or return a type that separates "absent" from "unresolvable". Otherwise the failure is invisible by construction and no test can see it, because the system is behaving exactly as designed for the benign case. And when writing the regression test, assert on **the thing that was missing** (a path was produced at all), not on the value you expect it to have — a test that asserts a *specific* path would have passed on the platform that worked and never run on the one that didn't.

## 168. A popover's layout pass resizes the TOPLEVEL — from GTK's stale remembered size — collapsing a natively-maximized window
> *Core GTK4 (GDK-Win32 toplevel sizing × `GtkWindow` remembered size × shared frame clocks). **NOT yet in the `gtk4-rs` skill — route to the `gtk4skiller` agent.** Verified against the shipped GTK 4.22.4 sources and measured live on Win11 26200. Only reachable with server-side decorations (`GTK_CSD=0`), which is why the whole GTK-on-Windows world has not tripped over it; the frame-clock half is backend-independent and the family (a child surface driving the parent's geometry) is not.*

**Symptom**: maximize the window with the **title bar's own** maximize button, then open any popover — a menubar menu, a `GtkDropDown`, a right-click `GtkPopoverMenu` — and the window snaps down to whatever size it had before it was maximized. It does **not** come back when the popover closes, the title bar still shows the *restore* glyph, `IsZoomed` still returns true, and the screen keeps a band of stale pixels where the window used to be. Measured: 4112×2128 → **1872×1052 with `WS_MAXIMIZE` still set**. Typing, clicking and every other interaction leave the geometry alone — only a popover does it, which makes it read as a menu bug and sends you into the menubar code, where there is nothing wrong.

**Root cause**: four independently reasonable pieces of GTK, composing into a hole.
- A `GtkPopover` is a `GtkNative` with its **own** `GdkSurface`, and a popup surface **shares its parent's frame clock**. `gdk_surface_layout_on_clock` does not check whose request raised the phase — it clears `pending_phases` and runs `compute_size` for **every** mapped surface on that clock. So the popup's layout request drags the toplevel through `gdk_win32_toplevel_compute_size` too.
- The toplevel never asked for that pass, so its `next_layout.configured_width/height` are still the zeros the previous `compute_size` left behind (it zeroes them on the way out). `compute_toplevel_size` honours a configured size when there is one — `*width = MAX(size.min_width, desired_width)` — and only falls back to `size.width` when there is not. This pass is exactly the "there is not" case.
- `size.width` is whatever `GtkWindow`'s `compute-size` handler returns, and that is `gtk_window_compute_default_size(priv->default_width, …)` — the **remembered** size. `should_remember_size()` returns false while maximized *by design*, so the remembered size is deliberately frozen at the **pre-maximize** value. It differs from the current size, `needs_resize` goes true, and GDK calls `gdk_win32_surface_resize` — a bare `SetWindowPos`, which changes the geometry **without clearing `WS_MAXIMIZE`**. Hence the impossible-looking end state.
- The branch in `compute_toplevel_size` that clamps the computed size up to the monitor work area — which would have masked all of the above — is gated on `gdk_toplevel_layout_get_maximized()`, and that flag is only ever set from `gtk_window_maximize()`/`unmaximize()` via `priv->is_set_maximized`. **Maximizing from the OS's own button never calls either.** Under CSD, GTK draws the maximize button and so always does; taking the native frame is what moves you onto the unclamped path.

**The obvious fix is wrong — do not spend a build on it.** Mirroring the OS state back with `gtk_window_maximize()` from a `notify::maximized` handler sets `is_set_maximized` and re-enables the clamp, and is the *supported* API, so it is where everyone will start. It fails twice. First, it calls `gdk_toplevel_present`, and `gdk_win32_toplevel_present` resizes the surface to the remembered (small) size **first** and only then calls `ShowWindow(SW_MAXIMIZE)` — which Win32 no-ops on a window that already carries `WS_MAXIMIZE`, so it performs the very shrink it was meant to prevent and cannot undo it. Second, the clamp it unlocks is **CSD-shaped**: it clamps to the *work area*, which equals the client area only when GTK draws the frame. With a native frame the client area is the work area minus the caption, so the clamp overshoots and pushes the bottom edge under the taskbar.

**Scribobulate**: `platform::win32::track_maximized_size` — while the window is maximized, and only then, keep GTK's remembered size equal to the size the OS actually gave it (`surface`'s `layout` signal → `set_default_size`). The fallback path then computes the size the window already has, `needs_resize` stays false, and **no resize happens at all**: no clamp, no `present`, no flicker, one comparison per layout pass. Nothing restores the remembered size on unmaximize — `should_remember_size()` becomes true the moment the state clears, so GTK resumes maintaining it from the first layout pass afterwards, and the *OS* (`WINDOWPLACEMENT`), not GTK, holds the rectangle a restore returns to. That last fact is why this is safe on Win32 and would **not** be on X11/Wayland, where GTK's remembered size *is* the restore target — hence `#[cfg(windows)]`, not a portable "fix". The decision core is the pure `remembered_size_while_maximized`, unit-tested for both directions (the inverted form — writing while *un*maximized — is the real regression risk: it would make restore land on the maximized size).

**Lesson**: **a child surface can drive its parent's geometry.** Popups share the toplevel's frame clock and GDK's layout phase is per-*clock*, not per-*surface*, so opening a popover runs the toplevel's size computation on a pass the toplevel never requested — with none of the inputs that pass normally has. Any state that is only refreshed "when we ask for a layout" is therefore live at moments you did not choose. Two corollaries. First, **a value a framework deliberately freezes is a landmine for any fallback that reads it**: `should_remember_size()` freezing the remembered size while maximized is correct in isolation and correct at its intended reader, but a second, unrelated code path reaching for the same field gets a value that is stale *on purpose* — when you find a "sensible default" being read from far away, check whether someone is deliberately holding it still. Second, **a masking clamp is not a fix, and finding out who unlocks it is the whole diagnosis**: this bug is invisible under CSD not because CSD is correct but because GTK happens to call the API that sets the flag, so the moment the platform's own chrome takes over a job the toolkit used to do, every invariant the toolkit maintained *as a side effect of doing that job* silently stops holding. That is the shape to look for after adopting native decorations anywhere — not "what did we lose", but "what was the toolkit quietly keeping true".

**See**: TDD 7.0d / MANUAL-TEST 7.0d (the running-window check, incl. the unmaximized control); POLICY § Architecture rules (why the native frame was adopted, and that `src/platform/win32/frame.rs` also owns native-frame repairs needing no OS call). Sibling of ScrAP-167 — both are the same porting shape: something the toolkit did for us until the platform's own conventions took the job.

> **165–168 and 175 landed here from the Windows port.** Anti-pattern numbers are
> frozen IDs, so slots are held for entries that currently live on in-progress
> branches rather than being recycled — recycling one would give two different
> lessons the same ID the moment those branches land.
>
> **Claimed / reserved across all branches** (measured 2026-07-28, not recalled):
>
> | Range | Holder | State |
> |---|---|---|
> | 165–168, 175 | Windows port | landed here |
> | **176–179** | **Windows port** | **claimed, unused** |
> | 171, 174 | macOS port | reserved — macOS-specific |
> | 173, 180, 181 | macOS port | written; platform-neutral, bound for master |
>
> **The next free number for a NEW claimant is 182.** Do **not** take "the next
> number after the highest in this file" as a rule in its own right — it is wrong
> here in both directions: it would land you on 176, which is already claimed, and
> it would have collided with the macOS port before that. Check the table, announce
> the range you are claiming in the shared room, and never fill a reserved gap.

## 169. A pruned `-symbolic` icon name degrades to a legacy raster instead of failing — and `has_icon` is *stricter* than the render path, so the audit scores it green

**Symptom**: on a host carrying a recent Adwaita (first measured on GTK 4.22.4 / adwaita-icon-theme 50) a batch of toolbar/menu icons rendered the broken-image placeholder. An audit over the app's whole icon-name table using `IconTheme::has_icon` reported 16 names missing. Installing the icon theme the platform lacked closed 14 of them, leaving 2 — but the *running app* only ever showed one placeholder. One name (`emblem-synchronizing-symbolic`) was reported missing by `has_icon` while its button rendered perfectly normal art.

**What was tried**:
- Trusted the running app's `GTK_DEBUG=icontheme` trace as the ground truth. It under-reports: it only shows names actually instantiated in that session (34 of 49 here), so menu-only and mode-only icons never appear and the gap looks smaller than it is.
- Trusted `has_icon` over the full table instead. It over-reports in the other direction, and for a subtly worse reason (below).
- Assumed the two disagreeing answers meant a stale build or a caching artifact. They didn't — both were correct about different questions.

**Root cause**: two separate mechanisms, pulling opposite ways.
1. GTK's icon lookup expands one requested name into a *candidate chain* — for `X-symbolic` it tries `X-symbolic-ltr`, `X-symbolic`, `X-ltr`, then **`X`**, and only then `image-missing`. So when a theme drops the symbolic SVG but keeps a legacy full-colour raster of the same base name, the request quietly resolves to that raster. It renders. It is also **not symbolic**, so it no longer follows the theme foreground — the failure mode is a baked-colour icon that will go invisible or muddy on the opposite light/dark variant, which is exactly the hazard the `-symbolic` suffix existed to avoid. Nothing warns.
2. `has_icon` tests the **exact name only** — it does not walk that chain. So it answers FALSE for a name that renders fine. That makes it *stricter* than the render path, the opposite direction from the usual "resolves ≠ renders visibly" caveat, and it means a `has_icon`-based audit produces both false alarms (a name that renders, degraded) and — with the raster present — a green score on the one case that genuinely needs fixing.

The upstream trigger was theme-version drift, not the platform: the icon theme's own `NEWS` pins the removal at **`48.alpha` — "symbolic: remove emblems (issue #287)"**, with `50.rc` — "reintroduce legacy icons because themes" — restoring some as *rasters*, which is precisely why one name half-works. A box on an older theme (measured: 41, and the GNOME-46 runtime) still has real symbolic SVGs for both and cannot reproduce this at all.

**Resolution**: audit by **rendering**, not by querying. The icon audit now rasterizes every name through the real `IconTheme` → `IconPaintable` → GSK Cairo path and prints *the file each name actually resolved to*, so "resolved to a legacy raster in the theme" and "resolved to our bundled symbolic SVG" are distinguishable at a glance and an empty render is caught. The two pruned names are bundled as real symbolic SVGs in the app's GResource, which also pre-empts the degradation: the exact `-symbolic` candidate is tried *before* the stripped one, so the bundled SVG wins over the legacy raster while a host theme that still ships the name continues to win over both.

**Scribobulate**: `tests/icon_resolution.rs` (the render-and-report audit, plus `--render`); `data/resources.gresource.xml` + `data/icons/scalable/emblems/` (the bundled replacements); `src/icons.rs` (the name table the audit walks).

**Lesson**: a missing icon name is not a binary. Between "renders correctly" and "renders the placeholder" sits a silent middle state — resolved via the toolkit's name-fallback chain to a *different asset with different properties*, most damagingly a non-symbolic one that stops tracking the theme foreground. Any predicate that checks a name rather than a render can't see that state, and `has_icon` in particular is exact-name-only, so it is unreliable in **both** directions: it says false where the render succeeds, and (when a legacy asset exists) it cannot say that the render succeeded *wrongly*. Audit assets by producing the pixels and reporting the resolved source path. And when a name disappears, pin the *version* it disappeared in from the upstream changelog before describing the scope — an inferred version range reads as authoritative and is the part that will be wrong.

## 170. Symbolic icon art drawn with strokes silently changes shape — the SVG rasterizer you preview in is not the renderer that ships it

**Symptom**: while drawing replacement `*-symbolic` icons, the working assumption was the usual one — GTK recolours symbolic icons, so a stroked outline would come out in the wrong colour and vanish on a dark theme. Wrong in a more interesting way: previewing the art through a plain SVG rasterizer and through GTK's own icon pipeline produced **different geometry** from the identical file.

**What was tried**: the design loop rasterized each candidate with `rsvg-convert` and judged it there — the natural workflow, and the one an icon-design skill prescribes. It is honest for a normal SVG and misleading for a symbolic icon, because the shipping path inserts a transformation the preview doesn't have.

**Root cause**: GTK wraps a symbolic SVG in a generated stylesheet before rendering it, and that stylesheet's first rule is

```
rect,circle,path { fill: <fg> !important; }
```

The `!important` is the whole story: it outranks an authored `fill="none"`, so every enclosed region of a stroked outline **fills in**. Measured on GTK 4.22.4, one file rendered two ways — a stroked circle with `fill="none"` is a **hollow ring** under `rsvg-convert` and a **solid disc** through GTK's symbolic path. So stroked outline art doesn't lose its colour, it loses its *shape*, silently, from valid SVG that "renders".

**Version scope** (this entry was re-measured on a second box before being trusted, and the versions disagree):
- **Force-fill: confirmed on BOTH 4.22.4 and 4.6.9** — the durable, version-independent half. On 4.6.9 the wrapper CSS above was read directly out of the shipped `libgtk-4.so.1`, and the ring fixture reproduces identically (centre alpha 0 → 255).
- **Stroke dropped unless it opts in via `class="foreground-stroke"`: GTK ≥ 4.22 only (measured).** On 4.6.9 it does **not** happen: a stroked straight line renders pixel-identically to the plain rasterizer, and the class is inert — `foreground-stroke` does not appear anywhere in the 4.6.9 binary. Stroke handling was evidently added to the wrapper later. So on an older GTK a hairline stroke survives while enclosed shapes still fill in — the trap is the same, the symptom is only half of it.
- Fills-only art is byte-identical through both renderers on both versions, so the pattern below is correct regardless of target.

**Resolution**: draw symbolic art as filled shapes only — an outline becomes a closed band (an outer arc out, an inner arc back), a frame becomes four filled bars — and verify through GTK's icon pipeline rather than the bare rasterizer. The project's icon audit renders each name through `IconTheme`/`IconPaintable` for exactly this reason, which is also what confirms the bundled art is correct in situ. (`class="foreground-stroke"` is the supported opt-in on 4.22+ if stroked art is genuinely wanted, but it is a no-op on older GTK, so it is not portable; filled art needs no opt-in and is what the reference themes overwhelmingly use — 641 of 645 symbolic icons in the installed set carry no stroke at all.)

**Scribobulate**: `data/icons/scalable/emblems/*.svg` (both bundled icons are fills-only, with the mechanism recorded in each file's header); `tests/icon_resolution.rs --render` (renders through the shipping path).

**Lesson**: when an asset is transformed by the toolkit before it is drawn, previewing it in a generic renderer for that format validates the wrong artifact — and the divergence can be structural, not cosmetic. Confirm what the *shipping* pipeline produces at least once before committing art. More generally: when a plausible mechanism ("strokes won't be recoloured") and the observed behaviour agree on the *remedy* ("use fills"), that agreement is not evidence the mechanism is right — here the remedy was correct and the reasoning behind it was wrong, which would have quietly mis-taught the next person to reach for `class="foreground-stroke"` as a fix for a colour problem that was never the problem.

## 171. Every `#[gtk::test]` aborts on macOS before its body runs — the harness dispatches onto a worker thread, and GTK there requires the main one

**Symptom**: running the GTK integration suite on macOS, every test fails immediately with `assertion left != right failed: Attempted to initialize GTK on OSX from non-main thread`, followed by `GTK has not been initialized. Call gtk::init first.` from whatever the body touched. No test body executes. The same suite is fully green on Linux under Xvfb.

**What was tried**: `--test-threads=1`, on the reasonable theory that libtest's serial mode runs tests on the main thread. It changes nothing — same panic, same place.

**Root cause**: the binding's test attribute doesn't run the body inline. It routes every body through a helper that lazily creates a **one-thread `glib::ThreadPool`**, calls `gtk::init()` on that worker, and dispatches each test onto it (this is deliberate: GTK is single-threaded, so the harness gives all tests one consistent thread instead of letting libtest's pool spread them). `gtk::init()` in turn asserts, on macOS only, that it is running on the process main thread — because macOS's windowing system genuinely reserves it. The two designs are individually correct and jointly fatal. `--test-threads=1` can't help: it governs how many tests libtest runs concurrently, not which thread the *binding's own* pool uses, so the body still lands on the worker.

**Resolution**: the harness is the problem, so remove the harness — declare the target `harness = false` in `Cargo.toml` and cargo runs the test file's own `main()` on the process main thread instead of wrapping it in libtest. It stays an ordinary `cargo test` target, executed by the same command on both platforms; no test moves out of `tests/`, and nothing has to be rebuilt. The icon-resolution check was converted this way, sharing the module under test via `#[path]` (not `include!` — a macro expansion may not introduce the inner doc comments the module starts with) so there is exactly one source of truth for the data being checked. The existing `#[gtk::test]` suite stays as-is and green on Linux; the write-up frames it as a platform capability gap, not a broken suite.

(First attempt put this in `examples/`, which also owns `main()` and does work — but an example is *documentation of how to use a crate*, and this is a test. Worse, `cargo test` only ever **compiles** an example, never runs it, so the check silently stopped being a gate on either platform. `harness = false` gets the same main thread while remaining a test that actually executes — ScrAP-124's lesson, that a suite outside the gate protects nothing, applied to a target type rather than a feature flag.)

**Scribobulate**: `tests/icon_resolution.rs` + its `[[test]] harness = false` declaration in `Cargo.toml` (the main-thread gate, plus the `#[path]` sharing trick); `src/icons.rs`, whose own `#[gtk::test]` was retired as redundant once one target covered both platforms.

**Lesson**: a test harness that promises "runs your GTK code on a consistent thread" is making a portability claim it may not hold on every OS — and when the platform's own requirement is *which* thread, a helper that owns thread creation leaves no seam for the caller to fix. Knobs that look like they control threading (`--test-threads`) govern the outer runner, not a library's internal pool, so verify which layer actually creates the thread before spending attempts on flags. When a harness is structurally unavailable, the escape hatch is any target that owns `main()` — an example or a small binary — rather than reimplementing the assertions or declaring the area untestable.

## 172. A synthesized-click UI-automation tool can be silently broken, making a real bug look unfixable across several attempts
> *Non-core (testing/automation process, not GTK) — do NOT fold into the gtk4-rs skill. Transferable to any agent verifying a GUI fix through OS-level input-injection tooling (AppleScript/`osascript`, `xdotool`, `AutoHotkey`, etc.) rather than a human hand on the mouse.*

**Symptom**: reproducing and then fixing a "clicking this button does nothing" bug, using an OS-level UI-automation tool (the one to hand — `xdotool`, `osascript`/System Events, AutoHotkey are all the same shape) to both reproduce the click and verify each fix attempt, since no human was driving the mouse in that session. Two different, independently-reasoned code changes were applied in turn; both were reported as "no change — button still doesn't respond" by the automation. The bug read as unusually stubborn.

**Root cause**: the automation tool's synthesized clicks were themselves unreliable on this OS/session combination, independent of the popover code under test. This surfaced only when an unrelated, definitely-functional control (a plain panel-close button, no popover involved) was clicked with the *same* tool as a sanity check — and also failed to register, on a freshly-launched instance that had never touched the code under investigation. A human then clicked the same unrelated control with a real mouse and it worked immediately. Every prior "no change" result was ambiguous the whole time between "the fix didn't work" and "the test's own input never arrived" — and there was no way to tell them apart without a control.

**Resolution**: stopped trusting the automation tool's negative results outright. Kept using it for the parts it *could* prove (opening the popover via a keyboard shortcut, activating a focused button with Space — keyboard-driven paths that don't depend on synthesized pointer-event delivery), added `RUST_LOG=debug` correlation logging inside the app itself to observe what actually happened on each attempt, and routed anything that specifically needed a real pointer click through the human operator instead of the automation.

**Lesson**: when a fix is verified through synthetic UI input rather than a human hand, a "still broken" result is ambiguous between *the fix is wrong* and *the test's own input delivery is broken* — and the second is invisible from inside the test, because it produces the exact same observed symptom as the first. Before trusting a negative result from click/keystroke automation (and especially before iterating on a second or third fix attempt on the strength of one), run a **positive control**: drive an unrelated control with an unambiguous, easily observed effect through the same tool, in the same session, and confirm it actually fires. If the control fails too, the automation is the thing that's broken, not the code. This check is cheap, and skipping it because "it worked in an earlier session" is exactly how the ambiguity goes unnoticed.

## 173. Freezing a drag icon with `current_image()` AFTER dimming the source widget captures a blank — `queue_draw` has already cleared the render node

**Symptom**: a tab drag works correctly end to end — the drop lands, the tab rehomes, no warning is logged, nothing errors — but **no drag icon follows the pointer**. The only sign a drag is in flight is the source widget's own dimming, which is easy to miss. Because the feature "works", the defect reads as a platform/backend deficiency: it was first filed as a suspected macOS/Quartz gap, on the reasoning that GTK's *default* drag-icon synthesis might simply be unimplemented there.

**What was tried / assumed**: the standing advice for a missing drag icon is *"don't rely on the platform default — set an explicit `GtkDragSource` icon."* That advice was already satisfied here: the code called `gtk::DragIcon::set_from_paintable` with an explicit, deliberately *frozen* paintable, and carried a comment showing the author knew a **live** `GtkWidgetPaintable` would blank once the handle was dimmed. The freeze was adopted as the cure. A second wrong theory followed: that `current_image()` *bakes in* `user_alpha`, so the icon was rendering at 40% and merely looked absent.

**Root cause**: the freeze was taken **one line too late**. The sequence was `set_dragging(true)` → `current_image()`. `set_opacity` issues a `queue_draw`, and `queue_draw` **clears the widget's cached `render_node` walking to the root** (`gtkwidget.c:3541-3552` — the same mechanism that produces this project's intermittent split-pane first-render blank). A `current_image()` in that same main-loop turn therefore finds no node and returns an **empty** paintable. Freezing does not rescue a late capture — it faithfully freezes the blank.

**Scope: platform-neutral, established by A/B on both backends** — *not* by deduction. An earlier draft of this entry argued "an empty paintable renders nothing on *any* backend, therefore the bug can't be platform-specific." That step was never observed, only inferred, and it is stated here as history because it nearly took the whole investigation down a wrong branch (see the closing lesson). What is actually measured is narrower and sufficient: *(i)* the paintable has **no render node at capture time** — both GTK versions, both backends; and *(ii)* a pre-fix/post-fix drag in the same session goes from **no icon** to **a correctly framed icon** — observed on 4.22.4/Quartz and on 4.6.9/X11 independently. What a backend does with an empty paintable *after* it is handed over remains deliberately unclaimed.

**Measured**, minimal GTK 4.22.4 probe — one label with a flat opaque `#c02040` background, snapshotted through the window's own renderer, centre pixel downloaded as premultiplied BGRA:

| condition | opacity | centre BGRA |
|---|---|---|
| untouched, node warm | 1.0 | `(64, 32, 192, 255)` |
| **dim, then capture same turn** (the bug) | 0.4 | **nothing drawn — `to_node()` returned `None`** |
| dim, then capture after frames land | 0.4 | `(26, 13, 77, 102)` |
| **capture, then dim** (the fix) | 1.0 | `(64, 32, 192, 255)` |

Row 3 is what retires the alpha theory and isolates the real cause: a dimmed widget snapshots *perfectly well* once a frame has landed (α=102=0.4×255, colour 77=0.4×192 premultiplied). So opacity was never the blocker — the **just-issued `queue_draw`** was. Row 2 is a total blank, not a faint image.

**Cross-confirmed on GTK 4.6.9 / X11** (independently rebuilt from the description, not run from the same code): all four rows reproduce identically. The behaviour is unchanged from the oldest supported GTK to the newest, on both backends, so this entry needs **no version caveat** — unlike its neighbour #170.

**Confirmed at the REAL call site, not just in the reduction** (GTK 4.22.4/Quartz, temporary `drag_begin` instrumentation, five consecutive drags across three tabs — unanimous):

```
capture-BEFORE-dim: handle=0x95861d7e0 opacity=1.00 intrinsic=119x44 render_node=PRESENT
capture-AFTER-dim:  handle=0x95861d7e0 opacity=0.40 intrinsic=119x44 render_node=ABSENT (empty paintable)
```

This is the step the whole investigation kept skipping. It pins three things a reduced probe cannot: the handle pointer is **printed and identical** across the pair (so the snapshotted widget provably *is* the dimmed one — a rival theory that it wasn't survived two rounds of argument until this ran); the opacity transition confirms the dim genuinely landed *between* the captures; and `intrinsic` is **byte-identical** across a `PRESENT`→`ABSENT` transition, which is the size-check trap below demonstrated on live application data rather than a synthetic case.

**⚠ The obvious regression test does not work.** On both versions the empty paintable still reports a full, plausible `intrinsic_width`/`intrinsic_height` (e.g. 300×200) while having nothing to draw — which is precisely why `DragIcon::set_from_paintable` accepts it without complaint and why no null- or size-check anywhere in the stack fires. A guard asserting "the icon has non-zero dimensions" therefore **passes on the broken code**. The only assertion that discriminates is whether the paintable actually produces a render node: snapshot it and require `Snapshot::to_node()` to return `Some`.

**Resolution**: capture the paintable **before** the state change. Rather than leave two ordered calls plus a warning comment — the exact shape that had already failed once — the capture and the dim were **fused into a single method** (`TabBar::begin_drag_visuals`) that returns the frozen icon and dims internally, so the call site cannot express the wrong order. That the fuse was right was confirmed structurally: it left the previously-public `TabView::handle_widget` accessor with no callers at all, and the compiler said so.

**Scribobulate**: `src/widgets/tab/bar.rs` (`begin_drag_visuals`, the fused capture-then-dim, plus the `drag_icon_freeze_must_be_taken_before_the_handle_is_dimmed` regression guard — which asserts the correct order yields a render node *and*, as a deliberate mutation, that the wrong order yields none, so a refactor cannot make it vacuous); `src/widgets/tab/view.rs` (the delegating wrapper); `src/window/tabs/dnd.rs` (`connect_drag_begin`, now one call). Contract: **TDD 7.9a** and its MANUAL-TEST counterpart — added because 7.9 covered only the drop-target highlight, so nothing asserted the drag icon itself and the bug had no rubric to fail.

**⚠ "Something follows the pointer" does NOT mean the drag icon works — verify FRAMING, not PRESENCE.** This wasted a full investigation cycle and produced a confident, wrong retraction. When the paintable is empty, GDK still creates a **real** drag surface: correctly sized (the empty paintable reports full intrinsic dimensions), override-redirect, and **tracking the pointer**. It is simply never painted, so it shows stale framebuffer/backing-store content. At normal viewing distance that is indistinguishable from a working drag icon, and it survives the obvious sanity checks — a capture of the surface at two different screen positions can even come back **byte-identical**, which looks like proof it has its own content but is equally explained by backing store. The discriminator is whether the thing under the pointer is a **clean, correctly framed** copy of the source widget, or a **misaligned slice** with boundaries in the wrong place. Judge by pre-fix/post-fix **differential** in one session, never by interpreting a single capture in isolation.

**A comment naming a hazard is not evidence the code avoids it.** The call site documented this exact trap — *"a LIVE `WidgetPaintable` would blank once the handle is dimmed above, so snapshot it first"* — and then performed the snapshot after the dim. The author had the mechanism right and the ordering wrong, so the stated mitigation captured the blank instead of preventing it. This is arguably worse than no comment at all: it signals the hazard was considered, which discourages the next reader from checking whether it was actually handled. Treat a hazard comment as a claim to verify, not a guarantee to trust.

**The structural lesson — the contract had a hole exactly the shape of the bug.** This project had **six** manual drag checks (MANUAL-TEST 7.6–7.10, 15.6) and every one asserted an **outcome**: the tab moves, a window opens, the order changes. Not one asserted **feedback**. So the drag icon was never contracted anywhere, and nothing *could* fail when it disappeared — the regression was invisible to the test suite by construction, not by oversight. "Nobody looked" is the lazy diagnosis; "there was no assertion that could have looked" is the useful one. This generalises to **any purely visual affordance**: hover highlights, focus rings, drag icons, busy cursors, transient badges. If the only assertions are about what an operation *achieves*, every piece of feedback along the way is unguarded, and a defect there survives indefinitely because the feature keeps working. When adding a rubric for such an affordance, place it beside the behaviour it accompanies (here TDD **7.9a** next to 7.9's drop-target highlight) rather than appended at the end of the section, so the feedback cluster stays legible.

**Process lesson — a measured mechanism is still only a prediction about the application.** Five predictions were made across this investigation by two independent agents, and *the only one that survived was the one tested against the real call site*. The reduced probe was correct and reproduced on two GTK versions; the extrapolation from probe to app was what kept failing, in **both** directions — first "the icon must be broken everywhere" (right, but unproven at the time), then "X11 shows an icon, so the app isn't the reduced case" (wrong, from an unpainted surface). A clean measurement makes a deduction *feel* observational, which is more dangerous than an ordinary guess because it arrives wearing evidence. Note also that a positive control validates only what it exercises: confirming the *drag* ran end-to-end (handle dims, tabs reorder) says nothing about whether the *capture* means what you think — "did my input arrive?" and "does my measurement mean what I think?" are different questions. Reach for a **differential** (same session, same coordinates, fix toggled) before interpreting any absolute reading.

**Lesson**: any GTK API that hands you a widget's *current pixels* — `current_image()`, and anything built on the cached render node — reads state that a `queue_draw` in the same turn has already invalidated, and it reports that invalidation as **empty output rather than an error**. Treat "snapshot the widget" and "change how the widget looks" as an ordered pair with exactly one correct order, and encode the order in an API rather than a comment. More generally: when a feature *works* but its *feedback* is missing, suspect your own ordering before suspecting the backend — a silent blank is far more often a cleared cache than an unimplemented platform path, and "set an explicit icon" advice is worthless if the explicit icon you set is itself empty. Verify by *measuring the artifact you hand to the toolkit* (download its pixels), not by inferring from what appears on screen; that measurement is cheap, is possible without reproducing the drag at all, and distinguishes "my paintable is blank" from "the backend won't draw it" — which no amount of staring at the screen can.

## 174. A single-instance guarantee that lives in a backend, not an API, fails silently where the backend is absent — and the platform's *other* launch path will falsely confirm it works

**Symptom**: on macOS every launch of the app spawned an independent process with its own window — two live-reload monitors on one file, two windows that could each save over the other — while the identical code single-instanced correctly on Linux. Nothing errored. `GApplication` was built with `HANDLES_OPEN`, `g_application_register()` succeeded, and `is_remote()` reported false, which is *exactly* what a legitimate first launch looks like.

**What was assumed**: the Windows port had already crossed this ground and recorded that single-instance "is confirmed cross-platform… very unlikely to need app-side code changes," reasoning that GIO abstracts the IPC backend per platform — D-Bus on Linux, a named mutex on Windows. The inference was that macOS must likewise have *some* backend. It does not: GIO implements `GApplication` uniqueness over a **D-Bus session bus**, macOS runs none by default (no `dbus-daemon`, no `DBUS_SESSION_BUS_ADDRESS`, the binaries not even installed), and there is no macOS-specific substitute to switch on. The API is fully present and fully inert.

**Root cause of the *silence*, which is the transferable part**: the failure mode of "no peer found" and the failure mode of "no mechanism to find a peer with" are the same value. A registration that cannot see other instances reports the same success as a registration that correctly sees none, so there is no return code to check and no warning to log. Two processes is the *first* observable symptom, and it only appears if someone launches twice on that platform and thinks to count PIDs.

**Measured — two live processes, both convinced they are the only one** (minimal `gio` probe, GLib via Homebrew, one process started 1 s after the other and both still running):

```
pid=91810 register=Ok is_remote=false is_registered=true dbus_conn=None bus_env=None
pid=91813 register=Ok is_remote=false is_registered=true dbus_conn=None bus_env=None
```

**But there IS one observable that discriminates, and it is not the one you would reach for**: `is_remote()` and the `register()` return are useless here — identical in both processes and identical to a healthy first launch — while **`g_application_get_dbus_connection()` returns NULL**. A `GApplication` that registered successfully and has no D-Bus connection is not negotiating uniqueness with anyone. That is a one-line self-check, on the *transport* rather than on the *outcome*, and it is the concrete form of this entry's lesson.

**⚠ The platform's second launch path will hand you a false confirmation.** macOS reuses a running **bundled** app through LaunchServices entirely outside the application's code: `open -a Scribobulate.app <file>` against a running instance opens the document in it and **execs no second process at all**. Measured by polling `pgrep -x scribobulate` every 20 ms across the whole operation — the pid set never changed, so the reuse cannot have been the app's own handoff. GTK supplies the other half: `libgtk-4.dylib` 4.22.4 ships `-[GtkApplicationQuartzDelegate application:openFiles:]` (confirmed by `nm`/`otool -oV` on the installed dylib), the `NSApplicationDelegate` callback LaunchServices invokes, which lands the document in the same `g_application_open` handler a D-Bus forward would. The result is indistinguishable from working single-instance activation. Testing via Finder / "Open With" / `open -a` — the *most natural* way to test a Mac app — therefore passes while the CLI path is completely broken, and would have closed this defect as unreproducible. Note this required **no** `LSMultipleInstancesProhibited` in `Info.plist`; the plist key the port plan had earmarked was never the reason it worked.

**Measured, after the fix** (GTK 4.22.4 / Quartz, release build): the second launch exits in **25 ms without initialising GTK at all**, and `lsof` on the survivor shows both documents held by the one process — a descriptor count is a good proxy for "which tabs exist" when you cannot drive the UI. Five simultaneous cold launches elect exactly one primary and four forwarders. A `kill -9`'d primary leaves its socket behind and the next launch takes it over silently.

**Resolution**: substitute the transport, not the behaviour. `src/platform/mac/single_instance.rs` elects a primary with an `flock`ed lock file and carries a second launch's arguments over a `$TMPDIR` Unix domain socket, converting them with the same `g_file_new_for_commandline_arg` GIO's own D-Bus forwarding uses, then emits `open`/`activate` on the existing `GApplication`. Everything downstream of that emission is shared with Linux, so the two platforms cannot drift in behaviour — only in transport. Three properties are worth copying: the lock is released by the **kernel** on any exit, so a crash cannot wedge the next launch; the rendezvous path is read from `std::env`, never a GLib user-dir helper, so it cannot move underneath a process whose environment was rewritten (`workaround.rs` redirects `XDG_CONFIG_HOME`); and a handoff that cannot complete falls back to launching independently, because failing to open the user's file is worse than opening it twice.

**Scribobulate**: `src/platform/mac/single_instance.rs` (whole module, `#[cfg]`-gated at its declaration in `platform/mod.rs` per the `src/platform/win32/` precedent); `scribobulate::run` in `src/lib.rs` (`elect` between `setup_app` and `run_with_args`, skipped under `--new-instance`; it sat in `src/main.rs` until the lib/bin split reduced that file to argument-free delegation). Contract: **TDD 8.1/8.2/8.5** unchanged — the point is that the *behaviour* did not need a platform clause — plus **TDD 8.7** for the crash-recovery property the new primitive introduces, and MANUAL-TEST **8.2m**, which forces both macOS launch paths to be exercised separately.

**Lesson**: when a framework says a capability is cross-platform, find out **where the portability physically lives**. If it lives in a backend selected at build or run time, the capability's presence on your platform is a question about that backend's existence, and the API will not answer it — a call into a missing backend usually returns the same "nothing to do" as a call into a working one. Prove the backend (is there a session bus? is the daemon installed? is the env var set? does the object expose a live connection?) before trusting a compile and a clean return — and prefer an assertion on the *transport* over one on the *outcome*, because the outcome is what the missing backend is busy faking. The second half generalises past GIO: **a platform that can launch your app more than one way has more than one contract**, and the paths do not share a mechanism — Finder/LaunchServices, `open -a`, a CLI invocation and a `cargo run` are four different negotiations on macOS, and the most convenient one to test was, here, the only one that already worked. When a defect is reported on one launch path and reproduces on none of the others, that is evidence about the *paths*, not about the defect.

## 175. A defect whose CONSEQUENCE is platform-dependent while the defect itself is not — the platform that never triggers it never tests for it, and a guard written on the triggering platform's symptom is permanently green where the bug actually lives
> *Non-core (testing methodology, not GTK) — do NOT fold into the gtk4-rs skill. Transferable to any project with more than one target platform. The GTK-specific trap that produced it (`gtk_text_view_remove` silently refusing a child it has no bookkeeping for, so `gtk_text_view_dispose`'s drain loop spins forever) is a separate, core-GTK lesson relayed to the `gtk4-rs` skill by the Linux side.*

**Symptom**: a tab drag-and-drop integration test hung forever on Windows and on macOS — 28 MB of identical `Gtk-WARNING` in 30 s on GDK-Win32, 12.1 GB / 274 million lines in five minutes on Quartz — and **passed in 0.50 s on Linux**. Two 4.22.4 backends wedged and 4.6.9 did not, so the whole investigation was framed as "what does GTK 4.22 do that 4.6 doesn't", across three seats, for hours. It was tracked as a Windows issue and a macOS issue. The Linux suite was 624 tests green and was twice cited — by the Linux side itself — as cross-platform reassurance.

**Root cause**: the defect was **shared application code on master, live on all three platforms**. `move_tab_to_new_window` handed a tab's editor to another window without detaching the source window's `set_parent`-attached popover from it. The stale parenting was then *measured* on GTK 4.6.9 and found to be **present there too** — the Linux side instrumented the move and got `overlay parented on the MOVED editor? true`, before and after. Linux never paid for it only because it never subsequently *disposes* that editor, so it never enters the loop that the stale child makes infinite. **The bug was platform-independent; only its consequence was platform-dependent.** Every hypothesis about a 4.6-vs-4.22 GTK difference was answering a question that did not apply — two such theories were raised and later falsified by measurement (an "incidental rescue" by a subsequent reparent; a `gtk_window_destroy` signal-timing difference, which turned out byte-identical across both versions).

**Resolution**: fix the shared code (one missing detach call), and — the load-bearing half — **write the regression guard against the STATE that constitutes the defect, not the SYMPTOM the triggering platform produces**. The guard asserts that the source window's overlay is *not parented* on the moved editor after the move. It is deliberately **not** a flood check, a hang check, or a timeout: every one of those is unobservable on Linux, so a guard shaped like the symptom would be permanently green on the platform where the defect actually lives and where most development happens. Because it asserts the state, **it fails on Linux today** — converting a structurally invisible defect into one that platform's own suite catches. Mutation-tested in both directions before being trusted (fails with the fix removed, passes with it restored), and the reasoning is written into the test's own doc comment so the next reader does not "improve" a parenting assertion back into a timeout check.

**Scribobulate**: `src/window/tabs/dnd.rs` — the `detach_overlay_from` call in `move_tab_to_new_window`, and `move_tab_to_new_window_detaches_the_source_windows_format_overlay` (the guard, with the "green for the wrong reason" argument in its doc comment).

**Lesson**: when a failure reproduces on some platforms and not others, **"platform-specific failure" and "platform-specific defect" are different claims, and the second does not follow from the first.** Before accepting the platform framing, measure the suspected bad *state* on the passing platform — not the symptom, the state. Here that was one instrumented boolean, and it moved the bug from "two platform ports each carrying an issue" to "one shared defect the third platform structurally cannot see."

Two corollaries worth more than the headline:

- **Scope of proof: a green suite bounds what was tested, never what is true.** "624 tests green on Linux" was accurate and uninformative — that suite never disposes the widget in question, so it cannot observe *any* dispose-time defect: leaked children, un-unparented popovers, dispose ordering. The class was structurally invisible, not merely uncovered. **The question is never "did the test pass", it is "could this test have failed"** — and if the answer is no, its passing is not evidence.
- **Guard shape follows the defect, not the discovery.** You will nearly always find the bug on the platform that hurts, and the symptom there is vivid and easy to assert on. Assert the invariant instead. A guard that can only fail where the bug already announces itself adds nothing; a guard that fails where the bug is silent is the one that pays.
## 180. A `set_parent`'d child left on a `GtkTextView` at dispose is an INFINITE loop, not a warning — and the suite that stayed green was the one never disposing anything

**Symptom**: one test body in the GTK suite never returned. Not slow — unbounded: **274,758,040** lines of `Gtk-WARNING **: GtkPopover is not a child of GtkSourceView` and **12.1 GB** of stderr in about five minutes, killed by hand before it filled the disk. A `sample` of the process put 100% of samples in one stack, all inside teardown:

```
Rc<TabState> drop → WindowChrome → TabView → gtk_stack_dispose → stack_remove
  → gtk_box_dispose → SplitView finalize → Overlay → gtk_scrolled_window_dispose
  → gtk_text_view_dispose → gtk_text_view_remove → g_log_structured_standard
```

**Root cause** (`gtk/gtktextview.c`; line numbers are 4.6.9, read by the Linux counterpart, and the code is byte-identical on 4.22.4). Dispose drains its children with a loop whose termination depends entirely on the callee actually removing one:

```c
  while ((child = gtk_widget_get_first_child (GTK_WIDGET (text_view))))   /* :3942 */
    gtk_text_view_remove (text_view, child);
```

and the callee refuses any child it does not recognise:

```c
  ac = g_object_get_qdata (G_OBJECT (child), quark_text_view_child);      /* :5968 */
  if (ac == NULL)
    {
      g_warning ("%s is not a child of %s", …);
      return;                       /* :5975 — returns WITHOUT unparenting */
    }
```

Only children attached through an anchor or a gutter slot carry `quark_text_view_child`. **Anything attached with a plain `gtk_widget_set_parent()` — every `set_parent`'d popover — has no qdata, is never removed, and is handed back by `get_first_child` forever.** ScrAP-90/GTK4Rs/AP-80 already said "unparent your `set_parent`'d popovers or you get a flood of warnings at teardown". The flood is not the failure. The failure is that there is no end to it.

**Two hypotheses killed, both worth recording because both were reasonable.** *(a) a 4.6.9→4.22.4 regression in the drain loop* — falsified by reading 4.6.9, where the loop is verbatim. *(b) libtest's output capture merely hid it on Linux* — falsified by measurement: GTK warnings are emitted by C straight to the process fd and bypass libtest's Rust-level capture, they appear without `--nocapture`, and `G_DEBUG=fatal-warnings` does abort the same test on a real warning, so a zero-warning Linux run is a real zero and not a blind one.

**And it is not GTK's own child.** Minimal C on 4.22.4: a `GtkTextView` and a `GtkSourceView`, each bare and each presented in a mapped window, pumped, with the whole buffer selected — the state that makes GTK build its `selection_bubble` — enumerated **zero children** at dispose and disposed with **zero warnings**. So a stuck child is *yours*, and that enumeration is the fastest way to settle it either way.

*(⚠ An earlier revision of this entry explained that measurement by asserting 4.22.4 had dropped the pre-unparents 4.6.9 does. **That was invented and is false** — the Windows port read the 4.22.4 tree: `gtktextview.c:4165` still clears `selection_bubble` and `magnifier_popover` immediately above the drain loop, identical to 4.6.9. The measurement and its conclusion stand; the mechanism attached to them did not. Recorded because the wrong version story survived one hand-off before anyone with both trees checked it.)*

**A timing fact that is NOT this bug's cause but is a standing gtk-rs hazard, so it is recorded here rather than lost — measured in C, and identically on 4.6.9 and 4.22.4:**

```
== caller holds a ref = 0 ==            == caller holds a ref = 1 ==
  calling gtk_window_destroy()...         calling gtk_window_destroy()...
      <<< destroy handler fired           gtk_window_destroy() returned
  gtk_window_destroy() returned           dropping the caller's ref...
                                              <<< destroy handler fired
```

Identical mapped and unmapped, and — measured by the Linux counterpart on the same probe — identical on 4.6.9. `destroy` is emitted from *dispose*, i.e. at refcount zero, so **`gtk_window_destroy()` only looks synchronous when the caller holds no reference of its own.** In gtk-rs a live `ApplicationWindow` value *is* such a reference, so `window.destroy()` with the binding still in scope defers every `connect_destroy` handler to whenever that binding drops, and Rust's reverse-declaration drop order then decides an ordering nobody wrote down. **This was offered, here, as the explanation for why one platform wedged and another did not. It is not** — it does the same thing on both. It is kept because it silently governs every teardown hook in every gtk-rs app, and because "measured on the platform that fails" is not the same as "differs on the platform that fails", which is the mistake that produced the claim.

**Root cause, established with the Linux and Windows counterparts**: application-side and **version-independent**. A window-scoped popover is `set_parent`'d onto whichever editor is active and detached before that editor is torn down — but one of the three paths that tears an editor down never calls the detach. The stale parent was then measured surviving the same operation on 4.6.9, so all three platforms carry the defect.

**The real difference between the platforms is the one worth carrying away.** On 4.6.9 the same test passes **not because the teardown is correct but because it never happens**: the leftover-children probe there ends with the moved editor still alive and still `parent=Some("GtkScrolledWindow")` after both windows have been `destroy()`ed. `gtk_text_view_dispose` never runs, so the drain loop never executes, so the stale children cost nothing. **That suite does not exercise widget disposal at all** — leaked children, un-unparented popovers and dispose-ordering bugs are structurally invisible to it, and its 624 green tests were true and uninformative about the entire class. The platform that *wedged* is the strict one.

**Fixed — and the first fix was on the wrong path, which is the sharpest part of this entry.** The detach was added to the menu/accelerator entry point. That path is *disabled for a window's only tab*, so a tab moved through it always leaves a sibling behind, that sibling activates, and the re-target pulls the overlay off the doomed editor **as a side effect** — the stale parent never forms there. The reachable path is the drag-out pop-out, which *can* take a window's only tab and therefore has no sibling to trigger the rescue, and it reaches the shared tail by a different route that the guard did not cover. So the guarantee now lives in the **shared tail**, resolved from the content box rather than passed in, which is what makes a future third caller unable to forget it.

**How that was caught is the reusable part**: a check written against the menu path passed with the detach commented out. It was mutation-tested, found vacuous, and reported as vacuous instead of being shipped — and that report is what exposed the fix being on the wrong path. A guard placed on a self-healing path is worse than no guard: it is green, it looks like coverage, and the defect it names is untouched.

Recorded here for the mechanism, which outlives the fix — the entry is deliberately self-contained rather than pointing at a debt-register entry that exists in order to be deleted (ScrAP-143).

**Scribobulate**: `src/saferizer/persistent_popover.rs` (`teardown` = popdown → unparent, the correct shape) and its callers in `src/window/mod.rs`, `src/window/tabs/switch.rs`. **No runner-side guard exists in this branch**: the wall-clock watchdog and log-volume cap that bounded this were part of a GTK test harness that was built, verified, and then reverted on proportionality grounds — so the numbers above are the only record of what an unbounded case costs, which is why they are quoted here rather than cited.

**Lesson**: four, and they are independent. **(0)** *A suite in which widgets are never disposed reports green on every dispose-time bug there is.* Nothing about that is visible from the suite — it passes, quickly, with no warnings — and the only way it surfaced was a second platform where the trees actually came down. So before treating a green integration suite as evidence about teardown, **prove that teardown happens**: enumerate a widget's children, or its parent, after the point the test believes it destroyed it. **(1)** A drain loop that delegates removal to a function which can decline is only bounded if the caller re-checks — so in any toolkit, "leaving a child attached" is a *liveness* risk, not a tidiness one, and a warning that repeats is a hang wearing a warning's clothes. Read the loop, not just the message. **(2)** `destroy`-signal teardown is refcount teardown; a language binding that hands you an owning handle silently changes *when* your handler runs, and no API in the toolkit tells you so. If order matters, drop the handle explicitly or do the teardown at a point you control, and verify with a print rather than by reading the docs — the docs describe the C caller, who owns nothing. **(3)** Any harness that runs foreign UI code needs a **wall-clock cap and an output-volume cap**, because this class of bug does not fail, it consumes — and the standard harness's instincts (hang quietly; buffer output in memory) turn a bounded disaster into an unbounded one.

## 181. A suite that has never RUN on a platform is full of assertions that only look portable

**Symptom**: the first execution of a GTK integration suite on a platform it had never run on (macOS, where the binding's own test attribute dispatches bodies onto a worker thread that GTK there forbids) failed 7 of 134 bodies. Six were nothing to do with macOS *behaviour* — they were assertions that had quietly encoded the Linux environment, and had been green for as long as they had only ever been run there. Three reported "the document never opened", which points at the feature under test and not at the fixture.

**Instance 1 — the temp dir is not the path you get back.** `tempfile::tempdir()` on macOS returns `/var/folders/…`, and `/var` is a symlink to `/private/var`. The app canonicalises a path before recording it, so the tab's recorded path was the `/private` spelling and every `tab.path == dir.path().join("TARGET.md")` comparison missed. Two spellings of one file, no error anywhere, and a failure message that blamed the navigation.

*Fix*: canonicalise the directory **once, at creation**, and build every expected path from that — not at each comparison, where the next test to be written will forget. Deliberately **not** `#[cfg(target_os = "macos")]`-gated: it is a no-op wherever nothing is symlinked, and a comparison that only holds on one platform is the defect rather than the platform being unusual.

**Instance 2 — the toolkit renders a modifier differently per backend.** `gtk_accelerator_get_label` spells `<Primary>` as `Ctrl+` under X11/Wayland and as the `⌃` glyph under Quartz. Three assertions had pinned the literal `"Ctrl"`. The production code was already correct and its comment already said "platform-correct (Linux `Ctrl+C`, macOS `⌘C`)" — *and that comment was wrong about which glyph*: GTK4 dropped GTK3's quartz `<Primary>`→Command mapping, so it is `⌃` and never `⌘`.

*Fix*: one `#[cfg]`'d constant per platform, asserted against. Explicitly **not** derived by calling the same function under test, which would make the test agree with itself and assert nothing.

**Root cause of the class**: a green suite is evidence about the environments it has run in and about nothing else. Neither of these is a bug in the code under test, neither would ever fire on the platform they were written on, and neither is visible in review — `dir.path().join(…)` and `contains("Ctrl")` both read as obviously correct.

**Scribobulate**: neither fix lives in this branch. Both are platform-neutral — a `canonical_tempdir` helper for the temp-dir case, a `#[cfg]`'d `PRIMARY_LABEL` constant for the accelerator one — and were conveyed to the shared branch rather than carried here, per the rule that a platform branch holds no platform-neutral change. The full form of each is in this entry deliberately, so the lesson does not depend on that handover having happened.

**Lesson**: when a suite runs on a new platform for the first time, **budget for a batch of failures that are about the tests rather than the port**, and triage them apart before touching the app — six of the seven here were in that class, and chasing the first one as a navigation bug would have been a wasted day. Two smells identify them cheaply: an assertion that pins a *string the platform renders* (a modifier label, a path separator, a locale-formatted number, a font name), and any comparison between a path **you built** and a path **the code resolved**, because resolution is where a platform's symlinks, case-folding and short names enter. Fix both at the point the value is *created* — canonicalise once, define the expected spelling once — never at each point it is compared, which is a per-call-site workaround the passing tests cannot catch (the ScrAP-116 shape).

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

## 183. A mutation that fails on an earlier precondition proves nothing about the guard under test

> *Non-core (test methodology, not GTK) — do NOT fold into the gtk4-rs skill. Transferable to any suite whose tests have preconditions.*

**Symptom**: a guard was strengthened, and mutation-testing was used to prove it now discriminates — the discipline this project already applies (ScrAP-78). The mutation was applied, the test went **red**, and that was taken as proof. It was not. The mutation had broken an *earlier* `assert!` in the same test — a precondition establishing the state the real assertion needed — so the assertion under test never ran at all. The run's red/green signal is identical either way.

**What was tried**: verifying `a_navigation_leaves_the_document_scrolled_to_the_target` after its fixture was made non-degenerate. Two mutations of the scroll logic were tried and *both* were misleading:

1. **Disable the scroll entirely** → failed at `"precondition: the navigation completed"`. The target never becomes visible, so the popover never opens, so the test dies before reaching the scroll assertion.
2. **Aim 100 px PAST the target** → failed at the same precondition, for the same reason: overshooting puts the chip *above* the viewport, equally invisible.

Only the third worked: **aim 200 px SHORT**, which keeps the target on screen so the navigation completes and the popover opens, isolating the landing. That one failed on the intended assertion — `value=304 want=504`.

**Root cause**: a test is a *sequence* of assertions, and a mutation perturbs the system, not one assertion. Anything upstream of the assertion you are probing can absorb the perturbation and fail first. The louder the precondition (and good preconditions are loud), the more likely it catches the mutation before the assertion does. So the very quality that makes a test diagnosable makes its mutation test easy to misread.

**Resolution**: **read the failure, not the exit code.** A mutation test is only evidence when the *named assertion under test* is the one that fired. Concretely:

- Run the mutated test with the panic message visible and confirm the **file:line and message** match the assertion you set out to probe.
- Prefer a mutation that leaves every precondition satisfiable — perturb the measured *quantity*, not the *mechanism*. "Land in the wrong place" is a good mutation; "don't act at all" usually is not, because preconditions frequently assert that something happened.
- If every available mutation trips a precondition, that is itself a finding: the assertion may be **redundant** with its own preconditions, and worth deleting or restating rather than defending.

**Lesson**: this is the vacuous-guard family — a check whose observable does not vary across the behaviours it claims to discriminate — one level up — applied to the *verification* rather than the code. A guard that cannot fail tells you nothing; a mutation test that fails for the wrong reason tells you nothing *while looking exactly like evidence*, which is worse, because it converts an unchecked assumption into a recorded one. Ask of a red run the same question asked of a green one: **what would have to be true for this result to mean what I think it means?**

## 184. Four green checks, none of them the outcome — the plumbing was verified and the user-visible result was not

**Symptom**: a feature shipped with four passing checks that each independently confirmed a link in its chain — the value mapping's polarity, the live OS read cross-checked against an independent tool, the toolkit setting moving the resolved palette, and an end-to-end assertion that the module's entry point left the toolkit agreeing with the OS. Every one passed, including under mutation. The feature did not work: the window still rendered in the old appearance, which was the entire user-visible point.

**What was tried**:
- Mutation-tested the polarity mapping in both machine states, and fixed a real vacuity found that way (one branch short-circuited before the mapping, so an inverted mapping went undetected on a machine in the default state). This was worth doing and did not touch the actual failure.
- Read the resolved palette back through the toolkit's own colour lookup after setting the property, in both directions. It moved, correctly, every time — which was taken as proof the change had landed.
- Only a screenshot showed it had not. The state was right, the derived colours were right, the re-render ran with the right inputs, and the pixels were unchanged.

**Root cause**: every check asserted an *intermediate* representation, and the chain had one more link than the checks covered. The toolkit's colour lookup reporting a dark palette and the toolkit actually painting dark turned out to be separable on this backend — so the strongest available non-visual assertion was still upstream of the failure. Nothing was wrong with any individual check; the set simply had a shape, and the shape stopped short of the thing being promised.

**Resolution**: add the missing terminal assertion and keep every one of the others. The suite now renders a real window through GSK and samples the painted pixel in both directions, which is the first check in the set that can fail when the feature is broken and the plumbing is not. Each remaining check also states its own scope in the terms it can actually defend — the palette assertion says it proves the *resolved palette* follows, and explicitly disclaims proving the render. Where a defect is visual, the artifact of record is pixels; note that a screenshot is not automatically that artifact, since screen capture can grab the wrong desktop and read exactly like the bug.

**The mechanical test, so this is not a matter of judgement**: each of those four checks was a **vacuous guard** — a check whose observable is *constant across the behaviours it claims to discriminate*. The colour lookup returns the dark value whether or not the window paints dark, so its value cannot tell those two states apart, and no amount of care in writing the assertion can rescue that. Apply the test to the observable, not to the assertion: ask what values this check could possibly return under the failure it is supposed to catch. If the answer is "the same one", the check is decoration. (Cf. ScrAP-183, a mutation that fails on an earlier precondition and so never reaches the guard under test — the same family, arrived at from the other end.)

**The counter-rule, which matters as much as the lesson**: this is *not* an argument against intermediate checks. Those four are exactly what localises a break once a terminal assertion exists and fails — they are the difference between "dark mode is broken somewhere" and "detection is fine, the paint step is not", which here was the entire diagnosis. The defect is a suite with **no** outcome assertion, never a suite that also has link assertions. Do not delete link checks on the strength of this entry; add the missing one.

**Lesson**: a chain of green checks is not evidence in proportion to its length. Each narrows *where* a failure can be, and a set of them can converge on high confidence while leaving the final link — usually the only one the user experiences — entirely unasserted; worse, that link is typically the hardest to assert, which is precisely why it gets approximated by a proxy upstream of it. So for any user-visible contract at least one check must assert the observable outcome, and every proxy check should say out loud what it does *not* cover. **When confidence comes from four checks that all read the same intermediate value in different ways, that is one check with four spellings, not four checks.**

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

**What was tried**: the working hypothesis — reached in minutes and entirely
plausible — was that the autohide seat grab had been *supplying* the behaviour.
The grab takes all seat capabilities, so the reasoning went: with the grab, a
scroll over the parent never reached the view, the document could not move while
a card was open, and therefore the card "stayed with" its text; remove the grab
and the document scrolls out from under it. That framing is coherent, fits every
observed fact, and implies a clear fix: restore the previous behaviour, or
reinstate autohide with a different parent so the grab comes back without the
misrouting.

Both implications were wrong, and neither would have been caught by testing —
they would have been caught only by *building the wrong thing and finding it
did not help*.

**Root cause**: the inference was never verified against the mechanism, only
against the timeline. Checked at the source (GTK 4.6.9): the autohide dismissal
path handles **button and touch input only — it never handled scroll**, and GTK
implements no anchor-tracking for view-parented popovers anywhere in the tree.
Its own view-anchored surfaces (the `GtkTextView` selection bubble, the
magnifier) are non-autohide, compute their anchor with the scroll offsets *at
show time*, and **hide** on interaction rather than following. The grab was also
not parent-dependent: it is installed from the autohide flag at popup-surface
creation, so re-parenting the card would not have restored it either.

So the behaviour was not lost — it never existed. What removing the grab
actually changed was narrower and duller: the document could now scroll *at all*
while a card was open, which exposed the absence of a feature nobody had written.
"It used to scroll with the document" almost certainly meant "it used not to be
possible to scroll".

**Resolution**: the entry that matters is the correction, not a patch. The
defect was re-specified as *missing behaviour to be designed* rather than
*regressed behaviour to be restored*, which changes the options: match GTK and
the app's own sibling card by dismissing on scroll, or implement tracking
deliberately. Restoring autohide was struck off entirely — it would have
reintroduced the click misrouting to buy back something it never provided.

**Evidence status, deliberately recorded**: the GTK facts above are
**source-traced and not yet verified by execution here** — the dependent fix has
not been built. That is exactly the caveat this entry is about, so it would be
incoherent to omit it: a source read is a prediction about behaviour, and the
correction of one unverified inference must not be filed as if it were a
measurement. Confirm at the fix.

**Lesson**: "it worked before change X" locates a regression in **time**, not in
**cause**, and the gap between those is where expensive wrong fixes come from.
The reasoning feels like a causal chain because a mechanism is named and a
plausible pathway is described — but the only evidence is adjacency, and a
removed mechanism is a magnet for attribution because it is the salient thing
that changed.

The failure mode is specific and worth recognising: **a removal can EXPOSE an
absence rather than CAUSE one.** When a mechanism is taken away, behaviour that
was previously unreachable becomes reachable, and any feature missing from that
newly-reachable path now presents as a fresh defect. The change is genuinely
responsible for the bug report and is genuinely not responsible for the missing
behaviour, which is why the intuition is so hard to shake.

Two checks that cost minutes and are worth running before designing any
restore-it fix:

- **Ask what the mechanism DID, from its source or docs, not from what stopped
  working when it left.** If the answer does not include the behaviour, the
  hypothesis is dead however well it fits the timeline.
- **Ask whether the toolkit implements the behaviour at all**, anywhere. If the
  framework has no such feature in its own comparable surfaces, then the code
  never had it either, and "restore" is not a description of any reachable
  state.

Corollary for reported symptoms: a user's account of *what changed* is reliable;
their account of *what used to happen* is a reconstruction, and where the two
readings differ ("it followed the document" vs "the document could not move"),
resolve it against the mechanism before it silently sets the specification.

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

## 192. `popdown()` is not animated, `closed` fires from `hide` — so your own transient hide trips your own "is it still open?" backstop

> *Core GTK4 (GtkPopover semantics on 4.6.9). Corrects a premise this project had carried in code comments since ScrAP-112.*

**Symptom**: a card that hides itself while its anchor is scrolled off the viewport — intending to come back when the anchor returns — never came back. Its session flag had been cleared by its own hide.

**What was tried**:
- Wrapping the "this hide is mine" flag tightly around the `set_visible(false)` call, on the assumption that the `closed` signal is emitted synchronously from inside it. This is the natural shape and it did not hold up.
- Reaching for the long-standing project belief that *"`popdown` is animated, so `is_visible()` lags during the close animation"*, which had justified keeping a separate explicit open flag. Source-checked against 4.6.9: **false**. `gtk_popover_popdown()` is `gtk_widget_hide()` plus a cascade that **early-returns for a non-autohide popover**; there is no transition, no tick callback, and `is_visible()` flips immediately. For a non-autohide popover `popdown()` and `set_visible(false)` are the same operation.

**Root cause**: `closed` is emitted from the **`hide` vfunc**, so *every* hide emits it — including a deliberate, temporary one. A `closed` handler used as a backstop for "the session ended" therefore fires on the widget's own transient hide and ends a session that was only being paused.

**Resolution**: keep the session flag distinct from visibility (still correct, for a better reason than the animation myth — a card can legitimately be *open but off screen*), and mark a self-initiated hide with a **sticky** flag: set when hiding, cleared when shown again or genuinely dismissed. That needs no assumption about when the signal arrives.

**Lesson**: a framework signal named for an *outcome* ("closed") is usually wired to a *mechanism* (the hide vfunc), and cannot distinguish your intent from the user's — so never infer intent from it; carry the intent explicitly, and make the marker outlive the call rather than bracket it, so the guard holds whenever the signal arrives. Separately: **audit inherited premises when you touch the code they justify.** "Popdown is animated" was load-bearing for real complexity here and was never true on this version; a belief that only ever appears in comments is one nobody re-tests.

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

## 197. A `#[path]`-included module's children resolve against the attribute's directory, not the module's own name
> *Non-core (Rust/Cargo module resolution — tooling, not GTK) — do NOT fold into the gtk4-rs skill. Surfaced while exploring where a second GTK-test crate root could live; see #198 for the sibling trap in the same investigation.*

**Symptom**: relocating a `harness = false` second crate root away from beside `lib.rs` (to `src/platform/mac/`, reached via `#[path = "../../gtk_suite_probe.rs"] mod gtk_suite_probe;`) failed to compile one of the modules it re-declared: `error[E0583]: file not found for module 'tests' … help: create "src/platform/mac/../../copymap.rs/tests.rs"` — `copymap.rs`'s own `mod tests;`, unrelated to the relocation, broke as a side effect of it.
**Root cause**: rustc resolves a `#[path]`-included module's *children* against the `#[path]` attribute's directory, without re-appending the module's own name — so `mod copymap;` pulled in via `#[path]` from `src/platform/mac/` looks for `copymap.rs`'s children at `src/platform/mac/`, not `src/platform/mac/copymap/`. A file named `mod.rs` is immune (its children resolve relative to its own directory by Rust's ordinary rule), but `copymap.rs` and any other non-`mod.rs` file with its own `mod x;` is not.
**Resolution**: don't relocate the second crate root away from the real one — place it directly beside `lib.rs`, in `src/` itself. From there `mod copymap;` resolves exactly as `lib.rs` resolves it (no `#[path]` involved at all), so the whole module tree compiles with zero special-casing. Confirmed by probe: a root placed at `src/gtk_suite_probe.rs` compiled the entire tree, `copymap.rs` included, with no `#[path]` anywhere.
**Lesson**: a second crate root's *placement* is not a cosmetic choice once anything in the tree has file-based child modules on a non-`mod.rs` file — check that constraint before reaching for `#[path]` to move a root anywhere other than beside the crate root it duplicates.

## 198. `pub use` cannot widen `pub(crate)` visibility — there is no test-façade shortcut around it
> *Non-core (Rust visibility rules — tooling, not GTK) — do NOT fold into the gtk4-rs skill. Sibling of #197 (same investigation: how does a `tests/*.rs` integration test reach a `pub(crate)`-everywhere crate's internals).*

**Symptom**: assumed a small `pub use crate::some_internal::Thing;` façade module, linked from an ordinary `tests/*.rs` integration test, could expose just enough of a `pub(crate)`-everywhere crate for a targeted test. Compiling it produced `error[E0364]: 'Thing' is only public within the crate, and cannot be re-exported outside`.
**Root cause**: Rust visibility is enforced at the re-export, not just at the original declaration — a `pub use` of a `pub(crate)` item cannot widen its effective visibility, so nothing short of making the item genuinely `pub` (or `pub(crate)`-widening the whole surface) reaches it from a `tests/*.rs` target, which links the crate as an *external* dependency and sees only what is truly `pub`.
**Resolution**: don't try to launder internals out through a façade. A test that needs `pub(crate)` internals must be compiled as **part of** the crate — a second crate root (`harness = false`, `--cfg test`, sharing `lib.rs`'s module tree) — never as an external target reaching in through a re-export.
**Lesson**: when a "just expose a little" façade for testing meets an `E0364`, that error is telling you the shortcut doesn't exist, not that the façade was built wrong — the fix is architectural (host the runner inside the crate), not a visibility tweak.


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
> *Non-core (testing/CI tooling, not GTK) — do NOT fold into the gtk4-rs skill. Found while answering "how do I test this on the Windows box"; sibling of #198/#199's investigation only in that they all touch the second crate root.*

**Symptom**: `packaging/windows/pipeline.ps1`'s step 5 ran `cargo test --features gtk-integration-tests -- --test-threads=1 --skip <known-failing-name>`, printed a pass, and exited 0 — having run **1 case of 149**: precisely the one it meant to omit, and nothing else. Every other case in the suite silently did not run on Windows.

**Root cause**: the `harness = false` runner (`src/gtk_suite.rs`) implemented filtering by pattern over raw argv — drop anything starting with `--`, treat the rest as libtest-style positional substrings. An unrecognised *flag* is harmless under that rule; an unrecognised flag's **value** is not. `--skip` was dropped, and the bare name after it fell through into the filter list, so "run everything except this" became "run only this". Nothing detects it: the selection is legal, the case passed, the exit code was 0. (The `--timeout` value escaped the same fate only by an accident of typing — a `parse::<u32>().is_err()` filter that a non-numeric value would never have satisfied.)

**Resolution**: parse, don't pattern-match — one pass with an explicit `VALUE_FLAGS` list whose values are consumed so they can never reach the filter list, and honour `--skip`/`--skip=` as a repeatable exclusion (`parse_args`). Guarded by `parse_args_excludes_skipped_cases_instead_of_selecting_them`, which asserts the pipeline's exact argv yields an empty filter list.

**Lesson**: adopting a harness's *interface* means adopting its *grammar*, not just its vocabulary — a runner that accepts positional filters "like libtest" has inherited libtest's whole argv shape, including which flags take values, and the gap shows up as a wrong test *population* rather than a wrong result. The general trap is that **a test-selection bug reports success**: the suite is green because the cases it ran passed, and no signal distinguishes "148 passed" from "1 passed" unless someone reads the count. Print the selected-vs-total count (this runner does: `running N of M cases`) and treat an unexpected N as a failure of the harness, not a quirk of the filter.

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

## 203. Restoring `SIG_DFL` and re-raising inside a fatal-signal handler exits *normally* with status 139 — the signal is blocked for the handler's own duration
> *Non-core (POSIX signal semantics + Rust/libc, no GTK internals involved) — do NOT fold into the gtk4-rs skill. Found while building the crash-forensics kit (`src/forensics/signal.rs`).*

**Symptom**: the new SIGSEGV handler wrote a complete, correct crash report and the process died — but the forked-child regression test failed with *"child exited normally (status 35584)"*. `WIFEXITED` was true, `WIFSIGNALED` false, and the exit status was **139** — exactly what a shell reports for a signal-killed process (`128 + SIGSEGV`), so every casual observation of the crash agreed with the intended behaviour.

**Root cause**: the canonical "report, then die as if unhandled" idiom

```rust
libc::signal(signal, libc::SIG_DFL);
libc::raise(signal);
libc::_exit(128 + signal);   // "unreachable"
```

is wrong on its own. `sigaction` without `SA_NODEFER` adds the delivered signal to the thread's blocked mask **for the duration of the handler** — that is what stops a handler re-entering itself. So `raise()` inside the handler cannot deliver: it merely marks the signal *pending*, to be taken when the handler returns. This handler never returns, so control reaches the "unreachable" `_exit`, and the process leaves by a normal exit that happens to carry the number everyone associates with a crash.

The consequences are all downstream of the process telling the truth about *how* it died:

* `WIFSIGNALED`/`WTERMSIG` say "exited cleanly", so any supervisor, test harness or wrapper that distinguishes a crash from a clean exit is misled;
* the kernel logs **no** `segfault at … ip … error 4 in <lib>` line — and that line is the entire evidence trail the crash-forensics plan was built to reconstruct from, so the fix would have cost more evidence than it added;
* `core_pattern` never runs, so a machine that *was* configured for cores stops getting them.

**Resolution**: unblock the signal between restoring the default disposition and raising it (`src/forensics/signal.rs::die`):

```rust
libc::signal(signal, libc::SIG_DFL);
let mut unblock: libc::sigset_t = std::mem::zeroed();
libc::sigemptyset(&mut unblock);
libc::sigaddset(&mut unblock, signal);
libc::pthread_sigmask(libc::SIG_UNBLOCK, &unblock, std::ptr::null_mut());
libc::raise(signal);
libc::_exit(128 + signal);   // now genuinely unreachable
```

`SA_NODEFER` at install time is the other route, but it is strictly worse here: it also lets a fault *inside* the handler re-enter it, which is the case the report's "write the header first" ordering exists to survive. Unblocking at the one point that wants delivery keeps re-entrancy defended everywhere else.

Guarded by `a_fatal_signal_leaves_a_full_report_and_still_kills_the_process`, which forks a child, raises a real SIGSEGV in it, and asserts `WIFSIGNALED` **before** it asserts anything about the report's contents.

**Lesson**: **a handler that "returns" by exiting has no unreachable code — only code you have not noticed running.** The generalisable trap is that this defect is invisible to every cheap check: the code reads correctly, the crash report is complete and accurate, the process does terminate, and the exit status is the same number a real signal produces. Only a test that inspects `WIFSIGNALED` rather than the exit *code* can tell the two apart — and it can only do so by driving a real signal through a child process, since the thing under test kills whatever runs it. When adopting a well-known idiom for process death, assert on the **mechanism** (signalled vs exited), never on the status number, because the status number is precisely what the broken version gets right.

## 204. Resolving a kernel segfault `ip` against `nm` output — the kernel's VMA base is the executable *segment*, not the ELF load base
> *Non-core (ELF/kernel diagnostics, no GTK internals) — do NOT fold into the gtk4-rs skill. The method that produced this project's recovered crash table; retained from the retired crash-forensics plan.*

**Symptom**: a kernel log line

```
scribobulate[1577312]: segfault at 1 ip 00007f3c9a4b5219 sp … error 4 in libgio-2.0.so.0.7200.4[7f3c9a17d000+38000]
```

is the only evidence a crash happened. Subtracting the bracketed base from the `ip` and looking the result up in `nm -D` yields a symbol — a confident, specific, **wrong** one. Nothing signals the error: the answer is a real exported function at a plausible offset, and it sends the investigation into code that never ran.

**Root cause**: two different bases are being conflated. `<VMA>` is the start address of the *mapping that contains the IP* — for a shared library that is its **executable** segment, which begins at a non-zero file offset. `nm` reports **link-time virtual addresses**, which are relative to the ELF load base. `IP − VMA` is therefore an offset into the exec segment, and comparing it against link-time vaddrs is an apples-to-oranges subtraction that happens to land inside *some* symbol.

**Resolution**: re-base onto the executable `PT_LOAD` segment's page-aligned vaddr before the lookup.

```
readelf -lW <lib>            # the LOAD entry with the E flag
vaddr_true = (exec_vaddr & ~0xfff) + (IP − VMA)
nm -D --defined-only -S --numeric-sort <lib>
```

Two checks keep it honest. **Sanity-check the base**: the kernel's bracketed `<size>` must match that segment's `MemSiz` rounded up to a page (libgtk-4 `0x9b000`/`0x40d000`; libgio `0x38000`/`0x112000`) — if it does not, the mapping is not the one assumed. **Then check the symbol's size** (`-S`): landing *past the end* of the nearest exported symbol means the real frame is a `static` function with no dynamic symbol, and on a distribution that ships no debug symbols (ScrAP-141) that is where naming stops. `g_file_equal +0x20` (size `0xF6`) is a genuine hit; `gtk_path_bar_get_type +0x14A89` is the same arithmetic reporting nonsense with a straight face. `error 4` means a user-mode **read** of an unmapped page.

The application now records its own executable mappings in a crash report (TECH.md § Diagnostics and crash forensics), which makes this arithmetic unnecessary for any crash the kit catches — the mapping line carries both the start address and the file offset. It remains the only route for a crash from a build without the kit, and for correlating a report against `journalctl -k`.

**Lesson**: **when an address-to-symbol calculation produces an answer, that is not evidence the bases matched** — a wrong base yields a wrong symbol, never an error, because every address is inside *something*. Any resolution across two tools' address spaces needs an independent consistency check (here, the segment size) before its output is believed, and a size-bounded symbol lookup so "nearest preceding symbol" cannot silently masquerade as "the function you are in".

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
## 206. A reference gate whose pattern demands a file extension the codebase's citations never write — clean, green, and blind to every dangler of that shape

> *Non-core (testing/CI tooling and documentation hygiene, not GTK) — do NOT fold into the gtk4-rs skill. Sibling of #201: both are gates that reported success over the exact defect they were written to catch.*

**Symptom**: `scripts/lint-references.sh` exited **0 with all six checks PASS** while **21 dangling plan pointers** sat in the tree — three retired plans cited 8, 12 and 1 times across `src/`, plus one in `sdd/TDD.md`. The plans had been retired and their files deleted; every pointer resolved to nothing. The commit that retired them recorded the sweep as complete, and `AGENTS.md` instructs that this check "is the only thing that can tell you the sweep is complete."

**Root cause**: check 6a matched `\bPLAN\.[A-Za-z0-9._-]+\.md` — it required the `.md`. But **code does not cite a plan by filename; it cites a plan by SECTION**: `// Footer status bar (PLAN.<topic> D3)`, `/// no row (PLAN.<topic> Q1 / TDD 20.2)`, `(PLAN.<topic> D1/D3)` — the real citations named their plans, spelled out here with a placeholder so this entry is not itself a dangling pointer. A section citation names the *document*, not the *file*, so it carries no extension — and every one of those still resolves to a file, so every one can dangle. The pattern was written from how a *path* looks, not from how the tree's authors actually write a reference.

**Resolution**: match the bare `PLAN.<topic>` form too, `.md` alternative listed FIRST so a full filename still matches whole under both POSIX leftmost-longest and .NET leftmost-first; normalise a bare citation by appending `.md` before resolving. Skip `PLAN.md` by name — SDD names plans `PLAN.<topic>.md`, so stripping the extension must leave a topic behind, and without that rule `src/links.rs`'s `./sub/PLAN.md#caf%C3%A9` link-parser fixture reads as a dangler. The pattern is now pinned in a variable beside the check-1 pattern and covered by `--self-test` with an **`expected → line` corpus that asserts WHICH substring is extracted**, not merely that something matched — a boolean corpus cannot see a pattern matching the wrong span.

**Lesson**: a gate's pattern encodes an assumption about **how the thing being hunted is actually written**, and that assumption is invisible in a passing run — a PASS means "no match", which is indistinguishable from "cannot match". Before trusting a reference gate, grep the tree for the *concept* by hand once (here: `PLAN\.`) and diff that against what the gate reports; the gap is the blind spot. And when a check's own header lists what it "deliberately does not match", re-read that list against real call sites, because the exclusions are where the author's model of the codebase is written down — and where it is wrong.

## 207. Two ports of one gate that share a pattern but not a file ENUMERATION — the parity claim is false, and the platform nobody runs is the lenient one

> *Non-core (testing/CI tooling and cross-platform process, not GTK) — do NOT fold into the gtk4-rs skill.*

**Symptom**: `sdd/POLICY.md` stated that `lint-references.sh` and its PowerShell twin "share one pattern and one `--self-test`/`-SelfTest` corpus, string-for-string, so **neither can drift into being the lenient one**." The claim was false at the time it was written. A dangling link in `.agents/`, `docs/` or `THIRD-PARTY-LICENSES.md` **failed the Linux gate and passed the Windows one**.

**Root cause**: the shared corpus pinned only the check-1 **regex**. Nothing pinned the **file set**. The shell side swept `find src tests sdd scripts packaging gtktest data . -maxdepth 4` — the bare `.` silently making one half repo-wide — while the PowerShell side built its set from seven named directories plus four named files. Two implementations, one pattern, two populations. Worse in both directions at once: the repo-wide sweep also pulled in **gitignored generated trees** (`docs/` review artifacts, `.agents/skills` third-party documents this project does not own), so the Linux verdict depended on what the developer had run locally rather than on the commit.

**Resolution**: `scripts/lint-references.scan` — one enumeration definition (roots, root files, excluded prefixes, maxdepth) that both scripts *read* rather than restate. Both gained `--list-scan` / `-ListScan`, which prints the enumerated set one path per line: **no automated test can compare the two implementations, because neither platform has the other's shell**, so the only available proof of parity is to run that on each and diff. POLICY now states the rule as "a gate is its pattern **and** the set it runs over" and requires the diff whenever either script's scanning changes. Two latent divergences found in the same sweep and closed: check 2's heading match (`^## [0-9]+\.`, exactly one space, vs `^##\s+(\d+)\.`), and a non-POSIX `\s` in check 4 that BSD/macOS grep would read as a literal `s`.

**Second round — the comparison artifact certified the wrong set.** Shipping `--list-scan` did not finish this; it moved the defect one level down, where it survived two more QA rounds. The contract said `maxdepth 6` and *three different enumerations* consumed it: check 6a was bounded on Windows and unbounded on Linux (`grep -r`), check 1 walked its own four-root list on both, and check 5b read the `prescriptive` class — an explicit document list that was neither. `--list-scan` printed the bounded one. So the artifact whose whole job was to prove parity **agreed with itself on a set some of the checks never looked at**, and both gates reported PASS either way. Measured by planting a file one level past the budget: caught by check 1 and missed by check 6 **in the same run, on the same port**. Invisible only because no real path was deeper than 4.

**Resolution, second round**: (a) *one* set — every check narrows the one enumeration instead of walking for itself, and `--list-scan` prints exactly it; (b) `maxdepth` became a **tripwire, not a filter** — exceeding it is a hard error naming the path. (b) is what makes (a) safe, and it is the non-obvious half: binding check 1 to a *truncating* set would have silently REDUCED what it caught, so the naive reading of (a) alone makes things worse. A budget that quietly drops files is indistinguishable from a tree that has none. (c) The self-test now asserts all of it — a file at exactly `maxdepth` is present, a file at `maxdepth`+1 makes the gate **fail** (not merely "is absent from the set": absence is also what a broken enumerator produces), and the artifact's output is byte-identical to what the checks consume. That last assertion is the one that would have caught this. Deriving depth from the repo-relative path also retired the `find -maxdepth` → `Get-ChildItem -Depth` off-by-one, which was the one licensed difference between the ports and which the contract had *claimed* the self-test verified while nothing did. Ordinal sort was pinned on both sides in the same change: the two ports already enumerated identical sets and the parity diff still showed fifteen changed lines from collation alone, and a proof whose clean state is noisy trains its reader to skim it.

**Lesson**: when one gate exists twice, **the pattern is the half everyone syncs and the input set is the half nobody does** — reviews diff the regexes, because regexes look like the rule. Enumeration is invisible: it lives in a `find` argument list or a `Get-ChildItem` loop and reads as boilerplate. Treat "which files does this run over" as part of the pinned contract, and be actively suspicious of a documented parity claim that no mechanism enforces — an unverified assurance is worse than none, because the next author trusts it and stops checking. Where the platforms genuinely cannot test each other, ship the *comparison artifact* (a `--list` mode) rather than a promise — **and then assert that the artifact prints what the consumers consume**, because an artifact derived separately from its consumers certifies nothing about them, which is the same defect it was introduced to close. A shared bound is not shared until every consumer reads it and the budget fails loudly when it is exceeded; and a parity artifact whose clean state is not an empty diff is only half an artifact.

## 208. A proc macro that moves the annotated item's attributes onto the generated BODY instead of the harness item — `#[ignore]` silently does nothing

> *Non-core (Rust proc-macro + test harness design, not GTK) — do NOT fold into the gtk4-rs skill.*

**Symptom**: `#[gtktest::test]` + `#[ignore]` on a test body ran the body anyway, under **both** harnesses. No warning, no error; the author's quarantine was discarded in silence.

**Root cause**: the attribute macro emits three items — the renamed body (`__gtktest_body_foo`, carrying the annotated item's own tokens), the libtest item (`#[gtk::test] fn foo() { __gtktest_body_foo() }`), and an `inventory::submit!` registration. Passing the annotated item's tokens through verbatim is the *right* default — it is what keeps a diagnostic inside a body pointing at the body as written — but it carries the **outer attributes along with them**, onto a plain helper function. `#[ignore]` on a plain function is inert: it is interpreted by the test harness, and that function is not a test. The real test — item 2 — got no `#[ignore]` at all. Nothing in the tree used `#[ignore]` when this was found, which is exactly why it survived: the first author to reach for it would have paid.

**Resolution**: partition the leading outer attributes before rewriting. `ignore` and `should_panic` are *harness* attributes and move to item 2; everything else (doc comments, `#[cfg]`, `#[allow]`) stays on the body, where it governs the code the author wrote. Because the second, `harness = false` runner has no libtest underneath it, `Case` gained an `ignored: bool` the macro fills in and the runner honours — and **reports** per case rather than filtering out of the selection, so a quarantined body stays visible in the output instead of vanishing (a quarantine that disappears from the report is how a quarantine becomes permanent).

**Lesson**: when a macro splits one annotated item into several, **every attribute on the original has exactly one correct destination, and "pass them through with the tokens" chooses that destination by accident**. Ask of each attribute *who interprets it* — the compiler, the test harness, or the body's own code — and route it to the item that reaches that interpreter. The failure mode is specifically dangerous because an attribute in the wrong place is almost never an error: attributes the harness owns are simply ignored elsewhere, so the code compiles, the intent evaporates, and the only signal is a test running that someone believed was not.

## 209. A guard test whose setup prevents the resource from ever existing cannot observe the leak it guards — it passes with the fix deleted

> *Non-core (testing discipline, not GTK) — do NOT fold into the gtk4-rs skill. The general "prove the gate fires" rule is #201/#206's; this is the specific shape where the SETUP, not the assertion, is what makes it vacuous.*

**Symptom**: `write_atomic` leaked its temp file on every failure path except a failed rename — `write_all` and `sync_all` returned through `?`, abandoning a randomised `.scribtmp` sibling next to the user's document on every full-disk or quota failure. The fix (a drop guard, so cleanup is the default and success the exception) came with a regression test that made the parent directory read-only, called `write_atomic`, and asserted no temp file was left. It passed. **It also passed with the guard's `remove_file` commented out.**

**Root cause**: a read-only parent fails at `create_new` — *before* a temp file exists. So "no temp file left behind" was true for the wrong reason: none was ever created. The test asserted a postcondition the setup guaranteed independently of the code under test. It was measuring the wrong failure: the fix is about a temp file that exists and is then orphaned, and the setup made orphaning unreachable.

**Resolution**: fail the rename *after* the temp file is successfully created and written — put a **directory** where the target file should be. The parent stays writable, so the temp file is created and written normally, then `rename` onto a directory fails (`EISDIR`/`ENOTDIR`) with a real orphan to clean up. Verified by mutation: with cleanup disabled this test now fails, and it passes when restored. The mechanism is separately covered by two tests over the guard type itself (armed → deletes on drop; disarmed → leaves the file alone, which is the direction that would delete a successful save).

**Lesson**: a test for a cleanup path must **reach the state that needs cleaning**, and the natural way to provoke a failure is often the one that short-circuits before that state exists. Read a guard test by asking *what would have to be true for this assertion to hold even with the fix removed* — if the setup alone answers it, the test is vacuous no matter how precise the assertion. Mutation-test cleanup and error-path guards specifically: they are the class where "the resource was never created" and "the resource was created and correctly released" produce byte-identical observations.

## 210. Windows PowerShell converting a value on your behalf instead of failing — the call site reads correctly in every instance

> *Non-core (Windows tooling and packaging, not GTK) — do NOT fold into the gtk4-rs skill.*

**Symptom**: at least seven defects across the Windows gates, packaging scripts and documentation, each of which presented as something other than what it was.

| Observed | Actually |
|---|---|
| A build gate died mid-run reporting a failure whose message text was the compiler's own **success** line — ``Finished `dev` profile … in 0.16s`` | stderr promoted to a terminating error |
| Six documentation files acquired mojibake and byte-order marks after a scripted rewrite that touched only the lines it was asked to touch | bytes decoded as CP1252, re-encoded as UTF-8-with-BOM |
| A version-control query answered `fatal: ambiguous argument 'dAByAGUAZQA='` for an argument typed correctly | `{tree}` serialised as a base64 `-encodedCommand` |
| A reference linter reported PASS over a corpus it was scanning **none** of | a non-matching capture group returned `''` |
| A guard reading a file that happened to be empty threw `NullReference` on `.Contains()` | `-Raw` on an empty file returned `$null`, not `''` |
| A gate written to tolerate a missing tool aborted the whole run when the tool was missing | the `2>$null` written to *provide* that tolerance is what made it fatal |
| The patch file carrying this entry's own fix was silently written as UTF-8-with-BOM with CRLF endings, which would have made it unapplicable | `>` redirection re-encoded what the tool had emitted correctly |
| A depth guard reported **every file in the tree** as past its budget, under `Set-StrictMode -Version Latest` | the value it tested was a collection, not a string, and `$x.StartsWith(…)` on a collection does not throw — it invokes the method per ELEMENT and returns a `bool[]`, which `if` reads as true |
| A `.ps1` stopped parsing with `Missing closing '}'` pointing at an innocent `{` three hundred lines from the real edit | the file has no BOM, so Windows PowerShell 5.1 decoded it as ANSI, and a UTF-8 em dash inside a **string literal** arrived as `â€”` whose last byte is `U+201D` — which PowerShell honours as a string delimiter |

**Treat that list as a floor, not a census.** Every one of the first seven was found *incidentally*, while doing something else — reading a file, checking a hash, sending a patch. None was found by systematic search, so the number describes who happened to be looking, not how many ways the mechanism manifests. An entry warning against unscoped measurement must not open with one. On meeting another, recognise the family rather than checking the list: a closed list invites "mine isn't one of the six, so this is something else", which is exactly how the next one survives.

**What was tried**: each was first pursued as a defect in the thing it appeared to blame — the compiler, the file's authors, the argument's spelling, the linter's regex, the file's contents. Every one of those investigations is a dead end, because in each case the component being blamed behaved correctly and the conversion happened between it and the call site. Two were additionally misdiagnosed *in writing* before being measured: the stderr case was recorded in the debt register with the wrong trigger ("any host that captures stderr") and sat there a full round; the encoding case was attributed to console display and dismissed, while it was corrupting committed files.

The stderr register entry deserves a closer look, because it failed in a way that is easy to mistake for an ordinary missing measurement. **The true cause was present in the text.** It sat third in a list of three — `2>&1 | …` — demoted to an example of a false generalisation, behind two confidently named and wrong ones. It was not missing; it was *outranked*. A reader consulting that entry would have read past the correct answer to reach the wrong one, because the wrong one was stated as the general rule and the right one as a mere instance. And the wrong rule *predicted the observation everyone actually had*: it said plain console runs were fine, and plain console runs were fine. So the entry looked **corroborated** every time anyone glanced at it. A false explanation that predicts your evidence is far harder to dislodge than one that is simply unsupported — it is not ignored, it is confirmed.

**Root cause**: one mechanism, seven costumes. PowerShell resolves an ambiguity by silently choosing a plausible value rather than raising, and the choice is invisible where it is made:

- `Get-Content -Raw` on an empty file yields `$null`, not `''`
- `[regex]::Match(…).Groups[1].Value` on a **non**-match yields `''`, indistinguishable from a real empty capture
- an unquoted `{…}` in a native command's arguments is parsed as a ScriptBlock and serialised as base64 UTF-16LE `-encodedCommand`
- `Get-Content`/`Set-Content` without `-Encoding` pick the ANSI codepage or add a BOM according to host and version — as do `>` and `Out-File`, which is how a tool's byte-correct output becomes a BOM'd CRLF file on the way to disk
- under `$ErrorActionPreference = 'Stop'`, `2>&1`/`2>$null` on a native command wraps each stderr line in a `NativeCommandError` and makes the first one terminating — and **the redirection operator is the trigger, not the fact that stderr is captured** (measured: a bare call, or its output assigned to a variable, does not throw; `cmd 2>&1` and `cmd 2>&1 | Out-String` do)
- a **method call on a collection is enumerated, not rejected** — `$arr.StartsWith('x')` returns one result per element instead of throwing, and `Set-StrictMode -Version Latest` does not catch it, so a value that accidentally became a collection yields a plausible answer rather than an error. Two return idioms feed this: `,$out` protects an empty array from unwrapping and **double**-wraps when the caller also writes `@(…)`, while a bare `return $collection` may hand the caller one object instead of N. Return a typed array (`[string[]]`), and pick the comma idiom to match how the caller reads it
- a `.ps1` **without a BOM is decoded as the ANSI codepage** by Windows PowerShell 5.1, so any non-ASCII character in the source becomes mojibake — harmless inside a comment, but a UTF-8 em dash inside a *string literal* becomes a curly quote PowerShell treats as a delimiter, and the parse error surfaces hundreds of lines away pointing at unrelated syntax. Keep every string literal ASCII (this project's scripts already write `--` for that reason), or give the file a BOM — and note that authoring on Linux, where the file is valid UTF-8, cannot reveal it

The last is the family's sharpest form because it inverts: the others convert a value into something plausible, this one converts a **success into a failure whose message is the success text**.

**Resolution**: pin every conversion explicitly rather than accepting a default — `-Encoding` on both halves of any read/write round-trip, quoting around any native argument containing braces, `$m.Success` before reading a capture, and a deliberate `$LASTEXITCODE` check instead of ambient preference behaviour (which differs by host *and* by PowerShell version). Where tolerance of a missing tool is wanted, set the preference locally around that call, restore it after, and say why at the site.

**But know what a wrapper does and does not fix, because "pin it explicitly" is weaker advice than it sounds.** A preference variable is *ambient*: it is re-armed at every new native call that does not go through whatever wrapper was written to contain it. A step-runner that neutralises the trap for the steps it runs protects exactly those steps and nothing else — a call added later, beside it rather than through it, is exposed again with no warning and no diff to review. That is a structural property of the mitigation, not a lapse in discipline, and it is why the eighth instance in this list appeared *while writing the fix for the fifth*, in the same file, hours apart. When the defect is ambient, ask what the fix's blast radius is: if the answer is "its callers" rather than "this script", it is a local remedy wearing the costume of a global one.

**Lesson**: in a shell that returns values rather than text, the dangerous defaults are the ones that produce a *usable* result from an unusable input — a null, an empty string, a re-encoded byte, a wrapped error. None announces itself, and the calling line reads correctly in every case, so review cannot catch them and only execution can.

Three corollaries paid for the hard way.

**An unmeasured trigger propagates further than an unmeasured symptom.** A symptom is checked the first time anyone looks at the feature, while a trigger is only checked when someone tries to *reproduce* — and a wrong trigger makes reproduction fail, which reads as "already fixed" rather than "wrong trigger". That is why a false trigger survived a full round in plain sight, in the title of a register entry, while everyone who tried the obvious invocation saw it work.

**A count does not carry its own scope — and neither does a search.** "219 files, 0 BOMs" and "244 files, 0 BOMs" read identically as evidence; nothing in the number says which population it ranged over. The first was extension-filtered and silently excluded every extensionless file. That makes a *scoped* measurement more dangerous than a wrong one — a wrong measurement can be contradicted, a scoped one can only be contradicted by someone who independently reconstructs the scope, which nobody does when the conclusion looks right. The same defect appeared twice in one session, in two agents, in opposite directions: a grep whose pattern structurally could not match the form being counted, reported as "5 references" against a true 28. The search form is identical and easier to miss: grepping for a *fixture's filename*, finding nothing, and concluding "this behaviour has no automated coverage" — when the coverage existed and simply built its own fixture instead of using that one. Absence of hits is evidence about the query, not about the world.

**A diagnosis written down before it is measured becomes evidence.** Three separate agents in one session each wrote a plausible causal story into a comment, a contract or a register title as established fact, and all three were caught by measurement rather than by review — review cannot catch them, because a wrong cause reads exactly like a right one. Brief the change, not the diagnosis, including when briefing yourself.

Finally, a technique rather than a warning, and the only one of these instances that came with a method attached rather than luck. **Encode a payload to inspect it, not to protect it.** The transport corruption was found by base64-ing a patch and reading the first four characters — `77u/` is a BOM you can see, while the same three bytes in front of `diff --git` are invisible in every editor, terminal and message window that would otherwise render them. Pair that with the inverse failure from the same session: a console rendered valid UTF-8 `§` (C2 A7) as `Â§` and nearly had a clean file reported as corrupt. One instrument, two opposite errors — inventing corruption that was not there, concealing corruption that was. The remedy for both is the same and it is not "assume corruption": put the bytes in a representation that cannot lie, and look at them.

**The two halves of that sentence are not interchangeable, and getting them backwards costs more than the corruption did.** Encoding a payload to *protect* it depends entirely on what the channel does to bytes. A channel that RE-ENCODES (charset conversion, editors, anything that rewrites bytes it does not understand) is made safe by base64, because ASCII-only survives it. A channel that RETYPES — a human or an agent reproducing the payload by hand — is made *less* safe, because base64 strips every error-detecting property the content had and supplies none. A unified diff is self-checking: its context lines must match the destination exactly, so applying it validates a large fraction of the payload against the target and a transcription slip is REJECTED. Base64 has no redundancy and no locality, so one wrong character silently alters six bits, and when the damage lands in added lines it passes a clean apply and produces a wrong result. Measured, on a retyping channel: five raw-text payloads, five exact matches; one base64 payload, one silent corruption that `git apply` accepted without complaint.

The near-miss is the part worth keeping. Had that one payload happened to survive, the conclusion would have been "base64 is the safer transport, use it for the large ones" — a rule adopted with more confidence than the failure produced, having quietly removed the only mechanism that would ever have revealed it was wrong. **A lucky success teaches the wrong lesson more firmly than a failure teaches the right one.**

## 211. A verification whose result nothing consumes — it reported the mismatch, and the corrupted payload was applied one line later

> *Non-core (shell/process discipline, not GTK) — do NOT fold into the gtk4-rs skill. Sibling of #206 and #209, and the distinction is the point: 206 is a gate that CANNOT match, 209 is an assertion that CANNOT fail, and this is a check that DID fail and was ignored by construction.*

**Symptom**: a patch carrying a permanent register entry was applied to the tree despite failing its integrity check. The check ran, computed the right answer, and printed `got: 8354…` beside `want: 21a6…` — and `git apply` executed immediately afterwards regardless, because the two were written as sequential commands rather than as a condition and its consequence:

```bash
echo -n "patch sha256: "; sha256sum /tmp/x.patch    # prints the mismatch
git apply /tmp/x.patch                              # runs anyway
```

The resulting file was byte-wrong in a way `git apply` cannot detect: the corruption sat inside *added* lines, not context lines, so the apply was clean and only the resulting blob hash differed. The entry being corrupted was the one documenting silent corruption.

**Root cause**: the check and the action were both present, both correct, and not connected. `echo` produces a verdict for a *reader*; the script had no reader. This is the failure mode a passing test suite cannot expose and a code review reliably misses, because the line containing the comparison is genuinely there and genuinely right — what is absent is the dependency between it and the next line, and absence has no syntax to notice.

It is also why the failure survives a habit that usually works: the author *did* read the output that time. Catching it required a human to notice a mismatch scrolling past, which is not a system working — it is a person compensating for a system that does not. The next run, executed unattended or with the output redirected, has nothing.

**Resolution**: make the comparison the control flow, never a diagnostic beside it.

```bash
got=$(git hash-object "$f"); want=ad83007765e5b4c388b795e1f36e8e57c7c63bcb
[ "$got" = "$want" ] || { echo "blob mismatch: $got != $want" >&2; exit 1; }
```

Verify the DESTINATION STATE rather than the transport: a content hash on the resulting file is strictly stronger than any check on the payload that produced it, and it is the only check that catches a corruption a clean apply cannot see. The same exposure exists in PowerShell, where `Write-Host` next to `git apply` reads identically and gates nothing.

**Lesson**: ask of every verification *what would be different if this check failed* — and if the answer is "a line of output would say so", it is not a check, it is a comment that happens to be computed. A gate that cannot fail is useless; a gate that fails into a void is worse, because its output is indistinguishable from verification and so it retires the suspicion that would otherwise have prompted a real one. Test this the way you would test any other guard: make it fail on purpose and confirm the run STOPS. Neither "it printed the right thing" nor a before/after comparison can tell you whether anything downstream depended on it.

## 212. `#[cfg(unix)]` on a test and "skipped on Windows" are indistinguishable in the report, and only one of them is true

> *Non-core (testing discipline, not GTK) — do NOT fold into the gtk4-rs skill. Related to #209 (a test that cannot observe what it guards) and #210's corollary that a search does not carry its own scope, but the mechanism here is neither: nothing is converted and nothing is mis-measured. The absence simply has nowhere to appear.*

**Symptom**: a documented behavioural rubric — containment must be decided on a symlink's *resolved* target — had a passing test, a checked-in fixture, and a manual checklist step. On one platform it had no coverage whatsoever, and every artifact reported success.

**What was tried**: the fixture was searched for by name across the tree; nothing referenced it from test code, which was read as "this behaviour has no automated coverage anywhere". That is the wrong conclusion from a correct search (#210), and it sent the investigation toward writing a new test rather than toward the one that already existed. The real defect was one attribute on that existing test.

**Root cause**: two independent silent absences over the same behaviour, which is why nobody hit it.

- The automated test carried `#[cfg(unix)]`. A cfg'd-out test **does not exist**: it is not skipped, not reported, not counted, and the test harness has no column in which "never compiled" could be distinguished from "passed". The suite is green over a limb it never built.
- The fixture was a version-control symlink. Where the platform cannot materialise one, the checkout writes an ordinary small text file holding the target path — so there is no link to resolve, the file is contained in its own folder, and the application *correctly* navigates to it. The manual step therefore produced the **inverse** of its specified outcome while looking like an unremarkable success: a tab opens containing one line of text.

Neither absence announced itself, and each made the other harder to notice: the automated gap was invisible because the test appeared to exist, and the manual gap was invisible because its failure looked like a pass.

**Resolution**: replace the platform *exclusion* with a platform-appropriate *implementation* plus an explicit runtime skip. Split the capability by target family so the test compiles and runs everywhere; where the platform genuinely refuses (creating a symlink on Windows needs elevation), print a marked `SKIPPED [...]` naming the real OS error and return, and where there is no such excuse, panic instead of skipping. Assert that the setup produced what it claims — a "successful" creation that left an ordinary file behind would make every assertion after it meaningless (#209). Then surface the skips: a harness that captures a passing test's output silences the notice precisely when it is needed, so the build pipeline greps for the marker and reports how many tests passed *without verifying their subject*. Finally, give the manual plan a precondition that says how to tell whether this checkout can exercise the step at all, and to record it NOT EXERCISED rather than ticking or failing it.

**Lesson**: **compiling a test out is not skipping it — it is deleting it, silently, on exactly the platform you were least sure about.** `#[cfg(platform)]` on a test reads as "this does not apply here", but what it produces is a report indistinguishable from full coverage, because no test harness has a representation for a test that was never built. Prefer a runtime skip that can be printed, counted and grepped over a compile-time exclusion that cannot. And when a rubric depends on a fixture the version-control system may not be able to materialise, treat the fixture's *existence in the form intended* as a precondition to be checked rather than an assumption — a fixture that degrades into a different valid object does not disable the test, it silently repoints it at something else.

## 213. An artifact that describes what you meant to do, shipped beside what you actually did, and never reconciled

> *Non-core (process and version control, not GTK) — do NOT fold into the gtk4-rs skill. Distinct from #211: there, a comparison ran and its verdict was discarded. Here no comparison ever existed — a description stood in for one.*

**Symptom**: a commit landed whose own message stated, in as many words, that one file was "deliberately NOT in this commit". The commit contained that file. Nothing failed, nothing warned, and every gate was green — the claim and the contents were simply never compared, because nothing in the toolchain compares them.

**What was tried**: nothing, and that is the point. The message was written during a debrief, before staging; the staging was performed later, by a different actor, from a working tree holding eight modified files rather than seven. Both steps were done carefully. Neither was wrong in isolation. The defect lives entirely in the join, which no step owned.

**Root cause**: a commit message, a changelog entry, a PR description and a doc comment are all **assertions about state that no tool evaluates**. They are written in the same change as the state they describe, published atomically with it, and read later as though they had been checked against it. They never were. Worse, they are written in the *intended* tense at a moment when the intention is fresh and the result does not yet exist — so the more careful the plan, the more confident and specific the false claim. A vague message ("various fixes") cannot be contradicted by its diff; a precise one can, and only the precise one carries the risk. The usual defences do not reach it: review reads the message and the diff as corroborating rather than as one checking the other, and a green test suite says nothing about whether a file's presence matches its description.

**Resolution**: **verify the destination state, not the intent.** Before treating a change as done, read what was actually produced rather than what was meant — `git show --stat` against the message's claims, a content hash of each file against the expected one, a tree hash when history is being rewritten. Where a claim is worth making it is worth checking: a message asserting a file's absence should be read against the file list, once, by someone. In this instance the discrepancy was caught by reading `git show --stat` on a commit that had already been made, purely because the claim was specific enough to check. Two related checks from the same episode illustrate the shape: a patch's *result blob hash* caught a payload that had corrupted in transit and applied cleanly, and a *tree hash* comparison before and after a history rewrite proved the rewrite had changed grouping and not content. In all three, the artifact verified was the destination state and not the operation that produced it.

**Scale-independence, learned by shipping one.** This was minted about a *commit message*, but the mechanism has nothing to do with commits: it is an assertion **co-located with the thing it describes, believed BECAUSE of the co-location, and verified by nothing**. The same defect at comment scale was shipped in the very round that minted this entry — a code comment describing arithmetic the same change had deleted, sitting directly above its replacement, because an edit replaced the statement and prepended a new comment without removing the old one. Nothing failed; a comment cannot fail. It read as authoritative precisely because it sat where authoritative things sit. So the entry applies to a commit message, a changelog, a PR description, a doc comment, a code comment, and a `README` claim about the tree — anywhere prose is published beside a result and their agreement is assumed rather than derived. Comments are the *worst* case of it, because a document at least gets reread at debriefing and nothing ever rereads a comment.

**Lesson**: **the plan is not the thing that decides — the staging is.** Any artifact written in the intended tense and published beside its result is unverified by construction, and its precision is what makes it dangerous rather than what makes it trustworthy. Prefer a check against the produced state to any assertion about it, and when you must assert, assert something a command can contradict. The general form: for every claim an artifact makes about state, ask *what would read this and disagree?* — if the answer is "a careful human, if they thought to", it is a description, not a verification.

## 214. `backtrace_symbols`'s BSD twin is not the safe half of the pair — the async-signal-safety argument you inherited is about a different hazard

> *Non-core (macOS/dyld dynamic-loader semantics + Rust/libc, no GTK internals involved) — do NOT fold into the gtk4-rs skill. Authored by the macOS seat during the QA round-3 audit of the fatal-signal handler; allocated here under the single-writer register protocol, reasoning unaltered. Kin to #205 (a platform's behaviour inferred from another's) and to #212's shape, where an absence had nowhere to appear.*

**Symptom**: none — this is a landmine marker rather than an incident, and it is recorded now precisely because the day it fires is the day nobody will re-derive it. A cross-platform review reported the fatal-signal handler's backtrace call as a live hazard on every platform. Measured on the macOS seat, it is not: the backtrace writer is gated to Linux/glibc, and every other target reaches a stub that records "(unavailable on this platform)" and calls nothing. The exposure below is INFERRED, not reproduced: it is what happens the day someone adds the missing platform arm.

**What was tried**: nothing yet, which is the hazard. The natural next step, once a macOS crash-forensics arm is built, is to mirror the working platform's path verbatim — the same two libc functions exist there under the same names with the same signatures, reachable through the same header. And the module's own doc comment already argues, at length and correctly, for the file-descriptor variant over the allocating one *specifically because the latter allocates*. A reader arriving with a `#[cfg]` arm to fill in finds the safety question apparently settled, and copies the call. It looks like a port, not a decision.

**Root cause**: the inherited argument is scoped to the **string-formatting** half of the pair. The allocating variant builds and returns an array of strings, so it must allocate; the fd variant writes the same strings straight out and genuinely does not. That comparison is sound and it exhausts nothing. Both functions must still **resolve** each frame address to a symbol, and on the BSD family that resolution goes through the dynamic loader's own image-list walk — machinery that is not on POSIX's async-signal-safe list and that takes loader locks. A signal arriving while any thread holds one of those locks — an ordinary lazy symbol bind in unrelated code is enough — deadlocks the handler, which is strictly worse than the missing report it was added to provide. The strongest available evidence short of reproducing the deadlock is architectural: the platform vendor's own crash reporter runs as a separate, already-running process that inspects the crashing process from outside, so that symbolication never happens inside the dying process at all. Nobody builds that if the in-process call is safe.

**Resolution** (for whoever adds the arm): do not call either symbolicating variant from the handler. Write **raw frame addresses only**, alongside the module/load-address map the handler already emits for other reasons, and resolve names **offline** from the saved report — an extension of work the module already does, not new machinery. Where symbol names are genuinely needed live, hand the raw addresses to a separate, already-running helper process, mirroring the vendor's own split. Both options stay inside the write/open/close/raise/arithmetic set the handler already restricts itself to, which is the actual test a new call must pass.

**Lesson**: **a same-named, same-signature API on a second platform does not inherit the first platform's safety argument** — and a safety argument that names one specific hazard it avoids is scoped to exactly that hazard, saying nothing about a different hazard hiding in the same call on a different implementation. The failure mode is not that the argument is wrong; it is that a correct, well-written, narrowly-scoped argument reads as a general clearance to the next person, who is arriving with a different platform in hand and no reason to suspect the scope. When porting a call across a `#[cfg]` boundary, re-derive its safety against the **new** platform's mechanism, and when writing such an argument, say what it does *not* cover — an unstated scope is the part that gets inherited.

## 215. Verifying a behaviour-preserving refactor with hand-written expectations tests your belief about the code, not the change you made

> *Non-core (testing discipline, not GTK) — do NOT fold into the gtk4-rs skill. Sibling of #209: there an assertion could not fail; here it can fail for a reason that has nothing to do with the change under test, which is the same waste pointed the other way.*

**Symptom**: a hot predicate was rewritten from a per-item backwards scan into a single bit carried forward — a change whose entire claim is that it computes the *same* answer more cheaply. The regression test written for it, asserting the transformed function's output on a handful of inputs chosen by hand, **failed on its first case**. The rewrite was correct. The expectation was wrong.

**What was tried**: the natural next move is to debug the rewrite, because a failing test over new code is overwhelmingly a defect in the new code. That is where the time went before the actual cause surfaced: the expectation had been reasoned from the *predicate's* contract while the assertion was made against the *function's output*, and between the two sat a second, unrelated mechanism that suppressed the effect entirely for that input. (Concretely: the input began with a tab, which in the document language makes the whole thing a verbatim block, so a wholly separate guard upstream of the predicate meant nothing was ever rewritten.) The expectation encoded a belief about the *language*, not about the change.

**Root cause**: hand-written expectations for a behaviour-preserving change assert the wrong proposition. The claim being made is *"new ≡ old"*, and a hand-written expectation instead asserts *"new ≡ what I believe"* — introducing the author's model of the whole pipeline as a third party to a two-party comparison. Every mechanism between the changed code and the observed output is now silently part of the test, so it fails when the model is wrong (wasting time on a correct change) and, far worse, it *passes* when the model is wrong in a way that happens to agree with a broken rewrite. The suite cannot distinguish these, because in neither case is the old behaviour present to compare against.

**Resolution**: **keep the old expression verbatim, in the test, as an oracle, and compare the two implementations at every input of a corpus.** The corpus targets what a rewrite of that shape gets wrong — for a carried-forward state bit, that the state must be the one *before* the current item, that every character class is handled as the original did, and that the reset condition restarts it. The result is a test that: asserts the proposition actually being claimed; cannot be fooled by a second mechanism downstream, because both sides run at the same layer; and needs no model of the surrounding pipeline at all. In the same episode a second, weaker version of the same evidence came free — the untouched suite of the module whose scan was replaced by an index passed unchanged, which is what established that change as a re-implementation rather than a behaviour change.

**Lesson**: for a change that claims to preserve behaviour, **the old code is the specification, and it is available**. Hand-written expectations encode what you *believe* the function does; a differential oracle encodes what it *did*. Only the second is evidence, and it is usually less work to write. Keep the old expression in the test rather than in the commit history — deleting it discards the only oracle you will ever have — and delete it later, if ever, as a separate decision.

## 216. A gate that checks a citation EXISTS cannot see one that points at the wrong real thing

> *Non-core (register/citation tooling, not GTK) — do NOT fold into the gtk4-rs skill. The fifth shape in the #206/#209/#211/#212 family, and the only one whose gate is not merely blind but structurally incapable: the property it checks and the property that matters are different questions.*

**Symptom**: a cross-register citation in a code comment named an entry about an unrelated subject. The comment described one lesson; the number beside it resolved to another. The reference lint passed. It had passed every run since the citation was introduced, and the citation had been introduced *by the sweep that existed to fix exactly this class of ambiguity*.

**What was tried**: nothing had been tried, because nothing had noticed. It surfaced only while auditing the citation convention for a different reason, and then only because the two registers involved happened to hold *each other's* lesson at the two numbers in question — a coincidence sharp enough to be visible. Every less symmetric instance of the same defect is still there.

**Root cause**: two independent failures that compose.

- **The gate asks the wrong question.** The check validates that a cited identifier is *defined* in the register. A citation naming a real entry about the wrong subject **is not a dangling reference** — it is a perfectly well-formed pointer to the wrong place. Existence and correspondence are different properties, and only existence is cheap: confirming a citation *corresponds* requires reading the comment and the entry and judging whether they are about the same thing, which no grep can do. So a passing run asserts strictly less than a reader assumes.
- **The mis-citation was produced by a normalising sweep.** Where two registers number independently, a citation's register lives in its *prefix* and its meaning lives in its *number*. Rewriting the prefix in bulk therefore looks like a formatting change and is a semantic one: the number silently re-resolves against a different register. A textual sweep cannot re-derive the number, because that requires knowing which lesson the comment meant.

The instance also shows why review does not catch it: the file contained two *correct* citations of the same number, about the subject that number really names. The wrong one was locally consistent with its neighbours.

(That count itself first went in as "three", unmeasured. It was `grep`-checked afterwards and corrected to two — in the entry about a citation naming a real thing while being wrong about it, which is the joke and also the point: an integer that reads as a measurement while being an impression is the *same defect one register level up*. Typeset a number you did not measure exactly like one you did and the reader cannot tell them apart, so **run the grep before writing the integer**, not after someone asks.)

And one detail settles how much to trust a register's own account of itself. The convention paragraph in *this file* states that the ambiguous form "was swept out of `src/`, `tests/`, and this file's cross-register citations, so the convention now holds tree-wide." Measured — and stated as the COMMAND, not as three integers, because the integers were the defect:
>
> ```
> git grep -ohE '(^|[^a-zA-Z-])AP-[0-9]+' -- '<dir>/*' | wc -l
> ```
>
> At the commit that first published this paragraph the answers were 334 / 256 / 7 for `src/` / `sdd/` / `tests/`. Within hours they were 335 / 257 / 7, because the round-3 additions landed in the same branch — the count was stale in the commit that published it. Worse, two honest measurements disagreed and neither could be adjudicated, because the paragraph never said what it was counting: bare `AP-N`, excluding the `ScrAP-` and `F-AP-` prefixes, is a predicate a reader has to invent. An integer with no predicate is not a weak measurement, it is an **unfalsifiable** one — you cannot reproduce it, so you cannot disagree with it specifically, so it is never corrected. The command is a fact a reader can re-run and dispute precisely. That is #213's shape — an artifact describing what was meant, published beside what was done, never reconciled — **sitting inside the register that defines #213**, written by the people who wrote it. No register is exempt from the failure modes it catalogues, least of all in the sentence where it certifies itself.

**Resolution**: fix the site, record the history at the site so the correction is not "corrected" back, and treat the convention itself as the defect. Make the ambiguous form **illegal rather than defaulted** — a scheme in which the correct and incorrect uses of a form are textually identical is not a convention — and gate on the property that is actually checkable given the constraints. Where one register is a document that may not exist on the machine, its citations cannot be validated at all; what *can* be validated is that no unqualified form remains, which needs nothing but a grep. Then migrate per site, re-deriving each number from the lesson its comment describes rather than carrying it across, one directory per commit so a mis-resolution is bisectable. And audit the already-normalised citations whose numbers exist in *both* registers, because those are precisely the ones a prefix-only sweep could have mis-resolved without leaving anything a checker could find.

**Two smaller variants, both found by the same round and both worse than the original in one specific way.**

*The mis-citation that is not greppable at all.* A comment asserted a coupling between two modules — "the handler sizes its buffer from this constant" — naming the other module in **English**, by its human name, not by path or identifier. The coupling did not exist: the buffer was a literal. This is the same defect as the entry above with the one property that made the original tractable removed. A citation naming the wrong entry at least names *something* a checker can resolve; a claim about structure written in prose resolves to nothing, so no gate can be built for it short of natural-language matching. A path-based enumeration cannot reach it, and neither can a register-ID lint. The only durable fix is to stop writing the claim and start writing the dependency — here, deriving the buffer's size from the constant with a `const` assertion, so the sentence becomes true by construction and the build fails if it stops being. Correcting the comment would have left prose in place of a check.

Note the trap in the obvious repair, because it is the more useful half: making the coupling literal (size the buffer to *exactly* the constant) would have made the prose true and introduced the fault the prose was warning about, since the format's width was a *minimum* and not a maximum. **A comment that describes a constraint is not evidence the constraint's stated form is the right one** — verify the claim before making the code match it.

*Verifying, then writing from recall.* A reviewer shipped nine wrong line references having grepped every one of them: it obtained the evidence, then wrote the finding a paragraph later from memory of the surrounding context. The same failure produced a wrong count in this very entry. It is not a discipline failure and "always grep first" does not prevent it — the grep *was* run. The gap is between obtaining evidence and using it, and it feels identical from the inside to having used it. The remedy has to be mechanical rather than attentional: **paste the tool's output into the artifact at the moment you write the claim**, so the number in the text and the number on the terminal are the same object rather than two things that agree.

**Lesson**: ask of every gate *what its PASS actually asserts*, and compare that against what the reader will assume it asserts. "The target exists" and "the target is the right one" feel like the same check and are not; the first is a grep and the second is a judgement. When they diverge, the gate does not partially cover the property — it covers a different property, and every green run spends the suspicion that would have prompted a real check. The corollary for bulk edits: **a rewrite that changes which namespace an identifier resolves in is a semantic change wearing the costume of a formatting one**, and is exactly the operation to do by hand.

## 217. A negative result is worthless without a positive control — "it was prevented" and "I cannot see it" produce identical output

> *Non-core (experiment design, not GTK) — do NOT fold into the gtk4-rs skill. The exact counterpart to #209: that entry says an assertion which cannot fail is worthless; this one says an observation which cannot succeed is worse, because it reads as evidence rather than as reassurance.*

**Symptom**: a containment property — a document-supplied URI must never reach the system launcher — rested on one hop nobody could settle by reading the code, because it is a claim about a toolkit's signal-emission semantics rather than about the application. The obvious experiment is to arm the guarded path, trigger it, and check that nothing launched. Run that way it returns "nothing launched" whether the guard works, whether the trigger never fired, whether the observation window closed too early, or whether the detector was watching the wrong thing.

**What was tried**: two seats attacked it independently and both had to correct the naive design before it could produce a result at all. Watching for a well-known application to appear answered "is that application running?", so a copy the operator had already opened read as a launch and a real launch could hide behind one. Checking synchronously right after triggering read zero **every time, including with the guard removed**, because the launch path is asynchronous — a wrong answer that looks right, survives review, and would have been reported as containment.

**Root cause**: a negative result is a statement about the *detector* as much as about the system. "The effect did not occur" and "this apparatus cannot register the effect" are the same observation, and nothing inside the experiment distinguishes them. Every specific way an apparatus can be blind — an ambient signal impersonating the effect, an asynchronous sink not yet reached, a trigger that silently did not fire — is invisible in exactly the reading the experimenter wants to be true. This is why the failure survives careful work: the more confident the guard, the more the null result confirms what was already believed.

**Resolution**: **run the same probe with the guard removed and require the effect to appear.** That control is not decoration, it is where the result lives: it proves the detector fires on this machine, through this toolkit, for this input. Alongside it, three method details each of which alone flips the verdict — wait long enough for an asynchronous sink before reading; key detection on an identifier nothing but this effect could produce; and isolate the hop under test from every other guard in the path, or a clean result may be some other layer doing all the work while telling you nothing about the one in question.

Best of all, **prefer an oracle with no side effect to control for**. In this case the toolkit's own default handler — the same code path that would have launched — also sets an ordinary observable property, so the question "did the default handler run?" became a synchronous state assertion instead of a search for a side effect in the world. That version needs no waiting, no unique identifier, and no launch on the control arm, which makes it safe to run anywhere and cheap enough to keep as a permanent regression test.

**Lesson**: when a result is a *negative*, the experiment is only as good as the proof that a *positive* would have been seen. Build the control first; if you cannot make the effect appear on demand, you cannot report its absence. And write the control's failure message to say what depends on it — a test that states its own trust dependency ("if this stops firing, the guard below proves nothing") outlives the memory of why it was written, which a comment beside it does not.

**The reporting variant (QA round 5, F-SEC5-001) — same claim, different register, and deliberately NOT given its own number.** A Windows pipeline step re-ran the test suite, grepped its output for `SKIPPED [`, and on an empty result printed, in green, *"none — every environment-dependent test verified its subject"*. If the re-run failed or produced nothing at all, `$skipNotices.Count` was 0 and that same line printed. **The "nothing to report" branch was also the "the report never ran" branch, and the failure case printed the MORE reassuring of the two messages.**

This is 217's claim moved from an *experiment* to a *report*: "prevented" and "unobservable" produce identical output, except here the second one is rendered in the vocabulary — and the colour — of the first. It is distinct from #211 (a check that fired into a void): this check does not fail, it is simply never taken, and its absence is reported as a clean measurement.

Numbered as an extension rather than a new ID on purpose. The register earns its keep by being filterable by TOC, and a near-duplicate entry costs every future reader a comparison to decide which one applies; the rule below is the same rule, so it belongs on the same page.

**The operative rule, general:** *any step that prints an all-clear must first prove that the thing it is clearing actually executed.* Capture the exit status of the command whose output you are about to interpret, and give "did not run" its own branch with its own words — `UNKNOWN — no conclusion available` is a different sentence from `none`, and an operator can act on the difference. Fixed at `packaging/windows/pipeline.ps1` step 4b.

## 218. Confidence ratchets across a relay — the hedge is dropped by whoever summarises, and nobody does anything wrong

> *Non-core (multi-agent / team process, not GTK) — do NOT fold into the gtk4-rs skill. Related to #213 (an artifact describing intent, published beside its result) but the mechanism is different: nothing here is written in the wrong tense; the claim is simply repeated with one qualifier fewer each time.*

**Symptom**: a note recording that one directory had not been reached by any reviewer was relayed one hop and arrived as a work assignment to give that directory a full review. It was neither — the directory turned out to be covered at five points by findings already in hand, four of which belonged to consolidations that had been deliberately deferred. The relay was caught only because the receiving side asked the originator to re-check their own claim, which they did by grepping their sources instead of trusting their summary.

**What was tried**: nothing failed and no step was careless. The originator wrote an accurate coverage note. The relayer wrote an accurate brief *from that note*. The recipient would have done accurate work *on that brief*. The defect exists only in the composition, which no single participant owns — the classic shape of a fault that survives conscientious review at every stage.

**Root cause**: **a qualifier and the claim it qualifies have different survival rates under summarisation.** Hedges are typically trailing ("…though this has not been measured", "…treat as unexamined rather than clean"), and a trailing clause is the first thing a summary drops, because it carries no new information about *what* is being said. The claim survives; the confidence marker does not. Each hop therefore raises the apparent certainty by one notch, and the format change compounds it: a *note* is a description, an *assignment* is a description plus an implicit "this is worth your day" that the note never asserted. Two hops turn "we don't know" into "go and do this". Nothing detects it, because at every stage the message is locally faithful to its immediate source.

**Resolution**: **label an unmeasured claim inline, on the sentence that makes it**, rather than in a caveat paragraph at the end — inline labels survive summarisation because they cannot be separated from the claim without rewriting it. Adopt a small closed vocabulary (MEASURED, ANALYSED, INFERRED, UNMEASURED) and attach it at the point of the assertion. When converting somebody else's claim into work for a third party, carry the label; if the source did not provide one, that is the moment to go back and ask rather than to supply a default. And on the receiving end, re-measure a load-bearing claim before acting on it — every instance in this project's record was caught that way and by nothing else.

**Lesson**: in any chain of more than two participants, **certainty is not conserved — it increases**. Treat the transformation of a claim into an instruction as the point of maximum risk, because that is where an unlabelled statement acquires an authority its author never gave it. The practical test before relaying: *would the person I'm sending this to be able to tell how sure the original author was?* If the answer depends on a caveat paragraph they may not read, the hedge is in the wrong place.

## 219. A remedy that lives inside one consumer reaches the consumers that already knew about it

> *Non-core (testing and tooling discipline, not GTK) — do NOT fold into the gtk4-rs skill. Found and fixed on the Windows seat during QA round 3, and allocated here under the single-writer register protocol; the module header it produced is the entry's own best statement of the rule. The discoverability rung of the enforcement ladder whose upper rungs are GTK4Rs/AP-130's clippy ban and module-privacy seal — **not** this register's #130, which is an unrelated librsvg lesson; the two registers number independently.*

**Symptom**: a recorded remedy — replace a compile-time platform exclusion on a test with a platform-appropriate implementation plus a **runtime** skip that can be printed, counted and grepped (#212) — had been written, reviewed, and landed. A later audit found it applied at **two of five** eligible sites. The other three still carried the exact construct it replaced, and were therefore still deleted rather than skipped on the platform nobody could check. No one had disagreed with the remedy, argued against it, or granted an exception.

**What was tried**: nothing, and that is the finding. There was no failed attempt to look back on, because the three unconverted sites were never considered — the remedy was applied where its author was already working and nowhere else. This is what makes the class hard to see: the sites that adopted the fix are the ones a reader finds when checking whether the fix exists, so the population *looks* fully converted from any vantage point that starts at the fix.

**Root cause**: the remedy was implemented **inside one consumer's own private test module**, where nothing else in the crate could import it. So adopting it anywhere else meant re-deriving it from scratch or copying it, and both cost more than leaving the old construct alone. The rule was not weakly enforced; it was **weakly reachable**, which behaves identically and is much easier to overlook, because reviewing the remedy shows a correct, well-documented, well-tested implementation. Nothing about reading it reveals that four-fifths of the codebase cannot call it.

The general shape: **a mitigation's adoption rate is bounded by its accessibility, not by its correctness or its documentation.** A rule that must be remembered *and* re-implemented is applied by whoever wrote it and by nobody else, and it decays silently because each unconverted site is individually unremarkable.

**Resolution**: hoist the remedy into a module every consumer can reach, so using it is cheaper than avoiding it, and give that module a header stating the rule it encodes rather than only the API it exposes — a reader arriving to use the helper is the reader most likely to act on the reason. Then convert the remaining sites. The hoisted version can also do things a per-consumer copy will not bother with: here, asserting that the fixture it built is **really** what it claims to be before any test trusts its own verdict — a control arm (#217) at fixture level, which is exactly the kind of rigour that gets dropped when each site re-implements.

**An open case, left open deliberately.** This register's own citation-accuracy remedy — *paste the tool's output into the artifact at the moment you write the claim* (#216) — has now failed **five times across three parties**, every one of them actively trying not to make the mistake. That is not five lapses of attention; it is a measurement of the remedy's adoption rate, and the reading is that it has not been adopted. **A mechanical remedy that only fires when you feel careful is not mechanical — it is a resolution with a tool's name attached.**

The diagnosis is this entry's subject rather than #216's: the remedy is correct, written down, and agreed by everyone who has hit the defect, and it is still not *reached* at the moment of writing, because the summary is the artifact in front of you and the primary source is not. That is reachability, not accuracy. What would make pasting the output cheaper than retyping the number is **not currently known**, and it is recorded here as an open question rather than a solved one — because a remedy filed as adopted when it is not is the precise shape this entry exists to catch, and this register is not exempt from it.

**Lesson**: when a lesson has been recorded and *still* is not being followed, the first hypothesis is not that people forgot — it is that **following it costs more than not following it**. Fix the cost, not the memory. Climb to the strongest rung the situation allows (convention → discoverability → lint ban → module-privacy seal), and reach for this rung specifically when the risky construct is legitimate elsewhere and so cannot be banned outright. The audit that catches this is not "does the remedy exist?" but **"how many sites are eligible, and how many adopted it?"** — a question worth asking of every mitigation this register contains, because the answer is never automatically all of them.

## 220. A regression guard built from the instance you fixed has coverage exactly equal to the fix

> *Non-core (testing discipline, not GTK) — do NOT fold into the gtk4-rs skill. Sits between #206 and #209: #206 is a gate whose pattern cannot match anything, #209 an assertion that cannot fail, and this one a guard that matches and can fail — but only over the very inputs already known to be fixed.*

**Symptom**: a super-linear scan was found in a parser, fixed, and shipped with a growth-ratio regression test that asserted the exponent rather than a wall-clock bound — careful work, the right property, machine-independent. An independent audit then found the **same** quadratic, in the **same** function, still live under a different delimiter of the same family. The new test passed with the survivor fully intact.

**What was tried**: nothing, because nothing had failed. The fix was correct and complete for what it covered; the test was correct and discriminating for what it covered; and the two covered the same thing.

**Root cause**: the guard's input was written by copying the reproduction that motivated the fix. That is the natural thing to do — it is the input you have just been staring at, the one you know provokes the defect — and it silently defines the test's coverage as *the fix's* coverage. A guard so constructed cannot discover anything; it can only confirm. Two independent parameters had been frozen to a literal, and freezing either alone was enough:

- **The instance.** The defect's shape was "a repeated fixed string is located by a scan rather than an index". Five delimiters had that shape. The test named one — the one that had been fixed.
- **The input shape.** The surviving scan sat *past* a guard clause that the test's input never satisfied: it built openers with no closer, so parsing stopped at the closer lookup and never entered the construct body where the second scan lived. Even the right delimiter with the wrong shape would have passed.

The deeper reason this survives review is that a defect and its guard are usually written in the same sitting by the same author, so the guard inherits that author's *current* model of where the defect lives — including its blind spots. Review then reads the guard, sees a real assertion over a real input, and has nothing to catch.

**Resolution**: parameterise the guard over the **enumeration the defect's shape is defined by**, not over the instance that produced it — and put that enumeration in the source rather than the test, so the production code and the guard read the same list and a sixth member is covered by both without anyone deciding to cover it. Then enumerate the input **shapes** too: one per guard clause on the path to the hazard, since a shape that stops early exercises none of what follows it. Here that meant a single delimiter table, a derived index of everything the pass searches for, and a test that loops over the table crossed with "openers alone" and "openers plus one closer".

Mutation-testing is what proves the difference and it should be run *per parameter*: the corrected guard names the surviving delimiter and shape in its failure message (`{~~ … ~~} (openers + one closer): grew 16.8x for 4x the input`), where the original could not have mentioned them at all.

**Lesson**: after fixing a defect, ask **"what is the population this defect is one member of, and does my test range over it?"** before asking whether the test is strong. A strong assertion over a co-extensive input is the most convincing form of no coverage — it survives review, mutation-testing *of the instance it names*, and its own author's re-reading. The general control is to derive the guard's inputs from a structure the production code also consumes, because a hand-written list in a test is a list that only grows when someone remembers it, and the person who would remember is the one who has just been reminded.

## 221. A comment explaining why a test asserts less than its name promises is where a false premise hides

> *Non-core (testing discipline, not GTK) — do NOT fold into the gtk4-rs skill. #209's companion: 209 is about an assertion that cannot fail; this is about the sentence that persuades a reviewer not to ask why it cannot.*

**Symptom**: a test named for refusing an over-limit input wrote a **one-byte** file, asserted it was **accepted**, and then asserted the wording of a refusal it constructed by hand. The branch it existed to cover was never executed by anything. It sat in the module written specifically to be the single home of that policy, in the round that minted the register entry about vacuous assertions.

**What was tried**: nothing was tried, and that is the finding. The test carried a comment stating that a real over-limit file was impractical to create, so the size half was asserted "through the constant rather than the filesystem". The premise was false and cheap to falsify: setting a file's length produces a **sparse** file of any size instantly, allocating no blocks, in two lines. Nobody checked, over multiple review passes.

**Root cause**: a stated impossibility is treated as settled. A missing assertion invites the question "why not?"; an *explained* missing assertion answers it pre-emptively, and the answer is accepted at the cost of reading it — nobody re-derives a constraint someone else has already reasoned about, especially when the reasoning sounds like it came from experience. So the comment does not merely fail to prevent the gap, it **converts the gap into a documented decision**, which is a stronger defence against review than the gap alone would have been. Both halves compose with the naming: a test's *name* is what a reader remembers and what a coverage summary reports, so the register of what is tested is built from names while the truth lives in bodies.

The same structure explains the neighbouring irony: the module's own prose argued that a size check and a type check are independent and neither implies the other — and shipped with the type half tested against a real named pipe and the size half asserted by construction. An artifact can state the principle it violates without anyone noticing, because the statement and the code are read as one act.

**Resolution**: execute the branch. Where creating the input looks expensive, spend five minutes checking whether it actually is — sparse files, memory-backed filesystems, injectable clocks and fakeable metadata cover most "impractical" cases, and a constructed value that stands in for a real one is only ever testing the constructor. Assert **both sides of every boundary**, at the boundary: a test that only probes far from the threshold cannot tell `>` from `>=`. Then mutation-test the branch *and* the comparison operator separately, since deleting the branch and loosening it fail in different ways.

**Lesson**: treat "we cannot test this because X" as an **unverified claim inside the test**, subject to the same standard as the code — and where it survives verification, write down how it was checked and when, so the next reader inherits evidence rather than a conclusion. Two search patterns fall out of it, both cheap: a test whose **name** describes a behaviour its **body** never provokes, and any test comment beginning "real … are impractical / too slow / not possible to". The second is a grep, and it points straight at the first.

## 222. Two gates, each correct, enforcing opposite things — and neither can see the other

> *Non-core (documentation/tooling governance, not GTK) — do NOT fold into the gtk4-rs skill. A new shape for the #206/#209/#211/#216 family and the first one where **nothing is broken**: the automated check is right, the pattern matches, the assertion can fail, and the property checked is the property that matters. What is stale is the WRITTEN RULE, and no artifact in the system is looking at it.*

**Symptom**: the policy document instructed developers to use an attribute that the repository's own lint gate rejects outright. A reader who followed the written rule broke the build; a reader who satisfied the gate was contradicting the written rule. Both artifacts were confident, both were internally consistent, and they had been disagreeing for as long as the gate had existed.

Worse than a contradiction between two documents: the *correct* rule was also present, in the same file, in full, with its reasoning — filed under a heading scoped to **one platform**, because that is the platform on which the problem had been discovered. A developer working on any other platform has no reason to open it. The superseding rule was therefore present, correct, and unreachable from the section it superseded.

**What was tried**: nothing, for the same reason as always — nothing failed. The gate passed on every run, because the code was right. The document was never executed by anything. The defect was found only because a reviewer happened to read the gate's own output, saw the attribute named there, and went looking for it in prose.

**Root cause**: two mechanisms compose, and each is individually reasonable.

- **A lint's input set is source, so a lint cannot see documentation.** Every check of this kind greps the code for the construct it bans, which means the earliest thing it can catch is a developer who has *already* written the wrong thing. It structurally cannot catch the document that told them to. The two artifacts are in different universes: one is executed, one is read, and nothing in the toolchain reads.
- **A rule gets filed where it was discovered, not where it is needed.** The corrected rule lived under a platform-specific heading because a platform-specific investigation produced it. That is the natural place for the author and the wrong place for the reader — the same mechanism as #219 (a remedy that reaches only the consumers who already knew about it), one level up, applied to a document instead of a module.

The compounding detail: a deliberate sweep found a **second** stale prescription elsewhere in the same document that the original reviewer had not seen. Only the first sat near text the gate's output made anyone look at. One instance is a correction; two is evidence that the class needs a check.

**Resolution**: correct the prose, then close the loop that let it drift — a gate over the *prescriptive* documents, failing on prose that names a banned construct without naming its replacement.

The discriminator is the whole design, because these documents **must** be able to name the banned construct: the paragraph explaining why it is banned says it three times, and a gate that failed on that would be #206 inverted — unable to distinguish a mention from a use, and therefore guaranteed to be switched off. Two attempts were needed:

- *Line proximity* (flag a mention with no replacement named within N lines) failed on three correct lines whose contrast sat five to nine lines away. Widening the window is a fudge that trades one arbitrary threshold for another.
- *The paragraph* is the honest unit. A passage that **contrasts** the two names both; a passage that **prescribes** names only one. That property does not depend on how the author wrapped their prose, and it needs no threshold.

**The general remedy, because this shape recurs within hours in unrelated tools:** when a checker cannot tell a MENTION from a USE, fix the **corpus**, not the regex. Sharpening the pattern is the instinctive move and it is the wrong one — every refinement is another arbitrary threshold, and the text that legitimately discusses a banned construct is written by people who are being careful, so it will keep finding new ways to look like the thing it describes. Narrowing the input set to documents where the construct could only ever be a *use* dissolves the ambiguity instead of adjudicating it. The same day this check was written, a second and completely separate one — a test asserting which modules a signal handler may reach — failed on the text of its own assertion, and was fixed the same way: by scanning only the implementation half of the file rather than by a cleverer pattern.

Three further details each of which is load-bearing: scope the gate to documents a reader could **act** on (scanning the historical register produced thirteen failures on entirely correct text, and a gate that cries wolf on correct text is a gate someone deletes); define that document set in the **shared contract both platform ports read**, never as a literal list in each script, because two hard-coded lists are two lists that drift; and make an **empty** document set a hard error, since deleting the class makes the check pass over nothing — both ports then agree, cleanly and vacuously, and the cross-platform diff that is supposed to catch divergence reports no difference at all. Mutation-test against both real instances, including the one the reviewer never found.

**Lesson**: **an automated check and a written instruction are two enforcement mechanisms with no channel between them, so the written one drifts silently and the automated one punishes whoever obeys it.** Whenever a lint bans a construct, ask immediately *what prose in this repository tells someone to use it* — the answer is a grep, and it is never asked because the gate is green.

Two corollaries worth keeping.

**A rule is filed where it was discovered, not where it is needed.** Worth stating as its own defect, because it is not staleness: the correct rule was *present, correct, complete with its reasoning, and unreachable*. Nothing about it was out of date. It sat under a heading naming one platform, because a one-platform investigation produced it, and a reader on any other platform had no reason to open that section. Staleness you find by re-reading; this you find only by asking "which section would someone with this question actually open?" — so cross-link it from the section it supersedes, or it does not exist.

**When a document and a gate disagree, the gate is not automatically right.** The tempting reason is observability — the gate is the one that gets run. That is true and it is the weaker half. The stronger asymmetry is **falsifiability**: a gate can be mutation-tested, and a document cannot be. So a check that has survived a deliberate attempt to prove it wrong has earned something the prose beside it never had the opportunity to earn. Note what that licenses, though — it is a reason to *distrust the document*, not a reason to *trust the gate*, and those are different moves.

Because the gate has its own failure mode, and it is the one this entry's own remedy walks into: **a gate can itself be an unratified artifact.** If a check encodes a constraint nobody with the authority to set it ever set — an agent added it, a contractor added it, it arrived with a template — then when it contradicts a document it is the *less* authoritative artifact winning on the sole ground that it executes. That is not hypothetical: the check described in this entry's Resolution was written by an agent in the same campaign, on the same branch, as an unratified policy section by the same author, and only one of the two had anyone asking who authorised it. Prose gets governance attention because it looks like a decision; a script looks like an implementation detail and slips through. So: **"it runs" establishes neither correctness nor authority.** Ask of a gate exactly what you ask of a rule — who decided this, and can it be shown to be wrong?

## 223. Write a finding as a testable proposition, not as a conclusion — a conclusion recruits agreement, a proposition recruits a measurement

> *Non-core (review/claim discipline, not GTK) — do NOT fold into the gtk4-rs skill. The counterpart to #218: that entry is about a claim losing its hedge as it travels; this is about the GRAMMATICAL FORM of a claim determining whether anyone ever checks it. #218 makes a claim honest. This one makes it productive.*

**Symptom**: a reviewer reported that a governance property could not be established — single-seat authorship of a shared document was, they said, unverifiable because the history had been squashed. The claim was accepted, in writing, twice. It was also wrong: the pre-squash tips survived in the reflog, and recovering them took four commands. The recovery then found an actual violation of the governance rule in question, which nobody had been looking for and which no other artifact would ever have surfaced.

**What was tried**: nothing, for a while, because the claim did not read as something to try anything about. It read as a result.

**Root cause**: the claim happened to be phrased as a **bounded, testable proposition** — "unverifiable *from the commit record*" — rather than as the conclusion it was serving — "attribution is lost". Those two sentences carry the same belief and invite opposite responses. A conclusion offers a reader two moves, agree or dispute, and both are cheap and neither produces new information. A proposition names the thing that would have to be true, which is an invitation to go and look. Had the sentence been "attribution is lost", there would have been nothing to attack and the violation would still be sitting there.

The asymmetry is worth stating in full because it inverts the usual advice: **an unmeasured claim written as a testable proposition is more useful than a measured one written as a conclusion.** The first recruits a second measurer, and the second closes the question — often with an answer neither party predicted. The second forecloses the exchange while sounding more rigorous. Confidence and productivity are not the same axis, and review culture consistently rewards the wrong one.

There is also a reason this is hard to self-correct. Writing a finding as a conclusion feels like *doing the reader a service* — you did the work, you are sparing them the derivation. But the derivation is the part that can come back different.

**Resolution**: state what would have to be true, and where you looked, in the sentence itself. Prefer "X is not recoverable *from Y*" to "X is lost"; "I grepped *Z* for *pattern* and found nothing" to "there are none"; "this holds *on the platform I ran it on*" to "this holds". Each of those is the same information with the search space attached, and the search space is what lets someone else falsify it cheaply — including you, later.

**Reproducibility is not correctness, and a shared method manufactures the appearance of both.** When two people measure the same claim with the same predicate and agree, the agreement is evidence that the predicate is *shared* — not that it is *right*. Two reviewers running the same flawed grep agree perfectly, and their agreement is the strongest possible signal in favour of the flaw. So a second measurement is only worth what its independence is worth: if you were handed the command, you have re-run someone else's method, and the thing still unchecked is the method. State the predicate precisely enough that a reader can disagree with *it* rather than only with your arithmetic.

Two corollaries this project has already paid for. **A negative result should name its corpus and its predicate**, or a second measurer must invent one and the two of you will disagree without being able to adjudicate (the failure #216 records). And when you *do* hand someone a proposition about their own work, expect it to come back inverted; that is the mechanism working, not a defect in the review.

**Lesson**: **findings have a grammar, and the grammar decides whether the finding gets checked.** Before sending one, read it back and ask what a reader can *do* with it. If the only available responses are "yes" and "no", it is a conclusion and it will end the enquiry wherever your own effort happened to stop. If a reader can see what to run, it is a proposition, and the enquiry continues past you — which is the entire point of having more than one reviewer.

## 224. A squash makes single-seat authorship unprovable — on a deadline nobody is watching

> *Non-core (version control / project governance, not GTK) — do NOT fold into the gtk4-rs skill. Written to outlive its own evidence: the artifacts that prove the instance below expire, by an accepted decision, and this entry is what remains afterwards.*

**Symptom**: a governance rule said an agent must not autonomously add a rule to the policy document. Whether that rule had been kept could not be answered, because the commits that would answer it had been squashed — three seats' work collapsed into one commit per review round, authored and dated as a single act. A claim that one seat had touched only one section of the document was accepted twice, in writing, because nothing available could check it.

**What was tried**: the commit record was read, and it genuinely cannot answer the question — the squash is lossless about *content* and total about *attribution*. The mistake was stopping there. The pre-squash tips were still reachable in the **reflog** of the machine that performed the squash, and recovering them took four commands. Applied, they showed the claim was false: one round's policy delta was 41 lines added, of which 13 came from another seat's merge and **28 were authored on the claiming seat between the reset and the squash commit — one contiguous block, an entire new policy section.** The governance rule had been broken, by the agent asserting it had not been.

**Root cause**: two separate properties, and conflating them is what makes this invisible.

- **A squash destroys attribution, not history.** The tree is preserved exactly; the question "who wrote this line" is what disappears. Nothing warns about this because nothing is lost that anyone was looking at.
- **The only artifact that can restore it is local, unreferenced, and on a timer.** A reflog is per-clone, is not pushed, is not fetched, and prunes on a schedule (90 days by default). So attribution after a squash is recoverable *only on the machine that squashed, only until it prunes, and never for anyone else at all*. Every day the evidence looks exactly as present as it did the day before, until it is simply gone.

The compounding governance problem is that this bites hardest on precisely the documents whose rules are *about who may change them*. A rule of the form "an agent may not autonomously add a policy" is unenforceable if no artifact can attribute a policy line to an agent — and unenforceable in a way that produces no error, no failing check, and no visible gap.

**Resolution**: decide deliberately, then **write the consequence down while the evidence still exists**. Tagging the pre-squash tips converts a pruning reflog into permanent refs and preserves attribution; letting them prune is a legitimate choice, but it must be a choice rather than a default, because the default is silent and irreversible.

This project chose not to tag. So, recorded here because in ninety days nothing else will be able to say it: **the round-3 policy addition was authored on the Linux seat, in the commit range `cd1b3f6..260f0cc`, +28/−0, the "SDD register writes" section.** It was reported by that seat against itself, flagged in place rather than deleted so that both the rule and the evidence survived, and subsequently ratified by the operator on its merits — so the rule now stands with the right authority behind it. That sequence is the reason flagging beat deleting: deleting would have destroyed a load-bearing constraint and its provenance together, and silently keeping it would have left an unratified rule wearing the costume of policy.

The two entries are a matched pair and are best read together: **#224 is what you find when you falsify a “cannot”; #223 is why the “cannot” was falsifiable in the first place.** The claim here was written as a bounded proposition — "unverifiable *from the commit record*" — which named a search space and therefore invited someone to look outside it. Twice this campaign a claim was falsified by a reader treating it as a proposition rather than a conclusion, and both times the falsification surfaced something neither party was hunting for.

**Lesson**: **a squash is a decision about attribution, not just about history, and its evidence expires.** Before squashing across authors, decide whether anyone will ever need to know who wrote what — governance rules, licence provenance, and security review all need it — and if so, tag the pre-squash tips, because a reflog is not a record, it is a grace period. More generally: when a fact is recoverable only from an artifact that is local, unreferenced and time-limited, **treat writing it into prose as part of establishing it**. A finding that outlives its evidence is a finding; one that does not is a memory. And the meta-lesson, which cost nothing to learn here: *"this cannot be determined"* is a claim about a search, and search spaces have edges someone has not looked past — the commit record was the wrong place to stop, and one command in a different place answered it.

## 225. Four denial-of-service paths in four subsystems were one omission: nobody had said the project had an opinion about input size

> *Non-core (input-cost policy / threat modelling, not GTK) — do NOT fold into the gtk4-rs skill.*

**Symptom**: a review round found four unrelated ways an ordinary-looking document could hang or kill the application — an unbounded recursion that aborted the process from a ~1.1 KiB file, two super-linear scans that froze the UI for minutes on a few megabytes, and an unbounded read. They sat in four subsystems, written by different hands at different times, none of them careless.

**What was tried**: nothing, because each was individually invisible. Every one of the four authors was writing correct code against the inputs they had in mind, and none of them was the person who should have decided what an input is allowed to cost.

**Root cause**: the four defects were not four oversights, they were **one** omission observed four times. The project had an explicit and well-developed opinion about what a document may *reference* — URL allow-listing, image-path containment, symlink and traversal checks — and no opinion at all about what a document may *cost*. Those are the same threat model seen from two sides, but only one side had ever been written down.

The consequence of a missing policy is not that people decide badly; it is that **each author reasonably assumes someone upstream decided.** A parser author assumes the loader bounded the file. The loader author assumes a document that parsed is a document worth rendering. Nobody is wrong locally, and the gap is only visible from a vantage point no individual change occupies. This is why the count matters: one instance is a bug, four in four subsystems is a missing decision, and fixing the four without making the decision leaves the fifth author in exactly the position the first four were in.

A second-order effect worth naming: a limit chosen by intuition and a limit chosen by measurement are **indistinguishable once written down**, and the intuitive one fails silently later — either it is too tight and rejects real documents, or too loose and does nothing. So the absence of a stated method is itself part of the omission.

**Resolution**: make the opinion a thing a reader can find, and split it by kind, because a lesson and a rule are different artifacts with different homes and filing them as one is what made this hard to place:

- **The rule is a priori and belongs in POLICY** — "every document read goes through the one admission test, never a comparison against the constants" — and it costs one line, because
- **the values stay in code, as their single source of truth**, each recording how it was measured and the margin over the threshold actually observed. A number restated in prose is a number that will stop matching.
- **The lesson is a posteriori and belongs here.**

The admission test is one function rather than a constant callers compare against, for a reason that generalises: **two conditions that are each necessary and neither sufficient must be enforced together or they will drift apart.** A size check alone admits a FIFO, whose reported length is zero, and then blocks forever on the read; a type check alone admits a 40 GiB regular file. Callers offered two constants will use one.

**Lesson**: when several unrelated components each mishandle the same class of input, **resist fixing them individually — count them first.** A cluster is evidence of a missing decision, and the decision is the deliverable; the individual fixes are its consequences. Ask of any project what it has an opinion about, and specifically ask whether its stated threat model covers *cost* as well as *content*, because cost is the half that gets omitted — it looks like a performance concern rather than a security one right up until an attacker sends a 1.1 KiB file that terminates the process. And write the decision somewhere a *future* author will collide with it, since the failure mode is not disagreement but the reasonable assumption that someone else already decided.

## 226. The check your self-test does not cover is the one that ships broken — and a single-file corpus cannot falsify a multi-file bug

> *Non-core (testing/gate discipline, not GTK) — do NOT fold into the gtk4-rs skill. #217's sibling from the other direction: 217 says a negative result needs a positive control; this says a control that cannot reach the failure mode is not a control at all, however carefully it was run.*

**Symptom**: a lint check was added, mutation-tested against both real defects it existed to catch, run green, cross-checked in a second language, and shipped. It contained two bugs. Every reported line number was correct and every reported **filename** was wrong — always the *next* file in the list, so only the last entry in the set read correctly. Worse, the same root cause silently swallowed real hits: a later file mentioning the search term suppressed an earlier file's finding entirely, turning the check into a false negative.

**What was tried**: everything the author knew how to do, and none of it could have found this.

- **Mutation testing** was real and passed. It planted a defect and asserted the check *failed* — which is true in all four positions. It never asserted **which file was named**, and a mutant planted in the last file reads correctly by accident.
- **A cross-implementation check** re-ran the algorithm in another language over the same corpus and matched exactly: same verdict, same line number. It ran **one file at a time**. Both bugs live at the *boundary between* files, so this apparatus was structurally incapable of registering either.
- **The gate's own self-test corpus** exercised every other check and did not exercise this one at all.

**Root cause**: two file-boundary state bugs — a filename read at *report* time rather than captured at *detection* time, and per-file parse state never reset when crossing into the next file. Both are invisible on a single input. But the mechanism worth keeping is what let them survive three separate verifications:

**Every one of those verifications shared an assumption with the code**: that a run is one file. The mutation test planted one defect at a time. The cross-check ran one file. The corpus had no entry. When apparatus and implementation share a blind spot, agreement between them is not evidence — it is the blind spot reporting itself twice, and a second measurement that inherits the first one's frame adds confidence without adding information.

The trigger was mundane and universal: a file whose final paragraph is not blank-terminated, which is every prose document ever written.

**Resolution**: three changes, of which only the first is the bug fix.

1. Capture the identifying context **when the observation is made**, not when it is reported — the general form of the filename bug, and it recurs wherever a loop variable is read after the loop moves on.
2. **Exercise every check in the self-test corpus, and make the corpus multi-file.** A gate not covered by its own proof-of-firing is unproven no matter how carefully the surrounding script was tested — and single-input corpora cannot express whole classes of defect. Assert *which* file and *which* line, never merely that something matched: a boolean corpus cannot see an answer that is right about the fact and wrong about the source.
3. **Define the program once** so the self-test exercises the program the gate actually runs. A self-test asserting against its own copy proves the copy.

**Lesson**: **ask what your verification apparatus cannot express, and check that the defect class you fear is not in that set.** A test that runs green tells you about the inputs it ranged over and nothing else; a cross-check in another language tells you the two implementations agree *on the shape of input you gave both*. Neither is worth what its confidence suggests when both inherit the same frame.

The corollary is a prediction discipline, learned by getting it backwards. This check's author predicted that a divergence between two ports would "far more likely" be the never-executed one than a real finding. Measured, it was the exact reverse: the unexecuted port was correct in every case and the mutation-tested one was not. **Testing effort is not evidence of correctness — it is evidence about the space that was tested**, and an untested implementation can be right for the same reason a heavily tested one can be wrong. Do not let a confidence ordering stand in for a measurement, especially when it flatters the artifact you spent the most time on.

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

> *Non-core (filesystem permissions + cross-platform security model, not GTK) — do NOT fold into the gtk4-rs skill. Proposed by the Windows seat, QA round 5; numbered and merged with its measured Linux half here.*

**Symptom**: TDD 21.12 ("a crash report is readable only by the user who ran the application") was satisfied by `private_options()` — an `OpenOptions` constructor applying `0600`, created precisely so a mode would not have to be remembered at N call sites. It was believed to hold everywhere. It held on neither platform completely.

**MEASURED, Windows 10 Pro 19045** (Windows seat, real hardware): under the default `%LOCALAPPDATA%\scribobulate` the property HOLDS — the profile root's DACL is `D:P` (protected), so the only inheritable ACEs are SYSTEM, Administrators and the owning user, and a real crash report comes out with every ACE *inherited*, i.e. the application contributes nothing. But `XDG_STATE_HOME` is checked first on **every** platform, so pointing it at an ordinary second NTFS volume yields `Authenticated Users:(I)(M)`, `BUILTIN\Users:(I)(RX)` — every local user can read the report and every authenticated user can modify it.

**MEASURED, Linux, on the DEFAULT path** (this seat): the same rubric was violated without any unusual configuration.

```text
drwxrwxr-x  ~/.local/state/scribobulate     ← 0777 & ~umask(0002) = 0775
-rw-rw-r--  session.toml                    ← records the path of every open document
```

The crash reports really were `0600`. That is what hid it: the per-file seam was correct, and **a private file inside a traversable directory still advertises its own name** — and these names encode a timestamp and a pid.

**What is NOT the fix, measured**: adding a `#[cfg(windows)]` arm to `private_options()` is the first thing anyone reaches for and it is a dead end. Rust's `OpenOptionsExt` exposes `access_mode`, `share_mode`, `custom_flags`, `attributes`, `security_qos_flags` and **no** `SECURITY_ATTRIBUTES` hook; `CreateFileW` with `lpSecurityAttributes = NULL` composes the DACL purely from the parent directory's inheritable ACEs, and Windows has no umask analogue. Per-file privacy is not expressible there at all.

**The fix is the DIRECTORY**, and it is the same seam on both platforms — POSIX expresses inheritance as traversal, Windows as inheritable ACEs, but the object is the same one. `session::create_state_dir` is now the only way the state directory comes into existence (three `create_dir_all` call sites routed through it): `0700` on unix at creation, plus a tighten for installations that already ran; a `CreateDirectoryW` + `SECURITY_ATTRIBUTES` protected DACL on Windows (preferred over `SetNamedSecurityInfoW` after the fact, which has a TOCTOU window). Doing it here also covers `session.toml` and `crash-last-seen`, **neither of which went through the per-file seam at all** — and it is the only placement that does not have to special-case `atomic_io::write_atomic`, which deliberately *relaxes* a new file's mode because the same function saves the user's own documents.

**The lesson worth carrying** (the Windows seat's wording, which cannot be improved): the generalisable point is not the `cfg` — that is #217's — but that **`private_options()` names itself for a guarantee it only delivers on one platform.** Its doc read "owner-only (0600) on unix, platform default elsewhere", which parses as graceful degradation and is in fact an unimplemented half. *A seam that is honest about being a no-op invites a reviewer to check whether the no-op is acceptable; one named `private_options` invites them to assume it is handled.* Corollary: a platform-conditional permission model needs a platform-conditional **rubric**, or the gate deletes the requirement — and check the container before congratulating yourself on the contents.

**Sub-lesson, from repairing it** (mutation-testing the fix): the first version tightened the directory unconditionally after creating it, which made the *creation* mode dead weight — `.mode(0o700)` → `.mode(0o755)` left the guard green, because the following chmod corrected either one. Two mechanisms where one carries the property is a mechanism nothing tests. Tightening only when migrating makes both load-bearing and both mutants fatal.

## 230. A clippy method ban does not cover the builder property of the same name

> *Non-core (Rust tooling / enforcement discipline, with a gtk-rs builder-idiom trigger) — do NOT fold into the gtk4-rs skill as a GTK lesson. The transferable half is about what a `disallowed-methods` matcher can and cannot see; the GTK part is only the idiom that makes the gap likely.*

**Symptom**: `WidgetExt::set_tooltip_text` was banned in `clippy.toml` so that every control had to be routed through `src/a11y.rs`, which sets the accessible name and the tooltip together. `cargo clippy --all-targets -- -D warnings` passed. Three toolbar controls nonetheless carried a tooltip and **no accessible name** — the exact defect the ban existed to make impossible — because they were built as `gtk::MenuButton::builder().tooltip_text("Documents")…build()`.

**What was tried**: nothing failed, which is the point. The ban was written, verified to fire (the four in-module calls needed `#[allow]`), and the whole tree converted. `-D warnings` was green on a tree that still contained the defect. The gap was found only when a separate tree-walk integration test — added for coverage, not because the ban was doubted — walked a live window and reported three unnamed controls.

**Root cause**: `disallowed-methods` matches a **path**. `gtk4::prelude::WidgetExt::set_tooltip_text` and `gtk4::builders::MenuButtonBuilder::tooltip_text` are different paths to the same effect, and the builder form is generated per widget type, so a complete ban would need one entry per builder in the crate rather than one entry per contract. The builder idiom is also *more* likely exactly where the risk is highest: a widget that needs three or more properties at construction is the one a developer reaches for `builder()` on.

**Resolution**: treat the ban and a runtime assertion as **one mechanism with two halves**, not as a primary gate plus a nice-to-have.

- The ban stops the common spelling at write time, where the feedback is cheapest.
- A live assertion over the **built object** — not over the source text — closes every spelling at once, because it tests the effect rather than the call. Here: walk a real window's widget tree and assert every icon-only control and label-less field has an accessible name (`gtk_test_accessible_has_property`, present at the 4.6 floor). It is the half that actually found the defect.

The general form: **an enforcement tool's coverage is a property of its matcher, not of its intent.** Before trusting a ban to make a contract unforgeable, ask what other spellings reach the same effect — a builder, a `.set_property("tooltip-text", …)` string, a UI-file attribute — and put the assertion where they converge. Kin to #219 (the enforcement ladder: this is why the top rung is not the last word).
## 231. Retiring an ambiguous citation form by LEGALISING it instead of banning it — and a completeness claim with no predicate

> *Non-core (documentation / register hygiene / lint tooling) — **do not fold into the gtk4-rs skill**; there is no GTK content here. Sibling of #145 (which renamed this register) and #143 (a citation that resolves to the wrong thing rather than to nothing). #145 is the collision; this is the *repair of the repair*.*

**Symptom**: the convention paragraph said the legacy citation forms "were swept out of `src/`, `tests/`, and this file's cross-register citations, so the convention now holds tree-wide." Nothing contradicted it — every gate was green, and `ScrAP-N` was in use everywhere one looked. **443 bare `AP-N` citations were in the tree at that moment** (310 in `src/`, 127 in `sdd/`, 6 in `tests/`), plus one in `data/` that no one had thought to look in.

**Root cause, and it is not the sweep's thoroughness**: #145 removed the collision by giving *this* register a unique prefix, then **legalised the abandoned form** — "a bare `AP-N` now means the skill." That is the one form that cannot be told apart from a citation the sweep missed, because it was simultaneously the *current* skill spelling and the *historical* project spelling: the same string is correct or wrong depending on the day it was typed, and nothing in the text says which. **A convention whose correct and incorrect uses are textually identical is not a convention** — it is an unfalsifiable claim about author intent.

Worse, the sweep's mechanism was a prefix rewrite, and **a citation's correctness is not a property of its syntax**. Rewriting a bare **79** → `ScrAP-79` preserves the number while silently changing which register it names, so a purely textual pass looks complete while every number it touched still needs a human to decide what it meant. `src/widgets/tab/bar.rs` is the proof: a `ScrAP-79` sat on a helper whose comment describes the *pump-loop* lesson (GTK4Rs/AP-79, and #88 here), among three **correct** `ScrAP-79` citations about a tab-close gesture — so the wrong one hid among right ones and read as consistent.

**What was tried, and why each was insufficient**:
- **The prefix sweep itself.** It fixed the *project* form's ambiguity and introduced the mis-resolution above. Do not re-run anything of that shape.
- **Reference-lint checks 2 and 3.** Both pass over a citation that names a real entry about the wrong subject — they ask whether a cited `ScrAP-N` is *defined*, which is a different question from whether it is *right*. This is the gap that let `bar.rs` through, and no check can close it.
- **The plan's own measurement, which was wrong in BOTH directions.** Its predicate `git grep -ohE '(^\|[^a-zA-Z-])AP-[0-9]+'` counted the *legal* `skill AP-N` citations as violations (337/261/7 reported against a true 310/127/6) **and** scoped itself to three directories, missing `data/resources.gresource.xml`. A count that over-reports and under-scopes at once still reads as authoritative because it comes with a command — the command is what invites the reader to check, so it must be the *right* command.

**Resolution** — make the ambiguous form illegal, gate on it, migrate per site:

1. **`ScrAP-N` for an entry here, `GTK4Rs/AP-N` for a skill entry, and a bare `AP-N` is ILLEGAL.** Not "means the skill" — illegal. This is the only option that is *checkable*: nothing on this machine can confirm what the skill's entry N says (the skill may not be installed), but "no bare `AP-N` exists" is a grep, on any machine, with no network.
2. **Both legal forms are SINGLE TOKENS, and that is load-bearing.** The first spelling of this fix was `skill AP-N`, with a space — and two citations in the tree were *already* split across a Markdown/`rustfmt` wrap (`sdd/TECH.md`'s `…gtk4-rs skill\nAP-147`, `overlay.rs`'s `…the exact skill\nAP-78`). That is not a lint inconvenience: the whole justification for legalising an unverifiable citation is that it becomes **enumerable**, and `grep 'skill AP-'` silently misses every wrapped one, so the human-audit set was incomplete while looking exhaustive. Nothing wraps inside a whitespace-free token. It also makes the check enforce the *spelling* — `/` is not in `[a-zA-Z-]`, so a *misspelled* prefix is still reported, where the two-word form's `skilll …` used to read as ordinary prose.
3. **`lint-references` check 8**, both ports, mutation-tested by planting a bare citation in a scanned file (a clean tree passes this check whether or not the pattern can match — #206). The rule is a lookbehind, which POSIX ERE lacks and `grep -P` cannot supply on the BSD grep macOS ships, so **both** ports do it in two stages — find candidates, delete the legal form, re-match the remainder — which also yields the right answer for a line carrying one of each.
4. **Migrate per site, and re-derive every number.** For each: read the surrounding comment, decide which lesson it describes, then confirm that lesson's number **in the register named**. Where a lesson lives in both, prefer `ScrAP-N` — this register is always resolvable.

**The one distinction that makes a mass edit safe rather than catastrophic**: a bulk token replace is legitimate **only for numbers where both registers hold the same lesson** (they were seeded together, so 1–41 largely agree). There the citation is meaning-preserving whichever register the author meant, and 148 sites moved that way safely. From 45 up the registers diverge and every site had to be read — and that is where the collision was *live in this tree*, not hypothetical:
- **The same lesson was cited under both numbers, 79 and 88,** in `src/` (the pump-loop bound) — the two registers hold each other's entry at those numbers, so the tree contained the inversion in both directions at once.
- **The same number named two different lessons in one tree**: a bare **136** was the skill's `WrapMode::WordChar` AT-SPI abort in `preview/render.rs` and *this* register's session-snapshot seeding in `winstate/mod.rs`.
- Nine further **bare** citations resolved to a real-but-wrong entry and were re-derived (bare number → target): **56**→GTK4Rs/AP-56 (Xvfb ≠ a real compositor; #56 *here* is a `size_allocate` lesson), **125**→#169, **48**→#39, **47**→#38, **61**→#53, **74**→#55, **127**→#150, and the pair forms **82**/#82 and **102**/#102 whose *skill* half was wrong; plus `#154`'s See-line, which named three skill numbers and got **all three** wrong.

**And the gate's own SET was incomplete, which is how this nearly shipped half-done.** With every scanned file clean and check 8 reporting PASS, a grep over the **whole tracked tree** still found five bare citations — in `.cargo/config.toml` and `clippy.toml`, neither of which was in `lint-references.scan`. Both are exactly where a lesson gets cited (the process-wide `[env]` choke point and the method-ban list), and both were invisible. This is #207's rule arriving from the direction nobody checks: a gate is its pattern **and** its set, and a clean report from an incomplete set is indistinguishable from a clean tree. So the last step of adopting a form gate is not "the gate passes" — it is **the same predicate run without the gate**, over `git ls-files`, and the two answers compared. They disagreed. (Fixed by naming both files in the contract, which obliges a fresh `--list-scan`/`-ListScan` parity diff — POLICY step 9.)

**Two smaller traps, both cheap and both worth the sentence**:
- **A citation-shaped literal in TEST DATA trips the gate on a line that means nothing.** `renderer/mod.rs` used a citation-shaped literal (`AP-` plus digits) twice over as arbitrary repeated text in a #115 fixture — renamed to a neutral token of the same length so the documented char offsets still hold. Test data is prose to every grep in the tree; do not spell it like an identifier the tree gates on.
- **`/` in the new token collided with `sed`'s `s///` delimiter** in the bash port's strip step. Note *how* it failed: `sed` wrote to stderr, the substitution silently did nothing, and the predicate then reported every legal citation as bare — loud, and therefore survivable. The same collision on the other side of a filter would have under-reported in silence. Use `s|…|…|`.

**Lesson**: **when you retire an ambiguous notation, ban it — do not redefine it.** Redefining leaves the ambiguity intact and moves it into the author's head, where no tool can reach it, and every later reader inherits a string they cannot resolve. And **a completeness claim about a mass edit is worth exactly the predicate it ships with**: "the sweep is done" is not a finding, `git grep -c <pattern> -- <every root the gate scans>` is — state it, scope it to the same set the gate uses, and expect the number rather than the prose to be the thing that is checked.

**The part with no completion criterion, stated because a plan that omits it reads as finished when it has merely been exhausted**: bare citations were only half the exposure, and the other half is the half no check can see — an *already-prefixed* `ScrAP-N` pointing at a real entry about the wrong subject. One such defect was known when this work started; **re-reading only the numbers where the two registers collide found seven more**, every one of them written by the prefix sweep itself (rounds 3 and 5): `session.rs` and `forensics/mod.rs` both cited `ScrAP-108/130` for the *enforcement-ladder* rule, where #108 here is an undo-barrier lesson and #130 an SVG one; `bar.rs` cited #78 (pulldown-cmark options) for mutation-testing a guard (#87); `actions.rs` cited #108 for the choke-point rule and #117 (a `GtkLabel` repaint lesson) for a headless-popover limitation (#101); `persistent_popover.rs` cited #117 for "reuse, never destroy per use" (#112); and `single_instance.rs` cited #122 (range translation) for the wall-clock-vs-frame-count shape (#134). Every one of them read as diligence. **An eighth turned up later, inside this register itself** (2026-08-01, found in passing while drafting an unrelated plan): #219's own header cited "#130's clippy ban and module-privacy seal" for the enforcement ladder — the same #108/#130 mis-resolution as `session.rs` and `forensics/mod.rs`, in the very entry whose subject is that ladder. Two lessons in that: the residue is **not confined to `src/`**, so a future pass must sweep `sdd/` too; and a bare `#N` inside this file is invisible to check 8 by design (it is the register's own legal shorthand), which makes prose here the *least*-guarded surface of all. **No tool will ever report that this audit is done** — 79/88 and the numbers listed above are now clean, and the rest of the colliding range is not proven. Priority if it is resumed: numbers where the two registers hold each other's lesson, because a mis-resolution there is not merely wrong but exactly inverted. The one structural tell, worth more than the list: **a citation whose neighbouring prose describes a lesson from a different DOMAIN than the entry's title** (a Rust-tooling claim citing a `GtkLabel` entry) is almost always a prefix sweep's residue, and domain mismatch is far faster to scan for than subject mismatch.

**Scribobulate**: `scripts/lint-references.{sh,ps1}` check 8 (the per-site migration rule and the audit's limits are documented at the check, next to the gate that enforces the form); the citation-convention paragraphs at the top of this file and at "Numbering reconciliation"; POLICY step 9. Retires the citations-registration plan (its analysis lives on in git history).

---

## 232. `g_file_replace_contents` is atomic only under the right flags — and its one remaining fallback deletes the previous file before failing

> *Core GIO (file I/O contract), not GTK widgetry — but it sits squarely in the gtk-rs application surface and belongs in the `gtk4-rs` skill's threading/async module. **Relay to the skill maintainer DELIVERED 2026-08-01** — to **`gtk` in the `scribobulate` room**, which is where that role now sits; two earlier attempts at `gtk4skiller` in the `skills` room failed because no process held that seat, so **address skill relays to `gtk`@`scribobulate`, not `gtk4skiller`@`skills`**. The relay carries this entry's corrected route-C matrix, the disk-full mechanism, the cancelled-close mitigation and the sampling-probe caveat, plus #234; #233 was explicitly flagged as NON-core so the skill does not absorb it. **Source-traced against GLib 2.72.4 (the reference host's exact version) and diffed to `main` 2.89.3.** Partly MEASURED since: the create-vs-replace atomicity boundary was probed on the reference host (see the measured amendment below). The one data-loss-shaped claim (route C) is out with the researcher.*

**Symptom**: none yet — this is a trap found at **design time**, before a line of it was written, which is the only reason it is cheap. The design under review was a crash-recovery swap file: a dirty editor buffer snapshotted periodically to the state directory, whose entire purpose is that the copy on disk is trustworthy after an unclean exit. `g_file_replace_contents_async` looks made for it: one call, "replace", asynchronous, and a `G_FILE_CREATE_PRIVATE` flag that appears to answer the permissions question. Adopting it on that reading would have produced a mechanism that is *usually* atomic, *sometimes* silently truncating, *never* durable across power loss, and — in one failure mode — actively destructive of the very snapshot it exists to preserve.

**⛔ SUPERSEDED IN PART — MEASURED 2026-08-01. The route-C analysis above is wrong in
both directions, and the real data-loss mechanism is somewhere neither the source-trace
nor the narrowing was looking. Read this block before acting on anything above it.**

Route C **cannot fail for the reason that sent it there**, so it is not the hazard. One
line both readings passed over: `glocalfileoutputstream.c:1194` does `g_close (fd, NULL)`
*before* the unlink, handing back the very descriptor `g_mkstemp_full` was denied. Measured
matrix (128 KiB payload over a 33-byte known-content file, `REPLACE_DESTINATION|PRIVATE`,
GLib 2.72.4):

| Stressor | mkstemp | Path | Previous file |
|---|---|---|---|
| dir `chmod 500` | EACCES | route C | ✅ INTACT (the narrowing got this half right) |
| EMFILE, 1 fd free | EMFILE | route C | REPLACED — the write *succeeded* |
| tmpfs inodes exhausted | ENOSPC | route C | REPLACED — the write *succeeded* |
| **tmpfs blocks exhausted** | ok | **atomic** | 🔴 **DESTROYED → 0 bytes** |
| same, `_async` | ok | atomic | 🔴 **DESTROYED → 0 bytes** |

🔴 **The actual hazard is an ordinary disk-full during the write, and it needs no exotic
precondition at all.** `gfile.c:7768-7776` closes the stream on a write error and
**ignores the close result** (`/* Ignore errors on close */`) — and for a local file
**close is where the temp→destination rename happens** (`glocalfileoutputstream.c:418-421`).
Nothing in the close path knows a write ever failed, so it promotes the truncated temp over
the previous good file and *then* returns a correct-looking `GError`.
`REPLACE_DESTINATION` is irrelevant to it, the async path carries the same defect
(`gfile.c:7823-7868`), and upstream `main` is unchanged — not a jammy artifact.

**The `GError` cannot tell you which happened.** Everything is `g-io-error-quark`; the only
discriminator is a *translated message prefix* (`Error writing to file:` = already
destroyed; `Error removing old file:` = intact), which no correct program can match on. So
**treat every `replace_contents` failure as "the previous snapshot may be gone"** — or
remove the condition, below.

**Mitigation — and there are two, of unequal quality.** The one Scribobulate ships is to
**own the promote decision**: write to a co-located `<name>.tmp` and `rename` it into
place *only after a complete successful write*. This looks like it is merely re-doing what
GIO already does, and that is the point — GIO's temp-and-rename is fine; what is broken is
*who decides to promote it and on what information*. Owning the rename rests on `rename(2)`
alone, so it needs no claim about GLib internals and no per-platform verification.

The alternative, kept below because it is measured and because it explains the shape of the
tests, is to steer GIO's own close down its discard branch: drive the stream
yourself — `g_file_replace()` + `write_all[_async]` — and **on a write error close with an
already-cancelled `GCancellable`**. That trips the guard at `glocalfileoutputstream.c:415`
→ `err_out`, which **unlinks the temp instead of renaming it** (`:461-462`). It is the
stream's own documented contract, stable 2.72 → `main`. **But it leans on an internal
branch**, which turns "does this hold on Win32/APFS?" into a question the design must
answer before it ships anywhere else — and that question is why the owned-rename above is
preferred. If you do take this route, pin it with a regression test so a moved branch
fails loudly rather than silently resuming data loss.

**The transferable form:** when a library's convenience call has the right *mechanism* but
makes the decision on information it has already discarded, do not look for a flag or a
cleverer invocation — **take the decision back**. Re-implementing the mechanism yourself is
often a few lines, and it converts a dependency on someone's internal control flow into a
dependency on a syscall contract.

⚠️ **A CORRECTION TO THIS ENTRY'S OWN EARLIER MEASUREMENT, because it reads as reassurance
and is not.** An earlier amendment here reported: *"a first write to an absent destination
streams in directly (0 bytes → full), while an overwrite never dips below the previous
content's length — so replacing is atomic."* The first half is true and still useful (it
separates create from replace, and makes a cheap rig). **The conclusion drawn from the
second half is false.** Size-sampling watches the *streaming* phase; the destruction
happens at **close**, after the sampling window ends — so the rig reports "safe" right up
to the instant it isn't. Both the implementing agent and the researcher drew the same wrong
inference from it. Generalised: **a sampling probe establishes a property only over the
interval it samples, and "nothing bad happened while I was watching" is not "nothing bad
happens"** — when the dangerous operation is a *state transition at the end* (close, commit,
rename, flush), the probe must straddle it or it is measuring the safe part.

**What survives and what does not.** The headline lesson stands and is sharpened: *a GIO
convenience call's guarantees are a function of its flags AND its fallback paths.* The
worked example does not — `REPLACE_DESTINATION` bought nothing against the failure that
actually loses data, because the hazard was never in the branch either reading was
examining. And the corollary "a flag can make a failure mode worse" turns out to be
unproven here: route C recovers. Kept as a caution, not a finding.

*Diagnostic lesson, and it is the same one GTK4Rs/AP-165 taught on the same day: **both**
analyses traced a branch and stopped at the operand that confirmed them.* One asked whether
`unlink` could succeed but never whether the *reopen* could fail; the other named the wrong
trigger entirely. Read *around* the line that confirms you.

**Two later measurements from the researcher, both sharpening this entry:**

- **The destructive rename reproduces on real ext4**, not just tmpfs — staged with
  `RLIMIT_FSIZE` (mkstemp succeeds, the write fails `EFBIG`, the atomic path is taken and
  the partial temp is renamed over the good file). So it is not a tmpfs accounting
  artefact. The `GError` there is `code=0` (`G_IO_ERROR_FAILED`), further confirmation
  that the code carries no survival information.
- 🔴 **On the async path the damage lands AFTER your error callback runs.**
  `gfile.c:7838-7845` returns the error to the main context *first*, then calls
  `close_async` — so the rename happens on a GTask worker thread afterwards. A probe that
  asserts immediately in the error callback reports a false INTACT (the researcher's first
  ext4 async run did exactly that, and chasing it rather than shipping it is what found
  this). **Consequence for application code: an error handler that reacts by re-writing
  the previous snapshot from memory is racing that close thread and can be silently
  overwritten by it** — intermittent, timing-dependent, and clean on a fast filesystem.
  Do not attempt in-callback recovery on this path. Owning the rename moots it entirely:
  no rename is ever scheduled, so there is no thread to race.

*(Route-C matrix, ENOSPC and mitigation all MEASURED by the researcher on GLib 2.72.4;
rig `routec.c` + `enospc.sh`. Honest limits: ENOSPC staged on tmpfs in a user namespace —
block accounting differs from ext4/btrfs though the GIO control flow does not; EDQUOT,
network filesystems and Windows untested; absence of a data-losing route-C case is not
proof one is impossible.)*

**Root cause**: the guarantees are properties of the **flags plus the fallback paths**, not of the function. `g_local_file_output_stream_replace` takes the temp-file-then-rename path only when the gate at `glocalfileoutputstream.c:1030-1041` opens; otherwise it `goto fallback_strategy` (`:1084`) and reaches `ftruncate (fd, 0)` at `:1230`. Four routes get there:

| | Route | Closed by `REPLACE_DESTINATION`? |
|---|---|---|
| A | target is a **symlink** | ✅ |
| B | target is **hard-linked** (`st_nlink > 1`) | ✅ |
| C | **`g_mkstemp_full` fails** — unwritable dir, ENOSPC, read-only mount (`:1046`) | ❌ |
| D | `fchown`/`fchmod` on the temp fails and the re-`fstat` still differs (`:1050-1082`) | ✅ (whole block is `if (!replace_destination_set && …)`) |

Route C is the one that matters, and the flag makes it *worse rather than better*: with `REPLACE_DESTINATION` set the fallback runs `g_close(fd)` → **`g_unlink(filename)`** → `g_open(...)` (`:1192-1211`), so the previous good file is deleted before the write that was going to replace it fails. A torn file is detectable (a truncated header, a missing terminator); a deleted file is indistinguishable from "there was never anything to recover", which is precisely the reading that makes a user's work vanish quietly.

⚠ **NARROWED 2026-08-01, before any probe — and the narrowing matters more than the original claim.** The first phrasing of this entry said an *unwritable directory* triggers the destruction. That is very probably **wrong**, and wrong in the direction that would have sent the mitigation to the wrong place: **`g_unlink()` needs write permission on the containing directory — the same permission `g_mkstemp()` was just denied** — so under a permissions failure the unlink should fail too and the old file should survive. The destructive window is therefore not "unwritable directory" but **any failure that stops `mkstemp` without also stopping `unlink`**: ENOSPC, EDQUOT, EMFILE/ENFILE. Both halves are pending measurement (a `chmod 500` case and an fd-exhaustion case). The practical consequence is the part to carry: **a startup writability check does not defend against this at all**, because the conditions that actually reach the destructive interleaving are free-space and fd exhaustion, neither of which a permissions probe can see.

Two further contract gaps in the same call:

- **Durability is partial and no flag adds to it.** The temp is fsynced before the rename **only when the destination already existed** (`sync_on_close` is set solely on the EEXIST branch, `:1305`; the fsync itself is `:328`), and the **parent directory is never fsynced** — `grep -n fsync glocalfileoutputstream.c` returns exactly one hit, on 2.72.4 and on `main` alike. So the first write of a session is unsynced, and a power loss can lose the newest rename. (A SIGSEGV, an OOM kill or `kill -9` cannot: the kernel survives, the page cache is intact, the rename is visible. Which failure you are defending against decides whether this matters.)
- **`PRIVATE` is silently undone on overwrite without `REPLACE_DESTINATION`.** The mode is genuinely applied at `open(2)` — `PRIVATE` → 0600 → `g_mkstemp_full` → `g_open(filename, flags, mode)` (`gfileutils.c:1649-1656`), so there is no world-readable window on the predictably-named temp sibling. But `:1048-1058` then fchmods the temp **back to the original file's mode** unless `REPLACE_DESTINATION` is set, so a file that ever acquired a laxer mode keeps it forever. With the flag, the temp's mode wins and the privacy is self-healing.

**What was tried, and why the reflexive checks would not have caught it**:

- **Reading the documentation.** It describes replacement and offers the flags; it does not enumerate the truncate-in-place fallbacks, and nothing in the API surface distinguishes "atomic" from "atomic unless".
- **Reasoning from the name.** `replace` reads as a whole-file substitution. It is — right up to the point where creating the temp fails, which is exactly the low-disk / bad-permissions condition under which a recovery mechanism is most likely to be exercised.
- **Trusting `PRIVATE` to mean private.** It does, at creation. It is the *later* fchmod, in a different branch, that undoes it — the same shape as this register's #229: a guarantee named on the seam, delivered conditionally underneath.
- **Assuming "async" might be cosmetic.** Here the pessimism was unwarranted, and that too is worth recording rather than re-litigating: it is genuinely thread-pooled (`g_file_real_replace_async` → `g_task_run_in_thread`; via-threads because `GLocalFileOutputStream` neither overrides `write_async` nor implements `GPollableOutputStream`, so `g_output_stream_async_write_is_via_threads` is TRUE), and the completion callback returns on the thread-default main context, so it is GTK-safe. gio-rs 0.21 *enforces* both halves — it asserts main-context ownership and `ThreadGuard`s the callback — rather than merely permitting them.

**Resolution**: `FileCreateFlags::REPLACE_DESTINATION | FileCreateFlags::PRIVATE`, always both, and then design *around* the two residual costs instead of assuming them away.

```rust
file.replace_contents_async(
    payload,                       // Vec<u8>: moved, NOT copied, and handed back in the
                                   // callback — recycle the allocation across snapshots.
                                   // (replace_contents_bytes_async is an un-bound TODO in 0.21.5.)
    None,                          // etag: sole writer; WRONG_ETAG is a needless failure mode
    false,                         // make_backup: an extra rename and a ~sibling, for no benefit
    FileCreateFlags::REPLACE_DESTINATION | FileCreateFlags::PRIVATE,
    Some(&cancellable),
    move |res| { /* main context; GTK-safe. Clear the in-flight gate HERE. */ },
);
```

- **Surface a write failure to the user**, do not log it quietly: under route C that failure may already have cost them the previous file, and it is the one moment they need to know the safety net is off. Note this is the *only* mitigation that covers the real trigger conditions — a startup permissions/writability check does not (see the narrowing above), because ENOSPC and fd exhaustion are runtime states, not startup ones.
- **Serialise your own writes.** GIO does not order concurrent replaces of the same file; two in flight can land out of order and silently resurrect older content. Gate on an in-flight flag per target and let the completion callback release it and fire whatever coalesced meanwhile.
- **If power-loss durability is a requirement, this call cannot deliver it at any flag setting** — that is the point at which a hand-rolled `write_atomic` (fsync the temp, rename, fsync the parent) on a worker thread becomes the only answer.

**Lesson**: **a GIO convenience call's guarantees are a function of its flags, and the fallback path is where the flag you set changes what a failure costs you.** Enumerate the fallbacks *before* adopting one — the question is never "does this function do X?" but "what does it do on each path where it cannot do X, and which of those paths does my flag combination leave open?". Two corollaries. First, **a flag can make a failure mode worse**: `REPLACE_DESTINATION` closes three truncation routes and converts the fourth from a torn file into a deleted one, so "set the safety flag" is not a strictly monotone improvement and cannot be reasoned about without reading the branch. Second, this is the same asymmetry the skill's standing rule already names — a source-read is a prediction — but pointed at the *positive* claims for once: the reflex was to distrust `async` (which turned out honest) and to trust `replace` (which turned out conditional). Distrust is not a substitute for reading; it just relocates the guess.

**Scribobulate**: `src/window/swap.rs` owns the promote — co-located `<name>.swap.tmp` opened with `replace_async` (`PRIVATE`), renamed into place only after a complete write; three tests pin it, one characterising the cancelled-close GLib branch. Format: SCHEMA.md § "Crash-recovery swap file". Contract: TDD §22.
**See**: researcher findings — `~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-replace-contents-atomicity-durability-threading.md` (GLib 2.72.4, diffed to 2.89.3). Kin: #229 (a seam named for a guarantee it delivers conditionally).

---

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

**What was tried**:
- Wrote integration tests over the recovery path asserting the recovered text, the dirty
  flag, the header round-trip, the invariant in both directions, permissions, and the
  discard action. All passed. All of them read the same accessor.
- Reasoned about which code paths the recovery touched, and correctly identified the
  buffer write. The projection was never *considered*, so it was never *tested*, so its
  absence could not surface.
- The defect was found only by looking at a screenshot of the running application.

**Root cause**: the document had **two representations** — the live editor buffer, and a
separate stored string that every derived view (preview, outline, annotations list)
renders from. Every other content-changing path in the tree wrote both in the same breath;
the new path wrote one. The tests could not catch it because every assertion had been
written against the accessor for the half that worked.

The deeper cause is not the missing line, it is what the green suite was taken to mean.
**A test suite is evidence about the surfaces its assertions name, and nothing else.** A
suite that is large, fast and green feels like coverage of *the feature*, when it is
coverage of *the parts the author thought of* — and the parts an author forgets to
implement are, with unpleasant reliability, the same parts they forget to assert. The two
omissions are correlated because they have the same origin, which is exactly why a suite
cannot audit its own aim.

**Resolution**: write the projection in the same place as the buffer, and add a regression
test that asserts the **projection** — plus that the two agree, so they cannot drift apart
later. Mutation-tested. Three habits generalise:

1. **Enumerate a feature's representations before writing its first test.** Source and
   projection, model and view, buffer and cache, database row and search index. If state
   has more than one representation, an assertion on one is a claim about one.
2. **Prefer asserting the outermost representation**, the one the user actually
   experiences. An assertion on the inner one passes whenever the outer is broken; the
   reverse is not true.
3. **Treat a live run as non-optional for anything with a rendered surface.** It is the
   only check that cannot be aimed at the wrong half, because it does not take an
   assertion — you look at it.

**Lesson**: **"All tests pass" answers "did I break what I was watching?", never "does the
feature work."** When a change adds a code path, ask what the *other* representations of
that state are and assert one of them deliberately — and when a feature has a visible
surface, look at it once with your eyes before believing the suite. Kin to #87 (a harness
settling a precondition away and masking a real bug) and ScrAP-56 (a clean headless result
is not a live one); the Derived-view CAM is the same obligation moved to write time, where
it is cheaper.

---

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
keystroke path *and* the capture path silently address the wrong window. Capture the
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

## 237. A `cfg`-gated gate proves nothing about the branches it did not compile

> *Testing/CI process and cross-platform build discipline, not GTK. Non-core. Found by the
> Windows seat while landing an unrelated fix.*

**Symptom**: `cargo clippy --all-targets --features gtk-integration-tests -- -D warnings`
— build-pipeline step 2, mandatory, run on every change for the life of the project — had
**never passed on Windows**. Seven errors, all pre-existing, none introduced by the change
that finally surfaced them: an unnecessary `mut` used only by a `#[cfg(unix)]` arm, a
constant dead on Windows because every use is a POSIX mode call, and five `return`s that
become unneeded once the unix arm is compiled out.

**Root cause**: the gate's *pattern* was right and universally applied; its **coverage was
selected by `cfg`**. Code inside `#[cfg(windows)]` — and code whose *shape* changes when a
`#[cfg(unix)]` arm disappears — is not compiled on Linux at all, so no amount of running
the gate there can say anything about it. The canonical-platform convention that keeps the
project coherent is exactly what hid this: the platform that runs the gate most often
compiles the least platform-specific code, and the platform whose branches most need
linting runs it least.

The deeper trap is that **a green gate reads as evidence of absence**. Nothing reports
"7 checks not performed"; the run simply passes, and its passing is indistinguishable from
the branches being clean.

**Resolution**: fix each mechanically rather than with `#[allow]` — a `cfg` split so the
binding is only `mut` where it is mutated, `#[cfg(unix)]` on the constant, and the five
`return`s deleted — and, more importantly, **treat step 2 as a per-platform obligation
rather than a per-change one**. A gate whose input set is chosen by `cfg` has to be run on
each `cfg` that selects code, and the pipeline should say so rather than leaving it to be
inferred.

**Lesson**: **a gate's guarantee is scoped to what it compiled, and `cfg` silently shrinks
that scope without shrinking the gate's apparent authority.** When a check is described as
"run on every change", ask *on which platform, over which `cfg` branches* — and treat a
clean run on one platform as evidence about that platform only. Kin to #207 (a gate is its
pattern **and** the set it runs over) one dimension across: there the set was an
enumeration someone had to keep in sync, here it is selected by the compiler, which is
worse because there is no list to diff. Kin also to #234, which is the same failure of
scope aimed at test assertions rather than at build gates: in all three, a green signal was
read as a claim about more than it covered.

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

## 239. `git stash pop` restores the source, not the binary — a control run that silently drives the old build

> *Testing/verification tooling and process, not GTK. Non-core. Found while proving the
> fix for the timed-status-notice cross-window defect.*

**Symptom**: a fix verified green by unit tests, two mutation-tested integration tests
and one driven Xvfb scenario appeared to **fail** the moment a second scenario was added.
Worse, it failed *identically* on the pre-change binary and the fixed one — the notice
stayed on screen in both columns — which reads as "the fix does not work", and, since the
first scenario had already passed, as "the fix works for one path and not the other". Four
drive cycles went into hunting a defect in the shipped code, ending at an
`eprintln!`-instrumented build which proved the timer fired and the retraction ran.

**Root cause**: the control build had been produced with `tests/MANUAL-TEST.md` §1.7's
recipe — `git stash && cargo build --release`, copy the binary aside, `git stash pop` —
and then the harness invoked **`./target/release/scribobulate` directly**. `git stash pop`
restores working-tree *source*; it does not touch `target/`, and running a binary by path
never triggers a rebuild. So the "fixed" column was executing the pre-change build. Both
columns were the same program.

The trap is not the staleness itself but that **the two names for the same file diverged
without any signal**: the shell said `./target/release/scribobulate`, the source tree said
"fixed", and nothing in between was wrong enough to complain. `cargo run` would have
rebuilt it; the direct-path launch the drive loop uses will not. The earlier scenario had
passed only because it ran *before* the stash.

**Resolution**: name the control explicitly (`cp target/release/scribobulate /tmp/pre`)
and rebuild explicitly right after the pop, so the comparison is always between **two
distinctly-named binaries** rather than one path that means different things at different
moments. §1.7 now spells both out. The re-run then produced a clean 2×2 — pre/fixed ×
tab-moved/tab-closed — with the pre binary stranding the notice in both rows and the fixed
one clearing it in both.

**Lesson**: **the diagnostic tell is symmetry.** When the control and the treatment behave
*identically*, the first hypothesis should be that they *are* identical — a build, a path,
a cached artefact — not that the treatment is ineffective. A positive control's whole value
is that it differs from the treatment (#217); when it does not differ, it has stopped being
a control and is silently reporting on one binary twice. More generally, a
verification harness must own the identity of every artefact it compares, exactly as it
must own the identity of the process and window it drives (#132: *an identifier chosen by
convention is not one you own*) — here the ambiguous identifier was a build output path
rather than a PID, and it mis-attributed the result the same way. Kin to #223: a claim
("verified against the fixed binary") is only as good as the step nobody checked.

## 240. A detector that enumerates the VOCABULARY of a free-text citation is defeated by a synonym

> *Reference-gate tooling and review discipline, not GTK. Non-core. Found landing a
> platform-specific crash-recovery fix across two seats.*

**Symptom**: an illegal cross-file issue citation — the register's name, the word `item`,
and an entry letter, in a doc comment — sat in `src/` through a clean
`scripts/lint-references.sh` run. Check 1 exists for exactly that citation form and
reported PASS. Worse, the citation was **born dangling**: the very commit carrying it
rewrote the entry it named, and that entry disappears entirely once its last platform
lands. (This write-up names no entry letter for the same reason — and the gate flagged the
draft that did, which is the rule enforcing itself on its own documentation.)

**Root cause**: the pattern enumerated the connecting *noun*. `ISSUES_RX` read
`\bISSUES(\.md)?([ .:_-]*(entry )?[A-Z]\b|…)` — `entry` was the one word the pattern's
author happened to write, and a citation saying `item` walked straight through the gap.
`issue` and `letter` would have too. The check was never wrong about the *rule*; it was
wrong about how many ways a human writes it.

This is the script's own recorded failure recurring one check across. Check 6 once passed
over 21 live danglers because its pattern required a `.md` the citations did not carry
(#207), and check 1's first version shipped missing three real forms, found by the Windows
seat mutation-testing its own port. Three instances, one shape: **a pattern written from
the examples in front of the author, tested against those same examples.**

**Resolution**: match the *shape*, not the vocabulary — `([a-z]+ )?`, any lowercase
connector, in both ports plus three new must-match and two new must-not-match cases in the
shared self-test corpus. Discrimination still comes from `[A-Z]\b` and case-sensitivity, so
prose (`read ISSUES.md and TECH.md together`) stays clean. The live citation then made the
strongest possible mutation test: the gate went from PASS to naming the real line.

**The second-order finding, which is the more expensive one.** The review named *one*
instance. The fix removed that instance — and introduced **two more of the same rule**, in
a doc comment and in `MANUAL-TEST.md`. Both were catchable by the pattern *as it already
stood*; they survived because the fixing seat's "full pipeline" was fmt, clippy, and every
test target, and omitted build-pipeline steps 6 and 9 — the only two that can see a
reference defect. Every other gate passes identically whether the citations are right or
wrong, which POLICY says in as many words.

**Lesson**: **name the class, not the instance — and make a gate hold it, because a review
finding is discharged by whoever fixes the line, not the category.** Two corollaries worth
carrying separately: (1) when you write a detector for something humans phrase freely,
enumerate its *structure* and let the discriminating token do the work — an enumeration of
synonyms is a list that is always one entry short, and it fails silently, reporting PASS;
(2) a per-platform pipeline is a *subset* claim — a seat that runs "everything" minus the
steps unique to a defect class has verified everything except that class, and its green run
reads identically to a complete one (#237's shape, aimed at which steps ran rather than
which branches compiled). Kin #219: the remedy has to live where the next person cannot
avoid it, and a lint is further up that ladder than a review comment.

## 241. A process NAME is not an identity — pid reuse defeats every liveness probe, on every platform

> *Crash-recovery design limitation, not GTK. Non-core. Found by the Windows seat while
> closing the last arm of the liveness probe; the finding is cross-platform.*

**Symptom**: none yet — this is a limitation recorded before it bites, which is the only
useful time to record one. The crash-recovery scan skips a snapshot whose `owner_pid` is a
live instance of this app, so two instances never fight over each other's unsaved work.
"Live instance" is answered by reading the process's executable name: `/proc/<pid>/comm` on
Linux, `proc_pidpath` on macOS, `QueryFullProcessImageNameW` on Windows.

**Root cause**: a pid is a reusable token and a name is not an identity, so the conjunction
of the two is not one either. If the recorded pid has been recycled onto *another
Scribobulate*, all three probes answer **live** — correctly, about the wrong process — and
the scan skips a snapshot whose real owner is dead. That is the losing direction: the
user's unsaved work is silently not offered back.

**The finding that makes this worth an entry is that it is NOT platform-specific.** It
surfaced on Windows, which recycles pids from a small pool far more eagerly than Linux's
near-monotonic allocator, and the natural conclusion — "a Windows problem, mitigate it in
the Windows arm" — is wrong. Linux and macOS have the identical hole and merely reach it
less often. A defect whose *probability* is platform-dependent while its *mechanism* is
shared will be mis-scoped by whichever platform happens to hit it first, and a fix built
into that platform's arm leaves the other two silently exposed while looking complete.
(Kin #175, where the consequence was the platform-dependent half and the defect was shared.)

**Resolution — deliberately not implemented.** Closing it means recording the owner's
process **start time** beside `owner_pid` in `SwapHeader` and requiring both to match: a
recycled pid has a later start time, so the pair is an identity where the pid alone is not.
That is a schema change to the on-disk format, affecting all three platforms and every
existing snapshot, to close a hole nobody has been observed to hit. It is documented here
and left undone on purpose — the point of the entry is that the next person to reason about
liveness starts from "the probe is name-based and therefore identity-blind" rather than
rediscovering it, and that if the schema is being revised for another reason, this rides
along at almost no cost.

**Lesson**: **when a probe answers "is this the thing I remember?" by comparing a
reusable handle plus a non-unique attribute, it is answering a weaker question than it
appears to** — and the gap is invisible in testing, because reproducing it requires
winning a race against the OS's allocator. Ask what would have to be true for the answer to
be wrong, not whether the call succeeded. Sibling of the CAM's held-reference row: an
`owner_pid` in a file is a reference across the longest gap this application has, a process
restart, and every rule about re-resolving a held reference applies to it.

## 242. `clippy --all-targets` WITHOUT the feature flag reports dead-code errors in files you never touched

> *Tooling/process, not GTK. Non-core. Hit by the Windows seat; costs a cycle on any seat
> whose loop omits the flag.*

**Symptom**: `cargo clippy --all-targets -- -D warnings` fails with two dead-code errors —
`a11y::has_name` and `PreviewFindCache::builds` — in modules the current change never
touched. It reads as "your change broke two unrelated files", which is the most expensive
possible framing: it sends you to read code that is not the problem.

**Root cause**: both symbols are used *only* from `#[cfg(feature = "gtk-integration-tests")]`
code. Without the feature, the callers do not compile, the symbols are genuinely unreachable,
and clippy is correct. The invocation was wrong, not the tree. Build-pipeline step 2 already
says the flag "is not optional here" — but it says so in terms of what the flag *protects*
(the feature-gated suite rotting unnoticed, ScrAP-124), and never in terms of **what it looks
like when you forget**, which is the form the knowledge is needed in.

**Resolution**: run step 2 exactly as written, with `--features gtk-integration-tests`. The
diagnostic reflex when clippy indicts files outside your change: check the invocation before
the code. Confirming the named files are unmodified in your tree (`git status`) settles it in
one command and is the right first move.

**Lesson**: **a rule stated only as its rationale is not usable at the moment it is needed.**
"Always pass the flag, because otherwise the suite rots" is advice for the person composing a
pipeline; the person who needs it is staring at two errors in files they have never opened,
and for them the load-bearing sentence is "if you see dead-code errors in unrelated modules,
you forgot the flag." Document the failure's *appearance* alongside its cause — the symptom is
the index by which the lesson is actually looked up. Kin #237: both are a `cfg`-selected input
set producing a confident, wrong-looking result, there by hiding checks and here by inventing
findings.


## 243. GLib's I/O thread pool is one process-wide pool of ten — moving I/O off the main thread makes it contend with the crash-recovery writer

**Symptom.** Document reads and writes were moved off the GTK main thread with
`gio::spawn_blocking` (issue: the window froze for as long as the filesystem took
to answer). Nothing broke. What was not visible from the API is that the
crash-recovery snapshot writer's `replace_async` goes through the **same pool**, so
a slow or unresponsive filesystem now delays the mechanism that protects unsaved
work — the one thing that must not be delayed while the filesystem is misbehaving.

**Where Scribobulate implements the fix.** `src/docio/pool.rs` — `MAX_CONCURRENT = 4`
admitted operations, a FIFO of waiters, and a `Slot` released on `Drop`. Its module
doc carries the measurements; `sdd/TECH.md` § Concurrency model carries the
consequence. Unit-tested headlessly (the gate is a plain future over a
`thread_local`, so it needs no display); the tests caught a real double-release in
the hand-off path.

**Sourced facts** (researcher, GLib 2.72; findings doc
`~/Documents/Projects/AI/Research/Gtk4Rust/researcher-findings-gio-task-thread-pool-sharing-starvation.md`,
rig `_src/gio-taskpool-starvation/`):

- One file-static pool (`gtask.c:619`), `G_TASK_POOL_SIZE 10` (`:643`), created at
  `:2195`. `GLocalFile` overrides **no** async `GFile` vfuncs, so `replace_async`
  falls through to `g_file_real_replace_async` → `g_task_run_in_thread`;
  `GLocalFileOutputStream` implements no pollable interface, so
  `g_output_stream_async_write_is_via_threads()` is TRUE and `goutputstream.c:1225`
  dispatches to the same place. Both workloads, one queue.
- It grows, but slowly: `:629-646` adds **one** thread per compounding wait (100 ms
  base, ×1.03 per running task), from GLib's own worker thread (`:2205`) so growth
  survives a wedged main loop. The pool is **not** capped at 330 —
  `set_max_threads(tasks_running+1)` is unbounded; the constant only stops the
  *wait* compounding, at which point it is ≈21 minutes per additional thread.
- MEASURED completion time of a 64 KiB snapshot write with N tasks blocked forever
  on an empty pipe: **9 → 0.2 ms · 10 → 206.6 ms · 15 → 686 ms · 20 → 1.36 s ·
  30 → 3.05 s · 50 → 8.37 s.** The cliff is exactly the base pool size. It never
  fails to complete at any N.
- The real two-stage path (`replace_async` → `write_all_async`) does **not** double
  the penalty (15 → 567 ms, 20 → 1.23 s, 30 → 2.87 s): once the pool grows to admit
  stage 1, stage 2 finds capacity ~120-190 ms later. Recorded so nobody
  "optimises" the two-stage writer back into `replace_contents` chasing a latency
  win that is not there — and that function is the one that renames a truncated
  temp over the previous good file on a write error (#232).
- `io_priority` is not a lever: `:2199` sets a queue sort function that compares
  `blocking_other_task` first, and that flag is set only for tasks queued from
  *inside* a pool thread (`:1534`). Nothing dispatched from the main thread sets it.
- `g_task_start_task_thread` has exactly two exits and both are
  `g_thread_pool_push` (`:1516` early-cancel, `:1536` normal), with a `g_assert` on
  pool creation (`:2197`) and a non-exclusive pool that queues rather than blocking
  the caller. **The blocking func can never run inline on the calling thread**, so
  the freeze this whole change removes cannot silently return by that route.
- Completion order is **not** guaranteed to match dispatch order, and the explicit
  re-sort above makes that stronger than mere concurrency: a later-queued task can
  dispatch earlier at equal priority. A per-document in-flight gate is therefore
  **required**, not belt-and-braces (see #244 and `winstate::WriteGate`).

**The rule.** Moving work off the main thread does not make it free — it makes it
**contend**, and what it contends with is whatever else the runtime put in the same
pool, which is not visible from the API you called. Before dispatching a new class
of work to a shared pool, ask what is *already* in it and what that thing's latency
budget is; then bound your own use at the source, because there is nothing to tune
downstream.

**Not taken, and why it is recorded.** The researcher's third option — give the
snapshot writer a dedicated thread of its own, which is immune to every mechanism
above — is the only fix that holds when document I/O hangs unboundedly (NFS). It
was declined because the project's architecture rule is that the application owns
no threads (`sdd/POLICY.md` § "All GTK access on the main thread"), and the bound
above is sufficient for every non-hanging filesystem. If that rule is ever revisited,
this is the case that should reopen it.

## 244. Making a window-scoped operation async turns "which tab is active?" into two different questions

**Symptom.** `save_window` resolved its target with `state(window)` — the window's
active tab. That was exact for as long as the write was synchronous, because
nothing could change which tab was active in the middle of a call. Once the guard
read and the write were dispatched to a thread pool, the main loop ran between
them, and the *same expression* asked before and after answered about two different
documents: the guard could check tab A's file for an external change and then
authorise a write of tab B's buffer. The conflict protection (C2) was defeated
without a symptom.

Three more instances of the same shape fell out of it:

- a modal "Overwrite?" whose response handler re-resolved the tab, so answering it
  after a tab switch wrote the wrong file;
- a completion that refreshed the window's chrome, which is correct, but was also
  the only thing retiring the **tab's** crash-recovery snapshot, which is not — a
  document saved while the user was looking elsewhere kept its snapshot, and the
  next crash offered already-saved work back as "unsaved";
- `save_as`, whose adoption of a chosen path and whose write were two separate
  lookups.

**Where Scribobulate implements the fix.** `src/window/save.rs` — every step takes
an explicit `Rc<TabState>`, resolved once when the user acts and carried through
the read, the dialog and the write. `save_window` additionally splits its
completion into **tab-scoped** work (`sync_tab_swap`, `badge_tab_label` — must
target the captured tab) and **window-scoped** work (`refresh_dirty_status`, the
toast — must target whatever is on screen). `src/window/reload.rs` does the same
with an explicit active-vs-background split after its read.

**The rule.** Resolve ambient context **once**, at the moment the user acts, and
carry it. An expression whose answer is a property of *now* must not be used to
describe *then*, and an `await` is exactly the boundary that turns one into the
other. When auditing an operation being made async, list every implicit lookup
inside it — the active tab, the focused widget, the current mode, the selection —
and decide for each whether it belongs to the gesture or to the moment.

**MEASURED trap in guarding this, and it cost a rebuild.** The obvious regression
test passes against the broken code: a test that saves and then asserts leaves the
same tab active throughout, so the two lookups agree and the bug is invisible. The
first version of the guard here survived its mutation run for exactly that reason.
A test must **force the divergence** — and can, deterministically:
`glib::MainContext::spawn_local` does not poll its future until the loop iterates,
so switching tabs synchronously on the line after issuing the save guarantees the
write completes against a background tab. Mutating the tab-scoped call out then
fails, as it must.

**Kin #46** (don't cache a reparent-able context) one step across: there a cached
value went stale across a *move*, here a freshly-read one goes stale across an
*await*. The tell is identical.


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

## 246. GDK-Win32 refuses an empty window title and substitutes a literal "."

**Symptom.** On Windows, every modal confirmation the application raises shows a
lone `.` beside the app icon in its title bar, and the same `.` in the taskbar and
in Alt+Tab. Nothing in the application sets that string, and no warning is emitted.

**Measured.** `GetWindowTextW` on the "Save changes before closing?" dialog returns
`"."`. The source of the substitution is GDK, not the app (gtk-4.22.4,
`gdk/win32/gdksurface-win32.c:1238`):

```c
/* Empty window titles not allowed, so set it to just a period. */
if (!title[0])
  title = ".";
```

The empty title upstream of it is `GtkMessageDialog`'s own default, and that default
is deliberate: GNOME's HIG asks for untitled message dialogs, and
`gtk_message_dialog_constructed` builds a header whose title label is hidden
precisely while the title is empty (`gtk/gtkmessagedialog.c:306-314`).

**Why no other seat can see it.** The widget tree is identical on all three
platforms. An empty caption is a *legitimate rendering* under CSD — the header
simply draws nothing — so Linux and macOS are not merely failing to notice a bug,
they are correct. The defect exists only where the window manager owns the caption
and cannot express "no caption". There is no warning, no `Gtk-CRITICAL`, and no test
that would go red.

**The rule.** Give every window you construct a title, at the shared construction
site rather than at each call, and set it **unconditionally**. A `#[cfg(windows)]`
caption would fork behaviour per platform and put platform-shaped code outside
`platform/<os>/`, both of which POLICY forbids; the cost of setting it everywhere is
a header label where a GNOME dialog previously had blank space, which is a design
consequence to state rather than a defect to hide.

**The generalisable half.** *A toolkit default chosen for one platform's design
language can become a defect on a platform whose window manager cannot express it.*
Every "we deliberately leave this empty/unset" is worth auditing against each
backend's substitution behaviour, because a substitution is silent by construction —
the backend does not report that it overrode you. Kin to #237, which is the same
"evidence about the canonical platform only" failure aimed at compilation rather
than rendering.

**Scribobulate.** `window::save::confirm_dialog` sets `winstate::APP_NAME`; the one
place all three modal confirmations (close prompt, overwrite warning, save error)
are built. Pinned by `a_modal_confirmation_carries_a_window_title`, which diffs the
window's modal transients across the call — a document window already owns another
modal transient (the Keyboard Shortcuts help window), so "the modal transient" is
not a well-formed question.

## 247. "No handler is registered for this scheme" is not a safety property

**Symptom.** A GTK test suite raised a modal Windows dialog that **outlived the test
process**, held foreground focus, and could not be dismissed by any of the usual
means — then silently swallowed the keystrokes of an unrelated driven UI run an hour
later.

**The claim that failed.** A control test deliberately let `GtkLinkButton`'s default
`activate-link` handler run — the point being to prove the oracle discriminates, so
the default handler *must* execute. Its URI was
`x-scribobulate-no-such-handler:///probe`, with a comment stating the scheme has no
registered handler anywhere, so `gtk_show_uri` "fails to launch instead of opening
something on the machine running the test".

On Linux that holds: an unknown scheme produces a `g_warning` and nothing else.

**On Windows it is false.** The shell answers an *unregistered* scheme by presenting
a modal "You'll need a new app to open this …" chooser, hosted by
`SystemSettings`/`ApplicationFrameHost`. Measured on Windows 10 19045 / GTK 4.22.4,
that dialog:

- is a **different process**, so it is absent from the test binary's window list and
  is not cleaned up when the binary exits;
- outlives the test run and stays on the desktop;
- holds foreground focus — it cost a full drive cycle before the "keys are being
  dropped" symptom was traced back to it;
- ignores `WM_CLOSE` (it is a UWP surface) and Escape, so a harness cannot dismiss
  it the usual ways;
- presents **OK beside an already-ticked "Always use this app"** — one stray click
  from writing a scheme handler under `HKCU`.

A test suite reached outside its process and could have reconfigured the host. No
association was in fact written (`assoc`, `FileExts\.md\UserChoice` and
`OpenWithProgids` were all checked afterwards and were empty), so the exposure was
the click, not the appearance.

**The fix: make the probe invalid rather than unclaimed.** GTK refuses to launch a
URI that does not parse, before it reaches any platform launcher
(gtk-4.22.4, `gtk/gtkurilauncher.c:333`):

```c
if (!g_uri_is_valid (self->uri, G_URI_FLAGS_NONE, &error))
  { g_task_return_new_error (task, ...); g_object_unref (task); return; }
```

That return precedes `gtk_show_uri_full`, so nothing is handed to the shell,
LaunchServices, or a desktop portal. `gtk_link_button_set_visited (…, TRUE)` runs
unconditionally afterwards (`gtk/gtklinkbutton.c:551`), so a `visited`-based oracle
is unaffected and the control still discriminates.

**The rule.** **Prefer a safety claim about your own dependency over one about the
host.** "GTK refuses to launch X" is checkable in-repo, on every run, on every
platform — and should be *asserted*, not commented, so it cannot rot back into
prose. "No machine has a handler for X" is a claim about every machine the tests will
ever run on; nothing can check it, and it fails on precisely the machine nobody
tested.

**Second-order, and the reason it sat undetected.** The Linux failure mode is
*silence*. The seat best placed to run this test constantly is the one structurally
unable to observe the defect — the same platform-shaped hole in what a gate can see
as #237 and #242.

**Scribobulate.** `renderer::end`'s `INERT_URI`, plus
`the_probe_uri_is_one_gtk_refuses_to_launch`, which asserts
`glib::Uri::is_valid(INERT_URI, UriFlags::NONE).is_err()` on every platform — the
enforcement mechanism for the claim, per POLICY § Typed GTK seams.

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

## 249. A capability whose backend is a HELPER EXECUTABLE is a packaging obligation, and the dev tree cannot fail the test

**Symptom.** A user reported that Scribobulate opened a **separate process** for
every Markdown file double-clicked in Explorer — two windows, two live-reload
monitors, two buffers that could each save over the other. It did not reproduce on
any developer machine. Nothing errored: `g_application_register()` succeeded,
`is_remote()` reported `false`, and every launch elected itself primary, which is
byte-for-byte what a legitimate first launch looks like — #174's signature exactly,
on a platform #174 had explicitly cleared.

**What was believed.** That Windows needed no single-instance seam because *"GIO's
Win32 backend performs single-instance forwarding itself."* Three documents said so
(`TECH.md` twice, `MANUAL-TEST.md` twice) and so did the Windows agent profile. **There
is no such backend.** GIO negotiates `GApplication` uniqueness over a D-Bus session
bus on Windows exactly as on Linux. What is Windows-specific is only how the bus comes
to exist: with no `DBUS_SESSION_BUS_ADDRESS` set, GLib autolaunches one by **spawning
`gdbus.exe`**, resolved from beside the loaded GLib DLL. Note where the wrong belief
came from — #174 recorded it as *the assumption that had misled the macOS work*, and
it survived being written down as a known error, in the register whose job is to stop
exactly that.

**Root cause: `packaging\windows\stage.ps1` shipped 32 DLLs and one `.exe`.** No
`gdbus.exe` in the redistributable ⇒ no session bus in the installed app ⇒ no
uniqueness. The capability's entire Windows implementation was a file nobody had
listed, because every list in the tree was a list of *libraries*.

**Why no test caught it, which is the transferable half.** The single-instance items
(TDD 8.1/8.2/8.5) run against `target\release\scribobulate.exe`, and that binary is
launched from a shell with gvsbuild's `bin` on `PATH` — which is where `gdbus.exe`
lives. **The dev tree passes whether or not the redistributable ships the helper: the
test is structurally incapable of failing.** The defect was not in a corner the suite
had not reached, it was in the *difference between the artefact tested and the
artefact shipped* — and that difference was invisible because both artefacts run the
same code. A green single-instance suite was, on Windows, evidence about `PATH`.

**Measured** (staged tree, clean `PATH`, GTK 4.22.4/gvsbuild — *not* the 4.6 floor;
two launches on one document each time):

| Tree | `gdbus.exe` reachable | live processes |
|---|---|---|
| staged, as shipped | no | **2** |
| `target\release\` + gvsbuild `bin` on `PATH` | yes | 1 |
| staged + `gdbus.exe` | yes | 1 |
| two staged copies at **different paths**, both with it | yes | 1 |
| **installed** by the compiled setup.exe | yes | 1 |
| **installed**, `gdbus.exe` renamed away in place | no | **2** |

The last two rows are the ones that settle it: the compiled installer was run
per-user and the property measured on the product, with the negative control taken
*in the installed tree* so the measurement is demonstrably not vacuous. The install
path (`%LOCALAPPDATA%\Programs\Scribobulate`) contains a **space**, which the earlier
staged runs did not — they came from an 8.3 short path and so could not have caught a
quoting fault in the helper spawn. Explorer double-click was then driven end to end
with `.md` associated: two documents, **one process, two windows** (TDD 8.1).

The fourth row matters: the install *location* is not a factor, so "they had another
copy installed" was a red herring and the helper's presence was the only variable.
Two further measurements, because a fix that ships a second executable owes them:
the daemon runs from the install's own `bin\` (confirming resolution is relative to
the GLib DLL, not `PATH` or the working directory), and it **exits on its own** once
the last client disconnects — even after a force-kill — so it never holds the install
directory open against an uninstaller.

**The rule.** *When a capability is delivered by a backend, ask what the backend
physically **is** — and if the answer is a file, it is a packaging obligation, not a
code one.* #174 said prove the backend exists; the case it did not cover is that the
backend can exist in your build environment and be absent from your product. Two
corollaries worth more than the GLib specifics:

- **A dependency-manifest that lists only libraries will lose helper executables**,
  because nothing links against them and no loader error names them. `stage.ps1`'s
  DLL list was rigorous — it hard-fails on a missing entry, and carries a comment
  about `rsvg-2-2` being needed though never observed loaded. The rigour was
  complete and the *category* was wrong.
- **A test that runs against the development tree is a test of the development
  tree.** Where the shipped artefact is assembled by a separate step, at least one
  item per capability must run against the assembled thing, from an environment
  scrubbed of the build environment's `PATH`. Kin #239 (verifying against the right
  artefact) and #234 (a suite that exercises only the entry point under which the
  defect is invisible).

**Scribobulate.** `packaging\windows\stage.ps1` `$helpers` (hard-fails like the DLL
list, so a future gvsbuild layout change is a build error rather than a silent
regression); the corrected claims in [TECH.md](TECH.md) (platform table + the
single-instance architecture bullet) and `tests/MANUAL-TEST.md` (§A *Launch & instance
identity*, the `-n` note); new manual item **8.2s**, which re-runs 8.1/8.2 against the
staged tree with a scrubbed `PATH` and asserts the `gdbus` daemon as a transport check
rather than trusting the outcome. Contract unchanged: **TDD 8.1/8.2** — the point is
that the *behaviour* never needed a Windows clause, only the *package* did.

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

**Measured** (Ubuntu jammy, GTK 4.6.9, GLib 2.72.4, `GSK_RENDERER=cairo`, Xvfb; a
throwaway C rig against the distro libraries, so nothing here depends on this
application):

| Channel | State | How it fails |
|---|---|---|
| `GTK_DEBUG` / `GDK_DEBUG` / `GSK_DEBUG` informational keys | Every key `[unavailable]`; only `interactive` (the Inspector) survives | **Warns**, one line on stderr: `"geometry" is only available when building GTK with G_ENABLE_DEBUG` — then runs normally and exits 0 |
| `g_type_get_instance_count()` (and so the Inspector's Statistics tab) | Symbol **exported**, links and calls fine, returns **0** always — before, during and after allocating 100 `GtkLabel`s | **Silent**, and the value it returns is the value a healthy process would return |
| sysprof marks (GTK frame/layout marks, GLib main-context marks) | Neither `libgtk-4.so.1` nor `libglib-2.0.so.0` links `libsysprof-capture`; no `sysprof` binary installed | **Absent** — no marks, no error, nothing to notice |

`GdkFrameClock` is the exception that proves where the line falls: it is ordinary
API rather than debug instrumentation, so `gdk_frame_timings_get_frame_time()`
returns real monotonic microseconds on this same stack (~16–17 ms apart under Xvfb).
`presentation_time` is `0` there — Xvfb gives no presentation feedback — so a
latency oracle must be built on `frame_time`, and any check of presentation timing
belongs on the real session.

**The mechanism.** These are not runtime toggles. `G_ENABLE_DEBUG` is a *compile*
option, and distributions build the shipping libraries without it, which compiles
out the debug key bodies and the instance-count bookkeeping alike; sysprof support
is a separate build option that is simply not enabled. The environment variables and
the symbol both still exist, because the *entry points* are unconditional — only the
work behind them is gone. This is the same wall as the missing debug symbols
(#141): one distro build decision, several instruments dark.

**The rule.** *An instrument that is switched off does not report "off" in any
standard way — and the dangerous ones report a value that is indistinguishable from
a clean result.* So **prove a channel emits before you trust its silence**: run it
once against a condition you know is present (allocate objects and watch the count
rise; trigger a relayout and watch the log fill) and only then use its emptiness as
evidence. Three practical consequences for any plan written against a distro
toolkit:

1. **Do not build a verification tier on toolkit introspection.** Prefer
   instrumentation the application owns (frame-clock deltas, phase timings through
   its own log) — it is also the only tier that survives the trip to the other
   platforms — plus process-external sampling, which needs nothing from the toolkit.
2. **A prescribed instrument is part of a plan's correctness.** An issue or plan
   naming a probe should say on which build it was verified to emit, or the next
   agent spends the session discovering it is dark — and may bank a false negative
   on the way.
3. **Getting the instrumentation back means a differently-built GTK**, not a flag:
   a locally built GTK 4.6.x configured with debug enabled, loaded ahead of the
   distro one. That is a real option and a real cost, and it is the *only* route to
   the geometry/size-request keys — decide it deliberately rather than discovering
   it mid-hunt.

*Non-core (verification tooling / distribution build configuration — not a GTK
widget contract). Do not fold into the `gtk4-rs` skill's core modules; the
transferable half is the "prove it emits" rule, which is a testing-methodology
lesson.* Kin #141 (the same build decision, seen as missing symbols — and the
`LD_PRELOAD` GType interposer that is the fallback when instance counting is dark),
#155 (leak attribution without symbols), #49 (toolkit noise in valgrind output).

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

*Non-core (verification tooling / test-harness design — not a GTK widget contract).
The transferable half is "assert the setup, not its delivery", plus the GTK-specific
fact that activating a disabled `GAction` is a silent no-op with no signal to hook.*
Kin #245 (the same silence one layer down — the input *channel* delivering nothing
while every diagnostic says otherwise), #217 (positive controls), #239 (a control
that cannot differ has stopped being a control).
