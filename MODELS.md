# Models (current recommended set — mid-2026)

Hardware target: **Apple M5 Max, 64 GB** unified memory. Everything below runs
comfortably; 8-bit quantization is the default for the larger models.

All weights live in ONE shared store so MFLUX, ComfyUI and opencode reuse a
single download — see [Shared store](#shared-store).

## What changed since the 2026-06 snapshot

The old inventory (`models-inventory.md`) was cleared to free disk. These are the
current equivalents, not the exact old files:

| Old (snapshot) | Current equivalent | Notes |
|---|---|---|
| Flux.1 Dev FP8 (all-in-one) | **FLUX.2 [klein] 9B** | Latest Black Forest Labs open model; image editing + multi-reference |
| Flux.1 split (t5xxl + clip_l + ae) | folded into FLUX.2 / FLUX.1 via MFLUX | MFLUX bundles the encoders/VAE per model |
| Qwen-Image 20B (+ qwen3 enc + vae) | **Qwen-Image-2.0 (7B)** | #1 text rendering, native 2K, lighter |
| Pony Diffusion V6 XL + sdxl_vae | **Pony Diffusion V7** (AuraFlow) | 1536px, better coherence — pixel-art pipeline |
| anima-preview | superseded by Qwen-Image-2.0 | was an experimental preview |
| flux-uncensored-v2 LoRA | (optional, user-supplied) | drop any `.safetensors` LoRA into the shared `loras/` |
| — | **Z-Image Turbo 6B** (new) | Apache-2.0, 8-step, fastest quality/speed on Apple Silicon |

## MFLUX models (native MLX — the fast Apple-Silicon path)

Selected via `MFLUX_MODEL=<alias>` for `./generate.sh`. First run auto-downloads
into the shared HF cache (`$HF_HOME`); nothing to place by hand.

| Alias | `--base-model` | Params | License | Good for |
|---|---|---|---|---|
| `z-image-turbo` *(default)* | z-image-turbo | 6B | Apache-2.0 | Fast, general, commercial-friendly |
| `z-image` | z-image | 6B | Apache-2.0 | Higher quality, more steps |
| `flux2` | flux2-klein-9b | 9B | FLUX dev non-commercial | Latest Flux, editing, best prompt adherence |
| `flux2-4b` | flux2-klein-4b | 4B | FLUX dev non-commercial | Faster/lighter klein |
| `qwen` | qwen | 7B | Apache-2.0 | Text/typography, posters, 2K |
| `flux-dev` | dev | 12B | FLUX dev non-commercial | Classic FLUX.1 |
| `flux-schnell` | schnell | 12B | Apache-2.0 | 4-step, quick drafts |
| `krea` | krea-dev | 12B | FLUX dev non-commercial | Photographic look |

MFLUX can also do editing / img2img / ControlNet / upscaling — see
`mflux-generate-*` commands in `.venv/bin` (`mflux-generate-qwen-edit`,
`mflux-generate-flux2-edit`, `mflux-generate-kontext`, `mflux-upscale-seedvr2`, …).

## ComfyUI models (node workflows, LoRAs, pixel-art pipeline)

Single-file weights placed under the shared store (`$COMFY_MODELS_DIR`):

| Model | Where it goes | Purpose |
|---|---|---|
| FLUX.2 [klein] 9B GGUF (`unsloth/FLUX.2-klein-9B-GGUF`, Q8) | `unet/` | Flux.2 workflows via ComfyUI-GGUF |
| FLUX.2 text encoder + VAE | `text_encoders/`, `vae/` | companions for the above |
| Pony Diffusion V7 base (AuraFlow) | `checkpoints/` | pixel-art character pipeline |
| Qwen-Image-2.0 (Comfy-Org repack) | `diffusion_models/` (+ enc/vae) | typography workflows in ComfyUI |

Fetch with `./download-models.sh` (recipes finalized per the set you pick), or
use **ComfyUI-Manager** inside the web UI (Manager → Model Manager) which drops
files straight into the shared store.

## Shared store

```
~/ai-models/                 ($AI_MODELS_DIR — override to relocate)
├── hf/                       $HF_HOME  — HuggingFace cache (MFLUX, diffusers, opencode)
└── comfyui/                  $COMFY_MODELS_DIR — ComfyUI single-file weights
    ├── checkpoints/  diffusion_models/  unet/
    ├── text_encoders/  clip/  clip_vision/
    ├── vae/  loras/  controlnet/  upscale_models/  embeddings/  style_models/
```

There are two sharing mechanisms because there are two model conventions:

- **HF-cache tools** (MFLUX, diffusers, opencode) → all honour `HF_HOME`, so
  pointing them at `~/ai-models/hf` means one copy, many tools.
- **ComfyUI single-file weights** → shared via `comfyui/extra_model_paths.yaml`
  (auto-written by `comfyui-start.sh`) pointing at `~/ai-models/comfyui`.

See [README.md](README.md#reusing-models-across-tools-opencode-etc) for how to
point opencode / a second ComfyUI / Draw Things at the same store.
