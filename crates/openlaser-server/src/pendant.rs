// SPDX-License-Identifier: GPL-3.0-or-later

//! The wireless Mlaser pendant (XHC, USB vendor 10CE): twenty keys, read
//! as HID input reports `04 seed key1 key2 00 check`. The last byte was
//! `seed - key1` on one day and 0x10 more on another, so it is not relied
//! on: the receiver checks the radio, the USB link is sound. Fn is key 0x0B; with
//! another key held it sends that key second. A held key repeats its
//! report a few times a second; its release sends key 0.
//!
//! Each key calls the same machine actions as the screen's buttons, with
//! their checks, once per press: a repeat of the key held does nothing new.
//! Held moves renew their lease while the repeats come and stop when the key
//! is released, when the receiver disappears, or when no repeat came for
//! [`DEADMAN`]: a pendant out of radio range sends neither.

use crate::coordinator::Shared;
use crate::machine::{self, JogRequest, OutputRequest, TableRequest};
use openlaser_controller::lease::Lease;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// How long a held move runs on without the key's repeat.
pub const DEADMAN: Duration = Duration::from_millis(600);
/// How often a held move's lease is renewed, as the screen's hold does.
const RENEW: Duration = Duration::from_millis(75);
/// One press in Step mode moves X or Y this far.
const STEP_MM: f64 = 1.;
/// The W table's speed when Fn turns Z into W.
const TABLE_MM_S: f64 = 20.;
/// How long Start must be held, as the screen's held buttons.
const START_HOLD: Duration = Duration::from_secs(1);
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
    /// The receiver was found.
    Found,
}

/// Reads one report. Reports of another kind, or naming no key, say nothing.
#[must_use]
pub fn decode(report: &[u8]) -> Option<Event> {
    let [kind, _, first, second, ..] = *report else { return None };
    if kind != 0x04 || report.len() < 6 {
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

/// How the arrows move, as Mlaser's Fast/Step key cycles them. Mlaser
/// starts held and slow (its `IsStepMove` 0, `JogSlowSpeed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// While held, at the slow jog speed.
    Slow,
    /// While held, at the fast jog speed.
    Fast,
    /// One step per press.
    Step,
}

/// What the screen shows of the pendant: whether its receiver is in, how
/// the arrows move, and the key held down.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct PendantView {
    /// Whether the receiver is plugged in and answering.
    pub connected: bool,
    /// How the arrows move: `slow`, `fast` or `step`.
    pub mode: String,
    /// The key held down, by its label, while it is down.
    pub key: Option<String>,
    /// Whether Fn+ is held with it.
    pub with_fn: bool,
    /// Counts the Start presses: the screen answers each as its own Start
    /// button, with the checklist and its countdown.
    pub start: u32,
}

/// The pendant's own state between reports.
struct Pendant {
    mode: Mode,
    /// The move or output a held key keeps going, its lease and start.
    held: Option<(Lease, Instant)>,
    /// The key down and when its report last came.
    down: Option<(Event, Instant)>,
    sequence: u64,
    /// Start presses so far, for the screen.
    start: u32,
    /// Since when Start is held, until it counts as a press.
    start_held: Option<Instant>,
}

