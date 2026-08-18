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
//!
//! The window is a split view, sized for a landscape screen: a resizable
//! sidebar on the left holds everything you *set* (model, references, prompt,
//! options) plus the gallery of past renders, and the whole right-hand side is
//! the picture — the detail pane. Menu bar, context menus and keyboard
//! shortcuts all funnel into the same [`Act`] list, so an action behaves
//! identically however it was triggered.

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

/// One past render in the gallery. `thumb` fills in asynchronously — the loader
/// thread decodes newest-first, so the tiles you can see resolve first.
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

// ---------------------------------------------------------------------------
// Keyboard shortcuts
// ---------------------------------------------------------------------------

/// ⌘ on macOS, Ctrl elsewhere — egui maps `COMMAND` per platform, and
/// `Context::format_shortcut` prints it with the right glyphs in the menus.
const CMD: egui::Modifiers = egui::Modifiers::COMMAND;
const CMD_SHIFT: egui::Modifiers = CMD.plus(egui::Modifiers::SHIFT);

const SC_GENERATE: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::Enter);
const SC_ADD_REF: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::O);
const SC_CLEAR_REFS: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD_SHIFT, egui::Key::K);
const SC_USE_REF: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::E);
const SC_REVEAL: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD_SHIFT, egui::Key::R);
const SC_OPEN: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD_SHIFT, egui::Key::O);
const SC_COPY_PATH: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD_SHIFT, egui::Key::C);
const SC_REFRESH: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::R);
const SC_NEWER: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::OpenBracket);
const SC_OLDER: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::CloseBracket);
const SC_SIDEBAR: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::B);
const SC_LOG: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::L);
const SC_FIT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::Num0);
const SC_ACTUAL: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::Num1);
const SC_PROMPT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(CMD, egui::Key::P);

/// What the platform calls "show this file in the file manager".
#[cfg(target_os = "macos")]
const REVEAL_LABEL: &str = "Reveal in Finder";
#[cfg(target_os = "windows")]
const REVEAL_LABEL: &str = "Show in Explorer";
#[cfg(all(unix, not(target_os = "macos")))]
const REVEAL_LABEL: &str = "Show in file manager";

