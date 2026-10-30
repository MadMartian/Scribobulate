# Line-break rendering

These three lines are separated by a single newline each (soft breaks), so they
must flow as one line of text and wrap to the pane:

Line one
Line two
Line three

This line ends in two trailing spaces  
so this one starts on a new rendered line.

This line ends in a backslash\
so this one starts on a new rendered line too.

- A list item whose source wraps
  onto a second line flows as one line.
- A list item with a backslash break\
  keeps its second line.

> A blockquote written across
> two source lines flows as one line.

Heading across
two lines
---------

[Jump to the two-line heading](#heading-across-two-lines)

The paragraph above ended with a blank line, so this is a separate, gap-separated
paragraph.
