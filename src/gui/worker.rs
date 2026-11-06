//! Background thread that owns the monitor connection, so the UI never waits on DDC/CI.
//! It only talks to the monitor when asked: no polling (polling closes the monitor's OSD).
//! Requests are answered in order, one reply each.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use odyssey_ctl::actions::{self, Change, Monitor, Reading};
use odyssey_ctl::caps::Capabilities;
use odyssey_ctl::device::Error;
use odyssey_ctl::features::Feature;
use odyssey_ctl::presets;

pub enum Request {
    Read(Vec<&'static Feature>),
    Caps,
    Write {
        feature: &'static Feature,
        value: u16,
        allow_unverified: bool,
    },
    Sdm(bool),
    ApplyPreset {
        name: String,
        values: presets::Values,
        allow_unverified: bool,
    },
}

pub enum Reply {
    Read(Vec<Reading>),
    /// Capabilities plus the monitor name from the EDID.
    Caps {
        name: Option<String>,
        caps: Capabilities,
    },
    Written {
        feature: &'static Feature,
        result: Result<Change, String>,
    },
    Sdm {
        eb: u16,
        caps: Capabilities,
    },
    PresetApplied {
        name: String,
        report: presets::Report,
    },
    /// The request failed. `lost` means the monitor itself did not answer (it is reopened next time).
    Failed {
        message: String,
        lost: bool,
    },
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
            let mut connection: Option<actions::Opened> = None;
            for request in requests {
                let reply = match connection.take().map_or_else(|| actions::open(bus), Ok) {
                    Ok(mut c) => {
                        let reply = handle(&mut c, request);
                        if !matches!(reply, Reply::Failed { lost: true, .. }) {
                            connection = Some(c);
                        }
                        reply
                    }
                    Err(e) => Reply::Failed {
                        message: e.to_string(),
                        lost: true,
                    },
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

fn handle(c: &mut actions::Opened, request: Request) -> Reply {
    let m: &mut Monitor = &mut c.monitor;
    let result = match request {
        Request::Read(list) => actions::read(m.as_mut(), &list).map(Reply::Read),
        Request::Caps => m.capabilities().map(|s| Reply::Caps {
            name: c.name.clone(),
            caps: Capabilities::parse(&s),
        }),
        Request::Write {
            feature,
            value,
            allow_unverified,
        } => Ok(Reply::Written {
            feature,
            result: actions::write(m.as_mut(), feature, value, allow_unverified)
                .map_err(|e| e.to_string()),
        }),
        Request::Sdm(on) => {
            actions::set_sdm(m.as_mut(), on).map(|(eb, caps)| Reply::Sdm { eb, caps })
        }
        Request::ApplyPreset {
            name,
            values,
            allow_unverified,
        } => Ok(Reply::PresetApplied {
            name,
            report: presets::apply(m.as_mut(), &values, allow_unverified),
        }),
    };
    result.unwrap_or_else(|e| Reply::Failed {
        message: e.to_string(),
        lost: matches!(e, Error::Io(_)),
    })
}
