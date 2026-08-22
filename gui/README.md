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

## Layout

A split view, sized for a landscape screen: everything you *set* lives in the
sidebar on the left, and the whole right-hand side is the picture.

```
┌──────────────────────────────────────────────────────────────┐
│ File   Render   View                                         │  in-window menu bar
├────────────────────────────┬─────────────────────────────────┤
│ Model  ▾                   │                                 │
│ Reference image  [Add…]    │                                 │
│ Prompt                     │        the render, scaled       │
│ Overrides (size/steps/seed)│        to fit the pane          │
│                            │                                 │
│── Generate ⌘⏎ · Abort ⌘. ──│                                 │
│                            ├─────────────────────────────────┤
│ Gallery                    │ file · model · steps · seed · px│
│  ▣ ▣ ▣ ▣                   │ the prompt that produced it     │
│  ▣ ▣ ▣ ▣                   ├─────────────────────────────────┤
│  ▣ ▣ ▣ ▣                   │ Log (⌘L)                        │
├────────────────────────────┴─────────────────────────────────┤
│ Saved: generated/20260818_154801_flux2.png            [Log]  │
└──────────────────────────────────────────────────────────────┘
```

Every divider is draggable — sidebar width, gallery height, log height — and
egui remembers where you put them for the session. **⌘B** hides the sidebar
entirely when you just want to look at a render; **Fit** / **1:1** in the caption
row (**⌘0** / **⌘1**) switch between fit-to-pane and actual pixels, the latter
scrollable. Fit never enlarges past 1:1, so a small image stays crisp instead of
being blown up into mush.

The log sits inside the right-hand pane rather than spanning the window, so
opening it shortens the picture and leaves the sidebar alone. It opens itself
when a render starts and when one fails — the reason is in the output.

## Menus and shortcuts

egui draws no native macOS menu, so the app carries its own menu bar. Every item
is also a keyboard shortcut, and both routes run the same code:

| Menu | Action | Shortcut |
|---|---|---|
| **Render** | Generate | ⌘⏎ |
| | Abort the running render | ⌘. |
| | Focus the prompt | ⌘P |
| | Newer / older render | ⌘\[ / ⌘] |
| **File** | Add reference image… | ⌘O |
| | Clear references | ⌘⇧K |
| | Use the shown image as a reference | ⌘E |
| | Reveal in Finder | ⌘⇧R |
| | Open in default viewer | ⌘⇧O |
| | Copy image path | ⌘⇧C |
| | Refresh gallery | ⌘R |
| **View** | Show/hide sidebar | ⌘B |
| | Show/hide log | ⌘L |
| | Fit to window / actual pixels | ⌘0 / ⌘1 |

⌘ is Ctrl on Windows/Linux — the shortcuts are declared with egui's `COMMAND`
modifier and the menus print whichever the platform uses. The image actions work
on whatever the detail pane is showing; right-clicking a specific file gets the
same list for *that* file (see below).

Two things worth knowing about the implementation: shortcuts are consumed before
any widget is drawn, so ⌘⏎ never lands in the prompt box as a newline, and the
⇧ variants are matched first — `consume_shortcut` ignores *extra* Shift, so a ⌘R
test run earlier would swallow ⌘⇧R too.

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
- **Abort** calls a running render off — for the prompt that came out wrong, or
  the 20 B model that is going to take another twenty minutes to say so. The
  button sits next to *Generate* and, because ⌘B can hide the sidebar, in the menu
  bar next to the spinner; ⌘. and **Render ▸ Abort render** do the same thing.
  What makes it honest is *what* gets signalled: the whole process group, not just
  the `bash` wrapper. `generate.sh` runs `mflux-generate` as a child, and that
  child is the one holding the GPU — killing the script alone would hand the
  window back while the render churned on invisibly. So the run is spawned with
  `process_group(0)`: a group of its own means one `killpg` reaches script and
  model together, and that signal can't travel back up into the app. SIGTERM
  first; if anything is still alive five seconds later (a half-loaded checkpoint,
  a stalled download) it gets a SIGKILL it can't decline.
- Its stdout/stderr stream into a live **Log** panel as the render runs (including
  the step progress bar), so you can watch instead of guessing.
