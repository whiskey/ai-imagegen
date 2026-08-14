#!/usr/bin/env python3
"""Draw the app icon: a teal->navy squircle holding a landscape that dissolves
into pixels on the left — a denoising pass caught halfway, which is what the app
actually does. Palette matches the README banner (teal, navy, burnt orange,
cream). Rendered 4x and downscaled for clean anti-aliased edges.
Output: a 1024x1024 RGBA PNG (path as argv[1])."""

import sys

import numpy as np
from PIL import Image, ImageChops, ImageDraw

S = 1024          # final size
SS = 4            # supersample factor
N = S * SS

TEAL = (31, 163, 174)      # #1FA3AE  body, top-left
NAVY = (20, 48, 79)        # #14304F  body, bottom-right
CREAM = (247, 239, 224)    # #F7EFE0  mountains
ORANGE = (242, 112, 58)    # #F2703A  sun

# The glyph: mountains solid on the right, breaking into cells to the left.
BASE_Y = 800                          # mountain baseline
PEAKS = [(110, BASE_Y), (290, 470), (415, 655), (566, 424),
         (690, 610), (790, 520), (896, BASE_Y)]
SUN = (760, 274, 72)                  # cx, cy, r
CELL = 52                             # dissolve grid pitch
# x range: fully diced -> solid silhouette. The right edge sits in the valley
# between the peaks, so the seam between cells and silhouette is short.
DISSOLVE = (120, 415)


def sc(v: float) -> int:
    return int(round(v * SS))


def smoothstep(t: float) -> float:
    t = min(max(t, 0.0), 1.0)
    return t * t * (3 - 2 * t)


def body() -> tuple[Image.Image, Image.Image]:
    """Rounded-square with a diagonal teal -> navy gradient, plus its mask.

    Everything drawn on top is clipped to that mask, so no glyph can leak past
    the rounded corners."""
    yy, xx = np.mgrid[0:N, 0:N].astype(np.float32)
    t = (xx / N) * 0.35 + (yy / N) * 0.65          # mostly top-to-bottom, slight tilt
    ramp = np.dstack([t, t, t])
    rgb = (np.array(TEAL, np.float32) * (1 - ramp)
           + np.array(NAVY, np.float32) * ramp).astype(np.uint8)
    fill = Image.fromarray(np.dstack([rgb, np.full((N, N), 255, np.uint8)]), "RGBA")

    margin, radius = 100, 186
    mask = Image.new("L", (N, N), 0)
    ImageDraw.Draw(mask).rounded_rectangle(
        [sc(margin), sc(margin), sc(S - margin), sc(S - margin)],
        radius=sc(radius), fill=255,
    )
    out = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    out.paste(fill, (0, 0), mask)
    return out, mask


def mountains() -> Image.Image:
    """Silhouette mask, solid on the right, dissolving into cells on the left."""
    solid = Image.new("L", (N, N), 0)
    ImageDraw.Draw(solid).polygon([(sc(x), sc(y)) for x, y in PEAKS], fill=255)
    solid_px = np.asarray(solid)

    x0, x1 = DISSOLVE
    out = Image.new("L", (N, N), 0)
    draw = ImageDraw.Draw(out)
    # keep everything right of the dissolve as one clean silhouette
    out.paste(solid.crop((sc(x1), 0, N, N)), (sc(x1), 0))

    rng = np.random.default_rng(7)                 # fixed: rebuilds are identical
    # grid anchored on the baseline and on the seam, so cells tile flush with both
    rows = range(BASE_Y - CELL, -CELL, -CELL)
    cols = range(x1 - CELL, x0 - CELL, -CELL)
    for gy in rows:
        for gx in cols:
            cell = solid_px[sc(gy):sc(gy + CELL), sc(gx):sc(gx + CELL)]
            if cell.size == 0 or cell.mean() < 70:  # cell barely touches the range
                continue
            p = smoothstep((gx + CELL / 2 - x0) / (x1 - x0))
            if rng.random() > 0.34 + 0.66 * p:      # thin out towards the left
                continue
            size = CELL * (0.58 + 0.42 * p)         # and shrink what survives
            pad = (CELL - size) / 2
            draw.rectangle(
                [sc(gx + pad), sc(gy + pad), sc(gx + pad + size), sc(gy + pad + size)],
                fill=255,
            )
    return out


icon, clip = body()

sun = Image.new("L", (N, N), 0)
cx, cy, r = SUN
ImageDraw.Draw(sun).ellipse([sc(cx - r), sc(cy - r), sc(cx + r), sc(cy + r)], fill=255)

for color, layer in ((ORANGE, sun), (CREAM, mountains())):
    icon.paste(Image.new("RGBA", (N, N), color + (255,)), (0, 0),
               ImageChops.multiply(layer, clip))

icon = icon.resize((S, S), Image.LANCZOS)          # downscale for anti-aliasing

out = sys.argv[1] if len(sys.argv) > 1 else "icon_1024.png"
icon.save(out)
print("wrote", out)
