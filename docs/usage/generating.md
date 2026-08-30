# Generating images

## MFLUX — command line (fastest path)

```bash
# default model = FLUX.2 [klein] 9B (4-step distilled, editing-capable)
./generate.sh "a red panda wearing a tiny top hat, watercolor style"

# pick a model (aliases in models-guide.md)
MFLUX_MODEL=z-image-turbo ./generate.sh "an astronaut above Earth, golden hour"  # fastest, Apache-2.0
MFLUX_MODEL=qwen-2512     ./generate.sh "a serene alpine lake at dawn, mist, pine forest"
```

Output lands in `generated/` as `<timestamp>_<model>.png` plus a sidecar
`.json` of the generation metadata — prompt, seed, steps, size.

> The sidecar is empty for FLUX.2. mflux's `mflux-generate-flux2` /
> `-flux2-edit` call `ImageUtil.save_image()` without passing `metadata=`, so
> `json.dump(None)` writes a literal `null` — every `flux2` / `flux2-4b` sidecar
> here is 4 bytes, while Qwen and Z-Image write theirs in full. Nothing is
> actually lost: mflux embeds the same record in the PNG's EXIF regardless of
> model, so read it from the image (`exiftool`, or the desktop app's gallery,
> which falls back to it automatically):
>
> ```bash
> .venv/bin/python -c "import json,sys; b=open(sys.argv[1],'rb').read(); i=b.find(b'{\"mflux_version'); print(json.JSONDecoder().raw_decode(b[i:].decode('utf-8','replace'))[0]['prompt'])" generated/<file>.png
> ```

### Tunables (env vars)

| Var | Default | Notes |
|---|---|---|
| `MFLUX_MODEL` | `flux2` | alias or raw model name |
| `MFLUX_QUANT` | `8` | `3/4/5/6/8`, or **`none`/`off`/`full`/`0`** for full precision |
| `MFLUX_STEPS` | per-model | inference steps |
| `MFLUX_GUIDANCE` | per-model | CFG; e.g. Qwen likes `2.5`, z-image-turbo is forced to `0` |
| `MFLUX_SEED` | random | fixed seed for reproducibility |
| `MFLUX_SIZE` | `1024` | square shorthand (or `MFLUX_W` / `MFLUX_H`) |
| `MFLUX_MAX_MP` | `1.3` | megapixel cap when the size comes from a reference image |
| `MFLUX_LOWRAM` | — | `1` to reduce peak RAM (slower) |
| `MFLUX_IMAGE` | — | one reference image (env form of the extra args below) |
| `MFLUX_MODE` | `auto` | `edit` \| `img2img` — see below |
| `MFLUX_STRENGTH` | `0.4` | img2img only: how much of the reference survives (0–1) |
| `MFLUX_COLORING` | — | `1` to turn the prompt into a coloring page — see below |
| `MFLUX_COLORING_HINTS` | built-in | replace the coloring recipe with your own wording |

### Reference images

Any argument after the prompt is a reference image:

```bash
# edit: the prompt is an INSTRUCTION, the picture comes from the reference
MFLUX_MODEL=flux2 ./generate.sh "put a knitted red scarf on the fox" fox.png

# several references combine (FLUX.2 only)
MFLUX_MODEL=flux2 ./generate.sh "make her wear those glasses" person.jpg glasses.jpg

# img2img: the reference only seeds the denoise, the prompt describes it all
MFLUX_MODE=img2img MFLUX_STRENGTH=0.55 ./generate.sh "the same fox, anime cel style" fox.png
```

The two modes are genuinely different, and `MFLUX_MODE=auto` (the default) picks
the better one per model:

| | what the reference is | what the prompt is | models |
|---|---|---|---|
| **edit** | conditioning, 1+ images | an instruction | `flux2`, `flux2-4b` (no extra download) · `qwen`, `qwen-2512` (pulls Qwen-Image-Edit-2509, ~58 GB) |
| **img2img** | the starting point of the denoise | a full description | all of them |

So `auto` = **edit** on the FLUX.2 [klein] models, which do it with the weights
they already have, and **img2img** everywhere else. Under the hood the edit path
switches to `mflux-generate-flux2-edit` / `mflux-generate-qwen-edit`; img2img just
adds `--image-path` / `--image-strength` to the normal command.

#### Output size with a reference

