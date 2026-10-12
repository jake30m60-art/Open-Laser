// SPDX-License-Identifier: GPL-3.0-or-later
//! The engraving calibration test: per speed, three blocks of four lines.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    reason = "known offline fixtures with exact millimetre positions"
)]
mod common;

use common::start;
use openlaser_core::LaserMode;
use openlaser_server::coordinator::{NewRecipe, RecipeChange, Values};
use openlaser_server::document::PathKind;
use openlaser_server::machine;
use openlaser_server::scan_offsets::{ScanCoupon, ScanOffsetsChange};

#[tokio::test]
async fn each_single_direction_engraves_four_lines() {
    let (_sim, shared) = start("scan-coupon").await;
    {
        let mut c = shared.lock().await;
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Mark".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        c.update_recipe(
            &recipe.id,
            RecipeChange {
                attributes: [("OpenLaserProcess".into(), "engrave".into())].into(),
                ..Default::default()
            },
        )
        .unwrap();
        c.scan_offset_coupon(ScanCoupon {
            recipe: recipe.id,
            angle: 0.,
            angles: Vec::new(),
            speeds: vec![100., 400.],
            origin: [300., 300.],
            length: 10.,
            spacing: 1.,
        })
        .unwrap();
    }
    machine::prepare(&shared).await.unwrap();
    let draft = shared.lock().await.document().draft.unwrap();
    assert!(draft.error.is_none(), "{:?}", draft.error);
    let preview = draft.preview.as_ref().unwrap();
    for (layer, run_up) in [("100 mm/s", 14.), ("400 mm/s", 43.)] {
        let contours: Vec<_> = preview.contours.iter().filter(|c| c.layer == layer).collect();
        // One uncompensated four-line group for this direction.
        let marks: Vec<_> =
            contours.iter().flat_map(|c| &c.paths).filter(|p| p.kind == PathKind::Cut).collect();
        assert_eq!(marks.len(), 4, "{layer}: four lines in one direction");
        let heights: std::collections::BTreeSet<i64> =
            marks.iter().map(|p| (p.points[0][1] * 1000.).round() as i64).collect();
        assert_eq!(heights.len(), 4, "{layer}: four lines one above the other");
        // The head runs in far enough to be at speed where a line starts.
        let lowest =
            marks.iter().flat_map(|p| &p.points).map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let start = contours
            .iter()
            .flat_map(|c| &c.paths)
            .flat_map(|p| &p.points)
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min);
        assert!(lowest - start > run_up - 0.5, "{layer}: run-up {}", lowest - start);
    }
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn angled_coupons_keep_their_four_lines_and_all_travel_inside_the_bed() {
    for angle in [45_f64, 90., 135.] {
        let (_sim, shared) = start("angled-scan-coupon").await;
        let bed;
        {
            let mut c = shared.lock().await;
            bed = c.scan_offsets_view(LaserMode::Fiber).unwrap().bed;
            let recipe = c
                .add_recipe(&NewRecipe {
                    name: "Mark".into(),
                    laser: LaserMode::Fiber,
                    thickness_mm: 1.,
                    values: Values::Bank(1),
                    gas: None,
                })
                .unwrap();
            c.update_recipe(
                &recipe.id,
                RecipeChange {
                    attributes: [("OpenLaserProcess".into(), "engrave".into())].into(),
                    ..Default::default()
                },
            )
            .unwrap();
            c.scan_offset_coupon(ScanCoupon {
                recipe: recipe.id,
                angle,
                angles: Vec::new(),
                speeds: vec![100., 400.],
                origin: [300., 300.],
                length: 10.,
                spacing: 1.,
            })
            .unwrap();
        }
        machine::prepare(&shared).await.unwrap();
        let draft = shared.lock().await.document().draft.unwrap();
        assert!(draft.error.is_none(), "{angle}: {:?}", draft.error);
        let preview = draft.preview.as_ref().unwrap();
        let (sin, cos) = angle.to_radians().sin_cos();
        for layer in ["100 mm/s", "400 mm/s"] {
            let paths: Vec<_> = preview
                .contours
                .iter()
                .filter(|c| c.layer == layer)
                .flat_map(|c| &c.paths)
                .collect();
            let marks: Vec<_> = paths.iter().filter(|p| p.kind == PathKind::Cut).collect();
            assert_eq!(marks.len(), 4, "{angle} {layer}");
            let rows: std::collections::BTreeSet<i64> = marks
                .iter()
                .map(|p| ((-p.points[0][0] * sin + p.points[0][1] * cos) * 1000.).round() as i64)
                .collect();
            assert_eq!(rows.len(), 4, "{angle} {layer}: four scan rows");
            for path in paths {
                for p in &path.points {
                    assert!(
                        p[0] >= bed.min.x - 1e-6
                            && p[0] <= bed.max.x + 1e-6
                            && p[1] >= bed.min.y - 1e-6
                            && p[1] <= bed.max.y + 1e-6,
                        "{angle}: travel outside bed at {p:?}"
                    );
                }
            }
        }
        openlaser_server::shutdown(&shared).await.unwrap();
    }
}

