// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline tests for manually placed sheet separation lines.
#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    reason = "offline fixtures with exact coordinates"
)]
mod common;
use common::start;
use openlaser_core::LaserMode;
use openlaser_server::coordinator::{NewRecipe, Values};
use openlaser_server::document::PathKind;
use openlaser_server::machine;
use openlaser_server::manual_cut::ManualCut;

#[tokio::test]
async fn a_separation_line_keeps_machine_coordinates_and_never_starts_motion() {
    let (_sim, shared) = start("manual-cut").await;
    {
        let mut c = shared.lock().await;
        let recipe = c
            .add_recipe(&NewRecipe {
                name: "Sheet cutting".into(),
                laser: LaserMode::Fiber,
                thickness_mm: 1.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        let invalid = ManualCut { recipe: recipe.id.clone(), start: [-1., 50.], end: [300., 50.] };
        let before = c.document().library.parts.len();
        assert!(c.manual_cut(invalid).is_err());
        assert!(
            c.manual_cut(ManualCut {
                recipe: recipe.id.clone(),
                start: [100., 50.],
                end: [100., 50.]
            })
            .is_err()
        );
        assert!(
            c.manual_cut(ManualCut {
                recipe: recipe.id.clone(),
                start: [f64::NAN, 50.],
                end: [300., 50.]
            })
            .is_err()
        );
        assert_eq!(c.document().library.parts.len(), before);
        c.manual_cut(ManualCut { recipe: recipe.id, start: [100., 50.], end: [300., 150.] })
            .unwrap();
        assert!(c.document().machine.operation.is_none());
        assert!(c.document().machine.program.is_none());
    }
    machine::prepare(&shared).await.unwrap();
    let draft = shared.lock().await.document().draft.unwrap();
    assert!(draft.error.is_none(), "{:?}", draft.error);
    let preview = draft.preview.as_ref().unwrap();
    let cuts: Vec<_> = preview
        .contours
        .iter()
        .flat_map(|c| &c.paths)
        .filter(|p| p.kind == PathKind::Cut)
        .collect();
    assert_eq!(cuts.len(), 1);
    assert_eq!(cuts[0].points.first().unwrap(), &[100., 50.]);
    assert_eq!(cuts[0].points.last().unwrap(), &[300., 150.]);
    openlaser_server::shutdown(&shared).await.unwrap();
}
