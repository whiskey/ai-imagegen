#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Minimal desktop front-end for the local image generator.
//!
//! It does not reimplement any model logic — it shells out to the repo's
//! `generate.sh` (setting `MFLUX_MODEL` and, optionally, `MFLUX_SIZE` /
//! `MFLUX_STEPS` / `MFLUX_SEED` / `MFLUX_MODE` / `MFLUX_STRENGTH`, and passing
//! reference images as extra arguments), which already maps aliases to the right
//! `mflux-generate-*` command, handles quantization, and writes a PNG. The GUI
//! collects a prompt + options, runs the script on a background thread while
//! streaming its output to a live log, and shows the resulting image.

use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use egui::ColorImage;

/// What a model can do with a reference image *beyond* plain img2img — i.e.
/// whether mflux has an `mflux-generate-*-edit` command for its family.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Edit {
    /// No edit command for this family: a reference can only seed the denoise.
    No,
    /// Edits with the weights the model already needs (FLUX.2 [klein]).
    Yes,
    /// Edits, but the edit tool first pulls a checkpoint of its own.
    Download(&'static str),
}

/// A `generate.sh` model alias (`MFLUX_MODEL=<alias>`), what it's good for, and
/// how it handles a reference image.
struct Model {
    alias: &'static str,
    desc: &'static str,
    edit: Edit,
}

/// First entry is the default (matches the script default).
const MODELS: &[Model] = &[
    Model {
        alias: "flux2",
        desc: "FLUX.2 klein 9B — strong prompt adherence + text",
        edit: Edit::Yes,
    },
    Model {
        alias: "flux2-4b",
        desc: "FLUX.2 klein 4B — lighter/faster klein",
        edit: Edit::Yes,
    },
    Model {
        alias: "z-image-turbo",
        desc: "Z-Image Turbo 6B — fastest (~18s)",
        edit: Edit::No,
    },
    Model {
        alias: "qwen-2512",
        desc: "Qwen-Image-2512 20B — best all-round quality",
        edit: Edit::Download("Qwen-Image-Edit-2509, ~58 GB"),
    },
    Model {
        alias: "z-image",
        desc: "Z-Image 6B base — higher quality, more steps",
        edit: Edit::No,
    },
    Model {
        alias: "qwen",
        desc: "Qwen-Image 20B — original",
        edit: Edit::Download("Qwen-Image-Edit-2509, ~58 GB"),
    },
    Model {
        alias: "flux-dev",
        desc: "FLUX.1 dev 12B — classic Flux",
        edit: Edit::No,
    },
    Model {
        alias: "flux-schnell",
        desc: "FLUX.1 schnell — 4-step drafts",
        edit: Edit::No,
    },
    Model {
        alias: "krea",
        desc: "FLUX.1 Krea dev — photographic",
        edit: Edit::No,
    },
];

/// How a reference image is handed to the model (`MFLUX_MODE`).
#[derive(PartialEq, Eq, Clone, Copy)]
enum RefMode {
    /// Reference as conditioning: the prompt is an instruction, and several
    /// references can be combined into one edit.
    Edit,
    /// Reference as the starting point of the denoise: the prompt still has to
    /// describe the whole picture.
    Img2Img,
}

impl RefMode {
    fn env(self) -> &'static str {
        match self {
            RefMode::Edit => "edit",
            RefMode::Img2Img => "img2img",
        }
    }
}

/// A chosen reference image: the path handed to `generate.sh`, plus a preview.
struct RefImage {
    path: PathBuf,
    /// None for formats the trimmed `image` crate can't decode — mflux (Pillow)
    /// still reads them, so we show the file name instead of a picture.
    thumb: Option<egui::TextureHandle>,
}

/// One past render in the gallery strip. `thumb` fills in asynchronously — the
/// loader thread decodes newest-first, so the tiles you can see resolve first.
struct Render {
    path: PathBuf,
    thumb: Option<egui::TextureHandle>,
}

/// A decoded gallery thumbnail on its way back from the loader thread.
struct ThumbMsg {
    path: PathBuf,
    image: ColorImage,
}

