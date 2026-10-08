// SPDX-License-Identifier: GPL-3.0-or-later
//! The sketch editor writes ellipses, elliptical arcs and splines as ELLIPSE
//! and SPLINE entities; the importer must cut them where the sketch drew them.
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::redundant_closure_for_method_calls,
    clippy::needless_range_loop,
    reason = "known fixtures; de Boor in its textbook names"
)]

use openlaser_core::geometry::Point;

#[test]
fn sketch_ellipses_and_splines_import_where_they_were_drawn() {
    let import = openlaser_dxf::import(include_bytes!("sketch_curves.dxf")).unwrap();
    let contours = &import.drawing.contours;
    let summary: Vec<_> =
        contours.iter().map(|c| (c.start(), c.end(), c.curves.len(), c.is_closed())).collect();
    assert_eq!(contours.len(), 3, "{summary:?} {:?}", import.warnings);
    let points = |i: usize| contours[i].curves.iter().map(|c| c.start()).collect::<Vec<_>>();
    // The whole ellipse about (50, 50): 30 by 10.
    let ellipse = contours.iter().position(|c| c.is_closed()).unwrap();
    for p in points(ellipse) {
        let (x, y) = ((p.x - 50.) / 30., (p.y - 50.) / 10.);
        assert!((x * x + y * y - 1.).abs() < 0.01, "{p:?}");
    }
    // The upper half of an ellipse about the origin, 20 by 10.
    let half = contours
        .iter()
        .position(|c| c.start().is_some_and(|s| s.distance(Point::new(20., 0.)) < 1e-6))
        .unwrap();
    let ends: Vec<_> = contours.iter().map(|c| (c.start(), c.end(), c.curves.len())).collect();
    assert!(contours[half].end().unwrap().distance(Point::new(-20., 0.)) < 1e-6, "{ends:?}");
    assert!(points(half).iter().all(|p| p.y >= -1e-6));
    // The spline from (0, 100) to (30, 100).
    let spline = contours
        .iter()
        .position(|c| c.start().is_some_and(|s| s.distance(Point::new(0., 100.)) < 1e-6))
        .unwrap();
    assert!(contours[spline].end().unwrap().distance(Point::new(30., 100.)) < 1e-6);
}

/// Points along a sketch curve.
fn along(p: &openlaser_dxf::Primitive) -> Vec<Point> {
    use openlaser_dxf::Primitive;
    match p {
        Primitive::Line { start, end } => {
            (0..=8).map(|i| *start + (*end - *start) * (f64::from(i) / 8.)).collect()
        }
        Primitive::Arc(arc) => (0..=16).map(|i| arc.point(f64::from(i) / 16.)).collect(),
        Primitive::Ellipse { center, major, minor_radius, start, end } => {
            let minor = Point::new(-major.y, major.x) * (minor_radius / major.norm());
            (0..=64)
                .map(|i| start + (end - start) * f64::from(i) / 64.)
                .map(|t| *center + *major * t.cos() + minor * t.sin())
                .collect()
        }
        Primitive::Spline { degree, knots, poles, weights } => {
            let (p, n) = (*degree, poles.len());
            let (from, to) = (knots[p], knots[n]);
            (0..=200)
                .map(|i| from + (to - from) * f64::from(i) / 200.)
                .map(|u| {
                    // de Boor in homogeneous coordinates.
                    let span = if u >= knots[n] {
                        n - 1
                    } else {
                        knots.partition_point(|&k| k <= u).saturating_sub(1).clamp(p, n - 1)
                    };
                    let mut d: Vec<[f64; 3]> = (0..=p)
                        .map(|j| {
                            let i = span - p + j;
                            let w = weights[i];
                            [poles[i].x * w, poles[i].y * w, w]
                        })
                        .collect();
                    for r in 1..=p {
                        for j in (r..=p).rev() {
                            let i = span - p + j;
                            let den = knots[i + p + 1 - r] - knots[i];
                            let a = if den == 0. { 0. } else { (u - knots[i]) / den };
                            for k in 0..3 {
                                d[j][k] = d[j - 1][k] * (1. - a) + d[j][k] * a;
                            }
                        }
                    }
                    Point::new(d[p][0] / d[p][2], d[p][1] / d[p][2])
                })
                .collect()
        }
    }
}

#[test]
fn sketch_curves_lie_on_what_the_import_cuts() {
    use openlaser_dxf::Primitive;
    let dir = format!("{}/../../fixtures/import", env!("CARGO_MANIFEST_DIR"));
    for name in [
        "fusion360-ellipses.dxf",
        "fusion360-spline-sketch.dxf",
        "qcad-fit-point-splines.dxf",
        "rhino-rational-circle.dxf",
        "draftsight-mixed-layers.dxf",
    ] {
        let bytes = std::fs::read(format!("{dir}/{name}")).unwrap();
        let options = openlaser_dxf::Options::default();
        let cut = openlaser_dxf::import_with(&bytes, &options).unwrap();
        let sketch = openlaser_dxf::sketch_with(&bytes, &options).unwrap();
        assert!(!sketch.curves.is_empty(), "{name}");
        let curves: Vec<_> = cut.drawing.contours.iter().flat_map(|c| c.curves.iter()).collect();
        let off = |p: Point| {
            curves
                .iter()
                .map(|c| openlaser_core::fit::curve_distance(p, c))
                .fold(f64::INFINITY, f64::min)
        };
        for (layer, primitive) in &sketch.curves {
            for p in along(primitive) {
                assert!(
                    off(p) < 0.02,
                    "{name} {layer}: {primitive:?} strays {} mm at {p:?}",
                    off(p)
                );
            }
        }
        let kinds = |f: fn(&Primitive) -> bool| sketch.curves.iter().filter(|(_, p)| f(p)).count();
        if name.contains("ellipses") {
            assert!(
                kinds(|p| matches!(p, Primitive::Ellipse { .. })) > 0,
                "{name} keeps its ellipses"
            );
        }
        if name.contains("spline-sketch") {
            assert!(
                kinds(|p| matches!(p, Primitive::Spline { .. })) > 0,
                "{name} keeps its splines"
            );
        }
    }
}
