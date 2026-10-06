// SPDX-License-Identifier: GPL-3.0-or-later

//! Scan offsets: how far an engraved line lands behind where it was meant
//! to, measured per laser on this machine for each scan angle and speed.
//! The axes can differ, so an entry belongs to an angle as well as a speed.
//!
//! A calibration coupon engraves one field per speed, scanned both ways at
//! the chosen angle with no compensation; the operator measures how far the
//! lines of the two directions are apart and enters the offset per field.
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
/// Fields per row of a coupon.
pub const COLUMNS: usize = 4;
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

/// Linear through the points, sorted by x; the nearest value beyond them.
fn interpolate(points: &[(f64, f64)], x: f64) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return 0. };
    if x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
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

/// A calibration coupon: one field per speed, `COLUMNS` to a row.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanCoupon {
    /// The engraving recipe the fields run with, at each test speed.
    pub recipe: Id,
    /// Scan angle in degrees.
    pub angle: f64,
    /// The speeds to test, in millimetres per second.
    pub speeds: Vec<f64>,
    /// Machine XY of the coupon's lower-left corner.
    pub origin: [f64; 2],
    /// Side of each square field, in millimetres.
    pub field: f64,
    /// Distance between scan lines, in millimetres.
    pub spacing: f64,
}

/// Lower-left corners of a coupon's fields: `COLUMNS` to a row, a gap of
/// half a field between them, rows upward from the origin.
#[must_use]
pub fn layout(origin: [f64; 2], field: f64, count: usize) -> Vec<[f64; 2]> {
    let pitch = field * 1.5;
    (0..count)
        .map(|i| {
            let (col, row) = (i % COLUMNS, i / COLUMNS);
            #[allow(clippy::cast_precision_loss, reason = "a few dozen fields")]
            [origin[0] + col as f64 * pitch, origin[1] + row as f64 * pitch]
        })
        .collect()
}

/// A layer name for a test speed.
fn layer_name(speed: f64) -> String {
    format!("{} mm/s", (speed * 100.).round() / 100.)
}

fn square(layer: String, at: [f64; 2], side: f64) -> Contour {
    let p = [
        Point::new(at[0], at[1]),
        Point::new(at[0] + side, at[1]),
        Point::new(at[0] + side, at[1] + side),
        Point::new(at[0], at[1] + side),
    ];
    Contour {
        layer,
        curves: (0..4).map(|i| Curve::Line { start: p[i], end: p[(i + 1) % 4] }).collect(),
    }
}

/// `recipe` running at `speed`.
fn at_speed(recipe: &Recipe, speed: f64) -> Recipe {
    let mut copy = recipe.clone();
    copy.attributes.insert("CutSpeed".into(), speed.to_string());
    copy
}

impl Coordinator {
    /// The scan offsets measured for one laser.
    pub fn scan_offsets_view(&self, mode: LaserMode) -> Result<ScanOffsetsView> {
        let mut entries = self.scan_offsets.entries(mode).clone();
        entries.sort_by(|a, b| a.angle.total_cmp(&b.angle).then(a.speed.total_cmp(&b.speed)));
        Ok(ScanOffsetsView {
            revision: self.scan_offsets.revision,
            mode,
            bed: crate::correction::bed(self)?,
            entries,
        })
    }

