// SPDX-License-Identifier: GPL-3.0-or-later
//! Machining set for a selection of contours reaches those contours only,
//! and only for the parts the selection sets itself.
#![allow(clippy::unwrap_used, reason = "known geometry fixtures")]

use openlaser_core::features::{
    ContourMachining, Features, JointBehaviour, JointPlacement, Joints, Kerf, Lead, LeadShape,
    Leads, Machining, MachiningPart, Side,
};
use openlaser_core::geometry::{Contour, Curve, Drawing, Point};
use openlaser_core::toolpath::{LeadRole, PreparedContour};
use openlaser_core::units::{Degrees, Millimeters};

/// A 10 mm square at `x`, drawn from the middle of its bottom edge.
fn square(x: f64) -> Contour {
    let points = [[x + 5., 0.], [x + 10., 0.], [x + 10., 10.], [x, 10.], [x, 0.]];
    Contour {
        layer: "cut".into(),
        curves: (0..5)
            .map(|i| Curve::Line { start: points[i].into(), end: points[(i + 1) % 5].into() })
            .collect(),
    }
}

fn leads(side: Side) -> Leads {
    Leads {
        entry: Some(Lead {
            shape: LeadShape::Line,
            length: Millimeters(3.),
            radius: Millimeters(1.),
            angle: Degrees(90.),
        }),
        exit: None,
        side,
        closed_only: true,
        clearance: None,
        overrides: Vec::new(),
    }
}

fn joints() -> Joints {
    Joints {
        placement: JointPlacement::Count(2),
        width: Millimeters(0.5),
        minimum_size: Millimeters(0.),
        outer_only: false,
        open_start: false,
        behaviour: JointBehaviour::LaserOff,
        slow_speed: None,
        repierce: false,
    }
}

fn prepared(features: &Features) -> Vec<PreparedContour> {
    let drawing = Drawing { contours: vec![square(0.), square(30.), square(60.)] };
    let mut contours = openlaser_prep::prepare(&drawing, features).unwrap().contours;
    contours.sort_by_key(|c| c.sources[0]);
    contours
}

fn has_entry(contour: &PreparedContour) -> bool {
    contour.segments.iter().any(|s| s.process.lead == Some(LeadRole::Entry))
}

fn entry_start(contour: &PreparedContour) -> Point {
    contour.segments.iter().find(|s| s.process.lead == Some(LeadRole::Entry)).unwrap().curve.start()
}

fn joint_count(contour: &PreparedContour) -> usize {
    contour.segments.iter().filter(|s| s.process.joint).count()
}

fn selection(
    contours: Vec<usize>,
    own: Vec<MachiningPart>,
    machining: Machining,
) -> ContourMachining {
    ContourMachining { contours, own, machining }
}

#[test]
fn joints_on_a_selection_leave_the_other_contours_alone() {
    let features = Features {
        contour_machining: vec![selection(
            vec![1],
            vec![MachiningPart::Joints],
            Machining { joints: Some(joints()), ..Machining::default() },
        )],
        ..Features::default()
    };
    let contours = prepared(&features);
    assert_eq!(contours.iter().map(joint_count).collect::<Vec<_>>()[0], 0);
    assert!(joint_count(&contours[1]) > 0);
    assert_eq!(joint_count(&contours[2]), 0);
}

#[test]
fn leads_off_for_the_job_but_on_for_a_selection_and_the_reverse() {
    let on_here = Features {
        contour_machining: vec![selection(
            vec![2],
            vec![MachiningPart::Leads],
            Machining { leads: Some(leads(Side::Auto)), ..Machining::default() },
        )],
        ..Features::default()
    };
    let contours = prepared(&on_here);
    assert_eq!(contours.iter().map(has_entry).collect::<Vec<_>>(), [false, false, true]);

    let off_here = Features {
        leads: Some(leads(Side::Auto)),
        contour_machining: vec![selection(
            vec![0],
            vec![MachiningPart::Leads],
            Machining::default(),
        )],
        ..Features::default()
    };
    let contours = prepared(&off_here);
    assert_eq!(contours.iter().map(has_entry).collect::<Vec<_>>(), [false, true, true]);
}

#[test]
fn one_contour_takes_its_leads_inside_while_the_others_stay_outside() {
    let features = Features {
        leads: Some(leads(Side::Outside)),
        contour_machining: vec![selection(
            vec![1],
            vec![MachiningPart::Leads],
            Machining { leads: Some(leads(Side::Inside)), ..Machining::default() },
        )],
        ..Features::default()
    };
    let contours = prepared(&features);
    let inside = |p: Point, x: f64| p.x > x && p.x < x + 10. && p.y > 0. && p.y < 10.;
    assert!(!inside(entry_start(&contours[0]), 0.));
    assert!(inside(entry_start(&contours[1]), 30.));
    assert!(!inside(entry_start(&contours[2]), 60.));
}

#[test]
fn a_selection_keeps_the_parts_it_does_not_set_from_the_job() {
    // The selection sets only joints; its contour keeps the job's kerf.
    let kerf = Kerf { width: Millimeters(1.), side: Side::Auto };
    let features = Features {
        kerf: Some(kerf),
        contour_machining: vec![selection(
            vec![0],
            vec![MachiningPart::Joints],
            Machining { joints: Some(joints()), kerf: None, ..Machining::default() },
        )],
        ..Features::default()
    };
    let with = prepared(&features);
    let plain = prepared(&Features { kerf: Some(kerf), ..Features::default() });
    let length = |c: &PreparedContour| c.segments.iter().map(|s| s.curve.length()).sum::<f64>();
    assert!((length(&with[0]) - length(&plain[0])).abs() < 1e-6, "kerf kept");
    assert!(joint_count(&with[0]) > 0);
}

#[test]
fn removed_contours_leave_their_selection_and_overlaps_are_refused() {
    let features = Features {
        contour_machining: vec![
            selection(vec![0, 1], vec![MachiningPart::Kerf], Machining::default()),
            selection(vec![1], vec![MachiningPart::Joints], Machining::default()),
        ],
        ..Features::default()
    };
    let drawing = Drawing { contours: vec![square(0.), square(30.)] };
    assert!(openlaser_prep::prepare(&drawing, &features).is_err());
    let renumbered = features.renumbered_locations(|i| (i != 1).then_some(i));
    assert_eq!(renumbered.contour_machining.len(), 1);
    assert_eq!(renumbered.contour_machining[0].contours, [0]);
}
