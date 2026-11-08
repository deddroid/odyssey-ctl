#![cfg_attr(windows, windows_subsystem = "windows")]

//! Desktop GUI. Environment variables:
//! * `ODYSSEY_CTL_DEMO=1`: in-memory monitor instead of a real one;
//! * `ODYSSEY_CTL_SCREENSHOTS=<dir>`: save a PNG of every page in every language, then exit
//!   (no tray, no single-instance check);
//! * `ODYSSEY_CTL_CONFIG_DIR=<dir>`: where settings.toml and presets.toml live.

mod app;
mod desktop;
mod i18n;
mod settings;
mod theme;
mod worker;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};

use clap::Parser;
use eframe::egui;
use odyssey_ctl::{actions, presets as preset_logic};

use desktop::{Cmd, Labels, Shell};
use i18n::tr;
use settings::Settings;

#[derive(Parser)]
#[command(version, about = "Graphical control for Samsung Odyssey monitors")]
struct Args {
    /// I2C bus number (/dev/i2c-N); detected automatically when omitted.
    #[arg(long)]
    bus: Option<u32>,
    /// Start in the tray without opening the window (used by autostart).
    #[arg(long)]
    hidden: bool,
}

fn native_options() -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Odyssey Control")
        .with_app_id("odyssey-ctl")
        .with_inner_size([1000.0, 680.0])
        .with_min_inner_size([760.0, 480.0]);
    if let Some(icon) = desktop::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    eframe::NativeOptions {
        viewport,
        ..Default::default()
    }
}

/// Applies a preset chosen from the tray while no window is open.
fn apply_preset_headless(bus: Option<u32>, name: &str) {
    let Some(preset) = preset_logic::Presets::load()
        .list
        .into_iter()
        .find(|p| p.name == name)
    else {
        return;
    };
    match actions::open(bus) {
        Ok(mut opened) => {
            let allow = Settings::load().allow_unverified;
            let report = preset_logic::apply(opened.monitor.as_mut(), &preset.values, allow);
            for (f, e) in report.failed {
                eprintln!("preset {name}: {}: {e}", f.name);
            }
        }
        Err(e) => eprintln!("preset {name}: {e}"),
    }
}

/// Runs the GUI. With the tray enabled, closing the window keeps the program in the tray: the
/// window is destroyed and created again on "Show" (Wayland cannot hide a window, so this works
/// the same everywhere).
fn main() -> eframe::Result {
    let args = Args::parse();
    let screenshots = std::env::var_os("ODYSSEY_CTL_SCREENSHOTS").map(PathBuf::from);
    let settings = Settings::load();
    let lang = settings.language;

    let (tx, rx) = mpsc::channel::<Cmd>();
    if screenshots.is_none() && !desktop::claim_single_instance(tx.clone()) {
        return Ok(()); // the running instance shows its window
    }
    let tray =
        screenshots.is_none() && (settings.close_to_tray || settings.start_hidden || args.hidden);
    if tray {
        desktop::spawn_tray(
            tx.clone(),
            Labels {
                show: tr(lang, "tray.show"),
                quit: tr(lang, "tray.quit"),
                presets: tr(lang, "nav.presets"),
                tooltip: "Odyssey Control".into(),
            },
        );
    }

    // Commands arriving while the window is open act on it directly; the others wake the main
    // loop below.
    let shell = Arc::new(Shell::default());
    let (main_tx, main_rx) = mpsc::channel::<Cmd>();
    {
        let shell = shell.clone();
        std::thread::spawn(move || {
            for cmd in rx {
                let ctx = shell.ctx.lock().ok().and_then(|c| c.clone());
                match (cmd, ctx) {
                    (Cmd::Show, Some(ctx)) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    (Cmd::Quit, Some(ctx)) => {
                        shell.quit.store(true, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    (Cmd::Preset(name), Some(ctx)) => {
                        if let Ok(mut q) = shell.preset_requests.lock() {
                            q.push(name);
                        }
                        ctx.request_repaint();
                    }
                    (cmd, None) => {
                        if main_tx.send(cmd).is_err() {
                            return;
                        }
                    }
                }
            }
        });
    }

    let mut visible = !tray || !(args.hidden || settings.start_hidden);
    loop {
        if visible {
            let (sh, shots) = (shell.clone(), screenshots.clone());
            eframe::run_native(
                "odyssey-ctl",
                native_options(),
                Box::new(move |cc| Ok(Box::new(app::App::new(cc, sh, args.bus, shots)))),
            )?;
        }
        // The tray may have been switched off in the settings meanwhile.
        let keep = tray && Settings::load().close_to_tray;
        if shell.quit.load(Ordering::SeqCst) || !keep {
            return Ok(());
        }
        visible = false;
        match main_rx.recv() {
            Ok(Cmd::Show) => visible = true,
            Ok(Cmd::Preset(name)) => apply_preset_headless(args.bus, &name),
            Ok(Cmd::Quit) | Err(_) => return Ok(()),
        }
    }
}
