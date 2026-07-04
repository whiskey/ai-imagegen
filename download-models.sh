#!/usr/bin/env bash
set -euo pipefail

# Fetch model weights into the SHARED store (~/ai-models) so MFLUX, ComfyUI and
# opencode all reuse the same download.
#
#   MFLUX models  -> auto-download into the shared HF cache ($HF_HOME) on first
#                    use; this script "warms" them with a tiny test render.
#   ComfyUI files -> single-file weights fetched into $COMFY_MODELS_DIR/<subdir>.
#
# Usage:
#   ./download-models.sh z-image-turbo            # warm one MFLUX model
#   ./download-models.sh z-image-turbo qwen flux2 # warm several
#   ./download-models.sh comfy-flux2-klein        # fetch ComfyUI FLUX.2 klein set
#   ./download-models.sh comfy-pony-v7            # fetch Pony V7 checkpoint
#   ./download-models.sh list                     # show known targets

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/env.sh"
source "$SCRIPT_DIR/.venv/bin/activate"

warm_mflux() {  # $1 = friendly MFLUX_MODEL alias
  echo ">>> Warming MFLUX model '$1' (downloads into $HF_HOME, then a 256px test render)"
  MFLUX_MODEL="$1" MFLUX_SIZE=256 MFLUX_STEPS="${2:-4}" \
    "$SCRIPT_DIR/generate.sh" "a small test swatch, colorful" >/dev/null
  echo ">>> '$1' ready."
}

hf_get_file() {  # repo, path_in_repo, dest_subdir, out_name  (single file, renamed)
  local dest="$COMFY_MODELS_DIR/$3"; mkdir -p "$dest"
  if [ -f "$dest/$4" ]; then echo ">>> $4 already present, skipping"; return; fi
  # stage on the SAME volume as the store so the final mv is instant (not a
  # cross-filesystem copy — matters for the multi-GB GGUF/text-encoder files).
  local tmp; tmp="$(mktemp -d "$COMFY_MODELS_DIR/.dltmp.XXXXXX")"
  echo ">>> hf download $1 :: $2  ->  $dest/$4"
  hf download "$1" "$2" --local-dir "$tmp" >/dev/null
  mv "$tmp/$2" "$dest/$4"; rm -rf "$tmp"
}

for target in "$@"; do
  case "$target" in
    list)
      echo "MFLUX (native, Apple Silicon):"
      echo "  z-image-turbo  z-image  flux2  flux2-4b  qwen  flux-dev  flux-schnell  krea"
      echo "ComfyUI (single-file weights):"
      echo "  comfy-flux2-klein   comfy-pony-v7   comfy-qwen-image2"
      ;;
    z-image-turbo|z-image|flux2|flux2-4b|qwen|flux-dev|flux-schnell|krea)
      warm_mflux "$target" ;;

    comfy-pony-v7)  # Pony Diffusion V7 (AuraFlow) for the pixel-art pipeline
      hf_get_file qpqpqpqpqpqp/pony-v7-base-fp8_scaled \
        pony-v7-base-fp8_scaled_original_hybrid.safetensors checkpoints pony-v7-base-fp8.safetensors
      hf_get_file purplesmartai/pony-v7-base \
        vae/diffusion_pytorch_model.safetensors vae pony-v7-vae.safetensors
      hf_get_file purplesmartai/pony-v7-base \
        text_encoder/model.safetensors text_encoders pony-v7-text_encoder.safetensors
      echo ">>> Pony V7 ready in $COMFY_MODELS_DIR (checkpoints/ vae/ text_encoders/)" ;;

    comfy-flux2-dev)  # FLUX.2 [dev] 32B for ComfyUI (GGUF Q6_K + Mistral TE + VAE + Turbo LoRA)
      hf_get_file unsloth/FLUX.2-dev-GGUF flux2-dev-Q6_K.gguf unet flux2-dev-Q6_K.gguf
      hf_get_file Comfy-Org/flux2-dev \
        split_files/text_encoders/mistral_3_small_flux2_fp8.safetensors text_encoders mistral_3_small_flux2_fp8.safetensors
      hf_get_file Comfy-Org/flux2-dev split_files/vae/flux2-vae.safetensors vae flux2-vae.safetensors
      hf_get_file Comfy-Org/flux2-dev \
        split_files/loras/Flux2TurboComfyv2.safetensors loras flux2-dev-turbo.safetensors
      echo ">>> FLUX.2 dev ready: unet/flux2-dev-Q6_K.gguf + text_encoders/ + vae/ + loras/flux2-dev-turbo" ;;
    *)
      echo "Unknown target: $target (try: ./download-models.sh list)" >&2 ;;
  esac
done
