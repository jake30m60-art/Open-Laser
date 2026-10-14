// SPDX-License-Identifier: GPL-3.0-or-later
//! Workpiece alignment: persistent search intent, isolated offline metrology,
//! and a server-side interlock. No native edge-seeking protocol is assumed.
pub(crate) mod geometry;
pub(crate) mod remnants;
#[cfg(test)]
mod tests;
mod workflow;

use crate::coordinator::{Coordinator, Shared};
use crate::{Error, Result};
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use geometry::{Fault, Fit, Plan, Template};
use serde::{Deserialize, Serialize};
use workflow::Workflow;

pub(crate) const BLOCKED: &str = "Workpiece alignment: real edge measurement and lid verification are not available; cutting is locked";
const FILE: &str = "workpiece-alignment.json";

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    enabled: bool,
    template: Option<Template>,
    #[serde(default)]
    templates: std::collections::BTreeMap<String, Template>,
}

#[derive(Default)]
pub(crate) struct Session {
    saved: Saved,
    revision: u64,
    flow: Workflow,
    result: Option<Fit>,
    result_stamp: Option<String>,
}
impl Session {
    pub fn open(root: &std::path::Path) -> Result<Self> {
        let bytes = match std::fs::read(root.join(FILE)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(Error::Refused(e.to_string())),
        };
        let saved: Saved = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Refused(format!("Alignment template: {e}")))?;
        if let Some(t) = &saved.template {
            t.shape.drawing()?;
        }
        // Enabled intent survives restart; measured authority never does.
        Ok(Self { saved, ..Self::default() })
    }
    pub fn guard(&self) -> Result<()> {
        if self.saved.enabled { Err(Error::Refused(BLOCKED.into())) } else { Ok(()) }
    }
    pub fn invalidate(&mut self) {
        self.flow.invalidate();
        self.result = None;
        self.result_stamp = None;
    }
}

