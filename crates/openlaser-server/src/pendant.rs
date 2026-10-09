// SPDX-License-Identifier: GPL-3.0-or-later

//! The wireless Mlaser pendant (XHC, USB vendor 10CE): twenty keys, read
//! as HID input reports `04 seed key1 key2 00 check`. Fn is key 0x0B; with
//! another key held it sends that key second. A key pressed sends one
//! report and its release one with key 0; nothing repeats while it is held.
//!
//! Each key calls the same machine actions as the screen's buttons, with
//! their checks. Held moves renew their lease while the key is down and stop
//! when it is released, when the receiver disappears, or after
//! [`MAX_HELD`]: a pendant that lost its radio link mid-press never sends
//! the release, so a move may not run on without one.

use crate::coordinator::Shared;
use crate::machine::{self, JogRequest, OutputRequest, TableRequest};
use openlaser_controller::lease::Lease;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// The longest a held move runs on one press.
pub const MAX_HELD: Duration = Duration::from_secs(10);
/// How often a held move's lease is renewed, as the screen's hold does.
const RENEW: Duration = Duration::from_millis(75);
/// One press in Step mode moves X or Y this far.
const STEP_MM: f64 = 1.;
/// The W table's speed when Fn turns Z into W.
const TABLE_MM_S: f64 = 20.;
/// A test pulse from the Laser key, as the screen's test defaults.
const PULSE: (u32, u8) = (100, 10);

/// A key of the pendant, named as printed on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs, reason = "each variant is the label printed on its key")]
pub enum Key {
    Follow,
    Shutter,
    RedLight,
    Laser,
    Blow,
    Frame,
    GoOrigin,
    Start,
    Pause,
    Stop,
    Fn,
    ZCalibrate,
    FastStep,
    ZDown,
    Up,
    ZUp,
    Left,
    Down,
    Right,
}

impl Key {
    /// The key a report's code names.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0x01 => Self::Follow,
            0x02 => Self::Shutter,
            0x03 => Self::RedLight,
            0x04 => Self::Laser,
            0x05 => Self::Blow,
            0x06 => Self::Frame,
            0x07 => Self::GoOrigin,
            0x08 => Self::Start,
            0x09 => Self::Pause,
            0x0A => Self::Stop,
            0x0B => Self::Fn,
            0x0C => Self::ZCalibrate,
            0x0D => Self::FastStep,
            0x0E => Self::ZDown,
            0x0F => Self::Up,
            0x10 => Self::ZUp,
            0x11 => Self::Left,
            0x12 => Self::Down,
            0x13 => Self::Right,
            _ => return None,
        })
    }
}

/// What one input report says: the key down, with Fn, or all keys up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A key went down; `with_fn` when Fn is held with it.
    Press {
        /// The key.
        key: Key,
        /// Whether Fn+ is held with it.
        with_fn: bool,
    },
    /// Every key is up.
    Released,
    /// The receiver is gone or stopped answering.
    Lost,
}

/// Reads one report. Reports with a wrong check, or of another kind, say nothing.
#[must_use]
pub fn decode(report: &[u8]) -> Option<Event> {
    let [kind, seed, first, second, _, check, ..] = *report else { return None };
    if kind != 0x04 || seed.wrapping_sub(first) != check {
        return None;
    }
    match (Key::from_code(first), first, second) {
        (_, 0, _) => Some(Event::Released),
        (Some(Key::Fn), _, 0) => Some(Event::Press { key: Key::Fn, with_fn: false }),
        (Some(Key::Fn), _, code) => {
            Key::from_code(code).map(|key| Event::Press { key, with_fn: true })
        }
        (Some(key), _, _) => Some(Event::Press { key, with_fn: false }),
        (None, _, _) => None,
    }
}

/// What a key does held: a jog, a head or table move, or an output.
enum Held {
    Jog(JogRequest),
    Output(OutputRequest),
    Table(TableRequest),
}

