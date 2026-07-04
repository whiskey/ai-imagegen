# Generating images

## MFLUX — command line (fastest path)

```bash
# default model = Z-Image Turbo (6B, Apache-2.0, ~18 s)
./generate.sh "a red panda wearing a tiny top hat, watercolor style"

# pick a model (aliases in models-guide.md)
MFLUX_MODEL=flux2 ./generate.sh "an astronaut above Earth, photorealistic, golden hour"
MFLUX_MODEL=qwen  ./generate.sh "a serene alpine lake at dawn, mist, pine forest"
```

Output lands in `generated/` as `<timestamp>_<model>.png` plus a sidecar
`.json` of the generation metadata.

### Tunables (env vars)

| Var | Default | Notes |
|---|---|---|
| `MFLUX_MODEL` | `z-image-turbo` | alias or raw model name |
| `MFLUX_QUANT` | `8` | `3/4/5/6/8`, or **`none`/`off`/`full`/`0`** for full precision |
| `MFLUX_STEPS` | per-model | inference steps |
| `MFLUX_GUIDANCE` | per-model | CFG; e.g. Qwen likes `2.5`, z-image-turbo is forced to `0` |
| `MFLUX_SEED` | random | fixed seed for reproducibility |
| `MFLUX_SIZE` | `1024` | square shorthand (or `MFLUX_W` / `MFLUX_H`) |
| `MFLUX_LOWRAM` | — | `1` to reduce peak RAM (slower) |

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
- For the **FLUX.2 [dev] 32B** workflow, see [setup/models](../setup/models.md#flux2-dev-comfyui).

## Pixel-art pipeline

A full ComfyUI (Pony V7) → downscale → image-to-video → sprite-frames workflow
lives in [pixel-art-pipeline.md](pixel-art-pipeline.md).
</content>
