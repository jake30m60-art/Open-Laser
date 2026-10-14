// SPDX-License-Identifier: GPL-3.0-or-later
//! Admission model for the future measured backend. No controller commands.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Idle,
    Measuring,
    Ready,
    Running,
    Paused,
    Failed,
    Stale,
}

#[derive(Clone, Debug)]
pub(crate) struct Workflow {
    pub phase: Phase,
    ticket: u64,
    stamp: String,
    simulated: bool,
    completed_press: u64,
}
impl Default for Workflow {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            ticket: 0,
            stamp: String::new(),
            simulated: true,
            completed_press: 0,
        }
    }
}
impl Workflow {
    pub fn current(&self, ticket: u64) -> bool {
        self.ticket == ticket && self.phase == Phase::Ready
    }
    pub fn begin(&mut self, stamp: String) -> u64 {
        self.ticket += 1;
        self.phase = Phase::Measuring;
        self.stamp = stamp;
        self.simulated = true;
        self.ticket
    }
    pub fn finish(&mut self, ticket: u64, stamp: &str, success: bool, press: u64) -> bool {
        if ticket != self.ticket || self.phase != Phase::Measuring {
            return false;
        }
        self.phase = if self.stamp != stamp {
            Phase::Stale
        } else if success {
            Phase::Ready
        } else {
            Phase::Failed
        };
        self.completed_press = press;
        self.phase == Phase::Ready
    }
    pub fn invalidate(&mut self) {
        self.ticket += 1;
        self.phase = Phase::Stale;
    }
    /// An open or unknown lid is never permission, and closing it is no press.
    #[allow(dead_code)] // exercised offline; no measured backend is installed
    pub fn start(&mut self, stamp: &str, lid_closed: Option<bool>, new_press: u64) -> bool {
        if self.phase != Phase::Ready
            || self.stamp != stamp
            || self.simulated
            || lid_closed != Some(true)
            || new_press <= self.completed_press
        {
            return false;
        }
        self.phase = Phase::Running;
        true
    }
    #[allow(dead_code)]
    pub fn pause(&mut self) {
        if self.phase == Phase::Running {
            self.phase = Phase::Paused;
        }
    }
    #[allow(dead_code)]
    pub fn resume(&mut self, stamp: &str, lid_closed: Option<bool>) -> bool {
        if self.phase != Phase::Paused || stamp != self.stamp || lid_closed != Some(true) {
            return false;
        }
        self.phase = Phase::Running;
        true
    }
    #[allow(dead_code)]
    pub fn new_job(&mut self, every_job: bool) {
        if every_job {
            self.invalidate();
        } else if self.phase == Phase::Running {
            self.phase = Phase::Ready;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn measured() -> Workflow {
        let mut w = Workflow::default();
        let id = w.begin("same".into());
        assert!(w.finish(id, "same", true, 8));
        w.simulated = false;
        w
    }
    #[test]
    fn closing_lid_is_not_a_start_press() {
        let mut w = measured();
        assert!(!w.start("same", Some(false), 9));
        assert!(!w.start("same", None, 9));
        assert!(!w.start("same", Some(true), 8));
        assert!(w.start("same", Some(true), 9));
        assert!(!w.start("same", Some(true), 10));
    }
    #[test]
    fn simulation_cannot_authorize_cutting() {
        let mut w = Workflow::default();
        let id = w.begin("same".into());
        w.finish(id, "same", true, 8);
        assert!(!w.start("same", Some(true), 9));
    }
    #[test]
    fn aborted_failed_and_stale_measurements_cannot_authorize_cutting() {
        let mut w = measured();
        w.invalidate();
        assert!(!w.start("same", Some(true), 9));
        let id = w.begin("same".into());
        w.invalidate();
        assert!(!w.finish(id, "same", true, 9));
        let id = w.begin("same".into());
        assert!(!w.finish(id, "changed", true, 9));
        assert_eq!(w.phase, Phase::Stale);
        let id = w.begin("same".into());
        w.finish(id, "same", false, 9);
        assert_eq!(w.phase, Phase::Failed);
        assert!(!w.start("same", Some(true), 10));
    }
    #[test]
    fn resume_keeps_measurement_new_job_invalidates() {
        let mut w = measured();
        assert!(w.start("same", Some(true), 9));
        w.pause();
        assert!(!w.resume("changed", Some(true)));
        assert!(!w.resume("same", None));
        assert!(w.resume("same", Some(true)));
        w.new_job(true);
        assert_eq!(w.phase, Phase::Stale);
    }
}