/// The pendant's own state between reports.
struct Pendant {
    /// Fast: X and Y move while held. Step: each press moves one step.
    fast: bool,
    /// The move or output a held key keeps going, its lease and start.
    held: Option<(Lease, Instant)>,
    sequence: u64,
    /// Whether the head was last sent to follow from here.
    following: bool,
}

/// Starts reading the pendant: a thread for the USB device, a task for its keys.
pub fn spawn(shared: Shared) {
    let (events, mut keys) = mpsc::channel(32);
    std::thread::Builder::new().name("pendant".into()).spawn(move || read(&events)).ok();
    tokio::spawn(async move {
        let mut pendant = Pendant { fast: false, held: None, sequence: 0, following: false };
        let mut renew = tokio::time::interval(RENEW);
        loop {
            tokio::select! {
                event = keys.recv() => match event {
                    Some(event) => pendant.event(&shared, event).await,
                    None => break,
                },
                _ = renew.tick() => pendant.renew(&shared).await,
            }
        }
    });
}

impl Pendant {
    async fn event(&mut self, shared: &Shared, event: Event) {
        match event {
            Event::Released | Event::Lost => self.release(shared).await,
            Event::Press { key, with_fn } => {
                self.release(shared).await;
                if let Err(error) = self.press(shared, key, with_fn).await {
                    shared.lock().await.note(format!("Pendant: {error}"), true);
                }
            }
        }
    }

    async fn press(&mut self, shared: &Shared, key: Key, with_fn: bool) -> crate::Result<()> {
        let jog = |axis: usize, positive: bool, fast: bool| JogRequest {
            axis,
            positive,
            step_mm: (!fast).then_some(STEP_MM),
            fast,
            diagonal: None,
        };
        match (key, with_fn) {
            (Key::Left | Key::Right | Key::Up | Key::Down, _) => {
                let (axis, positive) = match key {
                    Key::Left => (0, false),
                    Key::Right => (0, true),
                    Key::Up => (1, true),
                    _ => (1, false),
                };
                let request = jog(axis, positive, self.fast);
                if self.fast {
                    self.hold(shared, Held::Jog(request)).await?;
                } else {
                    machine::jog(shared, request, None).await?;
                }
            }
            (Key::ZUp | Key::ZDown, false) => {
                let output = OutputRequest::HeadJog { up: key == Key::ZUp, fast: self.fast };
                self.hold(shared, Held::Output(output)).await?;
            }
            (Key::ZUp | Key::ZDown, true) => {
                let table = TableRequest { positive: key == Key::ZUp, speed_mm_s: TABLE_MM_S };
                self.hold(shared, Held::Table(table)).await?;
            }
            (Key::RedLight, _) => self.hold(shared, Held::Output(OutputRequest::Pointer)).await?,
            (Key::Shutter, _) => self.hold(shared, Held::Output(OutputRequest::Shutter)).await?,
            (Key::Blow, _) => {
                let (selector, pressure) = shared.lock().await.recipe_gas()?;
                self.hold(shared, Held::Output(OutputRequest::Gas { selector, pressure })).await?;
            }
            (Key::Follow, _) => {
                if self.following {
                    machine::raise_head(shared).await?;
                } else {
                    machine::follow(shared).await?;
                }
                self.following = !self.following;
            }
            (Key::Laser, _) => machine::pulse(shared, PULSE.0, PULSE.1).await?,
            (Key::Frame, _) => machine::frame(shared).await?,
            (Key::GoOrigin, true) => machine::go_origin(shared, false).await?,
            (Key::ZCalibrate, true) => machine::calibrate(shared).await?,
            (Key::GoOrigin | Key::ZCalibrate, false) => {
                return Err(crate::Error::Refused("hold Fn+ with this key".into()));
            }
            (Key::Start, _) => {
                let held = shared.lock().await.held.is_some();
                if held { machine::resume(shared).await? } else { machine::run(shared).await? }
            }
            (Key::Pause, _) => machine::hold(shared).await?,
            (Key::Stop, _) => machine::stop(shared).await?,
            (Key::FastStep, _) => {
                self.fast = !self.fast;
                let mode = if self.fast {
                    "Fast: X and Y move while held"
                } else {
                    "Step: X and Y move 1 mm per press"
                };
                shared.lock().await.note(format!("Pendant {mode}"), false);
            }
            (Key::Fn, _) => {}
        }
        Ok(())
    }

