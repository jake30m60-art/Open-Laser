// SPDX-License-Identifier: GPL-3.0-or-later

//! Measuring a sheet by its edges, as the vendor's capacitive edge search
//! (capture of 2026-10-14): from where the head stands, one touch per point,
//! the head staying low between them and travelling on at 500 mm/s, then the sheet's corner and angle, or a
//! disc's centre and diameter, worked out from the edges found.
//!
//! - A rectangle, four points: two on the front edge (searching −Y) from the
//!   start and `spacing` further along X, two on the left edge (−X) from the
//!   start and `spacing` further along Y.
//! - A rectangle by two edges, three points: the front edge as above and one
//!   point on the left edge, which is taken square to the front one. For a
//!   sheet that reaches past the bed.
//! - A disc, three points: −X and +X from the start, then −Y from the middle
//!   of that chord; the circle through them.
//! - A bore of a given diameter, three points: the head stands roughly over
//!   its middle; each touch starts on the material beside the bore (left,
//!   right, front) and searches toward its middle; the circle through them.

use crate::coordinator::Shared;
use crate::{Error, Result};
use openlaser_controller::operations::edge::Touch;
use serde::{Deserialize, Serialize};

/// The search speed of the capture, in millimetres per second.
const SEARCH_MM_S: f64 = 50.;
/// How far outside a bore's nominal edge its touches start, millimetres.
const BORE_MARGIN: f64 = 10.;
/// The travel between touches in the capture, millimetres per second.
const TRAVEL_MM_S: f64 = 500.;
/// How far the found edges may lean before the measurement is refused.
const MAX_ANGLE_DEG: f64 = 15.;
/// How far from square the two measured edges of a rectangle may be.
const SQUARE_TOLERANCE_DEG: f64 = 2.;

/// What is measured.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetShape {
    /// A rectangle, four points.
    Rectangle,
    /// A rectangle by its front edge and one point on its left edge.
    TwoEdges,
    /// A disc, three points.
    Disc,
    /// A bore from outside, three points; its diameter is given.
    Bore,
}

/// A measurement as the operator asks for it.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct MeasureRequest {
    /// What is measured.
    pub shape: SheetShape,
    /// How far apart the two points on one edge are, in millimetres.
    pub spacing_mm: f64,
    /// How far each touch searches, in millimetres.
    pub search_mm: f64,
    /// The edge correction X/Y, millimetres: the capacitive head reports an
    /// edge only once it is past it, so each found point moves back by this
    /// much, against its search direction (the vendor's 5/5).
    #[serde(default)]
    pub correction_mm: [f64; 2],
    /// A bore's nominal diameter, millimetres.
    #[serde(default)]
    pub bore_mm: Option<f64>,
}

/// What a measurement found, in machine millimetres.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Measured {
    /// What was measured.
    pub shape: SheetShape,
    /// The edge points found, in touch order.
    pub points: Vec<[f64; 2]>,
    /// A rectangle's front-left corner.
    pub corner: Option<[f64; 2]>,
    /// A rectangle's angle: its front edge against the X axis, degrees,
    /// counter-clockwise positive.
    pub angle_deg: Option<f64>,
    /// How far from square the two edges of a four-point rectangle are, degrees.
    pub squareness_deg: Option<f64>,
    /// A disc's centre.
    pub center: Option<[f64; 2]>,
    /// A disc's diameter.
    pub diameter_mm: Option<f64>,
}

