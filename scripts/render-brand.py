#!/usr/bin/env python3
"""Renders every raster form of the mark from client/src/brand/mark.json.

The interface draws that file directly (client/src/brand/Mark.tsx).
Everything that cannot read SVG at runtime — the app icon, the menu-bar
template — is produced here, so the mark cannot drift between the window,
the Dock and the menu bar.

    python3 scripts/render-brand.py            # everything
    python3 scripts/render-brand.py favicon    # only client/public/favicon.svg

Pillow is the only requirement, and only to regenerate; the output is
committed and neither building nor running the app needs this script. The
path data is flattened and stroked here rather than handed to an SVG
rasteriser on purpose: mark.json uses nothing but moveto, cubic and close,
so a hundred lines of arithmetic buys independence from a native library
that turned out not to be installed anyway.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image, ImageDraw
except ImportError:  # pragma: no cover - a developer-machine script
    sys.exit("needs Pillow:  pip install Pillow")

ROOT = Path(__file__).resolve().parent.parent
MARK = json.loads((ROOT / "client/src/brand/mark.json").read_text())
ICONS = ROOT / "client/src-tauri/icons"

# --ink-800 and --clay from client/src/styles/tokens.css. An icon is the one
# surface that cannot read the tokens at runtime, so they are repeated here
# and nowhere else.
GROUND = (23, 19, 15, 255)
INK = (208, 120, 80, 255)
# Once a tray icon is marked as a template macOS draws it from alpha alone,
# so the colour is irrelevant there and only coverage matters.
TEMPLATE_INK = (0, 0, 0, 255)

# How finely a cubic is chopped into straight lines. At the sizes involved
# anything past this is invisible and just slows the supersample down.
SEGMENTS = 24


def parse(d: str) -> list[list[tuple[float, float]]]:
    """Flattens an SVG path of M/C/Z into polylines in viewBox units."""
    tokens = re.findall(r"[MCZmcz]|-?\d*\.?\d+", d)
    subpaths: list[list[tuple[float, float]]] = []
    points: list[tuple[float, float]] = []
    cursor = (0.0, 0.0)
    i = 0
    while i < len(tokens):
        op = tokens[i]
        i += 1
        if op in "Mm":
            if points:
                subpaths.append(points)
            cursor = (float(tokens[i]), float(tokens[i + 1]))
            i += 2
            points = [cursor]
        elif op in "Cc":
            while i + 5 < len(tokens) and not tokens[i].isalpha():
                p1 = (float(tokens[i]), float(tokens[i + 1]))
                p2 = (float(tokens[i + 2]), float(tokens[i + 3]))
                p3 = (float(tokens[i + 4]), float(tokens[i + 5]))
                i += 6
                for step in range(1, SEGMENTS + 1):
                    t = step / SEGMENTS
                    u = 1 - t
                    points.append(
                        (
                            u**3 * cursor[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t**3 * p3[0],
                            u**3 * cursor[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t**3 * p3[1],
                        )
                    )
                cursor = p3
        elif op in "Zz":
            if points:
                points.append(points[0])
                subpaths.append(points)
                points = []
    if points:
        subpaths.append(points)
    return subpaths


class Pen:
    """Draws mark.json's 24-unit box onto a Pillow image."""

    def __init__(self, image: Image.Image, side: float, offset: tuple[float, float], pad: float):
        self.draw = ImageDraw.Draw(image)
        # `pad` widens the box so a stroke sitting on the edge is not clipped.
        self.unit = side / (24 + 2 * pad)
        self.origin = (offset[0] + pad * self.unit, offset[1] + pad * self.unit)

    def _map(self, points):
        return [(self.origin[0] + x * self.unit, self.origin[1] + y * self.unit) for x, y in points]

    def stroke(self, d: str, width: float, colour) -> None:
        px = max(width * self.unit, 1.0)
        for points in parse(d):
            mapped = self._map(points)
            # `joint="curve"` is Pillow's round join; the two discs are the
            # round caps, which it has no option for.
            self.draw.line(mapped, fill=colour, width=round(px), joint="curve")
            for x, y in (mapped[0], mapped[-1]):
                r = px / 2
                self.draw.ellipse([x - r, y - r, x + r, y + r], fill=colour)

    def fill(self, d: str, colour) -> None:
        for points in parse(d):
            self.draw.polygon(self._map(points), fill=colour)


