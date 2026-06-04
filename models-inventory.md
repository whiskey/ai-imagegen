# Models Inventory (snapshot 2026-06-04)

Snapshot taken before clearing `comfyui/models/` to free disk space.
Plan: re-download updated versions later, not these exact files.

Total: **10 files, ~34 GB** under `comfyui/models/`.

## checkpoints/

| File | Size | Notes |
|---|---|---|
| `flux1-dev-fp8.safetensors` | 16 GB | Flux.1 Dev FP8 — all-in-one ComfyUI checkpoint. Originally from Comfy-Org/flux1-dev on HF. |
| `ponyDiffusionV6XL_v6StartWithThisOne.safetensors` | 6.5 GB | Pony Diffusion V6 XL — SDXL-based, used by pixel-art pipeline. |

## diffusion_models/

| File | Size | Notes |
|---|---|---|
| `anima-preview.safetensors` | 3.9 GB | Anima preview diffusion model. |

## text_encoders/

| File | Size | Notes |
|---|---|---|
| `t5xxl_fp8_e4m3fn.safetensors` | 4.6 GB | T5-XXL FP8 — paired with Flux when using split-component workflow. |
| `clip_l.safetensors` | 235 MB | CLIP-L text encoder for Flux. |
| `qwen_3_06b_base.safetensors` | 1.1 GB | Qwen 3 0.6B base — text encoder for Qwen-Image workflow. |

## vae/

| File | Size | Notes |
|---|---|---|
| `ae.safetensors` | 320 MB | Flux autoencoder (`ae`). |
| `sdxl_vae.safetensors` | 319 MB | SDXL VAE — used with Pony Diffusion. |
| `qwen_image_vae.safetensors` | 242 MB | Qwen-Image VAE. |

## loras/

| File | Size | Notes |
|---|---|---|
| `flux-uncensored-v2.safetensors` | 656 MB | Flux uncensored LoRA v2. |

## Other caches (not deleted)

- `~/.cache/huggingface/hub/` — only metadata stubs (4 KB each for `models--black-forest-labs--FLUX.1-dev` and `models--Comfy-Org--flux1-dev`); no actual weights cached. MFLUX (per `generate.sh` / README) would download Flux.1 Dev BF16 (~24 GB) here on first run — currently not present.

## Workflow groupings (for re-downloading)

- **ComfyUI Flux all-in-one**: `flux1-dev-fp8` (checkpoints).
- **ComfyUI Flux split**: `t5xxl_fp8_e4m3fn` + `clip_l` (text_encoders) + `ae` (vae) + Flux unet/diffusion model. Add `flux-uncensored-v2` LoRA as desired.
- **Pony Diffusion / pixel-art pipeline**: `ponyDiffusionV6XL_v6StartWithThisOne` (checkpoints) + `sdxl_vae` (vae).
- **Qwen-Image**: `qwen_3_06b_base` (text_encoders) + `qwen_image_vae` (vae) + Qwen diffusion model (was the `anima-preview`? confirm before re-downloading).
- **MFLUX CLI** (`generate.sh`): downloaded automatically by `mflux-generate` on first run into `~/.cache/huggingface/`.
