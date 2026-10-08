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

/// A 100 × 60 plate with a hole of radius 8 around (30, 30), as drawn by
/// most CAD programs: the circle starts at its rightmost point.
fn plate() -> Drawing {
    plate_with(Contour {
        layer: "cut".into(),
        curves: vec![Curve::circle(Point::new(30., 30.), 8.)],
    })
}

/// The plate with a square hole drawn from its corner, as a sketch draws one.
fn square_hole() -> Drawing {
    let corners = [[22., 22.], [38., 22.], [38., 38.], [22., 38.]];
    plate_with(Contour {
        layer: "cut".into(),
        curves: (0..4)
            .map(|i| Curve::Line { start: corners[i].into(), end: corners[(i + 1) % 4].into() })
            .collect(),
    })
}

fn plate_with(hole: Contour) -> Drawing {
    let points = [[0., 0.], [100., 0.], [100., 60.], [0., 60.]];
    let outline = Contour {
        layer: "cut".into(),
        curves: (0..4)
            .map(|i| Curve::Line { start: points[i].into(), end: points[(i + 1) % 4].into() })
            .collect(),
    };
    Drawing { contours: vec![outline, hole] }
}

#[test]
fn automatic_leads_go_into_holes_and_out_of_outlines_wherever_the_cut_starts() {
    use openlaser_core::features::StartPosition;
    let in_circle = |p: Point| p.distance(Point::new(30., 30.)) < 8.;
    let in_square = |p: Point| (p.x - 30.).abs() < 8. && (p.y - 30.).abs() < 8.;
    let on_plate = |p: Point| p.x > 0. && p.x < 100. && p.y > 0. && p.y < 60.;
    for position in [StartPosition::Keep, StartPosition::Automatic, StartPosition::Manual(0.3)] {
        for direction in
            [Direction::Keep, Direction::Clockwise, Direction::Counterclockwise, Direction::Reverse]
        {
            for side in [Side::Auto, Side::Inside, Side::Outside] {
                let mut features = Features {
                    leads: Some(Leads {
                        entry: Some(Lead {
                            shape: LeadShape::Line,
                            length: Millimeters(2.),
                            radius: Millimeters(1.),
                            angle: Degrees(45.),
                        }),
                        exit: None,
                        side,
                        closed_only: true,
                        overrides: Vec::new(),
                    }),
                    ..Features::default()
                };
                features.start.position = position;
                features.start.direction = direction;
                for (drawing, in_hole) in
                    [(plate(), &in_circle as &dyn Fn(Point) -> bool), (square_hole(), &in_square)]
                {
                    let path = openlaser_prep::prepare(&drawing, &features).unwrap();
                    for contour in &path.contours {
                        let hole = contour
                            .segments
                            .iter()
                            .filter(|s| s.process.lead.is_none())
                            .all(|s| s.curve.start().x > 1. && s.curve.start().x < 99.);
                        let lead = contour
                            .segments
                            .iter()
                            .find(|s| s.process.lead == Some(LeadRole::Entry))
                            .unwrap()
                            .curve
                            .start();
                        let into_waste = if hole { in_hole(lead) } else { !on_plate(lead) };
                        let expected = match side {
                            Side::Auto => true,
                            Side::Inside => hole,
                            Side::Outside => !hole,
                        };
                        assert_eq!(
                            into_waste, expected,
                            "{position:?} {direction:?} {side:?} hole {hole}: lead from {lead:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_start_on_a_contour_left_out_by_only_these_contours_waits() {
    let mut features = Features { only_contours: Some(vec![1]), ..Features::default() };
    features.start.spots = vec![Spot { contour: 0, fraction: 0.0533 }];
    let path = openlaser_prep::prepare(&plate(), &features).unwrap();
    assert_eq!(path.contours.len(), 1, "only the hole is prepared");
}
