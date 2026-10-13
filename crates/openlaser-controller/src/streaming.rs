// SPDX-License-Identifier: GPL-3.0-or-later

//! Running a program: the vendor's upload discipline and its ending.
//!
//! A fresh job resets the processing counter, clears the FIFO, learns the
//! FIFO's capacity, uploads a first batch, installs the axis mask and
//! starts execution. While running, every poll uploads as many blocks as
//! the reported free space admits, at most twenty. Once every block is
//! acknowledged the host waits for the FIFO to report itself empty and
//! running, stops it, and switches the outputs off. A hold or a stop
//! decelerates the axes, switches everything off, waits for the axes to
//! settle, captures the execution checkpoint, then stops and clears the
//! FIFO.

use crate::bindings::Bindings;
use crate::operations::home::Home;
use crate::operations::raise::Raise;
use crate::operations::{Operation, Step, admit_motion};
use crate::snapshot::Snapshot;
use crate::state::{CheckpointView, OperationKind, ProgramState};
use openlaser_core::LaserMode;
use openlaser_protocol::records::{self, Checkpoint, MAX_BLOCK_WORDS, MAX_FRAMES_PER_DRAIN};
use openlaser_protocol::requests::{self, Write};
use openlaser_protocol::sequences;
use std::time::{Duration, Instant};

/// A stopped program must settle within this host watchdog.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a pause waits for the head to finish rising before it ends
/// with the head still busy.
const RAISE_WAIT: Duration = Duration::from_secs(3);
/// A run's deadline is this many times its predicted duration, plus
/// [`RUN_DEADLINE_MARGIN_S`], so a slow but progressing run is never cut
/// short.
const RUN_DEADLINE_FACTOR: f64 = 3.;
/// Seconds added to every run's deadline, and its shortest deadline.
const RUN_DEADLINE_MARGIN_S: f64 = 60.;
/// Longest deadline of any run, in seconds: one day.
const RUN_DEADLINE_MAX_S: f64 = 86_400.;

/// A program ready to run.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    /// The exact accepted configuration used to build the records.
    pub configuration: crate::session::Configuration,
    /// Machine XY position used to bind the initial approach.
    pub position: [f64; 2],
    /// Raise a controlled head to its reference before the first XY travel.
    pub prepare_head: bool,
    /// The upload blocks, each the record words of one frame.
    pub blocks: Vec<Vec<u32>>,
    /// The laser the program was compiled for.
    pub mode: LaserMode,
    /// The CO2 PWM control type, for the FIFO mask.
    pub co2_pwm_type: u8,
    /// The expected duration, for the watchdog.
    pub expected_seconds: f64,
    /// Whether the program never fires the laser, as a frame: then it runs
    /// as a manual operation, and running-only inputs such as the door
    /// stay unarmed, as Mlaser frames with the door open.
    pub laser_off: bool,
}

impl Program {
    /// Checks the blocks against the vendor's frame limit.
    pub fn validate(&self) -> Result<(), String> {
        if self.blocks.is_empty() {
            return Err("the program is empty".into());
        }
        if self.blocks.iter().any(|block| block.is_empty() || block.len() > MAX_BLOCK_WORDS) {
            return Err("a program block is empty or larger than one frame".into());
        }
        if !self.expected_seconds.is_finite() || self.expected_seconds < 0. {
            return Err("the program's duration is not a number".into());
        }
        if self.position.iter().any(|v| !v.is_finite()) {
            return Err("the program needs a finite initial position".into());
        }
        Ok(())
    }

    /// The FIFO mask for X and Y under this program's laser.
    #[must_use]
    pub fn mask(&self) -> u32 {
        records::fifo_mask(self.mode, self.co2_pwm_type, &[0, 1])
    }
}

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ending {
    /// Every record executed.
    Completed,
    /// Paused; a continuation may follow.
    Held,
    /// Stopped by request.
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Start,
    Priming,
    Uploading,
    Finishing,
    Settling { ending: Ending, stopped: Instant },
    Done(Ending),
}

