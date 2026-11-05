//! Background thread that owns the monitor connection, so the UI never waits on DDC/CI.
//! It only talks to the monitor when asked: no polling (polling closes the monitor's OSD).

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use odyssey_ctl::actions::{self, Change, Monitor, Reading};
use odyssey_ctl::caps::Capabilities;
use odyssey_ctl::features::{FEATURES, Feature};

pub enum Request {
    /// Read the capability string and every known feature.
    Load,
    Write { feature: &'static Feature, value: u16, allow_unverified: bool },
    Sdm(bool),
}

pub enum Reply {
    Loaded { name: Option<String>, caps: Result<Capabilities, String>, readings: Vec<Reading> },
    Written { feature: &'static Feature, result: Result<Change, String> },
    Sdm(Result<(u16, Capabilities), String>),
    Failed(String),
}

pub struct Worker {
    tx: Sender<Request>,
    rx: Receiver<Reply>,
}

impl Worker {
    pub fn spawn(ctx: egui::Context, bus: Option<u32>) -> Self {
        let (tx, requests) = mpsc::channel::<Request>();
        let (replies, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut monitor: Option<Monitor> = None;
            let mut name = None;
            for request in requests {
                let opened = match monitor.take() {
                    Some(m) => Ok(m),
                    None => actions::open(bus).map(|o| {
                        name = o.name;
                        o.monitor
                    }),
                };
                let reply = match opened {
                    Ok(mut m) => {
                        let reply = handle(&mut m, request, &name);
                        // Keep the connection unless it failed; it is reopened on the next request.
                        if !matches!(reply, Reply::Failed(_)) {
                            monitor = Some(m);
                        }
                        reply
                    }
                    Err(e) => Reply::Failed(e.to_string()),
                };
                if replies.send(reply).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Worker { tx, rx }
    }

    pub fn send(&self, request: Request) {
        // The worker only stops when the UI is gone, so a failed send can be ignored.
        let _ = self.tx.send(request);
    }

    pub fn try_recv(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }
}

fn handle(m: &mut Monitor, request: Request, name: &Option<String>) -> Reply {
    match request {
        Request::Load => {
            let caps = m.capabilities().map(|s| Capabilities::parse(&s)).map_err(|e| e.to_string());
            let readings = actions::read(m.as_mut(), &FEATURES.iter().collect::<Vec<_>>());
            if caps.is_err() && readings.iter().all(|r| r.value.is_err()) {
                return Reply::Failed(caps.err().unwrap_or_default());
            }
            Reply::Loaded { name: name.clone(), caps, readings }
        }
        Request::Write { feature, value, allow_unverified } => Reply::Written {
            feature,
            result: actions::write(m.as_mut(), feature, value, allow_unverified).map_err(|e| e.to_string()),
        },
        Request::Sdm(on) => Reply::Sdm(actions::set_sdm(m.as_mut(), on).map_err(|e| e.to_string())),
    }
}
