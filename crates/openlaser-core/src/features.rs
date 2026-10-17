// SPDX-License-Identifier: GPL-3.0-or-later

//! The machining features an operator chooses for a job: how contours are
//! entered and left, held, cooled, compensated, bridged and ordered.
//!
//! These are persisted in jobs, so every type serialises; absent features
//! read as off, and new fields must carry defaults.

use crate::geometry::Point;
use crate::units::{Degrees, Millimeters, Milliseconds, MmPerSecond, Percent};
use serde::{Deserialize, Serialize};

/// Everything preparation needs beyond the drawing.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Features {
    /// Entry and exit leads.
    pub leads: Option<Leads>,
    /// Micro-joints that hold parts in the sheet.
    pub joints: Option<Joints>,
    /// Cooling stops.
    pub cooling: Option<Cooling>,
    /// Kerf compensation.
    pub kerf: Option<Kerf>,
    /// Bridges between contours.
    pub bridges: Option<Bridges>,
    /// Share compatible coincident spans between these placed contours.
    pub common: Option<CommonEdges>,
    /// Cut the straight edges of these placed contours in flying passes;
    /// left out of the file when off, as earlier builds wrote it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub fly: Option<FlyCut>,
    /// Where each contour starts and which way it runs.
    pub start: Start,
    /// How a closed contour's seam is treated.
    pub seam: Seam,
    /// The cutting order.
    pub order: CutOrder,
    /// Drawing layers left uncut.
    pub skip_layers: Vec<String>,
    /// Shapes moved to another layer than the drawing gives them; left
    /// out of the file when there are none, as earlier builds wrote it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "typescript", ts(as = "Option<Vec<LayerEdit>>", optional))]
    pub layer_edits: Vec<LayerEdit>,
    /// The layers in the order they run, with what each does and its own
    /// colour and machining. Empty until the operator changes a layer.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "typescript", ts(as = "Option<Vec<Layer>>", optional))]
    pub layers: Vec<Layer>,
    /// Explicit engraving areas: the owner boundary minus its direct holes,
    /// by placed contour identity. Empty preserves the legacy layer fill rule.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "typescript", ts(as = "Option<Vec<FillRegion>>", optional))]
    pub fill_regions: Vec<FillRegion>,
    /// Machining chosen for a selection of contours, over their layer's
    /// and the job's: leads on one hole only, joints on some parts. A
    /// placed contour belongs to one selection at most.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "typescript", ts(as = "Option<Vec<ContourMachining>>", optional))]
    pub contour_machining: Vec<ContourMachining>,
    /// Work only these placed contours; None works every active contour.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub only_contours: Option<Vec<usize>>,
    /// Images on placed contours. Filled in by the server when it
    /// prepares, from the parts; never saved.
    #[serde(skip)]
    #[cfg_attr(feature = "typescript", ts(skip))]
    pub rasters: Vec<crate::raster::PlacedRaster>,
}

/// One explicitly chosen engraving area. Hole boundaries retain their own
/// cutting or marking layer; they only subtract from this region's scan.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FillRegion {
    /// The placed contour enclosing the selected face.
    pub contour: usize,
    /// Directly nested placed boundaries excluded from the selected face.
    #[serde(default)]
    pub holes: Vec<usize>,
}

/// One part of the machining a selection of contours can set for itself.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MachiningPart {
    /// Entry and exit leads.
    Leads,
    /// Micro-joints.
    Joints,
    /// Cooling stops.
    Cooling,
    /// Kerf compensation.
    Kerf,
    /// Start position, direction and seam.
    Start,
}

impl MachiningPart {
    /// Every part, as a layer with its own machining sets them.
    pub const ALL: [Self; 5] = [Self::Leads, Self::Joints, Self::Cooling, Self::Kerf, Self::Start];
}

/// Machining a selection of placed contours sets for itself. Each part
/// named in `own` replaces what the contours' layer or the job gives them;
/// the rest still follows the layer. Picked places stay the job's.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContourMachining {
    /// Placed contour identities, before bridges.
    pub contours: Vec<usize>,
    /// The parts this selection sets itself.
    pub own: Vec<MachiningPart>,
    /// Its settings for those parts; the others are ignored.
    pub machining: Machining,
}

