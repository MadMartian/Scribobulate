# Schema Reference

Exact shapes of the contracts Scribobulate exposes or consumes. TECH.md describes
which components exist and where the boundaries fall; this document describes the
precise structure of what crosses them.

## GAction interface

Every command reachable from more than one surface — menu bar, context menu,
toolbar, keyboard accelerator, or the `org.gtk.Actions` D-Bus interface GIO
exports for a `GtkApplication` — is a single `gio::SimpleAction`. The action name
and its parameter/state types are the contract; the surfaces are interchangeable
views onto it. Because GIO exports these over D-Bus, this table is also the
external automation surface: tests and tooling drive the application by
activating these names rather than by synthesising input events.

Actions are registered in two scopes. **Application-scoped** (`app.`) actions are
registered once on the `Application` and are valid whether or not a window is
open. **Window-scoped** (`win.`) actions are registered per window; each open
window has its own instance, and a stateful `win.` action's state is that
window's state, not a global.

A **stateless** action carries out a command on `activate`. A **stateful** action
additionally holds a value: boolean-stateful actions back a checkbox or toggle,
and string-stateful actions back a radio group where the state names the selected
member. For a stateful action the work is driven by `change-state`, not
`activate`.

### Application-scoped actions (`app.`)

| Action | Parameter | State | Description |
|--------|-----------|-------|-------------|
| `app.new` | — | — | Open a new empty document window. |
| `app.open` | — | — | Present the file chooser and open the chosen file. |
| `app.quit` | — | — | Quit the application, prompting for unsaved documents. |
| `app.about` | — | — | Show the About dialog. |
| `app.markdown-help` | — | — | Show the bundled Markdown reference. |
| `app.preview-theme` | `s` | `s` | The active reading theme, identified by theme id. Radio group backing the theme menu. |
| `app.pick-preview-theme` | `s` | — | Select the reading theme named by the target; delegates to `app.preview-theme`'s state. |

### Window-scoped actions (`win.`)

