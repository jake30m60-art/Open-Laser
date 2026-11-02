// SPDX-License-Identifier: GPL-3.0-or-later

//! One touch of the capacitive edge search, as the vendor software does it
//! (capture of 2026-10-14): the head goes into its edge-search mode and
//! settles 2 mm over the sheet (status 3), XY then travel along one axis
//! until the head reports the edge (status 6), the axes stop, and the
//! controller's own latch (axis word 7) gives where the edge is, so host
//! latency does not matter. The head then lifts to its safe height. No sheet
//! under the head, no edge within reach, an alarm or a timeout end it as a
//! failure with the axes stopped and the head lifted.

use super::{Operation, Step, admit_motion};
use crate::snapshot::Snapshot;
use crate::state::OperationKind;
use openlaser_protocol::requests::{self, Write};
use std::time::{Duration, Instant};

/// Head status while following at the search height.
const SEARCHING: u8 = 3;
/// Head status once it found the edge.
const FOUND: u8 = 6;
/// Head alarm bit 1: on its bottom limit, as with no sheet under it.
const BOTTOM_LIMIT: u32 = 1 << 1;
/// How long the head may take to settle over the sheet.
const SETTLE: Duration = Duration::from_secs(20);
/// How long a search move may take to show motion before rest means it ended.
const START_GRACE: Duration = Duration::from_millis(800);
/// The stop of the capture, `[1, 31, 2, 5999, 200000]`: all axes. The
/// controller refuses `[1, 3, 2, 200000, …]` (machine test 2026-11-02), so
/// the axes ran on past the edge and the head sank over it.
const STOP_MASK: u32 = 0b1_1111;
const STOP_DECELERATION: u32 = 5_999;

/// What one touch searches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Touch {
    /// 0 for X, 1 for Y.
    pub axis: usize,
    /// Signed travel in controller units: the farthest the search goes.
    pub travel: i32,
    /// Search speed in controller units per second (50 mm/s in the capture).
    pub speed: u32,
    /// Acceleration in controller units per second squared.
    pub acceleration: u32,
    /// The height the head lifts to afterwards, in thousandths below its origin.
    pub lift_thousandths: u32,
    /// Its lift speed in tenths.
    pub lift_speed_tenths: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Start,
    Lowering,
    Searching,
    Lifting,
    Done,
}

/// One edge-search touch.
#[derive(Debug)]
pub struct EdgeProbe {
    touch: Touch,
    phase: Phase,
    started: Instant,
    searching: Option<Instant>,
    timeout: Duration,
    edge: Option<[i32; 2]>,
    failure: Option<String>,
}

impl EdgeProbe {
    /// A touch along `touch.axis`.
    #[must_use]
    pub fn new(touch: Touch, now: Instant) -> Self {
        // The whole travel at the search speed, with time to settle and lift.
        let seconds = f64::from(touch.travel.unsigned_abs()) / f64::from(touch.speed.max(1));
        let timeout = SETTLE + Duration::from_secs_f64(seconds.mul_add(2., 10.));
        Self {
            touch,
            phase: Phase::Start,
            started: now,
            searching: None,
            timeout,
            edge: None,
            failure: None,
        }
    }

    /// Where the edge is, X and Y in controller units, once found.
    #[must_use]
    pub const fn edge(&self) -> Option<[i32; 2]> {
        self.edge
    }

    fn ending(&self) -> Vec<Write> {
        vec![
            requests::stop_axes(STOP_MASK, STOP_DECELERATION),
            requests::stop_axes(STOP_MASK, STOP_DECELERATION),
            requests::head_cancel(),
            requests::head_retract(self.touch.lift_speed_tenths, self.touch.lift_thousandths),
        ]
    }

    /// Ends in failure: the axes stop and the head lifts first.
    fn give_up(&mut self, reason: &str) -> Step {
        self.failure = Some(reason.into());
        self.phase = Phase::Lifting;
        Step::Send(self.ending())
    }
}

impl Operation for EdgeProbe {
    fn kind(&self) -> OperationKind {
        OperationKind::EdgeSearch
    }

    fn phase(&self) -> &'static str {
        match self.phase {
            Phase::Start => "starting",
            Phase::Lowering => "lowering",
            Phase::Searching => "searching",
            Phase::Lifting => "lifting",
            Phase::Done => "done",
        }
    }

    fn step(
        &mut self,
        snapshot: &Snapshot,
        blocked: Option<&str>,
        now: Instant,
    ) -> Result<Step, String> {
        let head = &snapshot.head;
        if matches!(self.phase, Phase::Lowering | Phase::Searching) {
            if now.duration_since(self.started) > self.timeout {
                return Ok(self.give_up("the edge search did not finish in time"));
            }
            if head.alarm_word() & BOTTOM_LIMIT != 0 {
                return Ok(self.give_up("no sheet under the head: it reached its bottom limit"));
            }
        }
        match self.phase {
            Phase::Start => {
                if !head.referenced() {
                    return Err("the head has no reference; run Go Origin first".into());
                }
                admit_motion(snapshot, blocked)?;
                self.phase = Phase::Lowering;
                Ok(Step::Send(vec![requests::head_cancel(), requests::head_edge_search()]))
            }
            Phase::Lowering => {
                if head.status_byte() != SEARCHING {
                    if now.duration_since(self.started) > SETTLE {
                        return Ok(self
                            .give_up("the head did not settle over a sheet: no sheet under it?"));
                    }
                    return Ok(Step::Wait);
                }
                let mut delta = [0, 0];
                delta[self.touch.axis] = self.touch.travel;
                let write = requests::move_xy(delta, self.touch.speed, self.touch.acceleration)
                    .map_err(|e| e.to_string())?;
                self.phase = Phase::Searching;
                self.searching = Some(now);
                Ok(Step::Send(vec![write]))
            }
            Phase::Searching => {
                if head.status_byte() == FOUND {
                    let axis = self.touch.axis;
                    let mut at = [snapshot.axes.axis(0).position, snapshot.axes.axis(1).position];
                    at[axis] = snapshot.axes.axis(axis).edge;
                    self.edge = Some(at);
                    self.phase = Phase::Lifting;
                    return Ok(Step::Send(self.ending()));
                }
                if blocked.is_some() {
                    return Ok(self.give_up("an alarm interrupted the edge search"));
                }
                // Once the move had time to start, axes at rest mean it ran its
                // whole travel without meeting an edge.
                let moving = [0, 1].iter().any(|&a| snapshot.axes.axis(a).speed != 0);
                if !moving && self.searching.is_some_and(|t| now.duration_since(t) > START_GRACE) {
                    return Ok(self.give_up("no edge within the search travel"));
                }
                Ok(Step::Wait)
            }
            Phase::Lifting => {
                let still = [0, 1, 3].iter().all(|&a| snapshot.axes.axis(a).speed == 0);
                if still && head.command() != 117 {
                    self.phase = Phase::Done;
                    return match self.failure.take() {
                        Some(reason) => Err(reason),
                        None => Ok(Step::Finish(Vec::new())),
                    };
                }
                Ok(Step::Wait)
            }
            Phase::Done => Ok(Step::Finish(Vec::new())),
        }
    }

    fn cancel(&self) -> Vec<Write> {
        self.ending()
    }
}
