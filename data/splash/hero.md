# Scribobulate

**Read the Markdown your agents write** — live, full-fidelity, and native.

> A viewer *and* an editor for the plans, notes, and reports your AI agents
> generate — re-rendering the instant a file changes on disk.

## Why it's fast

Native GTK widgets on the CPU. **0 MiB VRAM, ~80 MiB RAM** — no browser engine,
no GPU pipeline. It just draws.

| Feature          | Scribobulate | Browser-based viewers |
| ---------------- | :----------: | :-------------------: |
| GPU memory       |    0 MiB     |     300+ MiB          |
| Live reload      |      ✅       |        rarely         |
| Native window    |      ✅       |          ❌           |
| Conflict-safe    |      ✅       |          ❌           |

## Syntax highlighting

```rust
fn render(doc: &Markdown) -> Widget {
    let view = PreviewView::new();
    view.set_theme(Theme::Synthwave);   // reading themes are pure data
    view.build(doc)                     // 0 MiB VRAM, all CPU
}
```

## Task lists that actually track

- [x] Render tables, code, and images
- [x] Live-reload on disk change
- [x] Reading themes — Sepia, Synthwave, Terminal
- [ ] Take over the world

Hover a [link](https://github.com) to see where it really leads — then open your
outline, split the editor, and keep writing.
