# FLUX.2 [dev] 32B in ComfyUI

The quality ceiling of this setup — best prompt adherence and, crucially, **the
only model here that renders exact text reliably** (spells "KYOTO" cleanly where
MFLUX-Qwen and FLUX.2-klein garble it). Runs on 64 GB via ComfyUI-GGUF.

## Ready-made workflows

Two presets are saved to `comfyui/user/default/workflows/` (they appear in the
ComfyUI **Workflows** sidebar; refresh the browser tab if new) and mirrored in the
repo under `workflows/`:

| Workflow | Steps | Use |
|---|---|---|
| **FLUX.2-dev-Turbo** | 8 (Turbo LoRA) | **Default.** Fast (~30 s), rich look. |
| **FLUX.2-dev-Turbo-HiRes** | 20 (Turbo LoRA) | Same look, a touch crisper. Hero shots. |

**Use:** open one from the sidebar → edit the text in the **positive
`CLIPTextEncode`** node → **Queue**. Files are pre-selected; nothing else to wire.

## The node graph

```
UnetLoaderGGUF(flux2-dev-Q6_K.gguf) → LoraLoaderModelOnly(flux2-dev-turbo, 1.0) ┐
CLIPLoader(mistral_3_small_flux2_fp8, type=flux2) → CLIPTextEncode(prompt)        │
                                                    → FluxGuidance(4.0) ──────────┤→ KSampler → VAEDecode(flux2-vae) → SaveImage
                                                    → CLIPTextEncode("") ──────────┘   (cfg 1, euler, simple)
EmptyFlux2LatentImage(1024²) ───────────────────────────────────────────────────┘
```

Key node facts (verified against the live registry):
- **`CLIPLoader` type must be `flux2`** (single Mistral encoder). Do **not** use
  `CLIPTextEncodeFlux` — that's the FLUX.1 dual-encoder node (clip_l + t5xxl).
- FLUX.2 is guidance-distilled → **cfg = 1**, and the real guidance is set by the
  **`FluxGuidance`** node (~3.5–4.5).
- Use **`EmptyFlux2LatentImage`**, not the SD/Flux1 empty-latent node.

## Settings that matter (tested)

- **Keep the Turbo LoRA.** It's not just a step-shortcut — it bakes in a richer,
  denser aesthetic. Dropping it for "more steps" produced *flatter, duller* images.
  So Turbo-8 or Turbo-20, not no-LoRA-25.
- **Steps:** 8 (turbo) is the sweet spot; 16–20 adds crispness with the same look.
- **Guidance:** 4 is a good default (`FluxGuidance` node). Higher = punchier.
- **RAM:** ~40 GB resident (26 GB model + ~14 GB encoder ComfyUI swaps out after
  encoding). Comfortable on 64 GB.

## What it can and can't do (tested)

- ✅ **Exact text** — renders headlines like "KYOTO" correctly, even a poster full
  of it.
- ✅ **Layered composition** — will put foliage/branches *in front of* text with
  real depth.
- ⚠️ **Won't fully hide text** — it has a strong legibility bias: it partially
  overlaps letters but resists burying them. For heavy occlusion (a letter mostly
  hidden), use **img2img / inpainting** (paint a branch over the rendered letter,
  regenerate that region) rather than prompting.

Files live in the shared store (`~/ai-models/comfyui/{unet,text_encoders,vae,loras}`);
see [setup/models](../setup/models.md#flux2-dev-comfyui) for the download recipe.
</content>