/// What the preview pane is showing, described from the file's own record.
struct Viewing {
    path: PathBuf,
    params: String,
    prompt: Option<String>,
}

const LOG_MAX_LINES: usize = 400;
/// Reference thumbnails: decoded at 2× so they stay sharp on a Retina display.
const THUMB: f32 = 76.0;
const THUMB_PX: u32 = 152;
/// Gallery tiles, likewise 2×.
const TILE: f32 = 72.0;
const TILE_PX: u32 = 144;
/// How many of the newest renders get a tile. The rest are counted in the
/// header rather than quietly dropped.
const GALLERY_MAX: usize = 200;

/// Where `generate.sh` lives. Baked in at build time (parent of this crate),
/// overridable at runtime with `AI_IMAGEGEN_ROOT` for a relocated checkout.
fn repo_root() -> PathBuf {
    if let Ok(p) = std::env::var("AI_IMAGEGEN_ROOT") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Streamed from the background generation thread to the UI.
enum GenMsg {
    Line(String),
    Done { path: PathBuf, image: ColorImage },
    Failed(String),
}

/// Everything the worker thread needs to run one generation.
struct GenParams {
    root: PathBuf,
    script: PathBuf,
    model: String,
    prompt: String,
    /// Reference images — extra positional args after the prompt.
    refs: Vec<PathBuf>,
    env: Vec<(String, String)>,
}

struct ImagenApp {
    root: PathBuf,
    prompt: String,
    model_idx: usize,
    refs: Vec<RefImage>,
    ref_mode: RefMode,
    strength: f32,
    override_size: bool,
    size: u32,
    override_steps: bool,
    steps: u32,
    fixed_seed: bool,
    seed: u32,
    status: String,
    generating: bool,
    rx: Option<Receiver<GenMsg>>,
    texture: Option<egui::TextureHandle>,
    /// Past renders in `generated/`, newest first.
    gallery: Vec<Render>,
    /// Renders beyond `GALLERY_MAX`, reported rather than silently dropped.
    gallery_extra: usize,
    thumb_rx: Option<Receiver<ThumbMsg>>,
    viewing: Option<Viewing>,
    log: Vec<String>,
    /// One-shot: if AI_IMAGEGEN_PROMPT was set, auto-run once on the first frame.
    autostart: bool,
    /// Gallery scan needs an egui Context, which `new` doesn't have handy.
    first_frame: bool,
}

impl ImagenApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let prompt = std::env::var("AI_IMAGEGEN_PROMPT").unwrap_or_default();
        let autostart = !prompt.trim().is_empty();
        // Same idea as AI_IMAGEGEN_PROMPT: preload reference images so a run (or a
        // documentation screenshot) can be driven entirely from the environment.
        let refs = std::env::var("AI_IMAGEGEN_IMAGE")
            .unwrap_or_default()
            .split(':')
            .filter(|p| !p.is_empty())
            .map(|p| RefImage {
                thumb: load_thumbnail(&cc.egui_ctx, Path::new(p)),
                path: PathBuf::from(p),
            })
            .collect();
        Self {
            root: repo_root(),
            prompt,
            model_idx: 0,
            refs,
            ref_mode: RefMode::Edit,
            strength: 0.4, // mflux's own --image-strength default
            override_size: false,
            size: 1024,
            override_steps: false,
            steps: 20,
            fixed_seed: false,
            seed: 0,
            status: "Ready.".to_owned(),
            generating: false,
            rx: None,
            texture: None,
            gallery: Vec::new(),
            gallery_extra: 0,
            thumb_rx: None,
            viewing: None,
            log: Vec::new(),
            autostart,
            first_frame: true,
        }
    }

    /// Rescan `generated/` and kick off thumbnailing for anything new.
    ///
    /// Textures already uploaded are kept, so the periodic refresh after a
    /// render only decodes the one file that appeared.
    fn refresh_gallery(&mut self, ctx: &egui::Context) {
        let dir = self.root.join("generated");
        // Sort on mtime, not name: `generate.sh` output leads with a
        // yyyymmdd_hhmmss stamp, but hand-named keepers (klein-style-anime.png)
        // don't, and letters sort after digits — by name those would all pile up
        // at the "newest" end. Name breaks ties, for renders within one second.
        let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
            .map(|path| {
                let mtime = path
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                (mtime, path)
            })
            .collect();
        found.sort_unstable();
        found.reverse();
        let mut paths: Vec<PathBuf> = found.into_iter().map(|(_, path)| path).collect();
        self.gallery_extra = paths.len().saturating_sub(GALLERY_MAX);
        paths.truncate(GALLERY_MAX);

        let mut known: HashMap<PathBuf, Option<egui::TextureHandle>> =
            self.gallery.drain(..).map(|r| (r.path, r.thumb)).collect();
        let mut todo = Vec::new();
        self.gallery = paths
            .into_iter()
            .map(|path| {
                let thumb = known.remove(&path).flatten();
                if thumb.is_none() {
                    todo.push(path.clone());
                }
                Render { path, thumb }
            })
            .collect();

        if todo.is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.thumb_rx = Some(rx);
        let ctx = ctx.clone();
        thread::spawn(move || {
            for path in todo {
                let Some(image) = thumbnail(&path, TILE_PX) else {
                    continue; // unreadable file: leave its tile as a spinner
                };
                if tx.send(ThumbMsg { path, image }).is_err() {
                    return; // a newer refresh replaced us
                }
                ctx.request_repaint();
            }
        });
    }

    /// Put a finished render in the preview pane, described by its own record.
    fn show_render(&mut self, ctx: &egui::Context, path: &Path) {
        match load_color_image(path) {
            Ok(image) => {
                self.texture =
                    Some(ctx.load_texture("preview", image, egui::TextureOptions::LINEAR));
                self.status = format!("Showing: {}", path.display());
                self.viewing = Some(Viewing::read(path));
            }
            Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
        }
    }

    fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > LOG_MAX_LINES {
            let cut = self.log.len() - LOG_MAX_LINES;
            self.log.drain(0..cut);
        }
    }

    /// The mode a run would really use: `edit` only where the model has an edit
    /// model. `reference_ui` normalises the radio to this, but a run can start
    /// before the first frame is drawn (autostart), so both paths ask here.
    fn effective_mode(&self) -> RefMode {
        match MODELS[self.model_idx].edit {
            Edit::No => RefMode::Img2Img,
            Edit::Yes | Edit::Download(_) => self.ref_mode,
        }
    }

    /// Take a file on as a reference image, ignoring duplicates.
    fn add_ref(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.refs.iter().any(|r| r.path == path) {
            return;
        }
        let thumb = load_thumbnail(ctx, &path);
        self.refs.push(RefImage { path, thumb });
    }

    fn start_generation(&mut self, ctx: &egui::Context) {
        let prompt = self.prompt.trim().to_owned();
        if prompt.is_empty() {
            self.status = "Enter a prompt first.".to_owned();
            return;
        }
        let script = self.root.join("generate.sh");
        if !script.exists() {
            self.status = format!("generate.sh not found at {}", script.display());
            return;
        }

        // Optional overrides -> the env vars generate.sh already reads. Left out,
        // each one keeps the script's own default (1024², model steps, random seed).
        let mut env = Vec::new();
        if self.override_size {
            env.push(("MFLUX_SIZE".to_owned(), self.size.to_string()));
        }
        if self.override_steps {
            env.push(("MFLUX_STEPS".to_owned(), self.steps.to_string()));
        }
        if self.fixed_seed {
            env.push(("MFLUX_SEED".to_owned(), self.seed.to_string()));
        }
        if !self.refs.is_empty() {
            let mode = self.effective_mode();
            env.push(("MFLUX_MODE".to_owned(), mode.env().to_owned()));
            if mode == RefMode::Img2Img {
                env.push(("MFLUX_STRENGTH".to_owned(), format!("{:.2}", self.strength)));
            }
        }

        let params = GenParams {
            root: self.root.clone(),
            script,
            model: MODELS[self.model_idx].alias.to_owned(),
            prompt,
            refs: self.refs.iter().map(|r| r.path.clone()).collect(),
            env,
        };

        let (tx, rx) = std::sync::mpsc::channel();
        self.rx = Some(rx);
        self.generating = true;
        self.log.clear();
        self.status = format!("Generating with '{}'…", params.model);

        let ctx = ctx.clone();
        thread::spawn(move || run_generate(params, tx, ctx));
    }

    /// Reference-image row: pick/drop files, see them, choose how they're used.
    ///
    /// Two different things hide behind "reference image", and the radio picks
    /// between them: an *edit* hands the image to the family's `*-edit` model as
    /// conditioning (prompt = instruction, several images can be combined), while
    /// a *starting point* is img2img — the image seeds the denoise and the prompt
    /// still describes the whole picture. Only some families have an edit model.
    fn reference_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let model = &MODELS[self.model_idx];
        self.ref_mode = self.effective_mode();

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Reference image");
            ui.add_enabled_ui(!self.generating, |ui| {
                if ui.button("Add…").clicked() {
                    if let Some(paths) = rfd::FileDialog::new()
                        .set_title("Reference image(s)")
                        .add_filter("images", IMAGE_EXTS)
                        .pick_files()
                    {
                        for path in paths {
                            self.add_ref(ctx, path);
                        }
                    }
                }
                if !self.refs.is_empty() && ui.button("Clear").clicked() {
                    self.refs.clear();
                }
            });
            if self.refs.is_empty() {
                ui.small("optional — or drop files onto the window");
            }
        });

        if self.refs.is_empty() {
            return;
        }

        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for (i, r) in self.refs.iter().enumerate() {
                ui.vertical(|ui| {
                    let preview = match &r.thumb {
                        Some(tex) => {
                            let px = tex.size_vec2();
                            ui.image(egui::load::SizedTexture::new(
                                tex.id(),
                                px * (THUMB / px.x.max(px.y)),
                            ))
                        }
                        None => ui.add_sized([THUMB, THUMB], egui::Label::new("(no preview)")),
                    };
                    preview.on_hover_text(r.path.display().to_string());
                    ui.horizontal(|ui| {
                        // "×", not "✕": egui's bundled fonts have no glyph for the
                        // latter and draw an empty box instead.
                        let x = egui::Button::new("×").small();
                        if ui
                            .add_enabled(!self.generating, x)
                            .on_hover_text("remove")
                            .clicked()
                        {
                            remove = Some(i);
                        }
                        ui.small(short_name(&r.path));
                    });
                });
            }
        });
        if let Some(i) = remove {
            self.refs.remove(i);
        }

        ui.add_enabled_ui(!self.generating, |ui| {
            ui.horizontal(|ui| {
                ui.label("Use as");
                ui.add_enabled_ui(model.edit != Edit::No, |ui| {
                    ui.radio_value(&mut self.ref_mode, RefMode::Edit, "edit")
                        .on_hover_text(
                            "Conditioning: the prompt is an instruction — \
                             \"put a red scarf on the fox\". Several references combine.",
                        );
                });
                ui.radio_value(&mut self.ref_mode, RefMode::Img2Img, "starting point")
                    .on_hover_text(
                        "img2img: the reference only seeds the denoise, \
                         so the prompt still describes the whole picture.",
                    );
                if self.ref_mode == RefMode::Img2Img {
                    ui.separator();
                    ui.label("Strength");
                    ui.add(egui::Slider::new(&mut self.strength, 0.0..=1.0).fixed_decimals(2))
                        .on_hover_text(
                            "How much of the reference survives: \
                             low follows the prompt, high stays close to the image.",
                        );
                }
            });
        });

        match (model.edit, self.ref_mode) {
            (Edit::No, _) => {
                ui.small(format!(
                    "{} has no edit model in mflux — starting point only.",
                    model.alias
                ));
            }
            (Edit::Download(what), RefMode::Edit) => {
                ui.small(format!("First edit run downloads {what}."));
            }
            _ => {}
        }
    }

    /// Strip of past renders from `generated/`. Click one to open it in the
    /// preview; from there it can be fed straight back in as a reference.
    fn gallery_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let header = match self.gallery_extra {
            0 => format!("Gallery ({})", self.gallery.len()),
            extra => format!("Gallery ({} newest, {extra} older)", self.gallery.len()),
        };
        egui::CollapsingHeader::new(header)
            .default_open(true)
            .show(ui, |ui| {
                if self.gallery.is_empty() {
                    ui.small("Nothing in generated/ yet.");
                    return;
                }
                let mut open = None;
                egui::ScrollArea::horizontal()
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for r in &self.gallery {
                                let Some(tex) = &r.thumb else {
                                    // Still being decoded by the loader thread.
                                    ui.add_sized([TILE, TILE], egui::Spinner::new());
                                    continue;
                                };
                                let px = tex.size_vec2();
                                let sized = egui::load::SizedTexture::new(
                                    tex.id(),
                                    px * (TILE / px.x.max(px.y)),
                                );
                                if ui
                                    .add(egui::ImageButton::new(sized))
                                    .on_hover_text(short_name(&r.path))
                                    .clicked()
                                {
                                    open = Some(r.path.clone());
                                }
                            }
                        });
                    });
                if let Some(path) = open {
                    self.show_render(ctx, &path);
                }
            });
    }

    /// Caption under the preview: what this image is, and what made it.
    fn viewing_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(viewing) = &self.viewing else {
            return;
        };
        let (params, prompt, path) = (
            viewing.params.clone(),
            viewing.prompt.clone(),
            viewing.path.clone(),
        );
        ui.horizontal(|ui| {
            ui.small(params);
            if ui
                .add_enabled(
                    !self.generating,
                    egui::Button::new("Use as reference").small(),
                )
                .on_hover_text("Edit this render further: adds it to the reference images")
                .clicked()
            {
                self.add_ref(ctx, path);
            }
        });
        match prompt {
            // Selectable so a prompt worth keeping can be copied back out.
            Some(prompt) => {
                ui.add(egui::Label::new(egui::RichText::new(prompt).small()).wrap());
            }
            None => {
                ui.small("No prompt recorded in this file.");
            }
        }
    }
}

