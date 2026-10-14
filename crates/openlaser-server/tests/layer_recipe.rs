// SPDX-License-Identifier: GPL-3.0-or-later
//! An engraving material is what the job engraves with: choosing it takes
//! back every layer's own engraving recipe.
#![allow(clippy::unwrap_used, reason = "known fixtures")]
mod common;

use common::start;
use openlaser_core::LaserMode;
use openlaser_server::coordinator::{NewRecipe, RecipeChange, Values};
use openlaser_server::layers::LayerChange;

#[tokio::test]
async fn choosing_an_engraving_material_takes_back_the_layers_own_engraving_recipes() {
    let (_sim, shared) = start("layer-recipe").await;
    let mut c = shared.lock().await;
    let pixels = vec![0u8; 16 * 16];
    let part =
        c.library.add_image_part("black.png", &pixels, (16, 16), pixels.clone(), [8., 8.]).unwrap();
    let engraving = |c: &mut openlaser_server::Coordinator, name: &str, speed: &str| {
        let recipe = c
            .add_recipe(&NewRecipe {
                name: name.into(),
                laser: LaserMode::Fiber,
                thickness_mm: 0.,
                values: Values::Bank(1),
                gas: None,
            })
            .unwrap();
        c.update_recipe(
            &recipe.id,
            RecipeChange {
                attributes: [
                    ("OpenLaserProcess".into(), "engrave".into()),
                    ("CutSpeed".into(), speed.into()),
                ]
                .into(),
                ..Default::default()
            },
        )
        .unwrap();
        recipe.id
    };
    let slow = engraving(&mut c, "Old engraving", "100");
    let fast = engraving(&mut c, "Steel engraving", "250");
    c.open_part(&part.id).unwrap();
    c.set_recipe(&slow).unwrap();
    let layer = c.document().draft.unwrap().layers[0].name.clone();
    c.change_layers(LayerChange::Recipe { layer: layer.clone(), recipe: Some(slow.clone()) })
        .unwrap();
    assert!(c.document().draft.unwrap().layers[0].recipe.is_some());
    // The operator picks the 250 mm/s engraving as the job's material.
    c.set_recipe(&fast).unwrap();
    let draft = c.document().draft.unwrap();
    assert!(draft.layers[0].recipe.is_none(), "the layer follows the material now");
    assert_eq!(draft.recipe.as_ref().unwrap().attributes["CutSpeed"], "250");
}

#[tokio::test]
async fn a_recipe_edited_in_the_library_reaches_the_open_job() {
    let (_sim, shared) = start("recipe-refresh").await;
    let mut c = shared.lock().await;
    let pixels = vec![0u8; 16 * 16];
    let part =
        c.library.add_image_part("black.png", &pixels, (16, 16), pixels.clone(), [8., 8.]).unwrap();
    let recipe = c
        .add_recipe(&NewRecipe {
            name: "Steel engraving".into(),
            laser: LaserMode::Fiber,
            thickness_mm: 0.,
            values: Values::Bank(1),
            gas: None,
        })
        .unwrap();
    let set = |c: &mut openlaser_server::Coordinator, key: &str, value: &str| {
        c.update_recipe(
            &recipe.id,
            RecipeChange { attributes: [(key.into(), value.into())].into(), ..Default::default() },
        )
        .unwrap();
    };
    set(&mut c, "OpenLaserProcess", "engrave");
    set(&mut c, "CutSpeed", "100");
    c.open_part(&part.id).unwrap();
    c.set_recipe(&recipe.id).unwrap();
    // Edited in the material library while the job is open: no new choice needed.
    set(&mut c, "CutSpeed", "250");
    set(&mut c, "OpenLaserEngravingHeight", "fixed");
    let draft = c.document().draft.unwrap();
    let job = draft.recipe.as_ref().unwrap();
    assert_eq!(job.attributes["CutSpeed"], "250");
    assert_eq!(job.attributes["OpenLaserEngravingHeight"], "fixed");
}
