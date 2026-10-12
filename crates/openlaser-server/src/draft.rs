// SPDX-License-Identifier: GPL-3.0-or-later

//! The job being set up: the contours of its parts' joined drawing placed
//! on the sheet, copies and all, each where the operator put it; prepared
//! into a toolpath, and compiled into a job the run page binds to the bed.
//! Every edit to the placement, the parts or the features can be undone.
//! The recipe's contour shift moves the prepared geometry once, at compile
//! time, and the film pass traces each contour's bare geometry under the
//! film recipe.

use crate::Result;
use crate::document::{
    Compiled, DraftView, FeatureSource, Move, Path, PathKind, PickView, Preview, PreviewContour,
};
use openlaser_compiler::cut;
use openlaser_compiler::pass::PassKind;
use openlaser_compiler::program::{Binding, Film, Job, Kind, Program};
use openlaser_compiler::settings::{Settings, Timeouts};
use openlaser_core::features::OrderStrategy;
use openlaser_core::features::{Features, LeadOverride};
use openlaser_core::geometry::{Bounds, Contour, Curve, Drawing, Placed, Point, Transform};
use openlaser_core::grouping::Grouping;
use openlaser_core::nesting::{NestStock, Nesting};
use openlaser_core::toolpath::{LeadRole, PreparedSegment, SegmentProcess, Toolpath};
use openlaser_library::{Anchor, Id, JobDrawing, Recipe};
use openlaser_protocol::records;
use openlaser_xml::Bundle;
use openlaser_xml::recipe::Request;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Largest contour shift, in millimetres, on either axis: the older host's
/// own domain for the setting.
const MAX_CONTOUR_SHIFT_MM: f64 = 100_000.;

/// How many edits can be undone.
const HISTORY: usize = 100;

/// The job being set up: its parts placed on the sheet, the recipe and
/// features, the edit history, and what was prepared and compiled from them.
///
/// Everything an edit can change, and so everything undo and redo restore,
/// lives in [`Snapshot`] as [`Draft::current`]. The other fields are the
/// draft's identity, choices kept outside the edit history, and views
/// rebuilt from the snapshot.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent run choice, coupon geometry, origin lifetime and preparation status"
)]
pub struct Draft {
    /// Identity of an unsaved setup, independent of its source part. Absent
    /// on legacy part-keyed drafts, which remain explicitly recoverable.
    #[serde(default)]
    pub(crate) workspace_id: Option<Id>,
    /// Operator's run choice, retained when geometry is rebuilt.
    #[serde(default)]
    pub dry_run: bool,
    /// The undoable authoring state: parts, placement, layout, features and
    /// recipe. Its fields are stored inline, beside the draft's own.
    #[serde(flatten)]
    pub current: Snapshot,
    /// Connection epoch in which a transient head position was captured.
    #[serde(skip)]
    pub capture_epoch: Option<u64>,
    /// This capture has already started a physical cut; a fresh run replaces it.
    #[serde(skip)]
    pub capture_used: bool,
    /// Calibration coupons must retain their nominal geometry.
    #[serde(default)]
    pub calibration: bool,
    /// Per filled marked layer, the scan offsets measured at its speed by
    /// angle, as the coordinator found them for this preparation.
    #[serde(skip)]
    pub scan_tables: std::collections::BTreeMap<String, Vec<[f64; 2]>>,
    /// Minimum laser-off scan ramps from active machine motion settings.
    #[serde(skip)]
    pub scan_runups: std::collections::BTreeMap<String, f64>,
    /// Tessellated stock reference for the canvas; never a cutting contour.
    #[serde(skip)]
    pub stock_outline: Vec<[f64; 2]>,
    /// Existing removed material for the canvas, excluded from cutting.
    #[serde(skip)]
    pub stock_cutouts: Vec<Vec<[f64; 2]>>,
    /// Source contour count for implicit placements in older saved jobs.
    #[serde(default)]
    pub source_count: usize,
    /// The saved value this working copy started from, for three-way merging.
    #[serde(default)]
    pub saved_base: Option<openlaser_library::Job>,
    /// The joined drawing of the parts, attached whenever they change.
    #[serde(skip)]
    sources: Option<Arc<JobDrawing>>,
    /// The saved job it was opened from, if any.
    pub job: Option<Id>,
    /// The contours that move together.
    #[serde(skip)]
    pub groups: Vec<Vec<usize>>,
    /// The states before each edit, latest last.
    past: Vec<Arc<Snapshot>>,
    /// The states each undo left, latest last.
    future: Vec<Arc<Snapshot>>,
    /// The sheet for the canvas: the prepared toolpath, or the contours as
    /// they lie when preparation fails, so they can still be moved apart.
    #[serde(skip)]
    pub preview: Option<Arc<Preview>>,
    /// The prepared toolpath.
    #[serde(skip)]
    pub prepared: Option<Arc<Prepared>>,
    /// The compiled job.
    #[serde(skip)]
    pub compiled: Option<Arc<CompiledJob>>,
    /// Why preparation or compilation failed.
    #[serde(skip)]
    pub error: Option<String>,
    /// Whether a revision-aware preparation task is needed.
    #[serde(skip)]
    pub preparing: bool,
}

/// The parts, the placement, the layout, the features and the recipe at one
/// point of the editing history: what one edit can change and one undo
/// restores. The draft holds the current one; its history holds the rest.
///
/// The field names and order are the stored format of retained drafts and
/// their history, so they must not change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The parts it cuts, whose drawings join in this order into its
    /// drawing; stored as a saved job stores them. Absent from history kept
    /// before a job could cut several parts, when every step had the
    /// draft's one part.
    #[serde(default, rename = "part", with = "openlaser_library::stored_parts")]
    pub parts: Vec<Id>,
    /// Saved placement intent; absent only on legacy drafts.
    #[serde(default)]
    pub placement: Option<openlaser_library::placement::Placement>,
    /// Other sheets retained alongside the active geometry.
    #[serde(default)]
    pub sheets: Option<crate::sheets::SheetSet>,
    /// Correction snapshot, applied after final placement.
    #[serde(default)]
    pub correction: Option<openlaser_correction::Profile>,
    /// Stock boundary and nesting settings.
    #[serde(default)]
    pub nesting: Option<openlaser_core::nesting::Nesting>,
    /// The drawing's contours on the sheet, copies and all.
    pub placed: Vec<Placed>,
    /// Operator overrides to automatic part grouping.
    #[serde(default)]
    pub grouping: Grouping,
    /// The machining features.
    pub features: Features,
    /// The recipe, once chosen.
    pub recipe: Option<Recipe>,
    /// The film process the recipe refers to, snapshotted with it.
    pub film: Option<Recipe>,
    /// How each drawing layer is cut; absent from drafts kept before
    /// layers had their own recipes, and left out when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<openlaser_library::LayerChoice>,
    /// The sheet offset: what the machine adds to a drawing coordinate to
    /// reach the bed, which is where the sheet lies on the bed. Absent until
    /// the sheet is placed. Stored as `zero`, its name in earlier builds.
    #[serde(rename = "zero")]
    pub sheet_offset: Option<[f64; 2]>,
    /// Which point of the placed part the origin stands for.
    pub anchor: Anchor,
    /// Operator checklist policy, saved with the job.
    pub preflight: openlaser_library::preflight::JobPreflight,
    /// Where the features came from.
    pub feature_source: Option<FeatureSource>,
}

/// An immutable authoring snapshot, excluding prepared geometry and other
/// runtime views. History snapshots are immutable and shared, so unchanged
/// history can be recognized without walking every old placement again.
#[derive(Clone, Serialize)]
pub(crate) struct AuthoringState {
    workspace_id: Option<Id>,
    dry_run: bool,
    calibration: bool,
    source_count: usize,
    job: Option<Id>,
    saved_base: Option<openlaser_library::Job>,
    #[serde(flatten)]
    current: Snapshot,
    past: Vec<Arc<Snapshot>>,
    future: Vec<Arc<Snapshot>>,
}

impl AuthoringState {
    /// The draft again, without its drawing: the coordinator attaches that.
    pub(crate) fn draft(&self) -> Draft {
        let mut draft = Draft::blank(self.current.parts.clone(), self.source_count);
        draft.workspace_id.clone_from(&self.workspace_id);
        draft.dry_run = self.dry_run;
        draft.load_snapshot(self.current.clone());
        draft.calibration = self.calibration;
        draft.job.clone_from(&self.job);
        draft.saved_base.clone_from(&self.saved_base);
        draft.past.clone_from(&self.past);
        draft.future.clone_from(&self.future);
        draft
    }

    /// How many parts the draft cuts.
    pub(crate) fn parts(&self) -> usize {
        self.current.parts.len()
    }

    pub(crate) fn matches(&self, draft: &Draft) -> bool {
        let same_history = |a: &[Arc<Snapshot>], b: &[Arc<Snapshot>]| {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| Arc::ptr_eq(a, b))
        };
        self.workspace_id == draft.workspace_id
            && self.dry_run == draft.dry_run
            && self.calibration == draft.calibration
            && self.source_count == draft.source_count
            && self.job == draft.job
            && self.saved_base == draft.saved_base
            && same_history(&self.past, &draft.past)
            && same_history(&self.future, &draft.future)
            && self.current == draft.current
    }
}

/// The placed part prepared into a toolpath.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The contours the compiler takes, in cutting order.
    pub contours: Vec<cut::Contour>,
    /// Per contour, the bare geometry a film pass traces, split where it
    /// breaks.
    pub film: Vec<Vec<cut::Contour>>,
    /// The contour shift already applied to the geometry.
    pub shift: [f64; 2],
    /// The canvas view.
    pub preview: Arc<Preview>,
    /// The physical source instances of each prepared path.
    pub instances: Arc<Vec<Vec<crate::document::InstanceView>>>,
}

impl Prepared {
    /// The geometry shifted to `target`, as the older host shifts it: by
    /// the difference from what is applied, so a shift is never applied
    /// twice, and never further than its domain of 100 000 mm.
    pub fn shifted(&self, target: [f64; 2]) -> Result<Self> {
        if target.iter().any(|v| !v.is_finite() || v.abs() > MAX_CONTOUR_SHIFT_MM) {
            return Err(crate::Error::Refused(
                "the contour shift must be finite and within 100 000 mm".into(),
            ));
        }
        let delta = [target[0] - self.shift[0], target[1] - self.shift[1]];
        if delta == [0., 0.] {
            return Ok(self.clone());
        }
        Ok(Self {
            contours: self.contours.iter().map(|c| c.translated(delta)).collect(),
            film: self
                .film
                .iter()
                .map(|runs| runs.iter().map(|c| c.translated(delta)).collect())
                .collect(),
            shift: target,
            preview: self.preview.clone(),
            instances: self.instances.clone(),
        })
    }
}

/// A compiled job, bound to a position at run time.
#[derive(Clone, Debug)]
pub struct CompiledJob {
    /// Whether it is a dry run.
    pub dry_run: bool,
    /// The controller scale it was planned for.
    pub scale: i32,
    /// Exact live configuration, absent for an offline preview.
    pub configuration: Option<openlaser_controller::session::Configuration>,
    /// The contour shift the geometry was compiled under.
    pub shift: [f64; 2],
    /// The planned passes.
    pub job: Arc<Job>,
    /// The summary.
    pub view: Arc<Compiled>,
}

impl Draft {
    /// Captures exactly the state stored with a retained draft. No compiled
    /// or preview geometry is kept alive by the persistence cache.
    pub(crate) fn authoring_state(&self) -> AuthoringState {
        AuthoringState {
            workspace_id: self.workspace_id.clone(),
            dry_run: self.dry_run,
            calibration: self.calibration,
            source_count: self.source_count,
            job: self.job.clone(),
            saved_base: self.saved_base.clone(),
            current: self.current.clone(),
            past: self.past.clone(),
            future: self.future.clone(),
        }
    }

    /// A draft cutting the parts of `sources`, with nothing chosen and
    /// every part as drawn: each its own copy, so parts never group together.
    #[must_use]
    pub fn new(sources: Arc<JobDrawing>) -> Self {
        let mut draft = Self::blank(sources.parts().cloned().collect(), sources.contours());
        draft.current.placed = (0..sources.parts().len())
            .flat_map(|part| {
                let copy = u32::try_from(part).unwrap_or(u32::MAX);
                sources.range(part).map(move |source| Placed { copy, ..Placed::drawn(source) })
            })
            .collect();
        draft.sources = Some(sources);
        draft.engrave_new_images();
        draft
    }

    /// Writes the engraving defaults for image layers that have no entry
    /// yet into the job, so every screen sees them as engraving.
    fn engrave_new_images(&mut self) {
        let layers = self.image_layers();
        engrave_images(&mut self.current.features, &layers);
    }

    /// A draft of `parts` without a drawing attached, the drawing of
    /// `contours` contours as drawn.
    fn blank(parts: Vec<Id>, contours: usize) -> Self {
        Self {
            workspace_id: Some(Id::generate()),
            dry_run: false,
            current: Snapshot {
                parts,
                placement: Some(openlaser_library::placement::Placement::Head {}),
                sheets: None,
                correction: None,
                nesting: None,
                placed: Placed::all(contours),
                grouping: Grouping::default(),
                features: Features::default(),
                recipe: None,
                film: None,
                layers: Vec::new(),
                sheet_offset: None,
                anchor: Anchor::default(),
                preflight: openlaser_library::preflight::JobPreflight::default(),
                feature_source: None,
            },
            capture_epoch: None,
            capture_used: false,
            calibration: false,
            scan_tables: std::collections::BTreeMap::new(),
            scan_runups: std::collections::BTreeMap::new(),
            stock_outline: Vec::new(),
            stock_cutouts: Vec::new(),
            source_count: contours,
            saved_base: None,
            sources: None,
            job: None,
            groups: Vec::new(),
            past: Vec::new(),
            future: Vec::new(),
            preview: None,
            prepared: None,
            compiled: None,
            error: None,
            preparing: false,
        }
    }

