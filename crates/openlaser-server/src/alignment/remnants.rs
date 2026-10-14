// SPDX-License-Identifier: GPL-3.0-or-later
//! Sketch-defined remnants share the existing stock database and nesting path.
use super::{
    geometry::{Shape, Template},
    *,
};
use axum::extract::Path;
use openlaser_core::geometry::{Drawing, Point};
use openlaser_core::nesting::NestStock;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemnantRef {
    pub id: String,
    pub revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SketchRemnant {
    pub id: Option<String>,
    pub revision: Option<u64>,
    pub name: String,
    pub material: String,
    pub thickness_mm: f64,
    pub mode: openlaser_core::LaserMode,
    pub drawing: Drawing,
    pub inspected: bool,
}

pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/alignment/remnant/{id}",
            get(|s, p| async move { load(s, p).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/alignment/stock",
            get(|s| async move { stock(s).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/alignment/sketch",
            post(|j| async move { sketch(j).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/remnants/sketch",
            post(|s, j| async move { save(s, j).await.map_err(crate::api::Failure::from) }),
        )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SketchSource {
    dxf: String,
}
async fn sketch(Json(source): Json<SketchSource>) -> Result<Json<Drawing>> {
    let imported =
        openlaser_dxf::import(source.dxf.as_bytes()).map_err(|e| Error::Request(e.to_string()))?;
    if !imported.skipped.is_empty() || !imported.warnings.is_empty() {
        return Err(Error::Refused(
            "Sketch import needs review before it can define remaining material".into(),
        ));
    }
    Ok(Json(imported.drawing))
}
fn drawing(stock: NestStock) -> Result<Drawing> {
    match stock {
        NestStock::Remnant { outline, cutouts, .. } => {
            Ok(Drawing { contours: std::iter::once(outline).chain(cutouts).collect() })
        }
        _ => Err(Error::Refused("Choose a remnant as nesting stock first".into())),
    }
}
fn loaded(c: &Coordinator, id: &str, d: Drawing) -> Result<Template> {
    let record = c.sheet_store.view(id)?;
    let shape = Shape::Special { drawing: d };
    shape.drawing()?;
    let d = shape.drawing()?;
    let b = d.contours[0].bounds().ok_or_else(|| Error::Refused("Empty remnant".into()))?;
    if let Some(saved) = c
        .alignment
        .saved
        .templates
        .get(id)
        .filter(|t| t.remnant.as_ref().is_some_and(|r| r.revision == record.revision))
    {
        let old = saved.shape.drawing()?;
        let old_b =
            old.contours[0].bounds().ok_or_else(|| Error::Refused("Empty remnant".into()))?;
        let shift = b.min - old_b.min;
        let mut t = saved.clone();
        t.shape = shape;
        t.drawing_point = [t.drawing_point[0] + shift.x, t.drawing_point[1] + shift.y];
        return Ok(t);
    }
    Ok(Template {
        shape,
        picks: vec![],
        drawing_point: ((b.min + b.max) * 0.5).into(),
        machine_point: ((b.min + b.max) * 0.5).into(),
        search_mm: 5.,
        angle_deg: 3.,
        tolerance_mm: 0.1,
        every_job: true,
        remnant: Some(RemnantRef { id: id.into(), revision: record.revision }),
    })
}
async fn load(State(shared): State<Shared>, Path(id): Path<String>) -> Result<Json<Template>> {
    let c = shared.lock().await;
    let d = drawing(c.sheet_store.stock(&id, Point::ORIGIN)?)?;
    Ok(Json(loaded(&c, &id, d)?))
}
async fn stock(State(shared): State<Shared>) -> Result<Json<Template>> {
    let c = shared.lock().await;
    let s = c
        .draft
        .as_ref()
        .and_then(|d| d.current.nesting.as_ref())
        .map(|n| n.stock.clone())
        .ok_or_else(|| Error::Refused("Choose a remnant as nesting stock first".into()))?;
    let NestStock::Remnant { ref reference, .. } = s else {
        return Err(Error::Refused("Choose a remnant as nesting stock first".into()));
    };
    c.sheet_store.validate_stock(&s)?;
    let id = reference.clone();
    let d = drawing(s)?;
    Ok(Json(loaded(&c, &id, d)?))
}
pub(super) fn validate_binding(c: &Coordinator, t: &Template) -> Result<()> {
    let Some(r) = &t.remnant else {
        return Ok(());
    };
    let record = c.sheet_store.view(&r.id)?;
    if record.used || record.revision != r.revision {
        return Err(Error::Refused(
            "Remnant changed or was consumed; reload its reference and measurement template".into(),
        ));
    }
    let supplied = t.shape.drawing()?;
    let at =
        supplied.contours[0].bounds().ok_or_else(|| Error::Refused("Empty remnant".into()))?.min;
    let stored = drawing(c.sheet_store.stock(&r.id, at)?)?;
    if stored != supplied {
        return Err(Error::Refused("Remnant reference differs from its stored contour".into()));
    }
    // A library reference alone is not enough for a nested job: bind the exact frame.
    let stock = c.draft.as_ref().and_then(|d| d.current.nesting.as_ref()).map(|n| &n.stock);
    if let Some(NestStock::Remnant { reference, outline, cutouts, .. }) = stock {
        let expected =
            Drawing { contours: std::iter::once(outline.clone()).chain(cutouts.clone()).collect() };
        if reference != &r.id
            || serde_json::to_value(&expected).ok() != serde_json::to_value(t.shape.drawing()?).ok()
        {
            return Err(Error::Refused(
                "Load the current nesting stock as alignment reference".into(),
            ));
        }
    }
    Ok(())
}
async fn save(
    State(shared): State<Shared>,
    Json(change): Json<SketchRemnant>,
) -> Result<Json<serde_json::Value>> {
    let mut c = shared.lock().await;
    c.alignment_idle()?;
    if !change.inspected {
        return Err(Error::Refused(
            "Confirm the sketch matches the actual remaining material".into(),
        ));
    }
    Shape::Special { drawing: change.drawing.clone() }.drawing()?;
    let id = c.sheet_store.save_sketch(&change)?;
    c.alignment.invalidate();
    c.library_changed();
    Ok(Json(serde_json::json!({"id":id})))
}
