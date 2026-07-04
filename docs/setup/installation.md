# Installation — isolated toolchain

Everything is isolated: **no system-wide Python, no global pip installs.** The
system Python (3.14, Homebrew) is never touched.

## How isolation works

- [`uv`](https://docs.astral.sh/uv/) manages a pinned, standalone **Python 3.12**
  in its own cache and creates a per-tool virtualenv:
  - `.venv/` — MFLUX
  - `comfyui/.venv/` — ComfyUI
- `hf` (HuggingFace CLI) is an isolated `uv tool` (on `PATH`, not in a project venv).
- The venvs are git-ignored; delete one and re-run the steps below to rebuild it.

Python 3.12 is chosen deliberately: 3.14 is too new for the ML wheels (torch/mlx),
so `uv` provisions 3.12 without disturbing the system interpreter.

## From scratch on a fresh machine

```bash
brew install uv
uv python install 3.12

# --- MFLUX env (repo-local) ---
uv venv --python 3.12 .venv
uv pip install mflux hf_transfer

# --- ComfyUI env ---
git clone --depth 1 https://github.com/comfyanonymous/ComfyUI comfyui
uv venv --python 3.12 comfyui/.venv
( cd comfyui && uv pip install -r requirements.txt )
# custom nodes: Manager (model/node management) + GGUF loader (for FLUX.2)
git clone --depth 1 https://github.com/ltdrdata/ComfyUI-Manager comfyui/custom_nodes/ComfyUI-Manager
git clone --depth 1 https://github.com/city96/ComfyUI-GGUF     comfyui/custom_nodes/ComfyUI-GGUF
( cd comfyui && uv pip install -r custom_nodes/ComfyUI-Manager/requirements.txt )

# --- hf CLI as an isolated tool ---
uv tool install "huggingface_hub[hf_transfer]"
```

Verify:
```bash
.venv/bin/python -c "import mlx.core as mx, mflux; print('mlx', mx.__version__, 'mflux OK')"
comfyui/.venv/bin/python -c "import torch; print('torch', torch.__version__, 'MPS', torch.backends.mps.is_available())"
```

## HuggingFace login (for gated models)

Black Forest Labs FLUX repos are gated. Log in **with `HF_HOME` set** so the token
lands in the shared cache where MFLUX reads it:

```bash
source env.sh && hf auth login
```

Gating is **per-repo** — accepting `FLUX.2-klein-9B` does not grant `FLUX.2-dev`.
Accept each model's license on its HF page.

## Reliability setting: Xet disabled

`env.sh` sets `HF_HUB_DISABLE_XET=1`. The newer Xet chunk backend wedged during
setup (pulled bytes at full speed but committed ~0 % to disk). The classic HTTP
downloader is reliable and resumes cleanly. Leave this as-is unless you know Xet
is behaving. More in [tips-and-gotchas](../usage/tips-and-gotchas.md).

## Next

- [Download models & wire the shared store →](models.md)
- [Start generating →](../usage/generating.md)
</content>
