// SPDX-License-Identifier: GPL-3.0-or-later
//! The sketch editor writes ellipses, elliptical arcs and splines as ELLIPSE
//! and SPLINE entities; the importer must cut them where the sketch drew them.
#![allow(clippy::unwrap_used, reason = "a known fixture")]

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
