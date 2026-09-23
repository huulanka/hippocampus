#!/usr/bin/env python3
"""Draws the mascot for the README, from the grid the app draws it from.

    python3 scripts/render-readme-art.py

The pixel grid lives in client/src/brand/Mascot.tsx and nowhere else; this
reads it out of that file rather than keeping a copy, so the brain on the
GitHub page is always the one in the app. The output is committed —
nothing builds or runs from it.

It blinks, the way the component does: shut for one beat in seven, a beat
being 900 ms. SMIL rather than CSS, because GitHub serves README images
through a proxy as <img>, and an <img> SVG runs SMIL but not script.
"""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "client/src/brand/Mascot.tsx"
OUT = ROOT / "docs/assets/mascot.svg"

# The .mascot custom properties in client/src/styles/brand.css, resolved
# against tokens.css (--clay, --ember). An <img> cannot read either file.
INK = {
    "O": "#17130f",
    "F": "#d07850",
    "H": "#e3a047",
    "D": "#a3573a",
    "C": "#a3573a",
    "E": "#17130f",
    "W": "#fff6ea",
}

BEAT = 0.9
BEATS = 7


def grid() -> list[str]:
    source = SOURCE.read_text()
    block = re.search(r"const REST = \[(.*?)\];", source, re.S)
    if not block:
        raise SystemExit(f"no REST grid in {SOURCE}")
    return re.findall(r'"([^"]+)"', block.group(1))


def runs(rows: list[str], keep) -> dict[str, list[tuple[int, int, int]]]:
    """Horizontal runs of one colour, so a row is a few rects, not 26."""
    out: dict[str, list[tuple[int, int, int]]] = {}
    for y, row in enumerate(rows):
        x = 0
        while x < len(row):
            cell = row[x]
            if not keep(cell):
                x += 1
                continue
            start = x
            while x < len(row) and row[x] == cell:
                x += 1
            out.setdefault(INK[cell], []).append((start, y, x - start))
    return out


def rects(groups: dict[str, list[tuple[int, int, int]]]) -> str:
    lines = []
    for colour, cells in groups.items():
        body = "".join(f'<rect x="{x}" y="{y}" width="{w}" height="1"/>' for x, y, w in cells)
        lines.append(f'  <g fill="{colour}">{body}</g>')
    return "\n".join(lines)


def main() -> None:
    rows = grid()
    width, height = len(rows[0]), len(rows)
    body = runs(rows, lambda c: c not in ".EW")
    eyes = runs(rows, lambda c: c in "EW")
    # A shut eye is the skin it sits on, which is always F.
    lids = [(x, y, w) for cells in eyes.values() for x, y, w in cells]
    lid = "".join(f'<rect x="{x}" y="{y}" width="{w}" height="1"/>' for x, y, w in lids)

    cycle = BEAT * BEATS
    # The shut beat goes last, not first: a renderer that does not animate
    # shows frame zero, and a still of the mascot should have its eyes open.
    shut = 1 - 1 / BEATS
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width * 6}" height="{height * 6}" shape-rendering="crispEdges">
  <title>Hippocampus</title>
{rects(body)}
{rects(eyes)}
  <g fill="{INK['F']}" opacity="0">{lid}<animate attributeName="opacity" values="0;1" keyTimes="0;{shut:.4f}" calcMode="discrete" dur="{cycle:.1f}s" repeatCount="indefinite"/></g>
</svg>
"""
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(svg)
    print(f"wrote {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
