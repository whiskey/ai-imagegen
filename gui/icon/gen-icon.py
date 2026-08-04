#!/usr/bin/env python3
"""Draw the app icon: a gradient rounded-square (macOS squircle-ish) with a
simple white landscape/"image" glyph. Rendered 4x and downscaled for clean
anti-aliased edges. Output: a 1024x1024 RGBA PNG (path as argv[1])."""

import sys

import numpy as np
from PIL import Image, ImageDraw

S = 1024          # final size
SS = 4            # supersample factor
N = S * SS


def sc(v: float) -> int:
    return int(round(v * SS))


# --- gradient body (top -> bottom), indigo -> violet ---
top = np.array([79, 70, 229], dtype=float)    # #4F46E5
bot = np.array([168, 85, 247], dtype=float)   # #A855F7
ramp = np.linspace(0.0, 1.0, N)[:, None]
col = (top[None, :] * (1 - ramp) + bot[None, :] * ramp).astype(np.uint8)  # (N,3)
grad = np.repeat(col[:, None, :], N, axis=1)                              # (N,N,3)
bg = np.dstack([grad, np.full((N, N), 255, np.uint8)])                    # (N,N,4)
body = Image.fromarray(bg, "RGBA")

# --- rounded-square mask (transparent corners) ---
margin, radius = 100, 186
mask = Image.new("L", (N, N), 0)
ImageDraw.Draw(mask).rounded_rectangle(
    [sc(margin), sc(margin), sc(S - margin), sc(S - margin)],
    radius=sc(radius), fill=255,
)

icon = Image.new("RGBA", (N, N), (0, 0, 0, 0))
icon.paste(body, (0, 0), mask)

# --- white glyph: sun + two-peak mountain range ---
d = ImageDraw.Draw(icon)
white = (255, 255, 255, 255)

# sun, upper-left sky
d.ellipse([sc(275), sc(285), sc(445), sc(455)], fill=white)  # center (360,370) r85

# mountain silhouette (single polygon, two peaks)
peaks = [(150, 815), (380, 500), (520, 650), (700, 430), (874, 815)]
d.polygon([(sc(x), sc(y)) for x, y in peaks], fill=white)

# downscale for anti-aliasing
icon = icon.resize((S, S), Image.LANCZOS)

out = sys.argv[1] if len(sys.argv) > 1 else "icon_1024.png"
icon.save(out)
print("wrote", out)
