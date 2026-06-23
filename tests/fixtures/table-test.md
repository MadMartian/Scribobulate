# Table Stress Test

Tables in many sections, plus a deep outline, so you can rapid‑far‑jump and force
the validation sweeps that used to blank the view. Try selecting **inside** cells,
clicking the **links**, right‑clicking (Copy / Insert Emoji), and resizing the window
(narrow → cells should wrap). Also try wrapping a selection in a **code block** and
**undo/redo**.

## 1. Intro

Body text. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod
tempor incididunt ut labore et dolore magna aliqua.

| # | Anti‑pattern | Layer |
|---|--------------|-------|
| 22 | Forcing GtkTextView layout validation inside the snapshot/draw or size‑allocate path | Core GTK4 |
| 23 | Embedding height‑for‑width block content as a widget at a GtkTextChildAnchor | Core GTK4 |
| 24 | Relying on the system theme to paint a GtkTreeExpander's disclosure chevron | Core GTK4 |

More body text after the table.

### 1.1 Subsection

> A blockquote near a table, to confirm both coexist without flicker.

## 2. Wide Table

A table whose cells hold long text, so columns must wrap when the pane is narrow.

| Column A | Column B has a much longer header to push width | C |
|----------|--------------------------------------------------|---|
| short | This cell contains a long sentence that should wrap onto multiple lines when the available column width is small, exercising height‑for‑width inside the custom widget. | x |
| `code` | A [link to example](https://example.com) plus **bold** and *italic* mixed inline content in one cell. | y |
| more | Another long paragraph of filler text to make the row tall and to verify the accent borders span the full row height even when one cell is much taller than its neighbours. | z |

### 2.1 Deep Dive

Body text between tables.

```rust
// A code block — try wrapping a selection in another fenced block via Format,
// then Undo/Redo, and watch for snapshot/allocation warnings.
fn main() {
    println!("hello");
}
```

## 3. Links Table

| Site | URL |
|------|-----|
| Example | [example.com](https://example.com) |
| Example Org | [example.org](https://example.org) |
| Rust | [rust-lang.org](https://www.rust-lang.org) |

## 4. Numbers

| Metric | Value | Notes |
|--------|-------|-------|
| Width | 814 | content column |
| Rows | 3 | body |
| Cols | 3 | incl. header |

### 4.1 Sub

Filler. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.

## 5. Another Wide One

| Feature | Description |
|---------|-------------|
| Selection | Each cell is a selectable label; drag within a cell to select its text. Tables are anchored islands (one cell at a time). |
| Links | Cells with links open them on click; the link colour is preserved. |
| Wrapping | Narrow the window and the cells re‑wrap; the table never needs a horizontal scrollbar. |

## 6. Bottom

Jump here from section 1 via the outline to force the longest far‑scroll, then back
up, repeatedly — the table lines get validated each time.

| End | Of | Document |
|-----|----|----|
| a | b | c |

## 7. Closing

The end.