/// A running program.
#[derive(Debug)]
pub struct Run {
    program: Program,
    capacity: u32,
    uploaded: usize,
    phase: Phase,
    started: Option<Instant>,
    deadline: Duration,
    checkpoint: Option<CheckpointView>,
    executing: Option<Instant>,
    shutdown: sequences::Shutdown,
    head: Option<Home>,
    head_started: bool,
    /// The head raise of a pause, and the gas it holds back.
    raise: Option<Raise>,
    /// How full the FIFO stayed while blocks were still to come.
    watch: FifoWatch,
    /// How fast the head was seen to move while the program ran.
    speed: SpeedWatch,
}

/// The head's speed as the feedback shows it, between polls: what the
/// machine actually did, to set against what the program planned.
#[derive(Debug, Default)]
struct SpeedWatch {
    last: Option<([f64; 2], Instant)>,
    /// Speeds seen, in mm/s, rounded to whole ones, with how often.
    seen: std::collections::BTreeMap<u32, u32>,
}

impl SpeedWatch {
    fn observe(&mut self, position: [f64; 2], now: Instant) {
        if let Some((before, at)) = self.last {
            let seconds = now.duration_since(at).as_secs_f64();
            if seconds > 0.02 {
                let speed = (position[0] - before[0]).hypot(position[1] - before[1]) / seconds;
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a speed"
                )]
                let rounded = (speed / 10.).round() as u32 * 10;
                if rounded > 0 {
                    *self.seen.entry(rounded).or_default() += 1;
                }
            }
        }
        self.last = Some((position, now));
    }

    fn report(&self) {
        let fastest = self.seen.keys().last().copied().unwrap_or(0);
        // The most frequent speeds, which are where the head cruised.
        let mut common: Vec<(u32, u32)> = self.seen.iter().map(|(s, n)| (*s, *n)).collect();
        common.sort_by_key(|c| std::cmp::Reverse(c.1));
        common.truncate(6);
        tracing::info!(fastest, ?common, "head speed seen during the program (mm/s, polls)");
    }
}

/// How full the FIFO stayed while the upload ran: an empty FIFO with
/// blocks still to upload stops the axes and starts them again, which
/// shakes the machine. Logged so a run that shook can be told apart.
#[derive(Debug, Default)]
struct FifoWatch {
    polls: u32,
    /// The fewest bytes queued at any poll.
    least: Option<u32>,
    /// Polls that found the FIFO all but empty.
    starved: u32,
    last_poll: Option<Instant>,
    /// The longest gap between two polls.
    longest_gap: Duration,
    reported: bool,
}

impl FifoWatch {
    /// Below this many bytes queued the axes are about to run dry: about
    /// 20 ms of 0.25 ms steps.
    const LOW: u32 = 1000;

    fn observe(&mut self, queued: u32, now: Instant) {
        self.polls += 1;
        self.least = Some(self.least.map_or(queued, |least| least.min(queued)));
        if let Some(last) = self.last_poll {
            self.longest_gap = self.longest_gap.max(now.duration_since(last));
        }
        self.last_poll = Some(now);
        if queued < Self::LOW {
            self.starved += 1;
            if self.starved <= 5 {
                tracing::warn!(queued, "program FIFO nearly empty while uploading");
            }
        }
    }

    fn report(&self, capacity: u32) {
        tracing::info!(
            polls = self.polls,
            starved = self.starved,
            least_queued = self.least.unwrap_or(0),
            capacity,
            longest_poll_gap_ms = self.longest_gap.as_millis(),
            "program FIFO during upload"
        );
    }
}

impl Run {
    /// A run of `program` in a FIFO of `capacity` bytes.
    pub fn new(program: Program, capacity: u32, bindings: &Bindings) -> Result<Self, String> {
        program.validate()?;
        if capacity < records::MIN_FIFO_CAPACITY {
            return Err("the FIFO capacity is below the vendor's minimum".into());
        }
        let deadline = Duration::from_secs_f64(
            (program.expected_seconds * RUN_DEADLINE_FACTOR + RUN_DEADLINE_MARGIN_S)
                .clamp(RUN_DEADLINE_MARGIN_S, RUN_DEADLINE_MAX_S),
        );
        Ok(Self {
            head: (program.prepare_head && bindings.head_enabled)
                .then(|| Home::new(bindings, true)),
            head_started: false,
            program,
            capacity,
            uploaded: 0,
            phase: Phase::Start,
            started: None,
            deadline,
            checkpoint: None,
            executing: None,
            shutdown: bindings.shutdown.clone(),
            raise: None,
            watch: FifoWatch::default(),
            speed: SpeedWatch::default(),
        })
    }