/// Something the user asked for, from a button, a menu item, a context menu or
/// a keyboard shortcut. They're collected while the frame is drawn and applied
/// afterwards: menus and shortcuts then run exactly the same code, and nothing
/// has to mutate the app while the widget that triggered it is still borrowed.
enum Act {
    Generate,
    AddRefs,
    ClearRefs,
    UseAsRef(PathBuf),
    /// Load into the detail pane.
    Show(PathBuf),
    Reveal(PathBuf),
    OpenExternally(PathBuf),
    Copy {
        text: String,
        what: &'static str,
    },
    /// Step through the gallery: -1 = newer, +1 = older.
    StepGallery(isize),
    RefreshGallery,
    FocusPrompt,
    ToggleSidebar,
    ToggleLog,
    ClearLog,
    /// true = fit to the pane, false = actual pixels.
    SetFit(bool),
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
    /// Panel visibility and preview zoom — driven by the View menu.
    show_sidebar: bool,
    show_log: bool,
    fit: bool,
    /// Set by ⌘P; consumed by the prompt box on the next frame.
    focus_prompt: bool,
    /// Keyboard navigation moved the selection — drag its tile into view.
    scroll_to_selected: bool,
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
            show_sidebar: true,
            show_log: false,
            fit: true,
            focus_prompt: false,
            scroll_to_selected: false,
            autostart,
            first_frame: true,
        }
    }

    /// The image the "current image" actions (reveal, open, copy path, use as
    /// reference) apply to: whatever the detail pane is showing.
    fn current(&self) -> Option<PathBuf> {
        self.viewing.as_ref().map(|v| v.path.clone())
    }

    /// A path as the status bar shows it: relative to the checkout, because
    /// `generated/20260818_154801_flux2.png` is the part that carries meaning
    /// (and the absolute one is a home directory nobody asked to read). Copy
    /// path and the clipboard still hand out the real thing.
    fn pretty(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
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
                self.status = format!("Showing: {}", self.pretty(path));
                self.viewing = Some(Viewing::read(path));
                self.scroll_to_selected = true;
            }
            Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
        }
    }

    /// Move the selection through the gallery (newest first, so -1 is newer).
    fn step_gallery(&mut self, ctx: &egui::Context, delta: isize) {
        if self.gallery.is_empty() {
            return;
        }
        let last = self.gallery.len() as isize - 1;
        let at = self
            .viewing
            .as_ref()
            .and_then(|v| self.gallery.iter().position(|r| r.path == v.path));
        let next = match at {
            Some(i) => (i as isize + delta).clamp(0, last),
            None => 0,
        } as usize;
        let path = self.gallery[next].path.clone();
        self.show_render(ctx, &path);
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

    /// Native file picker for reference images. Blocks the UI thread while it's
    /// open, which is what a modal panel does anyway.
    fn pick_refs(&mut self, ctx: &egui::Context) {
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

    /// Carry out one collected [`Act`], now that no widget is borrowing us.
    fn act(&mut self, ctx: &egui::Context, act: Act) {
        match act {
            Act::Generate => self.start_generation(ctx),
            Act::AddRefs => {
                if !self.generating {
                    self.pick_refs(ctx);
                }
            }
            Act::ClearRefs => {
                if !self.generating {
                    self.refs.clear();
                }
            }
            Act::UseAsRef(path) => {
                if !self.generating {
                    self.add_ref(ctx, path);
                }
            }
            Act::Show(path) => self.show_render(ctx, &path),
            Act::Reveal(path) => match reveal_in_file_manager(&path) {
                Ok(()) => self.status = format!("Revealed {}", short_name(&path)),
                Err(e) => self.status = format!("Could not reveal {}: {e}", path.display()),
            },
            Act::OpenExternally(path) => match open_externally(&path) {
                Ok(()) => self.status = format!("Opened {}", short_name(&path)),
                Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
            },
            Act::Copy { text, what } => {
                ctx.output_mut(|o| o.copied_text = text);
                self.status = format!("{what} copied to the clipboard.");
            }
            Act::StepGallery(delta) => self.step_gallery(ctx, delta),
            Act::RefreshGallery => self.refresh_gallery(ctx),
            Act::FocusPrompt => {
                self.show_sidebar = true;
                self.focus_prompt = true;
            }
            Act::ToggleSidebar => self.show_sidebar = !self.show_sidebar,
            Act::ToggleLog => self.show_log = !self.show_log,
            Act::ClearLog => self.log.clear(),
            Act::SetFit(fit) => self.fit = fit,
        }
    }

    fn start_generation(&mut self, ctx: &egui::Context) {
        if self.generating {
            return;
        }
        let prompt = self.prompt.trim().to_owned();
        if prompt.is_empty() {
            self.status = "Enter a prompt first.".to_owned();
            self.show_sidebar = true;
            self.focus_prompt = true;
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
        self.show_log = true; // the render is the interesting part — watch it
        self.status = format!("Generating with '{}'…", params.model);

        let ctx = ctx.clone();
        thread::spawn(move || run_generate(params, tx, ctx));
    }

    // -----------------------------------------------------------------------
    // Panels
    // -----------------------------------------------------------------------

    /// Anything the user typed that matches a shortcut, turned into an [`Act`].
    ///
    /// Consuming happens before any widget is drawn, so ⌘⏎ never reaches the
    /// prompt box as a newline.
    fn shortcuts(&self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let hit = |sc: &egui::KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(sc));
        // Shift variants first. `consume_shortcut` ignores *extra* Shift/Alt, so
        // a ⌘R test run earlier would happily swallow ⌘⇧R as well.
        let (reveal, open, copy) = (hit(&SC_REVEAL), hit(&SC_OPEN), hit(&SC_COPY_PATH));
        if hit(&SC_CLEAR_REFS) {
            acts.push(Act::ClearRefs);
        }
        if hit(&SC_GENERATE) {
            acts.push(Act::Generate);
        }
        if hit(&SC_ADD_REF) {
            acts.push(Act::AddRefs);
        }
        if hit(&SC_REFRESH) {
            acts.push(Act::RefreshGallery);
        }
        if hit(&SC_NEWER) {
            acts.push(Act::StepGallery(-1));
        }
        if hit(&SC_OLDER) {
            acts.push(Act::StepGallery(1));
        }
        if hit(&SC_SIDEBAR) {
            acts.push(Act::ToggleSidebar);
        }
        if hit(&SC_LOG) {
            acts.push(Act::ToggleLog);
        }
        if hit(&SC_FIT) {
            acts.push(Act::SetFit(true));
        }
        if hit(&SC_ACTUAL) {
            acts.push(Act::SetFit(false));
        }
        if hit(&SC_PROMPT) {
            acts.push(Act::FocusPrompt);
        }
        // The rest act on the image in the detail pane; the keys are swallowed
        // either way, so a shortcut with nothing to act on can't leak into a
        // text box as a stray character.
        let use_ref = hit(&SC_USE_REF);
        if let Some(path) = self.current() {
            if reveal {
                acts.push(Act::Reveal(path.clone()));
            }
            if open {
                acts.push(Act::OpenExternally(path.clone()));
            }
            if copy {
                acts.push(Act::Copy {
                    text: path.display().to_string(),
                    what: "Path",
                });
            }
            if use_ref {
                acts.push(Act::UseAsRef(path));
            }
        }
    }

    /// In-window menu bar. egui draws no native macOS menu, so the app carries
    /// its own — which is also where the shortcuts advertise themselves.
    fn menu_bar_ui(&self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                let has_image = self.current().is_some();
                ui.menu_button("File", |ui| {
                    if menu_item(ui, "Add reference image…", &SC_ADD_REF, !self.generating) {
                        acts.push(Act::AddRefs);
                    }
                    let clearable = !self.generating && !self.refs.is_empty();
                    if menu_item(ui, "Clear references", &SC_CLEAR_REFS, clearable) {
                        acts.push(Act::ClearRefs);
                    }
                    ui.separator();
                    if let Some(path) = self.current() {
                        image_menu_items(ui, &path, self.generating, true, acts);
                    } else {
                        // Same rows, greyed out, so the menu doesn't change shape.
                        menu_item(ui, REVEAL_LABEL, &SC_REVEAL, false);
                        menu_item(ui, "Open in default viewer", &SC_OPEN, false);
                        menu_item(ui, "Copy image path", &SC_COPY_PATH, false);
                        ui.separator();
                        menu_item(ui, "Use image as reference", &SC_USE_REF, false);
                    }
                    ui.separator();
                    if menu_item(ui, "Refresh gallery", &SC_REFRESH, true) {
                        acts.push(Act::RefreshGallery);
                    }
                });
                ui.menu_button("Render", |ui| {
                    let ready = !self.generating && !self.prompt.trim().is_empty();
                    if menu_item(ui, "Generate", &SC_GENERATE, ready) {
                        acts.push(Act::Generate);
                    }
                    if menu_item(ui, "Focus prompt", &SC_PROMPT, true) {
                        acts.push(Act::FocusPrompt);
                    }
                    ui.separator();
                    let many = self.gallery.len() > 1;
                    if menu_item(ui, "Newer render", &SC_NEWER, many && has_image) {
                        acts.push(Act::StepGallery(-1));
                    }
                    if menu_item(ui, "Older render", &SC_OLDER, many) {
                        acts.push(Act::StepGallery(1));
                    }
                });
                ui.menu_button("View", |ui| {
                    let sidebar = if self.show_sidebar {
                        "Hide sidebar"
                    } else {
                        "Show sidebar"
                    };
                    if menu_item(ui, sidebar, &SC_SIDEBAR, true) {
                        acts.push(Act::ToggleSidebar);
                    }
                    let log = if self.show_log {
                        "Hide log"
                    } else {
                        "Show log"
                    };
                    if menu_item(ui, log, &SC_LOG, true) {
                        acts.push(Act::ToggleLog);
                    }
                    ui.separator();
                    if check_item(ui, "Fit to window", &SC_FIT, self.fit) {
                        acts.push(Act::SetFit(true));
                    }
                    if check_item(ui, "Actual pixels", &SC_ACTUAL, !self.fit) {
                        acts.push(Act::SetFit(false));
                    }
                });
                // Progress belongs where the eye already is during a render.
                if self.generating {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spinner();
                        ui.small("rendering…");
                    });
                }
            });
        });
    }

    /// One-line status strip along the bottom of the window.
    fn status_bar_ui(&self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = match self.log.len() {
                        0 => "Log".to_owned(),
                        n => format!("Log ({n})"),
                    };
                    if ui
                        .selectable_label(self.show_log, label)
                        .on_hover_text(format!(
                            "Script output — {}",
                            ui.ctx().format_shortcut(&SC_LOG)
                        ))
                        .clicked()
                    {
                        acts.push(Act::ToggleLog);
                    }
                    ui.separator();
                    // Truncate rather than wrap: a saved path can be long, and a
                    // status bar that changes height jitters the whole layout.
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(self.status.as_str()).truncate());
                    });
                });
            });
        });
    }

    /// Live `generate.sh` output, along the bottom of the detail pane.
    ///
    /// It lives *inside* the right-hand side on purpose: a full-width log would
    /// shorten the sidebar too, and the render it reports on is over here.
    fn log_ui(&self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(170.0)
            .show_animated_inside(ui, self.show_log, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Log");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Hide").clicked() {
                            acts.push(Act::ToggleLog);
                        }
                        if ui.small_button("Clear").clicked() {
                            acts.push(Act::ClearLog);
                        }
                        if ui.small_button("Copy").clicked() {
                            acts.push(Act::Copy {
                                text: self.log.join("\n"),
                                what: "Log",
                            });
                        }
                    });
                });
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.log.is_empty() {
                            ui.small("Nothing yet — the script's output appears here as it runs.");
                        }
                        for line in &self.log {
                            ui.label(egui::RichText::new(line).monospace().size(11.0));
                        }
                    });
            });
    }

    /// The left half of the split: what you set, and what you made.
    fn sidebar_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(400.0)
            .width_range(320.0..=680.0)
            .show_animated(ctx, self.show_sidebar, |ui| {
                // Bottom-up: the gallery owns the lower part (drag to retrade the
                // space), the Generate row is pinned just above it so it stays
                // reachable however far the form is scrolled. Reserve enough for
                // the form with one reference thumbnail on it (~500 pt) and hand
                // the gallery the rest — a starting height only, egui remembers
                // wherever the divider is dragged to afterwards.
                let tall = (ui.available_height() - 500.0).clamp(180.0, 460.0);
                egui::TopBottomPanel::bottom("gallery")
                    .resizable(true)
                    .default_height(tall)
                    .show_inside(ui, |ui| self.gallery_ui(ui, acts));
                egui::TopBottomPanel::bottom("actions").show_inside(ui, |ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let ready = !self.generating && !self.prompt.trim().is_empty();
                        let go = egui::Button::new("Generate")
                            .shortcut_text(ui.ctx().format_shortcut(&SC_GENERATE))
                            .min_size(egui::vec2(0.0, 26.0));
                        if ui.add_enabled(ready, go).clicked() {
                            acts.push(Act::Generate);
                        }
                        if self.generating {
                            ui.spinner();
                            ui.label("working…");
                        }
                    });
                    ui.add_space(4.0);
                });
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.compose_ui(ui, acts));
                });
            });
    }

    /// Model, references, prompt, overrides — the scrolling part of the sidebar.
    fn compose_ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        ui.add_space(2.0);
        ui.strong("Model");
        egui::ComboBox::from_id_salt("model")
            .width(ui.available_width())
            .selected_text(MODELS[self.model_idx].alias)
            .show_ui(ui, |ui| {
                for (i, m) in MODELS.iter().enumerate() {
                    let label = format!("{} — {}", m.alias, m.desc);
                    ui.selectable_value(&mut self.model_idx, i, label);
                }
            });
        ui.small(MODELS[self.model_idx].desc);

        self.reference_ui(ui, acts);

        // What each mode wants is genuinely different, and getting it wrong
        // fails quietly: an instruction like "make a happy version of this"
        // in img2img names no subject, so the model invents one from scratch.
        // Hence a stated expectation next to the label, not just an example.
        ui.add_space(10.0);
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
        ui.strong("Prompt");
        ui.small(expects);
        let prompt = ui.add_enabled(
            !self.generating,
            egui::TextEdit::multiline(&mut self.prompt)
                .desired_rows(5)
                .desired_width(f32::INFINITY)
                .hint_text(hint),
        );
        if self.focus_prompt {
            self.focus_prompt = false;
            prompt.request_focus();
        }

        ui.add_space(10.0);
        ui.strong("Overrides");
        ui.add_enabled_ui(!self.generating, |ui| {
            egui::Grid::new("overrides")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.checkbox(&mut self.override_size, "Size");
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            self.override_size,
                            egui::DragValue::new(&mut self.size)
                                .range(256..=2048)
                                .speed(8),
                        );
                        ui.label("px");
                    });
                    ui.end_row();

                    ui.checkbox(&mut self.override_steps, "Steps");
                    ui.add_enabled(
                        self.override_steps,
                        egui::DragValue::new(&mut self.steps).range(1..=100),
                    );
                    ui.end_row();

                    ui.checkbox(&mut self.fixed_seed, "Seed");
                    ui.add_enabled(
                        self.fixed_seed,
                        egui::DragValue::new(&mut self.seed)
                            .range(0..=u32::MAX)
                            .speed(1.0),
                    );
                    ui.end_row();
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
        ui.add_space(6.0);
    }

    /// Reference-image block: pick/drop files, see them, choose how they're used.
    ///
    /// Two different things hide behind "reference image", and the radio picks
    /// between them: an *edit* hands the image to the family's `*-edit` model as
    /// conditioning (prompt = instruction, several images can be combined), while
    /// a *starting point* is img2img — the image seeds the denoise and the prompt
    /// still describes the whole picture. Only some families have an edit model.
    fn reference_ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let model = &MODELS[self.model_idx];
        self.ref_mode = self.effective_mode();

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.strong("Reference image");
            ui.add_enabled_ui(!self.generating, |ui| {
                if ui
                    .small_button("Add…")
                    .on_hover_text(ui.ctx().format_shortcut(&SC_ADD_REF))
                    .clicked()
                {
                    acts.push(Act::AddRefs);
                }
                if !self.refs.is_empty() && ui.small_button("Clear").clicked() {
                    acts.push(Act::ClearRefs);
                }
            });
        });

        if self.refs.is_empty() {
            ui.small("optional — or drop files onto the window");
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
                    preview
                        .interact(egui::Sense::click())
                        .on_hover_text(r.path.display().to_string())
                        .context_menu(|ui| {
                            image_menu_items(ui, &r.path, self.generating, false, acts);
                        });
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
            ui.horizontal_wrapped(|ui| {
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
            });
            if self.ref_mode == RefMode::Img2Img {
                ui.horizontal(|ui| {
                    ui.label("Strength");
                    ui.add(egui::Slider::new(&mut self.strength, 0.0..=1.0).fixed_decimals(2))
                        .on_hover_text(
                            "How much of the reference survives: \
                             low follows the prompt, high stays close to the image.",
                        );
                });
            }
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

    /// Past renders from `generated/`, as a wrapping grid of tiles. Click one to
    /// open it in the detail pane; right-click for Finder and friends.
    fn gallery_ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.strong("Gallery");
            ui.small(match self.gallery_extra {
                0 => format!("{}", self.gallery.len()),
                extra => format!("{} newest, {extra} older", self.gallery.len()),
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("Refresh")
                    .on_hover_text(format!(
                        "Rescan generated/ — {}",
                        ui.ctx().format_shortcut(&SC_REFRESH)
                    ))
                    .clicked()
                {
                    acts.push(Act::RefreshGallery);
                }
            });
        });
        if self.gallery.is_empty() {
            ui.small("Nothing in generated/ yet.");
            return;
        }

        let current = self.viewing.as_ref().map(|v| v.path.clone());
        let scroll_to = self.scroll_to_selected;
        let generating = self.generating;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for r in &self.gallery {
                        let Some(tex) = &r.thumb else {
                            // Still being decoded by the loader thread.
                            ui.add_sized([TILE, TILE], egui::Spinner::new());
                            continue;
                        };
                        let px = tex.size_vec2();
                        let sized =
                            egui::load::SizedTexture::new(tex.id(), px * (TILE / px.x.max(px.y)));
                        let selected = current.as_deref() == Some(r.path.as_path());
                        let tile = ui
                            .add(egui::ImageButton::new(sized).selected(selected))
                            .on_hover_text(format!(
                                "{}\nright-click for Finder, copy path, …",
                                file_name(&r.path)
                            ));
                        if tile.clicked() {
                            acts.push(Act::Show(r.path.clone()));
                        }
                        tile.context_menu(|ui| {
                            if ui.button("Show in preview").clicked() {
                                acts.push(Act::Show(r.path.clone()));
                                ui.close_menu();
                            }
                            ui.separator();
                            image_menu_items(ui, &r.path, generating, false, acts);
                        });
                        if selected && scroll_to {
                            tile.scroll_to_me(Some(egui::Align::Center));
                        }
                    }
                });
            });
        self.scroll_to_selected = false;
    }

    /// The right half of the split: the picture, as big as the window allows.
    fn detail_ui(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.log_ui(ui, acts);
            if self.viewing.is_some() {
                egui::TopBottomPanel::bottom("caption")
                    .show_inside(ui, |ui| self.caption_ui(ui, acts));
            }
            let Some(tex) = self.texture.clone() else {
                ui.centered_and_justified(|ui| {
                    let go = ui.ctx().format_shortcut(&SC_GENERATE);
                    ui.label(if self.generating {
                        "Rendering…".to_owned()
                    } else {
                        format!(
                            "Write a prompt and press {go} — or pick a render from the gallery."
                        )
                    });
                });
                return;
            };

            let size = tex.size_vec2();
            let menu_path = self.current();
            let generating = self.generating;
            if self.fit {
                // Never blow a render up past 1:1 — a stretched 1024² is mush.
                let avail = ui.available_size();
                let scale = (avail.x / size.x).min(avail.y / size.y).min(1.0);
                let shown = size * scale;
                let (rect, resp) = ui.allocate_exact_size(avail, egui::Sense::click());
                egui::Image::new(egui::load::SizedTexture::new(tex.id(), shown))
                    .paint_at(ui, egui::Rect::from_center_size(rect.center(), shown));
                if let Some(path) = menu_path {
                    resp.context_menu(|ui| image_menu_items(ui, &path, generating, true, acts));
                }
            } else {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Actual pixels: a point is `pixels_per_point` device
                        // pixels, so divide to land one texel on one pixel.
                        let shown = size / ui.ctx().pixels_per_point();
                        let resp = ui
                            .image(egui::load::SizedTexture::new(tex.id(), shown))
                            .interact(egui::Sense::click());
                        if let Some(path) = menu_path {
                            resp.context_menu(|ui| {
                                image_menu_items(ui, &path, generating, true, acts)
                            });
                        }
                    });
            }
        });
    }

    /// Caption under the preview: what this image is, and what made it.
    fn caption_ui(&self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let Some(viewing) = &self.viewing else {
            return;
        };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(!self.fit, "1:1").clicked() {
                    acts.push(Act::SetFit(false));
                }
                if ui.selectable_label(self.fit, "Fit").clicked() {
                    acts.push(Act::SetFit(true));
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.generating,
                        egui::Button::new("Use as reference").small(),
                    )
                    .on_hover_text(format!(
                        "Edit this render further — {}",
                        ui.ctx().format_shortcut(&SC_USE_REF)
                    ))
                    .clicked()
                {
                    acts.push(Act::UseAsRef(viewing.path.clone()));
                }
                if ui
                    .add(egui::Button::new(REVEAL_LABEL).small())
                    .on_hover_text(ui.ctx().format_shortcut(&SC_REVEAL))
                    .clicked()
                {
                    acts.push(Act::Reveal(viewing.path.clone()));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(file_name(&viewing.path)).truncate());
                    ui.small(viewing.params.as_str());
                });
            });
        });
        // The prompt can be a paragraph; give it its own scroll so it never
        // pushes the picture out of the window.
        egui::ScrollArea::vertical()
            .max_height(64.0)
            .auto_shrink([false, true])
            .show(ui, |ui| match &viewing.prompt {
                // Selectable so a prompt worth keeping can be copied back out.
                Some(prompt) => {
                    ui.add(egui::Label::new(egui::RichText::new(prompt).small()).wrap());
                }
                None => {
                    ui.small("No prompt recorded in this file.");
                }
            });
        ui.add_space(4.0);
    }
}

