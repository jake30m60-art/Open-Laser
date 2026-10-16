// SPDX-License-Identifier: GPL-3.0-or-later
use super::geometry::*;
use openlaser_core::geometry::{Contour, Curve, Drawing, Point};

fn rectangle() -> Template {
    Template {
        remnant: None,
        shape: Shape::Rectangle { width: 600., height: 500. },
        picks: vec![
            Pick { contour: 0, curve: 0, t: 0.3 },
            Pick { contour: 0, curve: 0, t: 0.7 },
            Pick { contour: 0, curve: 1, t: 0.5 },
            Pick { contour: 0, curve: 2, t: 0.3 },
            Pick { contour: 0, curve: 2, t: 0.7 },
            Pick { contour: 0, curve: 3, t: 0.5 },
        ],
        drawing_point: [300., 250.],
        machine_point: [700., 600.],
        search_mm: 50.,
        angle_deg: 5.,
        tolerance_mm: 0.1,
        every_job: true,
    }
}
#[test]
fn displaced_rotated_edges_fit_without_assuming_point_correspondence() {
    let t = rectangle();
    let plan = t.plan().unwrap();
    for delta in [[-50., -50.], [-50., 50.], [50., -50.], [50., 50.], [0., 0.]] {
        for angle in [-5., 0., 5.] {
            let f = simulate(&t, &plan, delta, angle, Fault::None).unwrap();
            let p = f.pose.apply(t.drawing_point.into());
            assert!(p.distance(Point::new(700. + delta[0], 600. + delta[1])) < 1e-6);
            assert!((f.pose.angle - angle.to_radians()).abs() < 1e-7);
            assert!(f.max_error_mm < 1e-6);
        }
    }
}
#[test]
fn one_edge_and_duplicate_points_are_ambiguous() {
    let mut t = rectangle();
    for p in &mut t.picks {
        p.curve = 0;
    }
    assert!(t.plan().is_err());
    t.picks = vec![Pick { contour: 0, curve: 0, t: 0.5 }; 6];
    assert!(t.plan().is_err());
}
#[test]
fn disc_measures_center_and_does_not_invent_angle() {
    let mut t = rectangle();
    t.shape = Shape::Disc { diameter: 600. };
    t.drawing_point = [25., -15.];
    t.picks.clear();
    let plan = t.plan().unwrap();
    assert_eq!(plan.searches.len(), 8);
    let fit = simulate(&t, &plan, [50., -50.], 19., Fault::None).unwrap();
    assert!(fit.pose.angle.abs() < 1e-12);
    assert!(fit.pose.apply(t.drawing_point.into()).distance(Point::new(750., 550.)) < 1e-6);
    t.shape = Shape::Special { drawing: t.shape.drawing().unwrap() };
    t.picks = (0..8).map(|i| Pick { contour: 0, curve: 0, t: f64::from(i) / 8. }).collect();
    assert!(t.plan().is_err(), "rotational symmetry is not a registration");
}
#[test]
fn rejects_missing_material_holes_wrong_size_outliers_and_range() {
    let t = rectangle();
    let plan = t.plan().unwrap();
    for fault in [Fault::Missing, Fault::Hole, Fault::WrongSize, Fault::Outside, Fault::Outlier] {
        assert!(simulate(&t, &plan, [0., 0.], 0., fault).is_err(), "{fault:?}");
    }
    assert!(simulate(&t, &plan, [51., 0.], 0., Fault::None).is_err());
    let mut disc = t;
    disc.shape = Shape::Disc { diameter: 600. };
    disc.drawing_point = [0., 0.];
    let p = disc.plan().unwrap();
    assert!(simulate(&disc, &p, [0., 0.], 0., Fault::WrongSize).is_err());
}
#[test]
fn small_stock_and_hole_under_start_cannot_assume_material() {
    let mut t = rectangle();
    t.shape = Shape::Rectangle { width: 80., height: 60. };
    t.drawing_point = [40., 30.];
    assert!(t.plan().is_err());
    t.search_mm = 1.;
    t.angle_deg = 0.;
    assert!(t.plan().is_ok());
    let mut t = rectangle();
    let plan = t.plan().unwrap();
    let s = Point::from(plan.searches[0].start);
    let q = plan.nominal.matrix().inverse().apply(s);
    let mut d = t.shape.drawing().unwrap();
    d.contours.push(Contour { layer: "hole".into(), curves: vec![Curve::circle(q, 10.)] });
    t.shape = Shape::Special { drawing: d };
    assert!(t.plan().is_err());
}
#[test]
fn invalid_inputs_and_corners_are_rejected() {
    let mut t = rectangle();
    t.shape = Shape::Rectangle { width: -1., height: 500. };
    assert!(t.plan().is_err());
    let mut t = rectangle();
    t.search_mm = f64::NAN;
    assert!(t.plan().is_err());
    let mut t = rectangle();
    t.picks[0].t = 0.;
    assert!(t.plan().is_err());
    let mut t = rectangle();
    t.shape = Shape::Special {
        drawing: Drawing {
            contours: vec![Contour {
                layer: "open".into(),
                curves: vec![Curve::Line { start: Point::ORIGIN, end: Point::new(10., 10.) }],
            }],
        },
    };
    assert!(t.plan().is_err());
}
#[test]
fn transformation_preserves_original_geometry_film_process_and_arc_radius() {
    let d = Drawing {
        contours: vec![Contour {
            layer: "Cut".into(),
            curves: vec![Curve::circle(Point::new(20., 30.), 10.)],
        }],
    };
    let mut draft = crate::draft::Draft::of(d.clone());
    draft.prepare(&d);
    let mut p = (*draft.prepared.unwrap()).clone();
    p.film = vec![vec![p.contours[0].clone()]];
    let before = p.contours.clone();
    let transformed = prepare(&p, Pose { translation: [300., 200.], angle: 0.1 });
    assert_eq!(p.contours, before);
    assert_eq!(transformed.contours.len(), before.len());
    let original = Point::from(before[0].segments[0].curve.point(0.));
    let expected = Pose { translation: [300., 200.], angle: 0.1 }.apply(original);
    assert!(
        Point::from(transformed.contours[0].segments[0].curve.point(0.)).distance(expected) < 1e-7
    );
    assert_eq!(transformed.film[0][0].segments, transformed.contours[0].segments);
}
#[test]
fn correction_is_evaluated_after_registration_in_machine_coordinates() {
    use openlaser_core::geometry::Bounds;
    use openlaser_correction::{Map, Measurement, Profile};
    let d = Drawing {
        contours: vec![Contour {
            layer: "Cut".into(),
            curves: vec![Curve::Line { start: Point::new(20., 30.), end: Point::new(70., 30.) }],
        }],
    };
    let mut draft = crate::draft::Draft::of(d.clone());
    draft.prepare(&d);
    let p = draft.prepared.unwrap();
    let pose = Pose { translation: [300., 200.], angle: 0.1 };
    let profile = Profile {
        bed: Bounds { min: Point::ORIGIN, max: Point::new(1000., 1000.) },
        measurements: [Measurement { x: 101., y: 99. }; 9],
    };
    let aligned = prepare(&p, pose);
    let corrected = crate::correction::prepare(&aligned, Some(&profile), [0., 0.]).unwrap();
    let command = Point::from(corrected.contours[0].segments[0].curve.point(0.));
    let map = Map::new(&profile).unwrap();
    assert!(map.forward(command).distance(pose.apply(Point::new(20., 30.))) < 1e-6);
}
#[test]
fn template_restart_retains_intent_but_no_measurement() {
    let root = std::env::temp_dir().join(format!("openlaser-alignment-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let saved =
        super::Saved { enabled: true, template: Some(rectangle()), ..super::Saved::default() };
    openlaser_library::atomic_write(&root.join(super::FILE), &serde_json::to_vec(&saved).unwrap())
        .unwrap();
    let loaded = super::Session::open(&root).unwrap();
    assert!(loaded.guard().is_err());
    assert!(loaded.result.is_none());
    assert_eq!(loaded.flow.phase, super::workflow::Phase::Idle);
    std::fs::remove_file(root.join(super::FILE)).unwrap();
    std::fs::remove_dir(root).unwrap();
}
