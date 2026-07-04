# Models Guide — which model for what

Hardware target: **Apple M5 Max, 64 GB**. See [tips-and-gotchas](tips-and-gotchas.md)
for the quantization/quality caveats (especially Qwen text).

## MFLUX models (native MLX CLI — `./generate.sh`)

Selected via `MFLUX_MODEL=<alias>`. First run auto-downloads into the shared HF
cache (`$HF_HOME`); nothing to place by hand.

| Alias | Model | Params | License | Best for |
|---|---|---|---|---|
| `z-image-turbo` *(default)* | Z-Image Turbo (4-bit build) | 6B | Apache-2.0 | Fast, general, commercial-OK. ~18 s |
| `z-image` | Z-Image (base) | 6B | Apache-2.0 | Higher quality, more steps |
| `flux2` | FLUX.2 [klein] 9B | 9B | FLUX dev non-commercial | **Best all-round**; great prompt adherence + text; ~8 s (4-step distilled) |
| `flux2-4b` | FLUX.2 [klein] 4B | 4B | FLUX dev non-commercial | Faster/lighter klein |
| `qwen` | **Qwen-Image (20B)** | 20B | Apache-2.0 | Gorgeous images, 2K. Text is unreliable — see gotchas |
| `flux-dev` | FLUX.1 [dev] | 12B | FLUX dev non-commercial | Classic Flux |
| `flux-schnell` | FLUX.1 [schnell] | 12B | Apache-2.0 | 4-step quick drafts |
| `krea` | FLUX.1 Krea [dev] | 12B | FLUX dev non-commercial | Photographic look |

> Note: MFLUX's `qwen` is the original **20B Qwen-Image**, not the newer 7B
> Qwen-Image-2.0 (MFLUX doesn't expose 2.0). It's a bigger download (~58 GB) but a
> strong image model.

Variant selection uses `--model` (e.g. `--model flux2-klein-9b`), **not**
`--base-model` — `generate.sh` handles this per alias.

MFLUX also does editing / img2img / ControlNet / upscaling — see the
`mflux-generate-*` commands in `.venv/bin` (`mflux-generate-qwen-edit`,
`mflux-generate-flux2-edit`, `mflux-generate-kontext`, `mflux-upscale-seedvr2`, …).

### Quick picks
- **Everyday / fast:** `z-image-turbo` (default) or `flux2`.
- **Prompt adherence + some text:** `flux2`.
- **Max photorealism:** `krea` or `flux2`.
- **Precise typography:** none of the MFLUX models nail it — use ComfyUI FLUX.2 dev.

## ComfyUI models (node workflows, LoRAs, pixel-art)

Single-file weights under the shared store (`$COMFY_MODELS_DIR`), fetched with
`./download-models.sh` or ComfyUI-Manager.

| Model | Files → dir | Purpose |
|---|---|---|
| **FLUX.2 [dev] 32B** (`comfy-flux2-dev`) | Q6_K GGUF → `unet/`, Mistral fp8 TE → `text_encoders/`, VAE → `vae/`, Turbo LoRA → `loras/` | Quality ceiling; best text. ~48 GB. See [setup/models](../setup/models.md) |
| **Pony Diffusion V7** (`comfy-pony-v7`) | fp8 checkpoint + TE + VAE | Pixel-art pipeline (AuraFlow) |
| FLUX.2 [klein] 9B GGUF | `unet/` (+ TE + VAE) | Klein in ComfyUI (also available via MFLUX CLI) |

## FLUX.2: klein 9B vs dev 32B — which to use

Two sizes, very different roles (measured on this M5 Max / 64 GB — a 6-style sweep
of the same subject/seed):

| | **klein 9B** (MFLUX CLI) | **dev 32B** (ComfyUI GGUF) |
|---|---|---|
| Speed | **~16 s/image** — 6 styles in **97 s** | ~1 min+/image; only 3 done in ~27 min |
| Memory | ~9 GB, no pressure | ~40 GB; **a batch maxes swap → crawls** |
| Stylized / illustration | near-identical | marginally finer |
| **Exact text in image** | unreliable | ✅ **spells it correctly** |

**Rule of thumb — klein is the daily driver, dev is the specialist.** Use **klein**
for fast style exploration and iteration (quality gap is marginal for stylized
work). Reach for **dev** only when you need **legible text baked into the image**
or a final hero shot worth the wait — and **don't batch dev renders on 64 GB**, it
swaps (see [comfyui-flux2-dev.md](comfyui-flux2-dev.md)).

## What changed since the 2026-06 snapshot

Old models were cleared to free disk; these are current equivalents (mid-2026),
not the exact old files. Full historical list: [setup/models-inventory](../setup/models-inventory.md).

| Old (snapshot) | Current equivalent |
|---|---|
| Flux.1 Dev FP8 | **FLUX.2 [klein] 9B** (CLI) / **FLUX.2 [dev] 32B** (ComfyUI) |
| Qwen-Image 20B + encoders | **Qwen-Image 20B** (via MFLUX) |
| Pony V6 XL + sdxl_vae | **Pony Diffusion V7** (AuraFlow) |
| — | **Z-Image Turbo 6B** (new, fast, Apache-2.0) |
</content>