| Action | Parameter | State | Description |
|--------|-----------|-------|-------------|
| `win.save` | — | — | Write the document to its current path. |
| `win.save-all` | — | — | Write every tab in this window that needs saving (dirty, or clean over a deleted backing file). Untitled tabs get Save As one at a time. |
| `win.save-as` | — | — | Choose a new path and write the document there. |
| `win.rename` | — | — | Change the filename of the document's backing file, within its own directory (never a move). Enabled only for a titled, clean document whose file is present; an in-flight write is refused by the write gate at apply time rather than by the enabled state. |
| `win.reload` | — | — | Discard in-memory changes and re-read the file from disk. |
| `win.auto-reload` | — | `b` | Whether external file changes are picked up automatically. |
| `win.copy-path` | — | — | Copy the document's filesystem path to the clipboard. |
| `win.copy-document` | — | — | Copy the whole document source to the clipboard. |
| `win.export` | `s` | — | Write the document to a presentation format at a chosen path. Targets below. Enabled whenever the tab holds a document, **including untitled and unsaved ones** — an export reads the buffer, never the disk file. |
| `win.copy-link-location` | — | — | Copy a link's destination URL: the link a context menu was opened on, else the Markdown link (or image) under the editor caret. Enabled while a right-clicked link is armed, or the editor is visible with a link under the caret. |
| `win.undo` | — | — | Undo the last editor-buffer change. |
| `win.redo` | — | — | Redo the last undone editor-buffer change. |
| `win.cut` | — | — | Cut the editor selection. |
| `win.copy` | — | — | Copy the selection. |
| `win.delete` | — | — | Delete the editor selection without copying it. |
| `win.select-all` | — | — | Select the whole buffer. |
| `win.change-case` | `s` | — | Recase the editor selection. Targets below. |
| `win.insert-emoji` | — | — | Open the platform emoji chooser at the caret. |
| `win.go-to-line` | — | — | Prompt for a line number and move the caret there. |
| `win.format` | `s` | — | Apply a Markdown formatting command. Targets below. |
| `win.find` | — | — | Reveal the find bar. |
| `win.find-replace` | — | — | Reveal the find bar with the replace row expanded. |
| `win.annotate` | — | — | Attach a CriticMarkup comment to the selection. |
| `win.next-annotation` | — | — | Go to the next annotation in the document, wrapping at the end. |
| `win.prev-annotation` | — | — | Go to the previous annotation in the document, wrapping at the start. |
| `win.view-mode` | `s` | `s` | The window's view mode. Radio group; targets below. |
| `win.split-swap` | — | `b` | Whether the split panes are swapped. |
| `win.split-orientation` | — | `b` | Whether the split is oriented vertically rather than horizontally. |
| `win.outline` | — | `b` | Whether the outline sidebar section is shown. |
| `win.annotations` | — | `b` | Whether the annotations sidebar section is shown. |
| `win.outline-expand-all` | — | — | Expand every outline node. |
| `win.outline-collapse-all` | — | — | Collapse the outline to its root headings. |
| `win.show-toolbar` | — | `b` | Whether the toolbar is shown. |
| `win.show-statusbar` | — | `b` | Whether the status bar is shown. |
| `win.show-unsafe-images` | — | `b` | Whether images outside the document's directory are rendered. Per tab. |
| `win.allow-outside-links` | — | `b` | Whether links outside the document's directory may be opened. |
| `win.zoom-in` | — | — | Increase the preview zoom one step. |
| `win.zoom-out` | — | — | Decrease the preview zoom one step. |
| `win.zoom-reset` | — | — | Return the preview zoom to its default. |
| `win.new-window` | — | — | Open a new window on the same session. |
| `win.close-tab` | — | — | Close the active tab, prompting if it is dirty. |
| `win.next-tab` | — | — | Activate the next tab. |
| `win.previous-tab` | — | — | Activate the previous tab. |
| `win.nav-back` | — | — | Return to the place visited before the current one in this window's Back/Forward history. A *place* is a document and, when the reader arrived by following a link to a section of it, the position within that document — so this may switch tabs, or may only scroll the tab already active. Enabled only while there is an earlier entry — at the oldest it is insensitive rather than wrapping. Also carried by Alt+Left, `XF86Back`, and mouse button 8. |
| `win.nav-forward` | — | — | Return to the place a Back left, in this window's history — the exact inverse of `win.nav-back`, and likewise a document, a position within one, or both. Enabled only while a forward trail exists; any new navigation discards it. Also carried by Alt+Right, `XF86Forward`, and mouse button 9. |
| `win.select-tab` | `s` | `s` | The active tab, identified by tab id. Radio group backing the View ▸ Documents menu and the toolbar's Documents combo (one action, two surfaces). |
| `win.move-tab-new-window` | — | — | Detach the active tab into a new window. |
| `win.show-help-overlay` | — | — | Show the shortcuts window. Provided by GTK, not registered by this application. |

### Parameterised action targets

**`win.format`** — the target selects a `FormatCmd`. Unrecognised targets are
rejected and the action does nothing.

| Target | Effect |
|--------|--------|
| `bold` | Wrap the selection in `**`. |
| `italic` | Wrap the selection in `*`. |
| `strike` | Wrap the selection in `~~`. |
| `highlight` | Wrap the selection in `==` (mark). |
| `code-span` | Wrap the selection in a backtick code span. |
| `sup` | Wrap the selection in `^`. |
| `sub` | Wrap the selection in `~`. |
| `code-block` | Convert the selected lines to a fenced code block. |
| `quote` | Prefix the selected lines with `>`. |
| `bulleted-list` | Convert the selected lines to a bulleted list. |
| `numbered-list` | Convert the selected lines to a numbered list. |
| `task-list` | Convert the selected lines to a task list. |
| `hr` | Insert a horizontal rule. |
| `h1`–`h6` | Set the selected lines' heading level. Only levels 1 through 6 parse. |

**`win.view-mode`** — the target and the state share this value set.

| Target | Meaning |
|--------|---------|
| `edit` | Source pane only. |
| `preview` | Rendered preview only. |
| `split` | Both panes, side by side or stacked per `win.split-orientation`. |

**`win.change-case`** — recases the editor selection.

| Target | Effect |
|--------|--------|
| `upper` | UPPER CASE. |
| `lower` | lower case. |
| `title` | Title Case. |
| `toggle` | Invert each character's case. |

**`win.export`** — writes the document to a presentation format. Both targets go
through one pipeline and differ only in their final sink.

