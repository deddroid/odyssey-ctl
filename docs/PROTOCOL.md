# Samsung Odyssey DDC/CI protocol notes

What `odyssey-ctl` knows about controlling a Samsung Odyssey monitor, and where each fact comes from.
Hardware results are from an **Odyssey OLED G6 G60SD** (LS27DG602SUXEN) over HDMI, on Linux.

Legend: **verified** = confirmed on hardware (reading compared with the on-screen menu, or a write
seen on screen); **from SDM** = taken from Samsung Display Manager, not tested.

## 1. Transport

Standard DDC/CI (VESA MCCS 2.0) on I2C address `0x37`; EDID on `0x50`.

| Message | Bytes |
|---|---|
| Get VCP request | `51 82 01 <code> <chk>` |
| Get VCP reply (11 bytes) | `6E 88 02 <result> <code> <type> <maxH> <maxL> <curH> <curL> <chk>` |
| Set VCP | `51 84 03 <code> <valH> <valL> <chk>` (no reply) |
| Capabilities request | `51 83 F3 <offH> <offL> <chk>` |
| Capabilities reply | `6E 80+n E3 <offH> <offL> <up to 32 bytes> <chk>`; an empty fragment ends the string |
| Null message (busy) | `6E 80 BE` |

Host checksum: XOR of all bytes with `0x6E`. Reply checksum: XOR of all bytes with `0x50`.
`result` = 1 means "unsupported code". Example captured on the G60SD:
`51 82 01 10 AC` -> `6E 88 02 00 10 00 00 32 00 32 A4` (brightness 50 of 50).

Timing that works on the G60SD: 50 ms between request and reply, 50 ms between transactions,
retry on bad checksum or null message. With these the capability string reads at the first try.

**Do not poll.** Reading the monitor every ~3 s makes its on-screen menu close immediately (verified).
Read only when the user asks.

## 2. Firmware profiles

The `model(...)` field of the capability string identifies the firmware profile.

| Firmware | Profile | Notes (verified) |
|---|---|---|
| 1003.0 | `FALCON` | Writes only to codes listed in the capability string; others are silently ignored (`2F` write ignored, `10` write works). Input `60` reports `05` for HDMI. Picture mode readable in `DC`. |
| 1006.0, default | `SEC` | Adds `87` and `CC` to the list. Writes to Samsung codes work (`2F` verified). Input `60` reports `11` for HDMI 1. |
| 1006.0 after `EB=A1` | `LEMON24v1` | The full Samsung Display Manager set (see §4). |

The 1006.0 image (`M-D6027GGAA-1006.0`, Novatek NT68550, 8051 code) contains the `SEC` and
`LEMON24v1` strings and two VCP descriptor tables (`u16 code, u16 handler, u16 max`), whose maxima match
what the monitor reports.

## 3. VCP EB: Samsung Display Manager control register

From `DisplayToolkit.dll` (SDM 1.2.6 for Windows) and the SDM macOS app; `A1`/`A2` verified.

| Write | Meaning |
|---|---|
| `EB=A1` | Enable: the monitor reports `LEMON24v1` capabilities (verified). SDM reads `EB` back, expects `A1`, otherwise waits 5 s and retries. |
| `EB=A2` | Disable: back to `SEC` (verified). Idle value after power-on reads `A0` (`support`). |
| `EB=B1`, then read `E0`..`E4` | Serial number, 3 bytes per code (from SDM). |
| `EB=B2`, then read `E0`..`E4` | Model code (from SDM). |
| `EB=C2`, then read `E0`..`E4` | Virtual USB serial number (from SDM). |
| `EB=C1`, then `E0=C0+n` | Generates a new virtual USB serial number (persistent identifier). Never send it. |
| monitor sets `A3` / `A4` | "Refresh picture mode status" / "refresh full status": SDM polls `EB` for these. |

## 3.1 Reading rules

- **`0xFD` means "disabled in the current state"**, not a value: e.g. Black Equalizer while Eye Saver is on,
  lighting colour with an effect that has no colour. It explains readings above the maximum (`FA` = `FD`, max `2F`).
  Do not write a feature whose last reading was `0xFD`.