    /// Replaces one angle's measurements, writing before publishing.
    pub fn save_scan_offsets(&mut self, change: ScanOffsetsChange) -> Result<()> {
        if change.revision != self.scan_offsets.revision {
            return Err(Error::Refused("scan offsets changed; reopen this page".into()));
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
    pub fn scan_offset_coupon(&mut self, coupon: ScanCoupon) -> Result<()> {
        if coupon.speeds.is_empty() || coupon.speeds.len() > MAX_SPEEDS {
            return Err(Error::Request(format!("test 1 to {MAX_SPEEDS} speeds")));
        }
        if !(coupon.field.is_finite() && (5. ..=100.).contains(&coupon.field)) {
            return Err(Error::Request("a field is 5 to 100 mm".into()));
        }
        if !(coupon.spacing.is_finite() && (0.05..=5.).contains(&coupon.spacing)) {
            return Err(Error::Request("the line spacing is 0.05 to 5 mm".into()));
        }
        for &speed in &coupon.speeds {
            check(&ScanOffset { angle: 0., speed, offset: 0. })?;
        }
        let angle = folded(coupon.angle);
        let recipe = self
            .library
            .recipes()
            .find(|r| r.id == coupon.recipe)
            .cloned()
            .ok_or_else(|| Error::Request("choose an engraving recipe".into()))?;
        let bed = crate::correction::bed(self)?;
        let corners = layout(coupon.origin, coupon.field, coupon.speeds.len());
        let size = coupon.field;
        if corners.iter().any(|c| {
            c[0] < bed.min.x
                || c[1] < bed.min.y
                || c[0] + size > bed.max.x
                || c[1] + size > bed.max.y
        }) {
            return Err(Error::Request("the coupon does not fit on the bed there".into()));
        }

        let names: Vec<String> = coupon.speeds.iter().map(|&s| layer_name(s)).collect();
        let drawing = Drawing {
            contours: corners
                .iter()
                .zip(&names)
                .map(|(at, name)| square(name.clone(), *at, size))
                .collect(),
        };
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\"><title>{:?} scan offset calibration at {angle}°</title>",
            bed.width(),
            bed.height(),
            bed.min.x,
            bed.min.y,
            bed.width(),
            bed.height(),
            recipe.laser,
        );
        for at in &corners {
            write!(svg, "<rect x=\"{}\" y=\"{}\" width=\"{size}\" height=\"{size}\" fill=\"none\" stroke=\"black\"/>", at[0], at[1])
                .map_err(problem)?;
        }
        svg.push_str("</svg>");

        self.queue_draft();
        let part = self.library.add_part("Scan offset calibration.svg", svg.as_bytes(), drawing)?;
        let sources = self.library.job_drawing(std::slice::from_ref(&part.id))?;
        let mut draft = Draft::new(std::sync::Arc::new(sources));
        draft.calibration = true;
        let fill = Fill {
            spacing: coupon.spacing,
            angle,
            run_up: 3.,
            outline: false,
            bidirectional: true,
            scan_offset: 0.,
            passes: 1,
            pass_angle: 0.,
            pass_shift: 0.,
            calibrated: Vec::new(),
        };
        draft.current.features.layers = names
            .iter()
            .map(|name| Layer {
                name: name.clone(),
                mode: LayerMode::Mark,
                color: None,
                machining: Some(Machining { fill: Some(fill.clone()), ..Machining::default() }),
            })
            .collect();
        draft.current.layers = names
            .iter()
            .zip(&coupon.speeds)
            .map(|(name, &speed)| LayerChoice {
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
    /// speed, by angle. Calibration coupons engrave uncompensated.
    pub(crate) fn scan_tables(&self, draft: &Draft) -> BTreeMap<String, Vec<[f64; 2]>> {
        let mut tables = BTreeMap::new();
        if draft.calibration {
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
        // Beyond what was measured, the nearest measurement holds.
        assert!((table_at_speed(&entries, 50.)[0][1] - 0.02).abs() < 1e-12);
        assert!((table_at_speed(&entries, 900.)[1][1] - 0.09).abs() < 1e-12);
    }

    #[test]
    fn a_coupon_lays_fields_four_to_a_row_upward() {
        let corners = layout([10., 20.], 20., 6);
        assert_eq!(corners[0], [10., 20.]);
        assert_eq!(corners[3], [100., 20.]);
        assert_eq!(corners[4], [10., 50.]);
        assert_eq!(corners[5], [40., 50.]);
    }

    #[test]
    fn angles_fold_into_half_a_turn() {
        assert!((folded(270.) - 90.).abs() < 1e-12);
        assert!((folded(-45.) - 135.).abs() < 1e-12);
        assert!(folded(179.99).abs() < 1e-12);
    }
}