/// What a layer does to the sheet.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerMode {
    /// Cut through: its closed shapes are the parts and their holes.
    #[default]
    Cut,
    /// Traced on the surface: never a part or a hole, and without the
    /// job's machining unless the layer has its own.
    Mark,
}

/// One of the job's layers.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// Its name.
    pub name: String,
    /// What it does to the sheet.
    #[serde(default)]
    pub mode: LayerMode,
    /// Its colour on screen, over the drawing's own.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Its own machining, over the job's.
    #[serde(default)]
    pub machining: Option<Machining>,
}

/// What happens along a layer's contours, when the layer has its own.
/// Picked places, such as manual starts, joints and lead edits, stay the
/// job's, so a layer's manual joints are the job's manual joints.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Machining {
    /// Entry and exit leads.
    pub leads: Option<Leads>,
    /// Micro-joints.
    pub joints: Option<Joints>,
    /// Cooling stops.
    pub cooling: Option<Cooling>,
    /// Kerf compensation.
    pub kerf: Option<Kerf>,
    /// Where each contour starts and which way it runs.
    pub start: Start,
    /// How a closed contour's seam is treated.
    pub seam: Seam,
    /// Fill the closed shapes of a marked layer with scan lines; left out
    /// of the file when off, as earlier builds wrote it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub fill: Option<Fill>,
}

/// One shape on a layer of the operator's choosing. The shape is a contour
/// of the job's drawing, so every copy of it follows.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerEdit {
    /// The drawing contour.
    pub contour: usize,
    /// The layer it is on.
    pub layer: String,
}