    /// Places and prepares the job's contours; a compiled job no longer applies.
    pub fn prepare(&mut self, drawing: &Drawing) {
        self.preparing = false;
        self.compiled = None;
        self.stock_cutouts = self
            .current
            .nesting
            .as_ref()
            .map(|n| {
                crate::nesting::cutouts(n)
                    .iter()
                    .filter_map(|c| openlaser_nest::polygon(c).ok())
                    .collect()
            })
            .unwrap_or_default();
        self.stock_outline = self
            .current
            .nesting
            .as_ref()
            .and_then(|n| crate::nesting::stock(drawing, n).ok())
            .and_then(|c| openlaser_nest::polygon(&c).ok())
            .unwrap_or_default();
        let automatic = groups(drawing, &self.current.placed, &[]);
        let matching = crate::correspondence::matching(drawing, &self.current.placed, &automatic);
        self.groups = self.current.grouping.resolve(automatic);
        let features = self.calibrated_features();
        match prepare(drawing, &self.current.placed, &features).and_then(|prepared| {
            crate::nesting::check_prepared(drawing, self, &prepared)?;
            Ok(prepared)
        }) {
            Ok(mut prepared) => {
                let preview = Arc::make_mut(&mut prepared.preview);
                let mut owners = vec![0; self.current.placed.len()];
                for target in preview.contours.iter().filter_map(|c| c.lead_target) {
                    owners[target.contour] += 1;
                }
                for contour in &mut preview.contours {
                    // Split or joined contours have no unambiguous counterpart.
                    contour.matching_contour = contour
                        .lead_target
                        .filter(|target| owners[target.contour] == 1 && contour.sources.len() == 1)
                        .and_then(|target| matching[target.contour]);
                }
                Arc::make_mut(&mut prepared.preview).rasters = self.raster_views();
                join_groups(&mut self.groups, &prepared.preview.contours);
                self.preview = Some(prepared.preview.clone());
                self.prepared = Some(Arc::new(prepared));
                self.error = None;
            }
            Err(error) => {
                let mut partial = partial_preview(drawing, &self.current.placed, &features);
                partial.rasters = self.raster_views();
                self.preview = Some(Arc::new(partial));
                self.prepared = None;
                self.error = Some(error.to_string());
            }
        }
    }

    // Placement --------------------------------------------------------------

    /// The anchor point in drawing coordinates, once the layout is prepared.
    #[must_use]
    pub fn anchor_point(&self) -> Option<[f64; 2]> {
        let bounds = crate::placement::reference_bounds(self)?;
        Some(self.current.anchor.on(bounds.min.into(), bounds.max.into()))
    }

    /// The sheet offset: what the machine adds to a drawing coordinate.
    pub fn sheet_offset(&self) -> Result<[f64; 2]> {
        self.current.sheet_offset.ok_or_else(|| crate::Error::Refused("nothing prepared".into()))
    }

    /// The origin in machine coordinates: where the anchor point lies.
    #[must_use]
    pub fn origin(&self) -> Option<[f64; 2]> {
        let (zero, dock) = (self.current.sheet_offset?, self.anchor_point()?);
        Some([zero[0] + dock[0], zero[1] + dock[1]])
    }

    /// Puts the anchor point at `origin`, in machine coordinates.
    pub fn place_anchor_at(&mut self, origin: [f64; 2]) -> Result<()> {
        let dock =
            self.anchor_point().ok_or_else(|| crate::Error::Refused("nothing prepared".into()))?;
        self.current.sheet_offset = Some([origin[0] - dock[0], origin[1] - dock[1]]);
        Ok(())
    }

    /// Places the sheet again from the saved placement. A fixed origin is
    /// put back where it was. A legacy draft without a placement keeps its
    /// sheet offset, or starts at the bed's corner of least X and Y, and is given
    /// a fixed placement at that origin. A "where the head is" placement
    /// stays unplaced until a run captures the head's position.
    pub fn restore_placement(&mut self, bed: Option<[[f64; 2]; 2]>) {
        use openlaser_library::placement::Placement;
        match self.current.placement.clone() {
            Some(Placement::Head {}) => {}
            Some(Placement::Fixed { origin }) => {
                let _ = self.place_anchor_at(origin);
            }
            None => {
                if self.current.sheet_offset.is_none() {
                    let corner = bed.map_or([0., 0.], |b| [b[0][0], b[1][0]]);
                    let _ = self.place_anchor_at(corner);
                }
                if let Some(origin) = self.origin() {
                    self.current.placement = Some(Placement::Fixed { origin });
                }
            }
        }
    }

    // Editing ----------------------------------------------------------------

    /// Remembers the placement and the features before an edit.
    pub fn remember(&mut self) {
        self.past.push(Arc::new(self.current.clone()));
        if self.past.len() > HISTORY {
            self.past.remove(0);
        }
        self.future.clear();
    }

    /// Goes back one edit.
    pub fn undo(&mut self) -> Result<()> {
        self.step(true)
    }

    /// Goes forward through one edit undone.
    pub fn redo(&mut self) -> Result<()> {
        self.step(false)
    }

    fn step(&mut self, back: bool) -> Result<()> {
        let snapshot = if back { self.past.pop() } else { self.future.pop() }.ok_or_else(|| {
            crate::Error::Refused(if back { "nothing to undo" } else { "nothing to redo" }.into())
        })?;
        let current = Arc::new(self.current.clone());
        if back {
            self.future.push(current);
        } else {
            self.past.push(current);
        }
        let snapshot = Arc::unwrap_or_clone(snapshot);
        self.load_snapshot(snapshot);
        Ok(())
    }

    /// Loads a step of the history. When its parts differ, the drawing
    /// must be attached again before the geometry is used.
    fn load_snapshot(&mut self, snapshot: Snapshot) {
        self.current = snapshot;
    }

