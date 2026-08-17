# Models Guide — which model for what

Hardware target: **Apple M5 Max, 64 GB**. See [tips-and-gotchas](tips-and-gotchas.md)
for the quantization/quality caveats (especially Qwen text).

## MFLUX models (native MLX CLI — `./generate.sh`)

Selected via `MFLUX_MODEL=<alias>`. First run auto-downloads into the shared HF
cache (`$HF_HOME`); nothing to place by hand.

| Alias | Model | Params | License | Best for |
|---|---|---|---|---|
| `z-image-turbo` | Z-Image Turbo (4-bit build) | 6B | Apache-2.0 | Fast, general, commercial-OK. ~18 s |
| `z-image` | Z-Image (base) | 6B | Apache-2.0 | Higher quality, more steps |
| `flux2` *(default)* | FLUX.2 [klein] 9B | 9B | FLUX dev non-commercial | **Best all-round**; great prompt adherence + text; ~8 s (4-step distilled) |
| `flux2-4b` | FLUX.2 [klein] 4B | 4B | FLUX dev non-commercial | Faster/lighter klein |
| `qwen` | **Qwen-Image (20B)** | 20B | Apache-2.0 | Original (Aug 2025). Gorgeous images, 2K. Text unreliable — see gotchas |
| `qwen-2512` | **Qwen-Image-2512 (20B)** | 20B | Apache-2.0 | **Recommended Qwen** — updated Dec-2025 base, better realism/detail/text. 8-bit MLX build |
| `flux-dev` | FLUX.1 [dev] | 12B | FLUX dev non-commercial | Classic Flux |
| `flux-schnell` | FLUX.1 [schnell] | 12B | Apache-2.0 | 4-step quick drafts |
| `krea` | FLUX.1 Krea [dev] | 12B | FLUX dev non-commercial | Photographic look |

> Note: `qwen` is the original **20B Qwen-Image** (Aug 2025); `qwen-2512` is the
> updated **Qwen-Image-2512** base (Dec 2025) — same ~20B, Apache-2.0, but improved
> realism, detail and text. Prefer `qwen-2512` unless you specifically want the
> original full-precision path for text (see gotchas). `qwen-2512` uses the
> pre-quantized `mlx-community/Qwen-Image-2512-8bit` build (~34 GB); the original
> `qwen` is a bigger (~58 GB) full-precision-capable download.
>
> The newer **Qwen-Image-2.0 (7B, Feb 2026)** and **3.0 (Jul 2026)** are **API-only
> with no open weights** — not runnable in MFLUX/MLX/ComfyUI at all, so they aren't
> an "MFLUX doesn't expose it" gap; there's simply nothing to download.

Variant selection uses `--model` (e.g. `--model flux2-klein-9b`), **not**
`--base-model` — `generate.sh` handles this per alias.

**Reference images** (`./generate.sh "prompt" image.png`, see
[generating](generating.md#reference-images)): every alias above can take one as an
img2img starting point. Instruction-style *editing* needs a dedicated model, and
`generate.sh` wires up two — `flux2` / `flux2-4b` (`mflux-generate-flux2-edit`,
multi-image, no extra weights) and `qwen` / `qwen-2512` (`mflux-generate-qwen-edit`,
downloads Qwen-Image-Edit-2509, ~58 GB).

MFLUX also does ControlNet / inpainting / upscaling, not wired up here — see the
remaining `mflux-generate-*` commands in `.venv/bin` (`mflux-generate-kontext`,
`mflux-generate-fill`, `mflux-upscale-seedvr2`, …).

### Quick picks
- **Everyday:** `flux2` (default) — or `z-image-turbo` when you want the fastest
  render, a smaller load, or an Apache-2.0 licence.
- **Prompt adherence + some text:** `flux2`.
- **Max photorealism:** `krea` or `flux2`.
- **Precise typography:** none of the MFLUX models nail it — use ComfyUI FLUX.2 dev.

## ComfyUI models (node workflows, LoRAs, pixel-art)

Single-file weights under the shared store (`$COMFY_MODELS_DIR`), fetched with
`./download-models.sh` or ComfyUI-Manager.

| Model | Files → dir | Purpose |
|---|---|---|
| **FLUX.2 [dev] 32B** (`comfy-flux2-dev`) | Q6_K GGUF → `unet/`, Mistral fp8 TE → `text_encoders/`, VAE → `vae/`, Turbo LoRA → `loras/` | Quality ceiling; best text. ~48 GB. See [setup/models](../setup/models.md) |
| **Pony Diffusion V7** (`comfy-pony-v7`) | fp8 checkpoint + TE + VAE | Pixel-art pipeline (AuraFlow). On MPS, native fp8 is unsupported — prefer a **GGUF** quant (`qpqpqpqpqpqp/pony_v7_base_GGUF`, Q6_K/Q8_0) via ComfyUI-GGUF if fp8 errors |
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
| Qwen-Image 20B + encoders | **Qwen-Image-2512 20B** (via MFLUX, `qwen-2512`) |
| Pony V6 XL + sdxl_vae | **Pony Diffusion V7** (AuraFlow) |
| — | **Z-Image Turbo 6B** (new, fast, Apache-2.0) |

## Update check — 2026-07-29

Web check of every model/tool in this guide against the latest releases. **Bottom
line:** almost everything is already the newest in its line; the one model upgrade
worth taking is **Qwen-Image → Qwen-Image-2512** (now `qwen-2512`), and **ComfyUI is
a couple of point releases behind**.

**Acted on (this update):**
- **Qwen** — original 20B superseded on the open-weight track by **Qwen-Image-2512**
  (2025-12-31, Apache-2.0, `Qwen/Qwen-Image-2512`): better realism/detail/text.
  Added the `qwen-2512` alias (`mlx-community/Qwen-Image-2512-8bit`, ~34 GB) and made
  it the recommended Qwen. Original `qwen` kept for the full-precision text path.
- **ComfyUI** — vendored copy is `0.27.0` (2026-06-30); latest is **`0.29.0`**
  (2026-07-29). Notable since 0.27: native **SeedVR2 upscaling** (0.28.0), **Krea 2**
  edit + **JoyImageEdit** (0.29.0). The new native int8/int4 "convrot" quant path is
  mostly NVIDIA-targeted — **keep ComfyUI-GGUF for FLUX.2 dev on MPS**. Update with a
  `git pull` in `comfyui/` when convenient (not auto-applied).

**Confirmed current — no change:**
- **MFLUX 0.18.0** (2026-06-07) — already the latest installed. Adds FLUX.2 Klein
  KV-cache (~2.4× faster multi-ref edits) + 9B OOM fixes.
- **FLUX.2** [klein] 9B/4B and [dev] 32B — latest checkpoints (Jan 2026). No FLUX.2
  Krea exists (Krea line stayed on FLUX.1).
- **FLUX.1** dev/schnell/krea — latest of the FLUX.1 line.
- **Z-Image** Turbo + base — latest (weights unchanged since Jan 2026).
- **Pony Diffusion V7.0** — still latest; V7.1 announced but unreleased.

**Watch-list (announced, no open weights yet — nothing to download):**
- **FLUX 3** (announced 2026-07-23) — multimodal image/video/audio. Only *Video* is
  live (partner API). Open-weight **FLUX 3 [dev]** promised "later in 2026".
- **Qwen-Image 2.0 (7B) / 3.0** — API-only, no weights.
- **Z-Image-Edit** and **Z-Image-Omni-Base** — listed "to be released", no repo/date.
</content>
