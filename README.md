# AI Image Generation

Local image generation using [ComfyUI](https://github.com/comfyanonymous/ComfyUI) and [Flux.1 Dev](https://huggingface.co/black-forest-labs/FLUX.1-dev) (via [MFLUX](https://github.com/filipstrand/mflux)) on Apple Silicon.

## ComfyUI

### Setup

```bash
cd comfyui
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

### Launch

```bash
./comfyui-start.sh
```

Opens the ComfyUI web interface at `http://localhost:8188`.

## Flux.1 Dev (Standalone via MFLUX)

Generate images from the command line using Flux.1 Dev (full BF16) natively on Apple Silicon.

### Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install mflux
```

### Usage

```bash
# Default prompt
./generate.sh

# Custom prompt
./generate.sh "a red panda wearing a tiny top hat, watercolor style"
```

Images are saved to `generated/` with timestamps.

### Example Prompts

```bash
./generate.sh "a vast cyberpunk cityscape at sunset, neon lights reflecting off wet streets, ultra detailed"
./generate.sh "an astronaut floating above Earth, photorealistic, golden hour lighting"
./generate.sh "a cozy cabin in a snowy forest, warm light glowing from windows, oil painting"
./generate.sh "a mechanical clockwork butterfly, intricate brass gears, macro photography"
./generate.sh "a samurai standing in a field of cherry blossoms, ink wash painting style"
```

### Notes

- First run downloads the Flux.1 Dev model (~24 GB BF16) from HuggingFace
- Requires Apple Silicon with Metal — runs natively, not in Docker
- 1024x1024 at 20 steps takes roughly 1-2 minutes on M4 Pro
- Adjust `STEPS` in `generate.sh` for speed/quality tradeoff (fewer = faster, more = finer detail)

## Pixel Art Pipeline

See `pixel-art-character-pipeline.md` for a workflow combining ComfyUI (Pony Diffusion) with image-to-video services to create 16-bit sprite animations.

## Project Structure

```
.
├── README.md
├── comfyui-start.sh              # Launch ComfyUI server
├── generate.sh                   # Flux.1 Dev CLI image generation
├── comfyui/                      # ComfyUI (upstream git repo)
├── generated/                    # Generated images
└── pixel-art-character-pipeline.md
```
