# Plan: Suggested-edit colours on the page

## Problem

A suggested edit (PRODUCT.md vocabulary) reads on the page like any highlight: the same
wash over its visible text, whatever kind it is. A reader cannot tell an addition from a
deletion without opening its card, and a replacement shows only its new text, so what it
would replace is invisible until the card is opened.

Each kind should read at a glance, the way tracked changes read in a word processor:
what would come in the theme's **addition colour**, what would go struck through in its
**deletion colour**, and a replacement as both.

### Root cause

Extraction (`annotate::scan`) keeps one visible run per construct, and a replacement's is
its new side only; the old side never reaches the buffer. Every suggested edit is then
tagged with the one annotation highlight (`Annotation::highlights_text`), whose colour is
the single theme key `annotation_hl_color`.

## Possible approaches

### ✅ 1. Both sides in the page text, each with its own tag

Extraction keeps a replacement's old side AND its new side as visible text, old first, and
records each as its own run. The renderer tags a deletion's text and a replacement's old
side with a deletion tag (struck through, deletion colours), and an addition's text and a
replacement's new side with an addition tag (addition colours). The table-cell path takes
the same two keys as Pango attributes (POLICY "One theme key, every application path").

Copy, find and scroll sync keep working through the existing cleaned↔original shift map,
which gains one more pair of shifts per replacement; copy additionally drops the
deletion-tagged runs (decision 2 below).

**Pros**: the old side is real text — selectable, findable, wrapped by the text view like any
other run, and present in exports through the same event stream.
**Cons**: the page text of a replacement changes, so every consumer of the cleaned text
(copy, find, scroll sync, fold keys, export, the annotation-refresh fast path) must be
re-checked against the Document Rendering CAM.

### ❌ 2. Draw the old side as chrome beside the new text

Paint the struck old side in `snapshot_layer` without putting it in the buffer. Rejected:
it would not wrap, select or find, and drawing laid-out text outside the buffer is the
anchored-content trap GTK4Rs records against height-for-width content.

## Recommendation

Approach 1. Decisions taken with the operator:

1. **Colours are theme keys, foreground and background each.** Every reading theme carries an
   addition foreground and a deletion foreground; a background for either is optional, and
   when a theme does not set one there is no background at all (no fallback to the
   annotation highlight's wash).
2. **Copying from the page yields the text as it would read once accepted**: a deletion's
   text and a replacement's old side are left out; an addition's text and a replacement's
   new side are copied.
3. Built after Add Follow-up.

## Proposed TDD Rubrics

### A suggested edit reads as the change it proposes
- **Given** a document containing suggested edits of every kind, in body text and in table cells
- **When** it is shown in the preview
- **Then** an addition's text is drawn in the theme's addition colours; a deletion's text is struck through in the theme's deletion colours; a replacement shows its old text struck through in the deletion colours, then its new text in the addition colours — and none of them carries the annotation highlight's wash

### A theme sets each colour, and an unset background is absent
- **Given** a reading theme
- **When** it defines an addition or deletion background, or leaves either out
- **Then** a defined background is drawn behind that kind's text, and an undefined one draws nothing behind it; the foregrounds always come from the theme, and every built-in theme's foregrounds pass the legibility floor on its own page

### Copying a suggested edit copies it as accepted
- **Given** a selection on the page that covers suggested edits
- **When** the reader copies it
- **Then** the clipboard holds the text as it would read once every covered edit were accepted: added and new text in, deleted and old text out

### Accepting or removing changes what the page shows
- **Given** a replacement on the page, showing both its sides
- **When** the reader accepts or removes it
- **Then** the page shows only the side that stays, in the ordinary text colour, and nothing else on the page moves

### An export shows suggested edits as the page does
- **Given** a document with suggested edits
- **When** it is exported to HTML or PDF
- **Then** each reads as it does on the page: coloured, the removed text struck through
