// SPDX-License-Identifier: GPL-3.0-or-later

//! Layers: the ones cut through decide parts and holes together, a marked
//! layer never makes a hole and has no machining of its own, and layers
//! run in their order.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests build known drawings")]

use openlaser_core::features::{Features, Kerf, Layer, LayerMode, Machining, Side};
use openlaser_core::geometry::{Contour, Curve, Drawing, Point};
use openlaser_core::units::Millimeters;

fn circle(layer: &str, x: f64, y: f64, radius: f64) -> Contour {
    Contour {
        layer: layer.into(),
        curves: vec![Curve::Arc {
            center: Point::new(x, y),
            radius,
            start_angle: 0.,
            sweep: std::f64::consts::TAU,
        }],
    }
}

fn square(layer: &str, size: f64) -> Contour {
    let p = [[0., 0.], [size, 0.], [size, size], [0., size]];
    Contour {
        layer: layer.into(),
        curves: (0..4)
            .map(|i| Curve::Line { start: p[i].into(), end: p[(i + 1) % 4].into() })
            .collect(),
    }
}

fn layer(name: &str, mode: LayerMode) -> Layer {
    Layer { name: name.into(), mode, color: None, machining: None }
}

/// A countersink: a drilled hole with the countersink's rim marked around
/// it. The rim is a mark, so the hole stays a hole.
#[test]
fn a_marked_rim_never_turns_the_hole_inside_it_into_a_part() {
    let drawing = Drawing {
        contours: vec![
            square("Cut", 60.),
            circle("Cut", 20., 20., 4.),
            circle("Rim", 20., 20., 7.),
        ],
    };
    let kerf = Kerf { width: Millimeters(0.4), side: Side::Auto };
    let unmarked = Features { kerf: Some(kerf), ..Features::default() };
    let before = openlaser_prep::prepare(&drawing, &unmarked).unwrap();
    let depth = |t: &openlaser_core::toolpath::Toolpath, source: usize| {
        t.contours.iter().find(|c| c.sources == [source]).unwrap().depth
    };
    assert_eq!(depth(&before, 1), 2, "cut through, the rim makes the hole an island");

    let marked = Features { layers: vec![layer("Rim", LayerMode::Mark)], ..unmarked.clone() };
    let after = openlaser_prep::prepare(&drawing, &marked).unwrap();
    assert_eq!(depth(&after, 1), 1, "the hole is a hole again");
    assert_eq!(depth(&after, 2), 0);
    let rim = after.contours.iter().find(|c| c.layer == "Rim").unwrap();
    assert!((rim.length() - std::f64::consts::TAU * 7.).abs() < 1e-6, "no kerf on the mark");
    let layers: Vec<_> = after.contours.iter().map(|c| c.layer.as_str()).collect();
    assert_eq!(layers, ["Rim", "Cut", "Cut"], "the smaller layer first, by default");

    // A mark with machining of its own gets it.
    let own = Machining { kerf: Some(kerf), ..Machining::default() };
    let rim_kerf = Layer { machining: Some(own), ..layer("Rim", LayerMode::Mark) };
    let kerfed = Features { layers: vec![rim_kerf], ..unmarked };
    let rim = openlaser_prep::prepare(&drawing, &kerfed).unwrap();
    let rim = rim.contours.iter().find(|c| c.layer == "Rim").unwrap();
    assert!((rim.length() - std::f64::consts::TAU * 7.2).abs() < 1e-6);
}

