#!/usr/bin/env bash
set -euo pipefail

# CLI image generation on Apple Silicon via MFLUX (native MLX, no Docker).
#
# Usage:
#   ./generate.sh "your prompt here"
#   MFLUX_MODEL=qwen ./generate.sh "a poster that reads 'HELLO WORLD'"
#
# Model is chosen with MFLUX_MODEL (default: z-image-turbo). Friendly aliases:
#   z-image-turbo  Z-Image Turbo 6B  Apache-2.0, ~8 steps, fastest  (default)
#   z-image        Z-Image 6B        higher quality, more steps
#   flux2          FLUX.2 [klein] 9B latest Black Forest Labs, editing-capable
#   flux2-4b       FLUX.2 [klein] 4B smaller/faster klein
#   qwen           Qwen-Image 20B    original (Aug 2025); full-precision-capable text
#   qwen-2512      Qwen-Image-2512   updated 20B base (Dec 2025), 8-bit MLX build (recommended)
#   flux-dev       FLUX.1 [dev] 12B  classic Flux
#   flux-schnell   FLUX.1 [schnell]  4-step Flux
#   krea           FLUX.1 Krea [dev] photographic Flux
# Any raw MFLUX --base-model value also works (e.g. MFLUX_MODEL=flux2-klein-base-9b).
#
# Tunables (env vars):
#   MFLUX_QUANT=8     quantization bits: 3/4/5/6/8, or "" for full precision
#   MFLUX_STEPS=...   override inference steps
#   MFLUX_SEED=...    fixed seed (default: random)
#   MFLUX_SIZE=1024   square size shorthand, or set MFLUX_W / MFLUX_H
#   MFLUX_LOWRAM=1    enable --low-ram

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/env.sh"
# shellcheck disable=SC1091  # venv is created at setup time, not in the repo
source "$SCRIPT_DIR/.venv/bin/activate"

MODEL="${MFLUX_MODEL:-z-image-turbo}"
QUANT="${MFLUX_QUANT:-8}"
# MFLUX_QUANT=none|off|full|0 -> full precision (skip -q). Needed for Qwen text,
# which quantization garbles — costs ~62GB RAM, so close other apps first.
case "$QUANT" in none|off|full|0) QUANT="" ;; esac

# Each model family has its OWN mflux command; the generic `mflux-generate` is
# FLUX.1 only (passing --base-model z-image/qwen to it routes through the wrong
# model class). Map alias -> (command+variant, default steps).
DEF_GUIDANCE=""; PREQUANT=""
case "$MODEL" in
  # the full Tongyi Z-Image repo renders pure noise via mflux; use the author's
  # pre-quantized 4-bit build (self-contained, already 4-bit so no -q), guidance MUST be 0.
  z-image-turbo) CMD=(mflux-generate-z-image-turbo -m filipstrand/Z-Image-Turbo-mflux-4bit); DEF_STEPS=9; DEF_GUIDANCE=0; PREQUANT=1 ;;
  z-image)       CMD=(mflux-generate-z-image);                    DEF_STEPS=28 ;;
  flux2)         CMD=(mflux-generate-flux2 --model flux2-klein-9b); DEF_STEPS=4 ;;  # variant is --model, NOT --base-model
  flux2-4b)      CMD=(mflux-generate-flux2 --model flux2-klein-4b); DEF_STEPS=4 ;;
  qwen)          CMD=(mflux-generate-qwen);                       DEF_STEPS="" ;;
  # updated Dec-2025 base as a pre-quantized 8-bit MLX build (self-contained, skip -q).
  qwen-2512)     CMD=(mflux-generate-qwen -m mlx-community/Qwen-Image-2512-8bit); DEF_STEPS=20; PREQUANT=1 ;;
  flux-dev|dev)  CMD=(mflux-generate --model dev);                DEF_STEPS=20 ;;
  flux-schnell|schnell) CMD=(mflux-generate --model schnell);     DEF_STEPS=4  ;;
  krea)          CMD=(mflux-generate --model krea-dev);           DEF_STEPS=28 ;;
  *)             CMD=(mflux-generate --model "$MODEL");            DEF_STEPS="" ;;  # raw model name
esac

STEPS="${MFLUX_STEPS:-$DEF_STEPS}"
GUIDANCE="${MFLUX_GUIDANCE:-$DEF_GUIDANCE}"
SIZE="${MFLUX_SIZE:-1024}"
WIDTH="${MFLUX_W:-$SIZE}"
HEIGHT="${MFLUX_H:-$SIZE}"

OUTPUT_DIR="$SCRIPT_DIR/generated"
mkdir -p "$OUTPUT_DIR"
PROMPT="${1:-a vast cyberpunk cityscape at sunset, neon lights reflecting off wet streets, ultra detailed}"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
OUTPUT_FILE="$OUTPUT_DIR/${TIMESTAMP}_${MODEL}.png"

# Assemble args (CMD already carries the right command + --base-model variant)
ARGS=(--prompt "$PROMPT" --width "$WIDTH" --height "$HEIGHT"
      --output "$OUTPUT_FILE" --metadata)
[ -n "$STEPS" ] && ARGS+=(--steps "$STEPS")
[ -n "$GUIDANCE" ] && ARGS+=(--guidance "$GUIDANCE")
[ -z "$PREQUANT" ] && [ -n "$QUANT" ] && ARGS+=(-q "$QUANT")   # skip -q for pre-quantized builds
[ -n "${MFLUX_SEED:-}" ] && ARGS+=(--seed "$MFLUX_SEED")
[ -n "${MFLUX_LOWRAM:-}" ] && ARGS+=(--low-ram)

echo "Model:  $MODEL  ->  ${CMD[*]}"
echo "Quant:  ${QUANT:-full precision}   Steps: ${STEPS:-model default}   Size: ${WIDTH}x${HEIGHT}"
echo "Prompt: $PROMPT"
echo "Cache:  $HF_HOME"
echo "Output: $OUTPUT_FILE"
echo

"${CMD[@]}" "${ARGS[@]}"

echo
echo "Saved: $OUTPUT_FILE"