/// Run `generate.sh`, streaming each output line back over `tx`, then report the
/// decoded PNG (or an error). Runs on a background thread.
fn run_generate(p: GenParams, tx: Sender<GenMsg>, ctx: egui::Context) {
    let mut cmd = Command::new("bash");
    cmd.arg(&p.script)
        .arg(&p.prompt)
        .args(&p.refs) // extra positional args = reference images
        .env("MFLUX_MODEL", &p.model)
        .current_dir(&p.root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &p.env {
        cmd.env(k, v);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(GenMsg::Failed(format!("failed to launch generate.sh: {e}")));
            return;
        }
    };

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    // The script prints `Saved: <path>` on stdout; capture it as we stream.
    let saved: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

    // Progress (tqdm) goes to stderr — pump it on its own thread.
    let err_thread = {
        let tx = tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || pump(stderr, &tx, &ctx, |_| {}))
    };

    // Pump stdout on this thread, watching for the Saved: line.
    {
        let saved = saved.clone();
        pump(stdout, &tx, &ctx, move |line| {
            if let Some(rest) = line.strip_prefix("Saved:") {
                *saved.lock().unwrap() = Some(PathBuf::from(rest.trim()));
            }
        });
    }

    let _ = err_thread.join();
    let status = child.wait();

    match status {
        Ok(s) if s.success() => {
            let path = saved.lock().unwrap().clone();
            match path {
                Some(path) => match load_color_image(&path) {
                    Ok(image) => {
                        let _ = tx.send(GenMsg::Done { path, image });
                    }
                    Err(e) => {
                        let _ = tx.send(GenMsg::Failed(format!(
                            "generated {} but failed to load it: {e}",
                            path.display()
                        )));
                    }
                },
                None => {
                    let _ = tx.send(GenMsg::Failed(
                        "generate.sh finished but printed no `Saved:` path".to_owned(),
                    ));
                }
            }
        }
        Ok(s) => {
            let _ = tx.send(GenMsg::Failed(format!("generate.sh exited with {s}")));
        }
        Err(e) => {
            let _ = tx.send(GenMsg::Failed(format!(
                "waiting on generate.sh failed: {e}"
            )));
        }
    }
    ctx.request_repaint();
}