/// Measures the sheet from where the head stands.
pub async fn measure(shared: &Shared, request: MeasureRequest) -> Result<Measured> {
    if !(request.spacing_mm.is_finite() && (5. ..=3000.).contains(&request.spacing_mm)) {
        return Err(Error::Request("the point spacing must be 5 to 3000 mm".into()));
    }
    if !(request.search_mm.is_finite() && (5. ..=3000.).contains(&request.search_mm)) {
        return Err(Error::Request("the search travel must be 5 to 3000 mm".into()));
    }
    if !request.correction_mm.iter().all(|v| v.is_finite() && (-20. ..=20.).contains(v)) {
        return Err(Error::Request("the edge correction must be within ±20 mm".into()));
    }
    let [cx, cy] = request.correction_mm;
    let epoch = shared.ensure_running()?;
    let start = head_xy(shared).await?;
    let spacing = request.spacing_mm;
    let mut points = Vec::new();
    let at = |dx: f64, dy: f64| [start[0] + dx, start[1] + dy];
    let measured = match request.shape {
        SheetShape::Rectangle | SheetShape::TwoEdges => {
            let four = request.shape == SheetShape::Rectangle;
            let search = request.search_mm;
            points.push(touch(shared, epoch, 1, false, search, None, false).await?);
            points
                .push(touch(shared, epoch, 1, false, search, Some(at(spacing, 0.)), false).await?);
            points.push(touch(shared, epoch, 0, false, search, Some(at(0., 0.)), !four).await?);
            if four {
                points.push(
                    touch(shared, epoch, 0, false, search, Some(at(0., spacing)), true).await?,
                );
            }
            go(shared, epoch, at(0., 0.)).await?;
            for (i, p) in points.iter_mut().enumerate() {
                p[usize::from(i >= 2)] += if i >= 2 { cx } else { cy };
            }
            rectangle(request.shape, points)?
        }
        SheetShape::Disc => {
            let search = request.search_mm;
            points.push(touch(shared, epoch, 0, false, search, None, false).await?);
            points.push(touch(shared, epoch, 0, true, search, Some(at(0., 0.)), false).await?);
            let middle = f64::midpoint(points[0][0], points[1][0]);
            points.push(
                touch(shared, epoch, 1, false, search, Some([middle, start[1]]), true).await?,
            );
            points[0][0] += cx;
            points[1][0] -= cx;
            points[2][1] += cy;
            disc(points)?
        }
        SheetShape::Bore => {
            let diameter =
                request
                    .bore_mm
                    .filter(|d| d.is_finite() && (5. ..=1000.).contains(d))
                    .ok_or_else(|| Error::Request("give the bore diameter, 5 to 1000 mm".into()))?;
            // Each start lies BORE_MARGIN outside the nominal edge, on the
            // material; the search reaches past the middle at most.
            let out = diameter / 2. + BORE_MARGIN;
            let reach = out + BORE_MARGIN;
            points.push(touch(shared, epoch, 0, true, reach, Some(at(-out, 0.)), false).await?);
            points.push(touch(shared, epoch, 0, false, reach, Some(at(out, 0.)), false).await?);
            points.push(touch(shared, epoch, 1, true, reach, Some(at(0., -out)), true).await?);
            points[0][0] -= cx;
            points[1][0] += cx;
            points[2][1] -= cy;
            let mut measured = disc(points)?;
            measured.shape = SheetShape::Bore;
            let found = measured.diameter_mm.unwrap_or(0.);
            if (found - diameter).abs() > diameter.mul_add(0.1, 2.) {
                return Err(Error::Refused(format!(
                    "the bore measured Ø {found:.1} mm, not Ø {diameter:.1} mm; check the diameter and where the head stood"
                )));
            }
            go(shared, epoch, measured.center.unwrap_or(start)).await?;
            measured
        }
    };
    tracing::info!(correction = ?request.correction_mm, points = ?measured.points, "edge search measured");
    shared.lock().await.note(
        match &measured {
            Measured { corner: Some(c), angle_deg: Some(a), .. } => {
                format!("Sheet measured: corner X {:.2} · Y {:.2} mm, angle {a:.3}°", c[0], c[1])
            }
            Measured { shape: SheetShape::Bore, center: Some(c), diameter_mm: Some(d), .. } => {
                format!("Bore measured: centre X {:.2} · Y {:.2} mm, Ø {d:.2} mm", c[0], c[1])
            }
            Measured { center: Some(c), diameter_mm: Some(d), .. } => {
                format!("Disc measured: centre X {:.2} · Y {:.2} mm, Ø {d:.2} mm", c[0], c[1])
            }
            _ => "Sheet measured".into(),
        },
        false,
    );
    Ok(measured)
}

/// The head's XY in machine millimetres, from fresh feedback.
async fn head_xy(shared: &Shared) -> Result<[f64; 2]> {
    let c = shared.lock().await;
    c.idle()?;
    let state = c.machine.state();
    if !state.session.homed {
        return Err(Error::Refused("home the machine first".into()));
    }
    let feedback = state
        .feedback
        .filter(|f| f.age_ms <= crate::coordinator::FRESH_FEEDBACK_MS)
        .ok_or_else(|| Error::Refused("no machine feedback".into()))?;
    Ok([feedback.position_mm[0], feedback.position_mm[1]])
}

