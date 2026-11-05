#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod i18n;
mod settings;
mod theme;
mod worker;

use std::path::PathBuf;

use clap::Parser;
use eframe::egui;

#[derive(Parser)]
#[command(version, about = "Graphical control for Samsung Odyssey monitors")]
struct Args {
    /// I2C bus number (/dev/i2c-N); detected automatically when omitted.
    #[arg(long)]
    bus: Option<u32>,
}

fn main() -> eframe::Result {
    let args = Args::parse();
    let screenshots = std::env::var_os("ODYSSEY_CTL_SCREENSHOTS").map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Odyssey Control")
            .with_app_id("odyssey-ctl")
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "odyssey-ctl",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, args.bus, screenshots)))),
    )
}