impl Features {
    /// The layer's entry in the table, when it has one.
    #[must_use]
    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }

    /// What the layer does to the sheet.
    #[must_use]
    pub fn mode(&self, name: &str) -> LayerMode {
        self.layer(name).map_or(LayerMode::Cut, |l| l.mode)
    }

    /// The features a layer's contours are prepared with: its own
    /// machining over the job's, keeping the job's picked places. A marked
    /// layer without machining of its own has none.
    #[must_use]
    pub fn for_layer(&self, name: &str) -> std::borrow::Cow<'_, Self> {
        let Some(layer) = self.layer(name) else { return std::borrow::Cow::Borrowed(self) };
        let own = match (&layer.machining, layer.mode) {
            (Some(machining), _) => machining.clone(),
            (None, LayerMode::Mark) => Machining::default(),
            (None, LayerMode::Cut) => return std::borrow::Cow::Borrowed(self),
        };
        std::borrow::Cow::Owned(self.machined(&own, &MachiningPart::ALL))
    }

    /// The selection of contours `sources` belongs to, if one has its own
    /// machining.
    #[must_use]
    pub fn selection_of(&self, sources: &[usize]) -> Option<&ContourMachining> {
        self.contour_machining.iter().find(|m| sources.iter().any(|s| m.contours.contains(s)))
    }

    /// These features with the `parts` of `own` in place of theirs, keeping
    /// the picked places: manual joints and cooling stops, chosen starts and
    /// lead edits.
    #[must_use]
    pub fn machined(&self, own: &Machining, parts: &[MachiningPart]) -> Self {
        let mut features = self.clone();
        for part in parts {
            match part {
                MachiningPart::Leads => {
                    features.leads = own.leads.clone().map(|mut leads| {
                        leads.overrides =
                            self.leads.as_ref().map(|l| l.overrides.clone()).unwrap_or_default();
                        leads
                    });
                }
                MachiningPart::Joints => {
                    features.joints = own.joints.clone().map(|mut joints| {
                        if matches!(joints.placement, JointPlacement::Manual(_)) {
                            joints.placement = match self.joints.as_ref().map(|j| &j.placement) {
                                Some(manual @ JointPlacement::Manual(_)) => manual.clone(),
                                _ => JointPlacement::Manual(Vec::new()),
                            };
                        }
                        joints
                    });
                }
                MachiningPart::Cooling => {
                    features.cooling = own.cooling.clone().map(|mut cooling| {
                        if matches!(cooling.placement, CoolingPlacement::Manual(_)) {
                            cooling.placement = match self.cooling.as_ref().map(|c| &c.placement) {
                                Some(manual @ CoolingPlacement::Manual(_)) => manual.clone(),
                                _ => CoolingPlacement::Manual(Vec::new()),
                            };
                        }
                        cooling
                    });
                }
                MachiningPart::Kerf => features.kerf = own.kerf,
                MachiningPart::Start => {
                    features.start = Start { spots: self.start.spots.clone(), ..own.start.clone() };
                    features.seam = own.seam;
                }
            }
        }
        features
    }

    /// Machining defaults for another drawing. Widths, speeds and automatic
    /// strategies carry over; picked locations and layer choices do not.
    #[must_use]
    pub fn reusable(&self) -> Self {
        let mut features = self.renumbered_locations(|_| None);
        features.common = None;
        features.only_contours = None;
        if matches!(features.start.position, StartPosition::Manual(_)) {
            features.start.position = StartPosition::default();
        }
        if matches!(features.order.strategy, OrderStrategy::Manual(_)) {
            features.order.strategy = OrderStrategy::default();
        }
        if let Some(bridges) = &mut features.bridges {
            bridges.connections.clear();
        }
        features.skip_layers.clear();
        features.layer_edits.clear();
        features.layers.clear();
        features.contour_machining.clear();
        features
    }

    /// Renumber manual spots and cut order by placed contour identity,
    /// dropping locations on removed contours. Bridge references name an
    /// evolving topology; their remapping belongs to the preparation crate.
    #[must_use]
    pub fn renumbered_locations(&self, renumber: impl Fn(usize) -> Option<usize>) -> Self {
        let mut features = self.clone();
        features.only_contours = self
            .only_contours
            .as_ref()
            .map(|ids| ids.iter().filter_map(|&i| renumber(i)).collect());
        features.fill_regions = self
            .fill_regions
            .iter()
            .filter_map(|r| {
                Some(FillRegion {
                    contour: renumber(r.contour)?,
                    holes: r.holes.iter().filter_map(|&i| renumber(i)).collect(),
                })
            })
            .collect();
        let keep = |spots: &mut Vec<Spot>| {
            spots.retain_mut(|spot| match renumber(spot.contour) {
                Some(contour) => {
                    spot.contour = contour;
                    true
                }
                None => false,
            });
        };
        if let Some(joints) = &mut features.joints
            && let JointPlacement::Manual(spots) = &mut joints.placement
        {
            keep(spots);
        }
        if let Some(cooling) = &mut features.cooling
            && let CoolingPlacement::Manual(spots) = &mut cooling.placement
        {
            keep(spots);
        }
        keep(&mut features.start.spots);
        if let Some(leads) = &mut features.leads {
            leads.overrides.retain_mut(|lead| match renumber(lead.location.contour) {
                Some(contour) => {
                    lead.location.contour = contour;
                    true
                }
                None => false,
            });
        }
        if let Some(common) = &mut features.common {
            common.contours = common.contours.iter().filter_map(|&i| renumber(i)).collect();
            if common.contours.len() < 2 {
                features.common = None;
            }
        }
        if let OrderStrategy::Manual(order) = &mut features.order.strategy {
            *order = order.iter().filter_map(|&contour| renumber(contour)).collect();
        }
        for selection in &mut features.contour_machining {
            selection.contours = selection.contours.iter().filter_map(|&i| renumber(i)).collect();
        }
        features.contour_machining.retain(|selection| !selection.contours.is_empty());
        features
    }
}

/// Selected contours whose shared spans can be cut once.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommonEdges {
    /// Placed contour identities, before bridges.
    pub contours: Vec<usize>,
    /// Maximum separation for equivalent spans, in millimetres.
    pub tolerance: f64,
    /// Permit retracing a shared span to connect remaining cuts on one contour.
    pub allow_overcut: bool,
}

