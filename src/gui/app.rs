use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use crate::desktop::{self, Shell};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Color32, ColorImage, Layout, RichText, Stroke, Vec2};
use odyssey_ctl::caps::{Capabilities, Profile};
use odyssey_ctl::ddc::VcpValue;
use odyssey_ctl::features::{self, Feature, SyncState};
use odyssey_ctl::presets as preset_logic;

use crate::i18n::{Lang, tr};
use crate::presets::{Preset, Presets};
use crate::settings::Settings;
use crate::theme::{self, ACCENT, ERR, OK, TEXT_DIM, WARN, dim};
use crate::worker::{Reply, Request, Worker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Presets,
    Picture,
    Game,
    Lighting,
    Source,
    Monitor,
    Settings,
}

impl Page {
    const ALL: [Page; 7] = [
        Page::Presets,
        Page::Picture,
        Page::Game,
        Page::Lighting,
        Page::Source,
        Page::Monitor,
        Page::Settings,
    ];

    fn slug(self) -> &'static str {
        match self {
            Page::Presets => "presets",
            Page::Picture => "picture",
            Page::Game => "game",
            Page::Lighting => "lighting",
            Page::Source => "source",
            Page::Monitor => "monitor",
            Page::Settings => "settings",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Page::Presets => "★",
            Page::Picture => "◐",
            Page::Game => "◎",
            Page::Lighting => "✦",
            Page::Source => "⇄",
            Page::Monitor => "▣",
            Page::Settings => "⚙",
        }
    }
}

/// Status line under the page title; translated when drawn so it follows language changes.
enum Message {
    Loaded(Instant),
    Info(String, Instant),
    Error(String),
}

/// `ODYSSEY_CTL_SCREENSHOTS=<dir>`: capture every page in every language, then quit.
struct Screenshots {
    dir: PathBuf,
    steps: Vec<(Lang, Page)>,
    index: usize,
    frames: u32,
    requested: bool,
    tx: mpsc::Sender<Arc<ColorImage>>,
    rx: mpsc::Receiver<Arc<ColorImage>>,
}

pub struct App {
    shell: Arc<Shell>,
    worker: Worker,
    settings: Settings,
    page: Page,
    name: Option<String>,
    caps: Option<Capabilities>,
    values: BTreeMap<u8, Result<VcpValue, String>>,
    /// Slider values being dragged, sent to the monitor on release.
    drafts: BTreeMap<u8, u16>,
    /// Requests sent to the worker, oldest first: true for those that write to the monitor.
    /// The worker answers in order.
    in_flight: VecDeque<bool>,
    connected: bool,
    message: Option<Message>,
    demo: bool,
    shots: Option<Screenshots>,
    presets: Presets,
    /// Name typed for a new preset.
    new_name: String,
    /// Preset waiting for the second click on "Delete".
    confirm_delete: Option<usize>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shell: Arc<Shell>,
        bus: Option<u32>,
        screenshots: Option<PathBuf>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        if let Ok(mut ctx) = shell.ctx.lock() {
            *ctx = Some(cc.egui_ctx.clone());
        }
        let shots = screenshots.map(|dir| {
            let (tx, rx) = mpsc::channel();
            let steps = Lang::ALL
                .iter()
                .flat_map(|&l| Page::ALL.map(|p| (l, p)))
                .collect();
            Screenshots {
                dir,
                steps,
                index: 0,
                frames: 0,
                requested: false,
                tx,
                rx,
            }
        });
        let mut app = App {
            shell,
            worker: Worker::spawn(cc.egui_ctx.clone(), bus),
            settings: Settings::load(),
            page: Page::Picture,
            name: None,
            caps: None,
            values: BTreeMap::new(),
            drafts: BTreeMap::new(),
            in_flight: VecDeque::new(),
            connected: false,
            message: None,
            demo: std::env::var("ODYSSEY_CTL_DEMO").is_ok_and(|v| v == "1"),
            shots,
            presets: Presets::load(),
            new_name: String::new(),
            confirm_delete: None,
        };
        app.load(true);
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

