// SPDX-License-Identifier: GPL-3.0-or-later

//! Scan offsets: how far an engraved line lands behind where it was meant
//! to, measured per laser on this machine for each scan angle and speed.
//! The axes can differ, so an entry belongs to an angle as well as a speed.
//!
//! A calibration coupon engraves, per speed, one block of four lines per direction
//! side by side, scanned both ways at the chosen angle with no
//! compensation; the operator measures how far the line ends of the two
//! directions are apart and enters it per speed.
//! Fills on marked layers then take the offset for their recipe's speed and
//! each pass's angle, interpolated between what was measured.

use crate::coordinator::Coordinator;
use crate::draft::Draft;
use crate::{Error, Result};
use openlaser_core::LaserMode;
use openlaser_core::features::{Fill, Layer, LayerMode, Machining};
use openlaser_core::geometry::{Bounds, Contour, Curve, Drawing, Point};
use openlaser_core::nesting::{NestSettings, NestStock, Nesting};
use openlaser_library::{Id, LayerChoice, Recipe};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

const FILE: &str = "scan_offsets.json";
/// Speeds per row of a coupon.
pub const COLUMNS: usize = 4;
/// Lines in each block, one above the other, scanned alternately both ways.
pub const LINES: usize = 4;
/// Blocks of lines side by side for each speed.
pub const BLOCKS: usize = 1;
/// Space between the tests of two speeds, in millimetres.
const GAP: f64 = 10.;
/// The most speeds one coupon tests.
const MAX_SPEEDS: usize = 24;
/// Angles closer than this, in degrees, are the same calibration.
const SAME_ANGLE: f64 = 0.05;

/// One measurement: at this scan angle and speed, lines land this far
/// behind where they were meant to.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanOffset {
    /// Scan direction in degrees, 0 to under 180.
    pub angle: f64,
    /// Engraving speed in millimetres per second.
    pub speed: f64,
    /// Offset in millimetres: half the shift between the two directions.
    pub offset: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    version: u32,
    revision: u64,
    fiber: Vec<ScanOffset>,
    co2: Vec<ScanOffset>,
}

impl Default for Store {
    fn default() -> Self {
        Self { version: 1, revision: 0, fiber: Vec::new(), co2: Vec::new() }
    }
}

fn problem(error: impl std::fmt::Display) -> Error {
    Error::Refused(format!("scan offsets: {error}"))
}

/// An angle in [0, 180): a scan line runs the same both ways round.
fn folded(angle: f64) -> f64 {
    let a = angle.rem_euclid(180.);
    if 180. - a < SAME_ANGLE { 0. } else { a }
}

fn check(entry: &ScanOffset) -> Result<()> {
    if !(entry.angle.is_finite() && (0. ..180.).contains(&entry.angle)) {
        return Err(Error::Request("a scan angle must be 0 to under 180°".into()));
    }
    if !(entry.speed.is_finite() && entry.speed > 0. && entry.speed <= 10_000.) {
        return Err(Error::Request("a speed must be above 0 and at most 10000 mm/s".into()));
    }
    if !(entry.offset.is_finite() && entry.offset.abs() <= 2.) {
        return Err(Error::Request("a scan offset must be within ±2 mm".into()));
    }
    Ok(())
}

impl Store {
    pub(crate) fn open(root: &Path) -> Result<Self> {
        let bytes = match std::fs::read(root.join(FILE)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(problem(e)),
        };
        let store: Self = serde_json::from_slice(&bytes).map_err(problem)?;
        if store.version != 1 {
            return Err(Error::Refused("unsupported scan offset file".into()));
        }
        for entry in store.fiber.iter().chain(&store.co2) {
            check(entry)?;
        }
        Ok(store)
    }

    fn entries(&self, mode: LaserMode) -> &Vec<ScanOffset> {
        match mode {
            LaserMode::Fiber => &self.fiber,
            LaserMode::Co2 => &self.co2,
        }
    }

    fn entries_mut(&mut self, mode: LaserMode) -> &mut Vec<ScanOffset> {
        match mode {
            LaserMode::Fiber => &mut self.fiber,
            LaserMode::Co2 => &mut self.co2,
        }
    }