/// Straight edges that lie on one line, cut in one continuous pass with the
/// laser switched off over the gaps between them: perforated sheet, grids
/// and rows of slots. Each line starts with the head already moving, so a
/// cut begins with a flying pierce rather than a stationary one.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools, reason = "independent switches the operator sets")]
pub struct FlyCut {
    /// Placed contour identities, before bridges. Contours made of straight
    /// lines fly along shared lines; circles and other closed contours fly
    /// whole when switched on below, or are cut as usual.
    pub contours: Vec<usize>,
    /// Laser-off approach before the first cut of a line and after its
    /// last, in millimetres, so the head is at speed when the beam comes on.
    /// Zero starts at the first cut with a normal pierce.
    pub run_up: f64,
    /// How far each cut runs past its ends, in millimetres, so corners
    /// cut by two crossing lines separate cleanly.
    pub overcut: f64,
    /// Maximum distance between edges that count as one line, in millimetres.
    pub tolerance: f64,
    /// Every pass of a direction runs the same way, instead of alternating.
    #[serde(default)]
    pub one_way: bool,
    /// Start from the far side: the last line of each direction first.
    #[serde(default)]
    pub from_far_side: bool,
    /// Run the directions in the opposite order, such as vertical lines
    /// before horizontal ones.
    #[serde(default)]
    pub directions_reversed: bool,
    /// Cut the selected round holes flying too: each circle is entered on
    /// a tangent at speed, the circles of a row joined in one movement.
    #[serde(default)]
    pub circles: bool,
    /// Cut the other selected closed contours, arcs and all, flying too:
    /// each starts from a laser-off approach and runs on to the next.
    #[serde(default)]
    pub outlines: bool,
    /// How far a flying circle or contour runs past its start to close,
    /// in millimetres.
    #[serde(default = "fly_overlap")]
    pub overlap: f64,
}

const fn fly_overlap() -> f64 {
    0.3
}

/// A marked layer's closed shapes engraved by scan lines: one continuous
/// zigzag with the laser on inside the shapes and off outside them.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// Distance between scan lines, in millimetres.
    pub spacing: f64,
    /// Scan direction, in degrees counterclockwise from X.
    pub angle: f64,
    /// Laser-off travel beyond the shapes at both ends of every line, in
    /// millimetres, for turning round at speed.
    pub run_up: f64,
    /// Trace the outlines after the fill, for crisp edges.
    pub outline: bool,
    /// Trace the holes too, such as the inside of an O; off, the trace
    /// follows only the outer outlines. Left out of the file while on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub outline_holes: Option<bool>,
    /// Engrave in both directions; off, every line runs the same way and
    /// the head returns with the laser off.
    #[serde(default = "yes")]
    pub bidirectional: bool,
    /// How many times the shapes are scanned, 1 to 20.
    #[serde(default = "one")]
    pub passes: u32,
    /// Degrees each pass turns from the one before: 90 crosshatches, 0
    /// scans the same way again.
    #[serde(default)]
    pub pass_angle: f64,
    /// How far each pass moves its lines across, as a share of the
    /// spacing: 0.5 puts a second pass between the first one's lines.
    #[serde(default)]
    pub pass_shift: f64,
    /// Scan offsets measured on this machine at the layer's speed, as
    /// `[angle, offset]` pairs, added to `scan_offset` for each pass's
    /// angle. Filled in by the server when it prepares; never saved.
    #[serde(skip)]
    #[cfg_attr(feature = "typescript", ts(skip))]
    pub calibrated: Vec<[f64; 2]>,
    /// Scan offset in millimetres: every line's marks move this far back
    /// against its travel, so lines run in opposite directions line up.
    /// Half the measured shift between the two directions.
    #[serde(default)]
    pub scan_offset: f64,
    /// Scan each disconnected object separately instead of across the whole layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub per_object: Option<bool>,
    /// How an image framed by the shapes becomes power; left out of the
    /// file when the defaults apply. Shapes without an image ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", ts(optional))]
    pub image: Option<ImageFill>,
}

impl Fill {
    /// What an image layer engraves with until the operator says otherwise:
    /// lines 0.1 mm apart, both ways, with a short overscan.
    #[must_use]
    pub fn for_image() -> Self {
        Self {
            spacing: 0.1,
            angle: 0.,
            run_up: 2.,
            outline: false,
            outline_holes: None,
            bidirectional: true,
            passes: 1,
            pass_angle: 0.,
            pass_shift: 0.,
            calibrated: Vec::new(),
            scan_offset: 0.,
            per_object: None,
            image: Some(ImageFill::default()),
        }
    }
}

