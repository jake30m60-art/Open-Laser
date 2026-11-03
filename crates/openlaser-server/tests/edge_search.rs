// SPDX-License-Identifier: GPL-3.0-or-later
//! Measuring a sheet by its edges over the loopback controller: from where
//! the head stands, the touches find the sheet the simulator lays down.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "known simulator fixtures")]
mod common;

use common::{config, seed, until};
use openlaser_controller::Simulator;
use openlaser_server::edge_search::{MeasureRequest, SheetShape, measure};
use openlaser_server::{Coordinator, connect, machine};

async fn ready(name: &str, at: [f64; 2]) -> (Simulator, openlaser_server::coordinator::Shared) {
    let simulator = Simulator::start().await.unwrap();
    let shared = Coordinator::start(config(name, simulator.config())).unwrap();
    seed(&shared, &simulator).await;
    connect::connect(&shared).await.unwrap();
    machine::home(&shared).await.unwrap();
    until(&shared, 10, |d| d.machine.session.homed && d.machine.operation.is_none()).await;
    machine::go_xy(&shared, at).await.unwrap();
    until(&shared, 60, |d| {
        d.machine.operation.is_none()
            && d.machine.feedback.as_ref().is_some_and(|f| {
                (f.position_mm[0] - at[0]).abs() < 0.01 && (f.position_mm[1] - at[1]).abs() < 0.01
            })
    })
    .await;
    (simulator, shared)
}

#[tokio::test]
async fn four_touches_measure_the_sheet_corner() {
    let (simulator, shared) = ready("edge-rect", [60., 60.]).await;
    simulator.control().sheet_bounds([20., 20.], [700., 500.]);
    let request = MeasureRequest {
        shape: SheetShape::Rectangle,
        spacing_mm: 100.,
        search_mm: 200.,
        correction_mm: [0., 0.],
        bore_mm: None,
    };
    let measured = measure(&shared, request).await.expect("a measurement");
    assert_eq!(measured.points.len(), 4);
    let corner = measured.corner.unwrap();
    assert!((corner[0] - 20.).abs() < 0.01 && (corner[1] - 20.).abs() < 0.01, "{corner:?}");
    assert!(measured.angle_deg.unwrap().abs() < 0.01);
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn three_touches_measure_a_disc() {
    let (simulator, shared) = ready("edge-disc", [200., 200.]).await;
    // The simulator's sheets are rectangles: a square stands in for the disc
    // here, so the circle through the three points is the square's.
    simulator.control().sheet_bounds([150., 150.], [250., 250.]);
    let request = MeasureRequest {
        shape: SheetShape::Disc,
        spacing_mm: 100.,
        search_mm: 200.,
        correction_mm: [0., 0.],
        bore_mm: None,
    };
    let measured = measure(&shared, request).await.expect("a measurement");
    assert_eq!(measured.points.len(), 3);
    let centre = measured.center.unwrap();
    assert!((centre[0] - 200.).abs() < 0.01, "{centre:?}");
    openlaser_server::shutdown(&shared).await.unwrap();
}