    /// Per measured angle, the offset at `speed`: linear between the speeds
    /// measured at that angle, the nearest one outside them.
    pub(crate) fn at_speed(&self, mode: LaserMode, speed: f64) -> Vec<[f64; 2]> {
        table_at_speed(self.entries(mode), speed)
    }
}

/// See [`Store::at_speed`].
fn table_at_speed(entries: &[ScanOffset], speed: f64) -> Vec<[f64; 2]> {
    let mut angles: Vec<f64> = Vec::new();
    for e in entries {
        if !angles.iter().any(|a| (a - e.angle).abs() < SAME_ANGLE) {
            angles.push(e.angle);
        }
    }
    angles.sort_by(f64::total_cmp);
    angles
        .into_iter()
        .map(|angle| {
            let mut points: Vec<(f64, f64)> = entries
                .iter()
                .filter(|e| (e.angle - angle).abs() < SAME_ANGLE)
                .map(|e| (e.speed, e.offset))
                .collect();
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
            [angle, interpolate(&points, speed)]
        })
        .collect()
}

/// Linear through the points, sorted by speed. Beyond them the offset
/// keeps in proportion to the speed of the nearest measurement: it is the
/// laser's fixed delay times the speed, so half the speed, half the offset.
fn interpolate(points: &[(f64, f64)], x: f64) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return 0. };
    if x <= first.0 {
        return if first.0 > 0. { first.1 * x.max(0.) / first.0 } else { first.1 };
    }
    if x >= last.0 {
        return if last.0 > 0. { last.1 * x / last.0 } else { last.1 };
    }
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x <= x1 {
            return if x1 - x0 < 1e-12 { y1 } else { y0 + (y1 - y0) * (x - x0) / (x1 - x0) };
        }
    }
    last.1
}

/// The calibration for one laser, and where a coupon would go.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Serialize)]
pub struct ScanOffsetsView {
    /// Concurrency token for saving.
    pub revision: u64,
    /// The laser.
    pub mode: LaserMode,
    /// The machine's travel, for placing the coupon.
    pub bed: Bounds,
    /// Cutting acceleration from the active machine backup, in mm/s².
    pub acceleration: f64,
    /// Planner acceleration time in seconds.
    pub acceleration_time: f64,
    /// Every measurement, by angle then speed.
    pub entries: Vec<ScanOffset>,
}

/// The offsets measured at one angle, replacing what that angle had.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanOffsetsChange {
    /// The revision the form was opened at.
    pub revision: u64,
    /// The laser.
    pub mode: LaserMode,
    /// The scan angle measured.
    pub angle: f64,
    /// Speed and offset pairs; an empty offset leaves that speed out.
    pub measured: Vec<(f64, Option<f64>)>,
}

/// A calibration coupon: per speed, `BLOCKS` blocks of `LINES` lines side
/// by side; `COLUMNS` speeds to a row.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanCoupon {
    /// The engraving recipe the fields run with, at each test speed.
    pub recipe: Id,
    /// Scan angle in degrees.
    pub angle: f64,
    /// Multiple scan directions; empty uses the legacy single angle.
    #[serde(default)]
    pub angles: Vec<f64>,
    /// The speeds to test, in millimetres per second.
    pub speeds: Vec<f64>,
    /// Machine XY of the coupon's lower-left corner.
    pub origin: [f64; 2],
    /// Length of each line, in millimetres.
    pub length: f64,
    /// Distance between the lines of a block, in millimetres.
    pub spacing: f64,
    /// Engrave with the stored offsets, to check them: the lines of a block
    /// meet when the offsets are right. Otherwise uncompensated, to measure.
    #[serde(default)]
    pub compensated: bool,
}

/// One speed's blocks in their own frame, scanned along X: rectangles
/// `length` long and `LINES` spacings high, half a length apart.
#[allow(clippy::cast_precision_loss, reason = "a handful of blocks and lines")]
fn blocks(length: f64, spacing: f64) -> Vec<[[f64; 2]; 4]> {
    let height = LINES as f64 * spacing;
    (0..BLOCKS)
        .map(|i| {
            let x = i as f64 * length * 1.5;
            [[x, 0.], [x + length, 0.], [x + length, height], [x, height]]
        })
        .collect()
}