- **`EE` and `EF` reads are stateful**: each GET advances an internal cycle (EE: 4 border items; EF: X then Y).
  Never read them during a general scan.
- SDM waits 400 ms after writing the picture mode before using the monitor again.

## 4. Feature codes

SDM macOS keeps a table from its `VcpFeature` enum to VCP codes (case names from the Swift type metadata,
codes from the enum's raw-value table); the Windows `MonToolkitProtocol` vtable uses the same codes.
Value meanings come from SDM's Swift enums and the decoders in both programs.

| Feature | VCP | Values / encoding | Status on G60SD 1006.0 |
|---|---|---|---|
| Brightness | `10` | 0–50 | verified (write) |
| Contrast | `12` | 0–50 | standard MCCS |
| Sharpness | `87` | 0–20 | standard MCCS |
| White balance R / G / B | `16` / `18` / `1A` | 0–100 | standard MCCS |
| White balance reset | `E0` | write `0` (LEMON only). In the SEC profile `E0` shares the colour-tone handler: never write it there | **no effect on the G60SD**: `E0` reports max 0 even in LEMON24v1, the write is accepted and ignored (gains unchanged) |
| Picture mode | `2D` | PC mode (`E4`=0): 0 Entertain, 1 Graphic, 2 Eco, 3 Game Standard, 4 RPG, 5 RTS, 6 FPS, 7 Sports, 8 Original, 9 Custom. AV mode: 0 Dynamic, 1 Standard, 2 Eco, 3 Movie, 4 Game Standard, 5 RPG, 6 RTS, 7 FPS, 8 Sports, 9 Original, 10 Custom (max 10 = AV list) | verified (read and write: Original ↔ Game Standard; the per-mode values follow) |
| PC/AV mode | `E4` | 0 PC, 1 AV | from SDM |
| Black Equalizer | `2F` | 0–10, `FD` when disabled | verified (write) |
| Color tone | `14` | 0 Cool, 1 Standard, 2 Warm 1, 3 Warm 2, 4 Natural | verified (write 1 = Standard on OSD) |
| Eye Saver Mode | `0A` | 0 Off, 1 Low, 2 High | verified (1 = "Lieve" on OSD; brightness, picture mode, colour tone and Black Equalizer then read `FD`) |
| Adaptive Picture | `E1` | 0 off, 1 on, `FD` deactivated (PC mode only) | from SDM (max 0 in SEC) |
| Input source | `60` | `0F` DP, `11` HDMI 1, `12` HDMI 2 | standard MCCS |
| Language | `CC` | MCCS codes: 02 English, 03 French, 04 German, 05 Italian, 0A Spanish, … | standard MCCS (5 = Italian) |
| Adaptive-Sync / FreeSync | `F2` | write 0 off / 1 on. Read: low nibble 0 off, 1 on, `D` deactivated; high nibble 1 = FreeSync type | verified (write 0/1; the OSD calls it Adaptive-Sync on HDMI). **The monitor leaves the bus for about a second after the write** (video re-sync): read back after 1 s and retry once |
| Response time | `F3` | 0 Standard, 1 Faster, 2 Extreme, 3 Extreme MBR | from SDM (not on this OLED) |
| Virtual Aim Point icon | `ED` | SDM enum: 0 Off, 1–3 type A red/blue/white, 4–6 type B red/blue/white, 7 reset position; SDM swaps 1↔2 and 4↔5 for LEMON23v3. **On the G60SD (LEMON24v1) the swap applies too: 1 = A blue, 2 = A red, 4 = B blue** | verified (1, 2, 4 on screen) |
| Virtual Aim Point move / border | `EE` | write 0 up, 1 down, 2 left, 3 right (one step = 2 units). Read (stateful, 4 GETs): bits 15–13 component (x, y, width, height), bits 12–0 value | verified. G60SD border: x 35, y 36, 1629 × 407 (firmware units, not pixels). **Steps closer than ~100 ms are partly dropped and hide the crosshair** (an isolated step shows it again); 500 ms apart every step lands. Reads of `EE`/`EF`/`ED` do not affect it. PIP on/off also hides it while `ED` keeps reading 1: write `ED` again to show it |
| Virtual Aim Point position | `EF` | write `X & 1FFF`, then `2000 \| (Y & 1FFF)`, 100 ms apart. Read (stateful) toggles X/Y | verified: default position 852 × 239 = centre of the border; a written position reads back exactly |
| Infinity Core Lighting / CoreSync | `F8` | write 0/1 = lighting off/on. Read: bit 0 lighting, bit 1 CoreSync, bit 2 sync name. CoreSync write differs between Windows (`10`/`11`) and macOS (`02`/`03`) | verified for 0/1 (effect and colour read `FD` while off). **CoreSync verified: only the Windows form works, and only with the Static effect (`F9`=3) selected first**; written alone (`11` or `03`) nothing changes. While CoreSync is on, effect and colour read `FD` |
| Lighting effect | `F9` | 0 Aurora, 1 Aurora rotation, 2 Comet, 3 Static, 4 Rainbow, 5 Breathing | verified (0, 3) |
| Lighting colour | `FA` | index into 48 colours, `FD` when the effect has no colour | verified writes (6 = light blue; 7, "blue" for SDM, looked whitish: names not trusted) |
| PIP/PBP command / status | `E2` | write one command: 0 off, 1 on, 2 PBP mode, 3–9 PBP layouts, 10 PIP mode, 11–13 PIP size, 14–17 PIP position. Read: bit field (bit 0 on, bits 3–1 mode, 5–4 size, 7–6 position, 9–8 / 11–10 supported sizes, 12 PIP supported, 13 PBP supported) | verified: on/off, size, position. Switching on re-syncs the video (bus away for ~1 s); the monitor picks the other HDMI as sub input by itself. While on, `ED` reads `FD` |
| PIP/PBP sources | `E3` | write `channel << 8 \| source` with source `type << 4 \| n` (HDMI 0x, DP 1x, USB-C 2x). Read: high byte main, low byte sub | verified (sub → DP 1); the new source reads back after more than 1 s |
| PIP/PBP sound | `E8` | 0 main, 1 sub 1, 2 sub 2 | from SDM |
| KVM USB source | `F1` | bit mask per input | from SDM (not on G60SD) |
| Reset all | `04` | write `F1` | from SDM; never sent by odyssey-ctl |
| Firmware auto update / MAPT | `FD` / `FC` | `FD` 0/1; `FC` bit 0 MAPT, bit 1 Dual Mode, bit 2 AI Upscaling | from SDM |

Picture mode order verified on hardware through `DC` on 1003.0 (0, 1, 2, 3, 8, 9).

Other observations on 1003.0: changing picture mode also resets `16/18/1A` and `87` to that mode's
values; `FE` goes from 2 to 1 when the current preset is modified.

## 5. Sources

- Reverse engineering, with Ghidra 12.2, of Samsung Display Manager 1.2.6 (Windows: `DisplayToolkit.dll`,
  `SamsungDisplayManager.exe`) and 1.0.0 (macOS), and of the G60SD firmware 1006.0. Only facts are kept here.
- [ilyamen/samsung-odyssey-ddc-unlock](https://github.com/ilyamen/samsung-odyssey-ddc-unlock): G91SD
  firmware analysis (SEC/LEMON profiles, writes to unlisted codes ignored).
- [clever-trevor/samsung-monitor-control](https://github.com/clever-trevor/samsung-monitor-control),
  [gurgelx/Samsung-Odyssey-DDC-tools](https://github.com/gurgelx/Samsung-Odyssey-DDC-tools): PBP codes `E2`/`E3` on other Odyssey models.
- [olebru/streamdeck-ddc-monitor-control](https://github.com/olebru/streamdeck-ddc-monitor-control): G93SC input values.
- VESA Monitor Control Command Set (MCCS) 2.0; [ddcutil](https://www.ddcutil.com/) for the first probes.
