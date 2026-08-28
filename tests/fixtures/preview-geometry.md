# Preview geometry probe

Control paragraph: this ordinary body paragraph is deliberately long so that it must soft-wrap at any pane width narrower than a few thousand pixels, giving every heading case below a same-width control to be judged against.

## A short heading

# A very long heading made entirely of ordinary words that should soft wrap to the preview pane rather than run past its right edge at any width

## Another long heading with ordinary words that must wrap the same way the level one heading above it does at the same pane width

### Third level long heading with ordinary words that must also wrap to the pane and not overflow it horizontally at a narrow width

# Supercalifragilisticexpialidociousantidisestablishmentarianismpneumonoultramicroscopicsilicovolcanoconiosis

Control paragraph with the same unbreakable run: Supercalifragilisticexpialidociousantidisestablishmentarianismpneumonoultramicroscopicsilicovolcanoconiosis

- # A long heading inside a list item that has enough ordinary words in it to need wrapping at a narrow pane width

> # A long heading inside a blockquote that has enough ordinary words in it to need wrapping at a narrow pane width

## Blockquote span shapes

### One quote, two paragraphs

> First paragraph of a two-paragraph quote, long enough to soft wrap at a narrow pane width and so occupy more than one display row.
>
> Second paragraph of the same quote. The accent bar must read as one continuous run from the first paragraph's top to this paragraph's bottom, with no seam and no tile restart between them.

### Two quotes separated by a blank line

> First quote, standing alone.

> Second quote, standing alone. The bar must restart here — these are two quotes, not one.

### A quote interrupted by a non-quote block

> Quote before the interruption.

An ordinary paragraph between the two quoted regions.

> Quote after the interruption.

### Nested quotes

> Outer quote, first paragraph.
>
> > Inner quote nested one level deeper, long enough to wrap at a narrow pane width.
> >
> > Inner quote, second paragraph.
>
> Outer quote, last paragraph, after the nested one closes.

### A quote containing a list

> Quote paragraph before the list.
>
> - First list item inside the quote
> - Second list item inside the quote
> - Third list item inside the quote
>
> Quote paragraph after the list.

### A quote containing a fenced code block

> Quote paragraph before the fence.
>
> ```rust
> fn main() {
>     println!("inside a quoted fence");
> }
> ```
>
> Quote paragraph after the fence.

### A quote containing a heading

> ## A heading inside a quote
>
> The paragraph that follows it, so the bar must span the heading and this paragraph as one run.

### A single-line quote

> One line, one row.

Trailing paragraph, so the last quote is not the end of the document.