/// One speed's blocks turned to `angle` and moved so their lower-left
/// bound is at `corner`.
fn placed(corner: [f64; 2], length: f64, spacing: f64, angle: f64) -> Vec<[[f64; 2]; 4]> {
    let (sin, cos) = angle.to_radians().sin_cos();
    let turned: Vec<[[f64; 2]; 4]> = blocks(length, spacing)
        .into_iter()
        .map(|r| r.map(|[x, y]| [x * cos - y * sin, x * sin + y * cos]))
        .collect();
    let min_x = turned.iter().flatten().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let min_y = turned.iter().flatten().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    turned
        .into_iter()
        .map(|r| r.map(|[x, y]| [corner[0] + x - min_x, corner[1] + y - min_y]))
        .collect()
}

/// The bounds one speed's test takes on the bed, along X and Y.
#[must_use]
pub fn group_size(length: f64, spacing: f64, angle: f64) -> [f64; 2] {
    let rects = placed([0., 0.], length, spacing, angle);
    let max = |i: usize| rects.iter().flatten().map(|p| p[i]).fold(0., f64::max);
    [max(0), max(1)]
}

/// Lower-left corners of each speed's test: `COLUMNS` to a row, rows
/// upward from the origin, `GAP` apart.
#[must_use]
pub fn layout(origin: [f64; 2], size: [f64; 2], count: usize) -> Vec<[f64; 2]> {
    (0..count)
        .map(|i| {
            let (col, row) = (i % COLUMNS, i / COLUMNS);
            #[allow(clippy::cast_precision_loss, reason = "a few dozen speeds")]
            [origin[0] + col as f64 * (size[0] + GAP), origin[1] + row as f64 * (size[1] + GAP)]
        })
        .collect()
}

/// How long the head runs at full speed, laser off, before a line marks:
/// a gantry still sways from the turn when it reaches speed, and on a
/// diagonal the two axes sway apart and the line waves.
pub const SETTLE_SECONDS: f64 = 0.25;

/// Estimated laser-off run-up distance, capped at the fill limit.
fn run_up(speed: f64, acceleration: f64, time: f64) -> f64 {
    // Same jerk-limited ramp timing as the vendor planner, plus 50% and
    // 5 mm reserve, then the settling run. Never shorten a ramp silently
    // to fit the bed.
    let time = time.max(0.06);
    let seconds =
        if speed <= acceleration * time * 0.5 { time } else { speed / acceleration + time * 0.5 };
    0.75 * speed * seconds + speed * SETTLE_SECONDS + 5.
}

/// A layer name for a test speed.
fn layer_name(speed: f64) -> String {
    format!("{speed} mm/s")
}

fn outline(layer: String, corners: [[f64; 2]; 4]) -> Contour {
    let p = corners.map(|[x, y]| Point::new(x, y));
    Contour {
        layer,
        curves: (0..4).map(|i| Curve::Line { start: p[i], end: p[(i + 1) % 4] }).collect(),
    }
}

/// `recipe` running at `speed`.
fn at_speed(recipe: &Recipe, speed: f64) -> Recipe {
    let mut copy = recipe.clone();
    copy.attributes.insert("CutSpeed".into(), speed.to_string());
    for key in ["SlowStart", "UD_UpEnable", "UD_DownEnable"] {
        copy.attributes.insert(key.into(), "0".into());
    }
    copy
}

