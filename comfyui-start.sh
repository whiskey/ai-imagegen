#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COMFY_DIR="$SCRIPT_DIR/comfyui"

source "$COMFY_DIR/.venv/bin/activate"
cd "$COMFY_DIR"

exec python main.py --force-fp16 --preview-method auto "$@"