#[test]
fn layers_run_in_the_tables_order_and_holes_on_their_own_layer_stay_holes() {
    let drawing = Drawing {
        contours: vec![
            square("Outer", 60.),
            circle("Inner", 20., 20., 4.),
            circle("Etch", 40., 40., 3.),
        ],
    };
    let features = Features::default();
    let order = openlaser_prep::layer_order(&drawing, &features);
    assert_eq!(order, ["Etch", "Inner", "Outer"], "smaller shapes first");
    let toolpath = openlaser_prep::prepare(&drawing, &features).unwrap();
    let inner = toolpath.contours.iter().find(|c| c.layer == "Inner").unwrap();
    assert_eq!(inner.depth, 1, "a hole on its own layer is still a hole");

    let dragged = Features {
        layers: vec![
            layer("Outer", LayerMode::Cut),
            layer("Etch", LayerMode::Mark),
            layer("Inner", LayerMode::Cut),
        ],
        ..Features::default()
    };
    let toolpath = openlaser_prep::prepare(&drawing, &dragged).unwrap();
    let layers: Vec<_> = toolpath.contours.iter().map(|c| c.layer.as_str()).collect();
    assert_eq!(layers, ["Outer", "Etch", "Inner"]);
}

/// An engraved logo: the marked layer's closed shapes become one zigzag
/// with the beam on only inside them, entered at speed, and their outlines
/// follow when the fill keeps them. The cut layer is untouched.
#[test]
fn a_filled_mark_engraves_one_zigzag_and_keeps_its_outlines() {
    use openlaser_core::features::Fill;
    let drawing = Drawing { contours: vec![square("Cut", 60.), circle("Logo", 30., 30., 10.)] };
    let fill = Fill {
        spacing: 0.5,
        angle: 0.,
        run_up: 2.,
        outline: true,
        outline_holes: None,
        bidirectional: true,
        scan_offset: 0.,
        per_object: None,
        passes: 1,
        pass_angle: 0.,
        pass_shift: 0.,
        calibrated: Vec::new(),
        image: None,
    };
    let logo = Layer {
        machining: Some(Machining { fill: Some(fill.clone()), ..Machining::default() }),
        ..layer("Logo", LayerMode::Mark)
    };
    let features = Features { layers: vec![logo], ..Features::default() };
    let toolpath = openlaser_prep::prepare(&drawing, &features).unwrap();
    let logo: Vec<_> = toolpath.contours.iter().filter(|c| c.layer == "Logo").collect();
    assert_eq!(logo.len(), 2, "the zigzag, then the outline");
    let zigzag = logo[0];
    assert!(!zigzag.closed);
    assert!(zigzag.segments[0].process.joint, "it enters at speed with the beam off");
    let lit: f64 =
        zigzag.segments.iter().filter(|s| !s.process.joint).map(|s| s.curve.length()).sum();
    let area = std::f64::consts::PI * 100.;
    assert!((lit * fill.spacing - area).abs() < area * 0.02, "covers the disc: {lit}");
    assert!(logo[1].closed);
    assert_eq!(toolpath.contours.iter().filter(|c| c.layer == "Cut").count(), 1);

    let bare = Fill { outline: false, ..fill };
    let logo = Layer {
        machining: Some(Machining { fill: Some(bare), ..Machining::default() }),
        ..layer("Logo", LayerMode::Mark)
    };
    let features = Features { layers: vec![logo], ..Features::default() };
    let toolpath = openlaser_prep::prepare(&drawing, &features).unwrap();
    assert_eq!(toolpath.contours.iter().filter(|c| c.layer == "Logo").count(), 1);
}

/// The outline trace after a fill can leave the holes out: a ring keeps
/// its outer edge and loses the inner one, an island in the hole is outer
/// again.
#[test]
fn the_outline_trace_can_skip_holes() {
    use openlaser_core::features::Fill;
    let drawing = Drawing {
        contours: vec![
            circle("Logo", 30., 30., 15.),
            circle("Logo", 30., 30., 8.),
            circle("Logo", 30., 30., 3.),
        ],
    };
    let traced = |holes: Option<bool>| {
        let fill = Fill { outline: true, outline_holes: holes, image: None, ..Fill::for_image() };
        let logo = Layer {
            machining: Some(Machining { fill: Some(fill), ..Machining::default() }),
            ..layer("Logo", LayerMode::Mark)
        };
        let features = Features { layers: vec![logo], ..Features::default() };
        let toolpath = openlaser_prep::prepare(&drawing, &features).unwrap();
        let mut depths: Vec<_> =
            toolpath.contours.iter().filter(|c| c.closed).map(|c| c.depth).collect();
        depths.sort_unstable();
        depths
    };
    assert_eq!(traced(None), [0, 1, 2]);
    assert_eq!(traced(Some(true)), [0, 1, 2]);
    assert_eq!(traced(Some(false)), [0, 2]);
}