    /// Blocks acknowledged so far.
    #[must_use]
    pub const fn uploaded(&self) -> usize {
        self.uploaded
    }

    /// Whether a complete feedback capture began after FIFO start was acknowledged.
    /// Until then, the item tag may still belong to the previous program.
    #[must_use]
    pub fn has_started(&self, snapshot: Option<&Snapshot>) -> bool {
        self.executing.is_some_and(|started| snapshot.is_some_and(|s| s.taken > started))
    }

    /// Only the initial head clearance may see through reference warnings.
    #[must_use]
    pub const fn preparing_head(&self) -> bool {
        matches!(self.phase, Phase::Start) && self.head.is_some()
    }

    /// Blocks in the program.
    #[must_use]
    pub fn total(&self) -> usize {
        self.program.blocks.len()
    }

    /// The checkpoint captured by a hold or stop.
    #[must_use]
    pub const fn checkpoint(&self) -> Option<CheckpointView> {
        self.checkpoint
    }

    /// How the run ended, once it has.
    #[must_use]
    pub const fn ending(&self) -> Option<Ending> {
        match self.phase {
            Phase::Done(ending) => Some(ending),
            _ => None,
        }
    }

    /// The program state for publication.
    #[must_use]
    pub const fn state(&self) -> ProgramState {
        match self.phase {
            Phase::Start | Phase::Priming | Phase::Uploading => ProgramState::Running,
            Phase::Finishing => ProgramState::Finishing,
            Phase::Settling { ending, .. } | Phase::Done(ending) => match ending {
                Ending::Completed => ProgramState::Completed,
                Ending::Held => ProgramState::Held,
                Ending::Stopped => ProgramState::Stopped,
            },
        }
    }

    /// Whether the program has yet to enter its ending, including priming.
    #[must_use]
    pub const fn active(&self) -> bool {
        matches!(self.phase, Phase::Start | Phase::Priming | Phase::Uploading | Phase::Finishing)
    }

    /// Begins a hold or a stop: the writes to send now, after which the
    /// run settles and captures its checkpoint.
    ///
    /// Both send the vendor's manual stop, as its Pause and Stop buttons do:
    /// the FIFO stop while the program runs from the FIFO, else the rapid
    /// axis stop; the laser, gas and outputs off; then the head cancel. On
    /// the first head feedback after it the head rises to its safe height,
    /// with `raise`, and a fiber machine is told its mode again. The run
    /// ends once X and Y stop and the head is still, or after `RAISE_WAIT`
    /// with the head still busy. A hold that cannot trust its feedback
    /// passes no `raise`, and the head stays where it is.
    pub fn end(
        &mut self,
        ending: Ending,
        snapshot: &Snapshot,
        raise: bool,
    ) -> Result<Vec<Write>, String> {
        if !self.active() && ending != Ending::Stopped {
            return Err("the program is not running".into());
        }
        let stopped = Instant::now();
        self.phase = Phase::Settling { ending, stopped };
        let fifo_active = snapshot.status.fifo_activity() == 1;
        let writes = sequences::manual_stop(&self.shutdown, fifo_active, snapshot.status.word(5))
            .map_err(|e| e.to_string())?;
        let request = if raise { self.shutdown.raise } else { None };
        self.raise = Some(Raise::new(request, sequences::after_stop(&self.shutdown), stopped));
        Ok(writes)
    }

    /// The uploads the reported free space admits, with their stamps.
    fn drain(&self, snapshot: &Snapshot) -> Vec<Write> {
        let mut free = snapshot.status.fifo_free();
        let mut stamp = snapshot.status.transfer_stamp().wrapping_add(1);
        let mut writes = Vec::new();
        for block in self.program.blocks.iter().skip(self.uploaded).take(MAX_FRAMES_PER_DRAIN) {
            if !records::admits(free, block.len()) {
                break;
            }
            writes.push(requests::program(stamp, block));
            free -= records::block_bytes(block.len());
            stamp = stamp.wrapping_add(1);
        }
        writes
    }
}

