//! VCP access over a raw byte bus, with DDC/CI timing and retries.

use std::io;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::ddc::{self, ReplyError, VcpValue};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Reply(#[from] ReplyError),
    #[error("{0}")]
    Other(String),
}

/// Raw byte access to the DDC/CI slave (I2C address 0x37).
pub trait Bus {
    fn write(&mut self, data: &[u8]) -> io::Result<()>;
    fn read(&mut self, buf: &mut [u8]) -> io::Result<()>;
}

/// VCP-level access, implemented over a raw bus on Linux and over dxva2 on Windows.
pub trait Vcp {
    fn get_vcp(&mut self, code: u8) -> Result<VcpValue, Error>;
    fn set_vcp(&mut self, code: u8, value: u16) -> Result<(), Error>;
    fn capabilities(&mut self) -> Result<String, Error>;
}

/// DDC/CI delays. The G60SD answers reliably with these (0 retries over ~200 transactions);
/// the 50 ms the standard suggests are available through the environment variables below.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Wait between sending a request and reading its reply.
    pub reply: Duration,
    /// Minimum gap between two transactions.
    pub gap: Duration,
    /// Base back-off before a retry (multiplied by the attempt number).
    pub retry: Duration,
    pub attempts: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            reply: Duration::from_millis(30),
            gap: Duration::from_millis(20),
            retry: Duration::from_millis(100),
            attempts: 4,
        }
    }
}

impl Timing {
    /// Defaults, overridden by `ODYSSEY_CTL_DDC_REPLY_MS` and `ODYSSEY_CTL_DDC_GAP_MS`
    /// (for monitors that need more time, or for measuring how little this one needs).
    pub fn from_env() -> Self {
        let ms = |name: &str, default: Duration| {
            std::env::var(name).ok().and_then(|v| v.parse().ok()).map_or(default, Duration::from_millis)
        };
        let base = Timing::default();
        Timing {
            reply: ms("ODYSSEY_CTL_DDC_REPLY_MS", base.reply),
            gap: ms("ODYSSEY_CTL_DDC_GAP_MS", base.gap),
            ..base
        }
    }
}

pub struct DdcDevice<B: Bus> {
    bus: B,
    timing: Timing,
    debug: bool,
    last: Option<Instant>,
}

impl<B: Bus> DdcDevice<B> {
    pub fn new(bus: B) -> Self {
        DdcDevice { bus, timing: Timing::from_env(), debug: crate::debug_enabled(), last: None }
    }

    pub fn with_timing(mut self, timing: Timing) -> Self {
        self.timing = timing;
        self
    }

    fn wait_gap(&self) {
        if let Some(last) = self.last {
            let elapsed = last.elapsed();
            if elapsed < self.timing.gap {
                sleep(self.timing.gap - elapsed);
            }
        }
    }

    fn send(&mut self, msg: &[u8]) -> io::Result<()> {
        self.wait_gap();
        if self.debug {
            eprintln!("TX {}", hex(msg));
        }
        let r = self.bus.write(msg);
        self.last = Some(Instant::now());
        r
    }

    fn request(&mut self, msg: &[u8], len: usize) -> io::Result<Vec<u8>> {
        self.send(msg)?;
        sleep(self.timing.reply);
        let mut buf = vec![0u8; len];
        let r = self.bus.read(&mut buf);
        self.last = Some(Instant::now());
        r?;
        if self.debug {
            eprintln!("RX {}", hex(&buf));
        }
        Ok(buf)
    }

    /// Runs `op` until it succeeds, retrying transient failures (I/O, checksum, busy).
    fn retry<T>(&mut self, mut op: impl FnMut(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        let mut attempt = 1;
        loop {
            match op(self) {
                Err(e) if attempt < self.timing.attempts && is_transient(&e) => {
                    if self.debug {
                        eprintln!("retry {attempt}: {e}");
                    }
                    sleep(self.timing.retry * attempt);
                    attempt += 1;
                }
                r => return r,
            }
        }
    }
}

fn is_transient(e: &Error) -> bool {
    !matches!(e, Error::Reply(ReplyError::Unsupported(_)) | Error::Other(_))
}

impl<B: Bus> Vcp for DdcDevice<B> {
    fn get_vcp(&mut self, code: u8) -> Result<VcpValue, Error> {
        self.retry(|d| {
            let buf = d.request(&ddc::get_vcp_request(code), ddc::VCP_REPLY_LEN)?;
            Ok(ddc::parse_vcp_reply(&buf, code)?)
        })
    }