/// Stop ends the measurement between its steps.
fn still_running(shared: &Shared, epoch: u64) -> Result<()> {
    if shared.ensure_running()? == epoch {
        Ok(())
    } else {
        Err(Error::Refused("the measurement was stopped".into()))
    }
}

/// To the next start, the head lifted.
async fn go(shared: &Shared, epoch: u64, target: [f64; 2]) -> Result<()> {
    still_running(shared, epoch)?;
    crate::machine::move_to(shared, target).await
}

/// One touch along `axis`, the positive or negative way, starting `from`
/// (mm, or where the head stands); the edge in mm. Only the last one lifts.
#[allow(clippy::too_many_arguments, reason = "the plan of one touch")]
async fn touch(
    shared: &Shared,
    epoch: u64,
    axis: usize,
    positive: bool,
    search_mm: f64,
    from: Option<[f64; 2]>,
    lift: bool,
) -> Result<[f64; 2]> {
    still_running(shared, epoch)?;
    let (touch, scale) = {
        let c = shared.lock().await;
        c.idle()?;
        let bound = c.bound()?;
        let scale = c.scale()?;
        let raise = bound.shutdown.raise.ok_or_else(|| {
            Error::Refused("this machine has no head controller to search with".into())
        })?;
        if let Some(at) = from {
            let extent = bound.jog.extent;
            if !(0..2).all(|i| at[i].is_finite() && at[i] >= extent[i][0] && at[i] <= extent[i][1])
            {
                return Err(Error::Refused("a measuring point lies outside the bed".into()));
            }
        }
        let position = from.unwrap_or(head_xy_locked(&c)?)[axis];
        // Never past the bed: the search ends at its edge at the latest.
        let [min, max] = bound.jog.extent[axis];
        let room = if positive { max - position } else { position - min };
        let reach = search_mm.min(room);
        if reach < 1. {
            return Err(Error::Refused(
                "the head is at the edge of the bed; move it over the sheet".into(),
            ));
        }
        let travel =
            openlaser_xml::bindings::coordinate(if positive { reach } else { -reach }, scale)?;
        let touch = Touch {
            axis,
            travel,
            speed: bound.jog.speed_word(SEARCH_MM_S, scale)?,
            acceleration: bound.jog.position_acceleration[1],
            lift_thousandths: raise.height_thousandths,
            lift_speed_tenths: raise.speed_tenths,
            from: from
                .map(|at| -> Result<[i32; 2]> {
                    Ok([
                        openlaser_xml::bindings::coordinate(at[0], scale)?,
                        openlaser_xml::bindings::coordinate(at[1], scale)?,
                    ])
                })
                .transpose()?,
            travel_speed: bound.jog.speed_word(TRAVEL_MM_S.min(bound.jog.speed[1]), scale)?,
            lift,
        };
        (touch, scale)
    };
    let edge = shared.machine.probe_edge(touch).await?;
    Ok(edge.map(|v| f64::from(v) / f64::from(scale)))
}

fn head_xy_locked(c: &crate::coordinator::Coordinator) -> Result<[f64; 2]> {
    let feedback =
        c.machine.state().feedback.ok_or_else(|| Error::Refused("no machine feedback".into()))?;
    Ok([feedback.position_mm[0], feedback.position_mm[1]])
}

/// The corner and angle of a rectangle from its edge points.
fn rectangle(shape: SheetShape, points: Vec<[f64; 2]>) -> Result<Measured> {
    let [p1, p2, p3] = [points[0], points[1], points[2]];
    let front =
        unit(sub(p2, p1)).ok_or_else(|| Error::Refused("the two front points coincide".into()))?;
    let angle = front[1].atan2(front[0]).to_degrees();
    if angle.abs() > MAX_ANGLE_DEG {
        return Err(Error::Refused(format!(
            "the front edge leans {angle:.1}°; lay the sheet straighter"
        )));
    }
    let (left, squareness) = match points.get(3) {
        Some(&p4) => {
            let left = unit(sub(p4, p3))
                .ok_or_else(|| Error::Refused("the two left points coincide".into()))?;
            let off = (front[0] * left[0] + front[1] * left[1]).asin().to_degrees();
            if off.abs() > SQUARE_TOLERANCE_DEG {
                return Err(Error::Refused(format!(
                    "the edges are {off:.1}° off square; check the sheet and the points"
                )));
            }
            (left, Some(off))
        }
        None => ([-front[1], front[0]], None),
    };
    let corner = intersect(p1, front, p3, left)
        .ok_or_else(|| Error::Refused("the edges do not meet".into()))?;
    Ok(Measured {
        shape,
        points,
        corner: Some(corner),
        angle_deg: Some(angle),
        squareness_deg: squareness,
        center: None,
        diameter_mm: None,
    })
}