- On success the script prints `Saved: <path>`; the app parses that line and loads
  the PNG. Images are also written to `../generated/` as usual. The status bar
  shows that path relative to the checkout — the file name is the part that
  carries meaning, and a screenshot of the window then leaks no home directory.

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
colon-separated paths, like `PATH`. A prompt set that way also auto-runs on the
first frame, which is how `docs/assets/gui.jpg` is shot: symlink the checkout and
the model store under `/Users/Shared`, then launch through them so the paths in
the log carry no user name (`generate.sh` takes `pwd` logically, so the symlink
survives into `SCRIPT_DIR` and no weights are re-downloaded):

```bash
ln -sfn "$PWD" /Users/Shared/ai-imagegen
ln -sfn "$HOME/ai-models" /Users/Shared/ai-models
open -n --env AI_IMAGEGEN_ROOT=/Users/Shared/ai-imagegen \
        --env AI_MODELS_DIR=/Users/Shared/ai-models \
        --env AI_IMAGEGEN_POS=40,60 \
        --env AI_IMAGEGEN_IMAGE=/Users/Shared/ai-imagegen/generated/klein-style-photo.png \
        --env "AI_IMAGEGEN_PROMPT=put a knitted red scarf on the fox, keep the pose and background" \
        "dist/AI ImageGen.app"
```

The status bar needs no such care any more — it prints the repo-relative path.

## Gallery

Everything in `../generated/` fills the lower half of the sidebar as a wrapping
grid of tiles, newest first. Click one to open it in the detail pane — the tile
of whatever is shown keeps a highlight — and the caption under the picture
reports the model, steps, seed and size plus the prompt that produced it, so an
old render can be traced back without leaving the app. **Use as reference** feeds
it straight back in as a reference image, which is how you iterate on your own
output. **⌘\[** / **⌘]** walk to the newer/older render and scroll its tile into
view, which beats hunting through a grid of near-identical foxes.

**Right-click any tile** (or the big preview, or a reference thumbnail) for the
things this app has no business reimplementing:

- **Reveal in Finder** — `open -R`, i.e. the file selected in its folder.
- **Open in default viewer** — hand the PNG to Preview or whatever owns it.
- **Copy image path** — the absolute path, for a terminal or another app.
- **Use image as reference** — the same iterate-on-your-own-output loop.

The same four sit in the **File** menu, where they act on the image currently in
the detail pane. On Windows/Linux the first one becomes Explorer's
`/select,<path>` and `xdg-open <folder>` respectively (no portable "select this
file" exists), and the helper process is reaped on a throwaway thread rather
than left as a zombie.

Two details worth knowing:

- **Ordering is by mtime, not by file name.** `generate.sh` writes
  `<timestamp>_<model>.png`, but hand-named keepers (`klein-style-anime.png`)
  have no timestamp, and letters sort after digits — by name those would all
  masquerade as the newest.
- **The prompt is read out of the PNG, not the sidecar.** mflux writes its
  generation record twice: into `<name>.metadata.json` and into the image's EXIF.
  The sidecar is unreliable — mflux's FLUX.2 CLIs call `ImageUtil.save_image()`
  without passing `metadata=`, so `json.dump(None)` lands a literal `null` in
  every flux2 sidecar, while the embedded copy is complete. The gallery tries the
  sidecar, then falls back to the record inside the file, which works for every
  model.

Thumbnails are decoded on a worker thread (a screenful of 1024² PNGs is tens of
megabytes), so the window stays live while they fill in. The newest
`GALLERY_MAX` (200) get tiles; anything older is counted in the header rather
than silently dropped.

## Notes / limits (it's a "simple start")

- **Backend is macOS / Apple Silicon** (MFLUX via `generate.sh`, a bash script).
  The *GUI* is cross-platform, but the generation backend is not — on Windows/Linux
  you'd point it at a different backend. The path to `generate.sh` is the parent of
  this crate; override with `AI_IMAGEGEN_ROOT=/path/to/ai-imagegen`.
- No quantization control yet (`MFLUX_QUANT` from the script would cover it), and
  the gallery still can't delete or rename — reveal, open and copy-path hand that
  housekeeping to Finder instead.
- **Quitting the window mid-render does not stop the render.** Abort is the way
  out. The run gets a process group of its own precisely so signals don't cross
  between it and the app, and that cuts both ways: close the window and mflux is
  orphaned, still working, still holding the GPU.
- **An aborted run leaves no image.** mflux writes its PNG once, at the end, so
  there is nothing half-saved to clean up — but also nothing to show for the steps
  that did run.
- The file picker is native (`rfd`); everything else is egui.
