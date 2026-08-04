#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Minimal desktop front-end for the local image generator.
//!
//! It does not reimplement any model logic — it shells out to the repo's
//! `generate.sh` (setting `MFLUX_MODEL` and, optionally, `MFLUX_SIZE` /
//! `MFLUX_STEPS` / `MFLUX_SEED`), which already maps aliases to the right
//! `mflux-generate-*` command, handles quantization, and writes a PNG. The GUI
//! collects a prompt + options, runs the script on a background thread while
//! streaming its output to a live log, and shows the resulting image.

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui;
use egui::ColorImage;

/// Model aliases understood by `generate.sh` (`MFLUX_MODEL=<alias>`), with a
/// short description. First entry is the default (matches the script default).
const MODELS: &[(&str, &str)] = &[
    ("z-image-turbo", "Z-Image Turbo 6B — fastest (~18s)"),
    ("qwen-2512", "Qwen-Image-2512 20B — best all-round quality"),
    ("flux2", "FLUX.2 klein 9B — strong prompt adherence + text"),
    ("flux2-4b", "FLUX.2 klein 4B — lighter/faster klein"),
    ("z-image", "Z-Image 6B base — higher quality, more steps"),
    ("qwen", "Qwen-Image 20B — original"),
    ("flux-dev", "FLUX.1 dev 12B — classic Flux"),
    ("flux-schnell", "FLUX.1 schnell — 4-step drafts"),
    ("krea", "FLUX.1 Krea dev — photographic"),
];

const LOG_MAX_LINES: usize = 400;

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
    env: Vec<(String, String)>,
}

struct ImagenApp {
    root: PathBuf,
    prompt: String,
    model_idx: usize,
    size: u32,
    override_steps: bool,
    steps: u32,
    fixed_seed: bool,
    seed: u32,
    status: String,
    generating: bool,
    rx: Option<Receiver<GenMsg>>,
    texture: Option<egui::TextureHandle>,
    log: Vec<String>,
}

impl ImagenApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            root: repo_root(),
            prompt: String::new(),
            model_idx: 0,
            size: 1024,
            override_steps: false,
            steps: 20,
            fixed_seed: false,
            seed: 0,
            status: "Ready.".to_owned(),
            generating: false,
            rx: None,
            texture: None,
            log: Vec::new(),
        }
    }

    fn push_log(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > LOG_MAX_LINES {
            let cut = self.log.len() - LOG_MAX_LINES;
            self.log.drain(0..cut);
        }
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

        // Optional overrides -> the env vars generate.sh already reads.
        let mut env = vec![("MFLUX_SIZE".to_owned(), self.size.to_string())];
        if self.override_steps {
            env.push(("MFLUX_STEPS".to_owned(), self.steps.to_string()));
        }
        if self.fixed_seed {
            env.push(("MFLUX_SEED".to_owned(), self.seed.to_string()));
        }

        let params = GenParams {
            root: self.root.clone(),
            script,
            model: MODELS[self.model_idx].0.to_owned(),
            prompt,
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
}

/// Run `generate.sh`, streaming each output line back over `tx`, then report the
/// decoded PNG (or an error). Runs on a background thread.
fn run_generate(p: GenParams, tx: Sender<GenMsg>, ctx: egui::Context) {
    let mut cmd = Command::new("bash");
    cmd.arg(&p.script)
        .arg(&p.prompt)
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
            let _ = tx.send(GenMsg::Failed(format!("waiting on generate.sh failed: {e}")));
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

impl eframe::App for ImagenApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain everything the worker has sent since last frame.
        if let Some(rx) = self.rx.take() {
            let mut finished = false;
            loop {
                match rx.try_recv() {
                    Ok(GenMsg::Line(line)) => self.push_log(line),
                    Ok(GenMsg::Done { path, image }) => {
                        self.texture =
                            Some(ctx.load_texture("generated", image, egui::TextureOptions::LINEAR));
                        self.status = format!("Saved: {}", path.display());
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

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Local Image Generation");
            ui.add_space(6.0);

            egui::ComboBox::from_label("Model")
                .selected_text(MODELS[self.model_idx].0)
                .show_ui(ui, |ui| {
                    for (i, (alias, desc)) in MODELS.iter().enumerate() {
                        ui.selectable_value(&mut self.model_idx, i, format!("{alias} — {desc}"));
                    }
                });

            ui.add_space(8.0);
            ui.label("Prompt");
            ui.add_enabled(
                !self.generating,
                egui::TextEdit::multiline(&mut self.prompt)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .hint_text("a vast cyberpunk cityscape at sunset, neon reflections…"),
            );

            ui.add_space(8.0);
            ui.add_enabled_ui(!self.generating, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Size");
                    ui.add(egui::DragValue::new(&mut self.size).range(256..=2048).speed(8));
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
                        egui::DragValue::new(&mut self.seed).range(0..=u32::MAX).speed(1.0),
                    );
                });
            });
            ui.small("Unchecked = model default steps / random seed.");

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ready = !self.generating && !self.prompt.trim().is_empty();
                if ui.add_enabled(ready, egui::Button::new("Generate")).clicked() {
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

            if let Some(tex) = &self.texture {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 960.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ai-imagegen",
        options,
        Box::new(|cc| Ok(Box::new(ImagenApp::new(cc)))),
    )
}
