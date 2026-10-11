// SPDX-License-Identifier: GPL-3.0-or-later
//! The Follow button over the loopback controller: down to the machine follow
//! height and following, then up again to the safe height.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "known simulator fixtures")]
mod common;

use common::{config, fixture, seed, until};
use openlaser_controller::Simulator;
use openlaser_core::LaserMode;
use openlaser_server::coordinator::{NewRecipe, Values};
use openlaser_server::{Coordinator, connect, machine};

#[tokio::test]
async fn follow_lowers_the_head_and_raise_takes_it_up() {
    let simulator = Simulator::start().await.unwrap();
    let shared = Coordinator::start(config("follow", simulator.config())).unwrap();
    seed(&shared, &simulator).await;
    connect::connect(&shared).await.unwrap();
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;
    let control = simulator.control();
    control.clear_writes();
    machine::follow(&shared).await.unwrap();
    let without_recipe =
        control.writes().into_iter().find(|w| w.words.first() == Some(&104)).unwrap();
    machine::raise_head(&shared).await.unwrap();
    until(&shared, 10, |d| {
        d.machine.operation.is_none()
            && d.machine.feedback.as_ref().is_some_and(|f| f.head.command != 104)
    })
    .await;
    {
        let mut c = shared.lock().await;
        let part = c
            .import_part("line.dxf", &std::fs::read(fixture("dxf/minimal-line.dxf")).unwrap())
            .unwrap();
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Fiber steel".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        c.open_part(&part.id).unwrap();
        c.set_recipe(&recipe.id).unwrap();
    }
    machine::prepare(&shared).await.unwrap();
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;

    let control = simulator.control();
    control.clear_writes();
    machine::follow(&shared).await.unwrap();
    assert!(
        control.writes().iter().any(|w| w.words.first() == Some(&104)),
        "the Follow button sends the head's follow command"
    );
    let with_recipe = control.writes().into_iter().find(|w| w.words.first() == Some(&104)).unwrap();
    assert_eq!(without_recipe, with_recipe, "a selected recipe must not change manual Follow");
    let following = |d: &openlaser_server::document::Document| {
        d.machine.feedback.as_ref().is_some_and(|f| f.head.command == 104 || f.head.height_mm > 1.)
    };
    until(&shared, 10, following).await;

    control.clear_writes();
    machine::raise_head(&shared).await.unwrap();
    assert!(
        control.writes().iter().any(|w| w.words.first() == Some(&103)),
        "raising retracts the head"
    );
    openlaser_server::shutdown(&shared).await.unwrap();
}
