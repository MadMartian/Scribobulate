# List rendering (all three bullet types)

Fixture for TDD 2.4a — the marker sits alone in a left column and every line of an
item's text (first line and wrapped continuations) aligns under the item text in the
content column, identically for unordered, ordered, and task lists, at every depth.

## Unordered

- Short item.
- A deliberately long unordered item that will wrap across more than one visual line
  so we can confirm the wrapped continuation aligns under the item text, not under or
  left of the bullet marker.
- Item whose source is written across several lines
  that in the source are separate physical lines
  yet should read as one flowing item under the marker.
- Last short item.

## Ordered

The renderer always renumbers from 1, so this list has enough items to reach two-digit
markers (`10.`, `11.`) and check whether a wider marker throws off the content column.

1. Short numbered item.
2. A deliberately long ordered item that will wrap across more than one visual line so
   we can confirm ordered lists behave identically to unordered ones under the shared
   hanging-indent tags.
3. Third item.
4. Fourth item.
5. Fifth item.
6. Sixth item.
7. Seventh item.
8. Eighth item.
9. Ninth item (single digit — last of the one-digit markers).
10. Tenth item — first two-digit marker; check the content column still lines up.
11. Eleventh item, two digits, long enough to wrap so we can confirm the continuation
    still aligns under the item text at the wider marker width.

## Task list

- [ ] Unchecked task, short.
- [x] Checked task that is long enough to wrap onto a second visual line so we can see
  where the continuation of a task item lands relative to its checkbox and text.
- [ ] Another unchecked task.
- [x] Final checked task.

## Nested (mixed depths and types)

- Top-level unordered item.
  - Nested unordered item long enough to wrap and confirm the hanging indent holds at
    nesting depth two.
  1. Nested ordered item under an unordered parent.
  2. Second nested ordered item.
- Back to top level.
  - [ ] Nested task item.
  - [x] Nested checked task item.

## Loose item

- First paragraph of a loose item.

  Second paragraph, separated by a blank line, still part of the same item — it must
  align at the SAME content margin as the first paragraph (the old outdent is gone).
- A normal item after the loose one.

## In a container — lists inside a blockquote (TDD 2.4a container clause)

Every list below must sit WHOLLY inside the quote: each marker draws to the RIGHT of
the quote's accent bar, never on or left of it, and the items indent from the QUOTE's
text margin — not from the body margin. A list that is indented on the right (by the
quote) but not on the left reads lopsided; that was the ScrAP-121 defect.

> Quoted paragraph, for reference — the items below start from THIS margin.
>
> - Quoted unordered item.
> - A long quoted item that wraps across more than one visual line, so we can confirm a
>   wrapped continuation stays inside the quote at the item's content margin.
>   - Nested quoted item — each level steps in by exactly one level's worth from the
>     level above, no more (stacking the parent's indent onto it was the ScrAP-121
>     corollary defect, which stranded the marker far left of its own text).
>
> 1. Quoted ordered item.
> 2. Second quoted ordered item — numbers still right-align inside the quote.
>
> - [ ] Quoted unchecked task — the checkbox draws inside the quote.
> - [x] Quoted checked task.
