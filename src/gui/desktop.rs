//! Desktop integration: tray icon, single instance and autostart.
//!
//! * Linux: tray via `ksni` (StatusNotifierItem, tokio backend), autostart via an XDG `.desktop`
//!   file in `~/.config/autostart`.
//! * Windows: tray via `tray-icon` on its own thread with a Win32 message loop, autostart via
//!   `HKCU\…\Run`.
//! * Both: a second launch only asks the running instance to show itself (local socket).

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;

use eframe::egui;

use odyssey_ctl::presets::Presets;

/// Window icon (embedded PNG).
pub const ICON_PNG: &[u8] = include_bytes!("../../assets/icon-256.png");
/// Tray icon (embedded PNG).
const TRAY_PNG: &[u8] = include_bytes!("../../assets/icon-64.png");

/// Commands coming from the tray, from a second instance or from the window manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    Show,
    Quit,
    /// Apply the preset with this name.
    Preset(String),
    /// A window got the focus (class and title), reported by the KWin script.
    #[cfg(target_os = "linux")]
    Window {
        class: String,
        title: String,
    },
    /// The application windows open right now (class and title), one entry per class.
    #[cfg(target_os = "linux")]
    Windows(Vec<(String, String)>),
}

/// Menu labels in the user's language.
#[derive(Clone)]
pub struct Labels {
    pub show: String,
    pub quit: String,
    pub presets: String,
    pub tooltip: String,
}

/// State shared between the window, the tray and the single-instance listener.
#[derive(Default)]
pub struct Shell {
    /// Context of the open window, if any.
    pub ctx: Mutex<Option<egui::Context>>,
    /// Quit requested from the tray.
    pub quit: AtomicBool,
    /// Presets chosen from the tray while the window is open; the window applies them.
    pub preset_requests: Mutex<Vec<String>>,
    /// Class and title of the last application window that got the focus (not this one).
    #[cfg(target_os = "linux")]
    pub last_window: Mutex<Option<(String, String)>>,
    /// Application windows open right now, one per class, for the rule picker.
    #[cfg(target_os = "linux")]
    pub open_windows: Mutex<Vec<(String, String)>>,
}

/// D-Bus name, object path and interface of the window service the KWin script calls.
#[cfg(target_os = "linux")]
pub const WINDOW_SERVICE: &str = "io.github.deddroid.OdysseyCtl";
#[cfg(target_os = "linux")]
pub const WINDOW_PATH: &str = "/io/github/deddroid/OdysseyCtl";
/// Window class of this application, as the compositor reports it.
#[cfg(target_os = "linux")]
pub const OWN_WINDOW_CLASS: &str = "odyssey-ctl";

/// RGBA pixels of an embedded PNG.
pub fn decode_png(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some((img.into_raw(), w, h))
}

pub fn window_icon() -> Option<egui::IconData> {
    decode_png(ICON_PNG).map(|(rgba, width, height)| egui::IconData {
        rgba,
        width,
        height,
    })
}

// ---- single instance --------------------------------------------------------

const SOCKET_NAME: &str = "odyssey-ctl-gui.sock";

/// Returns `false` if another instance is running (it has been asked to show its window);
/// otherwise starts listening for later launches.
pub fn claim_single_instance(tx: Sender<Cmd>) -> bool {
    use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream, prelude::*};
    use std::io::{BufRead, BufReader, Write};

    let Ok(name) = SOCKET_NAME.to_ns_name::<GenericNamespaced>() else {
        return true;
    };
    if let Ok(mut conn) = Stream::connect(name.clone()) {
        let _ = conn.write_all(b"show\n");
        return false;
    }
    let Ok(listener) = ListenerOptions::new().name(name).create_sync() else {
        // Could not listen (unusual): run anyway rather than refuse to start.
        return true;
    };
    std::thread::Builder::new()
        .name("odyssey-instance".into())
        .spawn(move || {
            for conn in listener.incoming().flatten() {
                let mut line = String::new();
                let _ = BufReader::new(conn).read_line(&mut line);
                if line.trim() == "show" && tx.send(Cmd::Show).is_err() {
                    return;
                }
            }
        })
        .ok();
    true
}

