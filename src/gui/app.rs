use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use crate::desktop::{self, Shell};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Color32, ColorImage, Layout, RichText, Stroke, Vec2};
use odyssey_ctl::actions::DisplayInfo;
use odyssey_ctl::aim::{self, Border, Move, Position};
use odyssey_ctl::caps::{Capabilities, Profile};
use odyssey_ctl::ddc::VcpValue;
use odyssey_ctl::features::{self, Feature};
use odyssey_ctl::pip::{self, Channel, Source};
use odyssey_ctl::presets as preset_logic;

use crate::i18n::{Lang, tr};
use crate::presets::{AimSpot, Preset, Presets};
use crate::settings::{Settings, ThemeChoice};
use crate::theme::{self, dim, palette};
use crate::worker::{Action, Reply, Request, Worker};

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

/// `ODYSSEY_CTL_SCREENSHOTS=<dir>`: capture every page in every language and theme, then quit.
struct Screenshots {
    dir: PathBuf,
    steps: Vec<(egui::Theme, Lang, Page)>,
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
    /// Bus the worker opens (None = the first Samsung monitor).
    bus: Option<u32>,
    aim_position: Option<Position>,
    aim_border: Option<Border>,
    /// Coordinates being typed for the aim point.
    aim_target: Option<Position>,
    aim_steps: u16,
    /// Name typed for a saved crosshair position.
    aim_name: String,
    displays: Option<Vec<DisplayInfo>>,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shell: Arc<Shell>,
        bus: Option<u32>,
        screenshots: Option<PathBuf>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        let settings = Settings::load();
        cc.egui_ctx.set_theme(settings.theme.preference());
        if let Ok(mut ctx) = shell.ctx.lock() {
            *ctx = Some(cc.egui_ctx.clone());
        }
        let shots = screenshots.map(|dir| {
            let (tx, rx) = mpsc::channel();
            let steps = [egui::Theme::Dark, egui::Theme::Light]
                .into_iter()
                .flat_map(|t| Lang::ALL.into_iter().map(move |l| (t, l)))
                .flat_map(|(t, l)| Page::ALL.map(|p| (t, l, p)))
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
            settings,
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
            bus,
            aim_position: None,
            aim_border: None,
            aim_target: None,
            aim_steps: 10,
            aim_name: String::new(),
            displays: None,
        };
        app.load(true);
        app
    }

    /// An operation from `features::ACTIONS` may be sent.
    fn action_allowed(&self, name: &str) -> bool {
        features::action_status(name).is_some_and(|s| s.safe_to_write())
            || self.settings.allow_unverified
    }

    fn act(&mut self, action: Action) {
        let allow_unverified = self.settings.allow_unverified;
        self.request(Request::Act {
            action,
            allow_unverified,
        });
    }

