//! Background thread that owns the monitor connection, so the UI never waits on DDC/CI.
//! It only talks to the monitor when asked: no polling (polling closes the monitor's OSD).
//! Requests are answered in order, one reply each.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use eframe::egui;
use odyssey_ctl::actions::{self, Change, DisplayInfo, Monitor, Reading};
use odyssey_ctl::aim::{Border, Move, Position};
use odyssey_ctl::caps::Capabilities;
use odyssey_ctl::device::Error;
use odyssey_ctl::features::Feature;
use odyssey_ctl::pip::{self, Channel, Source};
use odyssey_ctl::presets;

/// Operations that are not a single feature value (gated like unverified features).
pub enum Action {
    AimMove(Move, u16),
    AimPosition(Position),
    Pip(pip::Command),
    PipSource(Channel, Source),
    CoreSync(bool),
}

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
    /// Position and border of the aim point (stateful reads, done in full cycles).
    AimState,
    Act {
        action: Action,
        allow_unverified: bool,
    },
    /// Monitors on this machine (no connection needed).
    Discover,
}

impl Request {
    /// True for requests that change the monitor: the UI locks its controls meanwhile.
    pub fn writes(&self) -> bool {
        matches!(
            self,
            Request::Write { .. }
                | Request::Sdm(_)
                | Request::ApplyPreset { .. }
                | Request::Act { .. }
        )
    }
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
    Aim {
        position: Position,
        border: Option<Border>,
    },
    Pip(pip::Status),
    PipSources(Source, Source),
    /// Lighting reading after a CoreSync change: bit 0 lighting, bit 1 CoreSync.
    CoreSync(u16),
    Displays(Vec<DisplayInfo>),
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
                if matches!(request, Request::Discover) {
                    if replies.send(Reply::Displays(actions::discover())).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                    continue;
                }
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
    let m = m.as_mut();
    let result = match request {
        Request::Read(list) => actions::read(m, &list).map(Reply::Read),
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
            result: actions::write(m, feature, value, allow_unverified).map_err(|e| e.to_string()),
        }),
        Request::Sdm(on) => actions::set_sdm(m, on).map(|(eb, caps)| Reply::Sdm { eb, caps }),
        Request::ApplyPreset {
            name,
            values,
            allow_unverified,
        } => Ok(Reply::PresetApplied {
            name,
            report: presets::apply(m, &values, allow_unverified),
        }),
        Request::AimState => actions::aim_state(m).map(|(position, border)| Reply::Aim {
            position,
            border: Some(border),
        }),
        Request::Act {
            action,
            allow_unverified: allow,
        } => match action {
            Action::AimMove(dir, steps) => {
                actions::aim_move(m, dir, steps, allow).map(|position| Reply::Aim {
                    position,
                    border: None,
                })
            }
            Action::AimPosition(p) => {
                actions::aim_position(m, p, allow).map(|position| Reply::Aim {
                    position,
                    border: None,
                })
            }
            Action::Pip(cmd) => actions::pip_command(m, cmd, allow).map(Reply::Pip),
            Action::PipSource(channel, src) => actions::pip_source(m, channel, src, allow)
                .map(|(main, sub)| Reply::PipSources(main, sub)),
            Action::CoreSync(on) => actions::set_coresync(m, on, allow).map(Reply::CoreSync),
        },
        Request::Discover => unreachable!("answered without a connection"),
    };
    result.unwrap_or_else(|e| Reply::Failed {
        message: e.to_string(),
        lost: matches!(e, Error::Io(_)),
    })
}
