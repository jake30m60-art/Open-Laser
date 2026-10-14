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