| Target | Effect |
|--------|--------|
| `html` | One self-contained HTML file: local images embedded as data URIs, styling from the active reading theme, heading anchors preserved. The sharing format. |
| `pdf` | A paginated PDF, resolved against the System theme's light resolution. The record format. |

**`win.select-tab`** and **`app.preview-theme`** take a free-form string: a tab id
and a reading-theme id respectively. Neither has a fixed value set — the valid
values are whatever tabs are open, and whichever themes resolve on the current
search path.

## Annotation storage format (CriticMarkup)

Annotations are stored inline in the Markdown document itself, as CriticMarkup,
rather than in a sidecar file. The document remains valid Markdown, and
annotations survive editing in any other tool.

| Form | Syntax | Meaning |
|------|--------|---------|
| Highlight with comment | `{==highlighted span==}{>>comment<<}` | A comment attached to a span of text. |
| Point comment | `{>>comment<<}` | A comment with no highlighted span. Also the fallback when a selection crosses block boundaries. |

Three further CriticMarkup kinds are **recognised but rendered inert** — parsed so
they are not mistaken for body text, but carrying no editorial behaviour:

| Form | Syntax |
|------|--------|
| Insertion | `{++inserted++}` |
| Deletion | `{--deleted--}` |
| Substitution | `{~~old~>new~~}` |

## Crash-recovery swap file (`*.swap`)

A **swap file** is a periodic full-content snapshot of a dirty buffer, written to
`$XDG_STATE_HOME/scribobulate/swap/` so an unclean exit does not discard unsaved work.
It crosses a boundary in time rather than between components — one process writes it, a
later one reads it — which is why its shape is pinned here rather than left to the code.

Filename: `<sanitized-stem>-<doc_id>.swap`. **The stem is cosmetic and is never parsed**;
`doc_id` is the identity and the header is authoritative.

```
+++scribobulate-swap 1          <- opening fence: magic + format version
doc_id = "3f2ac91b4d5e6f708192a3b4c5d6e7f8"
path = "/home/u/Documents/notes.md"
untitled = false
baseline_digest = "ae25375ee0bc0092275a5f0e9b031b5b-39"
written_at = 1785634894
owner_pid = 4075958
app_version = "0.1.0"
+++                             <- closing fence, alone on its line
<the buffer's bytes, verbatim, to EOF>
```

| Field | Type | Meaning |
|-------|------|---------|
| `doc_id` | 32 lowercase hex chars | The document's identity for its tab's life — stable across a save, a Save As, a rename on disk, and a move to another window. Validated on read, because it is interpolated into a filename. |
| `path` | string, optional | The twin's absolute path. Absent for an untitled buffer **and** for a path not representable as text — which is why `untitled` is carried separately rather than inferred. |
| `untitled` | bool | Whether the document was never saved. Explicit, so a header that lost its `path` cannot silently become an untitled recovery. |
| `baseline_digest` | `<32 hex>-<len>` | FNV-1a 128 of the twin's content as of the last load or save, plus its byte length. Change detection, not integrity — it decides whether the file moved under the snapshot. |
| `written_at` | int | Unix epoch seconds; drives the recovery notice's wording. |
| `owner_pid` | int | The writing process, for the liveness guard. |
| `app_version` | string | Which build wrote it. |

Three properties of the format are load-bearing rather than stylistic:

- **The first line identifies the file.** A file whose first line is not the magic is not
  ours: ignored, logged, and **never deleted** — the state directory is a shared place.
  A file that *is* ours but whose header will not parse is likewise kept, because it may
  be the only surviving copy of the user's work.
- **The terminator search is bounded** (first ~8 KiB / 64 lines). Past that the file is
  malformed. Content *after* the terminator is therefore unconstrained — the body may
  contain `+++`, `---`, or a whole nested frontmatter block.
- **No header value may serialise to a bare `+++` line.** Line breaks are escaped out of
  every externally-derived string before serialisation, and the result is verified before
  the file is written. This is not something the TOML serialiser provides — see ScrAP-233.

**A snapshot is written to `<name>.swap.tmp`, co-located, and renamed into place only
after a complete successful write.** Co-location is a correctness requirement: `rename(2)`
is atomic only within one filesystem. The temp carries `0600` from `open(2)` and the
rename carries that mode to the destination.

