#!/usr/bin/env python3
"""Generate a LARGE disclosure fixture for TDD 2.26i.

2.26i's excursion is a value CLAMP, so it is proportional to document height and is
not observable on a 3 KB file — the rubric says to use a large one, and this makes
"large" reproducible instead of ad hoc. Defaults give ~7.8k lines / 96 KB, enough
that a full re-render cannot hide inside one frame interval, which matters because
the check is read by capturing the scrollbar thumb between frames.

A generator rather than a committed fixture: a 96 KB file is a diff nobody reads,
and the next question may want 30,000 lines rather than 7,811.

    python3 gen-big-disclosure.py > /tmp/bigdisclosure.md
    python3 gen-big-disclosure.py --above 1000 --hidden 1000 --below 6000 > bigger.md

Written by the macOS seat while running 2.26i; landed here so the next run of that
rubric measures the same shape.
"""
import argparse

p = argparse.ArgumentParser()
p.add_argument("--above", type=int, default=300)
p.add_argument("--hidden", type=int, default=300)
p.add_argument("--below", type=int, default=1500)
a = p.parse_args()

out = [
    "# Big disclosure fixture",
    "",
    "Generated fixture for TDD 2.26i. The block sits mid-document so a reader can",
    "park on its summary line with bulk both above and below it.",
    "",
]
for n in range(1, a.above + 1):
    out += [f"## Above {n}", "", f"Prose above the block, section {n}.", ""]
out += ["## Mid block", "", "<details>", f"<summary>Mid block — {a.hidden} hidden lines</summary>", ""]
for n in range(1, a.hidden + 1):
    out += [f"hidden {n:03d}", ""]
out += ["</details>", ""]
for n in range(1, a.below + 1):
    out += [f"## Below {n}", "", f"Prose below the block, section {n}.", ""]
print("\n".join(out))