impl Run {
    fn start(
        &mut self,
        snapshot: &Snapshot,
        blocked: Option<&str>,
        now: Instant,
    ) -> Result<Step, String> {
        if !self.head_started && snapshot.head.at_upper_reference() {
            self.head = None;
        }
        if let Some(head) = &mut self.head {
            self.head_started = true;
            return match head.step(snapshot, blocked, now)? {
                Step::Finish(writes) => {
                    self.head = None;
                    Ok(if writes.is_empty() { Step::Wait } else { Step::Send(writes) })
                }
                step => Ok(step),
            };
        }
        admit_motion(snapshot, blocked)?;
        let position = snapshot.position_mm().map_err(|e| e.to_string())?;
        let tolerance = 0.5 / f64::from(self.program.configuration.verified.scale);
        if (0..2).any(|axis| (position[axis] - self.program.position[axis]).abs() > tolerance) {
            return Err("the head moved while the program was being built".into());
        }
        self.started = Some(now);
        self.phase = Phase::Priming;
        Ok(Step::Send(vec![requests::process_counter_reset(), requests::fifo_clear()]))
    }

    fn prime(&mut self, snapshot: &Snapshot) -> Result<Step, String> {
        let mut writes = self.drain(snapshot);
        if writes.is_empty() {
            return Err("the FIFO has no space to prime the program".into());
        }
        writes.push(requests::fifo_configure(self.program.mask(), 0));
        writes.push(requests::fifo_start());
        self.phase = Phase::Uploading;
        Ok(Step::Send(writes))
    }

    fn upload(&self, snapshot: &Snapshot) -> Step {
        let writes = self.drain(snapshot);
        if writes.is_empty() { Step::Wait } else { Step::Send(writes) }
    }

    fn finish(&mut self, snapshot: &Snapshot) -> Result<Step, String> {
        if !records::finished(&snapshot.status, self.capacity) {
            return Ok(Step::Wait);
        }
        let mut writes = vec![requests::fifo_stop()];
        writes.extend(
            sequences::shutdown(&self.shutdown, false, false, &[]).map_err(|e| e.to_string())?,
        );
        self.speed.report();
        if !self.watch.reported {
            self.watch.reported = true;
            self.watch.report(self.capacity);
        }
        if let Some(started) = self.started {
            tracing::info!(
                planned_s = self.program.expected_seconds,
                took_s = started.elapsed().as_secs_f64(),
                "program run time"
            );
        }
        self.phase = Phase::Done(Ending::Completed);
        Ok(Step::Finish(writes))
    }

    fn settle(
        &mut self,
        snapshot: &Snapshot,
        now: Instant,
        ending: Ending,
        stopped: Instant,
    ) -> Result<Step, String> {
        if let Some(raise) = &mut self.raise
            && !raise.done()
        {
            let writes = raise.step(snapshot, now);
            if raise.done() {
                // Settling starts over from the raise, on feedback after it.
                self.phase = Phase::Settling { ending, stopped: now };
            }
            return Ok(if writes.is_empty() { Step::Wait } else { Step::Send(writes) });
        }
        if now.saturating_duration_since(stopped) > SETTLE_TIMEOUT {
            return Err("the axes or head did not settle after the stop".into());
        }
        // FIFO activity is a queue state, not proof of physical motion. A
        // stopped queue may still advertise activity until it is cleared.
        // The stop writes have already been acknowledged; use fresh axis
        // speeds/phases and head feedback before saving and clearing it.
        // A rising head finishes on its own; the pause waits only for XY.
        // `stopped` is when the raise went out. A head controller that stays
        // busy after a touch does not hold the pause past RAISE_WAIT.
        let still = if self.raise.is_some() {
            snapshot.xy_stationary()
                && (snapshot.head_idle() || now.saturating_duration_since(stopped) > RAISE_WAIT)
        } else {
            snapshot.stationary()
        };
        if snapshot.taken > stopped && still {
            if self.checkpoint.is_none() {
                let [x, y, _] = snapshot.position_mm().map_err(|e| e.to_string())?;
                let mut checkpoint = Checkpoint::from_status(&snapshot.status);
                if self.executing.is_none() || checkpoint.item == -1 {
                    checkpoint = Checkpoint { item: i32::MIN, progress: 0 };
                }
                self.checkpoint = Some(CheckpointView::new(checkpoint, [x, y]));
            }
            self.phase = Phase::Done(
                if ending == Ending::Held && self.checkpoint.is_some_and(|p| p.item == -2) {
                    Ending::Completed
                } else {
                    ending
                },
            );
            // A stopped FIFO can go on reporting activity until it is
            // cleared, which would hold the next start or a resume as
            // "the axes are not idle"; clear it once the checkpoint is kept.
            return Ok(Step::Finish(vec![requests::fifo_stop(), requests::fifo_clear()]));
        }
        Ok(Step::Wait)
    }
}