With a reference and no `MFLUX_SIZE` / `MFLUX_W` / `MFLUX_H`, no `--width/--height`
is passed at all, so the tools that default to the source image (FLUX.2 in both
modes, Z-Image Turbo) keep the reference's aspect ratio instead of squashing it into
a square; Qwen and Z-Image base still fall back to 1024² — set a size for those.

**Above `MFLUX_MAX_MP` (1.3 MP) that reference-derived size is scaled down** and
passed explicitly, aspect preserved, snapped to the multiple of 16 mflux wants. This
exists because it bites hard: a 2880×1800 wallpaper as reference renders at
2880×1800, which peaked at **69 GB on a 64 GB machine** — swapping, 25–34 s per step.
The same edit at the capped 1440×896 stays near 30 GB and runs ~4 s per step. Raise
the cap if you have headroom (`MFLUX_MAX_MP=3`), or set an explicit size to bypass it.

### Coloring pages (Ausmalbilder)

`MFLUX_COLORING=1` leaves the prompt as written and appends a line-art recipe to
it — crisp black outlines on plain white, fine detailed line work, every shape a
closed outline, no shading or grey tones. The prompt then only has to carry the
*idea*; the style comes from the toggle:

```bash
MFLUX_COLORING=1 ./generate.sh "a unicorn standing in a flower meadow, butterflies around it, a castle on a hill in the distance"
```

![A coloring page: a unicorn in a flower meadow, ornate line work, castle on the hill](../assets/coloring-page.jpg)

~12 s on FLUX.2 [klein] at its default 4 steps. The wording deliberately aims at
*detailed* pages — ornate patterns, fine enclosed shapes — rather than the
four-fat-outlines kind. For simpler pages, override the recipe:

```bash
MFLUX_COLORING_HINTS="simple black and white coloring page for a toddler, very thick bold outlines, large simple shapes, no detail, white background" \
  MFLUX_COLORING=1 ./generate.sh "a happy elephant with a balloon"
```

It composes with a reference image, which is the fastest way to turn a photo into
a page to colour in — the prompt is then the instruction, and the recipe is still
appended:

```bash
MFLUX_COLORING=1 ./generate.sh "keep the fox and the forest exactly as they are" fox.png
```

For printing, set the page shape explicitly:

```bash
MFLUX_COLORING=1 MFLUX_W=864 MFLUX_H=1216 ./generate.sh "a friendly dragon reading a book under a big tree, birds in the branches"
```

864 × 1216 is A4 upright to within half a percent, both edges are the multiples
of 16 mflux wants, and it is the same pixel count as the 1024² default — so the
page shape costs no extra time or memory. It is also exactly what the desktop
app's **Portrait (A4)** preset sends at Size 1024.

### Recipes worth remembering

```bash
# Qwen at full precision (only mode where its letters are sharp — ~62 GB RAM, ~12 min)
MFLUX_QUANT=none MFLUX_STEPS=30 MFLUX_GUIDANCE=2.5 MFLUX_MODEL=qwen ./generate.sh "..."

# FLUX.2 klein, reproducible, larger canvas
MFLUX_SEED=42 MFLUX_SIZE=1280 MFLUX_MODEL=flux2 ./generate.sh "..."

# tight on memory? add low-ram
MFLUX_LOWRAM=1 MFLUX_MODEL=qwen ./generate.sh "..."
```

First run of any model downloads it into the shared cache; later runs start
instantly. See [tips-and-gotchas](tips-and-gotchas.md) for the quantization/text
caveats.

### Pre-download / warm models

```bash
./download-models.sh list                       # show known targets
./download-models.sh z-image-turbo qwen flux2    # download + tiny test render
```

## ComfyUI — web UI (workflows, LoRAs, FLUX.2 dev, pixel-art)

```bash
./comfyui-start.sh          # http://localhost:8188
```

- Reads models from the shared store (`~/ai-models/comfyui`), not `comfyui/models`.
- **ComfyUI-Manager** installed → Manager button → Model Manager fetches
  checkpoints/LoRAs straight into the shared store.
- **ComfyUI-GGUF** installed for FLUX.2 GGUF workflows.
- For **FLUX.2 [dev] 32B** — best quality + the only reliable text renderer here —
  two ready-made workflows are in your Workflows sidebar. See
  [comfyui-flux2-dev.md](comfyui-flux2-dev.md).

## Pixel-art pipeline

A full ComfyUI (Pony V7) → downscale → image-to-video → sprite-frames workflow
lives in [pixel-art-pipeline.md](pixel-art-pipeline.md).
</content>
