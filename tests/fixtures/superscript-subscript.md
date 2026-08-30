# Super / Sub / Strike

Einstein: E=mc^2^ should raise the 2, and 4^th^ should raise "th".

Water: H~2~O should lower the 2.

Multi-tilde line: H~2~O and CO~2~ must BOTH lower their 2 (regression — ScrAP-66).

Strikethrough: ~~struck text~~ and ~~several words gone~~ should render struck.

Fence wrapping markup: ~~a **bold** b~~ must strike ALL of "a bold b" with "bold" still bold,
and ==a *em* b== must highlight all of it with "em" still italic. A fence over a link,
~~see [the docs](https://example.com/) now~~, strikes the caption too.

Fence over a line break: ~~struck across
this soft break~~ strikes both lines.

Refused shapes (must stay LITERAL, `~~` visible): interleaved ~~a **b~~ c** (the closing
fence sits inside the bold), a fence whose halves are really code `x ~~ y` b~~, and a fence
that would have to span two cells:

| ~~a | b~~ |
|---|---|
| c | d |

Literal fallbacks (must stay literal): 2^10 has no close, 1~2 approx, a^b c^d has a space.

All together: E=mc^2^ and H~2~O and ~~struck~~.
