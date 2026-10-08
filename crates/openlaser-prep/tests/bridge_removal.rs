// SPDX-License-Identifier: GPL-3.0-or-later
//! One bridge can be removed while the later ones stay where they are.
#![allow(clippy::unwrap_used, reason = "known geometry fixtures")]

use openlaser_core::features::{Bridge, Bridges, Features, Pick};
use openlaser_core::geometry::{Contour, Curve, Drawing, Point, Transform};
use openlaser_core::units::Millimeters;

fn square(x: f64) -> Contour {
    let p = [[x, 0.], [x + 10., 0.], [x + 10., 10.], [x, 10.]];
    Contour {
        layer: "cut".into(),
        curves: (0..4)
            .map(|i| Curve::Line { start: p[i].into(), end: p[(i + 1) % 4].into() })
            .collect(),
    }
}

fn sources(drawing: &Drawing, features: &Features) -> Vec<Vec<usize>> {
    let mut all: Vec<_> = openlaser_prep::prepare(drawing, features)
        .unwrap()
        .contours
        .into_iter()
        .map(|c| c.sources)
        .collect();
    all.sort();
    all.dedup();
    all
}

#[test]
fn removing_the_first_of_two_bridges_keeps_the_second_on_its_contours() {
    // A at 0, B at 20, C at 40; bridge 1 joins A and B, bridge 2 joins the
    // joined A+B (now the list's first entry) with C.
    let drawing = Drawing { contours: vec![square(0.), square(20.), square(40.)] };
    let features = Features {
        bridges: Some(Bridges {
            width: Millimeters(2.),
            connections: vec![
                Bridge {
                    first: Pick { contour: 0, point: Point::new(10., 5.) },
                    second: Pick { contour: 1, point: Point::new(20., 5.) },
                },
                Bridge {
                    first: Pick { contour: 0, point: Point::new(30., 5.) },
                    second: Pick { contour: 1, point: Point::new(40., 5.) },
                },
            ],
        }),
        ..Features::default()
    };
    assert_eq!(sources(&drawing, &features), [vec![0, 1, 2]]);
    let identity = vec![Transform::IDENTITY; 3];
    let (bridges, dropped) =
        openlaser_prep::without_bridge(&drawing, &features, &identity, 0).unwrap();
    assert_eq!(dropped, 0);
    assert_eq!(bridges.connections.len(), 1);
    let rest = Features { bridges: Some(bridges), ..features.clone() };
    assert_eq!(sources(&drawing, &rest), [vec![0], vec![1, 2]]);

    let (bridges, dropped) =
        openlaser_prep::without_bridge(&drawing, &features, &identity, 1).unwrap();
    assert_eq!((bridges.connections.len(), dropped), (1, 0));
    let rest = Features { bridges: Some(bridges), ..features };
    assert_eq!(sources(&drawing, &rest), [vec![0, 1], vec![2]]);
}
