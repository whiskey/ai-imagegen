#!/usr/bin/env bash
set -euo pipefail

# CLI image generation on Apple Silicon via MFLUX (native MLX, no Docker).
#
# Usage:
#   ./generate.sh "your prompt here"
#   MFLUX_MODEL=qwen ./generate.sh "a poster that reads 'HELLO WORLD'"
#   ./generate.sh "put the glasses on her" person.jpg glasses.jpg   # reference images
#
# Model is chosen with MFLUX_MODEL (default: flux2). Friendly aliases:
#   z-image-turbo  Z-Image Turbo 6B  Apache-2.0, ~8 steps, fastest
#   z-image        Z-Image 6B        higher quality, more steps
#   flux2          FLUX.2 [klein] 9B latest Black Forest Labs, editing-capable  (default)
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
#                     (with a reference image and none of these set: its own size)
#   MFLUX_MAX_MP=1.3  cap for that reference-derived size, in megapixels
#   MFLUX_LOWRAM=1    enable --low-ram
#
# Reference images (see the mode block below):
#   MFLUX_IMAGE=path      one reference image, env-var form of the extra args
#   MFLUX_MODE=auto       auto (default) | edit | img2img
#   MFLUX_STRENGTH=0.4    img2img only: how much of the reference survives (0..1)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/env.sh"
# shellcheck disable=SC1091  # venv is created at setup time, not in the repo
source "$SCRIPT_DIR/.venv/bin/activate"

# $1 is the prompt; any further positional args are reference images.
PROMPT="${1:-a vast cyberpunk cityscape at sunset, neon lights reflecting off wet streets, ultra detailed}"
REF_IMAGES=()
if [ "$#" -gt 1 ]; then
  shift
  REF_IMAGES=("$@")
elif [ -n "${MFLUX_IMAGE:-}" ]; then
  REF_IMAGES=("$MFLUX_IMAGE")
fi
for ref in ${REF_IMAGES[@]+"${REF_IMAGES[@]}"}; do
  [ -f "$ref" ] || { echo "Reference image not found: $ref" >&2; exit 1; }
done

MODEL="${MFLUX_MODEL:-flux2}"
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

# A reference image can be used two ways, and they are NOT the same thing:
#   img2img  --image-path/--image-strength on the normal command: the reference is
#            only the starting point of the denoise, so the prompt still has to
#            describe the whole picture. Every model above supports this.
#   edit     the family's own *-edit command: the reference is real conditioning,
#            the prompt is an instruction ("put the glasses on her"), and several
#            references can be combined. FLUX.2 [klein] does this with the weights
#            it already has; Qwen pulls its own Qwen-Image-Edit-2509 (~58 GB).
EDIT_CMD=(); EDIT_STEPS=""
case "$MODEL" in
  flux2)          EDIT_CMD=(mflux-generate-flux2-edit --model flux2-klein-9b); EDIT_STEPS=4 ;;
  flux2-4b)       EDIT_CMD=(mflux-generate-flux2-edit --model flux2-klein-4b); EDIT_STEPS=4 ;;
  qwen|qwen-2512) EDIT_CMD=(mflux-generate-qwen-edit);                         EDIT_STEPS=20 ;;  # this tool ignores -m
esac

MODE="${MFLUX_MODE:-auto}"
case "$MODE" in auto|edit|img2img) ;; *) echo "MFLUX_MODE must be auto, edit or img2img (got '$MODE')" >&2; exit 1 ;; esac
if [ "${#REF_IMAGES[@]}" -eq 0 ]; then
  [ "$MODE" = auto ] || { echo "MFLUX_MODE=$MODE needs a reference image: ./generate.sh \"prompt\" image.png" >&2; exit 1; }
  MODE=none
elif [ "$MODE" = auto ]; then
  # Prefer true reference conditioning wherever it costs no extra download.
  case "$MODEL" in flux2|flux2-4b) MODE=edit ;; *) MODE=img2img ;; esac
fi

if [ "$MODE" = edit ]; then
  if [ "${#EDIT_CMD[@]}" -eq 0 ]; then
    echo "No edit model in mflux for '$MODEL' — use MFLUX_MODE=img2img, or flux2 / qwen." >&2
    exit 1
  fi
  CMD=("${EDIT_CMD[@]}")
  DEF_STEPS="$EDIT_STEPS"
  DEF_GUIDANCE=""   # the edit tools pick their own (1.0 for FLUX.2, 2.5 for Qwen)
  PREQUANT=""       # edit weights come unquantized, so -q applies again
