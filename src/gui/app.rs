use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Color32, ColorImage, Layout, RichText, Stroke, Vec2};
use odyssey_ctl::caps::{Capabilities, Profile};
use odyssey_ctl::ddc::VcpValue;
use odyssey_ctl::features::{self, Feature, SyncState};

use crate::i18n::{Lang, tr};
use crate::settings::Settings;
use crate::theme::{self, ACCENT, ERR, OK, TEXT_DIM, WARN, dim};
use crate::worker::{Reply, Request, Worker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Picture,
    Game,
    Lighting,
    Source,
    Monitor,
    Settings,
}

impl Page {
    const ALL: [Page; 6] = [Page::Picture, Page::Game, Page::Lighting, Page::Source, Page::Monitor, Page::Settings];

    fn slug(self) -> &'static str {
        match self {
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
    worker: Worker,
    settings: Settings,
    page: Page,
    name: Option<String>,
    caps: Option<Capabilities>,
    values: BTreeMap<u8, Result<VcpValue, String>>,
    /// Slider values being dragged, sent to the monitor on release.
    drafts: BTreeMap<u8, u16>,
    pending: usize,
    connected: bool,
    message: Option<Message>,
    demo: bool,
    shots: Option<Screenshots>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, bus: Option<u32>, screenshots: Option<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let shots = screenshots.map(|dir| {
            let (tx, rx) = mpsc::channel();
            let steps = Lang::ALL.iter().flat_map(|&l| Page::ALL.map(|p| (l, p))).collect();
            Screenshots { dir, steps, index: 0, frames: 0, requested: false, tx, rx }
        });
        let mut app = App {
            worker: Worker::spawn(cc.egui_ctx.clone(), bus),
            settings: Settings::load(),
            page: Page::Picture,
            name: None,
            caps: None,
            values: BTreeMap::new(),
            drafts: BTreeMap::new(),
            pending: 0,
            connected: false,
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

    fn reading(&self, name: &str) -> Option<VcpValue> {
        let f = features::find(name)?;
        self.values.get(&f.code)?.as_ref().ok().copied()
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
                Reply::Loaded { name, caps, readings } => {
                    self.connected = true;
                    self.name = name;
                    self.values = readings.into_iter().map(|r| (r.feature.code, r.value)).collect();
                    self.drafts.clear();
                    match caps {
                        Ok(c) => {
                            self.caps = Some(c);
                            self.message = Some(Message::Loaded(Instant::now()));
                        }
                        Err(e) => self.message = Some(Message::Error(e)),
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
                                "{}: {} → {}",
                                self.feature_label(feature),
                                self.value_text(feature, c.before),
                                self.value_text(feature, c.after)
                            );
                            self.message = Some(Message::Info(text, Instant::now()));
                            // These change other settings or lock/unlock them: re-read everything.
                            const AFFECTS_OTHERS: [&str; 5] =
                                ["picture-mode", "eye-saver", "lighting", "lighting-effect", "pc-av-mode"];
                            if AFFECTS_OTHERS.contains(&feature.name) {
                                self.request(Request::Load);
                            }
                        }
                        Err(e) => {
                            self.message = Some(Message::Error(e));
                            self.request(Request::Load); // show what the monitor really has
                        }
                    }
                }
                Reply::Sdm(Ok((eb, caps))) => {
                    let model = caps.model.clone().unwrap_or_default();
                    self.message = Some(Message::Info(format!("EB = {eb:#04x} · {model}"), Instant::now()));
                    self.caps = Some(caps);
                }
                Reply::Sdm(Err(e)) => self.message = Some(Message::Error(e)),
                Reply::Failed(e) => {
                    self.connected = false;
                    self.message = Some(Message::Error(e));
                }
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
            let (lang, page) = s.steps[s.index];
            let path = s.dir.join(format!("{:02}_{}_{}.png", s.index, lang.code(), page.slug()));
            let [w, h] = image.size;
            if let Err(e) = image::save_buffer(&path, image.as_raw(), w as u32, h as u32, image::ColorType::Rgba8) {
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
            ui.label(RichText::new("Odyssey Control").strong().size(18.0).color(Color32::WHITE));
        });
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.label(dim(self.name.clone().unwrap_or_else(|| "Samsung Odyssey".into())));
        });
        ui.add_space(14.0);
        for page in Page::ALL {
            let selected = self.page == page;
            let text = RichText::new(format!("{}   {}", page.icon(), self.t(&format!("nav.{}", page.slug()))))
                .size(15.0)
                .color(if selected { Color32::WHITE } else { TEXT_DIM });
            let button = egui::Button::new(text)
                .fill(if selected { theme::ACCENT_SOFT } else { Color32::TRANSPARENT })
                .stroke(if selected { Stroke::new(1.0, ACCENT) } else { Stroke::NONE })
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
            let (color, text) = if self.pending > 0 {
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
                if ui.add_enabled(self.pending == 0, refresh).clicked() {
                    self.request(Request::Load);
                }
                if self.pending > 0 {
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
        let enabled = allowed && self.pending == 0;
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
            ui.label(RichText::new(format!("⚠ {}", self.t("locked"))).small().color(WARN))
                .on_hover_text(self.t("locked_hint"));
        }
        if let Some(value) = write {
            let allow_unverified = self.settings.allow_unverified;
            self.request(Request::Write { feature: f, value, allow_unverified });
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, f: &'static Feature, v: VcpValue) -> Option<u16> {
        // "Reset position" of the aim point is an action, not a state: not offered yet.
        let values: Vec<(u16, &str)> =
            f.values.iter().copied().filter(|&(x, n)| x <= v.max && n != "reset-position").collect();
        let is_switch = values.len() == 2 && values[0].1 == "off" && values[1].1 == "on";
        if is_switch {
            let mut on = v.current & 1 == 1;
            return theme::toggle(ui, &mut on).changed().then_some(u16::from(on));
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
        let label = features::find(name).map(|f| self.feature_label(f)).unwrap_or_default();
        theme::row(ui, &label, |ui| self.control(ui, name));
    }

    // ---- pages -------------------------------------------------------------

    fn page_picture(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.mode"), self.t("card.mode.hint"));
        theme::card(ui, &title, Some(&hint), |ui| self.control(ui, "picture-mode"));
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
        theme::card(ui, &title, Some(&hint), |ui| self.feature_row(ui, "eye-saver"));
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
        theme::card(ui, &title, Some(&hint), |ui| match self.reading("adaptive-sync") {
            Some(v) => {
                let sync = features::decode_adaptive_sync(v.current);
                let state = match sync.state {
                    SyncState::Off => self.t("sync.off"),
                    SyncState::On => self.t("sync.on"),
                    SyncState::Disabled => self.t("disabled_now"),
                };
                let kind = if sync.freesync { "FreeSync Premium Pro" } else { "Adaptive-Sync" };
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
        });
    }

    fn page_lighting(&mut self, ui: &mut egui::Ui) {
        let (title, hint) = (self.t("card.lighting"), self.t("card.lighting.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            for (key, name) in [("row.lighting", "lighting"), ("row.effect", "lighting-effect"), ("row.colour", "lighting-color")] {
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
                    ui.add(egui::Label::new(RichText::new(codes.join(" ")).monospace().small()).wrap());
                });
                egui::CollapsingHeader::new(self.t("caps_raw")).show(ui, |ui| {
                    ui.add(egui::Label::new(RichText::new(&caps.raw).monospace().small()).wrap());
                });
            }
        });
        let (title, hint) = (self.t("card.sdm"), self.t("card.sdm.hint"));
        theme::card(ui, &title, Some(&hint), |ui| {
            ui.horizontal(|ui| {
                if ui.add_enabled(self.pending == 0, egui::Button::new(self.t("sdm_on"))).clicked() {
                    self.request(Request::Sdm(true));
                }
                if ui.add_enabled(self.pending == 0, egui::Button::new(self.t("sdm_off"))).clicked() {
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
                            changed |= ui.selectable_value(&mut self.settings.language, lang, lang.native_name()).changed();
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
        let ctx = ui.ctx().clone();
        self.screenshot_step(&ctx);
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(210.0)
            .frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::same(10)))
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default_margins().show(ui, |ui| {
            self.top_bar(ui);
            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| match self.page {
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
