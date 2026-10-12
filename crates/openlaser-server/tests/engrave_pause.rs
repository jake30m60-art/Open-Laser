// SPDX-License-Identifier: GPL-3.0-or-later
//! A picture engraving paused mid-line resumes in flight over the
//! loopback controller: back to the turn that began the line, the beam off
//! up to where it stopped, then marking on at full speed.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "known simulator fixtures")]
mod common;

use common::{start, until};
use openlaser_controller::state::ProgramState;
use openlaser_core::LaserMode;
use openlaser_server::coordinator::{NewRecipe, Values};
use openlaser_server::{connect, machine};
use std::time::Duration;

/// Where the head was and whether the beam was on, sampled as it ran.
#[derive(Clone, Copy, Debug)]
struct Seen {
    at: [f64; 2],
    lit: bool,
}

fn watch(
    control: openlaser_controller::simulator::Control,
) -> (std::sync::Arc<std::sync::Mutex<Vec<Seen>>>, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (s, d) = (seen.clone(), done.clone());
    std::thread::spawn(move || {
        while !d.load(std::sync::atomic::Ordering::Relaxed) {
            let view = control.view();
            let lit = view.pwm.iter().any(|p| p[1] > 0);
            s.lock().unwrap().push(Seen { at: [view.position_mm[0], view.position_mm[1]], lit });
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    (seen, done)
}

#[tokio::test]
async fn a_paused_engraving_resumes_in_flight_from_its_line_start() {
    let (simulator, shared) = start("engrave-pause").await;
    let control = simulator.control();
    connect::connect(&shared).await.unwrap();
    // A black picture, 32 by 8 mm: every line marks end to end.
    let pixels = vec![0u8; 64 * 16];
    {
        let mut c = shared.lock().await;
        let part = c
            .library
            .add_image_part("black.png", &pixels, (64, 16), pixels.clone(), [32., 8.])
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

    let (seen, done) = watch(control.clone());
    common::run(&shared).await.unwrap();
    // Into the picture, then pause while a line marks.
    until(&shared, 60, |_| seen.lock().unwrap().iter().filter(|s| s.lit).count() > 400).await;
    machine::hold(&shared).await.unwrap();
    until(&shared, 20, |d| d.can_resume && d.machine.operation.is_none()).await;
    let paused = seen.lock().unwrap().len();
    let held_at = {
        let all = seen.lock().unwrap();
        all.iter().rev().find(|s| s.lit).map(|s| s.at).unwrap()
    };
    common::resume(&shared).await.unwrap();
    until(&shared, 120, |d| {
        d.machine.program.as_ref().is_some_and(|p| p.state == ProgramState::Completed)
            && d.machine.operation.is_none()
    })
    .await;
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let all = seen.lock().unwrap().clone();
    let after = &all[paused..];
    let first_lit = after.iter().position(|s| s.lit).expect("the resume marks again");
    let mark = after[first_lit].at;
    // The first mark after the resume is where the pause left off.
    let gap = (mark[0] - held_at[0]).hypot(mark[1] - held_at[1]);
    assert!(gap < 1.5, "resumed marking at {mark:?}, paused at {held_at:?}");
    // It got there at speed: the head moved well before the beam came on,
    // along the same line, not from rest at that spot.
    let run_in = after[..first_lit]
        .iter()
        .rev()
        .take(20)
        .map(|s| (s.at[0] - mark[0]).hypot(s.at[1] - mark[1]))
        .fold(0., f64::max);
    eprintln!(
        "paused at {held_at:?}, marked again at {mark:?} ({gap:.3} mm), moving {run_in:.2} mm before"
    );
    assert!(run_in > 0.5, "the head stood at the stop before marking: {run_in}");
    openlaser_server::shutdown(&shared).await.unwrap();
}
