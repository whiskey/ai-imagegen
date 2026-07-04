# AI Image Generation

Local image generation on Apple Silicon (**M5 Max, 64 GB**), two ways:

- **[MFLUX](https://github.com/filipstrand/mflux)** — native MLX, command-line, fastest. Z-Image, FLUX.2 [klein], Qwen-Image, FLUX.1.
- **[ComfyUI](https://github.com/comfyanonymous/ComfyUI)** — node UI for workflows, LoRAs, FLUX.2 [dev] 32B, and the pixel-art pipeline.

Everything is isolated (no system-wide Python) and every tool shares **one model
store** (`~/ai-models/`) so weights download once. Runs natively on Metal/MPS.

## Quick start

```bash
./generate.sh "a red panda wearing a tiny top hat, watercolor style"   # MFLUX CLI
MFLUX_MODEL=flux2 ./generate.sh "an astronaut above Earth, golden hour"
./comfyui-start.sh                                                       # ComfyUI @ :8188
```

## Documentation

Sorted into **setup** (provision once) and **usage & tips** (day-to-day):

**docs/setup/**
- [installation.md](docs/setup/installation.md) — isolated toolchain (uv, Python 3.12, venvs, ComfyUI, HF login)
- [models.md](docs/setup/models.md) — downloading models + the shared store (opencode reuse, FLUX.2 dev)
- [models-inventory.md](docs/setup/models-inventory.md) — archived pre-clear snapshot

**docs/usage/**
- [generating.md](docs/usage/generating.md) — `generate.sh`, tunables, recipes, ComfyUI launch
- [comfyui-flux2-dev.md](docs/usage/comfyui-flux2-dev.md) — FLUX.2 [dev] 32B in ComfyUI (ready-made workflows, best text)
- [models-guide.md](docs/usage/models-guide.md) — which model for what (aliases, params, licenses)
- [tips-and-gotchas.md](docs/usage/tips-and-gotchas.md) — **read this** (quant vs. text, Z-Image fix, download reliability, throttle)
- [pixel-art-pipeline.md](docs/usage/pixel-art-pipeline.md) — Pony V7 → sprite frames

## Scripts

| Script | Purpose |
|---|---|
| `generate.sh` | MFLUX multi-model CLI generation |
| `comfyui-start.sh` | launch ComfyUI wired to the shared store |
| `download-models.sh` | fetch/warm models into the shared store |
| `env.sh` | shared env (HF_HOME, store paths) — sourced by the others |
| `throttle.sh` | optional macOS inbound-bandwidth cap (see gotchas) |

## Layout

```
.
├── README.md · env.sh · generate.sh · comfyui-start.sh · download-models.sh · throttle.sh
├── docs/{setup,usage}/           # documentation
├── .venv/                        # MFLUX venv            (git-ignored)
├── comfyui/                      # ComfyUI clone + venv  (git-ignored)
└── generated/                    # output images         (git-ignored)

~/ai-models/                      # shared model store    (outside the repo)
```
</content>