/// The centre and diameter of the circle through three edge points.
fn disc(points: Vec<[f64; 2]>) -> Result<Measured> {
    let [p1, p2, p3] = [points[0], points[1], points[2]];
    let det = 2. * (p1[0] * (p2[1] - p3[1]) + p2[0] * (p3[1] - p1[1]) + p3[0] * (p1[1] - p2[1]));
    if det.abs() < 1e-9 {
        return Err(Error::Refused("the three points lie on a line; no disc there".into()));
    }
    let sq = |p: [f64; 2]| p[0] * p[0] + p[1] * p[1];
    let cx = (sq(p1) * (p2[1] - p3[1]) + sq(p2) * (p3[1] - p1[1]) + sq(p3) * (p1[1] - p2[1])) / det;
    let cy = (sq(p1) * (p3[0] - p2[0]) + sq(p2) * (p1[0] - p3[0]) + sq(p3) * (p2[0] - p1[0])) / det;
    let radius = (p1[0] - cx).hypot(p1[1] - cy);
    Ok(Measured {
        shape: SheetShape::Disc,
        points,
        corner: None,
        angle_deg: None,
        squareness_deg: None,
        center: Some([cx, cy]),
        diameter_mm: Some(2. * radius),
    })
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let l = v[0].hypot(v[1]);
    (l > 1e-9).then(|| [v[0] / l, v[1] / l])
}

/// Where the line through `p` along `d` meets the one through `q` along `e`.
fn intersect(
    from: [f64; 2],
    along: [f64; 2],
    other: [f64; 2],
    other_along: [f64; 2],
) -> Option<[f64; 2]> {
    let det = along[0] * other_along[1] - along[1] * other_along[0];
    if det.abs() < 1e-9 {
        return None;
    }
    let gap = sub(other, from);
    let t = (gap[0] * other_along[1] - gap[1] * other_along[0]) / det;
    Some([from[0] + along[0] * t, from[1] + along[1] * t])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet turned 1° with its corner at (20, 10): four points on its
    /// edges give back the corner and the angle.
    #[test]
    fn four_points_give_the_corner_and_the_angle() {
        let a = 1f64.to_radians();
        let (c, s) = (a.cos(), a.sin());
        let on = |u: f64, v: f64| [20. + u * c - v * s, 10. + u * s + v * c];
        let points = vec![on(50., 0.), on(450., 0.), on(0., 60.), on(0., 460.)];
        let m = rectangle(SheetShape::Rectangle, points).unwrap();
        let corner = m.corner.unwrap();
        assert!((corner[0] - 20.).abs() < 1e-9 && (corner[1] - 10.).abs() < 1e-9);
        assert!((m.angle_deg.unwrap() - 1.).abs() < 1e-9);
        assert!(m.squareness_deg.unwrap().abs() < 1e-9);
        // Two edges: the left one is taken square to the front.
        let three = rectangle(SheetShape::TwoEdges, m.points[..3].to_vec()).unwrap();
        assert!((three.corner.unwrap()[0] - 20.).abs() < 1e-9);
    }

    /// The disc of the capture: centre and diameter from three edge points.
    #[test]
    fn three_points_give_a_disc() {
        let m = disc(vec![[-30., 0.], [30., 0.], [0., -30.]]).unwrap();
        let c = m.center.unwrap();
        assert!(c[0].abs() < 1e-9 && c[1].abs() < 1e-9);
        assert!((m.diameter_mm.unwrap() - 60.).abs() < 1e-9);
    }

    /// Edges far off square are refused rather than trusted.
    #[test]
    fn edges_off_square_are_refused() {
        let points = vec![[50., 10.], [450., 10.], [20., 60.], [40., 460.]];
        assert!(rectangle(SheetShape::Rectangle, points).is_err());
    }
}
