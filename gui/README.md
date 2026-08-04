# ai-imagegen-gui

A tiny cross-platform desktop window to generate images with the local models —
pick a model, type a prompt, click **Generate**, see the result. Built with
[`egui`](https://github.com/emilk/egui)/`eframe` (pure Rust, single binary).

It's a thin front-end: it shells out to the repo's `generate.sh` (setting
`MFLUX_MODEL`), so every model alias, quantization default, and output path stays
defined in one place. No model logic is duplicated here.

## Build & run

Needs the Rust toolchain (`rustup`, which provides `cargo`). Then:

```bash
cd gui
cargo run --release      # first build compiles egui — a few minutes, once
```

The release binary lands at `gui/target/release/ai-imagegen-gui` — double-click it
or copy it wherever you like.

## How it works

- The model dropdown lists the `generate.sh` aliases (`z-image-turbo`, `qwen-2512`,
  `flux2`, …). Selecting one sets `MFLUX_MODEL` for the run.
- **Generate** runs `bash generate.sh "<prompt>"` on a background thread, so the
  window stays responsive during the render (~18 s for `z-image-turbo`, longer for
  the 20 B models).
- On success the script prints `Saved: <path>`; the app parses that line and loads
  the PNG. Images are also written to `../generated/` as usual.

## Notes / limits (it's a "simple start")

- **Backend is macOS / Apple Silicon** (MFLUX via `generate.sh`, a bash script).
  The *GUI* is cross-platform, but the generation backend is not — on Windows/Linux
  you'd point it at a different backend. The path to `generate.sh` is the parent of
  this crate; override with `AI_IMAGEGEN_ROOT=/path/to/ai-imagegen`.
- No steps/seed/size controls yet — uses each model's defaults from `generate.sh`.
  Those map cleanly onto extra widgets when wanted (env vars `MFLUX_STEPS`,
  `MFLUX_SEED`, `MFLUX_SIZE`).
- No live progress bar; a spinner shows while the render runs.
