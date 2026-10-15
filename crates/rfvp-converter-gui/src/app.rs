use eframe::egui::{self, Color32, RichText};
use rfvp_converter::{
    ConversionContext, ConversionRequest, Mode, Platform, ProgressEvent, Ps1Options,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const SETTINGS_KEY: &str = "converter-settings-v1";
const LOG_LIMIT: usize = 160;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    platform: Platform,
    input: String,
    output: String,
    mode: Mode,
    image_filter: String,
    layout: String,
    pack: String,
    threads: usize,
    overwrite: bool,
    ps1: Ps1Options,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            platform: Platform::Ps1,
            input: String::new(),
            output: String::new(),
            mode: Mode::Full,
            image_filter: "dpid-balanced".into(),
            layout: String::new(),
            pack: String::new(),
            threads: 0,
            overwrite: false,
            ps1: Ps1Options::default(),
        }
    }
}

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Ps1 => "PS1",
        Platform::Ps2 => "PS2",
        Platform::Psp => "PSP",
        Platform::ThreeDs => "3DS",
        Platform::Ps3 => "PS3",
        Platform::Psv => "PS Vita",
        Platform::Switch => "Switch",
    }
}

fn native(platform: Platform) -> bool {
    matches!(platform, Platform::Ps3 | Platform::Psv | Platform::Switch)
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Full => "Full conversion",
        Mode::ImagesOnly => "Update images",
        Mode::MasksOnly => "Update PS1 masks",
    }
}