/// Chosen engraving areas are engraved together: one zigzag runs line by
/// line across all of them, the beam off between them and over every hole,
/// rather than one area after another.
#[test]
fn chosen_areas_are_engraved_together_line_by_line() {
    use openlaser_core::features::{Fill, FillRegion};
    let drawing = Drawing {
        contours: vec![
            circle("Logo", 15., 30., 10.),
            circle("Logo", 15., 30., 4.),
            circle("Logo", 45., 30., 10.),
        ],
    };
    let fill = Fill {
        spacing: 0.5,
        angle: 0.,
        run_up: 2.,
        outline: false,
        outline_holes: None,
        bidirectional: true,
        scan_offset: 0.,
        per_object: None,
        passes: 1,
        pass_angle: 0.,
        pass_shift: 0.,
        calibrated: Vec::new(),
        image: None,
    };
    let logo = Layer {
        machining: Some(Machining { fill: Some(fill.clone()), ..Machining::default() }),
        ..layer("Logo", LayerMode::Mark)
    };
    let features = Features {
        layers: vec![logo],
        fill_regions: vec![
            FillRegion { contour: 0, holes: vec![1] },
            FillRegion { contour: 2, holes: Vec::new() },
        ],
        ..Features::default()
    };
    let toolpath = openlaser_prep::prepare(&drawing, &features).unwrap();
    let zigzags: Vec<_> = toolpath.contours.iter().filter(|c| c.layer == "Logo").collect();
    assert_eq!(zigzags.len(), 1, "one engraving across both areas");
    assert_eq!(zigzags[0].sources, [0, 2], "owned by the chosen areas, not the hole");
    // The line nearest both middles lights both discs and leaves the hole dark.
    let lit_segments: Vec<_> = zigzags[0].segments.iter().filter(|s| !s.process.joint).collect();
    let nearest = lit_segments
        .iter()
        .map(|s| s.curve.start().y)
        .min_by(|a, b| (a - 30.).abs().total_cmp(&(b - 30.).abs()))
        .unwrap();
    let middle: Vec<_> =
        lit_segments.iter().filter(|s| (s.curve.start().y - nearest).abs() < 1e-9).collect();
    assert_eq!(middle.len(), 3, "left of the hole, right of it, and the second disc");
    let lit: f64 =
        zigzags[0].segments.iter().filter(|s| !s.process.joint).map(|s| s.curve.length()).sum();
    let area = std::f64::consts::PI * (100. - 16. + 100.);
    assert!((lit * fill.spacing - area).abs() < area * 0.02, "covers both, not the hole: {lit}");
}