def glyph(side: int, colour, *, filled: bool, ground=None, pad: float = 1.6, supersample: int = 8) -> Image.Image:
    """The brain alone, transparent around it.

    `filled` swaps the outline for a solid silhouette with the folds cut
    back out of it — below about 32 px a stroked brain turns to mush, and a
    bold shape with two grooves survives where six thin strokes do not.
    """
    small = side < MARK["smallSize"] * 2 or filled
    outline_w = MARK["strokeSmall"] if small else MARK["strokeOutline"]
    inner_w = MARK["strokeSmall"] if small else MARK["strokeInner"]
    folds = list(MARK["essential"])

    big = side * supersample
    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    pen = Pen(image, big, (0, 0), pad)

    if filled:
        pen.fill(MARK["outline"], colour)
        pen.stroke(MARK["outline"], outline_w, colour)
        for d in folds:
            pen.stroke(d, inner_w, ground)
    else:
        pen.stroke(MARK["outline"], outline_w, colour)
        for d in folds:
            pen.stroke(d, inner_w, colour)

    return image.resize((side, side), Image.LANCZOS)


def app_icon(size: int) -> Image.Image:
    """The mark on its rounded ground.

    The squircle sits inside a transparent margin, the way every other macOS
    app icon does — without it this one would look a size larger than its
    neighbours in the Dock and in Finder.
    """
    supersample = 8 if size < 256 else 2
    big = size * supersample
    # Small icons carry less air. macOS draws a 16 or 32 pt icon nearly
    # full-bleed, and at that size the margin the large ones need is most
    # of the pixels — the motif ends up half the width of its own tile.
    small = size <= 64
    margin = round(big * (0.055 if small else 0.085))
    side = big - 2 * margin

    canvas = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(canvas).rounded_rectangle(
        [margin, margin, margin + side, margin + side],
        radius=round(side * 0.225),
        fill=GROUND,
    )

    inner = round(side * (0.74 if small else 0.62))
    canvas.alpha_composite(
        glyph(inner, INK, filled=size <= 32, ground=GROUND, supersample=1 if supersample > 1 else 4),
        (margin + (side - inner) // 2, margin + (side - inner) // 2),
    )
    return canvas.resize((size, size), Image.LANCZOS)


def ios_icon(size: int = 1024) -> Image.Image:
    """The mark on a full-bleed, opaque ground, as iOS wants it.

    iOS cuts the corners itself and fills any transparency with black, so
    the squircle and the margin of the Mac icon would show up there as a
    black frame around a smaller tile.
    """
    # Drawn at twice the size and scaled down, like `app_icon`: stroking
    # at the final size leaves hairline seams between the segments.
    big = size * 2
    canvas = Image.new("RGBA", (big, big), GROUND)
    inner = round(big * 0.66)
    canvas.alpha_composite(
        glyph(inner, INK, filled=False, ground=GROUND, supersample=1),
        ((big - inner) // 2, (big - inner) // 2),
    )
    return canvas.resize((size, size), Image.LANCZOS).convert("RGB")


def tray_template() -> Image.Image:
    """The menu-bar mark: black plus alpha, for macOS to tint.

    48x36 rather than square, and the extra 12 px on the right is
    deliberately empty. tray-icon scales whatever it is given to 18 points
    tall, so the brain lands at the conventional 16 pt inside an 18 pt box —
    and the gap is where the recording dot goes. Reserving it here is what
    keeps the item from changing width, so the mark itself never moves.
    """
    supersample = 8
    canvas = Image.new("RGBA", (48 * supersample, 36 * supersample), (0, 0, 0, 0))
    inner = 32 * supersample
    canvas.alpha_composite(glyph(inner, TEMPLATE_INK, filled=False, pad=1.4, supersample=1), (2 * supersample, 2 * supersample))
    return canvas.resize((48, 36), Image.LANCZOS)


def favicon_svg() -> str:
    """The browser build's tab icon, as the path data itself.

    A browser reads SVG, so unlike the icons above nothing is rasterised:
    the outline and the three essential strokes are written out as they
    stand in mark.json. It still comes from here rather than being copied
    by hand, so it cannot drift from the mark either.
    """
    rgb = "#{:02x}{:02x}{:02x}".format(*INK[:3])
    ground = "#{:02x}{:02x}{:02x}".format(*GROUND[:3])
    inner = "".join(
        f'<path d="{d}" stroke-width="{MARK["strokeInner"]}"/>' for d in MARK["essential"]
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{MARK["viewBox"]}">'
        f'<rect width="24" height="24" rx="5.5" fill="{ground}"/>'
        f'<g fill="none" stroke="{rgb}" stroke-linecap="round" stroke-linejoin="round">'
        f'<path d="{MARK["outline"]}" stroke-width="{MARK["strokeOutline"]}"/>{inner}</g></svg>\n'
    )


def write_favicon() -> None:
    public = ROOT / "client/public"
    public.mkdir(exist_ok=True)
    (public / "favicon.svg").write_text(favicon_svg())
    print("favicon.svg")


def main() -> None:
    write_favicon()
    if sys.argv[1:] == ["favicon"]:
        return

    ICONS.mkdir(parents=True, exist_ok=True)

    tray_template().save(ICONS / "tray-template.png")
    print("tray-template.png  48x36")

    named = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 1024,
        "StoreLogo.png": 50,
        "Square30x30Logo.png": 30,
        "Square44x44Logo.png": 44,
        "Square71x71Logo.png": 71,
        "Square89x89Logo.png": 89,
        "Square107x107Logo.png": 107,
        "Square142x142Logo.png": 142,
        "Square150x150Logo.png": 150,
        "Square284x284Logo.png": 284,
        "Square310x310Logo.png": 310,
    }
    for name, size in named.items():
        app_icon(size).save(ICONS / name)
    print(f"{len(named)} app icons, 30 to 1024 px")

    iconset = ICONS / "icon.iconset"
    iconset.mkdir(exist_ok=True)
    for base in (16, 32, 128, 256, 512):
        app_icon(base).save(iconset / f"icon_{base}x{base}.png")
        app_icon(base * 2).save(iconset / f"icon_{base}x{base}@2x.png")
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(ICONS / "icon.icns")], check=True)
    for leftover in iconset.iterdir():
        leftover.unlink()
    iconset.rmdir()
    print("icon.icns")

    # Tauri knows the sizes and names an asset catalogue wants; only the
    # `ios/` part of what `tauri icon` writes is kept.
    with tempfile.TemporaryDirectory() as scratch:
        source = Path(scratch) / "ios.png"
        ios_icon().save(source)
        subprocess.run(
            ["npx", "tauri", "icon", str(source), "-o", str(Path(scratch) / "out")],
            cwd=ROOT / "client",
            check=True,
            capture_output=True,
        )
        shutil.rmtree(ICONS / "ios", ignore_errors=True)
        shutil.copytree(Path(scratch) / "out" / "ios", ICONS / "ios")
    print(f"{len(list((ICONS / 'ios').iterdir()))} iOS icons")

    # The Xcode project is generated and not in the repository, so the
    # icons are copied in whenever it exists: run this again after
    # `tauri ios init`.
    catalogue = ROOT / "client/src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset"
    if catalogue.is_dir():
        for icon in (ICONS / "ios").iterdir():
            shutil.copy(icon, catalogue / icon.name)
        print("copied into the Xcode project")

    app_icon(256).save(ICONS / "icon.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (256, 256)])
    print("icon.ico")


if __name__ == "__main__":
    main()