An orphaned `.swap.tmp` is therefore, by definition, a write that never completed — there
is nothing in it worth keeping and no way to distinguish a truncated one from a whole one
— so the startup scan **deletes it outright**. That is the only deletion the scan
performs, and it matches the full `.swap.tmp` suffix precisely: a stray `.tmp` belonging
to anything else in this shared directory is left alone, as is a foreign `.swap`, as is a
*damaged* `.swap` of ours (which may be the only surviving copy of the user's work).

A future version bump is readable-by-refusal: a file whose version this build does not
understand is left untouched for the build that does.

## Reading-theme file (`themes.toml`)

A reading theme is data, not code: adding one is a TOML block. Themes are read
from `themes.toml` files found on the search path below.

### Search path

First match wins. A user file is merged over a built-in **per theme id and per
key**, so a user may override a single key of a shipped theme, override an
application default via the `[themes.system]` block, or define an entirely new
theme.

| Order | Path |
|-------|------|
| 1 | `${XDG_CONFIG_HOME:-~/.config}/scribobulate/themes.toml` (user override) |
| 2 | `${XDG_DATA_HOME:-~/.local/share}/scribobulate/themes.toml` (per-user install) |
| 3 | each `$XDG_DATA_DIRS` entry → `…/scribobulate/themes.toml` (system install) |

**On Windows** row 1's base is `%APPDATA%` (Roaming) rather than `~/.config` —
`XDG_CONFIG_HOME` is still honoured first if set, and only the fallback differs. Rows 2 and 3 come
from GLib's own data-dir resolution and are already platform-aware. Assuming the XDG spelling here
was literally the bug behind the (now-closed) register entry R: user theme overrides were
unreachable on Windows because the base directory could never be resolved.

### Key resolution

Each key resolves by the first source that supplies it: the selected theme's own
key, then `[themes.system]`, then a probe of the desktop GTK theme with derived
values. A theme therefore states only what makes it distinctive and derives the
rest.

The desktop probe (the third source) reads these base Adwaita named colours via
`StyleContext::lookup_color`: `theme_bg_color`, `theme_fg_color`, `theme_base_color`,
`theme_text_color`, `theme_selected_bg_color`, `theme_selected_fg_color`, `borders`.
The libadwaita-only names (`view_bg_color`, `accent_bg_color`, `card_bg_color`) are
used only as fallback-chain entries where the active theme defines them.

### `[themes.<id>]` keys

Every key is optional; omitting one means "derive or inherit it". Colours are
strings parsed as `RGBA` (`#RRGGBB`, `#RRGGBBAA`, or a CSS colour name).

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | `string` | the theme id | Display name in the theme chooser. |
| `symbol` | `string` | — | Decorative symbol shown left of the name in the picker. |
| `background` | colour | derived | Page background. One of the three base colours every derived colour follows from. |
| `foreground` | colour | derived | Body text colour. |
| `accent` | colour | derived | Accent colour. |
| `font_family` | `string` | derived | Body font stack. Sanitised, and always terminated with a generic family. |
| `syntect_theme` | `string` | by page luminance | Syntax-highlighting theme for code blocks. |
| `heading_color` | colour | body foreground | Heading colour (h1–h6). |
| `heading_font` | `string` | `font_family` | Heading font stack. Same sanitisation as `font_family`. |
| `heading_colors` | `[string; 5]` | `heading_color` | Per-level heading colours, h1 · h2 · h3 · h4 · h5-and-deeper. An empty (`""`), absent or unparseable slot falls back to `heading_color`. The table header is not a level and keeps reading `heading_color`. |
| `heading_fonts` | `[string; 5]` | `heading_font` | Per-level heading font stacks, same five slots and same empty-means-inherit rule, falling back to `heading_font`. Each slot is sanitised like `font_family`. |
| `heading_overline` | `"none"` \| `"single"` | `"none"` | A rule ABOVE the heading text, drawn in the heading's own ink. `"double"`/`"wavy"` are accepted and clamped to a single line (Pango's overline has no other value). ⚠️ There is deliberately **no** `heading_overline_rgba`: GTK 4.6 double-frees a text run carrying both a coloured overline and a coloured underline, and a link inside a heading is such a run (`src/theme.rs`, `HeadingRule`). |
| `heading_underline` | `"none"` \| `"single"` \| `"double"` \| `"wavy"` | `"none"` | A rule BELOW the heading text — the decoration line under the glyph run, not a column-width divider. |
| `heading_underline_rgba` | colour | heading ink | The below-rule's colour. Omitted, the line follows the heading's own foreground. |
| `heading_band_bg` | `[string; 5]` | — | The band drawn behind a heading's text, per level (h1 · h2 · h3 · h4 · h5-and-deeper). An empty or absent slot ⇒ that level carries no band. The band spans the **content column**, and survives soft-wrap as one continuous band. |
| `heading_band_gradient_to` | colour | — | A second stop: the band becomes a vertical gradient from the level's own fill down to this colour. Ignored where no level states a fill. |
| `sprite_heading_band` | `string` | — | A sprite **tiled at its natural size** across the band, in place of its fill. Theme-relative and validated like every sprite key; outranks the fill and the gradient. |
| `link` | colour | derived | Link colour. |
| `link_underline` | `"none"` \| `"single"` \| `"double"` \| `"wavy"` | `"single"` | A link's underline style. Defaults to the single line the app has always drawn, not to `"none"`. |
| `link_underline_rgba` | colour | the link colour | The link underline's colour, stated independently of the link's ink. Omitted, the line follows the ink. |
| `strikethrough_rgba` | colour | the struck text's ink | The colour of the line through `~~text~~`. Omitted, it follows the struck text's own foreground. Reaches the body tag, the table-cell span and both export sinks alike. |
| `code_inline_bg` | colour | derived | Inline code-span background. |
| `code_block_bg` | colour | derived | Fenced code-block background. |
| `blockquote_bar` | colour | derived | Blockquote indicator bar. |
| `selection_bg` | colour | derived | Selection background. |
| `selection_fg` | colour | derived | Ink for *selected* text. Omitted, it derives from `selection_bg` plus the page and its ink — whichever of the two reads better on the fill — so a theme cannot strand its own selected text by accident. State it when the derived answer is legible but wrong: contrast is not taste. |
| `table_border` | colour | derived | Table border. |
| `table_head_bg` | colour | derived | Table header background. |
| `rule` | colour | derived | Horizontal-rule colour. |
| `list_marker` | colour | widget foreground | Bullet, numeral and task-checkbox colour. Marker only; item text keeps the body foreground. |
| `mark_bg` | colour | `#fff59d_88` | Background band behind `==marked==` text. |
| `list_marker_2` | colour | `list_marker` | The **bullet's** colour at nesting depth 2. ⚠️ Bullet only, unlike the un-suffixed `list_marker` beside it, which colours all three marker kinds: a nested numeral is still a numeral and a nested task box still a box, where a bullet dot's whole job is to say which level you are on. |
| `list_marker_3` | colour | `list_marker_2` | The bullet's colour at depth 3 **and deeper**. Unset falls back to the next *shallower* tier, not to the base — so stating only `list_marker_2` colours every depth from 2 down. |
| `list_bullet_glyph` | `string` | — | A glyph drawn in place of the bullet dot. Trimmed; refused (falling back to the drawn marker) if empty, over 8 characters, or carrying a control character — over-long is refused rather than cut, since a cut can split a grapheme cluster. |
| `list_bullet_glyph_2` | `string` | `list_bullet_glyph` | The bullet's glyph at nesting depth 2. |
| `list_bullet_glyph_3` | `string` | `list_bullet_glyph_2` | The bullet's glyph at depth 3 and deeper. |
| `list_ordered_glyph` | `string` | — | A glyph drawn in place of the ordered numeral. ⚠️ This DISCARDS the ordinal — deliberate, and inert unless a theme asks for it. |
| `list_task_glyph` | `string` | — | A glyph drawn in place of the unchecked task box. |
| `list_task_checked_glyph` | `string` | — | A glyph drawn in place of the checked task box. Resolves independently of the unchecked one, so a theme may state either alone. |
| `sprite_list_bullet` | `string` | — | A sprite image drawn in place of the bullet dot. Theme-relative and validated like every sprite key (no absolute path, no traversal, symlink-contained, allowlisted extension, size-capped). **A sprite outranks a glyph for the same marker.** |
| `sprite_list_bullet_2` | `string` | `sprite_list_bullet` | The bullet's sprite at nesting depth 2. |
| `sprite_list_bullet_3` | `string` | `sprite_list_bullet_2` | The bullet's sprite at depth 3 and deeper. |
| `sprite_list_ordered` | `string` | — | A sprite drawn in place of the ordered numeral. |
| `sprite_list_task` | `string` | — | A sprite drawn in place of the unchecked task box. |
| `sprite_list_task_checked` | `string` | — | A sprite drawn in place of the checked task box. |
| `mark_fg` | colour | body foreground | Ink for `==marked==` text. Omitted, marked text keeps the body foreground and only its background changes — how a highlighter behaves on paper, and right for any `mark_bg` that is a translucent wash. State it when the band is opaque enough to need its own ink. Reaches the body tag and the table-cell span alike. |
| `annotation_hl` | colour | `#FFD133_61` | Annotation highlight overlay. |
| `annotation_chip_bg` | colour | hardcoded amber | CriticMarkup comment gutter chip's fill. Omitted, the chip stays the exact hardcoded amber/white it always was (TDD 18.2). |
| `annotation_chip_fg` | colour | hardcoded white | Ink for the chip's overflow-count numeral. |
| `sprite_annotation_chip` | `string` | — | A sprite image drawn in place of the flat chip fill, path relative to this theme file's own directory. Validated at load time (no absolute path, no `..` traversal, symlink-contained, allowlisted extension, size-capped — `crate::sprite::resolve`). No expression in the PDF's inline Pango markup — a stated scope limit (TDD 18.19). |
| `find_hl_all` | colour | `#f6d32d` | Highlight for all find matches. |
| `find_hl_current` | colour | derived | Highlight for the current find match. |