// ---------------------------------------------------------------------------
// Menus
// ---------------------------------------------------------------------------

/// One menu row: label left, its shortcut greyed out on the right. Returns true
/// (and closes the menu) when it was clicked.
fn menu_item(ui: &mut egui::Ui, label: &str, sc: &egui::KeyboardShortcut, enabled: bool) -> bool {
    let button = egui::Button::new(label).shortcut_text(ui.ctx().format_shortcut(sc));
    let clicked = ui.add_enabled(enabled, button).clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

/// A menu row that reports state — the active zoom mode, for instance.
fn check_item(ui: &mut egui::Ui, label: &str, sc: &egui::KeyboardShortcut, on: bool) -> bool {
    let button = egui::Button::new(label)
        .shortcut_text(ui.ctx().format_shortcut(sc))
        .selected(on);
    let clicked = ui.add(button).clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

/// The actions that apply to one image file, shared by the File menu, the
/// gallery tiles and the preview's own context menu.
///
/// `shortcuts` is off for the gallery tiles: the keys act on whatever the detail
/// pane shows, so advertising them next to a tile you merely right-clicked would
/// be a lie.
fn image_menu_items(
    ui: &mut egui::Ui,
    path: &Path,
    generating: bool,
    shortcuts: bool,
    acts: &mut Vec<Act>,
) {
    let row = |ui: &mut egui::Ui, label: &str, sc: &egui::KeyboardShortcut, enabled: bool| {
        if shortcuts {
            return menu_item(ui, label, sc, enabled);
        }
        let clicked = ui.add_enabled(enabled, egui::Button::new(label)).clicked();
        if clicked {
            ui.close_menu();
        }
        clicked
    };
    if row(ui, REVEAL_LABEL, &SC_REVEAL, true) {
        acts.push(Act::Reveal(path.to_path_buf()));
    }
    if row(ui, "Open in default viewer", &SC_OPEN, true) {
        acts.push(Act::OpenExternally(path.to_path_buf()));
    }
    if row(ui, "Copy image path", &SC_COPY_PATH, true) {
        acts.push(Act::Copy {
            text: path.display().to_string(),
            what: "Path",
        });
    }
    ui.separator();
    if row(ui, "Use image as reference", &SC_USE_REF, !generating) {
        acts.push(Act::UseAsRef(path.to_path_buf()));
    }
}

/// Select the file in the platform's file manager. The child is waited on in a
/// throwaway thread — `open`/`explorer` return at once, but nobody reaps a
/// spawned process that we never wait for.
fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = Command::new("open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("explorer");
        c.arg(format!("/select,{}", path.display()));
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        // No portable "select this file", so open the folder that holds it.
        let mut c = Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(Path::new(".")));
        c
    };
    detach(cmd.spawn()?);
    Ok(())
}

