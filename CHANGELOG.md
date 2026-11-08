# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Per-application presets on KDE Plasma 6: Presets → Per application installs a KWin script
  that reports the focused window to the app over D-Bus; rules match the window class (and
  optionally the title) with `*`/`?` wildcards, and a fallback preset is applied when a window
  without a rule gets the focus (and, by choice, when the desktop is clicked). Stored in
  `presets.toml` as `[[app]]` and `app_default`.
- Presets page: "Add" creates a preset from the current settings and lets you type its name in
  place; names can be renamed in place (rules follow the new name).
- `odyssey-ctl preset list|save|apply`: the presets of `presets.toml` from the command line, the
  same file the desktop app and its tray menu use. `apply` exits with 1 when a write fails, so
  scripts can tell.
- Relative values: `odyssey-ctl set brightness +5` (or `-5`) reads the current value and keeps
  the result within the monitor's range.
- Automation templates in `packaging/systemd/`: a user unit that applies a preset by name, an
  example timer, and a systemd-sleep hook that applies a preset after resume. The README lists
  the recipes for KDE shortcuts, Steam launch options and GameMode.

### Changed
- `serde`, `toml` and `dirs` are now always built in (the command line tool reads the presets
  file); the `gui` feature only adds the desktop app.

## [0.3.0] - 2026-11-08

### Added
- Light theme: the app follows the system theme, and Settings has a Theme row to force
  system, light or dark.
- Aim point: the screen outline shows the real crosshair of the chosen type and colour, and the
  icon is chosen in two rows, type (A or B) and colour (light blue, red, white).

### Changed
- New look: Inter and Rajdhani fonts (bundled, SIL Open Font License), pages as grouped lists
  with the control on the right, option menus with a check mark instead of chip rows, sliders
  with an editable value, concentric corner radii.
- Aim point: the arrows sit around the Centre button and move by exactly 1, 10 or 50 units;
  typed coordinates are written as soon as they settle; Y counts from the bottom like a
  Cartesian axis in the GUI (the CLI and `presets.toml` keep the monitor's coordinates); the box
  stays open while the position is read again after an icon change.
- `ODYSSEY_CTL_SCREENSHOTS` saves every page in both themes (`*_light.png`), without the index
  prefix in the file names.

### Removed
- The Apply button next to the aim point coordinates (they are applied as soon as they settle).

### Verified on hardware
- G60SD, firmware 1006.0: the new interface was tried on the monitor; the writes are the same as
  in 0.2.0.

## [0.2.0] - 2026-11-06

### Added
- Virtual Aim Point placement: `aim status`, `aim move`, `aim position` (and `--center`). In the
  GUI: a screen outline to click on, arrows, coordinates, a centre button and saved positions
  (`[[aim]]` in `presets.toml`). The stateful `EE`/`EF` registers are read in full cycles.
- Picture-in-Picture control: `pip on|off|mode|size|position|source|status`; switch, size, corner,
  input of each window and sound in the GUI.
- Adaptive-Sync / FreeSync is writable (switch in the Game page); presets can hold it.
- CoreSync switch in the Lighting page (`coresync on|off`): the Static effect is selected first,
  as the monitor requires.
- Monitor page lists the monitors found and switches between them.
- `--unverified` is a global flag; `features` also lists the actions and their status.
- `reset-white-balance` (E0 in the LEMON profile) on the command line; no effect on the G60SD.

### Changed
- Writes that make the monitor re-sync its video (Adaptive-Sync, PIP) wait longer and retry the
  read-back once after an I/O error.

### Verified on hardware
- G60SD, firmware 1006.0: picture mode write, Adaptive-Sync, aim point move and position, CoreSync,
  PIP on/off/size/corner/source.

## [0.1.0] - 2026-11-06

### Added
- DDC/CI library with packet encoding, capability parsing, EDID and a feature table that records
  what is verified on hardware.
- `odyssey-ctl` CLI: `list`, `info`, `get`, `set`, `features`, `sdm`, `raw`.
- `odyssey-ctl-gui`: picture, game, lighting, inputs/audio, monitor and settings pages; presets;
  English and Italian; tray icon with a presets menu; single instance; autostart.
- Samsung extended profile (`EB=A1`/`A2`) and the SDM feature codes, documented in
  `docs/PROTOCOL.md`.
- Linux packaging: udev rule, `modules-load.d` entry, desktop entry, icons, `install.sh`,
  AUR `PKGBUILD`s.

### Verified on hardware
- Samsung Odyssey OLED G6 G60SD (LS27DG602SUXEN), firmware 1003.0 and 1006.0, HDMI, Linux.

[Unreleased]: https://github.com/deddroid/odyssey-ctl/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/deddroid/odyssey-ctl/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/deddroid/odyssey-ctl/releases/tag/v0.1.0