    /// "to verify" marker shown next to a locked control.
    fn lock_label(&self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(format!("⚠ {}", self.t("locked")))
                .small()
                .color(palette(ui).warn),
        )
        .on_hover_text(self.t("locked_hint"));
    }

    /// Talks to another monitor: a new worker, and everything read so far is forgotten.
    fn switch_monitor(&mut self, ctx: &egui::Context, bus: u32) {
        self.bus = Some(bus);
        self.worker = Worker::spawn(ctx.clone(), Some(bus));
        self.in_flight.clear();
        self.values.clear();
        self.drafts.clear();
        self.caps = None;
        self.name = None;
        self.aim_position = None;
        self.aim_border = None;
        self.connected = false;
        self.load(true);
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
        self.in_flight.push_back(r.writes());
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
                            if feature.name == "aim-point" {
                                // A new icon may reset the position: read it again when shown.
                                self.aim_position = None;
                            }
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
                Reply::Aim { position, border } => {
                    if self.aim_position != Some(position) && self.aim_position.is_some() {
                        let text = self
                            .t("aim.at")
                            .replace("{x}", &position.x.to_string())
                            .replace("{y}", &position.y.to_string());
                        self.message = Some(Message::Info(text, Instant::now()));
                    }
                    self.aim_position = Some(position);
                    self.aim_target = None;
                    if border.is_some() {
                        self.aim_border = border;
                    } else {
                        // After a move: show at once if the monitor switched the icon off.
                        self.reread(&["aim-point"]);
                    }
                }
                Reply::Pip(status) => {
                    let state = self.t(if status.on { "sync.on" } else { "sync.off" });
                    self.message = Some(Message::Info(
                        self.t("pip.done").replace("{state}", &state),
                        Instant::now(),
                    ));
                    // PIP locks game and picture settings: read them again with the PIP state.
                    self.reread(&[
                        "pip",
                        "pip-source",
                        "pip-sound",
                        "adaptive-sync",
                        "black-equalizer",
                        "aim-point",
                        "picture-mode",
                        "contrast",
                        "sharpness",
                    ]);
                }
                Reply::PipSources(main, sub) => {
                    self.message = Some(Message::Info(
                        format!(
                            "{} {} · {} {}",
                            self.t("row.main"),
                            main.label(),
                            self.t("row.sub"),
                            sub.label()
                        ),
                        Instant::now(),
                    ));
                    self.reread(&["pip-source"]);
                }
                Reply::CoreSync(raw) => {
                    if let Some(Ok(v)) = self.values.get_mut(&0xF8) {
                        v.current = raw;
                    }
                    self.reread(&["lighting-effect", "lighting-color"]);
                }
                Reply::Displays(list) => self.displays = Some(list),
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
            let (theme, lang, page) = s.steps[s.index];
            let suffix = match theme {
                egui::Theme::Dark => "",
                egui::Theme::Light => "_light",
            };
            let path = s
                .dir
                .join(format!("{}_{}{suffix}.png", lang.code(), page.slug()));
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
        let Some(&(theme, lang, page)) = s.steps.get(s.index) else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        ctx.set_theme(theme);
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
        let p = palette(ui);
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Odyssey Control").font(theme::title_font(21.0)));
        });
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(dim(self
                .name
                .clone()
                .unwrap_or_else(|| "Samsung Odyssey".into())));
        });
        ui.add_space(14.0);
        ui.spacing_mut().item_spacing.y = 2.0;
        for page in Page::ALL {
            let selected = self.page == page;
            let text = RichText::new(format!(
                "{}   {}",
                page.icon(),
                self.t(&format!("nav.{}", page.slug()))
            ))
            .color(if selected { Color32::WHITE } else { p.text_dim });
            let button = egui::Button::new(text)
                .fill(if selected {
                    p.accent
                } else {
                    Color32::TRANSPARENT
                })
                .corner_radius(egui::CornerRadius::same(12))
                .min_size(Vec2::new(ui.available_width(), 36.0));
            if ui.add(button).clicked() {
                self.page = page;
            }
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(12.0);
            if let Some(model) = self.caps.as_ref().and_then(|c| c.model.clone()) {
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(dim(format!("{} {model}", self.t("row.profile"))));
                });
            }
            let (color, text) = if self.busy() && self.values.is_empty() {
                (p.warn, self.t("busy"))
            } else if self.demo {
                (p.accent, self.t("demo_mode"))
            } else if self.connected {
                (p.ok, self.t("connected"))
            } else {
                (p.err, self.t("disconnected"))
            };
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new("●").color(color));
                ui.label(RichText::new(text).color(p.text_dim));
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
        let p = palette(ui);
        let mut clear = false;
        match &self.message {
            Some(Message::Loaded(at)) if at.elapsed() < Duration::from_secs(5) => {
                ui.label(RichText::new(self.t("loaded")).color(p.ok).small());
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
            Some(Message::Info(text, at)) if at.elapsed() < Duration::from_secs(5) => {
                ui.label(RichText::new(text).color(p.ok).small());
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
            Some(Message::Error(text)) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(text).color(p.err).small());
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

    /// The right control for a feature (menu, toggle or slider), with the lock and
    /// "disabled now" states. Writes go to the worker.
    fn control(&mut self, ui: &mut egui::Ui, name: &str) {
        self.control_with(ui, name, Self::editor);
    }

    /// Like `control`, with a custom editor for the feature's value.
    fn control_with(
        &mut self,
        ui: &mut egui::Ui,
        name: &str,
        editor: fn(&mut Self, &mut egui::Ui, &'static Feature, VcpValue) -> Option<u16>,
    ) {
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
            Some(Ok(v)) if features::disabled(f, v.current, v.max) => {
                ui.label(dim(self.t("disabled_now")));
            }
            Some(Ok(v)) => {
                ui.add_enabled_ui(enabled, |ui| write = editor(self, ui, f, v));
            }
        }
        if !allowed {
            self.lock_label(ui);
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
            let mut on = features::state(f, v.current) == 1;
            return theme::toggle(ui, &mut on)
                .changed()
                .then_some(u16::from(on));
        }
        if !values.is_empty() {
            let options: Vec<(u16, String)> = values
                .iter()
                .map(|&(x, _)| (x, self.value_text(f, x)))
                .collect();
            return theme::choice(ui, f.code, v.current, &options);
        }
        if v.max > 0 && v.current <= v.max {
            let mut value = self.drafts.get(&f.code).copied().unwrap_or(v.current);
            let r = theme::slider(ui, &mut value, v.max);
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

    /// Aim point icon as "off / type A / type B"; a type switched on keeps the current colour.
    fn aim_type_editor(
        &mut self,
        ui: &mut egui::Ui,
        _f: &'static Feature,
        v: VcpValue,
    ) -> Option<u16> {
        let parts = aim::icon_parts(v.current);
        let options = [
            (0u16, self.t("v.off")),
            (1, self.t("v.type-a")),
            (2, self.t("v.type-b")),
        ];
        let selected = parts.map_or(0, |(kind, _)| kind + 1);
        let kind = theme::choice(ui, "aim-type", selected, &options)?;
        let colour = parts.map_or(0, |(_, c)| c);
        Some(if kind == 0 {
            0
        } else {
            aim::icon_value(kind - 1, colour)
        })
    }

    fn aim_colour_editor(
        &mut self,
        ui: &mut egui::Ui,
        _f: &'static Feature,
        v: VcpValue,
    ) -> Option<u16> {
        let (kind, colour) = aim::icon_parts(v.current)?;
        let options = [
            (0u16, self.t("v.light-blue")),
            (1, self.t("v.red")),
            (2, self.t("v.white")),
        ];
        let c = theme::choice(ui, "aim-colour", colour, &options)?;
        Some(aim::icon_value(kind, c))
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
        theme::group(ui, &title, Some(&hint), |ui| {
            let label = self.t("preset_name");
            theme::row(ui, &label, |ui| {
                let name = self.new_name.trim().to_string();
                let can = !name.is_empty() && !self.values.is_empty() && !self.writing();
                let save = theme::primary(ui, &self.t("save"));
                let clicked = ui.add_enabled(can, save).clicked();
                ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(240.0));
                if clicked {
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
        theme::group(ui, &title, Some(&hint), |ui| {
            if self.presets.list.is_empty() {
                let label = self.t("presets_empty");
                theme::row(ui, &label, |_| {});
                return;
            }
            let colors = palette(ui);
            let current = self.current_values();
            let mut apply = None;
            let mut overwrite = None;
            let mut delete = None;
            for (i, p) in self.presets.list.iter().enumerate() {
                let active = preset_logic::matches(&p.values, &current);
                theme::row(ui, &p.name, |ui| {
                    let apply_button = theme::primary(ui, &self.t("apply"));
                    if ui
                        .add_enabled(!self.writing() && !self.values.is_empty(), apply_button)
                        .clicked()
                    {
                        apply = Some(i);
                    }
                    if ui
                        .add_enabled(!self.writing(), egui::Button::new(self.t("overwrite")))
                        .clicked()
                    {
                        overwrite = Some(i);
                    }
                    let confirming = self.confirm_delete == Some(i);
                    let text = RichText::new(if confirming {
                        self.t("confirm_delete")
                    } else {
                        self.t("delete")
                    })
                    .color(if confirming {
                        colors.err
                    } else {
                        colors.text_dim
                    });
                    if ui.button(text).clicked() {
                        delete = Some(i);
                    }
                    ui.label(dim(self.preset_summary(p)));
                    if active {
                        ui.label(
                            RichText::new(format!("● {}", self.t("active")))
                                .small()
                                .color(colors.ok),
                        );
                    }
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
        theme::group(ui, &title, Some(&hint), |ui| {
            self.feature_row(ui, "picture-mode");
        });
        let title = self.t("card.basic");
        theme::group(ui, &title, None, |ui| {
            for name in ["brightness", "contrast", "sharpness"] {
                self.feature_row(ui, name);
            }
        });
        let (title, hint) = (self.t("card.color"), self.t("card.color.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            for name in ["color-tone", "red-gain", "green-gain", "blue-gain"] {
                self.feature_row(ui, name);
            }
        });
        let (title, hint) = (self.t("card.comfort"), self.t("card.comfort.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            self.feature_row(ui, "eye-saver");
        });
    }

    fn page_game(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.black_eq"), self.t("card.black_eq.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            let label = self.t("row.level");
            theme::row(ui, &label, |ui| self.control(ui, "black-equalizer"));
        });
        let (title, hint) = (self.t("card.aim"), self.t("card.aim.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            let label = self.t("aim.type");
            theme::row(ui, &label, |ui| {
                self.control_with(ui, "aim-point", Self::aim_type_editor)
            });
            let shown = self
                .reading("aim-point")
                .is_some_and(|v| aim::icon_parts(v.current).is_some());
            if shown {
                let label = self.t("row.colour");
                theme::row(ui, &label, |ui| {
                    self.control_with(ui, "aim-point", Self::aim_colour_editor)
                });
                self.aim_pad(ui);
            }
        });
        let (title, hint) = (self.t("card.sync"), self.t("card.sync.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            match self.reading("adaptive-sync") {
                Some(v) => {
                    let kind = if features::decode_adaptive_sync(v.current).freesync {
                        "FreeSync Premium Pro"
                    } else {
                        "Adaptive-Sync"
                    };
                    let label = self.t("row.state");
                    theme::row(ui, &label, |ui| self.control(ui, "adaptive-sync"));
                    let label = self.t("row.type");
                    theme::row(ui, &label, |ui| {
                        ui.label(kind);
                    });
                }
                None => {
                    let label = self.t("not_supported");
                    theme::row(ui, &label, |_| {});
                }
            }
        });
    }

    /// Screen outline to click on, arrows, coordinates, centre button and saved positions for
    /// the crosshair. Its position and border are read when first shown (stateful registers,
    /// so never with the other features).
    fn aim_pad(&mut self, ui: &mut egui::Ui) {
        if self.aim_position.is_none() && !self.busy() {
            self.request(Request::AimState);
        }
        let allowed = self.action_allowed("aim-move") && self.action_allowed("aim-position");
        let enabled = allowed && !self.writing();
        let label = self.t("aim.position");
        let Some(pos) = self.aim_position else {
            theme::row(ui, &label, |ui| {
                ui.label(dim(self.t("no_data")));
            });
            return;
        };
        let mut moved = None;
        let mut target = None;
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.add_enabled_ui(enabled, |ui| {
                if let Some(b) = self.aim_border {
                    if let Some(p) = screen_pad(ui, b, pos, self.aim_target) {
                        target = Some(p);
                    }
                    ui.add_space(16.0);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    // Arrows around the centre button, all the same size so they line up.
                    let cell = Vec2::new(72.0, 30.0);
                    let key = |ui: &mut egui::Ui, text: String| {
                        ui.add_sized(cell, egui::Button::new(text)).clicked()
                    };
                    let gap = |ui: &mut egui::Ui| {
                        ui.allocate_exact_size(cell, egui::Sense::hover());
                    };
                    egui::Grid::new("aim-dpad")
                        .spacing([6.0, 6.0])
                        .show(ui, |ui| {
                            gap(ui);
                            if key(ui, "▲".into()) {
                                moved = Some(Move::Up);
                            }
                            gap(ui);
                            ui.end_row();
                            if key(ui, "◀".into()) {
                                moved = Some(Move::Left);
                            }
                            if key(ui, self.t("aim.center"))
                                && let Some(b) = self.aim_border
                            {
                                target = Some(b.center());
                            }
                            if key(ui, "▶".into()) {
                                moved = Some(Move::Right);
                            }
                            ui.end_row();
                            gap(ui);
                            if key(ui, "▼".into()) {
                                moved = Some(Move::Down);
                            }
                            gap(ui);
                            ui.end_row();
                        });
                    ui.horizontal(|ui| {
                        ui.label(dim(self.t("aim.steps")));
                        let steps: Vec<(u16, String)> =
                            [1u16, 10, 50].map(|n| (n, n.to_string())).into();
                        if let Some(n) = theme::choice(ui, "aim-steps", self.aim_steps, &steps) {
                            self.aim_steps = n;
                        }
                    });
                    ui.horizontal(|ui| {
                        // Typed or dragged coordinates are written as soon as they settle,
                        // like a slider.
                        let mut t = self.aim_target.unwrap_or(pos);
                        ui.label(dim("X"));
                        let rx = ui.add(egui::DragValue::new(&mut t.x).range(0..=aim::MAX_COORD));
                        ui.label(dim("Y"));
                        let ry = ui.add(egui::DragValue::new(&mut t.y).range(0..=aim::MAX_COORD));
                        let dragging = rx.dragged() || ry.dragged();
                        let settled = rx.drag_stopped()
                            || ry.drag_stopped()
                            || ((rx.changed() || ry.changed()) && !dragging);
                        self.aim_target = (t != pos).then_some(t);
                        if settled && t != pos {
                            target = Some(t);
                        }
                    });
                    if let Some(b) = self.aim_border {
                        let text = self
                            .t("aim.area")
                            .replace("{x}", &b.x.to_string())
                            .replace("{y}", &b.y.to_string())
                            .replace("{w}", &b.width.to_string())
                            .replace("{h}", &b.height.to_string());
                        ui.label(dim(text));
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(dim(self.t("aim.pad_hint")));
            if !allowed {
                self.lock_label(ui);
            }
        });
        ui.add_space(8.0);
        let label = self.t("aim.saved");
        let mut save = false;
        let mut delete = None;
        theme::row(ui, &label, |ui| {
            let can = !self.aim_name.trim().is_empty();
            save = ui
                .add_enabled(can, egui::Button::new(self.t("aim.save")))
                .clicked();
            let hint = self.t("aim.name");
            ui.add(
                egui::TextEdit::singleline(&mut self.aim_name)
                    .hint_text(hint)
                    .desired_width(110.0),
            );
        });
        if !self.presets.aim.is_empty() {
            let colors = palette(ui);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                for (i, spot) in self.presets.aim.iter().enumerate() {
                    let here = Position {
                        x: spot.x,
                        y: spot.y,
                    };
                    let text = format!("{} ({}, {})", spot.name, spot.x, spot.y);
                    let selected = here == pos;
                    let button = egui::Button::new(RichText::new(text).color(if selected {
                        Color32::WHITE
                    } else {
                        colors.text
                    }))
                    .fill(if selected {
                        colors.accent
                    } else {
                        colors.track
                    });
                    if ui.add(button).clicked() && enabled && !selected {
                        target = Some(here);
                    }
                    if ui
                        .small_button("✕")
                        .on_hover_text(self.t("delete"))
                        .clicked()
                    {
                        delete = Some(i);
                    }
                    ui.add_space(6.0);
                }
            });
            ui.add_space(8.0);
        }
        if save {
            let name = self.aim_name.trim().to_string();
            self.presets.put_aim(AimSpot {
                name,
                x: pos.x,
                y: pos.y,
            });
            self.aim_name.clear();
            self.save_presets();
        }
        if let Some(i) = delete {
            self.presets.aim.remove(i);
            self.save_presets();
        }
        if let Some(dir) = moved {
            // Steps must be 500 ms apart or the crosshair hides, so arrows write the resulting
            // position instead (immediate, and read back). `aim_steps` is in coordinate units.
            match self.aim_border {
                Some(b) => target = Some(dir.shift(pos, self.aim_steps, b)),
                None => {
                    // Without the border only `EE` steps are possible (two units each).
                    let steps = self.aim_steps.div_ceil(aim::STEP_UNITS);
                    self.act(Action::AimMove(dir, steps));
                }
            }
        }
        if let Some(p) = target {
            self.act(Action::AimPosition(p));
        }
    }

    fn page_lighting(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.lighting"), self.t("card.lighting.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            for (key, name) in [
                ("row.lighting", "lighting"),
                ("row.effect", "lighting-effect"),
                ("row.colour", "lighting-color"),
            ] {
                let label = self.t(key);
                theme::row(ui, &label, |ui| self.control(ui, name));
            }
        });
        let (title, hint) = (self.t("row.coresync"), self.t("coresync.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            let label = self.t("row.state");
            let lighting = self.reading("lighting");
            let allowed = self.action_allowed("coresync");
            let mut change = None;
            theme::row(ui, &label, |ui| match lighting {
                Some(v) if v.current & 1 == 1 => {
                    let mut on = v.current & 2 != 0;
                    ui.add_enabled_ui(allowed && !self.writing(), |ui| {
                        if theme::toggle(ui, &mut on).changed() {
                            change = Some(on);
                        }
                    });
                    if !allowed {
                        self.lock_label(ui);
                    }
                }
                Some(_) => {
                    ui.label(dim(self.t("disabled_now")));
                }
                None => {
                    ui.label(dim(self.t("no_data")));
                }
            });
            if let Some(on) = change {
                self.act(Action::CoreSync(on));
            }
        });
    }

    /// PIP switch, window size and corner, the input of each window and the sound source.
    fn pip_controls(&mut self, ui: &mut egui::Ui, status: pip::Status) {
        let allowed = self.action_allowed("pip");
        let enabled = allowed && !self.writing();
        let mut cmd = None;
        let mut source = None;
        let label = self.t("row.state");
        theme::row(ui, &label, |ui| {
            let mut on = status.on;
            ui.add_enabled_ui(enabled && status.pip_supported, |ui| {
                if theme::toggle(ui, &mut on).changed() {
                    cmd = Some(if on {
                        pip::Command::On
                    } else {
                        pip::Command::Off
                    });
                }
            });
            if !allowed {
                self.lock_label(ui);
            }
        });
        if !status.pip_supported {
            let label = self.t("row.supported");
            theme::row(ui, &label, |ui| {
                ui.label(dim(self.t("not_supported")));
            });
        } else if status.on {
            let label = self.t("pip.size");
            let sizes: Vec<(u8, String)> = (0..status.pip_sizes)
                .map(|n| (n, (n + 1).to_string()))
                .collect();
            theme::row(ui, &label, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    if let Some(n) = theme::choice(ui, "pip-size", status.pip_size, &sizes) {
                        cmd = Some(pip::Command::PipSize(n));
                    }
                });
            });
            let label = self.t("pip.corner");
            let corners: Vec<(u8, String)> = (0..4u8)
                .map(|n| (n, self.t(&format!("pip.corner.{n}"))))
                .collect();
            theme::row(ui, &label, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    if let Some(n) = theme::choice(ui, "pip-corner", status.pip_position, &corners)
                    {
                        cmd = Some(pip::Command::PipPosition(n));
                    }
                });
            });
            // The monitor's own inputs, named as the PIP source register names them.
            let inputs: Vec<(Option<Source>, String)> = features::find("input")
                .map(|f| {
                    f.values
                        .iter()
                        .filter_map(|&(code, name)| {
                            Some((
                                Some(Source::from_input_code(code)?),
                                self.t(&format!("v.{name}")),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let current = self
                .reading("pip-source")
                .and_then(|v| pip::decode_sources(v.current));
            let allowed_src = self.action_allowed("pip-source");
            let windows = [
                ("pip.main", Channel::Main, current.map(|c| c.0)),
                ("pip.sub", Channel::Sub, current.map(|c| c.1)),
            ];
            for (key, channel, pick) in windows {
                let label = self.t(key);
                theme::row(ui, &label, |ui| {
                    ui.add_enabled_ui(allowed_src && !self.writing(), |ui| {
                        if let Some(Some(src)) = theme::choice(ui, key, pick, &inputs) {
                            source = Some((channel, src));
                        }
                    });
                    if !allowed_src {
                        self.lock_label(ui);
                    }
                });
            }
            let label = self.t("row.pip_sound");
            theme::row(ui, &label, |ui| self.control(ui, "pip-sound"));
        }
        if let Some(c) = cmd {
            self.act(Action::Pip(c));
        }
        if let Some((channel, src)) = source {
            self.act(Action::PipSource(channel, src));
        }
    }

    fn page_source(&mut self, ui: &mut egui::Ui) {
        let title = self.t("card.input");
        theme::group(ui, &title, None, |ui| {
            let label = self.t("row.input");
            theme::row(ui, &label, |ui| self.control(ui, "input"));
        });
        let title = self.t("card.audio");
        theme::group(ui, &title, None, |ui| {
            let label = self.t("row.volume");
            theme::row(ui, &label, |ui| self.control(ui, "volume"));
        });
        let (title, hint) = (self.t("card.pip"), self.t("card.pip.hint"));
        theme::group(ui, &title, Some(&hint), |ui| match self.reading("pip") {
            Some(v) => self.pip_controls(ui, pip::decode_status(v.current)),
            None => {
                let label = self.t("not_supported");
                theme::row(ui, &label, |_| {});
            }
        });
        let title = self.t("card.osd");
        theme::group(ui, &title, None, |ui| {
            self.feature_row(ui, "language");
            self.feature_row(ui, "pc-av-mode");
        });
    }

    fn page_monitor(&mut self, ui: &mut egui::Ui) {
        let title = self.t("card.monitor");
        theme::group(ui, &title, None, |ui| {
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
                    ui.label(dim(kind));
                    ui.label(caps.model.as_deref().unwrap_or("?"));
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
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    egui::CollapsingHeader::new(self.t("caps_raw")).show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&caps.raw).monospace().small()).wrap(),
                        );
                    });
                });
            }
        });
        if self.displays.is_none() && !self.busy() {
            self.request(Request::Discover);
        }
        let (title, hint) = (self.t("card.monitors"), self.t("card.monitors.hint"));
        let ctx = ui.ctx().clone();
        let mut pick = None;
        let mut rescan = false;
        theme::group(ui, &title, Some(&hint), |ui| {
            let colors = palette(ui);
            match self.displays.as_deref() {
                None => {
                    let label = self.t("no_data");
                    theme::row(ui, &label, |_| {});
                }
                Some([]) => {
                    let label = self.t("no_monitors");
                    theme::row(ui, &label, |_| {});
                }
                Some(list) => {
                    // Without --bus the worker opens the first Samsung monitor.
                    let current = self
                        .bus
                        .or_else(|| list.iter().find(|d| d.samsung).map(|d| d.bus));
                    for d in list {
                        theme::row(ui, &d.name, |ui| {
                            if current == Some(d.bus) {
                                ui.label(RichText::new(self.t("current")).small().color(colors.ok));
                            } else if ui
                                .add_enabled(!self.busy(), egui::Button::new(self.t("use_monitor")))
                                .clicked()
                            {
                                pick = Some(d.bus);
                            }
                            ui.label(dim(format!("/dev/i2c-{}", d.bus)));
                        });
                    }
                }
            }
            let label = self.t("rescan");
            theme::row(ui, "", |ui| {
                rescan = ui
                    .add_enabled(!self.busy(), egui::Button::new(label))
                    .clicked();
            });
        });
        if let Some(bus) = pick {
            self.switch_monitor(&ctx, bus);
        }
        if rescan {
            self.displays = None;
            self.request(Request::Discover);
        }
        let (title, hint) = (self.t("card.sdm"), self.t("card.sdm.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
            let label = self.t("row.profile");
            theme::row(ui, &label, |ui| {
                if ui
                    .add_enabled(!self.writing(), egui::Button::new(self.t("sdm_off")))
                    .clicked()
                {
                    self.request(Request::Sdm(false));
                }
                if ui
                    .add_enabled(!self.writing(), egui::Button::new(self.t("sdm_on")))
                    .clicked()
                {
                    self.request(Request::Sdm(true));
                }
            });
        });
    }

    fn page_settings(&mut self, ui: &mut egui::Ui) {
        let title = self.t("card.app");
        let hint = self.t("unverified_hint");
        theme::group(ui, &title, Some(&hint), |ui| {
            let mut changed = false;
            theme::row(ui, &self.t("row.language"), |ui| {
                let langs: Vec<(Lang, String)> =
                    Lang::ALL.map(|l| (l, l.native_name().to_string())).into();
                if let Some(l) = theme::choice(ui, "app-language", self.settings.language, &langs) {
                    self.settings.language = l;
                    changed = true;
                }
            });
            theme::row(ui, &self.t("row.theme"), |ui| {
                let themes: Vec<(ThemeChoice, String)> =
                    ThemeChoice::ALL.map(|t| (t, self.t(t.key()))).into();
                if let Some(t) = theme::choice(ui, "app-theme", self.settings.theme, &themes) {
                    self.settings.theme = t;
                    ui.ctx().set_theme(t.preference());
                    changed = true;
                }
            });
            theme::row(ui, &self.t("row.unverified"), |ui| {
                changed |= theme::toggle(ui, &mut self.settings.allow_unverified).changed();
            });
            if changed {
                self.save_settings();
            }
        });
        let (title, hint) = (self.t("card.desktop"), self.t("card.desktop.hint"));
        theme::group(ui, &title, Some(&hint), |ui| {
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
        theme::group(ui, &title, Some(&hint), |ui| {
            theme::row(ui, &self.t("row.version"), |ui| {
                ui.label(concat!("odyssey-ctl ", env!("CARGO_PKG_VERSION")));
            });
        });
    }
}

/// A 16:9 outline standing for the screen, with the crosshair's allowed area mapped onto it.
/// Draws the current position (and the one being typed) and returns the position clicked.
fn screen_pad(
    ui: &mut egui::Ui,
    border: Border,
    pos: Position,
    draft: Option<Position>,
) -> Option<Position> {
    let size = Vec2::new(256.0, 144.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let inner = rect.shrink(6.0);
    let to_pad = |p: Position| {
        let fx = (f32::from(p.x.saturating_sub(border.x)) / f32::from(border.width.max(1)))
            .clamp(0.0, 1.0);
        let fy = (f32::from(p.y.saturating_sub(border.y)) / f32::from(border.height.max(1)))
            .clamp(0.0, 1.0);
        egui::pos2(
            inner.left() + fx * inner.width(),
            inner.top() + fy * inner.height(),
        )
    };
    let p = palette(ui);
    let painter = ui.painter();
    let outline = if ui.is_enabled() {
        p.text_dim
    } else {
        p.text_dim.gamma_multiply(0.5)
    };
    painter.rect(
        rect,
        12.0,
        p.track,
        Stroke::new(1.0, outline),
        egui::StrokeKind::Inside,
    );
    // Stand for the monitor.
    painter.line_segment(
        [
            egui::pos2(rect.center().x, rect.bottom()),
            egui::pos2(rect.center().x, rect.bottom() + 4.0),
        ],
        Stroke::new(2.0, outline),
    );
    if let Some(d) = draft {
        painter.circle_stroke(to_pad(d), 5.0, Stroke::new(1.0, p.warn));
    }
    let c = to_pad(pos);
    let arm = Vec2::new(11.0, 0.0);
    painter.line_segment([c - arm, c + arm], Stroke::new(1.5, p.accent));
    painter.line_segment([c - arm.yx(), c + arm.yx()], Stroke::new(1.5, p.accent));
    painter.circle_stroke(c, 6.0, Stroke::new(2.0, p.accent));
    if response.hovered() && ui.is_enabled() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if !response.clicked() {
        return None;
    }
    let p = response.interact_pointer_pos()?;
    let fx = ((p.x - inner.left()) / inner.width()).clamp(0.0, 1.0);
    let fy = ((p.y - inner.top()) / inner.height()).clamp(0.0, 1.0);
    Some(Position {
        x: border.x + (fx * f32::from(border.width)).round() as u16,
        y: border.y + (fy * f32::from(border.height)).round() as u16,
    })
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_worker();
        self.poll_tray();
        let ctx = ui.ctx().clone();
        self.screenshot_step(&ctx);
        let p = palette(ui);
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(210.0)
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ui, |ui| self.sidebar(ui));
        let frame = egui::Frame::new()
            .fill(p.surface)
            .inner_margin(egui::Margin::symmetric(20, 16));
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
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