#[test]
fn scan_modes_keep_holes_and_limit_to_selected_output() {
    use openlaser_core::features::{Fill, FillRegion};
    let drawing = Drawing {
        contours: vec![
            circle("Logo", 15., 30., 10.),
            circle("Logo", 15., 30., 4.),
            circle("Logo", 45., 30., 10.),
        ],
    };
    let fill = Fill {
        spacing: 0.5,
        run_up: 2.,
        outline: false,
        per_object: Some(true),
        image: None,
        ..Fill::for_image()
    };
    let mut f = Features {
        layers: vec![Layer {
            machining: Some(Machining { fill: Some(fill), ..Machining::default() }),
            ..layer("Logo", LayerMode::Mark)
        }],
        fill_regions: vec![
            FillRegion { contour: 0, holes: vec![1] },
            FillRegion { contour: 2, holes: vec![] },
        ],
        ..Features::default()
    };
    let separate = openlaser_prep::prepare(&drawing, &f).unwrap();
    assert_eq!(separate.contours.len(), 2);
    assert_eq!(separate.contours[0].sources, vec![0]);
    assert_eq!(separate.contours[1].sources, vec![2]);
    let lit: f64 = separate
        .contours
        .iter()
        .flat_map(|c| &c.segments)
        .filter(|s| !s.process.joint)
        .map(|s| s.curve.length())
        .sum();
    assert!((lit * 0.5 - std::f64::consts::PI * 184.).abs() < 12.);
    f.layers[0].machining.as_mut().unwrap().fill.as_mut().unwrap().per_object = Some(false);
    assert_eq!(openlaser_prep::prepare(&drawing, &f).unwrap().contours.len(), 1);
    f.only_contours = Some(vec![2]);
    let selected = openlaser_prep::prepare(&drawing, &f).unwrap();
    assert_eq!(selected.contours.len(), 1);
    assert_eq!(selected.contours[0].sources, vec![2]);
}
#[test]
fn explicit_white_picture_is_not_filled_as_a_solid_frame() {
    use openlaser_core::features::{Fill, FillRegion};
    use openlaser_core::raster::{PlacedRaster, Raster};
    let drawing = Drawing { contours: vec![square("Logo", 10.)] };
    let f = Features {
        layers: vec![Layer {
            machining: Some(Machining { fill: Some(Fill::for_image()), ..Machining::default() }),
            ..layer("Logo", LayerMode::Mark)
        }],
        rasters: vec![PlacedRaster {
            contour: 0,
            raster: Raster::upright(1, 1, vec![255], [10., 10.]).unwrap(),
        }],
        fill_regions: vec![FillRegion { contour: 0, holes: vec![] }],
        only_contours: Some(vec![0]),
        ..Features::default()
    };
    assert!(openlaser_prep::prepare(&drawing, &f).unwrap().contours.is_empty());
}
#[test]
fn oversized_white_picture_is_rejected_before_shading() {
    use openlaser_core::features::Fill;
    use openlaser_core::raster::{PlacedRaster, Raster};
    let drawing = Drawing { contours: vec![square("Logo", 1000.)] };
    let f = Features {
        layers: vec![Layer {
            machining: Some(Machining {
                fill: Some(Fill { spacing: 0.01, ..Fill::for_image() }),
                ..Machining::default()
            }),
            ..layer("Logo", LayerMode::Mark)
        }],
        rasters: vec![PlacedRaster {
            contour: 0,
            raster: Raster::upright(1, 1, vec![255], [1000., 1000.]).unwrap(),
        }],
        ..Features::default()
    };
    assert!(openlaser_prep::prepare(&drawing, &f).unwrap_err().to_string().contains("cells"));
}
#[test]
fn bridged_engraving_uses_the_opened_boundary_not_the_old_hole() {
    use openlaser_core::features::{Bridge, Bridges, Fill, FillRegion, Pick};
    let drawing =
        Drawing { contours: vec![circle("Logo", 0., 0., 10.), circle("Logo", 0., 0., 4.)] };
    let fill = Fill { spacing: 0.2, run_up: 2., outline: false, image: None, ..Fill::for_image() };
    let f = Features {
        layers: vec![Layer {
            machining: Some(Machining { fill: Some(fill), ..Machining::default() }),
            ..layer("Logo", LayerMode::Mark)
        }],
        fill_regions: vec![FillRegion { contour: 0, holes: vec![1] }],
        bridges: Some(Bridges {
            width: Millimeters(2.),
            connections: vec![Bridge {
                first: Pick { contour: 0, point: Point::new(10., 0.) },
                second: Pick { contour: 1, point: Point::new(4., 0.) },
            }],
        }),
        ..Features::default()
    };
    let path = openlaser_prep::prepare(&drawing, &f).unwrap();
    assert!(path.contours.iter().all(|c| c.sources == [0, 1]));
    for segment in path.contours.iter().flat_map(|c| &c.segments).filter(|s| !s.process.joint) {
        let a = segment.curve.start();
        let b = segment.curve.end();
        let mid = Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
        assert!(!(mid.x > 4. && mid.x < 10. && mid.y.abs() < 0.9), "bridge must remain clear");
    }
}