    fn reading(&self, name: &str) -> Option<VcpValue> {
        let f = features::find(name)?;
        self.values.get(&f.code)?.as_ref().ok().copied()
    }

    fn request(&mut self, r: Request) {
        let writes = matches!(
            r,
            Request::Write { .. } | Request::Sdm(_) | Request::ApplyPreset { .. }
        );
        self.in_flight.push_back(writes);
        self.worker.send(r);
    }

    /// Any request in progress (shown with a spinner, controls stay usable for reads).
    fn busy(&self) -> bool {
        !self.in_flight.is_empty()
    }

    /// A write in progress: controls are locked so writes never overlap.
    fn writing(&self) -> bool {
        self.in_flight.contains(&true)
    }

    /// Re-reads `names` in the background.
    fn reread(&mut self, names: &[&str]) {
        let list: Vec<_> = names.iter().filter_map(|n| features::find(n)).collect();
        if !list.is_empty() {
            self.request(Request::Read(list));
        }
    }

    /// Reads every feature, then the capability string (when missing, or always with `with_caps`).
    /// Values arrive first, so the page fills before the slower capability read.
    fn load(&mut self, with_caps: bool) {
        self.request(Request::Read(features::FEATURES.iter().collect()));
        if with_caps || self.caps.is_none() {
            self.request(Request::Caps);
        }
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
            self.in_flight.pop_front();
            match reply {
                Reply::Read(readings) => {
                    self.connected = true;
                    if readings.len() == features::FEATURES.len() {
                        self.message = Some(Message::Loaded(Instant::now()));
                    }
                    for r in readings {
                        self.drafts.remove(&r.feature.code);
                        self.values.insert(r.feature.code, r.value);
                    }
                }
                Reply::Caps { name, caps } => {
                    self.connected = true;
                    self.name = name;
                    self.caps = Some(caps);
                }
                Reply::Written { feature, result } => {
                    self.drafts.remove(&feature.code);
                    match result {
                        Ok(c) => {
                            if let Some(Ok(v)) = self.values.get_mut(&feature.code) {
                                v.current = c.after;
                            }
                            let text = format!(
                                "{}: {} → {}",
                                self.feature_label(feature),
                                self.value_text(feature, c.before),
                                self.value_text(feature, c.after)
                            );
                            self.message = Some(Message::Info(text, Instant::now()));
                            // Settings that change or lock others: re-read just those.
                            self.reread(features::dependents(feature.name));
                        }
                        Err(e) => {
                            self.message = Some(Message::Error(e));
                            // Show what the monitor really has.
                            self.reread(&[feature.name]);
                            self.reread(features::dependents(feature.name));
                        }
                    }
                }
                Reply::Sdm { eb, caps } => {
                    let model = caps.model.clone().unwrap_or_default();
                    self.message = Some(Message::Info(
                        format!("EB = {eb:#04x} · {model}"),
                        Instant::now(),
                    ));
                    self.caps = Some(caps);
                }
                Reply::PresetApplied { name, report } => {
                    self.message = Some(match report.failed.first() {
                        None => Message::Info(
                            self.t("preset_applied")
                                .replace("{name}", &name)
                                .replace("{n}", &report.changed.len().to_string()),
                            Instant::now(),
                        ),
                        Some((f, e)) => Message::Error(format!("{}: {e}", self.feature_label(f))),
                    });
                    let names: Vec<&str> = features::FEATURES.iter().map(|f| f.name).collect();
                    self.reread(&names);
                }
                Reply::Failed { message, lost } => {
                    if lost {
                        self.connected = false;
                    }
                    self.message = Some(Message::Error(message));
                }
            }
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(s) = self.shots.as_mut() else { return };
        ctx.request_repaint();
        if !self.in_flight.is_empty() {
            return; // wait for the monitor data
        }
        if let Ok(image) = s.rx.try_recv() {
            let (lang, page) = s.steps[s.index];
            let path = s.dir.join(format!(
                "{:02}_{}_{}.png",
                s.index,
                lang.code(),
                page.slug()
            ));
            let [w, h] = image.size;
            if let Err(e) = image::save_buffer(
                &path,
                image.as_raw(),
                w as u32,
                h as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("{}: {e}", path.display());
            }
            s.index += 1;
            s.frames = 0;
            s.requested = false;
        }
        let Some(&(lang, page)) = s.steps.get(s.index) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        self.settings.language = lang;
        self.page = page;
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

    // ---- frame -------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Odyssey Control")
                    .strong()
                    .size(18.0)
                    .color(Color32::WHITE),
            );
        });
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(dim(self
                .name
                .clone()
                .unwrap_or_else(|| "Samsung Odyssey".into())));
        });
        ui.add_space(14.0);
        for page in Page::ALL {
            let selected = self.page == page;
            let text = RichText::new(format!(
                "{}   {}",
                page.icon(),
                self.t(&format!("nav.{}", page.slug()))
            ))
            .size(15.0)
            .color(if selected { Color32::WHITE } else { TEXT_DIM });
            let button = egui::Button::new(text)
                .fill(if selected {
                    theme::ACCENT_SOFT
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(if selected {
                    Stroke::new(1.0, ACCENT)
                } else {
                    Stroke::NONE
                })
                .min_size(Vec2::new(ui.available_width(), 36.0));
            if ui.add(button).clicked() {
                self.page = page;
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(12.0);
            if let Some(model) = self.caps.as_ref().and_then(|c| c.model.clone()) {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    ui.label(dim(format!("{} {model}", self.t("row.profile"))));
                });
            }
            let (color, text) = if self.busy() && self.values.is_empty() {
                (WARN, self.t("busy"))
            } else if self.demo {
                (ACCENT, self.t("demo_mode"))
            } else if self.connected {
                (OK, self.t("connected"))
            } else {
                (ERR, self.t("disconnected"))
            };
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("●").color(color));
                ui.label(RichText::new(text).color(TEXT_DIM));
            });
        });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(self.t(&format!("nav.{}", self.page.slug())));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let refresh = egui::Button::new(format!("⟳ {}", self.t("refresh")));
                if ui.add_enabled(!self.busy(), refresh).clicked() {
                    self.load(false);
                }
                if self.busy() {
                    ui.spinner();
                }
            });
        });
        let mut clear = false;
        match &self.message {
            Some(Message::Loaded(at)) if at.elapsed() < Duration::from_secs(5) => {
                ui.label(RichText::new(self.t("loaded")).color(OK).small());
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
            Some(Message::Info(text, at)) if at.elapsed() < Duration::from_secs(5) => {
                ui.label(RichText::new(text).color(OK).small());
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
            Some(Message::Error(text)) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(text).color(ERR).small());
                    clear = ui.small_button("✕").clicked();
                });
            }
            _ => {
                ui.label(RichText::new(" ").small());
            }
        }
        if clear {
            self.message = None;
        }
        ui.add_space(4.0);
    }

    // ---- controls ----------------------------------------------------------

    /// The right control for a feature (chips, toggle, slider or list), with the lock and
    /// "disabled now" states. Writes go to the worker.
    fn control(&mut self, ui: &mut egui::Ui, name: &str) {
        let f = features::find(name).expect("pages use known features");
        let allowed = f.status.safe_to_write() || self.settings.allow_unverified;
        let enabled = allowed && !self.writing();
        let mut write = None;
        match self.values.get(&f.code).cloned() {
            None => {
                ui.label(dim(self.t("no_data")));
            }
            Some(Err(_)) => {
                ui.label(dim(self.t("not_supported")));
            }
            Some(Ok(v)) if features::is_disabled(v.current, v.max) => {
                ui.label(dim(self.t("disabled_now")));
            }
            Some(Ok(v)) => {
                ui.add_enabled_ui(enabled, |ui| write = self.editor(ui, f, v));
            }
        }
        if !allowed {
            ui.label(
                RichText::new(format!("⚠ {}", self.t("locked")))
                    .small()
                    .color(WARN),
            )
            .on_hover_text(self.t("locked_hint"));
        }
        if let Some(value) = write {
            let allow_unverified = self.settings.allow_unverified;
            self.request(Request::Write {
                feature: f,
                value,
                allow_unverified,
            });
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, f: &'static Feature, v: VcpValue) -> Option<u16> {
        // "Reset position" of the aim point is an action, not a state: not offered yet.
        let values: Vec<(u16, &str)> = f
            .values
            .iter()
            .copied()
            .filter(|&(x, n)| x <= v.max && n != "reset-position")
            .collect();
        let is_switch = values.len() == 2 && values[0].1 == "off" && values[1].1 == "on";
        if is_switch {
            let mut on = v.current & 1 == 1;
            return theme::toggle(ui, &mut on)
                .changed()
                .then_some(u16::from(on));
        }
        if !values.is_empty() && values.len() <= 10 {
            let mut chosen = None;
            ui.horizontal_wrapped(|ui| {
                for &(x, _) in &values {
                    if theme::chip(ui, v.current == x, &self.value_text(f, x)) && x != v.current {
                        chosen = Some(x);
                    }
                }
            });
            return chosen;
        }
        if !values.is_empty() {
            let mut selected = v.current;
            egui::ComboBox::from_id_salt(f.code)
                .width(220.0)
                .selected_text(self.value_text(f, v.current))
                .show_ui(ui, |ui| {
                    for &(x, _) in &values {
                        ui.selectable_value(&mut selected, x, self.value_text(f, x));
                    }
                });
            return (selected != v.current).then_some(selected);
        }
        if v.max > 0 && v.current <= v.max {
            let mut value = self.drafts.get(&f.code).copied().unwrap_or(v.current);
            let r = ui.add(egui::Slider::new(&mut value, 0..=v.max));
            if r.changed() {
                self.drafts.insert(f.code, value);
            }
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                if value != v.current {
                    return Some(value);
                }
                self.drafts.remove(&f.code);
            }
            return None;
        }
        ui.label(dim(format!("{} {:#06x}", self.t("raw_value"), v.current)));
        None
    }

    fn feature_row(&mut self, ui: &mut egui::Ui, name: &str) {
        let label = features::find(name)
            .map(|f| self.feature_label(f))
            .unwrap_or_default();
        theme::row(ui, &label, |ui| self.control(ui, name));
    }

    // ---- pages -------------------------------------------------------------

    fn current_values(&self) -> BTreeMap<u8, VcpValue> {
        self.values
            .iter()
            .filter_map(|(&c, v)| Some((c, *v.as_ref().ok()?)))
            .collect()
    }

    fn apply_preset(&mut self, i: usize) {
        let p = &self.presets.list[i];
        let (name, values) = (p.name.clone(), p.values.clone());
        let allow_unverified = self.settings.allow_unverified;
        self.confirm_delete = None;
        self.request(Request::ApplyPreset {
            name,
            values,
            allow_unverified,
        });
    }

    /// Presets chosen from the tray menu while the window is open.
    fn poll_tray(&mut self) {
        if self.writing() {
            return; // picked up on a later frame
        }
        let Some(name) = self
            .shell
            .preset_requests
            .lock()
            .ok()
            .and_then(|mut q| q.pop())
        else {
            return;
        };
        if let Some(i) = self.presets.list.iter().position(|p| p.name == name) {
            self.apply_preset(i);
        }
    }

    fn save_presets(&mut self) {
        if self.shots.is_none()
            && let Err(e) = self.presets.save()
        {
            self.message = Some(Message::Error(e.to_string()));
        }
    }

    /// One line describing a preset: its picture mode, if any, and how many settings it holds.
    fn preset_summary(&self, p: &Preset) -> String {
        let count = self
            .t("n_settings")
            .replace("{n}", &p.values.len().to_string());
        let mode = features::find("picture-mode")
            .zip(p.values.get("picture-mode"))
            .map(|(f, &v)| self.value_text(f, v));
        match mode {
            Some(m) => format!("{m} · {count}"),
            None => count,
        }
    }

    fn page_presets(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.save_preset"), self.t("card.save_preset.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            ui.horizontal(|ui| {
                let hint = self.t("preset_name");
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_name)
                        .hint_text(hint)
                        .desired_width(260.0),
                );
                let name = self.new_name.trim().to_string();
                let can = !name.is_empty() && !self.values.is_empty() && !self.writing();
                let save = egui::Button::new(RichText::new(self.t("save")).color(Color32::WHITE))
                    .fill(ACCENT);
                if ui.add_enabled(can, save).clicked() {
                    let allow = self.settings.allow_unverified;
                    let values = preset_logic::capture(&self.current_values(), |f| {
                        f.status.safe_to_write() || allow
                    });
                    self.presets.put(Preset {
                        name: name.clone(),
                        values,
                    });
                    self.save_presets();
                    self.new_name.clear();
                    let text = self.t("preset_saved").replace("{name}", &name);
                    self.message = Some(Message::Info(text, Instant::now()));
                }
            });
        });

        let (title, hint) = (self.t("card.presets"), self.t("card.presets.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            if self.presets.list.is_empty() {
                ui.label(dim(self.t("presets_empty")));
                return;
            }
            let current = self.current_values();
            let mut apply = None;
            let mut overwrite = None;
            let mut delete = None;
            for (i, p) in self.presets.list.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&p.name).strong().color(Color32::WHITE));
                            if preset_logic::matches(&p.values, &current) {
                                ui.label(
                                    RichText::new(format!("● {}", self.t("active")))
                                        .small()
                                        .color(OK),
                                );
                            }
                        });
                        ui.label(dim(self.preset_summary(p)));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let confirming = self.confirm_delete == Some(i);
                        let label = if confirming {
                            self.t("confirm_delete")
                        } else {
                            self.t("delete")
                        };
                        let text =
                            RichText::new(label).color(if confirming { ERR } else { TEXT_DIM });
                        if ui.button(text).clicked() {
                            delete = Some(i);
                        }
                        if ui
                            .add_enabled(!self.writing(), egui::Button::new(self.t("overwrite")))
                            .clicked()
                        {
                            overwrite = Some(i);
                        }
                        let apply_button =
                            egui::Button::new(RichText::new(self.t("apply")).color(Color32::WHITE))
                                .fill(ACCENT);
                        if ui
                            .add_enabled(!self.writing() && !self.values.is_empty(), apply_button)
                            .clicked()
                        {
                            apply = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = apply {
                self.apply_preset(i);
            }
            if let Some(i) = overwrite {
                let allow = self.settings.allow_unverified;
                self.presets.list[i].values =
                    preset_logic::capture(&current, |f| f.status.safe_to_write() || allow);
                self.confirm_delete = None;
                self.save_presets();
                let text = self
                    .t("preset_saved")
                    .replace("{name}", &self.presets.list[i].name);
                self.message = Some(Message::Info(text, Instant::now()));
            }
            if let Some(i) = delete {
                if self.confirm_delete == Some(i) {
                    self.presets.list.remove(i);
                    self.confirm_delete = None;
                    self.save_presets();
                } else {
                    self.confirm_delete = Some(i);
                }
            }
        });
    }

    fn page_picture(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.mode"), self.t("card.mode.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            self.control(ui, "picture-mode")
        });
        let title = self.t("card.basic");
        theme::card(ui, &title, None, |ui| {
            for name in ["brightness", "contrast", "sharpness"] {
                self.feature_row(ui, name);
            }
        });
        let (title, hint) = (self.t("card.color"), self.t("card.color.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            for name in ["color-tone", "red-gain", "green-gain", "blue-gain"] {
                self.feature_row(ui, name);
            }
        });
        let (title, hint) = (self.t("card.comfort"), self.t("card.comfort.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            self.feature_row(ui, "eye-saver")
        });
    }

    fn page_game(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.black_eq"), self.t("card.black_eq.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            let label = self.t("row.level");
            theme::row(ui, &label, |ui| self.control(ui, "black-equalizer"));
        });
        let (title, hint) = (self.t("card.aim"), self.t("card.aim.hint"));
        theme::card(ui, &title, Some(&hint), |ui| self.control(ui, "aim-point"));
        let (title, hint) = (self.t("card.sync"), self.t("card.sync.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            match self.reading("adaptive-sync") {
                Some(v) => {
                    let sync = features::decode_adaptive_sync(v.current);
                    let state = match sync.state {
                        SyncState::Off => self.t("sync.off"),
                        SyncState::On => self.t("sync.on"),
                        SyncState::Disabled => self.t("disabled_now"),
                    };
                    let kind = if sync.freesync {
                        "FreeSync Premium Pro"
                    } else {
                        "Adaptive-Sync"
                    };
                    theme::row(ui, &self.t("row.state"), |ui| {
                        ui.label(state);
                    });
                    theme::row(ui, &self.t("row.type"), |ui| {
                        ui.label(kind);
                    });
                }
                None => {
                    ui.label(dim(self.t("not_supported")));
                }
            }
        });
    }

    fn page_lighting(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.lighting"), self.t("card.lighting.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            for (key, name) in [
                ("row.lighting", "lighting"),
                ("row.effect", "lighting-effect"),
                ("row.colour", "lighting-color"),
            ] {
                let label = self.t(key);
                theme::row(ui, &label, |ui| self.control(ui, name));
            }
            ui.label(dim(self.t("row.colour.hint")));
        });
    }

    fn page_source(&mut self, ui: &mut egui::Ui) {
        let title = self.t("card.input");
        theme::card(ui, &title, None, |ui| {
            let label = self.t("row.input");
            theme::row(ui, &label, |ui| self.control(ui, "input"));
        });
        let title = self.t("card.audio");
        theme::card(ui, &title, None, |ui| {
            let label = self.t("row.volume");
            theme::row(ui, &label, |ui| self.control(ui, "volume"));
        });
        let (title, hint) = (self.t("card.pip"), self.t("card.pip.hint"));
        theme::card(ui, &title, Some(&hint), |ui| match self.reading("pip") {
            Some(v) => {
                let pip = features::decode_pip_status(v.current);
                let state = self.t(if pip.on { "sync.on" } else { "sync.off" });
                let supported = match (pip.pip_supported, pip.pbp_supported) {
                    (true, true) => "PIP · PBP",
                    (true, false) => "PIP",
                    (false, true) => "PBP",
                    (false, false) => "—",
                };
                theme::row(ui, &self.t("row.state"), |ui| {
                    ui.label(state);
                });
                theme::row(ui, &self.t("row.supported"), |ui| {
                    ui.label(supported);
                });
                if pip.on
                    && let Some(src) = self.reading("pip-source")
                {
                    let [main, sub] = src.current.to_be_bytes();
                    let text = format!(
                        "{} {} · {} {}",
                        self.t("row.main"),
                        features::source_name(main).unwrap_or_default(),
                        self.t("row.sub"),
                        features::source_name(sub).unwrap_or_default()
                    );
                    theme::row(ui, &self.t("row.sources"), |ui| {
                        ui.label(text);
                    });
                }
                let label = self.t("row.pip_sound");
                theme::row(ui, &label, |ui| self.control(ui, "pip-sound"));
            }
            None => {
                ui.label(dim(self.t("not_supported")));
            }
        });
        let title = self.t("card.osd");
        theme::card(ui, &title, None, |ui| {
            self.feature_row(ui, "language");
            self.feature_row(ui, "pc-av-mode");
        });
    }

    fn page_monitor(&mut self, ui: &mut egui::Ui) {
        let title = self.t("card.monitor");
        theme::card(ui, &title, None, |ui| {
            let name = self.name.clone().unwrap_or_else(|| "—".into());
            theme::row(ui, &self.t("row.name"), |ui| {
                ui.label(name);
            });
            if let Some(caps) = self.caps.clone() {
                let kind = match caps.profile() {
                    Profile::Lemon(_) => self.t("profile.lemon"),
                    Profile::Sec => self.t("profile.sec"),
                    Profile::Falcon => self.t("profile.falcon"),
                    Profile::Other(_) => String::new(),
                };
                theme::row(ui, &self.t("row.profile"), |ui| {
                    ui.label(caps.model.as_deref().unwrap_or("?"));
                    ui.label(dim(kind));
                });
                theme::row(ui, &self.t("row.mccs"), |ui| {
                    ui.label(caps.mccs_version.as_deref().unwrap_or("?"));
                });
                let codes: Vec<String> = caps.vcp.keys().map(|c| format!("{c:02X}")).collect();
                theme::row(ui, &self.t("row.codes"), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(codes.join(" ")).monospace().small()).wrap(),
                    );
                });
                egui::CollapsingHeader::new(self.t("caps_raw")).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&caps.raw).monospace().small()).wrap());
                });
            }
        });
        let (title, hint) = (self.t("card.sdm"), self.t("card.sdm.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.writing(), egui::Button::new(self.t("sdm_on")))
                    .clicked()
                {
                    self.request(Request::Sdm(true));
                }
                if ui
                    .add_enabled(!self.writing(), egui::Button::new(self.t("sdm_off")))
                    .clicked()
                {
                    self.request(Request::Sdm(false));
                }
            });
        });
    }

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        let title = self.t("card.app");
        theme::card(ui, &title, None, |ui| {
            let mut changed = false;
            theme::row(ui, &self.t("row.language"), |ui| {
                egui::ComboBox::from_id_salt("app-language")
                    .selected_text(self.settings.language.native_name())
                    .show_ui(ui, |ui| {
                        for lang in Lang::ALL {
                            changed |= ui
                                .selectable_value(
                                    &mut self.settings.language,
                                    lang,
                                    lang.native_name(),
                                )
                                .changed();
                        }
                    });
            });
            theme::row(ui, &self.t("row.unverified"), |ui| {
                changed |= theme::toggle(ui, &mut self.settings.allow_unverified).changed();
            });
            ui.label(dim(self.t("unverified_hint")));
            if changed {
                self.save_settings();
            }
        });
        let (title, hint) = (self.t("card.desktop"), self.t("card.desktop.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            let mut changed = false;
            theme::row(ui, &self.t("row.close_to_tray"), |ui| {
                changed |= theme::toggle(ui, &mut self.settings.close_to_tray).changed();
            });
            theme::row(ui, &self.t("row.start_hidden"), |ui| {
                changed |= theme::toggle(ui, &mut self.settings.start_hidden).changed();
            });
            let mut autostart_error = None;
            theme::row(ui, &self.t("row.autostart"), |ui| {
                if theme::toggle(ui, &mut self.settings.autostart).changed() {
                    changed = true;
                    if let Err(e) = desktop::set_autostart(self.settings.autostart) {
                        self.settings.autostart = !self.settings.autostart;
                        autostart_error = Some(e.to_string());
                    }
                }
            });
            if let Some(e) = autostart_error {
                self.message = Some(Message::Error(e));
            }
            if changed {
                self.save_settings();
            }
        });
        let (title, hint) = (self.t("card.about"), self.t("card.about.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            theme::row(ui, &self.t("row.version"), |ui| {
                ui.label(concat!("odyssey-ctl ", env!("CARGO_PKG_VERSION")));
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_worker();
        self.poll_tray();
        let ctx = ui.ctx().clone();
        self.screenshot_step(&ctx);
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(210.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| {
            self.top_bar(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| match self.page {
                    Page::Presets => self.page_presets(ui),
                    Page::Picture => self.page_picture(ui),
                    Page::Game => self.page_game(ui),
                    Page::Lighting => self.page_lighting(ui),
                    Page::Source => self.page_source(ui),
                    Page::Monitor => self.page_monitor(ui),
                    Page::Settings => self.page_settings(ui),
                });
        });
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // The window is gone: tray commands go to the main loop again.
        if let Ok(mut ctx) = self.shell.ctx.lock() {
            *ctx = None;
        }
    }
}