impl Settings {
    fn request(&self) -> ConversionRequest {
        ConversionRequest {
            platform: self.platform,
            input: PathBuf::from(&self.input),
            output: PathBuf::from(&self.output),
            mode: self.mode,
            image_filter: self.image_filter.clone(),
            ui_layout: (!native(self.platform) && !self.layout.trim().is_empty())
                .then(|| PathBuf::from(&self.layout)),
            image_pack: if self.mode == Mode::Full {
                vec![]
            } else {
                self.pack
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            },
            threads: self.threads,
            overwrite: self.overwrite,
            ps1: if self.platform == Platform::Ps1 {
                self.ps1.clone()
            } else {
                Ps1Options::default()
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Idle,
    Checking,
    Running,
    Complete,
    Cancelled,
    Failed,
}

impl Status {
    fn active(self) -> bool {
        matches!(self, Self::Checking | Self::Running)
    }
}

struct Job {
    status: Status,
    progress: Option<ProgressEvent>,
    logs: VecDeque<String>,
    result: String,
    output: Option<PathBuf>,
    started: Option<Instant>,
    elapsed: Duration,
}

impl Default for Job {
    fn default() -> Self {
        Self {
            status: Status::Idle,
            progress: None,
            logs: VecDeque::new(),
            result: "Choose an original game directory and a separate output directory.".into(),
            output: None,
            started: None,
            elapsed: Duration::ZERO,
        }
    }
}

impl Job {
    fn log(&mut self, message: String) {
        if self.logs.len() >= LOG_LIMIT {
            self.logs.pop_front();
        }
        self.logs.push_back(message);
    }
}

pub struct ConverterApp {
    settings: Settings,
    job: Arc<Mutex<Job>>,
    cancelled: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    pending_close: bool,
}

impl ConverterApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "game-path-fallback".into(),
            Arc::new(egui::FontData::from_static(include_bytes!(
                "../../rfvp-rebuilder/assets/fonts/BIZUDGothic-Regular.ttf"
            ))),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("game-path-fallback".into());
        }
        cc.egui_ctx.set_fonts(fonts);
        cc.egui_ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = egui::vec2(10.0, 10.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
        });
        let settings = cc
            .storage
            .and_then(|storage| eframe::get_value(storage, SETTINGS_KEY))
            .unwrap_or_default();
        Self {
            settings,
            job: Arc::new(Mutex::new(Job::default())),
            cancelled: Arc::new(AtomicBool::new(false)),
            worker: None,
            pending_close: false,
        }
    }

    fn start(&mut self, ctx: &egui::Context, check_only: bool) {
        if self.worker.is_some() {
            return;
        }
        self.cancelled.store(false, Ordering::Relaxed);
        let request = self.settings.request();
        *self.job.lock().unwrap() = Job {
            status: if check_only {
                Status::Checking
            } else {
                Status::Running
            },
            result: if check_only {
                "Checking FFmpeg tools..."
            } else {
                "Preparing conversion..."
            }
            .into(),
            started: Some(Instant::now()),
            ..Default::default()
        };
        let job = Arc::clone(&self.job);
        let cancelled = Arc::clone(&self.cancelled);
        let repaint = ctx.clone();
        let spawn = std::thread::Builder::new().name("game-converter".into()).spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if check_only {
                    let tools = rfvp_converter::dependencies(request.platform, request.mode, &request.ps1);
                    anyhow::ensure!(!cancelled.load(Ordering::Relaxed), "Tool check cancelled.");
                    let mut state = job.lock().unwrap();
                    for tool in &tools {
                        state.log(format!("{}: {} ({})", tool.name,
                            if tool.available { "available" } else { "missing" },
                            if tool.required { "required" } else { "optional" }));
                    }
                    anyhow::ensure!(tools.iter().all(|tool| !tool.required || tool.available),
                        "Install FFmpeg and make ffmpeg/ffprobe available on PATH.");
                    return Ok(None);
                }
                let observer_job = Arc::clone(&job);
                let observer_cancelled = Arc::clone(&cancelled);
                let observer_repaint = repaint.clone();
                let context = ConversionContext::new(move |event| {
                    anyhow::ensure!(!observer_cancelled.load(Ordering::Relaxed), "Conversion cancelled.");
                    let mut state = observer_job.lock().unwrap();
                    state.progress = Some(event.clone());
                    if let Some(message) = &event.message {
                        state.log(message.clone());
                    }
                    drop(state);
                    observer_repaint.request_repaint();
                    Ok(())
                });
                anyhow::ensure!(!cancelled.load(Ordering::Relaxed), "Conversion cancelled.");
                rfvp_converter::convert(&request, &context).map(Some)
            }));
            let mut state = job.lock().unwrap();
            state.elapsed = state.started.map_or(Duration::ZERO, |started| started.elapsed());
            match outcome {
                Ok(Ok(summary)) => {
                    state.status = Status::Complete;
                    state.result = if let Some(summary) = summary {
                        state.output = Some(summary.output);
                        format!("Complete: {} files, {:.1} MiB", summary.files, summary.bytes as f64 / 1_048_576.0)
                    } else {
                        "Required tools are available. Ready to convert.".into()
                    };
                }
                Ok(Err(error)) => {
                    state.status = if cancelled.load(Ordering::Relaxed) { Status::Cancelled } else { Status::Failed };
                    state.result = if state.status == Status::Cancelled {
                        "Cancelled. Partial output may remain; use overwrite to retry a full conversion.".into()
                    } else {
                        format!("{error:#}")
                    };
                    let result = state.result.clone();
                    state.log(result);
                }
                Err(_) => {
                    state.status = Status::Failed;
                    state.result = "Conversion worker failed. Partial output may remain.".into();
                }
            }
            drop(state);
            repaint.request_repaint();
        });
        match spawn {
            Ok(worker) => self.worker = Some(worker),
            Err(error) => {
                let mut state = self.job.lock().unwrap();
                state.status = Status::Failed;
                state.result = format!("Cannot start conversion worker: {error}");
            }
        }
    }

    fn paths(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Game directories").strong());
        let field_width = (ui.available_width() - 240.0).max(160.0);
        egui::Grid::new("directories")
            .num_columns(3)
            .spacing([12.0, 10.0])
            .show(ui, |ui| {
                ui.label("Original game");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.input).desired_width(field_width),
                );
                if ui.button("Browse...").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("Choose the original game directory")
                        .pick_folder()
                {
                    if self.settings.output.is_empty()
                        && let Some(parent) = path.parent()
                    {
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        self.settings.output = parent
                            .join(format!(
                                "{name}-{}",
                                platform_name(self.settings.platform).replace(' ', "-")
                            ))
                            .to_string_lossy()
                            .into_owned();
                    }
                    self.settings.input = path.to_string_lossy().into_owned();
                }
                ui.end_row();
                ui.label("Output directory");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.output)
                        .desired_width(field_width),
                );
                if ui.button("Browse...").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .set_title("Choose a separate output directory")
                        .pick_folder()
                {
                    self.settings.output = path.to_string_lossy().into_owned();
                }
                ui.end_row();
            });
        ui.small(
            "Use a separate output directory for each platform. You can type a new directory path.",
        );
    }

    fn configuration(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for info in rfvp_converter::platforms() {
                if ui
                    .selectable_value(
                        &mut self.settings.platform,
                        info.platform,
                        platform_name(info.platform),
                    )
                    .changed()
                    && self.settings.mode == Mode::MasksOnly
                    && self.settings.platform != Platform::Ps1
                {
                    self.settings.mode = Mode::ImagesOnly;
                }
            }
        });
        let info = rfvp_converter::platforms()
            .into_iter()
            .find(|info| info.platform == self.settings.platform)
            .unwrap();
        ui.small(format!(
            "{} x {}   |   {}",
            info.display_width, info.display_height, info.install_directory
        ));
        ui.label(if native(self.settings.platform) {
            "Prepare original packs and font. Image dimensions are preserved."
        } else if self.settings.platform == Platform::Ps1 {
            "PS1 textures, offline Bayer mask dithering, bitmap font and native media."
        } else {
            "Reduce images for the target screen and generate bitmap font and media."
        });
        ui.separator();
        self.paths(ui);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label("Conversion");
            egui::ComboBox::from_id_salt("mode")
                .selected_text(mode_name(self.settings.mode))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.settings.mode, Mode::Full, mode_name(Mode::Full));
                    ui.selectable_value(
                        &mut self.settings.mode,
                        Mode::ImagesOnly,
                        mode_name(Mode::ImagesOnly),
                    );
                    if self.settings.platform == Platform::Ps1 {
                        ui.selectable_value(
                            &mut self.settings.mode,
                            Mode::MasksOnly,
                            mode_name(Mode::MasksOnly),
                        );
                    }
                });
            if self.settings.mode == Mode::Full {
                ui.checkbox(&mut self.settings.overwrite, "Overwrite existing output");
            }
        });
        if self.settings.mode != Mode::Full {
            ui.small(
                "Updates require an existing converted directory. Other assets are preserved.",
            );
            ui.horizontal(|ui| {
                ui.label("Image pack");
                ui.add(
                    egui::TextEdit::singleline(&mut self.settings.pack)
                        .hint_text("All packs (or graph)")
                        .desired_width(300.0),
                );
            });
        }
        egui::CollapsingHeader::new("Advanced options").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Worker threads");
                ui.add(egui::DragValue::new(&mut self.settings.threads).range(0..=128));
                ui.small("0 = automatic");
            });
            if !native(self.settings.platform) {
                ui.horizontal(|ui| {
                    ui.label("Image reduction");
                    egui::ComboBox::from_id_salt("filter")
                        .selected_text(&self.settings.image_filter)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.settings.image_filter,
                                "dpid-balanced".into(),
                                "Balanced detail",
                            );
                            ui.selectable_value(
                                &mut self.settings.image_filter,
                                "area".into(),
                                "Area average",
                            );
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("UI layout JSON");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.layout)
                            .hint_text("Optional")
                            .desired_width((ui.available_width() - 160.0).max(160.0)),
                    );
                    if ui.button("Browse...").clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON layout", &["json"])
                            .pick_file()
                    {
                        self.settings.layout = path.to_string_lossy().into_owned();
                    }
                    if ui.button("Clear").clicked() {
                        self.settings.layout.clear();
                    }
                });
            }
            if self.settings.platform == Platform::Ps1 {
                ui.horizontal(|ui| {
                    ui.label("Bitmap font size");
                    ui.add(egui::DragValue::new(&mut self.settings.ps1.font_size).range(6..=32));
                });
                ui.checkbox(&mut self.settings.ps1.no_movies, "Skip movies");
                ui.checkbox(&mut self.settings.ps1.no_music, "Skip music");
            }
        });
    }

    fn progress(&mut self, ui: &mut egui::Ui) {
        let state = self.job.lock().unwrap();
        let cancelling = self.cancelled.load(Ordering::Relaxed) && state.status.active();
        let color = match state.status {
            Status::Failed => ui.visuals().error_fg_color,
            Status::Complete => Color32::from_rgb(65, 180, 125),
            _ => ui.visuals().text_color(),
        };
        ui.label(
            RichText::new(if cancelling {
                "Stopping after in-flight work finishes..."
            } else {
                &state.result
            })
            .color(color),
        );
        if let Some(event) = &state.progress {
            let fraction = if event.total == 0 {
                0.0
            } else {
                event.completed as f32 / event.total as f32
            };
            let mut bar = egui::ProgressBar::new(fraction.min(1.0)).text(format!(
                "{}: {} / {}",
                event.stage, event.completed, event.total
            ));
            if state.status.active() && event.total == 0 {
                bar = bar.animate(true);
            }
            ui.add(bar);
            if let Some(resource) = &event.resource {
                ui.add(egui::Label::new(RichText::new(resource).small()).truncate());
            }
        } else if state.status.active() {
            ui.spinner();
        }
        let elapsed = if state.status.active() {
            state
                .started
                .map_or(Duration::ZERO, |started| started.elapsed())
        } else {
            state.elapsed
        };
        if state.started.is_some() {
            ui.small(format!(
                "Elapsed: {}m {:02}s",
                elapsed.as_secs() / 60,
                elapsed.as_secs() % 60
            ));
        }
        if let Some(output) = &state.output {
            ui.horizontal(|ui| {
                if ui.button("Open output directory").clicked()
                    && let Err(error) = open_directory(output)
                {
                    // The path remains visible even when the desktop file manager is unavailable.
                    ui.label(format!("Cannot open directory: {error}"));
                }
                if ui.button("Copy output path").clicked() {
                    ui.ctx().copy_text(output.to_string_lossy().into_owned());
                }
            });
            ui.add(egui::Label::new(output.to_string_lossy()).wrap());
        }
        egui::CollapsingHeader::new("Conversion log")
            .default_open(state.status == Status::Failed)
            .show(ui, |ui| {
                if ui.button("Copy log").clicked() {
                    ui.ctx()
                        .copy_text(state.logs.iter().cloned().collect::<Vec<_>>().join("\n"));
                }
                egui::ScrollArea::vertical()
                    .id_salt("log")
                    .max_height(180.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &state.logs {
                            ui.add(
                                egui::Label::new(RichText::new(line).monospace().small()).wrap(),
                            );
                        }
                    });
            });
    }
}