/// Read a stream and emit one message per line, treating BOTH `\n` and `\r` as
/// boundaries so in-place tqdm progress bars stream through live. `on_line` runs
/// on each completed line before it's forwarded.
fn pump<R: Read>(r: R, tx: &Sender<GenMsg>, ctx: &egui::Context, mut on_line: impl FnMut(&str)) {
    let mut reader = BufReader::new(r);
    let mut buf: Vec<u8> = Vec::new();
    let mut byte = [0u8; 1];
    let mut flush = |buf: &mut Vec<u8>| {
        if buf.is_empty() {
            return;
        }
        let line = String::from_utf8_lossy(buf).into_owned();
        on_line(&line);
        let _ = tx.send(GenMsg::Line(line));
        ctx.request_repaint();
        buf.clear();
    };
    while let Ok(n) = reader.read(&mut byte) {
        if n == 0 {
            break;
        }
        match byte[0] {
            b'\n' | b'\r' => flush(&mut buf),
            b => buf.push(b),
        }
    }
    flush(&mut buf);
}

fn load_color_image(path: &Path) -> Result<ColorImage, String> {
    let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = img.into_raw();
    Ok(ColorImage::from_rgba_unmultiplied(size, &pixels))
}

/// Small preview texture for a reference image. `None` if we can't decode it —
/// that's cosmetic only, the path is still handed to the script.
fn load_thumbnail(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    let color = thumbnail(path, THUMB_PX)?;
    Some(ctx.load_texture(path.to_string_lossy(), color, egui::TextureOptions::LINEAR))
}

