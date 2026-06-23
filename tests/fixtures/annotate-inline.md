# Annotate inline-construct regression fixture

Exhaustive manual verification checklist here. This complements
`cargo test` and is **not** automatable, so selecting a single plain
word — like verification — in this paragraph must annotate ONLY that
word and never run to the end of the block.

Second paragraph drives the app via `xdotool` and `import`:

```sh
xdotool selectwindow
import -window "$WID" out.png
```

Third paragraph closes the fixture with a **bold** run and some
`inline code` so wrapping across constructs stays well-formed.
