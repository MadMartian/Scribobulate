# Outside Doc

This document lives **outside** `tests/fixtures/`, so every link that reaches it from
a document inside that folder crosses the containment boundary (TDD §19.2/19.3).

If you are reading this in the app, note *how* you got here: it should only be
reachable with "Load Unsafe Linked Documents" turned ON for the linking tab.

## A heading to land on

Used as a `#fragment` target when checking that containment is evaluated before the
fragment scroll, not after.