/// Decode `path` down to a `max`-pixel thumbnail. The gallery calls this from a
/// worker thread — a full render is megabytes, and decoding a screenful of them
/// on the UI thread would stall the window.
fn thumbnail(path: &Path, max: u32) -> Option<ColorImage> {
    let rgba = image::open(path).ok()?.thumbnail(max, max).to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// The generation record mflux stores for a render: prompt, seed, steps, size.
///
/// It writes that record twice — into a `<name>.metadata.json` sidecar and into
/// the PNG's own EXIF — but the sidecar is unreliable: the FLUX.2 CLIs call
/// `ImageUtil.save_image()` without passing `metadata=`, so every flux2 sidecar
/// is the literal `null` while the embedded copy is complete. Hence: try the
/// sidecar, then fall back to the copy inside the file. `serde_json`'s streaming
/// parser stops at the end of the first value, so no brace-matching is needed
/// (and prompts containing braces or quotes can't derail it).
fn render_metadata(path: &Path) -> Option<serde_json::Value> {
    let sidecar = path.with_extension("metadata.json");
    if let Ok(text) = std::fs::read_to_string(&sidecar) {
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) if v.is_object() => return Some(v),
            _ => {} // `null` from the flux2 path — fall through to the PNG
        }
    }
    let bytes = std::fs::read(path).ok()?;
    let needle = br#"{"mflux_version"#;
    let start = bytes.windows(needle.len()).position(|w| w == needle)?;
    serde_json::Deserializer::from_slice(&bytes[start..])
        .into_iter::<serde_json::Value>()
        .next()?
        .ok()
}

