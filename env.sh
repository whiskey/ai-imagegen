#!/usr/bin/env bash
# Shared environment for the local image-generation tools.
#
# Source this file to point EVERY tool (MFLUX, ComfyUI, opencode, diffusers, ...)
# at one common model store, so weights are downloaded exactly once and reused.
#
#   Override the store location with:  export AI_MODELS_DIR=/some/other/path

# --- Common model store (tool-agnostic, lives OUTSIDE this repo on purpose) ---
export AI_MODELS_DIR="${AI_MODELS_DIR:-$HOME/ai-models}"

# HuggingFace cache — shared by MFLUX, diffusers, opencode and anything using
# huggingface_hub. All of them honour HF_HOME, so they share a single download.
export HF_HOME="$AI_MODELS_DIR/hf"

# The Xet chunk backend wedged under repeated pf/connection resets (downloaded
# bytes but committed 0% to cache). Force the classic HTTP downloader, which
# writes .incomplete files progressively and resumes reliably.
export HF_HUB_DISABLE_XET=1

# ComfyUI-style single-file weights (checkpoints/, loras/, vae/, ...) live here.
# ComfyUI reads them via comfyui/extra_model_paths.yaml (see comfyui-start.sh).
export COMFY_MODELS_DIR="$AI_MODELS_DIR/comfyui"

# --- Prompt enhancement (enhance.sh) ----------------------------------------
# The local LLM that expands a draft prompt into a full one. Served by Ollama,
# which keeps its own store — these weights are NOT in AI_MODELS_DIR. Both
# enhance.sh (which calls it) and generate.sh (which unloads it before a render,
# so the two don't fight over memory) read these.
export AI_ENHANCE_MODEL="${AI_ENHANCE_MODEL:-hf.co/lmstudio-community/Ministral-3-14B-Reasoning-2512-GGUF:Q4_K_M}"
export AI_ENHANCE_HOST="${AI_ENHANCE_HOST:-http://localhost:11434}"