impl Coordinator {
    fn scan_motion(&self) -> Result<(f64, f64)> {
        let bundle = self
            .bundle
            .as_ref()
            .ok_or_else(|| Error::Refused("import machine settings first".into()))?;
        let mc = bundle.group("ManuParam", "MC").map_err(problem)?;
        let read = |key: &str| -> Result<f64> {
            mc.get(key)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v > 0.)
                .ok_or_else(|| Error::Request(format!("invalid machine motion setting {key}")))
        };
        let mut acceleration = read("ManuAcc")?;
        let mut time = read("AccTime")? * 0.001;
        for axis in 0..2 {
            let group = format!("MachineAxisConfig_{axis}");
            let tag = if axis == 0 { "MAC".to_owned() } else { format!("MAC_{axis}") };
            let values = bundle.group(&group, &tag).map_err(problem)?;
            let value = |key: &str| {
                values
                    .get(key)
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0.)
            };
            // Some legacy backups omit these axis words; ManuParam remains
            // the planner's authority. Present axis values add reserve.
            if let Some(a) = value("Acceleration") {
                acceleration = acceleration.min(a);
            }
            if let Some(t) = value("AccelerationTime") {
                time = time.max(t * 0.001);
            }
        }
        Ok((acceleration, time.max(0.06)))
    }

    pub(crate) fn scan_runups(&self, draft: &Draft) -> Result<BTreeMap<String, f64>> {
        let mut ramps = BTreeMap::new();
        if !draft
            .current
            .features
            .layers
            .iter()
            .any(|l| l.machining.as_ref().is_some_and(|m| m.fill.is_some()))
        {
            return Ok(ramps);
        }
        let (acceleration, time) = self.scan_motion()?;
        for layer in &draft.current.features.layers {
            if layer.mode != LayerMode::Mark
                || layer.machining.as_ref().is_none_or(|m| m.fill.is_none())
            {
                continue;
            }
            let recipe = draft
                .current
                .layers
                .iter()
                .find(|c| c.layer == layer.name)
                .and_then(|c| c.recipe.as_ref())
                .or(draft.current.recipe.as_ref());
            if let Some(recipe) = recipe
                && let Some(speed) = recipe
                    .attributes
                    .get("CutSpeed")
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v > 0.)
            {
                let slow = ["SlowStartLength", "UD_UpLen", "UD_DownLen"]
                    .iter()
                    .filter_map(|k| recipe.attributes.get(*k)?.parse::<f64>().ok())
                    .filter(|v| v.is_finite() && *v >= 0.)
                    .fold(0., f64::max);
                let (acceleration, time) =
                    crate::draft::engraving_motion(recipe, acceleration, time);
                ramps.insert(layer.name.clone(), run_up(speed, acceleration, time).max(slow + 5.));
            }
        }
        Ok(ramps)
    }

    /// The scan offsets measured for one laser.
    pub fn scan_offsets_view(&self, mode: LaserMode) -> Result<ScanOffsetsView> {
        let mut entries = self.scan_offsets.entries(mode).clone();
        entries.sort_by(|a, b| a.angle.total_cmp(&b.angle).then(a.speed.total_cmp(&b.speed)));
        // The coupon preview plans with how an engraving moves by default.
        let (acceleration, acceleration_time) = self.scan_motion()?;
        let acceleration = acceleration.min(crate::draft::ENGRAVING_ACCELERATION);
        let acceleration_time = acceleration_time.max(crate::draft::ENGRAVING_ACCELERATION_TIME);
        Ok(ScanOffsetsView {
            revision: self.scan_offsets.revision,
            mode,
            acceleration,
            acceleration_time,
            bed: crate::correction::bed(self)?,
            entries,
        })
    }

    /// Replaces one angle's measurements, writing before publishing.
    pub fn save_scan_offsets(&mut self, change: ScanOffsetsChange) -> Result<()> {
        if change.revision != self.scan_offsets.revision {
            return Err(Error::Refused("scan offsets changed; reopen this page".into()));
        }
        if !change.angle.is_finite() {
            return Err(Error::Request("a scan angle must be finite".into()));
        }
        let angle = folded(change.angle);
        let mut next = self.scan_offsets.clone();
        let entries = next.entries_mut(change.mode);
        entries.retain(|e| (e.angle - angle).abs() >= SAME_ANGLE);
        for (speed, offset) in change.measured {
            let Some(offset) = offset else { continue };
            let entry = ScanOffset { angle, speed, offset };
            check(&entry)?;
            entries.retain(|e| {
                !((e.angle - angle).abs() < SAME_ANGLE && (e.speed - speed).abs() < 1e-6)
            });
            entries.push(entry);
        }
        next.revision += 1;
        openlaser_library::atomic_write(
            &self.config.data_dir.join(FILE),
            &serde_json::to_vec_pretty(&next).map_err(problem)?,
        )?;
        self.scan_offsets = next;
        self.publish();
        self.reprepare();
        Ok(())
    }

    /// Loads a coupon into Setup: one marked field per speed, each on its own
    /// layer with the recipe at that speed, scanned both ways at the angle
    /// and engraved without compensation.
    #[allow(
        clippy::too_many_lines,
        clippy::needless_pass_by_value,
        reason = "one coupon is laid out, checked and staged in one place; the API hands it over"
    )]
    pub fn scan_offset_coupon(&mut self, coupon: ScanCoupon) -> Result<()> {
        if coupon.speeds.is_empty() || coupon.speeds.len() > MAX_SPEEDS {
            return Err(Error::Request(format!("test 1 to {MAX_SPEEDS} speeds")));
        }
        if !(coupon.length.is_finite() && (3. ..=50.).contains(&coupon.length)) {
            return Err(Error::Request("a line is 3 to 50 mm long".into()));
        }
        if !(coupon.spacing.is_finite() && (0.3..=5.).contains(&coupon.spacing)) {
            return Err(Error::Request("the line spacing is 0.3 to 5 mm".into()));
        }
        for &speed in &coupon.speeds {
            check(&ScanOffset { angle: 0., speed, offset: 0. })?;
        }
        if !coupon.angle.is_finite() || coupon.origin.iter().any(|v| !v.is_finite()) {
            return Err(Error::Request("the coupon angle and origin must be finite".into()));
        }
        for (i, speed) in coupon.speeds.iter().enumerate() {
            if coupon.speeds[..i].iter().any(|other| (other - speed).abs() < 1e-6) {
                return Err(Error::Request("test each speed only once".into()));
            }
        }
        let angles: Vec<f64> = if coupon.angles.is_empty() {
            vec![folded(coupon.angle)]
        } else {
            if coupon.angles.len() > 8 || coupon.angles.iter().any(|a| !a.is_finite()) {
                return Err(Error::Request("test 1 to 8 finite scan angles".into()));
            }
            coupon.angles.iter().copied().map(folded).collect()
        };
        for (i, angle) in angles.iter().enumerate() {
            if angles[..i].iter().any(|other| (other - angle).abs() < SAME_ANGLE) {
                return Err(Error::Request("test each scan angle only once".into()));
            }
        }
        let recipe = self
            .library
            .recipes()
            .find(|r| r.id == coupon.recipe)
            .cloned()
            .ok_or_else(|| Error::Request("choose an engraving recipe".into()))?;
        if Some(recipe.laser) != self.mode
            || recipe.attributes.get("OpenLaserProcess").is_none_or(|v| v != "engrave")
        {
            return Err(Error::Request(
                "calibration needs an engraving recipe for the active laser".into(),
            ));
        }
        let bed = crate::correction::bed(self)?;
        let (acceleration, time) = self.scan_motion()?;
        let (acceleration, time) = crate::draft::engraving_motion(&recipe, acceleration, time);
        if coupon.speeds.iter().any(|&s| run_up(s, acceleration, time) > 1000.) {
            return Err(Error::Request(
                "the required scan run-up exceeds 1000 mm; lower the test speed".into(),
            ));
        }
        // Separate engraved blocks; full laser-off ramps may overlap other blocks.
        let tests: Vec<(f64, f64)> = coupon
            .speeds
            .iter()
            .flat_map(|&speed| angles.iter().map(move |&angle| (speed, angle)))
            .collect();
        let sizes: Vec<[f64; 2]> = tests
            .iter()
            .map(|&(_, angle)| group_size(coupon.length, coupon.spacing, angle))
            .collect();
        let padding: Vec<[f64; 2]> = tests
            .iter()
            .map(|&(speed, angle)| {
                let (sin, cos) = angle.to_radians().sin_cos();
                [
                    run_up(speed, acceleration, time) * cos.abs(),
                    run_up(speed, acceleration, time) * sin.abs(),
                ]
            })
            .collect();
        let cell = [0, 1].map(|axis| sizes.iter().map(|size| size[axis]).fold(0., f64::max));
        let columns = if angles.len() > 1 { angles.len() } else { COLUMNS };
        let corners: Vec<[f64; 2]> = (0..tests.len())
            .map(|i| {
                #[allow(clippy::cast_precision_loss, reason = "a small calibration grid")]
                [
                    coupon.origin[0] + (i % columns) as f64 * (cell[0] + GAP),
                    coupon.origin[1] + (i / columns) as f64 * (cell[1] + GAP),
                ]
            })
            .collect();
        if corners.iter().zip(&sizes).zip(&padding).any(|((c, size), p)| {
            c[0] - p[0] < bed.min.x - 1e-9
                || c[1] - p[1] < bed.min.y - 1e-9
                || c[0] + size[0] + p[0] > bed.max.x + 1e-9
                || c[1] + size[1] + p[1] > bed.max.y + 1e-9
        }) {
            return Err(Error::Request(
                "the coupon including its laser-off run-up does not fit on the bed there".into(),
            ));
        }
        let groups: Vec<Vec<[[f64; 2]; 4]>> = corners
            .iter()
            .zip(&tests)
            .map(|(&c, &(_, angle))| placed(c, coupon.length, coupon.spacing, angle))
            .collect();
        let names: Vec<String> = tests
            .iter()
            .map(|&(speed, angle)| {
                if angles.len() == 1 {
                    layer_name(speed)
                } else {
                    format!("{} · {angle}°", layer_name(speed))
                }
            })
            .collect();
        let drawing = Drawing {
            contours: groups
                .iter()
                .zip(&names)
                .flat_map(|(rects, name)| rects.iter().map(|r| outline(name.clone(), *r)))
                .collect(),
        };
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\"><title>{:?} scan offset calibration at {angles:?}°</title>",
            bed.width(),
            bed.height(),
            bed.min.x,
            bed.min.y,
            bed.width(),
            bed.height(),
            recipe.laser,
        );
        for r in groups.iter().flatten() {
            let points: Vec<String> = r.iter().map(|[x, y]| format!("{x},{y}")).collect();
            write!(
                svg,
                "<polygon points=\"{}\" fill=\"none\" stroke=\"black\"/>",
                points.join(" ")
            )
            .map_err(problem)?;
        }
        svg.push_str("</svg>");

        self.queue_draft();
        let part = self.library.add_part("Scan offset calibration.svg", svg.as_bytes(), drawing)?;
        let sources = self.library.job_drawing(std::slice::from_ref(&part.id))?;
        let mut draft = Draft::new(std::sync::Arc::new(sources));
        draft.calibration = true;
        draft.scan_check = coupon.compensated;
        let fill = |speed: f64, angle: f64| Fill {
            spacing: coupon.spacing,
            angle,
            run_up: run_up(speed, acceleration, time),
            outline: false,
            outline_holes: None,
            bidirectional: true,
            scan_offset: 0.,
            per_object: None,
            passes: 1,
            pass_angle: 0.,
            pass_shift: 0.,
            calibrated: Vec::new(),
            image: None,
        };
        draft.current.features.layers = names
            .iter()
            .zip(&tests)
            .map(|(name, &(speed, angle))| Layer {
                name: name.clone(),
                mode: LayerMode::Mark,
                color: None,
                machining: Some(Machining {
                    fill: Some(fill(speed, angle)),
                    ..Machining::default()
                }),
            })
            .collect();
        draft.current.layers = names
            .iter()
            .zip(&tests)
            .map(|(name, &(speed, _))| LayerChoice {
                layer: name.clone(),
                recipe: Some(at_speed(&recipe, speed)),
            })
            .collect();
        draft.current.recipe = Some(recipe);
        draft.current.placement =
            Some(openlaser_library::placement::Placement::Fixed { origin: bed.min.into() });
        draft.current.sheet_offset = Some([0., 0.]);
        draft.current.nesting = Some(Nesting {
            stock: NestStock::Rectangle { bounds: bed },
            settings: NestSettings::default(),
        });
        self.draft_generation += 1;
        self.draft = Some(draft);
        self.library_changed();
        self.reprepare();
        Ok(())
    }

    /// Per marked layer with a fill, the offsets measured at its recipe's
    /// speed, by angle. Calibration coupons engrave uncompensated, unless
    /// they check the stored offsets.
    pub(crate) fn scan_tables(&self, draft: &Draft) -> BTreeMap<String, Vec<[f64; 2]>> {
        let mut tables = BTreeMap::new();
        if draft.calibration && !draft.scan_check {
            return tables;
        }
        let Some(job) = draft.current.recipe.as_ref() else { return tables };
        for layer in &draft.current.features.layers {
            let filled = layer.mode == LayerMode::Mark
                && layer.machining.as_ref().is_some_and(|m| m.fill.is_some());
            if !filled {
                continue;
            }
            let recipe = draft
                .current
                .layers
                .iter()
                .find(|c| c.layer == layer.name)
                .and_then(|c| c.recipe.as_ref())
                .unwrap_or(job);
            let Some(speed) = recipe.attributes.get("CutSpeed").and_then(|s| s.parse::<f64>().ok())
            else {
                continue;
            };
            let table = self.scan_offsets.at_speed(recipe.laser, speed);
            if !table.is_empty() {
                tables.insert(layer.name.clone(), table);
            }
        }
        tables
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(angle: f64, speed: f64, offset: f64) -> ScanOffset {
        ScanOffset { angle, speed, offset }
    }

    #[test]
    fn offsets_interpolate_by_speed_within_each_angle() {
        let entries = vec![
            entry(0., 100., 0.02),
            entry(0., 300., 0.06),
            entry(90., 100., 0.05),
            entry(90., 300., 0.09),
        ];
        let table = table_at_speed(&entries, 200.);
        assert_eq!(table.len(), 2);
        assert!((table[0][0] - 0.).abs() < 1e-12 && (table[0][1] - 0.04).abs() < 1e-12);
        assert!((table[1][0] - 90.).abs() < 1e-12 && (table[1][1] - 0.07).abs() < 1e-12);
        // Beyond what was measured, in proportion to the speed: a fixed delay.
        assert!((table_at_speed(&entries, 50.)[0][1] - 0.01).abs() < 1e-12);
        assert!((table_at_speed(&entries, 900.)[1][1] - 0.27).abs() < 1e-12);
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "positions are exact sums of whole millimetres")]
    fn a_coupon_lays_speeds_four_to_a_row_upward() {
        // One block of four 10 mm lines, 1 mm apart.
        let size = group_size(10., 1., 0.);
        assert!((size[0] - 10.).abs() < 1e-9 && (size[1] - 4.).abs() < 1e-9);
        let corners = layout([10., 20.], size, 6);
        assert_eq!(corners[0], [10., 20.]);
        assert_eq!(corners[3], [70., 20.]);
        assert_eq!(corners[4], [10., 34.]);
        assert_eq!(corners[5], [30., 34.]);
    }

    #[test]
    fn a_turned_test_stands_on_its_corner() {
        // Scanned along Y, the blocks stand one above the other.
        let size = group_size(10., 1., 90.);
        assert!((size[0] - 4.).abs() < 1e-9 && (size[1] - 10.).abs() < 1e-9);
        let rects = placed([5., 5.], 10., 1., 90.);
        let min = |i: usize| rects.iter().flatten().map(|p| p[i]).fold(f64::INFINITY, f64::min);
        assert!((min(0) - 5.).abs() < 1e-9 && (min(1) - 5.).abs() < 1e-9);
    }

    #[test]
    fn faster_lines_run_up_further() {
        assert!((run_up(100., 6000., 0.25) - 48.75).abs() < 1e-9);
        assert!(run_up(500., 6000., 0.25) > run_up(100., 6000., 0.25));
        assert!(run_up(5000., 6000., 0.25) > 1000.);
        assert!(run_up(200., 1000., 0.25) > run_up(200., 6000., 0.25));
    }

    #[test]
    fn angles_fold_into_half_a_turn() {
        assert!((folded(270.) - 90.).abs() < 1e-12);
        assert!((folded(-45.) - 135.).abs() < 1e-12);
        assert!(folded(179.99).abs() < 1e-12);
    }
}
