# Tips & Gotchas

Hard-won, model-specific knowledge. Read this before fighting a weird result.

## Quantization vs. quality (the big one)

`generate.sh` defaults to **8-bit** (`MFLUX_QUANT=8`) — great for images, fast, low
RAM. But quantization is **death for text rendering**. Set `MFLUX_QUANT=none`
(also accepts `off`/`full`/`0`) for full precision when you need legible letters.

| Model | q8 (default) | full precision (`MFLUX_QUANT=none`) |
|---|---|---|
| Z-Image Turbo | ~11 GB peak, ~18 s | (already 4-bit build — see below) |
| Qwen 20B | ~37 GB peak, ~2.5 min | **~62 GB peak, ~12–15 min** (maxes 64 GB) |
| FLUX.2 klein 9B | ~28 GB peak, ~8 s (4 steps) | — |

Full-precision Qwen used **61.6 GB of 64 GB** — close other apps first, and don't
run it alongside a big download.

## Qwen text rendering — manage expectations

Qwen-Image is marketed as the text king, but **via MFLUX it cannot reliably spell
exact words**, even at full precision + tuned guidance. Observed across ~5 attempts:

- q8 → ghosted/gibberish glyphs.
- full precision, guidance 2.5 → sharp letters but wrong spelling (`KYO⛩E`).
- full precision, guidance 4.5, text-only prompt → closer (`Y//OTO`) but still wrong.

It renders **beautiful images and clean letterforms** — just not dependable
typography. For precise text, use **FLUX.2** (much better at text) or the **native
Qwen-Image ComfyUI workflow** (proper text conditioning + memory management).
MFLUX recommends `--steps 30 --guidance 2.5` for Qwen generally.

## Z-Image Turbo — use the 4-bit build

The full `Tongyi-MAI/Z-Image-Turbo` repo **renders pure noise** through MFLUX
(regardless of quant/steps/guidance). The fix, baked into `generate.sh`:

- Use the mflux author's pre-quantized build: `-m filipstrand/Z-Image-Turbo-mflux-4bit`
  (self-contained, already 4-bit → **no `-q`**).
- **Guidance MUST be 0** for the turbo model; 9 steps.

`MFLUX_MODEL=z-image-turbo ./generate.sh "..."` already does all this.

## HuggingFace downloads — reliability

- **Xet backend is disabled** (`HF_HUB_DISABLE_XET=1` in `env.sh`). The Xet chunk
  backend wedged badly here (downloaded bytes at full speed but committed 0% to
  disk). The classic HTTP downloader is slower per-connection but reliable and
  resumes cleanly from `.incomplete` files.
- Transient `peer closed connection` errors are normal — the downloader
  auto-retries the affected file.
- Interrupted downloads can leave **orphaned `.incomplete` scratch files** that
  never finish. If a repo shows more bytes on disk than its true size, delete
  them: `find ~/ai-models/hf/hub -name '*.incomplete' -delete`.
- Gated repos (Black Forest Labs FLUX) need per-repo license acceptance on HF —
  accepting `FLUX.2-klein-9B` does **not** grant `FLUX.2-dev`. Log in with
  `hf auth login` (with `HF_HOME` set, i.e. `source env.sh` first, so the token
  lands in the shared cache).

## Bandwidth throttle (`throttle.sh`)

`sudo ./throttle.sh on 5` caps inbound to 5 MB/s via macOS `dummynet` (pf).

- **Any pf change resets live TCP connections** — toggling the throttle on *or*
  off mid-download wedges in-flight transfers. Set it once and leave it.
- The classic HF downloader survives the reset (reconnects); Xet did not.
- dummynet runs ~30 % over its setpoint, so `on 4` lands ~5 MB/s. It also shapes
  **all** inbound HTTP/HTTPS, not just downloads. `sudo ./throttle.sh off` when done.

## ComfyUI

- The `triton unavailable` line at startup is **expected on Apple Silicon**
  (triton is CUDA-only) and harmless.
- ComfyUI reads weights from the shared store via an auto-generated
  `comfyui/extra_model_paths.yaml` (written by `comfyui-start.sh`) — not from
  `comfyui/models/`.
- 64 GB unified memory is reported as shared VRAM.
</content>