// ---- tray: Linux -------------------------------------------------------------

#[cfg(target_os = "linux")]
pub fn spawn_tray(tx: Sender<Cmd>, labels: Labels) {
    use ksni::TrayMethods;
    use ksni::menu::{StandardItem, SubMenu};

    struct Tray {
        tx: Sender<Cmd>,
        labels: Labels,
        icon: Vec<ksni::Icon>,
        preset_names: Vec<String>,
    }

    impl ksni::Tray for Tray {
        fn id(&self) -> String {
            "odyssey-ctl".into()
        }
        fn title(&self) -> String {
            self.labels.tooltip.clone()
        }
        fn icon_name(&self) -> String {
            "odyssey-ctl".into()
        }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.clone()
        }
        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip {
                title: self.labels.tooltip.clone(),
                ..Default::default()
            }
        }
        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.tx.send(Cmd::Show);
        }
        fn menu_about_to_show(&mut self) {
            // Presets may have been edited in the window since the last time.
            self.preset_names = Presets::load().list.into_iter().map(|p| p.name).collect();
        }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            let item = |label: String, cmd: Cmd| -> ksni::MenuItem<Self> {
                StandardItem {
                    label,
                    activate: Box::new(move |t: &mut Self| {
                        let _ = t.tx.send(cmd.clone());
                    }),
                    ..Default::default()
                }
                .into()
            };
            let mut items = vec![item(self.labels.show.clone(), Cmd::Show)];
            if !self.preset_names.is_empty() {
                let presets = self
                    .preset_names
                    .iter()
                    .map(|n| item(n.clone(), Cmd::Preset(n.clone())))
                    .collect();
                items.push(
                    SubMenu {
                        label: self.labels.presets.clone(),
                        submenu: presets,
                        ..Default::default()
                    }
                    .into(),
                );
            }
            items.push(ksni::MenuItem::Separator);
            items.push(item(self.labels.quit.clone(), Cmd::Quit));
            items
        }
    }

    // StatusNotifierItem wants ARGB32 in network byte order.
    let icon = decode_png(TRAY_PNG)
        .map(|(rgba, w, h)| {
            let data = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[3], p[0], p[1], p[2]])
                .collect();
            vec![ksni::Icon {
                width: w as i32,
                height: h as i32,
                data,
            }]
        })
        .unwrap_or_default();
    let preset_names = Presets::load().list.into_iter().map(|p| p.name).collect();
    std::thread::Builder::new()
        .name("odyssey-tray".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async move {
                // No tray host (e.g. a bare compositor): keep running without one.
                if let Ok(_handle) = (Tray {
                    tx,
                    labels,
                    icon,
                    preset_names,
                })
                .spawn()
                .await
                {
                    std::future::pending::<()>().await;
                }
            });
        })
        .ok();
}

// ---- active window (KDE Plasma) -----------------------------------------------

/// Serves `WindowActivated(class, title)` on the session bus; the KWin script calls it on every
/// focus change. Fails silently without a session bus (the feature is then unavailable).
#[cfg(target_os = "linux")]
pub fn spawn_window_service(tx: Sender<Cmd>) {
    struct Service {
        tx: Sender<Cmd>,
    }

    #[zbus::interface(name = "io.github.deddroid.OdysseyCtl")]
    impl Service {
        fn window_activated(&self, class: String, title: String) {
            // The receiver only disappears at shutdown.
            let _ = self.tx.send(Cmd::Window { class, title });
        }

        /// One window per line as `class<US>title` (see the KWin script).
        fn windows(&self, list: String) {
            let windows = list
                .lines()
                .filter_map(|l| l.split_once('\u{1f}'))
                .map(|(c, t)| (c.to_string(), t.to_string()))
                .collect();
            let _ = self.tx.send(Cmd::Windows(windows));
        }
    }

    std::thread::Builder::new()
        .name("odyssey-window-service".into())
        .spawn(move || {
            let connection = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name(WINDOW_SERVICE))
                .and_then(|b| b.serve_at(WINDOW_PATH, Service { tx }))
                .and_then(|b| b.build());
            match connection {
                Ok(_connection) => loop {
                    std::thread::park();
                },
                Err(e) => eprintln!("window service unavailable: {e}"),
            }
        })
        .ok();
}

