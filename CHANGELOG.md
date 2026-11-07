# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed
- GUI restyle: light and dark palettes that follow the system theme, Inter and Rajdhani fonts,
  pages as grouped lists with the control on the right, option menus with a check mark instead
  of chip rows, sliders with an editable value, concentric corner radii. Screenshots are now saved
  for both themes (`*_light.png`).

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