`syntect_theme`'s "by page luminance" default resolves to `InspiredGitHub` for a
light page and `base16-ocean.dark` for a dark one (the *page's* luminance, not the
desktop's).

### Typography

Pango attributes, so inherently zoom-safe. Out-of-range values are clamped, not
rejected — and so is an unrecognised *line style*: a `heading_underline = "zigzag"`
falls back to that key's default rather than failing the theme.

| Key | Type | Default | Range |
|-----|------|---------|-------|
| `heading_scale` | `[f64; 5]` | `[2.2, 1.8, 1.48, 1.2, 1.0]` | each `0.25`–`8.0` |
| `heading_weight` | `i32` | `700` | `100`–`1000` |
| `bold_weight` | `i32` | `700` | `100`–`1000` |
| `supsub_scale` | `f64` | `0.72` | `0.25`–`8.0` |
| `superscript_rise` | `i32` | `4` | `-64`–`64` |
| `subscript_rise` | `i32` | `-2` | `-64`–`64` |

### Decoration geometry

Design-time pixels at zoom 1.0, scaled on apply. Typed `i32` and clamped, so a
value cannot carry punctuation into a generated CSS rule.

| Key | Type | Default | Range |
|-----|------|---------|-------|
| `heading_space_below` | `[i32; 5]` | `[4, 4, 2, 2, 2]` | each `0`–`400` |
| `heading_space_above` | `[i32; 5]` | `[0, 0, 0, 0, 0]` | each `0`–`400` |
| `heading_band_radius` | `i32` | `0` | `0`–`400` |
| `blockquote_bar_width` | `i32` | `3` | `0`–`400` |
| `blockquote_text_gap` | `i32` | `10` | `0`–`400` |
| `list_step` | `i32` | `28` | `4`–`400` |
| `list_item_gap` | `i32` | `8` | `0`–`400` |
| `rule_space` | `i32` | `4` | `0`–`400` |
| `table_cell_padding_v` | `i32` | `4` | `0`–`400` |
| `table_cell_padding_h` | `i32` | `10` | `0`–`400` |
| `table_border_width` | `i32` | `1` | `0`–`400` |
| `table_cell_radius` | `i32` | `0` | `0`–`400` |

The five-element arrays are indexed h1 · h2 · h3 · h4 · h5-and-deeper. The renderer
maps h6-and-deeper onto the h5 tag, so no theme can differentiate h6 from h5 (that
fold applies on every surface — preview and outline alike).