    fn set_vcp(&mut self, code: u8, value: u16) -> Result<(), Error> {
        // Set has no reply, so only I/O errors can be retried.
        self.retry(|d| Ok(d.send(&ddc::set_vcp_request(code, value))?))
    }

    fn capabilities(&mut self) -> Result<String, Error> {
        let mut out = Vec::new();
        loop {
            let offset = out.len() as u16;
            let chunk = self.retry(|d| {
                let buf = d.request(&ddc::capabilities_request(offset), ddc::CAPS_REPLY_LEN)?;
                Ok(ddc::parse_capabilities_reply(&buf, offset)?.to_vec())
            })?;
            if chunk.is_empty() {
                break;
            }
            out.extend_from_slice(&chunk);
            if out.len() > 4096 {
                return Err(Error::Other("capability string longer than 4 KiB".into()));
            }
        }
        let s = String::from_utf8_lossy(&out);
        Ok(s.trim_end_matches('\0').to_string())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddc::checksum;
    use std::collections::VecDeque;

    /// Bus that records writes and returns scripted replies.
    #[derive(Default)]
    struct FakeBus {
        written: Vec<Vec<u8>>,
        replies: VecDeque<Vec<u8>>,
    }

    impl Bus for FakeBus {
        fn write(&mut self, data: &[u8]) -> io::Result<()> {
            self.written.push(data.to_vec());
            Ok(())
        }
        fn read(&mut self, buf: &mut [u8]) -> io::Result<()> {
            let r = self.replies.pop_front().ok_or_else(|| io::Error::other("no reply"))?;
            buf.fill(0);
            buf[..r.len()].copy_from_slice(&r);
            Ok(())
        }
    }

    fn device(replies: Vec<Vec<u8>>) -> DdcDevice<FakeBus> {
        let bus = FakeBus { replies: replies.into(), ..Default::default() };
        let zero = Duration::ZERO;
        DdcDevice::new(bus).with_timing(Timing { reply: zero, gap: zero, retry: zero, attempts: 3 })
    }

    fn frame(payload: &[u8]) -> Vec<u8> {
        let mut f = vec![0x6E, 0x80 | payload.len() as u8];
        f.extend_from_slice(payload);
        f.push(checksum(0x50, &f));
        f
    }

    #[test]
    fn get_retries_after_bad_checksum_and_null() {
        let good = vec![0x6E, 0x88, 0x02, 0x00, 0x2F, 0x00, 0x00, 0x0A, 0x00, 0x06, 0x97];
        let mut bad = good.clone();
        bad[10] ^= 1;
        let mut d = device(vec![bad, vec![0x6E, 0x80, 0xBE], good]);
        let v = d.get_vcp(0x2F).unwrap();
        assert_eq!((v.max, v.current), (10, 6));
        assert_eq!(d.bus.written.len(), 3);
        assert_eq!(d.bus.written[0], [0x51, 0x82, 0x01, 0x2F, 0x93]);
    }

    #[test]
    fn unsupported_is_not_retried() {
        let mut d = device(vec![frame(&[0x02, 0x01, 0xE7, 0, 0, 0, 0, 0])]);
        assert!(matches!(d.get_vcp(0xE7), Err(Error::Reply(ReplyError::Unsupported(0xE7)))));
        assert_eq!(d.bus.written.len(), 1);
    }

    #[test]
    fn gives_up_after_attempts() {
        let mut d = device(vec![]);
        assert!(matches!(d.get_vcp(0x10), Err(Error::Io(_))));
        assert_eq!(d.bus.written.len(), 3);
    }

    #[test]
    fn set_sends_one_packet() {
        let mut d = device(vec![]);
        d.set_vcp(0x2F, 6).unwrap();
        assert_eq!(d.bus.written, [vec![0x51, 0x84, 0x03, 0x2F, 0x00, 0x06, 0x91]]);
    }

    #[test]
    fn reads_capabilities_in_fragments() {
        let caps = b"(prot(monitor)type(lcd)model(SEC)vcp(10 12))";
        let mut replies = Vec::new();
        for (i, chunk) in caps.chunks(32).enumerate() {
            let off = (i * 32) as u16;
            let mut p = vec![0xE3, (off >> 8) as u8, off as u8];
            p.extend_from_slice(chunk);
            replies.push(frame(&p));
        }
        let end = caps.len() as u16;
        replies.push(frame(&[0xE3, (end >> 8) as u8, end as u8]));
        let mut d = device(replies);
        assert_eq!(d.capabilities().unwrap(), String::from_utf8_lossy(caps));
        assert_eq!(d.bus.written[1], ddc::capabilities_request(32));
    }
}