#[cfg(not(target_os = "linux"))]
pub fn spawn_window_service(_tx: Sender<Cmd>) {}

#[cfg(target_os = "linux")]
const KWIN_SCRIPT_ID: &str = "odyssey-ctl";
#[cfg(target_os = "linux")]
const KWIN_METADATA: &str = include_str!("../../packaging/kwin/odyssey-ctl/metadata.json");
#[cfg(target_os = "linux")]
const KWIN_MAIN_JS: &str = include_str!("../../packaging/kwin/odyssey-ctl/contents/code/main.js");

/// Installs (or removes) the KWin script under `~/.local/share/kwin/scripts/` and switches it on
/// (or off) in `kwinrc`, then asks KWin to reload its configuration.
#[cfg(target_os = "linux")]
pub fn set_window_watch(enabled: bool) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    let dir = dirs::data_dir()
        .ok_or_else(|| Error::other("no data directory"))?
        .join("kwin/scripts")
        .join(KWIN_SCRIPT_ID);
    if enabled {
        let changed = write_if_changed(&dir.join("metadata.json"), KWIN_METADATA)?
            | write_if_changed(&dir.join("contents/code/main.js"), KWIN_MAIN_JS)?;
        if changed {
            // KWin keeps running the old copy until the script is unloaded; `reconfigure`
            // below loads the new one.
            kwin_unload_script()?;
        }
        kwin_plugin(true)
    } else {
        kwin_plugin(false)?;
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn set_window_watch(_enabled: bool) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "per-application presets need KDE Plasma",
    ))
}

/// Writes `content` unless the file already holds it; true when it was written.
#[cfg(target_os = "linux")]
fn write_if_changed(path: &std::path::Path, content: &str) -> std::io::Result<bool> {
    if std::fs::read_to_string(path).is_ok_and(|current| current == content) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(true)
}

/// `org.kde.kwin.Scripting.unloadScript`: stops the running copy of the script.
#[cfg(target_os = "linux")]
fn kwin_unload_script() -> std::io::Result<()> {
    use std::io::Error;
    let connection = zbus::blocking::Connection::session().map_err(Error::other)?;
    connection
        .call_method(
            Some("org.kde.KWin"),
            "/Scripting",
            Some("org.kde.kwin.Scripting"),
            "unloadScript",
            &(KWIN_SCRIPT_ID,),
        )
        .map_err(|e| Error::other(format!("KWin did not unload the script: {e}")))?;
    Ok(())
}

/// `kwinrc [Plugins] odyssey-ctlEnabled=<enabled>`, then `org.kde.KWin.reconfigure`.
#[cfg(target_os = "linux")]
fn kwin_plugin(enabled: bool) -> std::io::Result<()> {
    use std::io::Error;
    let status = std::process::Command::new("kwriteconfig6")
        .args(["--file", "kwinrc", "--group", "Plugins", "--key"])
        .arg(format!("{KWIN_SCRIPT_ID}Enabled"))
        .arg(if enabled { "true" } else { "false" })
        .status()
        .map_err(|e| Error::other(format!("kwriteconfig6: {e} (is this KDE Plasma 6?)")))?;
    if !status.success() {
        return Err(Error::other(format!("kwriteconfig6 exited with {status}")));
    }
    let connection = zbus::blocking::Connection::session().map_err(Error::other)?;
    connection
        .call_method(
            Some("org.kde.KWin"),
            "/KWin",
            Some("org.kde.KWin"),
            "reconfigure",
            &(),
        )
        .map_err(|e| Error::other(format!("KWin did not reload its configuration: {e}")))?;
    Ok(())
}

// ---- tray: Windows -----------------------------------------------------------