/// Hand the file to whatever the desktop opens PNGs with.
fn open_externally(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg("start").arg("");
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = Command::new("xdg-open");
    detach(cmd.arg(path).spawn()?);
    Ok(())
}

/// Reap a fire-and-forget helper process off the UI thread.
fn detach(mut child: std::process::Child) {
    thread::spawn(move || {
        let _ = child.wait();
    });
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

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

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// File name, shortened to keep the thumbnail column narrow.
fn short_name(path: &Path) -> String {
    let name = file_name(path);
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
                        self.status = format!("Saved: {}", self.pretty(&path));
                        self.viewing = Some(Viewing::read(&path));
                        self.scroll_to_selected = true;
                        new_render = true;
                        finished = true;
                        break;
                    }
                    Ok(GenMsg::Failed(err)) => {
                        self.status = format!("Error: {err}");
                        self.show_log = true; // the reason is in the output
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

        // Keys first, so ⌘⏎ is gone before the prompt box could eat it. Then the
        // panels, outermost first: menu bar, status strip, sidebar, and the
        // detail pane (image + log) gets everything that's left.
        let mut acts: Vec<Act> = Vec::new();
        self.shortcuts(ctx, &mut acts);
        self.menu_bar_ui(ctx, &mut acts);
        self.status_bar_ui(ctx, &mut acts);
        self.sidebar_ui(ctx, &mut acts);
        self.detail_ui(ctx, &mut acts);
        for act in acts {
            self.act(ctx, act);
        }

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
    // Landscape by default: the sidebar wants ~400 pt, the image wants the rest.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([720.0, 480.0])
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