/// `20260818_130917_flux2.png` -> `flux2`; anything else keeps its stem.
fn model_from_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    match stem.splitn(3, '_').nth(2) {
        Some(model) if !model.is_empty() => model.to_owned(),
        _ => stem,
    }
}

impl Viewing {
    fn read(path: &Path) -> Self {
        let meta = render_metadata(path);
        let num = |key: &str| {
            meta.as_ref()
                .and_then(|m| m.get(key))
                .and_then(serde_json::Value::as_u64)
        };
        let mut params = vec![model_from_name(path)];
        if let Some(steps) = num("steps") {
            params.push(format!("{steps} steps"));
        }
        if let Some(seed) = num("seed") {
            params.push(format!("seed {seed}"));
        }
        if let (Some(w), Some(h)) = (num("width"), num("height")) {
            params.push(format!("{w}×{h}"));
        }
        Self {
            path: path.to_path_buf(),
            params: params.join(" · "),
            prompt: meta
                .as_ref()
                .and_then(|m| m.get("prompt"))
                .and_then(serde_json::Value::as_str)
                .filter(|p| !p.is_empty())
                .map(str::to_owned),
        }
    }
}

/// Extensions accepted from a drag-and-drop (the file picker filters its own).
/// Anything else is silently ignored rather than handed to mflux to choke on.
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp", "gif"];