fi
[ "$MODE" = img2img ] && [ "${#REF_IMAGES[@]}" -gt 1 ] &&
  echo "Note: img2img takes a single reference; using ${REF_IMAGES[0]}" >&2

STEPS="${MFLUX_STEPS:-$DEF_STEPS}"
GUIDANCE="${MFLUX_GUIDANCE:-$DEF_GUIDANCE}"
# With a reference image and no size asked for, pass no --width/--height at all:
# the tools that default to the source image (flux2, flux2-edit, z-image-turbo) then
# keep its aspect ratio instead of squashing it into a square. The rest (qwen,
# z-image base, flux1) fall back to their own 1024² default.
#
# But NOT unbounded: a 2880x1800 wallpaper as reference renders at 2880x1800, which
# peaked at 69 GB here and spent the run swapping. Past MFLUX_MAX_MP megapixels we
# scale the reference's aspect down and pass the result explicitly.
if [ "$MODE" != none ] && [ -z "${MFLUX_SIZE:-}${MFLUX_W:-}${MFLUX_H:-}" ]; then
  WIDTH=""; HEIGHT=""; SIZE_DESC="reference image"
  CAP="$("$SCRIPT_DIR/.venv/bin/python" - "${REF_IMAGES[0]}" "${MFLUX_MAX_MP:-1.3}" <<'PY' || true
import sys
from PIL import Image
limit = float(sys.argv[2]) * 1_000_000
w, h = Image.open(sys.argv[1]).size
if w * h > limit:
    scale = (limit / (w * h)) ** 0.5
    # mflux rounds non-multiples of 16 down and warns; snap here instead.
    print(16 * max(1, int(w * scale) // 16), 16 * max(1, int(h * scale) // 16))
PY
)"
  if [ -n "$CAP" ]; then
    WIDTH="${CAP% *}"
    HEIGHT="${CAP#* }"
    SIZE_DESC="${WIDTH}x${HEIGHT}  (reference scaled to ${MFLUX_MAX_MP:-1.3} MP)"
  fi
else
  SIZE="${MFLUX_SIZE:-1024}"
  WIDTH="${MFLUX_W:-$SIZE}"
  HEIGHT="${MFLUX_H:-$SIZE}"
  SIZE_DESC="${WIDTH}x${HEIGHT}"
fi

OUTPUT_DIR="$SCRIPT_DIR/generated"
mkdir -p "$OUTPUT_DIR"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
OUTPUT_FILE="$OUTPUT_DIR/${TIMESTAMP}_${MODEL}.png"

# Assemble args (CMD already carries the right command + --base-model variant)
ARGS=(--prompt "$PROMPT" --output "$OUTPUT_FILE" --metadata)
[ -n "$WIDTH" ] && ARGS+=(--width "$WIDTH")
[ -n "$HEIGHT" ] && ARGS+=(--height "$HEIGHT")
[ -n "$STEPS" ] && ARGS+=(--steps "$STEPS")
[ -n "$GUIDANCE" ] && ARGS+=(--guidance "$GUIDANCE")
[ -z "$PREQUANT" ] && [ -n "$QUANT" ] && ARGS+=(-q "$QUANT")   # skip -q for pre-quantized builds
[ -n "${MFLUX_SEED:-}" ] && ARGS+=(--seed "$MFLUX_SEED")
[ -n "${MFLUX_LOWRAM:-}" ] && ARGS+=(--low-ram)
case "$MODE" in
  edit)    ARGS+=(--image-paths "${REF_IMAGES[@]}") ;;
  img2img) ARGS+=(--image-path "${REF_IMAGES[0]}")
           [ -n "${MFLUX_STRENGTH:-}" ] && ARGS+=(--image-strength "$MFLUX_STRENGTH") ;;
esac

echo "Model:  $MODEL  ->  ${CMD[*]}"
echo "Quant:  ${QUANT:-full precision}   Steps: ${STEPS:-model default}   Size: $SIZE_DESC"
[ "$MODE" != none ] && echo "Ref:    ${REF_IMAGES[*]}   (mode: $MODE)"
echo "Prompt: $PROMPT"
echo "Cache:  $HF_HOME"
echo "Output: $OUTPUT_FILE"
echo

"${CMD[@]}" "${ARGS[@]}"

echo
echo "Saved: $OUTPUT_FILE"
