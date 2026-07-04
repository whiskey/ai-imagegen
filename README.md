# AI Image Generation

Local image generation on Apple Silicon (**M5 Max, 64 GB**), two ways:

- **[MFLUX](https://github.com/filipstrand/mflux)** — native MLX, command-line, fastest. Runs Z-Image, FLUX.2 [klein], Qwen-Image, FLUX.1.
- **[ComfyUI](https://github.com/comfyanonymous/ComfyUI)** — node-based UI for workflows, LoRAs, editing and the pixel-art pipeline.

Everything is isolated (no system-wide Python) and every tool shares **one model
store** so weights are downloaded once. See [MODELS.md](MODELS.md) for the model set.

## How isolation works

- The system Python (3.14, Homebrew) is **never touched**.
- [`uv`](https://docs.astral.sh/uv/) manages a pinned, standalone **Python 3.12**
  in its own cache and creates a per-tool virtualenv:
  - `.venv/` — MFLUX
  - `comfyui/.venv/` — ComfyUI
- Both are git-ignored; delete a `.venv` and re-run the setup to rebuild it.

## First-time setup

Already provisioned in this checkout. To rebuild from scratch on a fresh machine:

```bash
brew install uv
uv python install 3.12

# MFLUX env
uv venv --python 3.12 .venv
uv pip install mflux hf_transfer

# ComfyUI env
git clone --depth 1 https://github.com/comfyanonymous/ComfyUI comfyui
uv venv --python 3.12 comfyui/.venv
( cd comfyui && uv pip install -r requirements.txt )
# custom nodes (Manager + GGUF loader for FLUX.2)
git clone --depth 1 https://github.com/ltdrdata/ComfyUI-Manager comfyui/custom_nodes/ComfyUI-Manager
git clone --depth 1 https://github.com/city96/ComfyUI-GGUF     comfyui/custom_nodes/ComfyUI-GGUF
( cd comfyui && uv pip install -r custom_nodes/ComfyUI-Manager/requirements.txt )
```

## MFLUX — command line (fastest)

```bash
# default model = Z-Image Turbo (6B, Apache-2.0, ~8 steps)
./generate.sh "a red panda wearing a tiny top hat, watercolor style"

# pick a model
MFLUX_MODEL=flux2 ./generate.sh "an astronaut above Earth, photorealistic, golden hour"
MFLUX_MODEL=qwen  ./generate.sh "a vintage travel poster that reads 'KYOTO 1964'"
```

Aliases: `z-image-turbo` (default) · `z-image` · `flux2` · `flux2-4b` · `qwen` ·
`flux-dev` · `flux-schnell` · `krea`. Full table in [MODELS.md](MODELS.md).

Tunables (env vars): `MFLUX_QUANT` (3/4/5/6/8, default 8; `""`=full precision),
`MFLUX_STEPS`, `MFLUX_SEED`, `MFLUX_SIZE` (or `MFLUX_W`/`MFLUX_H`), `MFLUX_LOWRAM=1`.

Images are saved to `generated/` as `<timestamp>_<model>.png` (with a sidecar
`.json` of the generation metadata). First run of any model downloads it into the
shared cache (a few GB); later runs are instant to start.

### Warm up / pre-download models

```bash
./download-models.sh list                       # show known targets
./download-models.sh z-image-turbo qwen flux2    # download + tiny test render
```

## ComfyUI — web UI

```bash
./comfyui-start.sh          # http://localhost:8188
```

- Reads models from the shared store (`~/ai-models/comfyui`), not `comfyui/models`.
- **ComfyUI-Manager** is installed: use it (Manager button → Model Manager) to
  fetch checkpoints/LoRAs straight into the shared store.
- **ComfyUI-GGUF** is installed for FLUX.2 [klein] GGUF workflows.

## Reusing models across tools (opencode, etc.)

Weights live OUTSIDE this repo in a common store so any tool reuses them:

```
~/ai-models/
├── hf/          # HuggingFace cache  (MFLUX, diffusers, opencode)  -> $HF_HOME
└── comfyui/     # ComfyUI single-file weights                      -> extra_model_paths.yaml
```

- **HF-based tools (incl. opencode):** point them at the same cache —
  `export HF_HOME=~/ai-models/hf`. They'll find already-downloaded models and
  write new ones back to the same place. (`env.sh` sets this for our scripts.)
- **Another ComfyUI / Draw Things / any single-file loader:** point it at
  `~/ai-models/comfyui` — either via its own `extra_model_paths.yaml` or a
  symlink, e.g. `ln -s ~/ai-models/comfyui /path/to/other/ComfyUI/models`.

Relocate the whole store with `export AI_MODELS_DIR=/somewhere/else` before
sourcing `env.sh`.

> Note: MFLUX (HF-cache, full diffusers repos) and ComfyUI (repacked single files)
> use different file layouts, so a model used in *both* ecosystems may exist as two
> formats. Within each ecosystem, sharing is a single copy.

## Pixel Art Pipeline

See [pixel-art-character-pipeline.md](pixel-art-character-pipeline.md) — ComfyUI
(Pony Diffusion V7) → downscale → image-to-video → sprite frames.

## Project structure

```
.
├── env.sh                         # shared env: HF_HOME + store paths (sourced by scripts)
├── generate.sh                    # MFLUX multi-model CLI generation
├── comfyui-start.sh               # launch ComfyUI wired to the shared store
├── download-models.sh             # fetch/warm models into the shared store
├── MODELS.md                      # current recommended models + locations
├── models-inventory.md            # historical snapshot (pre-clear, 2026-06)
├── pixel-art-character-pipeline.md
├── .venv/                         # MFLUX venv            (git-ignored)
├── comfyui/                       # ComfyUI clone + venv  (git-ignored)
└── generated/                     # output images         (git-ignored)

~/ai-models/                       # shared model store    (outside the repo)
```

## Notes

- Runs natively on Metal/MPS — not in Docker.
- 64 GB unified memory is reported to ComfyUI as shared VRAM; 8-bit quantization
  keeps the big models fast with negligible quality loss.
- The `triton unavailable` line in ComfyUI logs is expected on Apple Silicon
  (triton is CUDA-only) and harmless.