impl eframe::App for ConverterApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            let _ = self.worker.take().unwrap().join();
        }
        let busy = self.worker.is_some();
        if ui.ctx().input(|input| input.viewport().close_requested()) && busy {
            self.pending_close = true;
            self.cancelled.store(true, Ordering::Relaxed);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.pending_close && !busy {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if busy {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("RFVP Converter");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::widgets::global_theme_preference_buttons(ui);
                    });
                });
                ui.label("Prepare your game for a console");
                ui.add_space(12.0);
                ui.add_enabled_ui(!busy, |ui| self.configuration(ui));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let ready = !busy
                        && !self.settings.input.trim().is_empty()
                        && !self.settings.output.trim().is_empty();
                    let button = egui::Button::new(
                        RichText::new("Start conversion")
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(40, 104, 200))
                    .min_size(egui::vec2(170.0, 38.0));
                    if ui.add_enabled(ready, button).clicked() {
                        self.start(ui.ctx(), false);
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Check media tools"))
                        .clicked()
                    {
                        self.start(ui.ctx(), true);
                    }
                    if ui
                        .add_enabled(
                            busy && !self.cancelled.load(Ordering::Relaxed),
                            egui::Button::new("Cancel"),
                        )
                        .clicked()
                    {
                        self.cancelled.store(true, Ordering::Relaxed);
                    }
                });
                ui.separator();
                self.progress(ui);
                egui::CollapsingHeader::new("Font license").show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("font-license")
                        .max_height(150.0)
                        .show(ui, |ui| {
                            ui.label(include_str!("../../rfvp-rebuilder/assets/fonts/OFL.txt"));
                        });
                });
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.settings);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

fn open_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let command = "explorer";
    #[cfg(target_os = "macos")]
    let command = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let command = "xdg-open";
    std::process::Command::new(command)
        .arg(path)
        .spawn()
        .map(|_| ())
}