    /// Whether the authoring values differ from the saved job.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.saved_base.as_ref().map_or(
            self.current.recipe.is_some() || !self.past.is_empty(),
            |base| {
                self.current.sheets.is_some()
                    || self.current.parts != base.parts
                    || self.current.correction != base.correction
                    || self.calibration != base.calibration
                    || self.current.nesting != base.nesting
                    || self.current.grouping != base.grouping
                    || self.current.recipe.as_ref() != Some(&base.recipe)
                    || self.current.film != base.film
                    || self.current.features != base.features
                    || !crate::placement::matches_saved(self, base)
                    || self.current.anchor != base.anchor
                    || self.current.preflight != base.preflight
                    || self.current.placed
                        != if base.placed.is_empty() {
                            Placed::all(self.source_count)
                        } else {
                            base.placed.clone()
                        }
            },
        )
    }

    /// Checks persisted authoring state before it is made editable again.
    /// `contours` counts the drawing of a list of parts; history steps whose
    /// parts are gone are checked if an undo ever returns to them.
    pub fn validate_saved(&self, contours: impl Fn(&[Id]) -> Option<usize>) -> Result<()> {
        for placement in std::iter::once(&self.current.placement)
            .chain(self.past.iter().map(|s| &s.placement))
            .chain(self.future.iter().map(|s| &s.placement))
            .flatten()
        {
            placement.validate().map_err(crate::Error::Refused)?;
        }
        if let Some(profile) = &self.current.correction {
            profile.validate().map_err(|e| crate::Error::Refused(e.to_string()))?;
        }
        if self.calibration && self.current.correction.is_some() {
            return Err(crate::Error::Refused("calibration jobs cannot have correction".into()));
        }
        let count = contours(&self.current.parts)
            .ok_or_else(|| crate::Error::Refused("a part of this job is missing".into()))?;
        self.check_layout(count)?;
        for step in self.past.iter().chain(&self.future) {
            if let Some(count) = contours(&step.parts) {
                check_layout(
                    &step.placed,
                    &step.grouping,
                    step.nesting.as_ref(),
                    step.sheets.as_ref(),
                    count,
                )?;
            }
        }
        if self.past.len() > HISTORY || self.future.len() > HISTORY {
            return Err(crate::Error::Refused("stored draft history exceeds its limit".into()));
        }
        self.current.preflight.validate()?;
        Ok(())
    }

    /// Checks the current layout against a drawing of `count` contours.
    fn check_layout(&self, count: usize) -> Result<()> {
        check_layout(
            &self.current.placed,
            &self.current.grouping,
            self.current.nesting.as_ref(),
            self.current.sheets.as_ref(),
            count,
        )
    }

    /// History kept before a job could cut several parts names no parts:
    /// every step had the draft's one part.
    pub(crate) fn complete_history(&mut self) {
        for step in self.past.iter_mut().chain(&mut self.future) {
            if step.parts.is_empty() {
                Arc::make_mut(step).parts.clone_from(&self.current.parts);
            }
        }
    }

    // Parts ------------------------------------------------------------------

    /// The joined drawing of the parts, when it is attached.
    #[must_use]
    pub fn sources(&self) -> Option<&Arc<JobDrawing>> {
        self.sources.as_ref().filter(|sources| sources.is_of(&self.current.parts))
    }

    /// The job's drawing: its parts' drawings joined in order.
    pub fn drawing(&self) -> Result<&Arc<Drawing>> {
        self.sources().map(|sources| sources.drawing()).ok_or_else(|| {
            crate::Error::Refused("this job's drawing is not loaded; open the job again".into())
        })
    }

    /// Attaches the drawing of the draft's parts, once the layout is known
    /// to fit it.
    pub(crate) fn attach(&mut self, sources: Arc<JobDrawing>) -> Result<()> {
        if !sources.is_of(&self.current.parts) {
            return Err(crate::Error::Refused("the drawing is not this job's".into()));
        }
        self.check_layout(sources.contours())?;
        self.source_count = sources.contours();
        self.sources = Some(sources);
        self.engrave_new_images();
        Ok(())
    }

    /// Replaces one source part as one undoable edit, retaining each copy's
    /// placement. A nested sheet keeps its nested places and every sheet of
    /// a job of several follows; a part that no longer fits its place is
    /// refused when prepared, as crossing a neighbour or leaving the stock.
    pub(crate) fn replace_part(&mut self, sources: Arc<JobDrawing>, index: usize) -> Result<()> {
        let old = self
            .sources()
            .ok_or_else(|| crate::Error::Refused("source drawing missing".into()))?
            .clone();
        let swap = PartSwap {
            old: &old,
            new: &sources,
            before: old.range(index),
            after: sources.range(index),
        };
        let mut next = self.current.clone();
        swap.apply(&mut next.placed, &mut next.features, &mut next.grouping, &mut next.nesting)?;
        if let Some(set) = &mut next.sheets {
            set.try_for_each_layout(|placed, features, grouping, nesting| {
                swap.apply(placed, features, grouping, nesting)
            })?;
        }
        self.remember();
        self.current = next;
        self.current.parts = sources.parts().cloned().collect();
        self.source_count = sources.contours();
        self.sources = Some(sources);
        self.prepared = None;
        self.compiled = None;
        self.engrave_new_images();
        Ok(())
    }

    /// Appends the parts `sources` adds to the draft's, their contours at
    /// `placed`, as one edit. Everything already placed keeps its index.
    pub(crate) fn add_parts(
        &mut self,
        sources: Arc<JobDrawing>,
        placed: Vec<Placed>,
    ) -> Result<()> {
        let kept = self.current.parts.len();
        if sources.parts().len() <= kept || !sources.parts().take(kept).eq(&self.current.parts) {
            return Err(crate::Error::Refused("the parts must follow the job's own".into()));
        }
        self.remember();
        let first = self.current.placed.len();
        self.current.parts = sources.parts().cloned().collect();
        self.source_count = sources.contours();
        self.sources = Some(sources);
        self.current.placed.extend(placed);
        if let OrderStrategy::Manual(order) = &mut self.current.features.order.strategy {
            order.extend(first..self.current.placed.len());
        }
        self.engrave_new_images();
        Ok(())
    }

    /// Drops the parts nothing refers to any more: no placed contour, no
    /// stock outline and no other unsaved sheet. The contours of the parts
    /// after them move down; placed indices, and every feature on them, stay.
    pub(crate) fn prune_parts(&mut self) {
        let Some(sources) = self.sources().cloned() else { return };
        let mut used = vec![false; self.current.parts.len()];
        let mut mark = |source: usize| {
            if let Some(part) = sources.part_of(source) {
                used[part] = true;
            }
        };
        self.current.placed.iter().for_each(|p| mark(p.source));
        outline_source(self.current.nesting.as_ref()).into_iter().for_each(&mut mark);
        if let Some(set) = &self.current.sheets {
            set.sources().for_each(&mut mark);
        }
        if used.iter().all(|&used| used) {
            return;
        }
        let (kept, map) = sources.retain(&used);
        let moved = |source: &mut usize| {
            if let Some(next) = map.get(*source).copied().flatten() {
                *source = next;
            }
        };
        self.current.placed.iter_mut().for_each(|p| moved(&mut p.source));
        remap_outline(&mut self.current.nesting, moved);
        let kept_contour = |source: usize| map.get(source).copied().flatten();
        remap_layer_edits(&mut self.current.features, kept_contour);
        if let Some(set) = &mut self.current.sheets {
            set.remap_sources(moved);
            set.remap_layer_edits(kept_contour);
        }
        self.current.parts = kept.parts().cloned().collect();
        self.source_count = kept.contours();
        self.sources = Some(Arc::new(kept));
    }

    /// Moves, turns or mirrors placed contours by `matrix`, after what they
    /// have; none puts them where the drawing has them.
    pub fn transform(&mut self, contours: &[usize], matrix: Option<Transform>) -> Result<()> {
        let changed: Vec<_> = self
            .current
            .placed
            .iter()
            .enumerate()
            .filter_map(|(i, placed)| {
                let group = self.groups.iter().find(|group| group.contains(&i));
                let selected = contours.contains(&i)
                    || group.is_some_and(|g| g.iter().any(|i| contours.contains(i)));
                if !selected {
                    return None;
                }
                let owner = group.and_then(|g| g.first()).copied().unwrap_or(i);
                let delta =
                    matrix.unwrap_or_else(|| self.current.placed[owner].transform.inverse());
                Some((i, delta.after(&placed.transform)))
            })
            .collect();
        if changed.iter().any(|(_, transform)| !transform.is_similarity()) {
            return Err(crate::Error::Request(
                "the resulting scale is outside the supported range".into(),
            ));
        }
        self.remember();
        for (i, transform) in changed {
            self.current.placed[i].transform = transform;
        }
        Ok(())
    }

    /// Groups or ungroups the complete selected groups in one undo step.
    pub fn set_grouped(&mut self, contours: &[usize], together: bool) {
        let chosen: std::collections::BTreeSet<_> = contours.iter().copied().collect();
        let expanded: Vec<_> = self
            .groups
            .iter()
            .filter(|g| g.iter().any(|i| chosen.contains(i)))
            .flatten()
            .copied()
            .collect();
        self.remember();
        self.current.grouping.assign(&expanded, together);
    }

    /// Adds drawing contours to the sheet as one more copy, each under its
    /// transform; their indices.
    pub fn add(&mut self, contours: &[(usize, Transform)]) -> Vec<usize> {
        self.add_with_leads(contours, &[])
    }

    /// Paste contour-local lead edits with locations relative to `contours`.
    pub fn add_with_leads(
        &mut self,
        contours: &[(usize, Transform)],
        leads: &[LeadOverride],
    ) -> Vec<usize> {
        self.add_grouped(contours, leads, &Grouping::default())
    }

    /// Pastes lead edits and grouping with indices relative to the paste.
    pub fn add_grouped(
        &mut self,
        contours: &[(usize, Transform)],
        leads: &[LeadOverride],
        grouping: &Grouping,
    ) -> Vec<usize> {
        self.remember();
        // A selection can contain several copies of the same source contour.
        // Give each occurrence its own copy identity, preserving the pairing of
        // corresponding contours. Reuse free IDs to avoid counter overflow.
        let used: std::collections::BTreeSet<_> =
            self.current.placed.iter().map(|p| p.copy).collect();
        let copies: Vec<_> =
            (0u32..).filter(|copy| !used.contains(copy)).take(contours.len()).collect();
        let mut occurrences = std::collections::BTreeMap::<usize, usize>::new();
        let first = self.current.placed.len();
        self.current.placed.extend(contours.iter().map(|&(source, transform)| {
            let occurrence = occurrences.entry(source).or_default();
            let copy = copies[*occurrence];
            *occurrence += 1;
            Placed { source, copy, transform }
        }));
        if let Some(settings) = &mut self.current.features.leads {
            settings.overrides.extend(leads.iter().cloned().map(|mut edited| {
                edited.location.contour += first;
                edited
            }));
        }
        self.current.grouping.append(grouping, first);
        (first..self.current.placed.len()).collect()
    }

    /// Takes contours off the sheet; the features lose what lay on them
    /// and the rest are renumbered.
    pub fn remove(&mut self, drawing: &Drawing, contours: &[usize]) -> Result<()> {
        let sheet = place(drawing, &self.current.placed);
        let bridges = openlaser_prep::retained_bridges(&sheet, &self.current.features, |i| {
            !contours.contains(&i)
        })?;
        self.remember();
        let mut next = 0;
        let renumbered: Vec<Option<usize>> = (0..self.current.placed.len())
            .map(|i| {
                if contours.contains(&i) {
                    None
                } else {
                    next += 1;
                    Some(next - 1)
                }
            })
            .collect();
        self.current.placed = self
            .current
            .placed
            .iter()
            .zip(&renumbered)
            .filter_map(|(placed, kept)| kept.map(|_| *placed))
            .collect();
        self.current.features =
            self.current.features.renumbered_locations(|i| renumbered.get(i).copied().flatten());
        self.current.features.bridges = bridges;
        self.current.grouping =
            self.current.grouping.remapped(|i| renumbered.get(i).copied().flatten());
        Ok(())
    }

    /// The part's drawing with the job's bridges on its first copy cut into
    /// it, in the part's own coordinates, for editing as a sketch; none when
    /// no bridge lies on the part alone. Bridges joining it to another part
    /// stay out: they belong to the layout, not to the part.
    pub fn bridged_part(&self, part: &Id) -> Result<Option<Drawing>> {
        let Some(bridges) = &self.current.features.bridges else { return Ok(None) };
        if bridges.connections.is_empty() {
            return Ok(None);
        }
        let sources = self.sources().ok_or_else(|| {
            crate::Error::Refused("this job's drawing is not loaded; open the job again".into())
        })?;
        let index = sources
            .parts()
            .position(|p| p == part)
            .ok_or_else(|| crate::Error::Refused("that part is not in this job".into()))?;
        let range = sources.range(index);
        let placed = &self.current.placed;
        let Some(copy) = placed.iter().filter(|p| range.contains(&p.source)).map(|p| p.copy).min()
        else {
            return Ok(None);
        };
        let own: Vec<usize> = (0..placed.len())
            .filter(|&i| range.contains(&placed[i].source) && placed[i].copy == copy)
            .collect();
        let drawing = sources.drawing();
        let sheet = place(drawing, placed);
        let features = placed_bridges(&sheet, &self.current.features, placed)?;
        let results = openlaser_prep::bridged_contours(&sheet, &features)?;
        // A result is bridged when it joins contours or shares its sources
        // with another result, as the halves of a split do.
        let is_bridged = |sources: &Vec<usize>| {
            sources.len() > 1 || results.iter().filter(|(other, _)| other == sources).count() > 1
        };
        let mut covered = Vec::new();
        let mut contours = Vec::new();
        for (from, contour) in &results {
            if !is_bridged(from) || !from.iter().all(|s| own.contains(s)) {
                continue;
            }
            contours.push(placed[from[0]].transform.inverse().contour(contour));
            covered.extend(from.iter().copied());
        }
        if contours.is_empty() {
            return Ok(None);
        }
        for &i in &own {
            if !covered.contains(&i) {
                contours.push(drawing.contours[placed[i].source].clone());
            }
        }
        Ok(Some(Drawing { contours }))
    }

    /// Removes one bridge, `index` from 0, and keeps the others where they
    /// are on the sheet. Returns how many later bridges went with it because
    /// they were drawn on its channel.
    pub fn remove_bridge(&mut self, drawing: &Drawing, index: usize) -> Result<usize> {
        let sheet = place(drawing, &self.current.placed);
        let transforms: Vec<_> = self.current.placed.iter().map(|p| p.transform).collect();
        let (bridges, dropped) =
            openlaser_prep::without_bridge(&sheet, &self.current.features, &transforms, index)?;
        self.remember();
        self.current.features.bridges = Some(bridges);
        Ok(dropped)
    }

    /// The view.
    #[must_use]
    pub fn view(&self) -> DraftView {
        DraftView {
            key: crate::workspace::key(self),
            dry_run: self.dry_run,
            placement: crate::placement::view(self),
            sheets: None,
            calibration: self.calibration,
            stock_outline: self.stock_outline.clone(),
            stock_cutouts: self.stock_cutouts.clone(),
            correction: self.current.correction.clone(),
            nesting: self.current.nesting.clone(),
            preflight: self.current.preflight.clone(),
            generation: 0,
            revision: 0,
            name: String::new(),
            parts: self
                .current
                .parts
                .iter()
                .enumerate()
                .map(|(index, id)| {
                    let range = self.sources().map_or(0..0, |s| s.range(index));
                    crate::document::DraftPart {
                        id: id.clone(),
                        first: range.start,
                        contours: range.len(),
                    }
                })
                .collect(),
            job: self.job.clone(),
            recipe: self.current.recipe.as_ref().map(crate::document::RecipeView::new),
            film: self.current.film.as_ref().map(crate::document::RecipeView::new),
            features: self.current.features.clone(),
            feature_source: self.current.feature_source.clone(),
            placed: self.current.placed.clone(),
            groups: self.groups.clone(),
            origin: self.origin(),
            sheet_offset: self.current.sheet_offset,
            anchor: self.current.anchor,
            dock: self.anchor_point(),
            past: self.past.len(),
            future: self.future.len(),
            preview: self.preview.clone(),
            compiled: self.compiled.as_ref().map(|c| c.view.clone()),
            error: self.error.clone(),
            layers: crate::layers::view(self),
        }
    }
}

// Placement ------------------------------------------------------------------

/// Checks a layout against a drawing of `count` contours.
fn check_layout(
    placed: &[Placed],
    grouping: &Grouping,
    nesting: Option<&Nesting>,
    sheets: Option<&crate::sheets::SheetSet>,
    count: usize,
) -> Result<()> {
    if let Some(sheets) = sheets {
        sheets.validate(count)?;
    }
    if let Some(nesting) = nesting {
        nesting.validate(count).map_err(crate::Error::Refused)?;
    }
    grouping.validate(placed.len()).map_err(crate::Error::Refused)?;
    Placed::validate_all(placed, count).map_err(crate::Error::Refused)
}

/// The drawing contour a stock outline was taken from.
pub(crate) fn outline_source(nesting: Option<&Nesting>) -> Option<usize> {
    match nesting.map(|n| &n.stock) {
        Some(NestStock::Outline { contour }) => Some(contour.source),
        _ => None,
    }
}

/// Moves the drawing contour a stock outline was taken from.
pub(crate) fn remap_outline(nesting: &mut Option<Nesting>, moved: impl Fn(&mut usize)) {
    if let Some(Nesting { stock: NestStock::Outline { contour }, .. }) = nesting {
        moved(&mut contour.source);
    }
}

/// The sheet: each placed contour of the drawing under its transform.
#[must_use]
pub fn place(drawing: &Drawing, placed: &[Placed]) -> Drawing {
    Drawing {
        contours: placed
            .iter()
            .filter_map(|p| drawing.contours.get(p.source).map(|c| p.transform.contour(c)))
            .collect(),
    }
}

/// The transform of an identified placed contour.
fn transform_of(placed: &[Placed], contour: usize) -> Result<Transform> {
    placed
        .get(contour)
        .map(|p| p.transform)
        .ok_or_else(|| crate::Error::Refused("the placed owner no longer exists".into()))
}

/// The contours that move together: each outline with what it encloses,
/// copy by copy, joined across bridges so a bridge never has to stretch.
fn groups(drawing: &Drawing, placed: &[Placed], prepared: &[PreviewContour]) -> Vec<Vec<usize>> {
    let mut copies = std::collections::BTreeMap::<u32, Vec<usize>>::new();
    for (i, p) in placed.iter().enumerate() {
        copies.entry(p.copy).or_default().push(i);
    }
    let mut groups: Vec<Vec<usize>> = copies
        .values()
        .flat_map(|indices| {
            // Recompute topology over the contours that still exist in this copy.
            // A removed stock outline must not keep all enclosed parts grouped.
            let contours: Vec<_> =
                indices.iter().map(|&i| drawing.contours[placed[i].source].clone()).collect();
            openlaser_prep::groups(&contours)
                .into_iter()
                .map(|members| members.into_iter().map(|i| indices[i]).collect())
                .collect::<Vec<_>>()
        })
        .collect();
    join_groups(&mut groups, prepared);
    groups
}

/// Add bridge/common-path relationships to the source groups without
/// recomputing containment over the same source geometry a second time.
fn join_groups(groups: &mut Vec<Vec<usize>>, prepared: &[PreviewContour]) {
    for contour in prepared {
        for pair in contour.sources.windows(2) {
            let at = |source| groups.iter().position(|g| g.contains(&source));
            if let (Some(from), Some(to)) = (at(pair[0]), at(pair[1]))
                && from != to
            {
                let taken = groups.remove(to);
                let keep = if from > to { from - 1 } else { from };
                groups[keep].extend(taken);
                groups[keep].sort_unstable();
            }
        }
    }
}

// Preparation -----------------------------------------------------------------

/// A tap at `at` on the sheet, snapped to the nearest contour within
/// `tolerance`; with `bridging`, to the contours as the bridges leave
/// them. The point comes back in the drawing's own coordinates, as bridge
/// picks are kept.
pub fn pick(
    drawing: &Drawing,
    draft: &Draft,
    at: [f64; 2],
    tolerance: f64,
    bridging: bool,
) -> Result<Option<PickView>> {
    let drawing = crate::layers::relayered(drawing, &draft.current.features.layer_edits);
    let sheet = place(&drawing, &draft.current.placed);
    let features = placed_bridges(&sheet, &draft.current.features, &draft.current.placed)?;
    let picked = openlaser_prep::pick(&sheet, &features, Point::from(at), tolerance, bridging)?;
    let Some((spot, point)) = picked else { return Ok(None) };
    let owner = if bridging {
        openlaser_prep::bridge_owner(&sheet, &features, spot.contour)?
    } else {
        spot.contour
    };
    Ok(Some(PickView {
        spot,
        point: transform_of(&draft.current.placed, owner)?.inverse().apply(point).into(),
        owner,
    }))
}