/// How an image's gray levels become laser power.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageMode {
    /// Power follows the gray level, between the least and the most.
    #[default]
    Grayscale,
    /// Dots at full power, spread by error diffusion to give the gray.
    Dither,
    /// Full power where darker than middle gray, none elsewhere.
    Threshold,
}

/// An image engraving's settings.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageFill {
    /// How gray becomes power.
    pub mode: ImageMode,
    /// Power for the lightest gray engraved, in percent of the layer's power.
    pub min_power: f64,
    /// Power for black, in percent of the layer's power.
    pub max_power: f64,
    /// Engrave the light parts instead of the dark ones.
    pub invert: bool,
    /// Midtone correction: above 1 lightens them, below 1 darkens them.
    pub gamma: f64,
}

impl Default for ImageFill {
    fn default() -> Self {
        Self {
            mode: ImageMode::Grayscale,
            min_power: 10.,
            max_power: 100.,
            invert: false,
            gamma: 1.,
        }
    }
}

const fn yes() -> bool {
    true
}

const fn one() -> u32 {
    1
}

/// A place along a drawing contour: the contour's index in the drawing and
/// the fraction of its length, as drawn.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    /// The contour.
    pub contour: usize,
    /// How far along it, 0 to 1.
    pub fraction: f64,
}

/// Entry and exit leads.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Leads {
    /// The lead-in, if any.
    pub entry: Option<Lead>,
    /// The lead-out, if any.
    pub exit: Option<Lead>,
    /// Which side of the contour the leads lie on.
    pub side: Side,
    /// Leave open contours without leads.
    pub closed_only: bool,
    /// Inside a closed contour, how far a lead's free end keeps off the
    /// contour; longer leads are shortened to keep it. None is 0.5 mm.
    pub clearance: Option<Millimeters>,
    /// Leads edited on the drawing. Locations name placed contours and
    /// distinguish pieces created by a bridge split.
    pub overrides: Vec<LeadOverride>,
}

impl Leads {
    /// Resolve one contour's edit against the job defaults. Entry/exit
    /// switches still disable that role across the job, including overrides.
    #[must_use]
    pub fn resolved(&self, edited: Option<&LeadOverride>) -> Self {
        Self {
            entry: self.entry.map(|base| edited.and_then(|v| v.entry).unwrap_or(base)),
            exit: self.exit.map(|base| edited.and_then(|v| v.exit).unwrap_or(base)),
            side: self.side,
            closed_only: self.closed_only,
            clearance: self.clearance,
            overrides: Vec::new(),
        }
    }
}

/// A lead edit anchored to one placed contour. The retained location follows
/// that contour through moves, copies and bridge operations; it does not
/// change the cutting start. A missing role inherits its job default.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LeadOverride {
    /// A retained point on the owning source contour, before compensation.
    pub location: Spot,
    /// The local entry definition, if edited.
    #[serde(default)]
    pub entry: Option<Lead>,
    /// The local exit definition, if edited.
    #[serde(default)]
    pub exit: Option<Lead>,
}

/// One lead.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lead {
    /// Its shape.
    pub shape: LeadShape,
    /// The length of the straight part.
    pub length: Millimeters,
    /// The radius of the arc part.
    pub radius: Millimeters,
    /// The angle between the lead and the contour, or the arc's sweep.
    pub angle: Degrees,
}

/// The shape of a lead.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadShape {
    /// A straight line at an angle to the contour.
    Line,
    /// An arc tangent to the contour.
    Arc,
    /// A straight line into an arc tangent to the contour.
    LineArc,
}

/// Which side of a contour something lies on.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// Inside for holes, outside for outlines, by nesting depth.
    #[default]
    Auto,
    /// Inside the contour.
    Inside,
    /// Outside the contour.
    Outside,
}