fn looks_like_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// File name, shortened to keep the thumbnail column narrow.
fn short_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.chars().count() > 14 {
        format!("{}…", name.chars().take(13).collect::<String>())
    } else {
        name
    }
}

impl eframe::App for ImagenApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain everything the worker has sent since last frame.
        let mut new_render = false;
        if let Some(rx) = self.rx.take() {
            let mut finished = false;
            loop {
                match rx.try_recv() {
                    Ok(GenMsg::Line(line)) => self.push_log(line),
                    Ok(GenMsg::Done { path, image }) => {
                        self.texture =
                            Some(ctx.load_texture("preview", image, egui::TextureOptions::LINEAR));
                        self.status = format!("Saved: {}", path.display());
                        self.viewing = Some(Viewing::read(&path));
                        new_render = true;
                        finished = true;
                        break;
                    }
                    Ok(GenMsg::Failed(err)) => {
                        self.status = format!("Error: {err}");
                        finished = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }
            if finished {
                self.generating = false;
            } else {
                self.rx = Some(rx); // still running — keep listening
            }
        }

        // Thumbnails trickle in from the loader thread.
        if let Some(rx) = self.thumb_rx.take() {
            let mut alive = true;
            loop {
                match rx.try_recv() {
                    Ok(ThumbMsg { path, image }) => {
                        let tex = ctx.load_texture(
                            path.to_string_lossy(),
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        if let Some(item) = self.gallery.iter_mut().find(|r| r.path == path) {
                            item.thumb = Some(tex);
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        alive = false;
                        break;
                    }
                }
            }
            if alive {
                self.thumb_rx = Some(rx);
            }
        }

        // First frame builds the gallery; later, a finished render adds to it.
        if self.first_frame || new_render {
            self.first_frame = false;
            self.refresh_gallery(ctx);
        }

        // One-shot auto-run (AI_IMAGEGEN_PROMPT) fires on the first frame.
        if self.autostart && !self.generating {
            self.autostart = false;
            self.start_generation(ctx);
        }

        // Files dropped anywhere on the window become reference images.
        if !self.generating {
            let dropped: Vec<PathBuf> = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .filter(|p| looks_like_image(p))
                    .collect()
            });
            for path in dropped {
                self.add_ref(ctx, path);
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Local Image Generation");
            ui.add_space(6.0);

            egui::ComboBox::from_label("Model")
                .selected_text(MODELS[self.model_idx].alias)
                .show_ui(ui, |ui| {
                    for (i, m) in MODELS.iter().enumerate() {
                        let label = format!("{} — {}", m.alias, m.desc);
                        ui.selectable_value(&mut self.model_idx, i, label);
                    }
                });

            self.reference_ui(ui, ctx);

            // What each mode wants is genuinely different, and getting it wrong
            // fails quietly: an instruction like "make a happy version of this"
            // in img2img names no subject, so the model invents one from scratch.
            // Hence a stated expectation next to the label, not just an example.
            ui.add_space(8.0);
            let (expects, hint) = match (self.refs.is_empty(), self.ref_mode) {
                (true, _) => (
                    "describe the whole image — subject, setting, style, light",
                    "a vast cyberpunk cityscape at sunset, neon reflections…",
                ),
                (false, RefMode::Edit) => (
                    "an instruction: what to change, and what to keep",
                    "put a knitted red scarf on the fox, keep the pose and background",
                ),
                (false, RefMode::Img2Img) => (
                    "describe the whole image again — the reference only seeds it",
                    "a red fox in a snowy clearing, anime cel illustration…",
                ),
            };
            // Wrapped, so a narrowed window pushes the hint to the next line
            // instead of clipping it.
            ui.horizontal_wrapped(|ui| {
                ui.label("Prompt");
                ui.small(format!("— {expects}"));
            });
            ui.add_enabled(
                !self.generating,
                egui::TextEdit::multiline(&mut self.prompt)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint),
            );

            ui.add_space(8.0);
            ui.add_enabled_ui(!self.generating, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.override_size, "Size");
                    ui.add_enabled(
                        self.override_size,
                        egui::DragValue::new(&mut self.size)
                            .range(256..=2048)
                            .speed(8),
                    );
                    ui.label("px");
                    ui.separator();
                    ui.checkbox(&mut self.override_steps, "Steps");
                    ui.add_enabled(
                        self.override_steps,
                        egui::DragValue::new(&mut self.steps).range(1..=100),
                    );
                    ui.separator();
                    ui.checkbox(&mut self.fixed_seed, "Seed");
                    ui.add_enabled(
                        self.fixed_seed,
                        egui::DragValue::new(&mut self.seed)
                            .range(0..=u32::MAX)
                            .speed(1.0),
                    );
                });
            });
            // Unchecked Size sends no MFLUX_SIZE: the script then derives the size
            // from the reference (capped at MFLUX_MAX_MP), and without one FLUX.2 /
            // Z-Image Turbo match it while the other tools use their 1024² default.
            let unchecked = if self.refs.is_empty() {
                "Unchecked = 1024², model default steps, random seed."
            } else {
                "Unchecked = the reference's size (capped at 1.3 MP) where the model supports it, model default steps, random seed."
            };
            ui.small(unchecked);

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ready = !self.generating && !self.prompt.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("Generate"))
                    .clicked()
                {
                    self.start_generation(ctx);
                }
                if self.generating {
                    ui.spinner();
                    ui.label("working…");
                }
            });

            ui.add_space(6.0);
            ui.separator();
            ui.label(&self.status);

            if !self.log.is_empty() {
                egui::CollapsingHeader::new("Log")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .auto_shrink([false, false])
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for line in &self.log {
                                    ui.label(egui::RichText::new(line).monospace().size(11.0));
                                }
                            });
                    });
            }

            ui.add_space(4.0);
            self.gallery_ui(ui, ctx);

            if self.texture.is_some() {
                ui.add_space(4.0);
                self.viewing_ui(ui, ctx);
            }
            if let Some(tex) = &self.texture {
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let avail = ui.available_width();
                        let size = tex.size_vec2();
                        let scale = (avail / size.x).min(1.0);
                        ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
                    });
            }
        });

        // While a job runs, keep polling the channel even without user input.
        if self.generating {
            ctx.request_repaint_after(Duration::from_millis(120));
        }
    }
}

/// The `.app` bundle icon only covers Finder. A few frames after launch eframe
/// calls macOS `setApplicationIconImage:` with whatever icon the app supplied —
/// and falls back to the *egui logo* when that is None, which is what replaced
/// the Dock tile mid-launch. So hand it the same artwork the bundle uses.
fn app_icon() -> egui::IconData {
    let png = include_bytes!("../icon/icon_1024.png");
    let rgba = image::load_from_memory(png)
        .expect("embedded app icon is a valid PNG")
        .into_rgba8();
    let (width, height) = rgba.dimensions();
    egui::IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([760.0, 960.0])
        .with_icon(app_icon());
    // Optional deterministic placement: AI_IMAGEGEN_POS="x,y" (screen points).
    if let Ok(pos) = std::env::var("AI_IMAGEGEN_POS") {
        if let Some((x, y)) = pos.split_once(',') {
            if let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>()) {
                viewport = viewport.with_position([x, y]);
            }
        }
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "ai-imagegen",
        options,
        Box::new(|cc| Ok(Box::new(ImagenApp::new(cc)))),
    )
}
