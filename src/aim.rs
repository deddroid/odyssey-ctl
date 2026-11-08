//! Virtual Aim Point placement: VCP `EE` (move / border) and `EF` (position).
//!
//! Both registers answer reads statefully: every GET of `EE` returns the next of four border
//! items (x, y, width, height) and every GET of `EF` alternates between X and Y. The functions
//! here read in full cycles so callers never see a half state; a plain `get` elsewhere would move
//! the cycle on. Encoding from Samsung Display Manager (docs/PROTOCOL.md).

use std::thread::sleep;
use std::time::Duration;

use crate::device::{Error, Vcp};

pub const VCP_MOVE: u8 = 0xEE;
pub const VCP_POSITION: u8 = 0xEF;

/// Largest coordinate the 13-bit fields carry.
pub const MAX_COORD: u16 = 0x1FFF;

/// Units one `EE` step moves the crosshair (verified on the G60SD).
pub const STEP_UNITS: u16 = 2;

/// The icon register `ED`: 0 is off, then type A and type B, each in light blue, red and white.
pub const ICON_COLOURS: u16 = 3;

/// Icon type (0 = A, 1 = B) and colour (0 light blue, 1 red, 2 white) of an `ED` value, when it
/// is one of the six icons.
pub fn icon_parts(value: u16) -> Option<(u16, u16)> {
    (1..=2 * ICON_COLOURS)
        .contains(&value)
        .then(|| ((value - 1) / ICON_COLOURS, (value - 1) % ICON_COLOURS))
}

/// The `ED` value of an icon type and colour.
pub fn icon_value(kind: u16, colour: u16) -> u16 {
    kind * ICON_COLOURS + colour + 1
}

/// Minimum time between two `EE` steps: closer writes are partly dropped and hide the
/// crosshair (verified on the G60SD).
pub const STEP_PACING_MS: u64 = 500;

impl Move {
    /// Where `steps` steps from `p` land, kept inside `border`.
    pub fn apply(self, p: Position, steps: u16, border: Border) -> Position {
        self.shift(p, steps.saturating_mul(STEP_UNITS), border)
    }

    /// `p` moved by `units` coordinate units, kept inside `border`.
    pub fn shift(self, p: Position, units: u16, border: Border) -> Position {
        let d = units;
        let (min_x, max_x) = (border.x, border.x + border.width.saturating_sub(1));
        let (min_y, max_y) = (border.y, border.y + border.height.saturating_sub(1));
        match self {
            Move::Up => Position {
                x: p.x,
                y: p.y.saturating_sub(d).max(min_y),
            },
            Move::Down => Position {
                x: p.x,
                y: p.y.saturating_add(d).min(max_y),
            },
            Move::Left => Position {
                x: p.x.saturating_sub(d).max(min_x),
                y: p.y,
            },
            Move::Right => Position {
                x: p.x.saturating_add(d).min(max_x),
                y: p.y,
            },
        }
    }
}

/// One step of the crosshair (write to `EE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up = 0,
    Down = 1,
    Left = 2,
    Right = 3,
}

impl Move {
    pub const ALL: [Move; 4] = [Move::Up, Move::Down, Move::Left, Move::Right];

    pub fn as_str(self) -> &'static str {
        match self {
            Move::Up => "up",
            Move::Down => "down",
            Move::Left => "left",
            Move::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<Move> {
        Move::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(s))
    }
}

