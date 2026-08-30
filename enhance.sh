#!/usr/bin/env bash
set -euo pipefail

# Expand a short draft into a full image prompt, using a local LLM via Ollama.
#
# For when you know what you want a picture of but don't want to write out the
# setting, the light and the composition. Prints the expanded prompt on stdout
# and nothing else, so it can be captured:
#
#   ./enhance.sh "a dragon"
#   PROMPT="$(./enhance.sh "a dragon")" && ./generate.sh "$PROMPT"
#
# The draft's own nouns are always kept — this elaborates, it does not reinterpret.
#
# Modes (AI_ENHANCE_MODE), because "a good prompt" is not one thing here:
#   scene        (default) describe a whole picture: subject, setting, light, mood
#   coloring     for MFLUX_COLORING renders: shape and detail only, no colour or
#                lighting words — those fight the line-art recipe generate.sh appends
#   instruction  for reference-image edits: stays an instruction ("put a scarf on
#                the fox"), says what to change and what to keep, invents no new scene
#
# Tunables (env vars):
#   AI_ENHANCE_MODEL=...   Ollama model (default in env.sh); `ollama list` to see yours
#   AI_ENHANCE_WORDS=...   length cap; default 60 scene / 70 coloring / 30 instruction
#   AI_ENHANCE_HOST=...    Ollama endpoint (default http://localhost:11434)
#   AI_ENHANCE_KEEP=5m     how long Ollama keeps the model resident afterwards.
#                          Set 0 to unload immediately — see the note below.
#
# On memory: a 14B at Ollama's default context sits at ~31 GB resident, and an
# mflux render peaks near 28 GB. Both at once will swap a 64 GB machine, so
# `generate.sh` unloads this model before it renders. Nothing to configure; it
# just means the first enhance after a render pays the load again (~20 s).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/env.sh"   # AI_ENHANCE_MODEL / AI_ENHANCE_HOST defaults

MODEL="$AI_ENHANCE_MODEL"
HOST="$AI_ENHANCE_HOST"
# Set per mode further down unless asked for explicitly — a scene wants prose,
# an edit instruction wants to stay terse.
WORDS="${AI_ENHANCE_WORDS:-}"
KEEP="${AI_ENHANCE_KEEP:-5m}"
MODE="${AI_ENHANCE_MODE:-scene}"
case "$MODE" in
  scene)       WORDS="${WORDS:-60}" ;;
  coloring)    WORDS="${WORDS:-70}" ;;
  instruction) WORDS="${WORDS:-30}" ;;
esac

DRAFT="${1:-}"
[ -n "${DRAFT//[[:space:]]/}" ] || { echo "Usage: ./enhance.sh \"a short draft\"" >&2; exit 2; }

command -v jq >/dev/null || { echo "enhance.sh needs jq (brew install jq)" >&2; exit 3; }
curl -sf -m 5 "$HOST/api/version" >/dev/null 2>&1 || {
  echo "No Ollama at $HOST — start it with 'ollama serve' (brew install ollama)." >&2
  exit 4
}

COMMON="Reply with the prompt ONLY: no preamble, no quotes, no markdown, no options,
no explanation, no trailing commentary. One paragraph, $WORDS words at most.
Keep every subject the draft names — elaborate it, never replace it.
Never add text, letters, words, signatures or watermarks to the picture."

case "$MODE" in
  scene)
    SYSTEM="You expand short drafts into vivid prompts for a text-to-image model.
Add setting, composition, lighting, time of day, mood and concrete visual
detail that suit what the draft already describes.
$COMMON" ;;
  coloring)
    # The line-art recipe generate.sh appends does the styling. Colour, lighting
    # and texture words here would contradict it, so this mode adds none.
    SYSTEM="You expand short drafts into prompts for a black-and-white coloring
book page. Add subject detail, pose, props, setting and composition — things that
become SHAPES to colour in, plus decorative pattern worth outlining.
Never mention colour, paint, hue, shading, lighting, sunlight, shadow, texture,
photography or any art medium: the line-art style is added separately and those
words fight it.
$COMMON" ;;
  instruction)
    # The picture is in front of the *image* model, not this one. Anything it
    # says about the subject is therefore a guess, and a guess that reaches the
    # edit model as fact — hence the flat ban on naming what it cannot see.
    SYSTEM="You sharpen one short instruction for an image-editing model that is
looking at a reference picture. You CANNOT see that picture.
Make the requested change specific, then add a short clause preserving what should
stay: pose, background, framing, lighting, likeness.
You must NOT name, describe, gender or invent any subject, person, object, place,
garment or title the draft does not itself name — you do not know what is there.
Reuse the draft's own words and pronouns for whatever it refers to.
Describe no new scene: the picture already exists.
$COMMON" ;;
  *) echo "AI_ENHANCE_MODE must be scene, coloring or instruction (got '$MODE')" >&2; exit 2 ;;
esac

REQ="$(jq -Rn --arg m "$MODEL" --arg s "$SYSTEM" --arg p "$DRAFT" --arg k "$KEEP" \
  '{model:$m, system:$s, prompt:$p, stream:false, think:false, keep_alive:$k,
    options:{temperature:0.8, num_predict:400}}')"

RESP="$(curl -sf -m 300 "$HOST/api/generate" -d "$REQ")" || {
  echo "enhance.sh: Ollama request failed (is '$MODEL' pulled? try 'ollama list')" >&2
  exit 5
}

ERR="$(printf '%s' "$RESP" | jq -r '.error // empty')"
[ -z "$ERR" ] || { echo "enhance.sh: Ollama said: $ERR" >&2; exit 5; }

# Reasoning models leak a <think> block even with think:false on some builds, and
# instruction-tuned ones like to wrap the answer in quotes. Strip both, collapse
# the paragraph onto one line, and trim.
printf '%s' "$RESP" | jq -r '.response' | perl -0777 -pe '
  s{<think>.*?</think>}{}gs;      # drop reasoning traces
  s{\A\s+|\s+\z}{}g;              # trim
  s{\A["“”'"'"']+|["“”'"'"']+\z}{}g;  # unwrap quoted answers
  s{\s*\n\s*}{ }g;                # one paragraph, one line
  s{\s{2,}}{ }g;
'
echo