/// Micro-joints: short gaps in the cut that hold a part in the sheet.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Joints {
    /// Where the joints go.
    pub placement: JointPlacement,
    /// How wide each joint is along the contour.
    pub width: Millimeters,
    /// Contours smaller than this in both directions get no joints.
    pub minimum_size: Millimeters,
    /// Joint only outlines, never holes.
    pub outer_only: bool,
    /// On an open contour, put a joint at its start as well.
    #[serde(default)]
    pub open_start: bool,
    /// How the laser behaves in a joint.
    pub behaviour: JointBehaviour,
    /// A slower speed through the joint.
    pub slow_speed: Option<MmPerSecond>,
    /// Pierce again after each joint.
    pub repierce: bool,
}

/// Where micro-joints go.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointPlacement {
    /// So many, evenly spread along the contour.
    Count(u32),
    /// Every so far along the contour.
    Spacing(Millimeters),
    /// At grid lines across the X axis, so many between the contour's ends.
    AcrossX(u32),
    /// At grid lines across the Y axis, so many between the contour's ends.
    AcrossY(u32),
    /// At these places, each on its own contour.
    Manual(Vec<Spot>),
}

/// What the laser does in a joint.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointBehaviour {
    /// Off.
    #[default]
    LaserOff,
    /// On at a fraction of the recipe's power, leaving a trace.
    Power(Percent),
}

/// Cooling stops: the head waits with the laser off.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cooling {
    /// How long each stop lasts.
    pub dwell: Milliseconds,
    /// Where the stops go.
    pub placement: CoolingPlacement,
}

/// Where cooling stops go.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoolingPlacement {
    /// Chosen from the geometry.
    Automatic {
        /// A stop where the contour starts.
        at_start: bool,
        /// A stop at every corner sharper than this interior angle.
        corners_below: Option<Degrees>,
    },
    /// At these places, each on its own contour.
    Manual(Vec<Spot>),
}

/// Kerf compensation: the path moves half the kerf into the waste.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Kerf {
    /// The width the beam removes.
    pub width: Millimeters,
    /// Which side the waste is on.
    pub side: Side,
}

/// Bridges: two contours joined through a channel so they cut as one.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bridges {
    /// The channel width.
    pub width: Millimeters,
    /// The connections, applied in order.
    pub connections: Vec<Bridge>,
}

/// One bridge, between two picked points.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bridge {
    /// One end.
    pub first: Pick,
    /// The other end.
    pub second: Pick,
}

/// A point picked on a contour.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pick {
    /// The contour, as an index into the contours at the time of the pick.
    pub contour: usize,
    /// Point in the local coordinates of the result's lowest original
    /// placed owner; preparation resolves that owner through prior bridges.
    pub point: Point,
}

/// Where each contour starts and which way it runs.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Start {
    /// Where cutting starts.
    pub position: StartPosition,
    /// Which way the cut runs.
    pub direction: Direction,
    /// Starts chosen on the drawing, one per contour, over the position.
    #[serde(default)]
    pub spots: Vec<Spot>,
}

/// Where a contour's cut starts.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartPosition {
    /// Where the drawing starts it.
    #[default]
    Keep,
    /// Halfway along the longest segment of a closed contour.
    Automatic,
    /// At this fraction of the contour's length, as drawn.
    Manual(f64),
}

/// Which way a cut runs.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// As drawn.
    #[default]
    Keep,
    /// Clockwise around closed contours.
    Clockwise,
    /// Counterclockwise around closed contours.
    Counterclockwise,
    /// The opposite of how it was drawn.
    Reverse,
}

/// How a closed contour's seam, where the cut ends where it began, is cut.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seam {
    /// Cut exactly around.
    #[default]
    Seal,
    /// Stop this far short, leaving the part attached.
    Gap(Millimeters),
    /// Cut this far past the start again.
    Overcut(Millimeters),
}

/// The cutting order.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CutOrder {
    /// The base order.
    pub strategy: OrderStrategy,
    /// Cut holes before the outline around them.
    pub inner_first: bool,
    /// Cut circles before other shapes.
    pub circles_first: bool,
    /// Keep consecutive cuts apart to spread heat.
    pub spread_heat: bool,
}

