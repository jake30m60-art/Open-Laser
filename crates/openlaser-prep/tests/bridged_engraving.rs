// SPDX-License-Identifier: GPL-3.0-or-later
//! A bridge across an engraved ring leaves its channel unengraved, however
//! the engraving is chosen.
#![allow(clippy::unwrap_used, reason = "known geometry fixtures")]

use openlaser_core::features::{
    Bridge, Bridges, Features, Fill, FillRegion, Layer, LayerMode, Machining, Pick,
};
use openlaser_core::geometry::{Contour, Curve, Drawing, Point};
use openlaser_core::units::Millimeters;

fn circle(radius: f64) -> Contour {
    Contour {
        layer: "Logo".into(),
        curves: vec![Curve::Arc {
            center: Point::new(30., 30.),
            radius,
            start_angle: 0.,
            sweep: std::f64::consts::TAU,
        }],
    }
}

/// Engraved area: lit scan length times the spacing.
fn engraved(features: &Features) -> f64 {
    let drawing = Drawing { contours: vec![circle(10.), circle(5.)] };
    let path = openlaser_prep::prepare(&drawing, features).unwrap();
    let spacing = 0.05;
    path.contours
        .iter()
        .filter(|c| !c.closed)
        .flat_map(|c| &c.segments)
        .filter(|s| !s.process.joint)
        .map(|s| s.curve.length())
        .sum::<f64>()
        * spacing
}

fn features(per_object: bool, chosen: bool, bridged: bool) -> Features {
    let fill = Fill {
        spacing: 0.05,
        run_up: 1.,
        outline: false,
        per_object: Some(per_object),
        image: None,
        ..Fill::for_image()
    };
    Features {
        layers: vec![Layer {
            name: "Logo".into(),
            mode: LayerMode::Mark,
            color: None,
            machining: Some(Machining { fill: Some(fill), ..Machining::default() }),
        }],
        fill_regions: if chosen { vec![FillRegion { contour: 0, holes: vec![1] }] } else { Vec::new() },
        bridges: bridged.then(|| Bridges {
            width: Millimeters(2.),
            connections: vec![Bridge {
                first: Pick { contour: 0, point: Point::new(30., 40.) },
                second: Pick { contour: 1, point: Point::new(30., 35.) },
            }],
        }),
        ..Features::default()
    }
}

fn square() -> Contour {
    let p = [[0., 0.], [20., 0.], [20., 10.], [0., 10.]];
    Contour {
        layer: "Logo".into(),
        curves: (0..4).map(|i| Curve::Line { start: p[i].into(), end: p[(i + 1) % 4].into() }).collect(),
    }
}

/// A bridge across one shape splits it in two halves with a channel
/// between them; the channel is not engraved.
#[test]
fn a_shape_split_by_a_bridge_is_engraved_as_two_halves() {
    for per_object in [false, true] {
        for chosen in [false, true] {
            let mut f = features(per_object, chosen, true);
            f.fill_regions = if chosen { vec![FillRegion { contour: 0, holes: Vec::new() }] } else { Vec::new() };
            f.bridges.as_mut().unwrap().connections = vec![Bridge {
                first: Pick { contour: 0, point: Point::new(10., 0.) },
                second: Pick { contour: 0, point: Point::new(10., 10.) },
            }];
            let drawing = Drawing { contours: vec![square()] };
            let path = openlaser_prep::prepare(&drawing, &f).unwrap();
            let area = path
                .contours
                .iter()
                .filter(|c| !c.closed)
                .flat_map(|c| &c.segments)
                .filter(|s| !s.process.joint)
                .map(|s| s.curve.length())
                .sum::<f64>()
                * 0.05;
            assert!((area - 180.).abs() < 6., "split square engraves {area} (per object {per_object}, chosen {chosen})");
        }
    }
}

#[test]
fn the_bridge_channel_stays_unengraved() {
    let ring = std::f64::consts::PI * (100. - 25.);
    for per_object in [false, true] {
        for chosen in [false, true] {
            let whole = engraved(&features(per_object, chosen, false));
            let bridged = engraved(&features(per_object, chosen, true));
            assert!((whole - ring).abs() < ring * 0.02, "ring {whole} ({per_object}, {chosen})");
            // The channel is 2 mm wide across the 5 mm ring.
            assert!(
                (whole - bridged - 10.).abs() < 2.,
                "bridged {bridged} vs whole {whole} (per object {per_object}, chosen {chosen})"
            );
        }
    }
}