#[tokio::test]
async fn invalid_coupons_do_not_replace_the_draft_or_add_library_parts() {
    let (_sim, shared) = start("invalid-scan-coupon").await;
    {
        let mut c = shared.lock().await;
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Mark".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        c.update_recipe(
            &recipe.id,
            RecipeChange {
                attributes: [("OpenLaserProcess".into(), "engrave".into())].into(),
                ..Default::default()
            },
        )
        .unwrap();
        let original = c.document();
        let valid = ScanCoupon {
            recipe: recipe.id,
            angle: 0.,
            angles: Vec::new(),
            speeds: vec![400.],
            origin: [300., 300.],
            length: 10.,
            spacing: 1.,
        };
        let invalid = [
            ScanCoupon { origin: [10., 20.], ..valid.clone() }, // marks fit, run-up leaves bed
            ScanCoupon { angle: f64::NAN, ..valid.clone() },
            ScanCoupon { angles: vec![0., 180.], ..valid.clone() },
            ScanCoupon { angles: vec![f64::NAN], ..valid.clone() },
            ScanCoupon { origin: [f64::NAN, 100.], ..valid.clone() },
            ScanCoupon { speeds: vec![100., 100.], ..valid.clone() },
        ];
        for coupon in invalid {
            assert!(c.scan_offset_coupon(coupon).is_err());
            let after = c.document();
            assert_eq!(after.library.parts.len(), original.library.parts.len());
            assert_eq!(after.draft.is_some(), original.draft.is_some());
        }
        // Distinct fractional speeds must not collapse onto the same layer name.
        c.scan_offset_coupon(ScanCoupon { speeds: vec![100.001, 100.002], ..valid }).unwrap();
        let draft = c.document().draft.unwrap();
        let layers = &draft.layers;
        assert_eq!(layers.len(), 2);
        assert_ne!(layers[0].name, layers[1].name);
    }
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn measured_offsets_keep_their_sign_other_angles_and_other_laser() {
    let (_sim, shared) = start("signed-scan-offsets").await;
    {
        let mut c = shared.lock().await;
        for (mode, angle, measured) in [
            (LaserMode::Fiber, 0., vec![(100., Some(-0.1)), (400., Some(0.2))]),
            (LaserMode::Fiber, 90., vec![(100., Some(0.3))]),
            (LaserMode::Co2, 0., vec![(100., Some(-0.4))]),
        ] {
            let revision = c.scan_offsets_view(mode).unwrap().revision;
            c.save_scan_offsets(ScanOffsetsChange { revision, mode, angle, measured }).unwrap();
        }
        let fiber = c.scan_offsets_view(LaserMode::Fiber).unwrap();
        assert_eq!(fiber.entries.len(), 3);
        assert_eq!(fiber.entries[0].offset, -0.1);
        assert_eq!(fiber.entries[2].angle, 90.);
        assert_eq!(c.scan_offsets_view(LaserMode::Co2).unwrap().entries[0].offset, -0.4);
        assert!(
            c.save_scan_offsets(ScanOffsetsChange {
                revision: fiber.revision,
                mode: LaserMode::Fiber,
                angle: f64::NAN,
                measured: Vec::new()
            })
            .is_err()
        );
        assert_eq!(c.scan_offsets_view(LaserMode::Fiber).unwrap().entries, fiber.entries);
    }
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn each_speed_has_twelve_lines_in_zero_fortyfive_ninety_order() {
    let (_sim, shared) = start("three-direction-coupon").await;
    {
        let mut c = shared.lock().await;
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Mark".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        c.update_recipe(
            &recipe.id,
            RecipeChange {
                attributes: [("OpenLaserProcess".into(), "engrave".into())].into(),
                ..Default::default()
            },
        )
        .unwrap();
        c.scan_offset_coupon(ScanCoupon {
            recipe: recipe.id,
            angle: 0.,
            angles: vec![0., 45., 90.],
            speeds: vec![200., 250.],
            origin: [300., 300.],
            length: 10.,
            spacing: 1.,
        })
        .unwrap();
    }
    machine::prepare(&shared).await.unwrap();
    let draft = shared.lock().await.document().draft.unwrap();
    assert!(draft.error.is_none(), "{:?}", draft.error);
    let preview = draft.preview.as_ref().unwrap();
    let expected = [
        "200 mm/s · 0°",
        "200 mm/s · 45°",
        "200 mm/s · 90°",
        "250 mm/s · 0°",
        "250 mm/s · 45°",
        "250 mm/s · 90°",
    ];
    assert_eq!(draft.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), expected);
    let names: Vec<_> = preview.contours.iter().map(|c| c.layer.as_str()).collect();
    assert_eq!(names, expected, "the prepared engraving must keep speed/direction order");
    for (index, name) in expected.iter().enumerate() {
        let paths: Vec<_> =
            preview.contours.iter().filter(|c| c.layer == *name).flat_map(|c| &c.paths).collect();
        let marks: Vec<_> = paths.iter().filter(|p| p.kind == PathKind::Cut).collect();
        assert_eq!(marks.len(), 4, "{name}: four lines, twelve per speed");
        let angle = [0_f64, 45., 90.][index % 3].to_radians();
        for mark in marks {
            let first = mark.points.first().unwrap();
            let last = mark.points.last().unwrap();
            let dx = last[0] - first[0];
            let dy = last[1] - first[1];
            assert!((dy * angle.cos() - dx * angle.sin()).abs() < 0.01, "{name}: direction");
            assert!((dx.hypot(dy) - 10.).abs() < 0.01, "{name}: length");
        }
    }
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn calibration_rejects_cutting_recipes_before_changing_the_draft() {
    let (_sim, shared) = start("scan-cutting-rejection").await;
    {
        let mut c = shared.lock().await;
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Cut".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        let before = c.document();
        let result = c.scan_offset_coupon(ScanCoupon {
            recipe: recipe.id,
            angle: 0.,
            angles: vec![0., 45., 90.],
            speeds: vec![200.],
            origin: [300., 300.],
            length: 10.,
            spacing: 1.,
        });
        assert!(result.unwrap_err().to_string().contains("engraving recipe"));
        let after = c.document();
        assert_eq!(after.library.parts.len(), before.library.parts.len());
        assert_eq!(after.draft.is_some(), before.draft.is_some());
    }
    openlaser_server::shutdown(&shared).await.unwrap();
}