impl Draft {
    /// The layers holding image frames, after the operator's layer moves.
    pub(crate) fn image_layers(&self) -> Vec<String> {
        let Some(sources) = self.sources() else { return Vec::new() };
        if sources.images().is_empty() {
            return Vec::new();
        }
        let drawing =
            crate::layers::relayered(sources.drawing(), &self.current.features.layer_edits);
        let mut names: Vec<String> = sources
            .images()
            .iter()
            .filter_map(|(frame, _)| drawing.contours.get(*frame).map(|c| c.layer.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// The features with every image layer that has no entry of its own
    /// engraving the image, as an image layer starts out.
    pub(crate) fn features_with_images(&self) -> Features {
        let mut features = self.current.features.clone();
        engrave_images(&mut features, &self.image_layers());
        features
    }

    /// The images on the placed contours that frame them, placed.
    fn placed_rasters(&self) -> Vec<openlaser_core::raster::PlacedRaster> {
        let Some(sources) = self.sources() else { return Vec::new() };
        self.current
            .placed
            .iter()
            .enumerate()
            .filter_map(|(contour, placed)| {
                let (_, raster) =
                    sources.images().iter().find(|(frame, _)| *frame == placed.source)?;
                Some(openlaser_core::raster::PlacedRaster {
                    contour,
                    raster: raster.transformed(&placed.transform),
                })
            })
            .collect()
    }

    /// Where each placed image lies, for the canvas.
    fn raster_views(&self) -> Vec<crate::document::RasterView> {
        let Some(sources) = self.sources() else { return Vec::new() };
        self.placed_rasters()
            .into_iter()
            .filter_map(|placed| {
                let source = self.current.placed.get(placed.contour)?.source;
                let part = sources.parts().nth(sources.part_of(source)?)?.clone();
                let r = &placed.raster;
                Some(crate::document::RasterView {
                    part,
                    contour: placed.contour,
                    origin: r.origin.into(),
                    across: r.across.into(),
                    down: r.down.into(),
                })
            })
            .collect()
    }

    /// The features with each filled layer's measured scan offsets, the
    /// image layers' defaults and the placed images.
    fn calibrated_features(&self) -> Features {
        let mut features = self.features_with_images();
        features.rasters = self.placed_rasters();
        for layer in &mut features.layers {
            if let Some(fill) = layer.machining.as_mut().and_then(|m| m.fill.as_mut()) {
                if let Some(table) = self.scan_tables.get(&layer.name) {
                    fill.calibrated.clone_from(table);
                }
                if let Some(distance) = self.scan_runups.get(&layer.name) {
                    fill.run_up = fill.run_up.max(*distance);
                }
            }
        }
        features
    }
}

/// One part of a job's drawing swapped for another: `before` its contours
/// in the old drawing, `after` in the new one.
struct PartSwap<'a> {
    old: &'a JobDrawing,
    new: &'a JobDrawing,
    before: std::ops::Range<usize>,
    after: std::ops::Range<usize>,
}

impl PartSwap<'_> {
    /// Where an old drawing contour lies in the new drawing: the same place
    /// before the part, moved by the change in its size after it, and in the
    /// part itself only where the new part still has that very shape.
    fn contour(&self, source: usize) -> Option<usize> {
        if source < self.before.start {
            return Some(source);
        }
        if source >= self.before.end {
            return Some(source - self.before.len() + self.after.len());
        }
        let shape = self.old.drawing().contours.get(source)?;
        let same = self.after.start + (source - self.before.start);
        if self.after.contains(&same) && self.new.drawing().contours[same] == *shape {
            return Some(same);
        }
        self.after.clone().find(|&s| self.new.drawing().contours[s] == *shape)
    }

    /// Swaps the part on one sheet's layout: each copy gets the new part's
    /// contours, an unchanged shape keeping its own transform and every
    /// feature on it; locations on changed shapes go, bridges go, and a
    /// manual order takes the new contours last.
    fn apply(
        &self,
        placed: &mut Vec<Placed>,
        features: &mut Features,
        grouping: &mut Grouping,
        nesting: &mut Option<Nesting>,
    ) -> Result<()> {
        // Bridges on other parts stay where they are on the sheet; those on
        // this part go, as an edited part carries its own bridges in its shape.
        let kept_ends: Vec<[openlaser_core::geometry::Point; 2]> = match &features.bridges {
            Some(bridges) if !bridges.connections.is_empty() => {
                let transforms: Vec<_> = placed.iter().map(|p| p.transform).collect();
                openlaser_prep::bridge_ends(
                    &place(self.old.drawing(), placed),
                    features,
                    &transforms,
                )
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, joined)| {
                    !joined.iter().any(|&i| self.before.contains(&placed[i].source))
                })
                .map(|(ends, _)| ends)
                .collect()
            }
            _ => Vec::new(),
        };
        let mut next = Vec::new();
        let mut map = vec![None; placed.len()];
        let mut copies = std::collections::BTreeSet::new();
        for (i, p) in placed.iter().enumerate() {
            if self.before.contains(&p.source) {
                if copies.insert(p.copy) {
                    for source in self.after.clone() {
                        let equivalent = placed.iter().enumerate().find(|(_, q)| {
                            q.copy == p.copy
                                && self.before.contains(&q.source)
                                && self.old.drawing().contours[q.source]
                                    == self.new.drawing().contours[source]
                        });
                        let transform = equivalent.map_or(p.transform, |(_, q)| q.transform);
                        if let Some((old_index, _)) = equivalent {
                            map[old_index] = Some(next.len());
                        }
                        next.push(Placed { source, copy: p.copy, transform });
                    }
                }
            } else {
                map[i] = Some(next.len());
                let source = self.contour(p.source).ok_or_else(|| {
                    crate::Error::Refused("a placed contour has no place in the new drawing".into())
                })?;
                next.push(Placed { source, ..*p });
            }
        }
        if next.len() > openlaser_core::geometry::MAX_PLACED_CONTOURS {
            return Err(crate::Error::Request("too many placed contours".into()));
        }
        if let Some(source) = outline_source(nesting.as_ref()) {
            let moved = self.contour(source).ok_or_else(|| {
                crate::Error::Refused(
                    "the sheet's outline is a contour of this part that changed; choose the stock again after editing".into(),
                )
            })?;
            remap_outline(nesting, |s| *s = moved);
        }
        *features = features.renumbered_locations(|i| map.get(i).copied().flatten());
        remap_layer_edits(features, |s| self.contour(s));
        if let OrderStrategy::Manual(order) = &mut features.order.strategy {
            for i in 0..next.len() {
                if !order.contains(&i) {
                    order.push(i);
                }
            }
        }
        *grouping = grouping.remapped(|i| map.get(i).copied().flatten());
        *placed = next;
        // Evolving bridge references cannot survive changed topology as
        // numbers; the kept bridges are found again by their ends.
        if let Some(width) = features.bridges.as_ref().map(|b| b.width.0) {
            let mut again = Vec::new();
            if !kept_ends.is_empty() {
                let transforms: Vec<_> = placed.iter().map(|p| p.transform).collect();
                let sheet = place(self.new.drawing(), placed);
                let mut bare = features.clone();
                bare.bridges = None;
                if let Ok((found, _)) =
                    openlaser_prep::bridges_from_ends(&sheet, &bare, &transforms, width, &kept_ends)
                {
                    again = found;
                }
            }
            if let Some(bridges) = &mut features.bridges {
                bridges.connections = again;
            }
        }
        Ok(())
    }
}

/// Layer moves follow their drawing contours when the contours move; a
/// move on a contour that is gone goes with it, so it never lands on
/// another shape.
pub(crate) fn remap_layer_edits(features: &mut Features, moved: impl Fn(usize) -> Option<usize>) {
    features.layer_edits = features
        .layer_edits
        .iter()
        .filter_map(|edit| {
            Some(openlaser_core::features::LayerEdit {
                contour: moved(edit.contour)?,
                layer: edit.layer.clone(),
            })
        })
        .collect();
}

/// Gives each of `layers` without an entry of its own the engraving an
/// image layer starts with, so a picture's frame is never cut.
pub(crate) fn engrave_images(features: &mut Features, layers: &[String]) {
    use openlaser_core::features::{Fill, Layer, LayerMode, Machining};
    for name in layers {
        if features.layer(name).is_none() {
            features.layers.push(Layer {
                name: name.clone(),
                mode: LayerMode::Mark,
                color: None,
                machining: Some(Machining {
                    fill: Some(Fill::for_image()),
                    ..Machining::default()
                }),
            });
        }
    }
}

