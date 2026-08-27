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
is not a hypothetical: it once made user theme overrides unreachable on Windows outright, because
the base directory could never be resolved there.

### Key resolution

Each key resolves by the first source that supplies it. A key that varies by
heading level or by list depth is consulted in its **specific** form before its
bare form *within each source*, so a theme's own bare key still outranks
`[themes.system]`'s specific one:

| Order | Source |
|-------|--------|
| 1 | the selected theme's specific key (`heading_color_h2`, `list_marker_color_3`) |
| 2 | the selected theme's bare key (`heading_color`, `list_marker_color`) |
| 3 | `[themes.system]`'s specific key |
| 4 | `[themes.system]`'s bare key |
| 5 | the key's own default — a fixed value, a value derived from the base colours, or a probe of the desktop GTK theme |

A theme therefore states only what makes it distinctive and derives the rest.

One consequence is worth stating, because it is the same rule read from the other
end: a user file merges over a built-in **per key** (see the search path above), so
displacing a built-in's `heading_color_h1` takes a `heading_color_h1` of your own.
A bare `heading_color` does not displace it, exactly as it does not displace
`[themes.system]`'s bare key.

The desktop probe (source 5's colour half) reads these base Adwaita named colours via
`StyleContext::lookup_color`: `theme_bg_color`, `theme_fg_color`, `theme_base_color`,
`theme_text_color`, `theme_selected_bg_color`, `theme_selected_fg_color`, `borders`.
The libadwaita-only names (`view_bg_color`, `accent_bg_color`, `card_bg_color`) are
used only as fallback-chain entries where the active theme defines them.

**An unrecognised key is ignored and logged at `warn`**, naming the theme id and the
key. A themes file is hand-written, so a misspelling is the ordinary failure mode;
silence would make a key that never applies indistinguishable from one that applied
and did nothing.

### How a `*_sprite` key resolves

A sprite key names an image, and **where that image comes from is decided by which
file states the key** — not by the key and not by the theme:

| The key is stated in… | It names… |
|---|---|
| a `themes.toml` **on disk** (any search-path row) | a file **relative to that file's own directory**, admitted only if it passes every check in `crate::sprite::resolve`: no absolute path, no `..`/root/prefix component, canonicalised and contained (a symlink cannot point out), an allowlisted extension (`png`/`webp`/`jpg`), and under the size cap |
| the **compiled-in** `data/themes.toml` (a built-in theme) | an image **compiled into the binary** (`include_bytes!`, `src/sprite.rs`'s embedded table). No file, no install step, no search path — and no validation, because the bytes are this project's own and were fixed at build time |

The split exists because a built-in theme is compiled in precisely so it renders on
a host with nothing on disk; a shipped decoration that needed an installed asset
would be present on a packaged host and absent on every fresh install, developer
build and macOS bundle, **silently**, since an unresolved sprite is inert by design.

Two consequences worth stating, both load-bearing:

- A built-in theme **cannot** name an image that is not in the embedded table — a
  reference the table does not carry is refused and logged, exactly like a user
  theme naming a missing file. A theme file on disk, conversely, cannot reach the
  embedded table; it may only name files beside itself.
- The installed copy of `themes.toml` is itself read as a themes file on disk, so a
  packaging omission of `data/sprites/` costs a log line and nothing else: the
  refused override leaves the compiled-in sprite standing.

### Key naming

A key's suffix states its type, so a theme author can tell what a key takes without
consulting the tables below.

| Suffix | Type | Meaning |
|--------|------|---------|
| `_color` | colour | Every colour-valued key, except the pair spelling below. |
| `_bg` / `_fg` | colour | A text surface's fill and its ink — `mark_bg`/`mark_fg`, `selection_bg`/`selection_fg`. A surface with no ink key of its own keeps a lone `_bg`. The page's own pair is spelled `background`/`foreground`. |
| `_sprite` | sprite path | An image drawn or tiled in place of a flat value. Its value is a path, resolved as above — never an icon name and never an absolute path. |
| `_glyph` | string | A character drawn in place of a marker the engine would otherwise draw. |
| `_h1` … `_h5` | (the key's own type) | Narrows the key to one heading level. |
| `_2`, `_3` | (the key's own type) | Narrows the key to one list-nesting depth. |

Colours are strings parsed as `RGBA`: `#RRGGBB`, `#RRGGBBAA`, or a CSS colour name.

**No key takes an array.** Anything that varies by heading level or list depth is
stated as a suffixed key, one value each.

**A sprite outranks its flat sibling.** Where a decoration has both a `_sprite` and
a colour (or a `_glyph`), a theme that states both gets the sprite and the other
value is ignored — not composited under it, and not used to tint it. Every renderer
states that with an explicit branch rather than leaving it to paint order: filling
first and tiling over looks identical for an opaque sprite and lets the flat colour
bleed through a transparent one, which is a defect that appears only for the sprites
nobody tested.

**Metrics are design-time pixels at zoom 1.0**, typed `i32` and scaled on apply, so
a value cannot carry punctuation into a generated CSS rule. Out-of-range values are
clamped rather than rejected, and so is an unrecognised *line style*: a
`heading_underline = "zigzag"` falls back to that key's default rather than failing
the theme.

### Heading keys are per level

Every key in the Headings table below may be stated in either form:

| Form | Applies to |
|------|------------|
| `heading_color` | every level |
| `heading_color_h1` … `heading_color_h5` | that level only, overriding the bare form |

**There are five levels, not six**: h1 · h2 · h3 · h4 · h5-and-deeper. The renderer
maps h6 onto the h5 tag before a tag is ever chosen — on every surface, preview and
outline alike — so no theme can differentiate them and no key spells `_h6`. Where a
key's default varies by level, the table gives it as five values in order.

The **table header is not a heading level**: it reads `table_head_fg`, which falls
back to the bare `heading_color` and never to a per-level key.

### `[themes.<id>]` keys

Every key is optional; omitting one means "derive or inherit it".

#### Base

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `name` | `string` | the theme id | Display name in the theme chooser. |
| `symbol` | `string` | — | Decorative symbol shown left of the name in the picker. |
| `background` | colour | derived | Page background. One of the three base colours every derived colour follows from. |
| `foreground` | colour | derived | Body text colour. |
| `accent_color` | colour | derived | Accent colour. |
| `font_family` | `string` | derived | Body font stack. Sanitised, and always terminated with a generic family. |
| `syntect_theme` | `string` | by page luminance | Syntax-highlighting theme for code blocks. `InspiredGitHub` on a light page, `base16-ocean.dark` on a dark one — the *page's* luminance, not the desktop's. |

#### Headings

Each key here also takes an `_h1` … `_h5` form (see above).

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `heading_color` | colour | body foreground | Heading ink. A link inside a heading still takes the link colour. |
| `heading_font` | `string` | `font_family` | Heading font stack. Same sanitisation as `font_family`. |
| `heading_scale` | `f64` | 2.2 · 1.8 · 1.48 · 1.2 · 1.0 | Size relative to the body. Clamped `0.25`–`8.0`. |
| `heading_weight` | `i32` | `700` | Clamped `100`–`1000`. |
| `heading_overline` | `"none"` \| `"single"` | `"none"` | A rule ABOVE the heading text, drawn in the heading's own ink. `"double"`/`"wavy"` are accepted and clamped to a single line (Pango's overline has no other value). ⚠️ There is deliberately **no** overline colour key at any level: GTK 4.6 double-frees a text run carrying both a coloured overline and a coloured underline, and a link inside a heading is such a run (`src/theme.rs`, `HeadingRule`). |
| `heading_underline` | `"none"` \| `"single"` \| `"double"` \| `"wavy"` | `"none"` | A rule BELOW the heading text — the decoration line under the glyph run, not a column-width divider. |
| `heading_underline_color` | colour | heading ink | The below-rule's colour. Omitted, the line follows the heading's own foreground. |
| `heading_band_color` | colour | — | The band drawn behind a heading's text. Absent ⇒ that level carries no band. The band spans the **content column**, and survives soft-wrap as one continuous band. |
| `heading_band_gradient_to_color` | colour | — | A second stop: the band becomes a vertical gradient from its own fill down to this colour. Ignored where the level states no fill. |
| `heading_band_sprite` | sprite path | — | A sprite **tiled at its natural size** across the band, in place of its fill. Outranks the fill and the gradient. |
| `heading_band_radius` | `i32` | `0` | Corner radius of the band. Clamped `0`–`400`. |
| `heading_band_padding` | `i32` | `12` | Space between the band's edge and the heading text inside it, each side. Clamped `0`–`400`. ⚠️ Non-zero by default, unlike every other decoration key: padding is part of drawing a band correctly rather than a flourish. Applied **only to a level that carries a band**, so a theme that bands nothing is untouched by it whatever its value. |
| `heading_space_above` | `i32` | `0` (every level) | Space above the heading line. Clamped `0`–`400`. |
| `heading_space_below` | `i32` | 4 · 4 · 2 · 2 · 2 | Space below the heading line. Clamped `0`–`400`. |

#### Body and inline text

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `bold_weight` | `i32` | `700` | Clamped `100`–`1000`. |
| `supsub_scale` | `f64` | `0.72` | Superscript/subscript size. Clamped `0.25`–`8.0`. |
| `superscript_rise` | `i32` | `4` | Clamped `-64`–`64`. |
| `subscript_rise` | `i32` | `-2` | Clamped `-64`–`64`. |
| `strikethrough_color` | colour | the struck text's ink | The colour of the line through `~~text~~`. Omitted, it follows the struck text's own foreground. Reaches the body tag, the table-cell span and both export sinks alike. |
| `mark_bg` | colour | `#fff59d_88` | Background band behind `==marked==` text. |
| `mark_fg` | colour | body foreground | Ink for `==marked==` text. Omitted, marked text keeps the body foreground and only its background changes — how a highlighter behaves on paper, and right for any `mark_bg` that is a translucent wash. State it when the band is opaque enough to need its own ink. Reaches the body tag and the table-cell span alike. |
| `code_inline_bg` | colour | derived | Inline code-span background. |
| `code_block_bg` | colour | derived | Fenced code-block background. |

#### Links

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `link_color` | colour | derived | Link ink. |
| `link_underline` | `"none"` \| `"single"` \| `"double"` \| `"wavy"` | `"single"` | A link's underline style. Defaults to the single line the app has always drawn, not to `"none"`. |
| `link_underline_color` | colour | the link colour | The link underline's colour, stated independently of the link's ink. Omitted, the line follows the ink. |

#### Lists

The three bullet keys marked ⓷ also take `_2` and `_3` forms — the bullet at nesting
depth 2, and at depth 3 **and deeper**. Each falls back to the next *shallower*
tier, not to the bare key, so stating only the `_2` form reaches every depth from 2
down. Bullets alone are tiered: a nested numeral is still a numeral and a nested
task box is still a box, where a bullet dot's whole job is to say which level you
are on.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `list_marker_color` ⓷ | colour | widget foreground | Bullet, numeral and task-checkbox ink. Marker only; item text keeps the body foreground. ⚠️ The bare key colours all three marker kinds; its `_2`/`_3` forms are **bullet only**. |
| `list_task_marker_color` | colour | `list_marker_color` | The **task checkbox's** colour, both states. One key rather than one per state: a checked and an unchecked box are the same control in two positions, and the glyph is what carries the state. |
| `list_bullet_glyph` ⓷ | `string` | — | A glyph drawn in place of the bullet dot. Trimmed; refused (falling back to the drawn marker) if empty, over 8 characters, or carrying a control character — over-long is refused rather than cut, since a cut can split a grapheme cluster. |
| `list_ordered_glyph` | `string` | — | A glyph drawn in place of the ordered numeral. ⚠️ This DISCARDS the ordinal — deliberate, and inert unless a theme asks for it. |
| `list_task_glyph` | `string` | — | A glyph drawn in place of the unchecked task box. |
| `list_task_checked_glyph` | `string` | — | A glyph drawn in place of the checked task box. Resolves independently of the unchecked one, so a theme may state either alone. |
| `list_bullet_sprite` ⓷ | sprite path | — | A sprite drawn in place of the bullet dot. |
| `list_ordered_sprite` | sprite path | — | A sprite drawn in place of the ordered numeral. |
| `list_task_sprite` | sprite path | — | A sprite drawn in place of the unchecked task box. |
| `list_task_checked_sprite` | sprite path | — | A sprite drawn in place of the checked task box. |
| `list_step` | `i32` | `28` | Indent added per nesting depth. Clamped `4`–`400`. |
| `list_item_gap` | `i32` | `8` | Space between items. Clamped `0`–`400`. |

#### Blockquote

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `blockquote_bar_color` | colour | derived | The quote's accent bar. |
| `blockquote_bar_sprite` | sprite path | — | A sprite **tiled at its natural size** down the accent bar, in place of the flat colour. ⚠️ The tile is clipped to the bar, so a theme using one wants `blockquote_bar_width` at the tile's own width — a 24px tile in a 4px bar is a 4px slice of a tile. |
| `blockquote_bar_width` | `i32` | `3` | Clamped `0`–`400`. |
| `blockquote_text_gap` | `i32` | `10` | Bar → quoted text. Clamped `0`–`400`. |
| `blockquote_bg` | colour | — | A panel behind quoted text. Absent unless stated: a quote sits on the page background, as it always has. Independent of `blockquote_bar_color` — an accent bar and a panel are two decisions, and a themed bar seeds no panel. |
| `blockquote_fg` | colour | body foreground | The ink quoted **body** text takes on that panel. Re-inks the quote's prose only: a link, a heading or a `==mark==` inside the quote keeps its own colour, because this rides the lowest-priority ink tag in the preview and the cairo pen (rather than the markup) on the page. |

#### Table

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `table_border_color` | colour | derived | Table border. |
| `table_border_width` | `i32` | `1` | Clamped `0`–`400`. |
| `table_head_bg` | colour | derived | Table header background. |
| `table_head_fg` | colour | `heading_color` | The header ROW's text colour. Omitted, the header takes the bare `heading_color` exactly as it always did, and omitting that too leaves it on the body ink. Stating it is what frees `table_head_bg` to be a fill of the theme's own choosing: while the header's ink was the heading's, a header fill had to be picked for legibility against a colour chosen for a different surface. |
| `table_cell_padding_v` | `i32` | `4` | Clamped `0`–`400`. |
| `table_cell_padding_h` | `i32` | `10` | Clamped `0`–`400`. |
| `table_cell_radius` | `i32` | `0` | Clamped `0`–`400`. |

#### Horizontal rule

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `rule_color` | colour | derived | The `---` rule's colour. |
| `rule_sprite` | sprite path | — | A sprite **tiled horizontally at its natural size** across the rule, in place of the flat colour. ⚠️ Unlike every other sprite key, this one changes which WIDGET the rule is: a flat rule is a stock `GtkSeparator` filled by generated CSS, and a GTK CSS `url()` cannot name a sprite compiled into the binary, so a tiled rule is a widget that paints the texture itself. The rule's height becomes the tile's own; `rule_space` still sets the gap around it. |
| `rule_space` | `i32` | `4` | Space above and below the rule. Clamped `0`–`400`. |

#### Selection

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `selection_bg` | colour | derived | Selection background. |
| `selection_fg` | colour | derived | Ink for *selected* text. Omitted, it derives from `selection_bg` plus the page and its ink — whichever of the two reads better on the fill — so a theme cannot strand its own selected text by accident. State it when the derived answer is legible but wrong: contrast is not taste. |

#### Annotations and find

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `annotation_hl_color` | colour | `#FFD133_61` | Annotation highlight overlay. |
| `annotation_chip_bg` | colour | hardcoded amber | CriticMarkup comment gutter chip's fill. Omitted, the chip stays the exact hardcoded amber/white it always was (TDD 18.2). |
| `annotation_chip_fg` | colour | hardcoded white | Ink for the chip's overflow-count numeral. |
| `annotation_chip_sprite` | sprite path | — | A sprite drawn in place of the flat chip fill. No expression in the PDF's inline Pango markup — a stated scope limit (TDD 18.19). |
| `find_hl_all_color` | colour | `#f6d32d` | Highlight for all find matches. |
| `find_hl_current_color` | colour | derived | Highlight for the current find match. |
