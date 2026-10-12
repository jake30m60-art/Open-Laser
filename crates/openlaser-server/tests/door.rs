// SPDX-License-Identifier: GPL-3.0-or-later
//! An open door with the door alarm set to running only, as Mlaser offers
//! it: homing, moving and following work, the laser does not fire.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "known simulator fixtures")]
mod common;

use common::{config, fixture, scratch, seed, until};
use openlaser_controller::Simulator;
use openlaser_server::bindings::Files;
use openlaser_server::{Coordinator, connect, machine};

#[tokio::test]
async fn an_open_door_stops_only_the_laser() {
    // The fixture's door on input 4, armed while running only.
    let xml = std::fs::read_to_string(fixture("xml/harness-backup.xml")).unwrap();
    let xml = xml.replace("AlarmDIStr='Door#4#1#0,", "AlarmDIStr='Door#4#1#1,");
    let backup = scratch("door-backup").join("backup.xml");
    std::fs::write(&backup, xml).unwrap();
    let simulator = Simulator::start().await.unwrap();
    let mut configuration = config("door", simulator.config());
    configuration.files = Files { backup: Some(backup) };
    let shared = Coordinator::start(configuration).unwrap();
    seed(&shared, &simulator).await;
    // Door open: the active-low input low.
    simulator.control().input(4, Some(false));
    connect::connect(&shared).await.unwrap();
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;
    machine::follow(&shared).await.unwrap();
    machine::raise_head(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.operation.is_none()).await;

    let error = machine::pulse(&shared, 100, 10).await.unwrap_err().to_string();
    assert!(error.contains("door"), "{error}");
    let error = machine::run(&shared).await.unwrap_err().to_string();
    assert!(error.contains("door"), "{error}");
    let readiness = shared.lock().await.document().readiness.clone();
    assert!(!readiness.run.ok);
    assert!(readiness.home.ok, "{:?}", readiness.home);

    // Door shut: the laser's gate no longer names it.
    simulator.control().input(4, Some(true));
    until(&shared, 10, |d| d.readiness.run.reason.as_deref().is_none_or(|r| !r.contains("door")))
        .await;
}
