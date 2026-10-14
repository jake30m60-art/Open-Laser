// SPDX-License-Identifier: GPL-3.0-or-later
//! A held jog takes the other axis while it runs and lets either go on its
//! own, as Mlaser's jog keys: X runs, Y joins, X is let go and Y keeps on.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "known simulator fixtures")]
mod common;

use common::{start, until};
use openlaser_controller::lease::Lease;
use openlaser_server::machine::JogRequest;
use openlaser_server::{connect, machine};
use std::time::Duration;

#[tokio::test]
async fn a_second_axis_joins_a_held_jog_and_each_is_let_go_alone() {
    let (simulator, shared) = start("jog-axes").await;
    let control = simulator.control();
    connect::connect(&shared).await.unwrap();
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;
    let lease = Lease { client: "pendant-test".into(), sequence: 1 };
    let beat = {
        let (shared, lease) = (shared.clone(), lease.clone());
        tokio::spawn(async move {
            loop {
                let _ = machine::heartbeat(&shared, lease.clone());
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
    };
    let at = || control.view().position_mm;
    let request =
        JogRequest { axis: 0, positive: true, step_mm: None, fast: false, diagonal: None };
    machine::jog(&shared, request, Some(lease.clone())).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let before = at();
    assert!(before[0] > 0.5, "X runs: {before:?}");

    // Y joins without X stopping.
    machine::jog_axis(&shared, lease.clone(), 1, Some(true)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let both = at();
    assert!(both[0] > before[0] + 1., "X kept running: {before:?} → {both:?}");
    assert!(both[1] > before[1] + 1., "Y joined: {before:?} → {both:?}");

    // X is let go: it stops, Y keeps on.
    machine::jog_axis(&shared, lease.clone(), 0, None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let parted = at();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let later = at();
    assert!((later[0] - parted[0]).abs() < 0.01, "X stopped: {parted:?} → {later:?}");
    assert!(later[1] > parted[1] + 1., "Y kept running: {parted:?} → {later:?}");

    // Letting go of the last stops the whole jog.
    machine::release(&shared, lease).await.unwrap();
    beat.abort();
    until(&shared, 10, |d| {
        d.machine.operation.is_none() && d.machine.feedback.as_ref().is_some_and(|f| f.stationary)
    })
    .await;
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn a_jog_after_a_job_takes_the_head_up_first() {
    use openlaser_controller::state::ProgramState;
    use openlaser_core::LaserMode;
    use openlaser_server::coordinator::{NewRecipe, Values};
    let (simulator, shared) = start("jog-after-job").await;
    let control = simulator.control();
    connect::connect(&shared).await.unwrap();
    let pixels = vec![0u8; 16 * 8];
    {
        let mut c = shared.lock().await;
        let part = c
            .library
            .add_image_part("black.png", &pixels, (16, 8), pixels.clone(), [8., 4.])
            .unwrap();
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Steel".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        c.open_part(&part.id).unwrap();
        c.set_recipe(&recipe.id).unwrap();
    }
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;
    machine::calibrate(&shared).await.unwrap();
    until(&shared, 10, |d| d.calibration.current && d.machine.operation.is_none()).await;
    machine::prepare(&shared).await.unwrap();
    shared.lock().await.set_origin([300., 300.]).unwrap();
    machine::compile(&shared, false).await.unwrap();
    common::run(&shared).await.unwrap();
    until(&shared, 120, |d| {
        d.machine.program.as_ref().is_some_and(|p| p.state == ProgramState::Completed)
            && d.machine.operation.is_none()
    })
    .await;
    // The job left the head at its finishing height, below its reference.
    let low = control.view().position_mm[3];
    let before = control.view().position_mm[0];
    let request =
        JogRequest { axis: 0, positive: true, step_mm: Some(2.), fast: false, diagonal: None };
    machine::jog(&shared, request, None).await.unwrap();
    until(&shared, 20, |d| {
        d.machine.operation.is_none() && d.machine.feedback.as_ref().is_some_and(|f| f.stationary)
    })
    .await;
    let after = control.view().position_mm;
    assert!((after[0] - before - 2.).abs() < 0.01, "X stepped 2 mm: {before} → {}", after[0]);
    assert!(after[3] < low || low < 1., "the head went up first: {low} → {}", after[3]);
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn the_z_keys_stop_at_the_head_soft_limits_once_referenced() {
    use openlaser_server::machine::OutputRequest;
    let (simulator, shared) = start("z-soft-limits").await;
    let control = simulator.control();
    connect::connect(&shared).await.unwrap();
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;
    let hold = |up: bool, sequence: u64| {
        let shared = shared.clone();
        async move {
            let lease = Lease { client: "z-test".into(), sequence };
            machine::outputs(&shared, OutputRequest::HeadJog { up, fast: true }, lease.clone())
                .await
                .unwrap();
            for _ in 0..80 {
                let _ = machine::heartbeat(&shared, lease.clone());
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let _ = machine::release(&shared, lease).await;
            until(&shared, 10, |d| d.machine.operation.is_none()).await;
        }
    };
    hold(false, 1).await;
    let low = control.view().position_mm[3];
    assert!((low - 63.).abs() < 0.1, "down stops at 63 mm, not on the limit: {low}");
    assert_eq!(
        shared.lock().await.document().machine.alarms.iter().filter(|a| a.blocking).count(),
        0
    );
    hold(true, 2).await;
    let high = control.view().position_mm[3];
    assert!(high.abs() < 0.1, "up stops at the reference, not past it: {high}");
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn a_floating_origin_follows_the_head_and_setting_it_makes_it_absolute() {
    use openlaser_core::LaserMode;
    use openlaser_server::coordinator::{NewRecipe, Values};
    use openlaser_server::placement::{self, PlacementChange, PlacementMode};
    let (_simulator, shared) = start("floating-origin").await;
    connect::connect(&shared).await.unwrap();
    let pixels = vec![0u8; 16 * 8];
    {
        let mut c = shared.lock().await;
        let part = c
            .library
            .add_image_part("black.png", &pixels, (16, 8), pixels.clone(), [8., 4.])
            .unwrap();
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Steel".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        c.open_part(&part.id).unwrap();
        c.set_recipe(&recipe.id).unwrap();
    }
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;
    let settled = |shared: &openlaser_server::coordinator::Shared| {
        let shared = shared.clone();
        async move {
            until(&shared, 20, |d| {
                d.machine.operation.is_none()
                    && d.machine.feedback.as_ref().is_some_and(|f| f.stationary)
            })
            .await;
        }
    };
    machine::prepare(&shared).await.unwrap();
    // A reference point off the drawing's zero, as a real part has: the
    // origin is then a sum that need not round back to the head exactly.
    shared.lock().await.set_origin_point(Some([3.3, 1.7])).unwrap();
    let revision = shared.lock().await.document().draft_revision;
    placement::change(&shared, PlacementChange::Head {}, revision).await.unwrap();
    machine::go_xy(&shared, [200.123, 150.457]).await.unwrap();
    settled(&shared).await;
    placement::prepare(&shared).await.unwrap();
    let first = shared.lock().await.document().draft.unwrap().origin.unwrap();
    // A start prepares again without the head moving: the job stays as
    // reviewed, so the checklist's approval still holds.
    let reviewed = shared.lock().await.document().draft_revision;
    placement::prepare(&shared).await.unwrap();
    assert_eq!(shared.lock().await.document().draft_revision, reviewed, "the job changed unmoved");
    assert!((first[0] - 200.123).abs() < 0.01 && (first[1] - 150.457).abs() < 0.01, "{first:?}");
    // Moved on: the next start takes the head where it stands now.
    machine::go_xy(&shared, [260., 180.]).await.unwrap();
    settled(&shared).await;
    placement::prepare(&shared).await.unwrap();
    let draft = shared.lock().await.document().draft.unwrap();
    let second = draft.origin.unwrap();
    assert!((second[0] - 260.).abs() < 0.01 && (second[1] - 180.).abs() < 0.01, "{second:?}");
    assert_eq!(draft.placement.mode, PlacementMode::Head);
    // Setting the origin makes it absolute: it no longer follows the head.
    let revision = shared.lock().await.document().draft_revision;
    placement::change(&shared, PlacementChange::SetOrigin {}, revision).await.unwrap();
    machine::go_xy(&shared, [300., 200.]).await.unwrap();
    settled(&shared).await;
    placement::prepare(&shared).await.unwrap();
    let draft = shared.lock().await.document().draft.unwrap();
    assert_eq!(draft.placement.mode, PlacementMode::Fixed);
    let kept = draft.origin.unwrap();
    assert!((kept[0] - 260.).abs() < 0.01 && (kept[1] - 180.).abs() < 0.01, "{kept:?}");
    openlaser_server::shutdown(&shared).await.unwrap();
}
