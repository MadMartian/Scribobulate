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
