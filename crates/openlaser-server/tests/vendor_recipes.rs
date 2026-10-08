// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline audit of operator-supplied vendor recipes against a machine backup.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "explicit offline acceptance fixture")]
mod common;

use openlaser_server::{Coordinator, machine};
use serde_json::json;

#[tokio::test]
#[ignore = "requires OPENLASER_VENDOR_RECIPES, OPENLASER_TEST_BACKUP and OPENLASER_VENDOR_REPORT"]
async fn vendor_recipes_compile_on_a_plate_with_holes() {
    let simulator = openlaser_controller::Simulator::start().await.unwrap();
    let mut config = common::config("vendor-recipes", simulator.config());
    config.files.backup = Some(std::env::var("OPENLASER_TEST_BACKUP").unwrap().into());
    config.co2_manual_focus = true;
    let destination = config.data_dir.join("recipes");
    std::fs::create_dir_all(&destination).unwrap();
    let source = std::env::var("OPENLASER_VENDOR_RECIPES").unwrap();
    let mut count = 0;
    for entry in std::fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let recipe: openlaser_library::Recipe = serde_json::from_slice(&bytes).unwrap();
        if !recipe.name.contains("(Gweike)") {
            continue;
        }
        std::fs::write(destination.join(path.file_name().unwrap()), bytes).unwrap();
        count += 1;
    }
    assert!(count > 0, "no Gweike recipes found");
    let shared = Coordinator::start(config).unwrap();
    common::seed(&shared, &simulator).await;
    let recipes = shared.lock().await.library.recipes().cloned().collect::<Vec<_>>();
    let part = shared
        .lock()
        .await
        .import_part(
            "audit-plate.dxf",
            &std::fs::read(common::fixture("dxf/plate-with-holes.dxf")).unwrap(),
        )
        .unwrap();
    let mut rows = Vec::new();
    for recipe in recipes {
        let result = async {
            {
                let mut c = shared.lock().await;
                c.open_part(&part.id)?;
                c.set_recipe(&recipe.id)?;
            }
            machine::prepare(&shared).await?;
            machine::compile(&shared, false).await?;
            Ok::<_, openlaser_server::Error>(())
        }
        .await;
        let d = shared.lock().await.document();
        let compiled = d.draft.as_ref().and_then(|d| d.compiled.as_ref());
        let gas = recipe.attributes.get("CutGasType");
        let air_source = recipe.note.to_lowercase().contains("1200w_air");
        let row = json!({
            "id": recipe.id, "name": recipe.name, "laser": recipe.laser,
            "source": recipe.file_name, "note": recipe.note,
            "gas_index": gas, "air_source_uses_n2": air_source && gas.is_some_and(|g| g == "5"),
            "speed_mm_s": recipe.attributes.get("CutSpeed"),
            "height_mm": recipe.attributes.get("CutHeight"),
            "pressure_source_value": recipe.attributes.get("CutAirPressure"),
            "compiled": result.is_ok(), "error": result.err().map(|e| e.to_string()),
            "passes": compiled.map(|c| c.plan.len()), "seconds": compiled.map(|c| c.seconds),
            "blocks": compiled.map(|c| c.blocks),
        });
        println!("VENDOR_RECIPE {row}");
        rows.push(row);
    }
    let report = std::env::var("OPENLASER_VENDOR_REPORT").unwrap();
    std::fs::write(report, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
    openlaser_server::shutdown(&shared).await.unwrap();
    assert_eq!(rows.len(), count);
    let failures = rows.iter().filter(|r| r["compiled"] != true).collect::<Vec<_>>();
    assert!(failures.is_empty(), "vendor compilation failures: {failures:?}");
}