impl Coordinator {
    fn alignment_stamp(&self) -> String {
        format!(
            "{}/{}/{:?}/{:?}",
            self.document().draft_revision,
            self.draft_generation,
            self.machine.state().configuration,
            self.machine.state().session.homed
        )
    }
    fn alignment_idle(&self) -> Result<()> {
        self.idle()?;
        if self.held.is_some() {
            return Err(Error::Refused(
                "Finish or stop the retained job before changing alignment".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct View {
    revision: u64,
    enabled: bool,
    template: Option<Template>,
    phase: workflow::Phase,
    result: Option<Fit>,
    hardware_available: bool,
    lid_verified: bool,
}
fn view(c: &Coordinator) -> View {
    let a = &c.alignment;
    let stale = a.result_stamp.as_ref().is_some_and(|s| *s != c.alignment_stamp());
    View {
        revision: a.revision,
        enabled: a.saved.enabled,
        template: a.saved.template.clone(),
        phase: if stale { workflow::Phase::Stale } else { a.flow.phase },
        result: if stale { None } else { a.result.clone() },
        hardware_available: false,
        lid_verified: false,
    }
}

pub(crate) fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/api/alignment",
            get(status)
                .post(|s, j| async move { change(s, j).await.map_err(crate::api::Failure::from) }),
        )
        .merge(remnants::routes())
        .route(
            "/api/alignment/plan",
            post(|j| async move { plan(j).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/alignment/associate",
            post(|s, j| async move { associate(s, j).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/alignment/measure",
            post(|s| async move { measure(s).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/alignment/simulate",
            post(|s, j| async move { simulate(s, j).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/alignment/cancel",
            post(|s| async move { cancel(s).await.map_err(crate::api::Failure::from) }),
        )
        .route(
            "/api/alignment/reference",
            get(|s| async move { reference(s).await.map_err(crate::api::Failure::from) }),
        )
}
async fn status(State(shared): State<Shared>) -> Json<View> {
    Json(view(&*shared.lock().await))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    revision: u64,
    enabled: bool,
    template: Option<Template>,
}
async fn change(State(shared): State<Shared>, Json(request): Json<Change>) -> Result<Json<View>> {
    let mut c = shared.lock().await;
    c.alignment_idle()?;
    if c.alignment.revision != request.revision {
        return Err(Error::Refused(
            "Alignment changed on another screen; reopen the dialog".into(),
        ));
    }
    // Incomplete point selections may be saved; only the planner admits measurement.
    if let Some(t) = &request.template {
        t.shape.drawing()?;
        if t.picks.len() > 32 {
            return Err(Error::Request("Too many measurement points".into()));
        }
    }
    let mut templates = c.alignment.saved.templates.clone();
    if let Some(t) = &request.template {
        if request.enabled {
            remnants::validate_binding(&c, t)?;
        }
        if let Some(r) = &t.remnant {
            templates.insert(r.id.clone(), t.clone());
        }
    }
    let saved = Saved { enabled: request.enabled, template: request.template, templates };
    let bytes = serde_json::to_vec_pretty(&saved).map_err(|e| Error::Request(e.to_string()))?;
    openlaser_library::atomic_write(&c.config.data_dir.join(FILE), &bytes)?;
    c.alignment.saved = saved;
    c.alignment.revision += 1;
    c.alignment.invalidate();
    c.publish();
    Ok(Json(view(&c)))
}
async fn plan(Json(template): Json<Template>) -> Result<Json<Plan>> {
    Ok(Json(template.plan()?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Association {
    revision: u64,
    drawing_point: [f64; 2],
}
async fn associate(
    State(shared): State<Shared>,
    Json(request): Json<Association>,
) -> Result<Json<View>> {
    let mut c = shared.lock().await;
    c.alignment_idle()?;
    if c.alignment.revision != request.revision {
        return Err(Error::Refused(
            "Alignment changed on another screen; reopen the dialog".into(),
        ));
    }
    let state = c.machine.state();
    if !state.session.homed
        || !matches!(state.connection, openlaser_controller::state::Connection::Connected { .. })
    {
        return Err(Error::Refused("Home XY before associating the head".into()));
    }
    let f = state
        .feedback
        .as_ref()
        .filter(|f| {
            f.age_ms <= crate::coordinator::FRESH_FEEDBACK_MS
                && f.stationary
                && f.head.command == 0
                && f.referenced.iter().all(|v| *v)
        })
        .ok_or_else(|| Error::Refused("Fresh stationary feedback is required".into()))?;
    if request.drawing_point.iter().any(|x| !x.is_finite() || x.abs() > 100000.) {
        return Err(Error::Request("Invalid drawing point".into()));
    }
    let mut saved = c.alignment.saved.clone();
    let t = saved
        .template
        .as_mut()
        .ok_or_else(|| Error::Refused("Save a measurement template first".into()))?;
    t.drawing_point = request.drawing_point;
    t.machine_point = [f.position_mm[0], f.position_mm[1]];
    if let Some(r) = &t.remnant {
        saved.templates.insert(r.id.clone(), t.clone());
    }
    let bytes = serde_json::to_vec_pretty(&saved).map_err(|e| Error::Request(e.to_string()))?;
    openlaser_library::atomic_write(&c.config.data_dir.join(FILE), &bytes)?;
    c.alignment.saved = saved;
    c.alignment.revision += 1;
    c.alignment.invalidate();
    c.publish();
    Ok(Json(view(&c)))
}
async fn measure(State(shared): State<Shared>) -> Result<Json<View>> {
    let mut c = shared.lock().await;
    c.alignment_idle()?;
    c.alignment.invalidate();
    c.publish();
    Err(Error::Refused(BLOCKED.into()))
}
async fn cancel(State(shared): State<Shared>) -> Result<Json<View>> {
    let mut c = shared.lock().await;
    c.alignment.invalidate();
    c.alignment.revision += 1;
    c.publish();
    Ok(Json(view(&c)))
}
async fn reference(
    State(shared): State<Shared>,
) -> Result<Json<openlaser_core::geometry::Drawing>> {
    let c = shared.lock().await;
    let d = c.draft.as_ref().ok_or_else(|| Error::Refused("Open a part first".into()))?;
    Ok(Json(crate::draft::place(d.drawing()?, &d.current.placed)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Simulation {
    revision: u64,
    delta: [f64; 2],
    angle_deg: f64,
    fault: Fault,
}
#[derive(Serialize)]
struct SimulationReply {
    view: View,
    plan: Plan,
    compiled: Option<std::sync::Arc<crate::document::Compiled>>,
    compile_problem: Option<String>,
}
async fn simulate(
    State(shared): State<Shared>,
    Json(request): Json<Simulation>,
) -> Result<Json<SimulationReply>> {
    let (template, ticket, stamp, stop_epoch) = {
        let mut c = shared.lock().await;
        c.alignment_idle()?;
        if c.alignment.revision != request.revision {
            return Err(Error::Refused(
                "Alignment changed on another screen; reopen the dialog".into(),
            ));
        }
        let t = c
            .alignment
            .saved
            .template
            .clone()
            .ok_or_else(|| Error::Refused("Save a measurement template first".into()))?;
        if !c.alignment.saved.enabled {
            return Err(Error::Refused(
                "Enable alignment before simulating; this locks machine Start".into(),
            ));
        }
        remnants::validate_binding(&c, &t)?;
        let stamp = c.alignment_stamp();
        c.alignment.result = None;
        c.alignment.result_stamp = None;
        let ticket = c.alignment.flow.begin(stamp.clone());
        c.publish();
        (t, ticket, stamp, shared.ensure_running()?)
    };
    let result = tokio::task::spawn_blocking(move || {
        let plan = template.plan()?;
        let fit =
            geometry::simulate(&template, &plan, request.delta, request.angle_deg, request.fault)?;
        Ok::<_, Error>((plan, fit))
    })
    .await
    .map_err(|e| Error::Refused(e.to_string()))?;
    let mut c = shared.lock().await;
    let current = c.alignment_stamp();
    let stopped = shared.ensure_running().ok() != Some(stop_epoch);
    if stopped {
        c.alignment.invalidate();
    }
    if !c.alignment.flow.finish(ticket, &current, result.is_ok(), 0) {
        c.alignment.result = None;
        c.publish();
        return Err(result.err().unwrap_or_else(|| {
            Error::Refused("Measurement was cancelled or became stale".into())
        }));
    }
    let (plan, fit) = result?;
    c.alignment.result = Some(fit.clone());
    c.alignment.result_stamp = Some(stamp.clone());
    c.publish();
    // Compiler receives a transformed COPY. It is never installed as a runnable draft.
    let profile = c.draft.as_ref().and_then(|d| d.current.correction.clone());
    let inputs = c.compile_inputs(false);
    let extent = c.extent();
    drop(c);
    let searches = plan.searches.clone();
    let compiled = tokio::task::spawn_blocking(move || {
        let inputs = inputs?;
        let shifted = inputs.prepared.shifted(inputs.settings.contour_shift)?;
        let aligned = geometry::prepare(&shifted, fit.pose);
        let corrected = crate::correction::prepare(&aligned, profile.as_ref(), [0., 0.])?;
        let p = crate::draft::compile(
            &corrected,
            &inputs.settings,
            inputs.film.as_ref(),
            &inputs.layered,
            false,
            inputs.scale,
        )?;
        let bounds = crate::envelope::process_bounds(
            &p.job,
            u32::try_from(inputs.scale).map_err(|_| Error::Refused("Invalid scale".into()))?,
        )?;
        let bed = extent.ok_or_else(|| Error::Refused("Machine travel is unknown".into()))?;
        for s in &searches {
            for p in [s.start, s.end] {
                if (0..2).any(|i| p[i] < bed[i][0] || p[i] > bed[i][1]) {
                    return Err(Error::Refused("Search corridor leaves machine travel".into()));
                }
            }
        }
        for p in [bounds.min, bounds.max] {
            if p.x < bed[0][0] || p.x > bed[0][1] || p.y < bed[1][0] || p.y > bed[1][1] {
                return Err(Error::Refused("Aligned toolpath leaves machine travel".into()));
            }
        }
        Ok::<_, Error>(p.view)
    })
    .await
    .map_err(|e| Error::Refused(e.to_string()))?;
    let c = shared.lock().await;
    if c.alignment_stamp() != stamp
        || !c.alignment.flow.current(ticket)
        || shared.ensure_running().ok() != Some(stop_epoch)
    {
        return Err(Error::Refused("Measurement was cancelled or became stale".into()));
    }
    let (compiled, compile_problem) = match compiled {
        Ok(p) => (Some(p), None),
        Err(e) => (None, Some(e.to_string())),
    };
    Ok(Json(SimulationReply { view: view(&c), plan, compiled, compile_problem }))
}
