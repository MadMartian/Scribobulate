# Code blocks

Every shape a code block takes, in one document, so a copy-as-Markdown pass
(TDD 2.8h) can drive all four from one file. The paragraph before a block and
the paragraph after it are what a *crossing* selection needs.

Prose before the fenced block.

```rust
fn main() {
    let greeting = "syntax highlighted";
    let answer = 42;
    println!("{greeting}: {answer}");
}
```

Prose after the fenced block.

An unlanguaged fence, so the copy path is exercised with no syntax highlighting
in play:

```
plain line one
plain line two
plain line three
```

An indented (four-space) block — its indent lives in the source only, never in
the rendered buffer:

    indented line one
    indented line two

Inside a blockquote, where every line also carries a `> ` marker:

> ```
> quoted line one
> quoted line two
> ```

And inside a list item, where the block carries the item's continuation indent:

- an item with a block under it

  ```sh
  echo "in a list"
  echo "second line"
  ```

- a following item, so the list does not end at the block

A **one-line** block, whose card is too short for a corner affordance's full
inset — the shortest block that must still offer one (TDD 2.3b):

```sh
echo "one line only"
```

And a block **taller than the preview pane**, so an affordance pinned to the
block's real first line would be unreachable from the middle of it (TDD 2.3b):

```rust
let line_1 = 1; // a line of a very long block
let line_2 = 2; // a line of a very long block
let line_3 = 3; // a line of a very long block
let line_4 = 4; // a line of a very long block
let line_5 = 5; // a line of a very long block
let line_6 = 6; // a line of a very long block
let line_7 = 7; // a line of a very long block
let line_8 = 8; // a line of a very long block
let line_9 = 9; // a line of a very long block
let line_10 = 10; // a line of a very long block
let line_11 = 11; // a line of a very long block
let line_12 = 12; // a line of a very long block
let line_13 = 13; // a line of a very long block
let line_14 = 14; // a line of a very long block
let line_15 = 15; // a line of a very long block
let line_16 = 16; // a line of a very long block
let line_17 = 17; // a line of a very long block
let line_18 = 18; // a line of a very long block
let line_19 = 19; // a line of a very long block
let line_20 = 20; // a line of a very long block
let line_21 = 21; // a line of a very long block
let line_22 = 22; // a line of a very long block
let line_23 = 23; // a line of a very long block
let line_24 = 24; // a line of a very long block
let line_25 = 25; // a line of a very long block
let line_26 = 26; // a line of a very long block
let line_27 = 27; // a line of a very long block
let line_28 = 28; // a line of a very long block
let line_29 = 29; // a line of a very long block
let line_30 = 30; // a line of a very long block
let line_31 = 31; // a line of a very long block
let line_32 = 32; // a line of a very long block
let line_33 = 33; // a line of a very long block
let line_34 = 34; // a line of a very long block
let line_35 = 35; // a line of a very long block
let line_36 = 36; // a line of a very long block
let line_37 = 37; // a line of a very long block
let line_38 = 38; // a line of a very long block
let line_39 = 39; // a line of a very long block
let line_40 = 40; // a line of a very long block
let line_41 = 41; // a line of a very long block
let line_42 = 42; // a line of a very long block
let line_43 = 43; // a line of a very long block
let line_44 = 44; // a line of a very long block
let line_45 = 45; // a line of a very long block
let line_46 = 46; // a line of a very long block
let line_47 = 47; // a line of a very long block
let line_48 = 48; // a line of a very long block
let line_49 = 49; // a line of a very long block
let line_50 = 50; // a line of a very long block
let line_51 = 51; // a line of a very long block
let line_52 = 52; // a line of a very long block
let line_53 = 53; // a line of a very long block
let line_54 = 54; // a line of a very long block
let line_55 = 55; // a line of a very long block
let line_56 = 56; // a line of a very long block
let line_57 = 57; // a line of a very long block
let line_58 = 58; // a line of a very long block
let line_59 = 59; // a line of a very long block
let line_60 = 60; // a line of a very long block
```

Prose after the long block.
