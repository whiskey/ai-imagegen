#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/.venv/bin/activate"

MODEL="dev"
STEPS=20
OUTPUT_DIR="$SCRIPT_DIR/generated"
mkdir -p "$OUTPUT_DIR"

PROMPT="${1:-a vast cyberpunk cityscape at sunset, neon lights reflecting off wet streets, ultra detailed}"

TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
OUTPUT_FILE="$OUTPUT_DIR/${TIMESTAMP}.png"

echo "Model:  Flux.1 Dev (BF16)"
echo "Steps:  $STEPS"
echo "Prompt: $PROMPT"
echo "Output: $OUTPUT_FILE"
echo ""

mflux-generate \
    --model "$MODEL" \
    --steps "$STEPS" \
    --seed -1 \
    --height 1024 \
    --width 1024 \
    --prompt "$PROMPT" \
    --output "$OUTPUT_FILE"
