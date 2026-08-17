# ai-imagegen-gui

A tiny cross-platform desktop window to generate images with the local models —
pick a model, type a prompt, optionally drop in a reference image, click
**Generate**, see the result. Built with
[`egui`](https://github.com/emilk/egui)/`eframe` (pure Rust, single binary).

It's a thin front-end: it shells out to the repo's `generate.sh` (setting
`MFLUX_MODEL` and friends), so every model alias, quantization default, and output
path stays defined in one place. No model logic is duplicated here.

## Build & run

Needs the Rust toolchain (`rustup`, which provides `cargo`). Then:

```bash
cd gui
cargo run --release      # first build compiles egui — a few minutes, once
```

The release binary lands at `gui/target/release/ai-imagegen-gui` — double-click it
or copy it wherever you like.

## Package as a macOS .app

To get a double-clickable Finder app:

```bash
cd gui
./make-app.sh            # -> dist/AI ImageGen.app  (arm64, ad-hoc signed)
open "dist/AI ImageGen.app"
```

`make-app.sh` builds the release binary and wraps it in a standard bundle
(`Contents/{Info.plist,MacOS/…}`). It's ad-hoc signed, so it launches without a
Gatekeeper prompt on the machine that built it. Move it to `/Applications` if you
like.

The app locates `generate.sh` via the repo path baked in at build time. If you
relocate the checkout, launch with `AI_IMAGEGEN_ROOT=/path/to/ai-imagegen` set (or
rebuild). `dist/` is gitignored.

### Icon

A teal→navy squircle holding a mountain range that dissolves into pixels on the
left — a denoising pass caught halfway. To change it, edit the drawing in
`icon/gen-icon.py` and regenerate:

```bash
./make-icon.sh           # redraws icon_1024.png + rebuilds icon/AppIcon.icns
./make-app.sh            # re-bundle so the app picks it up
```

`make-icon.sh` needs the repo venv (Pillow); `make-app.sh` only needs the committed
`.icns`. Both outputs are committed, because **the icon is set in two places**:

- `icon/AppIcon.icns` → `Contents/Resources` + `CFBundleIconFile`. This is what
  Finder and the Dock use *until the process is up*.
- `icon/icon_1024.png` → `include_bytes!`'d by `main.rs` and passed as
  `ViewportBuilder::with_icon`. eframe calls macOS `setApplicationIconImage:` a
  few frames after launch, and **falls back to the egui logo when the app supplies
  no icon** ([`epi_integration.rs`](https://docs.rs/eframe/0.29.1/src/eframe/native/epi_integration.rs.html)),
  which is why the Dock tile used to turn into a black "e" the moment the window
  appeared. Keep the two in sync — `make-icon.sh` writes both from one drawing.

If a rebuilt icon still doesn't refresh in Dock/Finder, that part *is* macOS icon
caching — `killall Dock` or move the `.app` to force it.

## How it works

- The model dropdown lists the `generate.sh` aliases (`flux2`, `flux2-4b`,
  `z-image-turbo`, …). Selecting one sets `MFLUX_MODEL` for the run; the first entry
  is preselected and matches the script's own default.
- **Size / Steps / Seed** map to `MFLUX_SIZE` / `MFLUX_STEPS` / `MFLUX_SEED`. Each
  has a checkbox — leave them unchecked for the script's defaults (1024², the
  model's own step count, a random seed); check to override (e.g. a fixed seed for
  reproducibility).
- **Generate** runs `bash generate.sh "<prompt>" [reference images…]` on a
  background thread, so the window stays responsive during the render (seconds per
  image for `flux2` / `z-image-turbo` once the weights are loaded, longer for the
  20 B models — and the first run of a session pays the load + quantize cost).
- Its stdout/stderr stream into a live **Log** panel as the render runs (including
  the step progress bar), so you can watch instead of guessing.
- On success the script prints `Saved: <path>`; the app parses that line and loads
  the PNG. Images are also written to `../generated/` as usual.

## Reference images

**Add…** (or drag files onto the window) attaches reference images; they show up as
thumbnails with a × to drop them again. The paths are passed to `generate.sh` as
extra arguments after the prompt, and **Use as** picks `MFLUX_MODE`:

- **edit** — the reference is *conditioning*: the prompt is an instruction ("put a
  red scarf on the fox"), and several references combine into one edit. Available
  on the FLUX.2 [klein] models, which need no other checkpoint for it, and on the
  Qwen ones, which first download `Qwen-Image-Edit-2509` (~58 GB) — the UI says so
  before you start.
- **starting point** — plain img2img (`--image-path`): the reference only seeds the
  denoise, the prompt still describes the whole picture, and **Strength**
  (`MFLUX_STRENGTH`) sets how much of it survives. Works with every model.

The radio disables itself where a model has no edit variant, so what you see is
what mflux can actually do. With a reference attached and **Size** unchecked, no
`--width/--height` reaches mflux — FLUX.2 and Z-Image Turbo then keep the
reference's own dimensions, the rest fall back to 1024².

For scripted runs (or a documentation screenshot), `AI_IMAGEGEN_IMAGE` preloads
reference images the same way `AI_IMAGEGEN_PROMPT` preloads the prompt —
colon-separated paths, like `PATH`.

## Notes / limits (it's a "simple start")

- **Backend is macOS / Apple Silicon** (MFLUX via `generate.sh`, a bash script).
  The *GUI* is cross-platform, but the generation backend is not — on Windows/Linux
  you'd point it at a different backend. The path to `generate.sh` is the parent of
  this crate; override with `AI_IMAGEGEN_ROOT=/path/to/ai-imagegen`.
- No quantization control and no gallery of past renders yet. The `generate.sh` env
  vars make these straightforward to add later.
- The file picker is native (`rfd`); everything else is egui.
