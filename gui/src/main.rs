#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Minimal desktop front-end for the local image generator.
//!
//! It does not reimplement any model logic — it shells out to the repo's
//! `generate.sh` (setting `MFLUX_MODEL`), which already maps aliases to the right
//! `mflux-generate-*` command, handles quantization, and writes a PNG. The GUI
//! just collects a prompt + model, runs the script on a background thread, and
//! shows the resulting image.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};
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

/// Result of one background generation run.
enum GenMsg {
    Done { path: PathBuf, image: ColorImage },
    Failed(String),
}

struct ImagenApp {
    root: PathBuf,
    prompt: String,
    model_idx: usize,
    status: String,
    generating: bool,
    rx: Option<Receiver<GenMsg>>,
    texture: Option<egui::TextureHandle>,
}

impl ImagenApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            root: repo_root(),
            prompt: String::new(),
            model_idx: 0,
            status: "Ready.".to_owned(),
            generating: false,
            rx: None,
            texture: None,
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

        let model = MODELS[self.model_idx].0.to_owned();
        let root = self.root.clone();
        let ctx = ctx.clone();
        let (tx, rx): (Sender<GenMsg>, Receiver<GenMsg>) = std::sync::mpsc::channel();

        self.rx = Some(rx);
        self.generating = true;
        self.status =
            format!("Generating with '{model}'…  (a model's first run also loads weights)");

        thread::spawn(move || {
            let msg = run_generate(&root, &script, &model, &prompt);
            let _ = tx.send(msg);
            ctx.request_repaint(); // wake the UI as soon as we're done
        });
    }
}

/// Run `generate.sh` to completion and decode the PNG it reports.
fn run_generate(root: &Path, script: &Path, model: &str, prompt: &str) -> GenMsg {
    let output = Command::new("bash")
        .arg(script)
        .arg(prompt)
        .env("MFLUX_MODEL", model)
        .current_dir(root)
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => return GenMsg::Failed(format!("failed to launch generate.sh: {e}")),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let lines: Vec<&str> = stderr.lines().collect();
        let tail = lines[lines.len().saturating_sub(8)..].join("\n");
        return GenMsg::Failed(format!("generate.sh exited with {}\n{tail}", output.status));
    }

    // The script prints `Saved: <path>` as its final line.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let saved = stdout
        .lines()
        .rev()
        .find_map(|l| l.strip_prefix("Saved:").map(|s| s.trim().to_owned()));

    let path = match saved {
        Some(p) => PathBuf::from(p),
        None => return GenMsg::Failed("could not find output path in generate.sh output".to_owned()),
    };

    match load_color_image(&path) {
        Ok(image) => GenMsg::Done { path, image },
        Err(e) => GenMsg::Failed(format!("generated {} but failed to load it: {e}", path.display())),
    }
}

fn load_color_image(path: &Path) -> Result<ColorImage, String> {
    let img = image::open(path).map_err(|e| e.to_string())?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = img.into_raw();
    Ok(ColorImage::from_rgba_unmultiplied(size, &pixels))
}

impl eframe::App for ImagenApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Pick up a finished background run, if any.
        if let Some(rx) = &self.rx {
            if let Ok(msg) = rx.try_recv() {
                match msg {
                    GenMsg::Done { path, image } => {
                        self.texture =
                            Some(ctx.load_texture("generated", image, egui::TextureOptions::LINEAR));
                        self.status = format!("Saved: {}", path.display());
                    }
                    GenMsg::Failed(err) => self.status = format!("Error: {err}"),
                }
                self.generating = false;
                self.rx = None;
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
            ui.horizontal(|ui| {
                let ready = !self.generating && !self.prompt.trim().is_empty();
                if ui.add_enabled(ready, egui::Button::new("Generate")).clicked() {
                    self.start_generation(ctx);
                }
                if self.generating {
                    ui.spinner();
                }
            });

            ui.add_space(6.0);
            ui.separator();
            ui.label(&self.status);

            if let Some(tex) = &self.texture {
                ui.add_space(8.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let avail = ui.available_width();
                    let size = tex.size_vec2();
                    let scale = (avail / size.x).min(1.0);
                    ui.image(egui::load::SizedTexture::new(tex.id(), size * scale));
                });
            }
        });

        // While a job runs, keep polling the channel each frame.
        if self.generating {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ai-imagegen",
        options,
        Box::new(|cc| Ok(Box::new(ImagenApp::new(cc)))),
    )
}
