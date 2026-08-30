#!/usr/bin/env python3
"""Repair the `*.metadata.json` sidecars mflux writes next to a render.

mflux `--metadata` is supposed to drop a JSON record beside every PNG — model,
seed, steps, guidance, size, quantization, generation time and the full prompt.
For the FLUX.2 family (0.18.0) it instead writes the four bytes `null` and loses
the lot, which is every `flux2` / `flux2-4b` render this repo makes by default.

Nothing is actually lost: mflux embeds the same record in the PNG's own EXIF
UserComment tag whichever model wrote it. So lift it back out and write the
sidecar mflux meant to write — same JSON, same 4-space indent, byte-identical to
the ones the working models produce.

`generate.sh` calls this after each render. Point it at files or directories to
repair a backlog:

    ./png-metadata.py generated/                 # every stale sidecar under it
    ./png-metadata.py generated/a.png b.png      # just these
    ./png-metadata.py --dry-run generated/       # say what would change

Only missing sidecars and ones containing `null` are touched; a sidecar with
real content is left exactly as it is. Needs the repo venv (Pillow):
`.venv/bin/python png-metadata.py ...`
"""
import argparse
import json
import sys
from pathlib import Path

from PIL import Image

# Where mflux puts the record: EXIF UserComment, inside the Exif sub-IFD. The
# tag's value carries an 8-byte character-set prefix ("ASCII\0\0\0") ahead of
# the JSON, per the EXIF spec.
EXIF_IFD = 0x8769
USER_COMMENT = 37510


def read_embedded(png):
    """The mflux record embedded in `png`, or None if it carries none."""
    try:
        ifd = Image.open(png).getexif().get_ifd(EXIF_IFD)
    except Exception:
        return None
    raw = ifd.get(USER_COMMENT)
    if raw is None:
        return None
    if isinstance(raw, bytes):
        raw = raw[8:].decode("utf-8", "replace")
    try:
        record = json.loads(raw)
    except json.JSONDecodeError:
        return None
    return record if isinstance(record, dict) and "mflux_version" in record else None


def is_stale(sidecar):
    """True if `sidecar` is absent or holds mflux's `null` placeholder."""
    if not sidecar.exists():
        return True
    return sidecar.read_text(encoding="utf-8").strip() in ("", "null")


def repair(png, dry_run=False):
    """Rewrite `png`'s sidecar from its EXIF record. Returns what happened."""
    sidecar = png.with_suffix(".metadata.json")  # exactly how mflux names it
    if not is_stale(sidecar):
        return "ok"
    record = read_embedded(png)
    if record is None:
        return "no-record"
    if not dry_run:
        # No trailing newline: this is what mflux itself writes.
        sidecar.write_text(json.dumps(record, indent=4), encoding="utf-8")
    return "repaired"


def collect(paths):
    """Every PNG named directly or living in a named directory, sorted."""
    pngs = []
    for p in (Path(p) for p in paths):
        if p.is_dir():
            pngs += sorted(q for q in p.iterdir() if q.suffix.lower() == ".png")
        elif p.suffix.lower() == ".png":
            pngs.append(p)
        else:
            sys.exit(f"not a PNG or directory: {p}")
    return pngs


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("paths", nargs="+", help="PNG files, or directories of them")
    ap.add_argument("--dry-run", action="store_true", help="report without writing")
    ap.add_argument("--quiet", action="store_true", help="only report the summary")
    args = ap.parse_args()

    counts = {"repaired": 0, "ok": 0, "no-record": 0}
    for png in collect(args.paths):
        outcome = repair(png, dry_run=args.dry_run)
        counts[outcome] += 1
        if not args.quiet and outcome != "ok":
            verb = "would repair" if args.dry_run and outcome == "repaired" else outcome
            print(f"{verb}: {png.name}")

    # Quiet says nothing when there was nothing to do — that is the common case
    # for the per-render call in generate.sh, which shouldn't chatter.
    if not args.quiet or counts["repaired"] or counts["no-record"]:
        verb = "would repair" if args.dry_run else "repaired"
        n = counts["repaired"]
        # Self-describing: this line surfaces mid-render in generate.sh output,
        # where a bare "repaired 1" reads as a non-sequitur.
        parts = [f"metadata: {verb} {n} sidecar{'' if n == 1 else 's'}"]
        if counts["ok"]:
            parts.append(f"already good {counts['ok']}")
        if counts["no-record"]:
            parts.append(f"no embedded record {counts['no-record']}")
        print(", ".join(parts))


if __name__ == "__main__":
    main()
