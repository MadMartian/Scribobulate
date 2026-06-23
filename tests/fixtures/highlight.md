# Highlight (`==mark==`) demo

The quick brown fox is ==definitely radioactive== today — hazmat suit recommended.

Highlight allows internal spaces: this is ==a marked phrase with spaces== mid-sentence,
and it composes beside **bold**, *italic*, and `code` on the same line.

## Inside containers

- A bulleted item with ==a highlighted span== inside it
- Another item — ==toxic green== on Synthwave

> A blockquote containing ==a highlighted quote==, to show container parity holds
> inside block quotes as well as top-level prose.

| Column A     | Column B               |
|--------------|------------------------|
| plain cell   | ==a highlighted cell== |
| ==mark A==   | normal cell            |

## Must stay literal (not highlights)

Comparisons and operators are NOT marks: `a == b`, `x -= 1`, `y == 2`, and a
`== spaced ==` fence (a space just inside the markers) all render verbatim.

## Toggle / edit

Select any phrase and press **Ctrl+Alt+H** (or the **H** toolbar button) to wrap it
in `==…==`; press it again to remove the marks.
