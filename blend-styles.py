#!/usr/bin/env python3
"""Blend same-seed renders of one prompt into a single cross-faded image.

Render the same prompt N times with the SAME seed, changing only the style
clause — MFLUX keeps the composition, so the frames line up and can be masked
together with soft vertical seams:

    for st in "watercolour painting" "cinematic photograph" "anime cel illustration"; do
      MFLUX_MODEL=flux2 MFLUX_SEED=7 MFLUX_W=1536 MFLUX_H=640 \
        ./generate.sh "a red fox sitting in a forest clearing, $st"
    done
    ./blend-styles.py out.jpg generated/<a>.png generated/<b>.png generated/<c>.png

Panels are equal width by default. Put the seams where the subject isn't:

    ./blend-styles.py out.jpg a.png b.png c.png --bounds 0,420,1100,1536 --feather 240

Needs the repo venv (Pillow + NumPy):  .venv/bin/python blend-styles.py ...
"""
import argparse
import sys

import numpy as np
from PIL import Image


def blend(paths, out, bounds=None, feather=240, quality=92):
    ims = [Image.open(p).convert("RGB") for p in paths]
    w, h = ims[0].size
    frames = [np.asarray(im if im.size == (w, h) else im.resize((w, h))).astype(np.float32)
              for im in ims]
    n = len(frames)

    if bounds is None:
        bounds = [round(i * w / n) for i in range(n + 1)]
    elif len(bounds) != n + 1:
        sys.exit(f"--bounds needs {n + 1} values for {n} images, got {len(bounds)}")

    x = np.arange(w, dtype=np.float32)
    acc = np.zeros((h, w, 3), np.float32)
    weight = np.zeros((h, w, 1), np.float32)

    for i, frame in enumerate(frames):
        lo, hi = bounds[i], bounds[i + 1]
        # ramp in from the left seam / out at the right one; outer edges stay opaque
        left = np.clip((x - (lo - feather / 2)) / feather, 0, 1) if i else np.ones(w, np.float32)
        right = (np.clip(((hi + feather / 2) - x) / feather, 0, 1)
                 if i < n - 1 else np.ones(w, np.float32))
        mask = left * right
        mask = mask * mask * (3 - 2 * mask)          # smoothstep — no banding at the seam
        mask = mask[None, :, None]
        acc += frame * mask
        weight += mask

    result = (acc / np.maximum(weight, 1e-6)).clip(0, 255).astype(np.uint8)
    img = Image.fromarray(result)
    if out.lower().endswith((".jpg", ".jpeg")):
        img.save(out, quality=quality, subsampling=0, optimize=True)
    else:
        img.save(out, optimize=True)
    print(f"Saved: {out}  ({w}x{h}, seams at {bounds[1:-1]}, feather {feather}px)")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("output", help="output .jpg or .png")
    ap.add_argument("images", nargs="+", help="input renders, left to right")
    ap.add_argument("--bounds", help="comma-separated panel edges in px, e.g. 0,420,1100,1536")
    ap.add_argument("--feather", type=int, default=240, help="seam width in px (default 240)")
    ap.add_argument("--quality", type=int, default=92, help="JPEG quality (default 92)")
    args = ap.parse_args()

    bounds = [int(v) for v in args.bounds.split(",")] if args.bounds else None
    blend(args.images, args.output, bounds=bounds, feather=args.feather, quality=args.quality)


if __name__ == "__main__":
    main()