impl Operation for Run {
    fn kind(&self) -> OperationKind {
        OperationKind::Program
    }

    fn phase(&self) -> &'static str {
        match self.phase {
            Phase::Start => "starting",
            Phase::Priming => "priming",
            Phase::Uploading => "uploading",
            Phase::Finishing => "finishing",
            Phase::Settling { .. } => "settling",
            Phase::Done(_) => "done",
        }
    }

    fn step(
        &mut self,
        snapshot: &Snapshot,
        blocked: Option<&str>,
        now: Instant,
    ) -> Result<Step, String> {
        if self.started.is_some_and(|started| now.duration_since(started) > self.deadline)
            && self.active()
        {
            return Err("the program did not finish in time".into());
        }
        if matches!(self.phase, Phase::Priming | Phase::Uploading | Phase::Finishing)
            && !snapshot.alarms_clear()
        {
            return Err("controller alarms are active".into());
        }
        if matches!(self.phase, Phase::Uploading | Phase::Finishing)
            && let Ok(position) = snapshot.position_mm()
        {
            self.speed.observe([position[0], position[1]], now);
        }
        match self.phase {
            Phase::Start => self.start(snapshot, blocked, now),
            Phase::Priming => self.prime(snapshot),
            Phase::Uploading => {
                // Only while blocks remain: at the end the FIFO drains anyway.
                if self.uploaded < self.program.blocks.len() {
                    let queued = self.capacity.saturating_sub(snapshot.status.fifo_free());
                    self.watch.observe(queued, now);
                } else if !self.watch.reported {
                    self.watch.reported = true;
                    self.watch.report(self.capacity);
                }
                Ok(self.upload(snapshot))
            }
            Phase::Finishing => self.finish(snapshot),
            Phase::Settling { ending, stopped } => self.settle(snapshot, now, ending, stopped),
            Phase::Done(_) => Ok(Step::Finish(Vec::new())),
        }
    }

    fn cancel(&self) -> Vec<Write> {
        // The stop sequence that follows covers the axes and outputs; the
        // FIFO is discarded so queued enabling records cannot run.
        vec![requests::fifo_stop(), requests::fifo_clear()]
    }

    fn acknowledged(&mut self, write: &Write) {
        if write.address == 102 {
            self.uploaded += 1;
            if self.uploaded == self.total() && self.phase == Phase::Uploading {
                self.phase = Phase::Finishing;
            }
        } else if *write == requests::fifo_start() {
            self.executing = Some(Instant::now());
        }
    }

    fn operation_state(&self) -> u32 {
        if self.program.laser_off {
            return 4;
        }
        match self.phase {
            Phase::Done(Ending::Held) => 3,
            _ => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::tests::{bindings, snapshot_with};

    fn program(blocks: usize) -> Program {
        Program {
            position: [0.; 2],
            prepare_head: false,
            configuration: crate::session::Configuration {
                epoch: 1,
                binding: 1,
                mode: LaserMode::Fiber,
                verified: crate::session::Verified {
                    scale: 1000,
                    cycle_us: 500,
                    banks: [[0; 14]; 5],
                    system: [0; 26],
                    pwm: [0; 2],
                },
            },
            blocks: (0..blocks).map(|i| vec![0x40bba, u32::try_from(i).unwrap(), 0xbb9]).collect(),
            mode: LaserMode::Fiber,
            co2_pwm_type: 0,
            expected_seconds: 1.,
            laser_off: false,
        }
    }

    fn words(step: &Step) -> Vec<(u32, Vec<u32>)> {
        match step {
            Step::Send(writes) | Step::Finish(writes) => {
                writes.iter().map(|w| (w.address, w.words.clone())).collect()
            }
            Step::Wait => Vec::new(),
        }
    }

    fn acknowledged(run: &mut Run, step: &Step) -> Vec<(u32, Vec<u32>)> {
        if let Step::Send(writes) | Step::Finish(writes) = step {
            for write in writes {
                run.acknowledged(write);
            }
        }
        words(step)
    }

    /// A run resets the counter and clears the FIFO, primes with as many
    /// blocks as the free space admits in one drain, installs the mask and
    /// starts; later drains stamp on from the reported stamp; completion
    /// stops the FIFO and switches outputs off.
    #[test]
    fn a_run_follows_the_vendor_upload_discipline() {
        let now = Instant::now();
        let mut run = Run::new(program(25), 100_000, &bindings()).unwrap();
        let idle = snapshot_with(&[(15, 40), (16, 100_000)], &[], &[]);
        assert_eq!(
            words(&run.step(&idle, None, now).unwrap()),
            [(101, vec![9999, 16]), (103, vec![1])]
        );
        let step = run.step(&idle, None, now).unwrap();
        assert_eq!(run.uploaded(), 0, "scheduling a batch does not acknowledge it");
        let primed = acknowledged(&mut run, &step);
        assert_eq!(primed.len(), 22);
        assert_eq!(primed[0], (102, vec![41, 0x40bba, 0, 0xbb9]));
        assert_eq!(primed[19], (102, vec![60, 0x40bba, 19, 0xbb9]));
        assert_eq!(primed[20], (101, vec![9999, 1, 0x6680_0003, 0]));
        assert_eq!(primed[21], (103, vec![2]));
        assert_eq!(run.uploaded(), 20);
        let tight = snapshot_with(&[(15, 60), (16, 2028), (19, 1)], &[], &[]);
        let step = run.step(&tight, None, now).unwrap();
        assert_eq!(acknowledged(&mut run, &step), [(102, vec![61, 0x40bba, 20, 0xbb9])]);
        let roomy = snapshot_with(&[(15, 61), (16, 100_000), (19, 1)], &[], &[]);
        let step = run.step(&roomy, None, now).unwrap();
        assert_eq!(acknowledged(&mut run, &step).len(), 4);
        assert_eq!(run.phase(), "finishing");
        let busy = snapshot_with(&[(16, 99_000), (19, 1)], &[], &[]);
        assert_eq!(run.step(&busy, None, now).unwrap(), Step::Wait);
        let finished = snapshot_with(&[(16, 100_000), (19, 1)], &[], &[]);
        let end = words(&run.step(&finished, None, now).unwrap());
        assert_eq!(end[0], (103, vec![3]));
        assert_eq!(end[1], (101, vec![9999, 3, 0, 0, 0]));
        assert_eq!(run.ending(), Some(Ending::Completed));
    }

    #[test]
    fn previous_program_feedback_is_not_progress_for_a_new_start() {
        let mut run = Run::new(program(2), 100_000, &bindings()).unwrap();
        let previous = snapshot_with(&[(23, u32::MAX - 1)], &[], &[]);
        assert!(!run.has_started(Some(&previous)));
        run.acknowledged(&requests::fifo_start());
        assert!(!run.has_started(Some(&previous)), "start acknowledgement is not fresh feedback");
        assert!(!run.has_started(None));
        let mut current = snapshot_with(&[(23, 0x8000_0000)], &[], &[]);
        current.taken = run.executing.unwrap() + Duration::from_millis(1);
        assert!(run.has_started(Some(&current)));
    }

    /// A hold sends the vendor's manual stop: the FIFO stop while the
    /// program runs from the FIFO, never the rapid stop with it, then the
    /// laser off. On fresh feedback the fiber head is told its mode, and
    /// once the axes settle the checkpoint is captured and the FIFO stopped
    /// and cleared; a stop does the same and ends stopped.
    #[test]
    fn holds_settle_and_capture_the_checkpoint() {
        let now = Instant::now();
        let mut run = Run::new(program(2), 100_000, &bindings()).unwrap();
        let idle = snapshot_with(&[(16, 100_000)], &[], &[]);
        let step = run.step(&idle, None, now).unwrap();
        acknowledged(&mut run, &step);
        let step = run.step(&idle, None, now).unwrap();
        acknowledged(&mut run, &step);
        let running =
            snapshot_with(&[(16, 90_000), (19, 1), (23, 7), (24, 3)], &[(0, 0x0201_0000)], &[]);
        let writes = run.end(Ending::Held, &running, false).unwrap();
        assert_eq!((writes[0].address, writes[0].words.clone()), (103, vec![3]));
        assert!(!writes.iter().any(|w| w.words.starts_with(&[1, 31])), "no rapid stop with it");
        assert_eq!(writes[1].words, vec![9999, 3, 0, 0, 0]);
        assert_eq!(run.step(&running, None, now).unwrap(), Step::Wait);
        let fresh = snapshot_with(&[(19, 1), (23, 7), (24, 3)], &[], &[]);
        assert_eq!(words(&run.step(&fresh, None, now).unwrap()), [(101, vec![118, 5, 0])]);
        let settled = snapshot_with(&[(19, 1), (23, 7), (24, 3)], &[], &[]);
        assert_eq!(settled.class(), openlaser_protocol::feedback::AxisClass::Fifo);
        assert_eq!(
            words(&run.step(&settled, None, now).unwrap()),
            [(103, vec![3]), (103, vec![1])]
        );
        assert_eq!(
            run.checkpoint(),
            Some(CheckpointView::new(Checkpoint { item: 7, progress: 3 }, [0., 0.]))
        );
        assert_eq!(run.ending(), Some(Ending::Held));
        assert_eq!(run.state(), ProgramState::Held);
        run.end(Ending::Stopped, &settled, false).unwrap();
        assert_eq!(run.state(), ProgramState::Stopped);
    }

    #[test]
    fn partial_acknowledgement_counts_only_accepted_blocks_and_empty_priming_fails() {
        let now = Instant::now();
        let idle = snapshot_with(&[(16, 100_000)], &[], &[]);
        for accepted in [0, 1, 9, 19] {
            let mut run = Run::new(program(25), 100_000, &bindings()).unwrap();
            run.step(&idle, None, now).unwrap();
            let Step::Send(writes) = run.step(&idle, None, now).unwrap() else { panic!("batch") };
            for write in writes.iter().take(accepted) {
                run.acknowledged(write);
            }
            assert_eq!(run.uploaded(), accepted);
            assert!(run.executing.is_none());
        }
        let mut run = Run::new(program(1), 100_000, &bindings()).unwrap();
        run.step(&idle, None, now).unwrap();
        assert!(run.step(&snapshot_with(&[(16, 0)], &[], &[]), None, now).is_err());
        assert_eq!(run.uploaded(), 0);
    }

    #[test]
    fn hold_requires_fresh_still_axes_and_head_and_has_a_deadline() {
        let now = Instant::now();
        let idle = snapshot_with(&[(16, 100_000)], &[], &[]);
        let mut run = Run::new(program(2), 100_000, &bindings()).unwrap();
        run.step(&idle, None, now).unwrap();
        let step = run.step(&idle, None, now).unwrap();
        acknowledged(&mut run, &step);
        let writes = run.end(Ending::Held, &idle, false).unwrap();
        assert_eq!(
            writes[0].words,
            vec![1, 31, 2, 5999, 200_000],
            "no FIFO running: the rapid stop"
        );
        assert_eq!(
            run.step(&idle, None, Instant::now()).unwrap(),
            Step::Wait,
            "old idle feedback is insufficient"
        );
        let moving = snapshot_with(&[], &[(1, 1)], &[]);
        assert_eq!(
            words(&run.step(&moving, None, Instant::now()).unwrap()),
            [(101, vec![118, 5, 0])]
        );
        for snapshot in [snapshot_with(&[], &[(1, 1)], &[]), snapshot_with(&[], &[], &[(3, 104)])] {
            assert_eq!(snapshot.class(), openlaser_protocol::feedback::AxisClass::None);
            assert_eq!(run.step(&snapshot, None, Instant::now()).unwrap(), Step::Wait);
            assert_eq!(run.checkpoint(), None);
        }
        assert!(
            run.step(&idle, None, Instant::now() + SETTLE_TIMEOUT + Duration::from_secs(1))
                .is_err()
        );
        assert_eq!(run.checkpoint(), None);

        let mut pending = Run::new(program(2), 100_000, &bindings()).unwrap();
        pending.end(Ending::Held, &idle, false).unwrap();
        pending.step(&snapshot_with(&[], &[], &[]), None, Instant::now()).unwrap();
        pending.step(&snapshot_with(&[], &[], &[]), None, Instant::now()).unwrap();
        assert_eq!(pending.ending(), Some(Ending::Held));
        assert_eq!(pending.checkpoint().unwrap().item, i32::MIN, "restart before the first pass");
    }
}
