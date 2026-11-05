use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use eframe::egui::{self, Color32, ColorImage, RichText};
use odyssey_ctl::caps::{Capabilities, Profile};
use odyssey_ctl::ddc::VcpValue;
use odyssey_ctl::features::{self, Feature, Status};

use crate::i18n::{Lang, tr};
use crate::settings::Settings;
use crate::worker::{Reply, Request, Worker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Picture,
    Game,
    Lighting,
    Source,
    Info,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::Picture, Tab::Game, Tab::Lighting, Tab::Source, Tab::Info];

    fn slug(self) -> &'static str {
        match self {
            Tab::Picture => "picture",
            Tab::Game => "game",
            Tab::Lighting => "lighting",
            Tab::Source => "source",
            Tab::Info => "info",
        }
    }

    fn features(self) -> &'static [&'static str] {
        match self {
            Tab::Picture => &[
                "picture-mode", "brightness", "contrast", "sharpness", "color-tone", "eye-saver",
                "red-gain", "green-gain", "blue-gain",
            ],
            Tab::Game => &["black-equalizer", "adaptive-sync", "aim-point"],
            Tab::Lighting => &["lighting", "lighting-effect", "lighting-color"],
            Tab::Source => &["input", "volume", "mute", "pip", "pip-source", "pip-sound", "pc-av-mode", "language"],
            Tab::Info => &[],
        }
    }
}

/// Status line; translated when drawn so it follows language changes.
enum Message {
    Loaded,
    Info(String),
    Error(String),
}

/// `ODYSSEY_CTL_SCREENSHOTS=<dir>`: capture every tab in every language, then quit.
struct Screenshots {
    dir: PathBuf,
    steps: Vec<(Lang, Tab)>,
    index: usize,
    frames: u32,
    requested: bool,
    tx: mpsc::Sender<Arc<ColorImage>>,
    rx: mpsc::Receiver<Arc<ColorImage>>,
}