/// Places and prepares a drawing. Bridge picks are kept in the drawing's
/// own coordinates, so they are placed with their contours, and shapes
/// take the layers the operator moved them to.
pub fn prepare(drawing: &Drawing, placed: &[Placed], features: &Features) -> Result<Prepared> {
    let drawing = crate::layers::relayered(drawing, &features.layer_edits);
    let sheet = place(&drawing, placed);
    let features = placed_bridges(&sheet, features, placed)?;
    let toolpath = openlaser_prep::prepare(&sheet, &features)?;
    let bounds = if let Some(ids) = &features.only_contours {
        Drawing {
            contours: sheet
                .contours
                .iter()
                .enumerate()
                .filter(|(i, c)| ids.contains(i) && !features.skip_layers.contains(&c.layer))
                .map(|(_, c)| c.clone())
                .collect(),
        }
        .bounds()
    } else {
        sheet.bounds()
    };
    let mut display = preview(&toolpath, bounds);
    display.source_contours = Some(plain(&sheet).contours);
    display.engraving_regions = area_boundaries(&sheet, &features);
    Ok(Prepared {
        contours: contours(&toolpath),
        film: toolpath.contours.iter().map(|c| film_runs(&c.film)).collect(),
        shift: [0.; 2],
        preview: Arc::new(display),
        instances: Arc::new(
            toolpath
                .contours
                .iter()
                .map(|contour| {
                    contour
                        .sources
                        .iter()
                        .map(|&index| {
                            let instance = placed.get(index).ok_or_else(|| {
                                crate::Error::Refused(
                                    "a prepared path has no source placement".into(),
                                )
                            })?;
                            Ok(crate::document::InstanceView {
                                source: instance.source,
                                copy: instance.copy,
                            })
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<Vec<_>>>()?,
        ),
    })
}

fn area_boundaries(sheet: &Drawing, features: &Features) -> Vec<PreviewContour> {
    let mut originals = plain(sheet).contours;
    if features.bridges.is_some()
        && let Ok(joined) = openlaser_prep::bridge_boundaries(sheet, features)
    {
        for (sources, contour) in joined.into_iter().filter(|(sources, _)| sources.len() > 1) {
            originals.retain(|c| !c.sources.iter().any(|s| sources.contains(s)));
            let mut boundary = plain(&Drawing { contours: vec![contour] }).contours.remove(0);
            boundary.sources = sources;
            originals.push(boundary);
        }
    }
    originals
        .iter()
        .filter(|c| {
            c.closed
                && !features.skip_layers.contains(&c.layer)
                && features.only_contours.as_ref().is_none_or(|ids| ids.contains(&c.sources[0]))
                && features.layers.iter().any(|l| {
                    l.name == c.layer
                        && l.mode == openlaser_core::features::LayerMode::Mark
                        && l.machining.as_ref().is_some_and(|m| m.fill.is_some())
                })
                && (!features.fill_regions.iter().any(|r| r.holes.contains(&c.sources[0]))
                    || features.fill_regions.iter().any(|r| r.contour == c.sources[0]))
        })
        .map(|c| {
            let mut region = c.clone();
            if let Some(chosen) = features.fill_regions.iter().find(|r| r.contour == c.sources[0]) {
                for &hole in &chosen.holes {
                    if let Some(boundary) = originals.iter().find(|c| c.sources == [hole]) {
                        region.paths.extend(boundary.paths.clone());
                    }
                }
            }
            region
        })
        .collect()
}

/// Keep editable outlines on a failed job, and show each valid filled marking
/// layer independently. This is display geometry only: no Prepared is retained,
/// so the original error continues to block compilation and execution.
fn partial_preview(drawing: &Drawing, placed: &[Placed], features: &Features) -> Preview {
    let relayered = crate::layers::relayered(drawing, &features.layer_edits);
    let sheet = place(&relayered, placed);
    let mut display = plain(&sheet);
    display.source_contours = Some(display.contours.clone());
    let invalid: Vec<_> = sheet
        .contours
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            !features.skip_layers.contains(&c.layer) && openlaser_prep::crosses_itself(c)
        })
        .map(|(i, _)| i)
        .collect();
    for &i in &invalid {
        display.contours[i].problem = Some("crosses itself".into());
    }
    for i in openlaser_prep::tangled(&sheet.contours) {
        if !features.skip_layers.contains(&sheet.contours[i].layer)
            && display.contours[i].problem.is_none()
        {
            display.contours[i].problem = Some("touches another contour".into());
        }
    }
    if !invalid.is_empty() {
        let mut valid_sheet = sheet.clone();
        let mut isolated = features.clone();
        let excluded = "__openlaser_invalid_preview__";
        isolated.skip_layers.push(excluded.into());
        for &i in &invalid {
            valid_sheet.contours[i].layer = excluded.into();
        }
        let result = placed_bridges(&valid_sheet, &isolated, placed)
            .and_then(|f| openlaser_prep::prepare(&valid_sheet, &f).map_err(crate::Error::from));
        if let Err(error) = &result {
            display.warnings.push(format!("Partial preview: {error}"));
        }
        if let Ok(toolpath) = result {
            let mut valid = preview(&toolpath, sheet.bounds());
            valid.engraving_regions = area_boundaries(&sheet, features)
                .into_iter()
                .filter(|r| !invalid.contains(&r.sources[0]))
                .collect();
            valid.contours.extend(invalid.iter().map(|&i| display.contours[i].clone()));
            return valid;
        }
    }
    for layer in features.layers.iter().filter(|l| {
        l.mode == openlaser_core::features::LayerMode::Mark
            && l.machining.as_ref().is_some_and(|m| m.fill.is_some())
            && !features.skip_layers.contains(&l.name)
    }) {
        let mut isolated = features.clone();
        for contour in &sheet.contours {
            if contour.layer != layer.name && !isolated.skip_layers.contains(&contour.layer) {
                isolated.skip_layers.push(contour.layer.clone());
            }
        }
        let Ok(prepared) = prepare(drawing, placed, &isolated) else {
            continue;
        };
        let original_length: f64 =
            sheet.contours.iter().filter(|c| c.layer == layer.name).map(Contour::length).sum();
        display.length_mm += prepared.preview.length_mm - original_length;
        display.contours.retain(|c| c.layer != layer.name);
        display
            .contours
            .extend(prepared.preview.contours.iter().filter(|c| c.layer == layer.name).cloned());
        display.engraving_regions.extend(
            prepared.preview.engraving_regions.iter().filter(|c| c.layer == layer.name).cloned(),
        );
        display.warnings.extend(prepared.preview.warnings.iter().cloned());
    }
    display
}

/// The runs of a film pass over bare geometry: every break in the curves
/// starts a run of its own, with its own travel and start.
fn film_runs(curves: &[Curve]) -> Vec<cut::Contour> {
    let mut runs: Vec<Vec<cut::Segment>> = Vec::new();
    for curve in curves {
        let start: [f64; 2] = curve.start().into();
        let connected = runs.last().and_then(|run| run.last()).is_some_and(|previous| {
            let end = previous.point(1.);
            (end[0] - start[0]).hypot(end[1] - start[1]) < 1e-8
        });
        if !connected {
            runs.push(Vec::new());
        }
        if let Some(run) = runs.last_mut() {
            run.push(cut::Segment::from(*curve));
        }
    }
    runs.into_iter()
        .map(|segments| cut::Contour {
            segments: segments
                .into_iter()
                .map(|s| PreparedSegment::new(s, SegmentProcess::default()))
                .collect(),
            overrides: openlaser_compiler::settings::Overrides::default(),
        })
        .collect()
}

/// The features with their bridge picks carried onto the sheet by the
/// contours they lie on.
fn placed_bridges(sheet: &Drawing, features: &Features, placed: &[Placed]) -> Result<Features> {
    if sheet.contours.len() != placed.len() {
        return Err(crate::Error::Refused("a placed contour has no source drawing".into()));
    }
    let transforms: Vec<_> = placed.iter().map(|p| p.transform).collect();
    Ok(openlaser_prep::place_bridges(sheet, features, &transforms)?)
}

/// The compiler's contours from a toolpath.
fn contours(toolpath: &Toolpath) -> Vec<cut::Contour> {
    toolpath
        .contours
        .iter()
        .map(|c| cut::Contour {
            segments: c
                .segments
                .iter()
                .map(|s| s.map_curve(|curve| cut::Segment::from(*curve)))
                .collect(),
            overrides: openlaser_compiler::settings::Overrides::default(),
        })
        .collect()
}

fn preview(toolpath: &Toolpath, bounds: Option<Bounds>) -> Preview {
    let contours = toolpath
        .contours
        .iter()
        .map(|c| {
            let mut paths: Vec<Path> = Vec::new();
            let mut cooling = Vec::new();
            for segment in &c.segments {
                let (curve, process) = (&segment.curve, &segment.process);
                if process.cool_ms > 0 {
                    cooling.push(curve.start().into());
                }
                let kind = path_kind(process);
                let points = polyline(std::slice::from_ref(curve), 32);
                match paths.last_mut() {
                    Some(last) if last.kind == kind => {
                        last.points.extend(points.into_iter().skip(1));
                    }
                    _ => paths.push(Path { kind, points }),
                }
            }
            let start = c
                .segments
                .iter()
                .find(|s| s.process.lead.is_none())
                .or_else(|| c.segments.first())
                .map_or([0., 0.], |s| s.curve.start().into());
            PreviewContour {
                lead_target: c.lead_target,
                matching_contour: None,
                closed: c.closed,
                depth: c.depth,
                layer: c.layer.clone(),
                sources: c.sources.clone(),
                paths,
                cooling,
                start,
                problem: None,
            }
        })
        .collect();
    Preview {
        source_contours: None,
        engraving_regions: Vec::new(),
        contours,
        bounds,
        // What the beam cuts: stretches travelled with the laser off, such
        // as a flying cut's run-ups and gaps, are not cut length.
        length_mm: toolpath
            .contours
            .iter()
            .flat_map(|c| &c.segments)
            .filter(|s| !(s.process.joint && s.process.power == Some(0.)))
            .map(|s| s.curve.length())
            .sum(),
        warnings: toolpath.warnings.clone(),
        rasters: Vec::new(),
    }
}

/// The sheet as it lies, contour by contour, for the canvas when the
/// toolpath cannot be prepared.
fn plain(sheet: &Drawing) -> Preview {
    let contours = sheet
        .contours
        .iter()
        .enumerate()
        .map(|(i, c)| PreviewContour {
            lead_target: None,
            matching_contour: None,
            closed: c.is_closed(),
            depth: 0,
            layer: c.layer.clone(),
            sources: vec![i],
            paths: vec![Path { kind: PathKind::Cut, points: polyline(&c.curves, 32) }],
            cooling: Vec::new(),
            start: c.start().map_or([0., 0.], Into::into),
            problem: None,
        })
        .collect();
    Preview {
        source_contours: None,
        engraving_regions: Vec::new(),
        contours,
        bounds: sheet.bounds(),
        length_mm: sheet.contours.iter().map(Contour::length).sum(),
        warnings: Vec::new(),
        rasters: Vec::new(),
    }
}

/// The canvas layer of a prepared segment.
const fn path_kind(process: &SegmentProcess) -> PathKind {
    if process.run_up {
        // A run-up or turn with the laser off reads as a move, not a joint.
        PathKind::Travel
    } else if process.joint {
        PathKind::Joint
    } else {
        match process.lead {
            Some(LeadRole::Entry) => PathKind::LeadIn,
            Some(LeadRole::Exit) => PathKind::LeadOut,
            None => PathKind::Cut,
        }
    }
}

/// A polyline through the curves, with up to `arc_steps` steps per full arc.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "step counts are small"
)]
fn polyline(curves: &[Curve], arc_steps: usize) -> Vec<[f64; 2]> {
    let mut points: Vec<[f64; 2]> = Vec::new();
    for curve in curves {
        let steps = match curve {
            Curve::Line { .. } => 1,
            Curve::Arc { sweep, .. } => {
                let share = sweep.abs() / std::f64::consts::TAU;
                ((share * arc_steps as f64).ceil() as usize).clamp(2, arc_steps.max(2))
            }
        };
        if points.is_empty() {
            points.push(curve.start().into());
        }
        for step in 1..=steps {
            points.push(curve.point(step as f64 / steps as f64).into());
        }
    }
    points
}

/// Every contour of a drawing as a polyline, for thumbnails.
#[must_use]
pub fn outline(drawing: &Drawing, arc_steps: usize) -> Vec<Vec<[f64; 2]>> {
    drawing.contours.iter().map(|c| polyline(&c.curves, arc_steps)).collect()
}

// Compilation -----------------------------------------------------------------

/// How a recipe binds.
#[derive(Clone, Copy, Debug)]
pub struct Binder {
    /// Imported host process overrides, or the vendor defaults.
    pub timeouts: Timeouts,
    /// A dry run: motion without process.
    pub dry_run: bool,
    /// Bypass the height controller for the CO2 setup; unrelated to optical focus.
    pub manual_focus: bool,
    /// Bind the recipe as the film process.
    pub film: bool,
    /// The controller's scale.
    pub scale: i32,
}

/// The recipe bound into settings: the machine files with the recipe's
/// bank as a borrowed overlay, read by the vendor's rules. The recipe's
/// attributes lie over the machine's own bank, as M-Laser's import keeps
/// a bank's other values, so a recipe file missing a newer attribute still
/// binds; each recipe has its own overlay, so a cutting
/// recipe kept in bank 11 never reaches the film process.
pub fn settings(bundle: &Bundle, recipe: &Recipe, binder: Binder) -> Result<Settings> {
    let request = Request {
        mode: recipe.laser,
        layer: recipe.layer,
        dry_run: binder.dry_run,
        manual_focus: binder.manual_focus,
        film: binder.film,
        timeouts: binder.timeouts,
    };
    let mut settings = openlaser_xml::recipe::bind_overlay(bundle, request, &recipe.attributes)?;
    settings.bind_speed_cap(binder.scale)?;
    (settings.acceleration, settings.acceleration_time) =
        engraving_motion(recipe, settings.acceleration, settings.acceleration_time);
    if is_engraving(recipe) {
        // An engraving measures the height once, at its first contour, and
        // flies every field and line at that fixed height, never lifting.
        settings.fixed_height = true;
        settings.hold_height = true;
        settings.no_follow = false;
        settings.absolute_cut_height = None;
        // Between the fields and objects of an engraving the head travels
        // often and short: gently, as the lines themselves.
        settings.travel_acceleration =
            settings.travel_acceleration.min(ENGRAVING_TRAVEL_ACCELERATION);
        settings.travel_acceleration_time =
            settings.travel_acceleration_time.max(ENGRAVING_ACCELERATION_TIME);
    }
    Ok(settings)
}

/// How hard an engraving's travel moves accelerate at most, in mm/s².
pub const ENGRAVING_TRAVEL_ACCELERATION: f64 = 3000.;

fn is_engraving(recipe: &Recipe) -> bool {
    recipe.attributes.get("OpenLaserProcess").is_some_and(|p| p == "engrave")
}

/// The gentlest a gantry engraves with by default: every scan line stops and
/// turns, and the machine's cutting acceleration shakes it at each turn.
pub const ENGRAVING_ACCELERATION: f64 = 1500.;
/// The shortest ramp an engraving turn takes by default, in seconds.
pub const ENGRAVING_ACCELERATION_TIME: f64 = 0.3;

/// The acceleration and ramp time an engraving recipe moves with: its own
/// `OpenLaserFillAcceleration` (mm/s²) and `OpenLaserFillAccelerationTime`
/// (ms), else the gentle defaults, never harder than the machine's own.
/// A cutting recipe keeps the machine's.
#[must_use]
pub fn engraving_motion(recipe: &Recipe, acceleration: f64, time: f64) -> (f64, f64) {
    if !is_engraving(recipe) {
        return (acceleration, time);
    }
    let read = |key: &str| {
        recipe
            .attributes
            .get(key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.)
    };
    let own = read("OpenLaserFillAcceleration").unwrap_or(ENGRAVING_ACCELERATION);
    let own_time =
        read("OpenLaserFillAccelerationTime").map_or(ENGRAVING_ACCELERATION_TIME, |ms| ms * 0.001);
    (own.clamp(1., acceleration), own_time.max(time))
}