/// Starts reading the pendant: a thread for the USB device, a task for its keys.
pub fn spawn(shared: Shared) {
    let (events, mut keys) = mpsc::channel(32);
    std::thread::Builder::new().name("pendant".into()).spawn(move || read(&events)).ok();
    tokio::spawn(async move {
        let mut pendant = Pendant {
            mode: Mode::Slow,
            held: None,
            down: None,
            sequence: 0,
            start: 0,
            start_held: None,
        };
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
    /// Shows the pendant's state on every screen.
    async fn show(&self, shared: &Shared, connected: bool) {
        let key = match self.down {
            Some((Event::Press { key, with_fn }, _)) => Some((format!("{key:?}"), with_fn)),
            _ => None,
        };
        let view = PendantView {
            connected,
            mode: match self.mode {
                Mode::Slow => "slow",
                Mode::Fast => "fast",
                Mode::Step => "step",
            }
            .into(),
            with_fn: key.as_ref().is_some_and(|(_, f)| *f),
            key: key.map(|(k, _)| k),
            start: self.start,
        };
        let mut c = shared.lock().await;
        if c.pendant.as_ref() != Some(&view) {
            c.pendant = Some(view);
            c.publish();
        }
    }

    async fn event(&mut self, shared: &Shared, event: Event) {
        let connected = !matches!(event, Event::Lost);
        self.event_inner(shared, event).await;
        self.show(shared, connected).await;
    }

    async fn event_inner(&mut self, shared: &Shared, event: Event) {
        match event {
            Event::Found => {}
            Event::Released | Event::Lost => {
                self.down = None;
                self.release(shared).await;
            }
            Event::Press { key, with_fn } => {
                // The held key repeating: it keeps its move alive, nothing more.
                if let Some((down, seen)) = &mut self.down
                    && *down == event
                {
                    *seen = Instant::now();
                    return;
                }
                tracing::info!(?key, with_fn, "pendant key");
                self.down = Some((event, Instant::now()));
                self.release(shared).await;
                if let Err(error) = self.press(shared, key, with_fn).await {
                    // A step pressed while the last one still moves is dropped
                    // quietly, as a quick double press would be.
                    let busy =
                        self.mode == Mode::Step && error.to_string().contains("a move is active");
                    if !busy {
                        shared.lock().await.note(format!("Pendant: {error}"), true);
                    }
                }
            }
        }
    }

    async fn press(&mut self, shared: &Shared, key: Key, with_fn: bool) -> crate::Result<()> {
        self.allowed(shared, key, with_fn).await?;
        let mode = self.mode;
        let jog = |axis: usize, positive: bool| JogRequest {
            axis,
            positive,
            step_mm: (mode == Mode::Step).then_some(STEP_MM),
            fast: mode == Mode::Fast,
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
                let request = jog(axis, positive);
                if mode == Mode::Step {
                    machine::jog(shared, request, None).await?;
                } else {
                    self.hold(shared, Held::Jog(request)).await?;
                }
            }
            (Key::ZUp | Key::ZDown, false) => {
                let output =
                    OutputRequest::HeadJog { up: key == Key::ZUp, fast: mode == Mode::Fast };
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
                // As the screen's Follow button: the head's own command says
                // whether it follows now (104 or 117), so a second press raises it.
                let following = shared
                    .machine
                    .state()
                    .feedback
                    .is_some_and(|f| matches!(f.head.command, 104 | 117));
                if following {
                    machine::raise_head(shared).await?;
                } else {
                    machine::follow(shared).await?;
                }
            }
            (Key::Laser, _) => machine::pulse(shared, PULSE.0, PULSE.1).await?,
            (Key::Frame, _) => machine::frame(shared).await?,
            (Key::GoOrigin, true) => machine::go_origin(shared, false).await?,
            (Key::ZCalibrate, true) => machine::calibrate(shared).await?,
            (Key::GoOrigin | Key::ZCalibrate, false) => {
                return Err(crate::Error::Refused("hold Fn+ with this key".into()));
            }
            // The screen starts it as its own Start button: checklist,
            // countdown, then the run or the resume.
            // Held for a second, it counts, as the screen's Start button.
            (Key::Start, _) => self.start_held = Some(Instant::now()),
            (Key::Pause, _) => machine::hold(shared).await?,
            (Key::Stop, _) => machine::stop(shared).await?,
            (Key::FastStep, _) => {
                self.mode = match self.mode {
                    Mode::Slow => Mode::Fast,
                    Mode::Fast => Mode::Step,
                    Mode::Step => Mode::Slow,
                };
                // The top bar and the jog speed button show the new mode.
            }
            (Key::Fn, _) => {}
        }
        Ok(())
    }

    /// Whether the screen's button for this key would be enabled now: the
    /// pendant keeps to the same gates, so it never drives the head into a
    /// limit switch the screen would not.
    async fn allowed(&self, shared: &Shared, key: Key, with_fn: bool) -> crate::Result<()> {
        let c = shared.lock().await;
        let r = c.document().readiness;
        let held = c.held.is_some();
        drop(c);
        let gate = match (key, with_fn) {
            (Key::Left, _) => Some(r.xy_jog[0][0].clone()),
            (Key::Right, _) => Some(r.xy_jog[0][1].clone()),
            (Key::Down, _) => Some(r.xy_jog[1][0].clone()),
            (Key::Up, _) => Some(r.xy_jog[1][1].clone()),
            (Key::ZUp, false) => Some(r.head_up),
            (Key::ZDown, false) => Some(r.head_down),
            (Key::ZUp | Key::ZDown, true) => Some(r.jog),
            (Key::RedLight | Key::Shutter | Key::Blow | Key::Laser, _) => Some(r.outputs),
            (Key::Frame, _) => Some(r.frame),
            (Key::GoOrigin, _) => Some(r.position),
            (Key::ZCalibrate | Key::Follow, _) => Some(r.calibrate),
            (Key::Start, _) => Some(if held { r.resume } else { r.run }),
            (Key::Pause, _) => Some(r.hold),
            (Key::Stop | Key::Fn | Key::FastStep, _) => None,
        };
        match gate {
            Some(g) if !g.ok => {
                Err(crate::Error::Refused(g.reason.unwrap_or_else(|| "not possible now".into())))
            }
            _ => Ok(()),
        }
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

    /// Keeps a held move going while its key repeats, or ends it; counts
    /// Start once it has been held long enough.
    async fn renew(&mut self, shared: &Shared) {
        let start_down = matches!(self.down, Some((Event::Press { key: Key::Start, .. }, seen)) if seen.elapsed() < DEADMAN);
        if !start_down {
            self.start_held = None;
        } else if self.start_held.is_some_and(|since| since.elapsed() >= START_HOLD) {
            self.start_held = None;
            self.start = self.start.wrapping_add(1);
            self.show(shared, true).await;
        }
        let Some((lease, _)) = &self.held else { return };
        if self.down.is_none_or(|(_, seen)| seen.elapsed() >= DEADMAN) {
            self.down = None;
            self.release(shared).await;
            shared.lock().await.note("Pendant out of reach: the move stopped.", true);
            self.show(shared, true).await;
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
            .find_map(|d| {
                d.open_device(&api).ok().map(|dev| (dev, d.path().to_string_lossy().into_owned()))
            });
        let Some((device, path)) = device else {
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        tracing::info!(%path, "pendant receiver found");
        if events.blocking_send(Event::Found).is_err() {
            return;
        }
        let mut report = [0u8; 16];
        let mut first = true;
        let mut unknown = 0u32;
        loop {
            match device.read_timeout(&mut report, 500) {
                Ok(0) => {}
                Ok(n) => {
                    if first {
                        first = false;
                        tracing::info!(report = ?&report[..n], "pendant reports arriving");
                    }
                    if decode(&report[..n]).is_none() && unknown < 20 {
                        unknown += 1;
                        tracing::info!(report = ?&report[..n], "pendant report not understood");
                    }
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
        // As the receiver sent them on 12 October, the last byte 0x10 higher.
        assert_eq!(decode(&[0x04, 0x10, 0x0B, 0, 0, 0x15]), press(Key::Fn, false));
        assert_eq!(decode(&[0x04, 0x10, 0x01, 0, 0, 0x1F]), press(Key::Follow, false));
        assert_eq!(decode(&[0x04, 0x10, 0x14, 0, 0, 0x0C]), None, "no such key");
        assert_eq!(decode(&[0x04, 0x10, 0x01, 0]), None, "too short");
        assert_eq!(decode(&[0x05, 0x10, 0x01, 0, 0, 0x0F]), None);
    }
}
