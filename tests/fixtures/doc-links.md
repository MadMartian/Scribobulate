# Document Links

Every link below is a case in TDD §19. Click them in the preview; none of them
should ever reach an external browser or file handler.

## Should navigate (in-folder, Markdown)

- [A Markdown sibling](second-doc.md) — opens as a new tab in this window
- [The same sibling again](./second-doc.md) — a second spelling of one file; must
  reuse the tab the first link opened, not duplicate it
- [A sibling with a fragment](anchors.md#deep) — opens the target *and* scrolls it
  so that heading is at the top
- [A fragment matching no heading](anchors.md#no-such-heading) — still opens, scrolls
  nowhere, says nothing

## Should be refused, with a visible notice

- [Traversal out of the folder](../other-fixtures/outside-doc.md) — out of bounds
- [A symlink pointing out of the folder](escapes-via-symlink.md) — out of bounds; the
  refusal must be decided on the *resolved* target, not the link text
- [A home-directory path](~/notes.md) — `~` expands first, so this is an absolute path
  outside the folder: **Refused**, never "not found"
- [A non-Markdown local file](logo.png) — "Not a Markdown document"; never handed to
  the external launcher
- [An explicit file:// URL](file:///etc/hostname) — disallowed scheme, always
- [A missing sibling](no-such-file.md) — "Link target not found", which must read
  differently from a containment refusal

## Should still launch externally

- [An https link](https://example.com) — the external-URI path is unchanged
- [A mailto link](mailto:nobody@example.com) — likewise

## Literal tilde

- [A directory literally named ~](./~/x.md) — `~` anywhere but position 0 is an
  ordinary path component, so this is an in-folder reference (and, since the
  directory does not exist, a "not found", not a refusal)
