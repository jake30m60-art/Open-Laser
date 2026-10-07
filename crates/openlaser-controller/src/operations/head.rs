// SPDX-License-Identifier: GPL-3.0-or-later

//! The operator's Follow button, as Mlaser's: one head command lowers the
//! head to the cut height and keeps it following the sheet; pressed again,
//! the head retracts to the safe height. The head controller does the
//! following on its own, so the operation ends once the command is
//! acknowledged, as the vendor's button does.

use super::{Operation, Step, admit, admit_motion};
use crate::snapshot::Snapshot;
use crate::state::OperationKind;
use openlaser_protocol::requests::{self, Write};
use std::time::Instant;

/// What the head is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadAction {
    /// Follow the sheet at the height a cut uses.
    Follow {
        /// Follow speed in tenths.
        speed_tenths: u32,
        /// The follow target word, as a cut's follow record carries it.
        height: u32,
    },
    /// Retract to the safe height the stop raises to, as configured.
    Raise,
    /// Retract to this height.
    Retract {
        /// Retract speed in tenths.
        speed_tenths: u32,
        /// Thousandths below the head's origin.
        height_thousandths: u32,
    },
}

impl HeadAction {
    fn write(self) -> Write {
        match self {
            Self::Follow { speed_tenths, height } => requests::head_follow(speed_tenths, height),
            Self::Retract { speed_tenths, height_thousandths } => {
                requests::head_retract(speed_tenths, height_thousandths)
            }
            // Resolved to a retract before an operation is made.
            Self::Raise => requests::head_cancel(),
        }
    }
}

/// One head command from the operator.
#[derive(Debug)]
pub struct Head {
    action: HeadAction,
    sent: bool,
    acknowledged: bool,
}

impl Head {
    /// A head command not yet sent.
    #[must_use]
    pub const fn new(action: HeadAction) -> Self {
        Self { action, sent: false, acknowledged: false }
    }
}

impl Operation for Head {
    fn kind(&self) -> OperationKind {
        OperationKind::Head
    }

    fn phase(&self) -> &'static str {
        match (self.sent, self.acknowledged) {
            (false, _) => "starting",
            (true, false) => "requested",
            (true, true) => "done",
        }
    }

    fn step(
        &mut self,
        snapshot: &Snapshot,
        blocked: Option<&str>,
        _now: Instant,
    ) -> Result<Step, String> {
        if self.acknowledged {
            return Ok(Step::Finish(Vec::new()));
        }
        if self.sent {
            return Ok(Step::Wait);
        }
        if !snapshot.head.referenced() {
            return Err("the head has no reference; run Go Origin first".into());
        }
        if snapshot.status.fifo_activity() == 1 {
            return Err("a program is running".into());
        }
        // Following needs idle axes and head; retracting goes up, away from
        // the sheet, and ends a follow, so it only needs the alarms clear.
        match self.action {
            HeadAction::Follow { .. } => admit_motion(snapshot, blocked)?,
            HeadAction::Raise | HeadAction::Retract { .. } => admit(snapshot, blocked, 0)?,
        }
        self.sent = true;
        Ok(Step::Send(vec![self.action.write()]))
    }

    fn cancel(&self) -> Vec<Write> {
        vec![requests::head_cancel()]
    }

    fn acknowledged(&mut self, write: &Write) {
        if *write == self.action.write() {
            self.acknowledged = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::tests::snapshot_with;

    #[test]
    fn follow_sends_one_command_and_ends_when_it_is_acknowledged() {
        let action = HeadAction::Follow { speed_tenths: 500, height: 1000 };
        let mut head = Head::new(action);
        let idle = snapshot_with(&[], &[], &[(2, 0x8000_0001)]);
        let now = Instant::now();
        match head.step(&idle, None, now).unwrap() {
            Step::Send(writes) => assert_eq!(writes, vec![requests::head_follow(500, 1000)]),
            other => panic!("expected the follow command, got {other:?}"),
        }
        assert!(matches!(head.step(&idle, None, now).unwrap(), Step::Wait));
        head.acknowledged(&requests::head_follow(500, 1000));
        assert!(matches!(head.step(&idle, None, now).unwrap(), Step::Finish(_)));
    }

    #[test]
    fn an_unreferenced_head_does_not_follow() {
        let mut head = Head::new(HeadAction::Follow { speed_tenths: 500, height: 1000 });
        let unreferenced = snapshot_with(&[], &[], &[(2, 0x0000_0001)]);
        assert!(head.step(&unreferenced, None, Instant::now()).is_err());
    }
}
