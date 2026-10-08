// SPDX-License-Identifier: GPL-3.0-or-later
//! The lead side setting puts the entry lead inside or outside the contour.
#![allow(clippy::unwrap_used, reason = "known geometry fixtures")]

use openlaser_core::features::{Direction, Features, Kerf, Lead, LeadShape, Leads, Side, Spot};
use openlaser_core::geometry::{Contour, Curve, Drawing, Point};
use openlaser_core::toolpath::LeadRole;
use openlaser_core::units::{Degrees, Millimeters};

/// A square drawn from the middle of its bottom edge, so the leads start
/// away from a corner.
fn square() -> Contour {
    let points = [[5., 0.], [10., 0.], [10., 10.], [0., 10.], [0., 0.]];
    Contour {
        layer: "cut".into(),
        curves: (0..5)
            .map(|i| Curve::Line { start: points[i].into(), end: points[(i + 1) % 5].into() })
            .collect(),
    }
}

fn entry_start(features: &Features) -> Point {
    let path = openlaser_prep::prepare(&Drawing { contours: vec![square()] }, features).unwrap();
    let contour = &path.contours[0];
    contour.segments.iter().find(|s| s.process.lead == Some(LeadRole::Entry)).unwrap().curve.start()
}

fn inside(p: Point) -> bool {
    p.x > 0. && p.x < 10. && p.y > 0. && p.y < 10.
}

#[test]
fn a_chosen_start_point_moves_the_pierce_with_and_without_leads() {
    // Halfway along the square drawn from (5, 0) is the top edge's middle.
    for leads in [false, true] {
        let mut features = Features::default();
        if leads {
            features.leads = Some(Leads {
                entry: Some(Lead {
                    shape: LeadShape::Line,
                    length: Millimeters(3.),
                    radius: Millimeters(1.),
                    angle: Degrees(90.),
                }),
                exit: None,
                side: Side::Auto,
                closed_only: true,
                overrides: Vec::new(),
            });
        }
        features.start.spots = vec![Spot { contour: 0, fraction: 0.5 }];
        let path =
            openlaser_prep::prepare(&Drawing { contours: vec![square()] }, &features).unwrap();
        let first_cut =
            path.contours[0].segments.iter().find(|s| s.process.lead.is_none()).unwrap();
        let at = first_cut.curve.start();
        assert!(
            (at.x - 5.).abs() < 1e-6 && (at.y - 10.).abs() < 1e-6,
            "cut starts at {at:?} (leads {leads})"
        );
    }
}

#[test]
fn side_moves_the_entry_lead_in_and_out() {
    for kerf in [None, Some(Kerf { width: Millimeters(0.2), side: Side::Auto })] {
        for direction in [Direction::Keep, Direction::Reverse] {
            let mut features = Features {
                leads: Some(Leads {
                    entry: Some(Lead {
                        shape: LeadShape::Line,
                        length: Millimeters(3.),
                        radius: Millimeters(1.),
                        angle: Degrees(90.),
                    }),
                    exit: None,
                    side: Side::Inside,
                    closed_only: true,
                    overrides: Vec::new(),
                }),
                kerf,
                ..Features::default()
            };
            features.start.direction = direction;
            let a = entry_start(&features);
            assert!(inside(a), "inside lead starts at {a:?} ({kerf:?}, {direction:?})");
            features.leads.as_mut().unwrap().side = Side::Outside;
            let b = entry_start(&features);
            assert!(!inside(b), "outside lead starts at {b:?} ({kerf:?}, {direction:?})");
        }
    }
}
