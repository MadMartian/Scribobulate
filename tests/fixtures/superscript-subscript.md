# Super / Sub / Strike

Einstein: E=mc^2^ should raise the 2, and 4^th^ should raise "th".

Water: H~2~O should lower the 2.

Multi-tilde line: H~2~O and CO~2~ must BOTH lower their 2 (regression — ScrAP-66).

Strikethrough: ~~struck text~~ and ~~several words gone~~ should render struck.

Nested-in-strike limitation: ~~a **bold** b~~ shows the `~~` literally — expected, not a regression.

Literal fallbacks (must stay literal): 2^10 has no close, 1~2 approx, a^b c^d has a space.

All together: E=mc^2^ and H~2~O and ~~struck~~.