    /// Starts a held move or output under a new lease.
    async fn hold(&mut self, shared: &Shared, what: Held) -> crate::Result<()> {
        self.sequence += 1;
        let lease = Lease { client: "pendant".into(), sequence: self.sequence };
        match what {
            Held::Jog(request) => machine::jog(shared, request, Some(lease.clone())).await?,
            Held::Output(request) => machine::outputs(shared, request, lease.clone()).await?,
            Held::Table(request) => machine::table(shared, request, lease.clone()).await?,
        }
        self.held = Some((lease, Instant::now()));
        Ok(())
    }

    /// Keeps a held move going, or ends it after [`MAX_HELD`].
    async fn renew(&mut self, shared: &Shared) {
        let Some((lease, since)) = &self.held else { return };
        if since.elapsed() >= MAX_HELD {
            self.release(shared).await;
            shared.lock().await.note(
                "Pendant key held over 10 s: the move stopped. Press it again to go on.",
                true,
            );
        } else if machine::heartbeat(shared, lease.clone()).is_err() {
            self.held = None;
        }
    }

    async fn release(&mut self, shared: &Shared) {
        if let Some((lease, _)) = self.held.take() {
            let _ = machine::release(shared, lease).await;
        }
    }
}

/// Reads the receiver for as long as the program runs, finding it again
/// after it is unplugged.
#[cfg(windows)]
fn read(events: &mpsc::Sender<Event>) {
    let Ok(mut api) = hidapi::HidApi::new() else { return };
    loop {
        let _ = api.refresh_devices();
        let device = api
            .device_list()
            .filter(|d| d.vendor_id() == 0x10CE && (0xEB61..=0xEBAA).contains(&d.product_id()))
            .filter(|d| keys(d))
            .find_map(|d| d.open_device(&api).ok());
        let Some(device) = device else {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        tracing::info!("pendant receiver found");
        let mut report = [0u8; 16];
        loop {
            match device.read_timeout(&mut report, 500) {
                Ok(0) => {}
                Ok(n) => {
                    if let Some(event) = decode(&report[..n])
                        && events.blocking_send(event).is_err()
                    {
                        return;
                    }
                }
                Err(_) => {
                    let _ = events.blocking_send(Event::Lost);
                    tracing::warn!("pendant receiver lost");
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Whether this collection of the receiver delivers the key reports: the
/// first does, the second only takes the lamps.
#[cfg(windows)]
fn keys(info: &hidapi::DeviceInfo) -> bool {
    let path = info.path().to_string_lossy().to_lowercase();
    !path.contains("&col") || path.contains("&col01")
}

#[cfg(not(windows))]
fn read(_events: &mpsc::Sender<Event>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_read_as_the_receiver_sent_them() {
        let press = |key, with_fn| Some(Event::Press { key, with_fn });
        assert_eq!(decode(&[0x04, 0x10, 0x01, 0, 0, 0x0F]), press(Key::Follow, false));
        assert_eq!(decode(&[0x04, 0x10, 0x13, 0, 0, 0xFD]), press(Key::Right, false));
        assert_eq!(decode(&[0x04, 0x10, 0x00, 0, 0, 0x10]), Some(Event::Released));
        assert_eq!(decode(&[0x04, 0x10, 0x0B, 0x07, 0, 0x05]), press(Key::GoOrigin, true));
        assert_eq!(decode(&[0x04, 0x10, 0x0B, 0x0C, 0, 0x05]), press(Key::ZCalibrate, true));
        assert_eq!(decode(&[0x04, 0x10, 0x0B, 0, 0, 0x05]), press(Key::Fn, false));
        assert_eq!(decode(&[0x04, 0x10, 0x01, 0, 0, 0x0E]), None, "a wrong check is noise");
        assert_eq!(decode(&[0x05, 0x10, 0x01, 0, 0, 0x0F]), None);
    }
}