/// The base cutting order.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStrategy {
    /// As drawn.
    #[default]
    AsDrawn,
    /// By position, left to right.
    LeftToRight,
    /// By position, right to left.
    RightToLeft,
    /// By position, bottom to top.
    BottomToTop,
    /// By position, top to bottom.
    TopToBottom,
    /// Each next contour the nearest to the last.
    Nearest,
    /// Drawing contours in this order.
    Manual(Vec<usize>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layer_takes_its_own_machining_and_keeps_the_jobs_picked_places() {
        let spot = Spot { contour: 3, fraction: 0.25 };
        let kerf = Kerf { width: Millimeters(0.2), side: Side::Auto };
        let job = Features {
            kerf: Some(kerf),
            start: Start { spots: vec![spot], ..Start::default() },
            layers: vec![
                Layer { name: "Mark".into(), mode: LayerMode::Mark, color: None, machining: None },
                Layer {
                    name: "Slots".into(),
                    mode: LayerMode::Cut,
                    color: None,
                    machining: Some(Machining {
                        seam: Seam::Overcut(Millimeters(1.)),
                        ..Machining::default()
                    }),
                },
            ],
            ..Features::default()
        };
        assert_eq!(*job.for_layer("0"), job, "a layer without an entry cuts as the job");
        let mark = job.for_layer("Mark");
        assert!(mark.kerf.is_none(), "a mark has no machining of its own, so none");
        assert_eq!(mark.start.spots, [spot]);
        let slots = job.for_layer("Slots");
        assert!(slots.kerf.is_none() && slots.seam == Seam::Overcut(Millimeters(1.)));
        assert_eq!(job.mode("Mark"), LayerMode::Mark);
        assert_eq!(job.mode("0"), LayerMode::Cut);
    }

    #[test]
    fn reusable_defaults_keep_settings_without_drawing_anchors() {
        let spot = Spot { contour: 3, fraction: 0.4 };
        let features = Features {
            cooling: Some(Cooling {
                dwell: Milliseconds(300),
                placement: CoolingPlacement::Manual(vec![spot]),
            }),
            bridges: Some(Bridges {
                width: Millimeters(0.5),
                connections: vec![Bridge {
                    first: Pick { contour: 0, point: Point::ORIGIN },
                    second: Pick { contour: 1, point: Point::new(1., 0.) },
                }],
            }),
            start: Start {
                position: StartPosition::Manual(0.5),
                direction: Direction::Clockwise,
                spots: vec![spot],
            },
            order: CutOrder {
                strategy: OrderStrategy::Manual(vec![1, 0]),
                inner_first: true,
                ..CutOrder::default()
            },
            skip_layers: vec!["construction".into()],
            ..Features::default()
        };
        let defaults = features.reusable();
        assert!(defaults.bridges.as_ref().unwrap().connections.is_empty());
        assert_eq!(defaults.bridges.unwrap().width, Millimeters(0.5));
        assert_eq!(
            defaults.cooling.unwrap(),
            Cooling { dwell: Milliseconds(300), placement: CoolingPlacement::Manual(vec![]) }
        );
        assert!(defaults.start.spots.is_empty() && defaults.skip_layers.is_empty());
        assert_eq!(defaults.start.direction, Direction::Clockwise);
        assert_eq!(defaults.start.position, StartPosition::Keep);
        assert_eq!(defaults.order.strategy, OrderStrategy::AsDrawn);
        assert!(defaults.order.inner_first);
        assert_eq!(features.start.spots, [spot], "the saved job keeps its anchors");
    }

    /// A job file round-trips, absent features read as off, and an older
    /// file without newer fields still loads.
    #[test]
    fn features_round_trip_and_default_to_off() {
        let features = Features {
            leads: Some(Leads {
                entry: Some(Lead {
                    shape: LeadShape::LineArc,
                    length: Millimeters(2.),
                    radius: Millimeters(1.),
                    angle: Degrees(90.),
                }),
                exit: None,
                side: Side::Auto,
                closed_only: true,
                clearance: None,
                overrides: Vec::new(),
            }),
            joints: Some(Joints {
                placement: JointPlacement::Count(2),
                width: Millimeters(0.4),
                minimum_size: Millimeters(40.),
                outer_only: true,
                open_start: false,
                behaviour: JointBehaviour::Power(Percent(20.)),
                slow_speed: None,
                repierce: false,
            }),
            cooling: Some(Cooling {
                dwell: Milliseconds(300),
                placement: CoolingPlacement::Automatic {
                    at_start: true,
                    corners_below: Some(Degrees(60.)),
                },
            }),
            kerf: Some(Kerf { width: Millimeters(0.2), side: Side::Auto }),
            bridges: None,
            common: None,
            fly: None,
            start: Start {
                position: StartPosition::Manual(0.25),
                direction: Direction::Clockwise,
                spots: vec![Spot { contour: 1, fraction: 0.5 }],
            },
            seam: Seam::Overcut(Millimeters(1.)),
            order: CutOrder {
                strategy: OrderStrategy::Nearest,
                inner_first: true,
                ..CutOrder::default()
            },
            skip_layers: vec!["NOTES".into()],
            layer_edits: vec![LayerEdit { contour: 2, layer: "Etch".into() }],
            fill_regions: vec![FillRegion { contour: 2, holes: vec![3] }],
            contour_machining: vec![ContourMachining {
                contours: vec![1],
                own: vec![MachiningPart::Joints],
                machining: Machining::default(),
            }],
            only_contours: Some(vec![2]),
            rasters: Vec::new(),
            layers: vec![Layer {
                name: "Etch".into(),
                mode: LayerMode::Mark,
                color: Some([255, 0, 0]),
                machining: Some(Machining::default()),
            }],
        };
        let json = serde_json::to_string(&features).unwrap();
        assert_eq!(serde_json::from_str::<Features>(&json).unwrap(), features);
        assert_eq!(serde_json::from_str::<Features>("{}").unwrap(), Features::default());
        let partial: Features =
            serde_json::from_str(r#"{"kerf":{"width":0.3,"side":"inside"}}"#).unwrap();
        assert_eq!(partial.kerf, Some(Kerf { width: Millimeters(0.3), side: Side::Inside }));
        assert_eq!(partial.order.strategy, OrderStrategy::AsDrawn);
    }

    /// Renumbering drops what lay on a contour that is gone and shifts the
    /// rest: without contour 1, its joint, its bridge and its place in the
    /// order go with it, and contour 2 becomes 1.
    #[test]
    fn renumbering_drops_the_gone_and_shifts_the_rest() {
        let spot = |contour| Spot { contour, fraction: 0.5 };
        let pick = |contour| Pick { contour, point: Point::ORIGIN };
        let features = Features {
            joints: Some(Joints {
                placement: JointPlacement::Manual(vec![spot(0), spot(1), spot(2)]),
                width: Millimeters(0.4),
                minimum_size: Millimeters(0.),
                outer_only: false,
                open_start: false,
                behaviour: JointBehaviour::LaserOff,
                slow_speed: None,
                repierce: false,
            }),
            bridges: Some(Bridges {
                width: Millimeters(1.),
                connections: vec![
                    Bridge { first: pick(0), second: pick(1) },
                    Bridge { first: pick(0), second: pick(2) },
                ],
            }),
            only_contours: Some(vec![0, 1, 2]),
            fill_regions: vec![
                FillRegion { contour: 0, holes: vec![1, 2] },
                FillRegion { contour: 1, holes: vec![] },
            ],
            start: Start { spots: vec![spot(2)], ..Start::default() },
            order: CutOrder {
                strategy: OrderStrategy::Manual(vec![2, 1, 0]),
                ..CutOrder::default()
            },
            ..Features::default()
        };
        let renumbered = features.renumbered_locations(|contour| match contour {
            0 => Some(0),
            2 => Some(1),
            _ => None,
        });
        let Some(JointPlacement::Manual(joints)) = renumbered.joints.map(|j| j.placement) else {
            panic!("the joints stay manual");
        };
        assert_eq!(joints.iter().map(|s| s.contour).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(renumbered.bridges, features.bridges);
        assert_eq!(renumbered.start.spots, vec![Spot { contour: 1, fraction: 0.5 }]);
        assert_eq!(renumbered.order.strategy, OrderStrategy::Manual(vec![1, 0]));
        assert_eq!(renumbered.fill_regions, vec![FillRegion { contour: 0, holes: vec![1] }]);
        assert!(features.reusable().fill_regions.is_empty());
        assert_eq!(renumbered.only_contours, Some(vec![0, 1]));
        assert!(features.reusable().only_contours.is_none());
    }
}