pub struct App {
    worker: Worker,
    settings: Settings,
    tab: Tab,
    caps: Option<Capabilities>,
    values: BTreeMap<u8, Result<VcpValue, String>>,
    /// Slider values being dragged, sent to the monitor on release.
    drafts: BTreeMap<u8, u16>,
    pending: usize,
    message: Option<Message>,
    demo: bool,
    shots: Option<Screenshots>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, bus: Option<u32>, screenshots: Option<PathBuf>) -> Self {
        let shots = screenshots.map(|dir| {
            let (tx, rx) = mpsc::channel();
            let steps = Lang::ALL.iter().flat_map(|&l| Tab::ALL.map(|t| (l, t))).collect();
            Screenshots { dir, steps, index: 0, frames: 0, requested: false, tx, rx }
        });
        let mut app = App {
            worker: Worker::spawn(cc.egui_ctx.clone(), bus),
            settings: Settings::load(),
            tab: Tab::Picture,
            caps: None,
            values: BTreeMap::new(),
            drafts: BTreeMap::new(),
            pending: 0,
            message: None,
            demo: std::env::var("ODYSSEY_CTL_DEMO").is_ok_and(|v| v == "1"),
            shots,
        };
        app.request(Request::Load);
        app
    }

    fn t(&self, key: &str) -> String {
        tr(self.settings.language, key)
    }

    fn feature_label(&self, f: &Feature) -> String {
        self.t(&format!("f.{}", f.name))
    }

    fn value_text(&self, f: &Feature, value: u16) -> String {
        match f.value_name(value) {
            Some(n) => self.t(&format!("v.{n}")),
            None => value.to_string(),
        }
    }

    fn request(&mut self, r: Request) {
        self.pending += 1;
        self.worker.send(r);
    }

    fn save_settings(&mut self) {
        if self.shots.is_none()
            && let Err(e) = self.settings.save()
        {
            self.message = Some(Message::Error(e.to_string()));
        }
    }

    fn poll_worker(&mut self) {
        while let Some(reply) = self.worker.try_recv() {
            self.pending = self.pending.saturating_sub(1);
            match reply {
                Reply::Loaded { caps, readings } => {
                    match caps {
                        Ok(c) => self.caps = Some(c),
                        Err(e) => self.message = Some(Message::Error(e)),
                    }
                    self.values = readings.into_iter().map(|r| (r.feature.code, r.value)).collect();
                    self.drafts.clear();
                    if !matches!(self.message, Some(Message::Error(_))) {
                        self.message = Some(Message::Loaded);
                    }
                }
                Reply::Written { feature, result } => {
                    self.drafts.remove(&feature.code);
                    match result {
                        Ok(c) => {
                            if let Some(Ok(v)) = self.values.get_mut(&feature.code) {
                                v.current = c.after;
                            }
                            let text = format!(
                                "{}: {} -> {}",
                                self.feature_label(feature),
                                self.value_text(feature, c.before),
                                self.value_text(feature, c.after)
                            );
                            self.message = Some(Message::Info(text));
                        }
                        Err(e) => {
                            self.message = Some(Message::Error(e));
                            self.request(Request::Load); // show what the monitor really has
                        }
                    }
                }
                Reply::Sdm(Ok((eb, caps))) => {
                    let model = caps.model.clone().unwrap_or_default();
                    self.message = Some(Message::Info(format!("EB = {eb:#04x}, {model}")));
                    self.caps = Some(caps);
                }
                Reply::Sdm(Err(e)) | Reply::Failed(e) => self.message = Some(Message::Error(e)),
            }
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(s) = self.shots.as_mut() else { return };
        ctx.request_repaint();
        if self.pending > 0 {
            return; // wait for the monitor data
        }
        if let Ok(image) = s.rx.try_recv() {
            let (lang, tab) = s.steps[s.index];
            let path = s.dir.join(format!("{:02}_{}_{}.png", s.index, lang.code(), tab.slug()));
            let [w, h] = image.size;
            if let Err(e) = image::save_buffer(&path, image.as_raw(), w as u32, h as u32, image::ColorType::Rgba8) {
                eprintln!("{}: {e}", path.display());
            }
            s.index += 1;
            s.frames = 0;
            s.requested = false;
        }
        let Some(&(lang, tab)) = s.steps.get(s.index) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        self.settings.language = lang;
        self.tab = tab;
        s.frames += 1;
        if s.frames >= 5 && !s.requested {
            s.requested = true;
            let tx = s.tx.clone();
            let repaint = ctx.clone();
            ctx.request_screenshot(move |image| {
                let _ = tx.send(image);
                repaint.request_repaint();
            });
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Samsung Odyssey");
            if let Some(model) = self.caps.as_ref().and_then(|c| c.model.as_deref()) {
                ui.label(RichText::new(model).weak());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(self.pending == 0, egui::Button::new(self.t("refresh"))).clicked() {
                    self.request(Request::Load);
                }
                if self.pending > 0 {
                    ui.spinner();
                }
            });
        });
        ui.horizontal(|ui| {
            for tab in Tab::ALL {
                let label = self.t(&format!("tab.{}", tab.slug()));
                ui.selectable_value(&mut self.tab, tab, label);
            }
        });
        ui.add_space(4.0);
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.pending > 0 {
                ui.label(self.t("busy"));
            } else {
                match &self.message {
                    Some(Message::Loaded) => {
                        ui.label(self.t("loaded"));
                    }
                    Some(Message::Info(s)) => {
                        ui.label(s);
                    }
                    Some(Message::Error(s)) => {
                        ui.colored_label(Color32::from_rgb(230, 90, 80), s);
                    }
                    None => {}
                }
            }
            if self.demo {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(self.t("demo")).weak());
                });
            }
        });
    }

    fn feature_grid(&mut self, ui: &mut egui::Ui, tab: Tab) {
        let locked = tab
            .features()
            .iter()
            .filter_map(|n| features::find(n))
            .any(|f| !f.read_only && !f.status.safe_to_write());
        if locked && !self.settings.allow_unverified {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(self.t("locked_banner")).color(Color32::from_rgb(230, 140, 70)));
                if ui.button(self.t("unlock")).clicked() {
                    self.settings.allow_unverified = true;
                    self.save_settings();
                }
            });
            ui.add_space(6.0);
        }
        egui::Grid::new(tab.slug()).num_columns(3).spacing([18.0, 12.0]).striped(true).show(ui, |ui| {
            for name in tab.features() {
                let f = features::find(name).expect("tab lists known features");
                self.feature_row(ui, f);
                ui.end_row();
            }
        });
    }

    fn feature_row(&mut self, ui: &mut egui::Ui, f: &'static Feature) {
        ui.label(self.feature_label(f));
        let allowed = f.status.safe_to_write() || self.settings.allow_unverified;
        let enabled = allowed && self.pending == 0;
        let mut write = None;
        match self.values.get(&f.code).cloned() {
            None => {
                ui.label(RichText::new(self.t("no_data")).weak());
            }
            Some(Err(_)) => {
                ui.label(RichText::new(self.t("not_supported")).weak());
            }
            Some(Ok(v)) if features::is_disabled(v.current, v.max) => {
                ui.label(RichText::new(self.t("disabled_now")).weak());
            }
            Some(Ok(v)) if f.read_only => {
                ui.label(format!("{} {} ({:#06x})", self.t("raw_value"), v.current, v.current))
                    .on_hover_text(self.t("raw_hint"));
            }
            Some(Ok(v)) if !f.values.is_empty() => {
                let mut selected = v.current;
                ui.add_enabled_ui(enabled, |ui| {
                    egui::ComboBox::from_id_salt(f.code)
                        .width(200.0)
                        .selected_text(self.value_text(f, v.current))
                        .show_ui(ui, |ui| {
                            for &(value, _) in f.values.iter().filter(|(value, _)| *value <= v.max) {
                                ui.selectable_value(&mut selected, value, self.value_text(f, value));
                            }
                        });
                });
                if selected != v.current {
                    write = Some(selected);
                }
            }
            Some(Ok(v)) if v.max > 0 && v.current <= v.max => {
                let mut value = self.drafts.get(&f.code).copied().unwrap_or(v.current);
                let r = ui.add_enabled(enabled, egui::Slider::new(&mut value, 0..=v.max));
                if r.changed() {
                    self.drafts.insert(f.code, value);
                }
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    if value != v.current {
                        write = Some(value);
                    } else {
                        self.drafts.remove(&f.code);
                    }
                }
            }
            Some(Ok(v)) => {
                ui.label(format!("{} {} ({:#06x})", self.t("raw_value"), v.current, v.current))
                    .on_hover_text(self.t("raw_hint"));
            }
        }
        let color = match f.status {
            Status::Verified => Color32::from_rgb(110, 190, 120),
            Status::Standard => Color32::from_rgb(130, 160, 200),
            Status::ReadVerified => Color32::from_rgb(220, 190, 90),
            Status::Unverified => Color32::from_rgb(230, 140, 70),
        };
        let badge = ui.label(RichText::new(self.t(&format!("status.{}", f.status.as_str()))).small().color(color));
        if !allowed {
            badge.on_hover_text(self.t("locked_hint"));
        }
        if let Some(value) = write {
            let allow_unverified = self.settings.allow_unverified;
            self.request(Request::Write { feature: f, value, allow_unverified });
        }
    }

    fn info(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.t("monitor"));
        if let Some(caps) = &self.caps {
            egui::Grid::new("caps").num_columns(2).spacing([18.0, 8.0]).show(ui, |ui| {
                ui.label(self.t("model"));
                let kind = match caps.profile() {
                    Profile::Lemon(_) => self.t("profile.lemon"),
                    Profile::Sec => self.t("profile.sec"),
                    Profile::Falcon => self.t("profile.falcon"),
                    Profile::Other(_) => String::new(),
                };
                ui.label(format!("{}  {kind}", caps.model.as_deref().unwrap_or("?")));
                ui.end_row();
                ui.label(self.t("mccs"));
                ui.label(caps.mccs_version.as_deref().unwrap_or("?"));
                ui.end_row();
                ui.label(self.t("codes"));
                let codes: Vec<String> = caps.vcp.keys().map(|c| format!("{c:02X}")).collect();
                ui.add(egui::Label::new(RichText::new(codes.join(" ")).monospace()).wrap());
                ui.end_row();
            });
            egui::CollapsingHeader::new(self.t("caps_raw")).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(&caps.raw).monospace().small()).wrap());
            });
        } else {
            ui.label(RichText::new(self.t("no_data")).weak());
        }

        ui.separator();
        ui.heading(self.t("sdm"));
        ui.label(RichText::new(self.t("sdm_hint")).weak());
        ui.horizontal(|ui| {
            if ui.add_enabled(self.pending == 0, egui::Button::new(self.t("sdm_on"))).clicked() {
                self.request(Request::Sdm(true));
            }
            if ui.add_enabled(self.pending == 0, egui::Button::new(self.t("sdm_off"))).clicked() {
                self.request(Request::Sdm(false));
            }
        });

        ui.separator();
        ui.heading(self.t("app_settings"));
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(self.t("language"));
            egui::ComboBox::from_id_salt("language")
                .selected_text(self.settings.language.native_name())
                .show_ui(ui, |ui| {
                    for lang in Lang::ALL {
                        changed |= ui.selectable_value(&mut self.settings.language, lang, lang.native_name()).changed();
                    }
                });
        });
        let label = self.t("allow_unverified");
        changed |= ui.checkbox(&mut self.settings.allow_unverified, label).changed();
        ui.label(RichText::new(self.t("allow_unverified_hint")).weak());
        if changed {
            self.save_settings();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_worker();
        let ctx = ui.ctx().clone();
        self.screenshot_step(&ctx);
        egui::Panel::top("header").show(ui, |ui| self.header(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.spacing_mut().slider_width = 300.0;
            egui::ScrollArea::vertical().show(ui, |ui| match self.tab {
                Tab::Info => self.info(ui),
                tab => self.feature_grid(ui, tab),
            });
        });
    }
}
