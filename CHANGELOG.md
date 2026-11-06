# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