#[cfg(windows)]
pub fn spawn_tray(tx: Sender<Cmd>, labels: Labels) {
    std::thread::Builder::new()
        .name("odyssey-tray".into())
        .spawn(move || {
            use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
            use tray_icon::{MouseButton, TrayIconBuilder, TrayIconEvent};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
            };

            let show = MenuItem::new(&labels.show, true, None);
            let quit = MenuItem::new(&labels.quit, true, None);
            let menu = Menu::new();
            let _ = menu.append(&show);
            // The preset list is read once: Windows menus are built up front.
            let presets: Vec<(MenuItem, String)> = Presets::load()
                .list
                .into_iter()
                .map(|p| (MenuItem::new(&p.name, true, None), p.name))
                .collect();
            if !presets.is_empty() {
                let sub = Submenu::new(&labels.presets, true);
                for (item, _) in &presets {
                    let _ = sub.append(item);
                }
                let _ = menu.append(&sub);
            }
            let _ = menu.append_items(&[&PredefinedMenuItem::separator(), &quit]);
            let mut builder = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip(&labels.tooltip);
            if let Some((rgba, w, h)) = decode_png(TRAY_PNG)
                && let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h)
            {
                builder = builder.with_icon(icon);
            }
            let Ok(_tray) = builder.build() else { return };
            // The tray icon lives on this thread, which therefore needs its own Win32 message loop.
            loop {
                unsafe {
                    let mut msg: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                while let Ok(ev) = MenuEvent::receiver().try_recv() {
                    let cmd = if ev.id == *show.id() {
                        Cmd::Show
                    } else if ev.id == *quit.id() {
                        Cmd::Quit
                    } else if let Some((_, name)) =
                        presets.iter().find(|(item, _)| ev.id == *item.id())
                    {
                        Cmd::Preset(name.clone())
                    } else {
                        continue;
                    };
                    if tx.send(cmd).is_err() {
                        return;
                    }
                }
                while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        ..
                    } = ev
                        && tx.send(Cmd::Show).is_err()
                    {
                        return;
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        })
        .ok();
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn spawn_tray(_tx: Sender<Cmd>, _labels: Labels) {}

// ---- autostart ---------------------------------------------------------------

/// Enables or disables starting the GUI (hidden, in the tray) at login.
pub fn set_autostart(enabled: bool) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    platform_autostart(enabled, &exe)
}

#[cfg(target_os = "linux")]
fn autostart_file() -> std::path::PathBuf {
    // `ODYSSEY_CTL_AUTOSTART_DIR` overrides the directory, for tests.
    let dir = match std::env::var_os("ODYSSEY_CTL_AUTOSTART_DIR") {
        Some(d) => std::path::PathBuf::from(d),
        None => dirs::config_dir().unwrap_or_default().join("autostart"),
    };
    dir.join("odyssey-ctl.desktop")
}

#[cfg(target_os = "linux")]
fn platform_autostart(enabled: bool, exe: &std::path::Path) -> std::io::Result<()> {
    let file = autostart_file();
    if !enabled {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(
        &file,
        format!(
            "[Desktop Entry]\nType=Application\nName=Odyssey Control\n\
             Exec=\"{}\" --hidden\nIcon=odyssey-ctl\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
            exe.display()
        ),
    )
}

#[cfg(windows)]
fn platform_autostart(enabled: bool, exe: &std::path::Path) -> std::io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let (run, _) = RegKey::predef(HKEY_CURRENT_USER)
        .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")?;
    if enabled {
        run.set_value("odyssey-ctl", &format!("\"{}\" --hidden", exe.display()))
    } else {
        match run.delete_value("odyssey-ctl") {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn platform_autostart(_enabled: bool, _exe: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn autostart_file_is_created_and_removed() {
        let dir = std::env::temp_dir().join(format!("odyssey-autostart-{}", std::process::id()));
        // SAFETY: tests in this binary run on one thread for this variable's lifetime of use.
        unsafe { std::env::set_var("ODYSSEY_CTL_AUTOSTART_DIR", &dir) };
        set_autostart(true).unwrap();
        let text = std::fs::read_to_string(autostart_file()).unwrap();
        assert!(text.contains("Exec=\"") && text.contains("--hidden"));
        set_autostart(false).unwrap();
        assert!(!autostart_file().exists());
        set_autostart(false).unwrap(); // removing twice is fine
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn icons_decode() {
        let (rgba, w, h) = decode_png(ICON_PNG).unwrap();
        assert_eq!((w, h, rgba.len()), (256, 256, 256 * 256 * 4));
        assert!(window_icon().is_some());
    }
}
