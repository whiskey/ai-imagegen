# Models & the shared store

All weights live in ONE store **outside the repo** so MFLUX, ComfyUI and other
tools (opencode, a second ComfyUI, Draw Things, …) reuse a single download.

## Shared store layout

```
~/ai-models/                 ($AI_MODELS_DIR — override to relocate)
├── hf/                       $HF_HOME  — HuggingFace cache (MFLUX, diffusers, opencode)
└── comfyui/                  $COMFY_MODELS_DIR — ComfyUI single-file weights
    ├── checkpoints/  diffusion_models/  unet/
    ├── text_encoders/  clip/  clip_vision/
    ├── vae/  loras/  controlnet/  upscale_models/  embeddings/  style_models/
```

Two sharing mechanisms, because there are two model conventions:

- **HF-cache tools** (MFLUX, diffusers, opencode) honour `HF_HOME` → point them at
  `~/ai-models/hf` and it's one copy, many tools. `env.sh` sets this.
- **ComfyUI single-file weights** are shared via `comfyui/extra_model_paths.yaml`
  (auto-written by `comfyui-start.sh`) pointing at `~/ai-models/comfyui`.

> A model used in *both* ecosystems may exist as two formats (HF diffusers repo
> vs. repacked single file). Within each ecosystem, sharing is a single copy.

### Reusing across tools (opencode, etc.)

- **HF-based tools:** `export HF_HOME=~/ai-models/hf` — they'll find existing
  downloads and write new ones back to the same place.
- **Another ComfyUI / single-file loader:** point it at `~/ai-models/comfyui` via
  its own `extra_model_paths.yaml` or a symlink:
  `ln -s ~/ai-models/comfyui /path/to/other/ComfyUI/models`.

Relocate the whole store with `export AI_MODELS_DIR=/somewhere/else` before
sourcing `env.sh`.

## Downloading

```bash
./download-models.sh list                 # show known targets
./download-models.sh z-image-turbo qwen-2512  # MFLUX: download + tiny test render
./download-models.sh comfy-pony-v7         # ComfyUI: Pony V7 (pixel-art)
./download-models.sh comfy-flux2-dev       # ComfyUI: FLUX.2 dev 32B set
```

- MFLUX models auto-download into `$HF_HOME` on first `generate.sh` run too.
- ComfyUI single files are staged and moved into `$COMFY_MODELS_DIR` (staging is
  on the same volume, so the move is instant even for the 27 GB GGUF).

See [tips-and-gotchas](../usage/tips-and-gotchas.md) for download reliability
(Xet disabled, orphaned `.incomplete` cleanup, gated-repo licenses).

## FLUX.2 dev (ComfyUI) {#flux2-dev-comfyui}

The "big" FLUX.2 [dev] 32B — quality ceiling, best text — runs on 64 GB via
**ComfyUI-GGUF** (MFLUX only supports the klein variants). `comfy-flux2-dev` fetches:

| File | Size | → |
|---|---|---|
| `unsloth/FLUX.2-dev-GGUF` → `flux2-dev-Q6_K.gguf` | 27.4 GB | `unet/` |
| `Comfy-Org/flux2-dev` → Mistral text-encoder fp8 | 18 GB | `text_encoders/` |
| `Comfy-Org/flux2-dev` → `flux2-vae.safetensors` | 0.34 GB | `vae/` |
| `Comfy-Org/flux2-dev` → Turbo LoRA | 2.76 GB | `loras/` |

The unsloth GGUF is **ungated**. The Turbo LoRA lets the 32B run in ~6–8 steps.
Workflow: `UnetLoaderGGUF` (Q6_K) → CLIP loader (Mistral) → sampler → `VAELoader`;
ComfyUI ships a FLUX.2 template, or use ComfyUI-Manager.

## History

`models-inventory.md` (in this dir) is the archived snapshot of what existed
before the models were cleared — kept for reference, superseded by the current
[models-guide](../usage/models-guide.md).
</content>
