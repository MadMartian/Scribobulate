# Disclosure fixture

Every `<details>` shape the renderer must handle, in one document.

## Ordinary collapsed block

<details>
<summary>Show the ASCII fallback</summary>

Body text before the code.

```text
  +--------+
  | fallback |
  +--------+
```

- a list item
- another, with **bold** and `code`

| col | col |
|-----|-----|
| a   | b   |

</details>

Prose after the first block, so a copy can start above it and end below it.

## Marked open

<details open>
<summary>Already expanded</summary>

This body is visible without any user action, and can still be collapsed.

</details>

## Two siblings

<details>
<summary>Sibling one</summary>

Only sibling one's body.

</details>

<details>
<summary>Sibling two</summary>

Only sibling two's body.

</details>

## Nested

<details>
<summary>Outer</summary>

Outer body.

<details>
<summary>Inner</summary>

Inner body.

</details>

Outer body after the inner block.

</details>

## Headings inside a collapsed block

<details>
<summary>Hidden sections</summary>

### Hidden section one

Prose under hidden section one.

### Hidden section two

Prose under hidden section two.

</details>

<!-- Filler below, so a reader can sit well past the blocks above and watch the
     reading position hold across a toggle (TDD 2.26h). -->

## Filler 1

Filler prose 1.

## Filler 2

Filler prose 2.

## Filler 3

Filler prose 3.

## Filler 4

Filler prose 4.

## Filler 5

Filler prose 5.

## Filler 6

Filler prose 6.

## Filler 7

Filler prose 7.

## Filler 8

Filler prose 8.

## Filler 9

Filler prose 9.

## Filler 10

Filler prose 10.

## Mid-document collapsed block

<details>
<summary>Mid-document block — 20 hidden lines</summary>

hidden 01

hidden 02

hidden 03

hidden 04

hidden 05

hidden 06

hidden 07

hidden 08

hidden 09

hidden 10

hidden 11

hidden 12

hidden 13

hidden 14

hidden 15

hidden 16

hidden 17

hidden 18

hidden 19

hidden 20

</details>

## Filler 11

Filler prose 11.

<!-- A table BELOW the mid-document block, deliberately. Rubric 2.26j needs a widget
     that must SURVIVE a splice as the same live object rather than be rebuilt by it:
     everything inside a block is re-rendered when it toggles, so a table in a hidden
     body cannot answer that question. This one sits after the block, so toggling the
     block above it must leave this exact widget in place with its anchor intact. -->

| Survivor | Why it is here |
|---|---|
| below the fold | it must not be rebuilt when the block above it toggles |
| anchored | it holds a `U+FFFC` the copy map has to keep accounting for |

A [link below the fold](https://example.invalid/below-the-fold) — rubric 2.26j asks
that activating it opens **its own** target after a toggle above it, which is the
half a stale link span fails silently: the span still resolves, still looks like a
link, and carries the URL of whatever now occupies those offsets.

## Filler 12

Filler prose 12.

## Filler 13

Filler prose 13.

## Filler 14

Filler prose 14.

## Filler 15

Filler prose 15.

## Filler 16

Filler prose 16.

## Filler 17

Filler prose 17.

## Filler 18

Filler prose 18.

## Filler 19

Filler prose 19.

## Filler 20

Filler prose 20.

## Malformed shapes

**Last on purpose.** An unclosed `<details>` must not swallow the remainder of the
document (TDD 2.26d) — and while that is exactly what these shapes exist to check,
putting them anywhere but the end would make every check above them depend on that
one behaviour being correct.

<details>

A `<details>` with no `<summary>` — the label falls back to "Details".

</details>

<details>
<summary>No blank lines around the body</summary>
This line is not separated by a blank line, so CommonMark keeps it literal.
</details>

<details>
<summary>Never closed</summary>

An unclosed `<details>` must not swallow the rest of the document.

## After the unclosed block

This heading and this paragraph are still part of the document.