/// Compiles the prepared contours under `settings`, shifted as the recipe
/// asks, with the film passes under `film` when the recipe removes film.
/// The summary binds a trial program at the drawing origin; the run binds
/// again at the head.
pub fn compile(
    prepared: &Prepared,
    settings: &Settings,
    film: Option<&Settings>,
    layered: &crate::layers::Layered,
    dry_run: bool,
    scale: i32,
) -> Result<CompiledJob> {
    let prepared = prepared.shifted(settings.contour_shift)?;
    let film = film.map(|settings| Film { settings, runs: &prepared.film });
    let job = Arc::new(match layered.processes(settings) {
        Some(processes) => Job::schedule_layered(settings, &prepared.contours, film, &processes)?,
        None => Job::schedule_prepared(settings, &prepared.contours, film)?,
    });
    let z_units_per_mm = u32::try_from(scale).ok().filter(|scale| *scale > 0);
    let program = job.program(Binding { current: [0.; 2], z_units_per_mm })?;
    let blocks = records::encoded_blocks(&program.records)
        .try_fold(0, |count, block| block.map(|_| count + 1))
        .map_err(openlaser_compiler::Error::from)?;
    let plan = job
        .passes
        .iter()
        .map(|pass| {
            Ok(crate::document::PassView {
                ordinal: pass.pass.ordinal,
                kind: match pass.pass.kind {
                    PassKind::PrePierce => crate::document::PassKind::PrePierce,
                    PassKind::Film => crate::document::PassKind::Film,
                    PassKind::Cut => crate::document::PassKind::Cut,
                },
                instances: prepared.instances.get(pass.pass.source).cloned().ok_or_else(|| {
                    crate::Error::Refused("a pass has no physical source identity".into())
                })?,
                omitted_cooling: pass.omitted_cooling,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut view = summary(&job, &program, blocks, dry_run, plan);
    view.engraving_regions.clone_from(&prepared.preview.engraving_regions);
    let filled = |layer: &str| view.engraving_regions.iter().any(|c| c.layer == layer);
    view.engraving_paths = prepared
        .preview
        .contours
        .iter()
        .filter(|c| !c.closed && filled(&c.layer))
        .flat_map(|c| c.paths.iter().filter(|p| p.kind == PathKind::Cut).cloned())
        .collect();
    view.engraving_passes = job
        .passes
        .iter()
        .enumerate()
        .filter(|(_, pass)| {
            prepared.preview.contours.get(pass.pass.source).is_some_and(|c| filled(&c.layer))
        })
        .map(|(i, _)| i)
        .collect();
    Ok(CompiledJob {
        dry_run,
        scale,
        configuration: None,
        shift: prepared.shift,
        job,
        view: Arc::new(view),
    })
}

/// A preview of the emitted program, with the plan's immutable identities.
pub(crate) fn summary(
    job: &Job,
    program: &Program,
    blocks: usize,
    dry_run: bool,
    plan: Vec<crate::document::PassView>,
) -> Compiled {
    let pierces =
        job.passes.iter().filter(|pass| pass.pierces_on_entry()).map(|pass| pass.start).collect();
    let pass_usage = crate::gas::usage::of(job, program);
    Compiled {
        engraving_paths: Vec::new(),
        engraving_passes: Vec::new(),
        engraving_regions: Vec::new(),
        dry_run,
        seconds: program.seconds,
        plan,
        pierces,
        blocks,
        moves: moves(program),
        usage: pass_usage.total.clone(),
        pass_usage: Arc::new(pass_usage),
    }
}

/// The program of a job bound to the head at `current` in drawing
/// coordinates, and its upload blocks.
pub fn program(job: &Job, current: [f64; 2], scale: i32) -> Result<(Program, Vec<Vec<u32>>)> {
    let z_units_per_mm = u32::try_from(scale).ok().filter(|scale| *scale > 0);
    let program = job.program(Binding { current, z_units_per_mm })?;
    let blocks = blocks(&program)?;
    Ok((program, blocks))
}

/// A program's records as upload blocks.
pub fn blocks(program: &Program) -> Result<Vec<Vec<u32>>> {
    Ok(records::encode_blocks(&program.records).map_err(openlaser_compiler::Error::from)?)
}

/// The motion sections of a program as polylines.
#[must_use]
pub fn moves(program: &Program) -> Vec<Move> {
    program
        .sections
        .iter()
        .filter(|section| section.points.len() > 1)
        .filter_map(|section| {
            let kind = match section.kind {
                Kind::Travel | Kind::FrogJump | Kind::Frame => PathKind::Travel,
                Kind::Cut => PathKind::Cut,
                Kind::Film => PathKind::Film,
                Kind::LeadIn => PathKind::LeadIn,
                Kind::LeadOut => PathKind::LeadOut,
                Kind::Joint => PathKind::Joint,
                Kind::Cooling => PathKind::Cooling,
                Kind::ResidueSpiral | Kind::ResidueReturn => PathKind::Cleaning,
                _ => return None,
            };
            Some(Move {
                kind,
                pass: section.pass,
                points: crate::display_path::simplify(&section.points),
            })
        })
        .collect()
}

#[cfg(test)]
impl Draft {
    /// A draft of one part with `drawing`, as drawn.
    pub(crate) fn of(drawing: Drawing) -> Self {
        Self::new(Arc::new(JobDrawing::new(vec![(Id::from("part"), Arc::new(drawing))])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openlaser_core::features::{Bridge, Bridges, Pick, Spot};

    fn recipe_with(attributes: &[(&str, &str)]) -> Recipe {
        Recipe {
            id: Id::from("r"),
            name: "Steel".into(),
            laser: openlaser_core::LaserMode::Fiber,
            thickness_mm: 0.,
            gas: "N2".into(),
            layer: 7,
            attributes: attributes.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect(),
            film: None,
            source_sha256: None,
            note: String::new(),
            tags: Vec::new(),
            file_name: None,
            photo: None,
            favourite: false,
            created: 0,
            updated: 0,
        }
    }

    #[test]
    fn engraving_moves_gently_and_cutting_keeps_the_machine() {
        let cut = recipe_with(&[]);
        assert_eq!(engraving_motion(&cut, 6000., 0.25), (6000., 0.25));
        let engrave = recipe_with(&[("OpenLaserProcess", "engrave")]);
        assert_eq!(engraving_motion(&engrave, 6000., 0.25), (1500., 0.3));
        let own = recipe_with(&[
            ("OpenLaserProcess", "engrave"),
            ("OpenLaserFillAcceleration", "800"),
            ("OpenLaserFillAccelerationTime", "500"),
        ]);
        assert_eq!(engraving_motion(&own, 6000., 0.25), (800., 0.5));
        let harder =
            recipe_with(&[("OpenLaserProcess", "engrave"), ("OpenLaserFillAcceleration", "9000")]);
        assert_eq!(engraving_motion(&harder, 6000., 0.25), (6000., 0.3), "never above the machine");
    }
    use openlaser_core::geometry::Contour;
    use openlaser_core::units::Millimeters;

    /// A 10 mm square, one line per side, from `corner`.
    fn square_at(corner: [f64; 2]) -> Contour {
        let corners = [[0., 0.], [10., 0.], [10., 10.], [0., 10.]];
        let curves = (0..4)
            .map(|i| Curve::Line {
                start: Point::from([corners[i][0] + corner[0], corners[i][1] + corner[1]]),
                end: Point::from([
                    corners[(i + 1) % 4][0] + corner[0],
                    corners[(i + 1) % 4][1] + corner[1],
                ]),
            })
            .collect();
        Contour { layer: "0".into(), curves }
    }

    /// A 10 mm square at the drawing's own origin.
    fn square() -> Drawing {
        Drawing { contours: vec![square_at([0., 0.])] }
    }

    #[test]
    fn fill_only_keeps_editable_regions_without_adding_contour_cuts() {
        use openlaser_core::features::{Fill, Layer, LayerMode, Machining};
        let fill: Fill = serde_json::from_value(serde_json::json!({
            "spacing": 1., "angle": 0., "run_up": 2., "outline": false,
            "passes": 2, "pass_angle": 90.
        }))
        .unwrap();
        let mut drawing = square();
        let mut hole = square_at([2., 2.]);
        for curve in &mut hole.curves {
            if let Curve::Line { start, end } = curve {
                *start = Point::new(2. + (start.x - 2.) * 0.6, 2. + (start.y - 2.) * 0.6);
                *end = Point::new(2. + (end.x - 2.) * 0.6, 2. + (end.y - 2.) * 0.6);
            }
        }
        drawing.contours.push(hole);
        let mut features = Features::default();
        features.layers.push(Layer {
            name: "0".into(),
            mode: LayerMode::Mark,
            color: None,
            machining: Some(Machining { fill: Some(fill), ..Machining::default() }),
        });
        // Settings survive saving/reopening, including multipass and outline=false.
        let saved = serde_json::to_vec(&features).unwrap();
        features = serde_json::from_slice(&saved).unwrap();
        let prepared = prepare(&drawing, &Placed::all(2), &features).unwrap();
        assert_eq!(prepared.preview.engraving_regions.len(), 2);
        assert_eq!(prepared.preview.engraving_regions[1].sources, vec![1]);
        assert!(prepared.preview.contours.iter().all(|c| !c.closed));
        let marks: Vec<_> = prepared
            .preview
            .contours
            .iter()
            .flat_map(|c| &c.paths)
            .filter(|p| p.kind == PathKind::Cut)
            .collect();
        assert!(marks.iter().any(|p| (p.points[0][0] - p.points.last().unwrap()[0]).abs() > 1.));
        assert!(marks.iter().any(|p| (p.points[0][1] - p.points.last().unwrap()[1]).abs() > 1.));
        for path in marks {
            for segment in path.points.windows(2) {
                let x = segment[0][0].midpoint(segment[1][0]);
                let y = segment[0][1].midpoint(segment[1][1]);
                assert!(
                    !(x > 2.0001 && x < 7.9999 && y > 2.0001 && y < 7.9999),
                    "hole must stay clear"
                );
            }
        }
        // Changing back to contour engraving removes the fill, not the original shapes.
        features.layers[0].machining.as_mut().unwrap().fill = None;
        let contours = prepare(&drawing, &Placed::all(2), &features).unwrap();
        assert!(contours.preview.engraving_regions.is_empty());
        assert_eq!(contours.preview.contours.len(), 2);
        assert!(contours.preview.contours.iter().all(|c| c.closed));
    }

    #[test]
    fn every_crossing_contour_is_marked_though_the_error_names_one() {
        let bow_tie = |dx: f64| Contour {
            layer: "0".into(),
            curves: [[0., 0.], [10., 10.], [0., 10.], [10., 0.]]
                .iter()
                .zip([[10., 10.], [0., 10.], [10., 0.], [0., 0.]])
                .map(|(a, b)| Curve::Line {
                    start: Point::new(a[0] + dx, a[1]),
                    end: Point::new(b[0] + dx, b[1]),
                })
                .collect(),
        };
        let mut drawing = square();
        drawing.contours.extend([bow_tie(20.), bow_tie(40.)]);
        let mut draft = Draft::of(drawing.clone());
        draft.prepare(&drawing);
        assert!(draft.error.as_ref().unwrap().contains("crosses itself"));
        let marked: Vec<_> = draft
            .preview
            .as_ref()
            .unwrap()
            .contours
            .iter()
            .filter(|c| c.problem.is_some())
            .flat_map(|c| c.sources.clone())
            .collect();
        assert_eq!(marked, vec![1, 2]);
    }

    #[test]
    fn contours_that_touch_are_marked_too() {
        let mut drawing = square();
        drawing.contours.extend([square_at([10., 10.]), square_at([40., 0.])]);
        let mut draft = Draft::of(drawing.clone());
        draft.prepare(&drawing);
        assert!(draft.error.as_ref().unwrap().contains("cross or touch"));
        let preview = draft.preview.as_ref().unwrap();
        let problems: Vec<_> = preview.contours.iter().map(|c| c.problem.as_deref()).collect();
        assert_eq!(
            problems,
            vec![Some("touches another contour"), Some("touches another contour"), None]
        );
    }

    #[test]
    #[allow(clippy::items_after_statements, reason = "the types belong to the second half")]
    fn failed_preparation_keeps_assigned_layers_without_toolpaths() {
        let mut drawing = square();
        let corners = [[0., 0.], [10., 10.], [0., 10.], [10., 0.]];
        drawing.contours.push(Contour {
            layer: "0".into(),
            curves: (0..4)
                .map(|i| Curve::Line {
                    start: Point::from(corners[i]),
                    end: Point::from(corners[(i + 1) % 4]),
                })
                .collect(),
        });
        let mut draft = Draft::of(drawing.clone());
        draft.current.features.layer_edits = serde_json::from_value(serde_json::json!([
            { "contour": 0, "layer": "Engraving" }
        ]))
        .unwrap();
        draft.prepare(&drawing);
        assert!(draft.error.as_ref().unwrap().contains("crosses itself"));
        assert!(draft.prepared.is_none());
        let preview = draft.preview.as_ref().unwrap();
        assert_eq!(preview.contours[0].layer, "Engraving");
        assert_eq!(preview.contours[0].sources, vec![0]);
        assert!(preview.engraving_regions.is_empty());
        assert_eq!(preview.contours.len(), 2);

        // An unrelated bad cutting contour must not hide a valid fill preview.
        use openlaser_core::features::{Fill, Layer, LayerMode, Machining};
        draft.current.features.layers.push(Layer {
            name: "Engraving".into(),
            mode: LayerMode::Mark,
            color: None,
            machining: Some(Machining {
                fill: Some(
                    serde_json::from_value::<Fill>(serde_json::json!({
                        "spacing": 1., "angle": 0., "run_up": 2., "outline": false
                    }))
                    .unwrap(),
                ),
                ..Machining::default()
            }),
        });
        draft.prepare(&drawing);
        assert!(draft.error.as_ref().unwrap().contains("crosses itself"));
        assert!(draft.prepared.is_none());
        let preview = draft.preview.as_ref().unwrap();
        assert_eq!(preview.engraving_regions.len(), 1);
        assert_eq!(preview.engraving_regions[0].sources, vec![0]);
        assert!(preview.contours.iter().any(|c| c.layer == "Engraving"
            && !c.closed
            && c.paths.iter().any(|p| p.kind == PathKind::Travel)));
        assert!(preview.contours.iter().any(|c| c.layer == "0" && c.sources == vec![1]));

        // Invalid engraving itself remains unprepared and gets no invented hatch.
        draft.current.features.layer_edits[0].contour = 1;
        draft.prepare(&drawing);
        assert!(draft.error.is_some());
        assert!(draft.prepared.is_none());
        assert!(draft.preview.as_ref().unwrap().engraving_regions.is_empty());
        drawing.contours.push(square_at([20., 20.]));
        let mut cuts = Draft::of(drawing.clone());
        cuts.current.features.leads = Some(
            serde_json::from_value(serde_json::json!({
                "entry":{"shape":"line","length":2.,"radius":1.,"angle":45.},
                "exit":{"shape":"line","length":3.,"radius":1.,"angle":45.},
                "side":"auto","closed_only":true,"overrides":[]
            }))
            .unwrap(),
        );
        cuts.prepare(&drawing);
        assert!(cuts.prepared.is_none());
        let paths: Vec<_> = cuts
            .preview
            .as_ref()
            .unwrap()
            .contours
            .iter()
            .filter(|c| c.sources == vec![2])
            .flat_map(|c| &c.paths)
            .collect();
        assert!(paths.iter().any(|p| p.kind == PathKind::LeadIn));
        assert!(paths.iter().any(|p| p.kind == PathKind::LeadOut));
    }

    #[test]
    fn selection_output_is_exact_and_keeps_the_waste_side_of_an_inner_contour() {
        let mut drawing = square();
        let mut hole = square_at([2., 2.]);
        for curve in &mut hole.curves {
            if let Curve::Line { start, end } = curve {
                *start = Point::new(2. + (start.x - 2.) * 0.6, 2. + (start.y - 2.) * 0.6);
                *end = Point::new(2. + (end.x - 2.) * 0.6, 2. + (end.y - 2.) * 0.6);
            }
        }
        drawing.contours.push(hole);
        let mut f = Features { only_contours: Some(vec![1]), ..Features::default() };
        let selected = prepare(&drawing, &Placed::all(2), &f).unwrap();
        assert_eq!(selected.preview.contours.len(), 1);
        assert_eq!(selected.preview.contours[0].sources, vec![1]);
        assert_eq!(selected.preview.contours[0].depth, 1);
        assert_eq!(
            selected.preview.source_contours.as_ref().unwrap().len(),
            2,
            "excluded outlines stay editable without becoming machine paths"
        );
        assert_eq!(
            selected.preview.source_contours.as_ref().unwrap().len(),
            2,
            "excluded outlines stay editable without becoming machine paths"
        );
        f.only_contours = None;
        assert_eq!(prepare(&drawing, &Placed::all(2), &f).unwrap().preview.contours.len(), 2);
    }

    #[test]
    fn explicit_face_subtracts_cut_hole_and_inner_face_can_be_filled_alone() {
        use openlaser_core::features::{Fill, FillRegion, Layer, LayerMode, Machining};
        let mut drawing = square();
        let mut hole = square_at([2., 2.]);
        for curve in &mut hole.curves {
            if let Curve::Line { start, end } = curve {
                *start = Point::new(2. + (start.x - 2.) * 0.6, 2. + (start.y - 2.) * 0.6);
                *end = Point::new(2. + (end.x - 2.) * 0.6, 2. + (end.y - 2.) * 0.6);
            }
        }
        drawing.contours.push(hole);
        let mut f = Features {
            layer_edits: serde_json::from_value(
                serde_json::json!([{ "contour":0,"layer":"Engraving" }]),
            )
            .unwrap(),
            ..Features::default()
        };
        f.layers.push(Layer {
            name: "Engraving".into(),
            mode: LayerMode::Mark,
            color: None,
            machining: Some(Machining {
                fill: Some(
                    serde_json::from_value::<Fill>(
                        serde_json::json!({"spacing":1.,"angle":0.,"run_up":2.,"outline":false}),
                    )
                    .unwrap(),
                ),
                ..Machining::default()
            }),
        });
        f.fill_regions.push(FillRegion { contour: 0, holes: vec![1] });
        let ring = prepare(&drawing, &Placed::all(2), &f).unwrap();
        assert_eq!(ring.preview.engraving_regions.len(), 1);
        assert_eq!(ring.preview.engraving_regions[0].paths.len(), 2);
        assert!(
            ring.preview
                .contours
                .iter()
                .any(|c| c.layer == "0" && c.sources == vec![1] && c.closed)
        );
        let marks: Vec<_> = ring
            .preview
            .contours
            .iter()
            .filter(|c| c.layer == "Engraving")
            .flat_map(|c| &c.paths)
            .filter(|p| p.kind == PathKind::Cut)
            .collect();
        assert!(!marks.is_empty());
        for path in marks {
            for span in path.points.windows(2) {
                let x = span[0][0].midpoint(span[1][0]);
                let y = span[0][1].midpoint(span[1][1]);
                assert!(!(x > 2.0001 && x < 7.9999 && y > 2.0001 && y < 7.9999));
            }
        }
        f.layer_edits[0].contour = 1;
        f.fill_regions = vec![FillRegion { contour: 1, holes: vec![] }];
        let disk = prepare(&drawing, &Placed::all(2), &f).unwrap();
        assert_eq!(disk.preview.engraving_regions[0].sources, vec![1]);
        assert!(
            disk.preview
                .contours
                .iter()
                .filter(|c| c.layer == "Engraving")
                .flat_map(|c| &c.paths)
                .filter(|p| p.kind == PathKind::Cut)
                .all(|p| p
                    .points
                    .iter()
                    .all(|p| p[0] >= 2. && p[0] <= 8. && p[1] >= 2. && p[1] <= 8.))
        );
        f.fill_regions[0].holes = vec![0];
        assert!(prepare(&drawing, &Placed::all(2), &f).is_err());
    }

    #[test]
    fn persistence_tracks_authoring_and_history_without_runtime_geometry() {
        let mut draft = Draft::of(square());
        let saved = draft.authoring_state();
        let bytes = serde_json::to_vec(&draft).unwrap();
        draft.prepare(&square());
        assert!(saved.matches(&draft));
        assert_eq!(bytes, serde_json::to_vec(&draft).unwrap());

        // Each identity or authoring edit must cause a new durable write.
        let edits: [fn(&mut Draft); 8] = [
            |d| d.current.parts = vec![Id::from("other")],
            |d| d.job = Some(Id::from("job")),
            |d| d.source_count = 2,
            |d| d.current.sheet_offset = Some([1., 2.]),
            |d| d.current.placed[0].transform = Transform::translation(Point::new(1., 0.)),
            |d| d.current.features.order.inner_first = true,
            |d| {
                d.current.preflight =
                    openlaser_library::preflight::JobPreflight::Custom { steps: Vec::new() }
            },
            Draft::remember,
        ];
        for edit in edits {
            let mut changed = draft.clone();
            edit(&mut changed);
            assert_ne!(serde_json::to_vec(&changed).unwrap(), bytes);
            assert!(!saved.matches(&changed));
        }
        draft.remember();
        draft.current.placed[0].transform = Transform::translation(Point::new(5., 0.));
        let edited = draft.authoring_state();
        assert!(edited.matches(&draft.clone()));
        draft.undo().unwrap();
        assert!(!edited.matches(&draft));
        let undone = draft.authoring_state();
        draft.redo().unwrap();
        assert!(!undone.matches(&draft));
    }

    /// A placed contour moves under its transform while the others stay;
    /// a bridge pick rides with the contour it lies on and comes back
    /// through the inverse when picked again; an outline and its hole form
    /// one group, and a bridge joins two groups.
    #[test]
    fn placed_contours_move_and_carry_their_picks() {
        let drawing = Drawing { contours: vec![square_at([0., 0.]), square_at([20., 0.])] };
        let moved = Transform::translation(Point::new(100., 50.));
        let placed = vec![Placed::drawn(0), Placed { source: 1, copy: 0, transform: moved }];
        let sheet = place(&drawing, &placed);
        assert_eq!(sheet.contours[0], drawing.contours[0]);
        assert!(sheet.contours[1].start().unwrap().distance(Point::new(120., 50.)) < 1e-9);
        let features = Features {
            bridges: Some(Bridges {
                width: Millimeters(1.),
                connections: vec![Bridge {
                    first: Pick { contour: 0, point: Point::new(10., 5.) },
                    second: Pick { contour: 1, point: Point::new(20., 5.) },
                }],
            }),
            ..Features::default()
        };
        let bridged = placed_bridges(&place(&drawing, &placed), &features, &placed).unwrap();
        let ends = &bridged.bridges.unwrap().connections[0];
        assert!(ends.first.point.distance(Point::new(10., 5.)) < 1e-9);
        assert!(ends.second.point.distance(Point::new(120., 55.)) < 1e-9);
        assert_eq!(groups(&drawing, &placed, &[]), vec![vec![0], vec![1]]);
        assert_eq!(
            groups(
                &drawing,
                &placed,
                &prepare(&drawing, &placed, &features).unwrap().preview.contours
            ),
            vec![vec![0, 1]]
        );
        let mut draft = Draft::of(drawing.clone());
        draft.current.placed = placed;
        let picked = pick(&drawing, &draft, [125., 50.5], 2., false).unwrap().unwrap();
        assert_eq!(picked.spot.contour, 1);
        assert!(Point::from(picked.point).distance(Point::new(25., 0.)) < 1e-9);
    }

    /// A copy is its own shape under its own transform; removing a contour
    /// takes the features on it and renumbers the rest; undo and redo walk
    /// the edits both ways, and a fresh edit drops what was undone.
    #[test]
    fn copies_removal_undo_and_redo() {
        let drawing = square();
        let mut draft = Draft::of(drawing.clone());
        let added = draft.add(&[(0, Transform::translation(Point::new(20., 0.)))]);
        assert_eq!(added, vec![1]);
        assert_eq!(draft.current.placed[1].copy, 1);
        assert_eq!(groups(&drawing, &draft.current.placed, &[]), vec![vec![0], vec![1]]);
        let sheet = place(&drawing, &draft.current.placed);
        assert!(sheet.contours[1].start().unwrap().distance(Point::new(20., 0.)) < 1e-9);
        draft.current.features.start.spots =
            vec![Spot { contour: 0, fraction: 0.5 }, Spot { contour: 1, fraction: 0.25 }];
        draft.remove(&drawing, &[0]).unwrap();
        assert_eq!(draft.current.placed.len(), 1);
        assert_eq!(draft.current.placed[0].copy, 1);
        assert_eq!(draft.current.features.start.spots, vec![Spot { contour: 0, fraction: 0.25 }]);
        draft.undo().unwrap();
        assert_eq!(draft.current.placed.len(), 2);
        assert_eq!(draft.current.features.start.spots.len(), 2);
        draft.undo().unwrap();
        assert_eq!(draft.current.placed.len(), 1, "the copy is gone again");
        assert!(draft.undo().is_err(), "nothing before the first edit");
        draft.redo().unwrap();
        draft.redo().unwrap();
        assert_eq!((draft.current.placed.len(), draft.current.placed[0].copy), (1, 1));
        assert!(draft.redo().is_err());
        draft.undo().unwrap();
        draft.transform(&[1], Some(Transform::translation(Point::new(0., 5.)))).unwrap();
        assert!(draft.redo().is_err(), "an edit drops what was undone");
        assert!(
            draft.current.placed[1].transform.apply(Point::ORIGIN).distance(Point::new(20., 5.))
                < 1e-9
        );
        draft.transform(&[1], None).unwrap();
        assert_eq!(draft.current.placed[1].transform, Transform::IDENTITY);
    }

    /// Two bridges across differently rotated copies keep their owners
    /// through compaction, common group motion, removal and undo/redo.
    #[test]
    fn sequential_bridges_keep_placement_identity() {
        let drawing = square();
        let mut draft = Draft::of(drawing.clone());
        // Unrelated copy first, then three squares in a horizontal row.
        draft.current.placed = [
            Transform::translation(Point::new(0., 30.)),
            Transform::IDENTITY,
            Transform([0., 1., -1., 0., 30., 0.]),
            Transform([-1., 0., 0., -1., 50., 10.]),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, transform)| Placed { source: 0, copy: u32::try_from(i).unwrap(), transform })
        .collect();
        draft.current.features.bridges =
            Some(Bridges { width: Millimeters(2.), connections: vec![] });
        for [a, b] in [[[10., 5.], [20., 5.]], [[30., 5.], [40., 5.]]] {
            let pick_end = |at| {
                let picked = pick(&drawing, &draft, at, 0.01, true).unwrap().unwrap();
                Pick { contour: picked.spot.contour, point: picked.point.into() }
            };
            let bridge = Bridge { first: pick_end(a), second: pick_end(b) };
            draft.current.features.bridges.as_mut().unwrap().connections.push(bridge);
            draft.prepare(&drawing);
            assert!(draft.error.is_none(), "{:?}", draft.error);
        }
        assert!(draft.groups.contains(&vec![1, 2, 3]));
        let original = draft.prepared.as_ref().unwrap().preview.length_mm;
        draft.transform(&[2], Some(Transform::translation(Point::new(7., 8.)))).unwrap();
        draft.prepare(&drawing);
        assert!(draft.error.is_none(), "{:?}", draft.error);
        assert!((draft.prepared.as_ref().unwrap().preview.length_mm - original).abs() < 1e-8);
        draft.remove(&drawing, &[0]).unwrap();
        draft.prepare(&drawing);
        assert!(draft.error.is_none(), "{:?}", draft.error);
        assert_eq!(draft.current.features.bridges.as_ref().unwrap().connections.len(), 2);
        assert_eq!(draft.groups, vec![vec![0, 1, 2]]);
        let expected = draft.prepared.as_ref().unwrap().contours.clone();
        draft.undo().unwrap();
        draft.prepare(&drawing);
        assert!(draft.error.is_none());
        draft.redo().unwrap();
        draft.prepare(&drawing);
        assert_eq!(draft.prepared.as_ref().unwrap().contours, expected);
    }

    /// The anchor names a point of the prepared part's bounds. Pinning puts
    /// that point at an origin on the bed by fixing what the machine adds
    /// to a drawing coordinate; from then on the origin reads back as the
    /// anchor point wherever the part is moved, and settling lands an
    /// unplaced sheet at the bed's front-left once.
    #[test]
    fn pinning_puts_the_anchor_point_at_the_origin() {
        let moved = Drawing { contours: vec![square_at([30., 40.])] };
        let mut draft = Draft::of(moved.clone());
        draft.prepare(&moved);
        assert_eq!(draft.anchor_point(), Some([30., 40.]));
        assert!(draft.sheet_offset().is_err(), "not placed yet");
        draft.place_anchor_at([500., 300.]).unwrap();
        let zero = draft.sheet_offset().unwrap();
        assert!((zero[0] - 470.).abs() < 1e-9 && (zero[1] - 260.).abs() < 1e-9, "{zero:?}");
        draft.current.anchor = Anchor::Center;
        assert_eq!(draft.anchor_point(), Some([35., 45.]));
        assert_eq!(draft.origin(), Some([505., 305.]), "the marker moves, the part stays");
        draft.transform(&[0], Some(Transform::translation(Point::new(10., 0.)))).unwrap();
        draft.prepare(&moved);
        let again = draft.sheet_offset().unwrap();
        assert!(
            (again[0] - zero[0]).abs() < 1e-9 && (again[1] - zero[1]).abs() < 1e-9,
            "a move on the sheet leaves zero alone"
        );
        assert_eq!(draft.origin(), Some([515., 305.]));
        let mut fresh = Draft::of(moved.clone());
        fresh.prepare(&moved);
        fresh.restore_placement(Some([[0., 1300.], [0., 900.]]));
        assert_eq!(fresh.origin(), None, "head mode waits for a physical capture");
        fresh.restore_placement(None);
        assert_eq!(fresh.origin(), None);
        fresh.current.placement = None;
        fresh.restore_placement(Some([[0., 1300.], [0., 900.]]));
        assert_eq!(fresh.origin(), Some([0., 0.]), "legacy placement is preserved");
    }

    /// A sheet that cannot be prepared, here two squares crossing, still
    /// shows every contour where it lies, so the operator can pull them
    /// apart; the error says why.
    #[test]
    fn an_unpreparable_sheet_still_shows() {
        let mut draft = Draft::of(square());
        draft.add(&[(0, Transform::translation(Point::new(5., 5.)))]);
        draft.prepare(&square());
        assert!(draft.prepared.is_none());
        assert!(draft.error.as_deref().is_some_and(|e| e.contains("cross")), "{:?}", draft.error);
        let preview = draft.preview.as_ref().unwrap();
        assert_eq!(preview.contours.len(), 2);
        assert_eq!(preview.contours[1].sources, vec![1]);
        assert_eq!(draft.anchor_point(), Some([0., 0.]));
    }

    /// Preparation yields the compiler's contours and a preview with the
    /// cut start after the lead-in.
    #[test]
    fn preparation_produces_contours_and_a_preview() {
        let prepared = prepare(&square(), &Placed::all(1), &Features::default()).unwrap();
        assert_eq!(prepared.contours.len(), 1);
        assert!(prepared.contours[0].segments.iter().all(|s| s.source.is_some()));
        assert!((prepared.preview.length_mm - 40.).abs() < 1e-9);
        assert!(prepared.preview.contours[0].closed);
        assert_eq!(outline(&square(), 8)[0].len(), 5);
    }

    /// A machine backup as a bundle.
    fn backup() -> Bundle {
        let path = format!("{}/../../fixtures/xml/harness-backup.xml", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        Bundle::from_backup(
            openlaser_xml::Document::parse(openlaser_xml::Kind::Backup, &bytes).unwrap(),
        )
    }

    /// A film-removing cutting recipe and a film recipe kept in the same
    /// bank bind apart: each lies over its own copy of the files, so the
    /// compiled film pass runs the film recipe's power and the cut its
    /// own; without a film process the compile names what is missing.
    #[test]
    fn the_film_process_binds_apart_from_the_cutting_bank() {
        let bundle = backup();
        let mut cut =
            crate::recipes::from_bank(&bundle, openlaser_core::LaserMode::Fiber, 1, "Steel", 2.)
                .unwrap();
        cut.layer = 11;
        cut.attributes.insert("CutPower".into(), "77".into());
        cut.attributes.insert("WithFilm".into(), "1".into());
        let mut film = cut.clone();
        film.attributes.insert("CutPower".into(), "12".into());
        film.attributes.remove("WithFilm");
        let binder = Binder {
            dry_run: false,
            manual_focus: false,
            film: false,
            scale: 1000,
            timeouts: Timeouts::default(),
        };
        let cutting = settings(&bundle, &cut, binder).unwrap();
        let filming = settings(&bundle, &film, Binder { film: true, ..binder }).unwrap();
        assert!(cutting.with_film && cutting.power == 77);
        assert!(filming.pierce.is_empty() && !filming.with_film && filming.power == 12);
        let prepared = prepare(&square(), &Placed::all(1), &Features::default()).unwrap();
        assert_eq!(prepared.film[0].len(), 1, "one run of bare geometry");
        let compiled = compile(
            &prepared,
            &cutting,
            Some(&filming),
            &crate::layers::Layered::default(),
            false,
            1000,
        )
        .unwrap();
        let kinds: Vec<PassKind> = compiled.job.passes.iter().map(|p| p.pass.kind).collect();
        assert_eq!(kinds, [PassKind::Film, PassKind::Cut]);
        assert_eq!(compiled.job.passes[0].settings.power, 12);
        assert_eq!(compiled.job.passes[1].settings.power, 77);
        assert_eq!(compiled.view.plan.len(), 2);
        assert!(compiled.view.moves.iter().any(|m| m.kind == PathKind::Film && m.pass == Some(0)));
        assert_eq!(compiled.view.pierces.len(), 1, "the film pass does not pierce");
        let missing =
            compile(&prepared, &cutting, None, &crate::layers::Layered::default(), false, 1000)
                .unwrap_err()
                .to_string();
        assert!(missing.contains("Resolve a separate film process"), "{missing}");
    }

    /// The contour shift moves the prepared geometry by the difference
    /// from what is applied: asking twice moves once, asking for another
    /// shift moves by the difference, and the source stays where it is.
    #[test]
    fn the_contour_shift_is_applied_once() {
        let close =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        let prepared = prepare(&square(), &Placed::all(1), &Features::default()).unwrap();
        let corner = prepared.contours[0].segments[0].curve.point(0.);
        let shifted = prepared.shifted([10., -3.]).unwrap();
        let moved = shifted.contours[0].segments[0].curve.point(0.);
        assert!(close(moved, [corner[0] + 10., corner[1] - 3.]));
        assert!(close(shifted.film[0][0].segments[0].curve.point(0.), moved));
        let again = shifted.shifted([10., -3.]).unwrap();
        assert!(close(again.contours[0].segments[0].curve.point(0.), moved), "applied once");
        let retargeted = shifted.shifted([5., 0.]).unwrap();
        assert!(close(
            retargeted.contours[0].segments[0].curve.point(0.),
            [corner[0] + 5., corner[1]]
        ));
        assert!(close(retargeted.shift, [5., 0.]));
        assert!(
            close(prepared.contours[0].segments[0].curve.point(0.), corner),
            "the source stays"
        );
        assert!(prepared.shifted([100_001., 0.]).is_err());
    }
    #[test]
    fn editing_a_part_keeps_other_parts_copies_and_undo() {
        let a = Id::generate();
        let b = Id::generate();
        let replacement = Id::generate();
        let mut drawing = square();
        drawing.contours.push(square_at([20., 0.]));
        let old = Arc::new(JobDrawing::new(vec![
            (a.clone(), Arc::new(drawing.clone())),
            (b.clone(), Arc::new(Drawing { contours: vec![square_at([50., 0.])] })),
        ]));
        let mut draft = Draft::new(old.clone());
        draft.current.placed = vec![
            Placed::drawn(0),
            Placed::drawn(1),
            Placed::drawn(2),
            Placed { source: 0, copy: 1, transform: Transform::translation(Point::new(100., 0.)) },
            Placed { source: 1, copy: 1, transform: Transform::translation(Point::new(100., 0.)) },
        ];
        draft.current.features.only_contours = Some(vec![2]);
        draft.current.features.order.strategy = OrderStrategy::Manual(vec![2, 0, 1, 3, 4]);
        let original = draft.current.clone();
        drawing.contours[1] = square_at([25., 0.]);
        drawing.contours.push(square_at([40., 0.]));
        let new = Arc::new(JobDrawing::new(vec![
            (replacement.clone(), Arc::new(drawing)),
            (b.clone(), Arc::new(Drawing { contours: vec![square_at([50., 0.])] })),
        ]));
        draft.replace_part(new.clone(), 0).unwrap();
        assert_eq!(draft.current.parts, vec![replacement, b]);
        assert_eq!(draft.current.placed.len(), 7);
        assert_eq!(draft.current.placed[3].source, 3);
        assert_eq!(draft.current.features.only_contours, Some(vec![3]));
        assert_eq!(draft.current.placed[4].transform, original.placed[3].transform);
        let edited = draft.current.clone();
        draft.undo().unwrap();
        draft.attach(old).unwrap();
        assert_eq!(draft.current, original);
        draft.redo().unwrap();
        draft.attach(new).unwrap();
        assert_eq!(draft.current, edited);
    }

    fn edit(contour: usize, layer: &str) -> openlaser_core::features::LayerEdit {
        openlaser_core::features::LayerEdit { contour, layer: layer.into() }
    }

    /// A layer move stays on its shape when a part before it grows: the
    /// later part's contour moves up and its layer goes with it. A move on an
    /// unchanged contour of the edited part stays, one on a changed contour
    /// goes, as the shape it named is gone.
    #[test]
    fn layer_moves_follow_their_contours_when_a_part_is_replaced() {
        let (a, b, replacement) = (Id::generate(), Id::generate(), Id::generate());
        let mut drawing = square();
        drawing.contours.push(square_at([20., 0.]));
        let other = Arc::new(Drawing { contours: vec![square_at([50., 0.])] });
        let old = Arc::new(JobDrawing::new(vec![
            (a, Arc::new(drawing.clone())),
            (b.clone(), other.clone()),
        ]));
        let mut draft = Draft::new(old);
        draft.current.features.layer_edits =
            vec![edit(0, "Kept"), edit(1, "Gone"), edit(2, "Etch")];
        drawing.contours[1] = square_at([25., 0.]);
        drawing.contours.push(square_at([40., 0.]));
        let new = Arc::new(JobDrawing::new(vec![(replacement, Arc::new(drawing)), (b, other)]));
        draft.replace_part(new, 0).unwrap();
        assert_eq!(draft.current.features.layer_edits, vec![edit(0, "Kept"), edit(3, "Etch")]);
    }

    /// On a nested sheet the edited part keeps its copies' nested places and
    /// the sheet keeps its stock; only a stock outline that is itself the
    /// edited shape refuses, as the sheet would change under the parts.
    #[test]
    fn a_nested_part_is_replaced_in_place() {
        use openlaser_core::nesting::{NestSettings, NestStock};
        let (stock, part, other, edited) =
            (Id::generate(), Id::generate(), Id::generate(), Id::generate());
        let one = |c: Contour| Arc::new(Drawing { contours: vec![c] });
        let sheet = one(square_at([-5., -5.]));
        let old = Arc::new(JobDrawing::new(vec![
            (stock.clone(), sheet.clone()),
            (part.clone(), one(square().contours[0].clone())),
            (other.clone(), one(square_at([50., 0.]))),
        ]));
        let mut draft = Draft::new(old.clone());
        let nested = |x: f64| Transform::translation(Point::new(x, 0.));
        draft.current.placed = vec![
            Placed { source: 1, copy: 0, transform: nested(10.) },
            Placed { source: 1, copy: 1, transform: nested(30.) },
            Placed { source: 2, copy: 2, transform: nested(0.) },
        ];
        let outline = NestStock::Outline { contour: Placed::drawn(0) };
        draft.current.nesting = Some(Nesting { stock: outline, settings: NestSettings::default() });
        // The part gains a hole.
        let mut holed = square().contours;
        holed.push(square_at([2., 2.]));
        let new = Arc::new(JobDrawing::new(vec![
            (stock.clone(), sheet),
            (edited.clone(), Arc::new(Drawing { contours: holed })),
            (other.clone(), one(square_at([50., 0.]))),
        ]));
        draft.replace_part(new.clone(), 1).unwrap();
        let placed: Vec<_> =
            draft.current.placed.iter().map(|p| (p.source, p.copy, p.transform)).collect();
        assert_eq!(
            placed,
            vec![
                (1, 0, nested(10.)),
                (2, 0, nested(10.)),
                (1, 1, nested(30.)),
                (2, 1, nested(30.)),
                (3, 2, nested(0.)),
            ],
            "both copies keep their nested place; the other part moves up a contour"
        );
        assert_eq!(outline_source(draft.current.nesting.as_ref()), Some(0), "the stock stays");

        // Editing the stock outline itself is refused, and nothing changes.
        let before = draft.current.clone();
        let reshaped = Arc::new(JobDrawing::new(vec![
            (Id::generate(), one(square_at([-9., -9.]))),
            (edited, Arc::new(Drawing { contours: new.drawing().contours[1..3].to_vec() })),
            (other, one(square_at([50., 0.]))),
        ]));
        assert!(draft.replace_part(reshaped, 0).is_err());
        assert_eq!(draft.current, before);
    }

    /// Dropping a part nothing places moves the later parts' contours down,
    /// and their layer moves with them.
    #[test]
    fn layer_moves_follow_their_contours_when_a_part_is_dropped() {
        let one = |x: f64| Arc::new(Drawing { contours: vec![square_at([x, 0.])] });
        let parts =
            vec![(Id::generate(), one(0.)), (Id::generate(), one(20.)), (Id::generate(), one(40.))];
        let mut draft = Draft::new(Arc::new(JobDrawing::new(parts)));
        draft.current.placed.retain(|p| p.source != 1);
        draft.current.features.layer_edits = vec![edit(1, "Dropped"), edit(2, "Etch")];
        draft.prune_parts();
        assert_eq!(draft.current.parts.len(), 2);
        assert_eq!(draft.current.features.layer_edits, vec![edit(1, "Etch")]);
    }
}