/// Area the crosshair may be placed in, as the monitor reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Border {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Border {
    pub fn center(&self) -> Position {
        Position {
            x: self.x + self.width / 2,
            y: self.y + self.height / 2,
        }
    }

    pub fn contains(&self, p: Position) -> bool {
        (self.x..self.x + self.width).contains(&p.x)
            && (self.y..self.y + self.height).contains(&p.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: u16,
    pub y: u16,
}

/// Splits a stateful reading into its component (bits 15-13) and 13-bit value.
/// Bit 15 set means the monitor does not support the item.
pub fn decode_item(raw: u16) -> Option<(u8, u16)> {
    if raw & 0x8000 != 0 {
        return None;
    }
    Some((((raw >> 13) & 0x3) as u8, raw & MAX_COORD))
}

/// The two `EF` writes that set a position: X, then Y flagged with bit 13.
pub fn encode_position(p: Position) -> [u16; 2] {
    [p.x & MAX_COORD, 0x2000 | (p.y & MAX_COORD)]
}

/// Reads the border: four `EE` items in whatever order the cycle is at.
pub fn read_border(m: &mut dyn Vcp) -> Result<Border, Error> {
    let mut items = [None; 4];
    for _ in 0..6 {
        let raw = m.get_vcp(VCP_MOVE)?.current;
        let (component, value) = decode_item(raw).ok_or_else(|| {
            Error::Other("the monitor does not report the aim point border".into())
        })?;
        items[usize::from(component)] = Some(value);
        if items.iter().all(Option::is_some) {
            break;
        }
    }
    match items {
        [Some(x), Some(y), Some(width), Some(height)] => Ok(Border {
            x,
            y,
            width,
            height,
        }),
        _ => Err(Error::Other(
            "incomplete aim point border reading (try again)".into(),
        )),
    }
}

/// Reads the position: `EF` alternates X and Y, so align on X first.
pub fn read_position(m: &mut dyn Vcp) -> Result<Position, Error> {
    let mut x = None;
    for _ in 0..4 {
        let raw = m.get_vcp(VCP_POSITION)?.current;
        let (component, value) = decode_item(raw).ok_or_else(|| {
            Error::Other("the monitor does not report the aim point position".into())
        })?;
        match (component, x) {
            (0, _) => x = Some(value),
            (1, Some(x)) => return Ok(Position { x, y: value }),
            _ => {}
        }
    }
    Err(Error::Other(
        "incomplete aim point position reading (try again)".into(),
    ))
}

/// Writes `p` (X then Y, as Samsung Display Manager does) and reads it back.
pub fn set_position(m: &mut dyn Vcp, p: Position) -> Result<Position, Error> {
    if p.x > MAX_COORD || p.y > MAX_COORD {
        return Err(Error::Other(format!(
            "aim point coordinates must be at most {MAX_COORD}"
        )));
    }
    let [x, y] = encode_position(p);
    m.set_vcp(VCP_POSITION, x)?;
    sleep(Duration::from_millis(150));
    m.set_vcp(VCP_POSITION, y)?;
    sleep(Duration::from_millis(200));
    let after = read_position(m)?;
    if after != p {
        return Err(Error::Other(format!(
            "aim point: asked for ({}, {}) but the monitor reports ({}, {})",
            p.x, p.y, after.x, after.y
        )));
    }
    Ok(after)
}

/// Moves the crosshair one step. The monitor gives no acknowledgement for a step.
pub fn step(m: &mut dyn Vcp, dir: Move) -> Result<(), Error> {
    m.set_vcp(VCP_MOVE, dir as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoDevice;

    #[test]
    fn decodes_readings_seen_on_the_g60sd() {
        // EE read 0x0023: component 0 (x) = 35. EF read 0x0354: component 0 (x) = 852.
        assert_eq!(decode_item(0x0023), Some((0, 35)));
        assert_eq!(decode_item(0x0354), Some((0, 852)));
        // EF maximum 0x3FFF = component 1 (y) with every value bit set.
        assert_eq!(decode_item(0x3FFF), Some((1, MAX_COORD)));
        assert_eq!(decode_item(0x8000), None);
        assert_eq!(
            encode_position(Position { x: 852, y: 600 }),
            [0x0354, 0x2000 | 600]
        );
        assert_eq!(Move::parse("LEFT"), Some(Move::Left));
        assert_eq!(Move::Left as u16, 2);
    }

    #[test]
    fn steps_become_positions_inside_the_border() {
        let b = Border {
            x: 35,
            y: 36,
            width: 1629,
            height: 407,
        };
        let p = Position { x: 852, y: 239 };
        assert_eq!(Move::Right.apply(p, 10, b), Position { x: 872, y: 239 });
        assert_eq!(Move::Up.apply(p, 50, b), Position { x: 852, y: 139 });
        assert_eq!(Move::Up.apply(p, 500, b).y, 36);
        assert_eq!(Move::Right.apply(p, 5000, b).x, 35 + 1629 - 1);
    }

    #[test]
    fn icon_values_split_into_type_and_colour() {
        assert_eq!(icon_parts(0), None);
        assert_eq!(icon_parts(1), Some((0, 0)));
        assert_eq!(icon_parts(5), Some((1, 1)));
        assert_eq!(icon_parts(7), None);
        for v in 1..=6 {
            let (k, c) = icon_parts(v).unwrap();
            assert_eq!(icon_value(k, c), v);
        }
    }

    #[test]
    fn border_geometry() {
        let b = Border {
            x: 35,
            y: 20,
            width: 2490,
            height: 1400,
        };
        assert_eq!(b.center(), Position { x: 1280, y: 720 });
        assert!(b.contains(Position { x: 35, y: 20 }));
        assert!(!b.contains(Position { x: 2525, y: 20 }));
    }

    #[test]
    fn reads_full_cycles_and_sets_position() {
        let mut d = DemoDevice::new();
        // A stray read leaves the cycle mid-way; the readers must still assemble a full state.
        d.get_vcp(VCP_MOVE).unwrap();
        d.get_vcp(VCP_POSITION).unwrap();
        let border = read_border(&mut d).unwrap();
        assert_eq!(border.x, 35);
        assert_eq!(read_position(&mut d).unwrap(), Position { x: 852, y: 600 });
        let p = Position { x: 1000, y: 700 };
        assert_eq!(set_position(&mut d, p).unwrap(), p);
        assert!(set_position(&mut d, Position { x: 9000, y: 0 }).is_err());
        step(&mut d, Move::Right).unwrap();
        assert_eq!(
            read_position(&mut d).unwrap().x,
            1000 + crate::demo::AIM_STEP
        );
    }
}
