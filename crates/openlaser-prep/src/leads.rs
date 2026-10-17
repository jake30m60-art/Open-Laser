// SPDX-License-Identifier: GPL-3.0-or-later

//! Entry and exit leads: the approach before the contour and the departure
//! after it, on the waste side, so the pierce and the finish stay off the
//! part's edge.

use crate::{Result, feature};
use openlaser_core::features::{Lead, LeadShape, Leads, Side};
use openlaser_core::geometry::{Curve, Point};
use openlaser_core::toolpath::{LeadRole, PreparedSegment, SegmentProcess};
use openlaser_core::units::Millimeters;
use std::f64::consts::FRAC_PI_2;

pub(crate) fn check(leads: &Leads) -> Result<()> {
    let roles = [("entry lead", leads.entry), ("exit lead", leads.exit)];
    let edited = leads.overrides.iter().flat_map(|edited| {
        [("edited entry lead", edited.entry), ("edited exit lead", edited.exit)]
    });
    for (name, lead) in roles.into_iter().chain(edited) {
        let Some(lead) = lead else { continue };
        check_lead(name, lead)?;
    }
    for edited in &leads.overrides {
        crate::joints::spots_ok("leads", std::slice::from_ref(&edited.location))?;
        if edited.entry.is_none() && edited.exit.is_none() {
            return Err(feature("leads", "an override must edit an entry or exit"));
        }
    }
    Ok(())
}

fn check_lead(name: &'static str, lead: Lead) -> Result<()> {
    let positive = |value: f64| value.is_finite() && value > 0.;
    let angle = lead.angle.0;
    if !angle.is_finite() {
        return Err(feature(name, "the angle must be a number"));
    }
    if matches!(lead.shape, LeadShape::Line | LeadShape::LineArc) && !positive(lead.length.0) {
        return Err(feature(name, "the length must be positive"));
    }
    if matches!(lead.shape, LeadShape::Arc | LeadShape::LineArc)
        && (!positive(lead.radius.0) || angle <= 0. || angle >= 360.)
    {
        return Err(feature(
            name,
            "an arc needs a positive radius and an angle between 0 and 360 degrees",
        ));
    }
    Ok(())
}

/// The contour with its leads attached and tagged. `counterclockwise` is
/// the winding of the contour as it runs; with it and the side, the leads
/// know which way is into the waste.
pub(crate) fn attached(
    segments: Vec<PreparedSegment>,
    leads: Option<&Leads>,
    closed: bool,
    hole: bool,
    counterclockwise: bool,
) -> Vec<PreparedSegment> {
    let Some(leads) = leads else { return segments };
    if (leads.closed_only && !closed) || segments.is_empty() {
        return segments;
    }
    let inside = match leads.side {
        Side::Inside => true,
        Side::Outside => false,
        Side::Auto => hole,
    };
    // Counterclockwise, the inside is to the left of travel.
    let side = if counterclockwise == inside { 1. } else { -1. };
    let first = segments[0].curve;
    let last = segments[segments.len() - 1].curve;
    // Inside a closed contour a lead longer than the hole allows would cut
    // into the part on the far side: shrink it until it fits.
    let fit = if closed && inside { Some(outline(&segments)) } else { None };
    let clearance = leads.clearance.map_or(LEAD_CLEARANCE_MM, |c| c.0.max(0.));
    let fit_lead =
        |lead: Lead, anchor: Point, tangent: Point, entry: bool, line_along: Option<Point>| {
            let Some(polygon) = fit.as_deref() else { return lead };
            fitted(lead, polygon, clearance, |l| match line_along {
                Some(b) if l.shape == LeadShape::Line => {
                    let far = anchor + b * l.length.0;
                    vec![if entry {
                        Curve::Line { start: far, end: anchor }
                    } else {
                        Curve::Line { start: anchor, end: far }
                    }]
                }
                _ => shape(l, anchor, tangent, side, entry),
            })
        };
    // A closed contour starting in a corner: a straight lead comes in along
    // the bisector of the corner on the waste side, so it lies in a hole and
    // outside an outline whichever way the corner turns.
    let bisector = if closed { corner(last.tangent(1.), first.tangent(0.), side) } else { None };
    let entry =
        leads.entry.map(|lead| fit_lead(lead, first.start(), first.tangent(0.), true, bisector));
    let exit = leads.exit.map(|lead| fit_lead(lead, last.end(), last.tangent(1.), false, bisector));
    let entry = entry.map_or_else(Vec::new, |lead| match bisector {
        Some(b) if lead.shape == LeadShape::Line => {
            vec![Curve::Line { start: first.start() + b * lead.length.0, end: first.start() }]
        }
        _ => shape(lead, first.start(), first.tangent(0.), side, true),
    });
    let exit = exit.map_or_else(Vec::new, |lead| match bisector {
        Some(b) if lead.shape == LeadShape::Line => {
            vec![Curve::Line { start: last.end(), end: last.end() + b * lead.length.0 }]
        }
        _ => shape(lead, last.end(), last.tangent(1.), side, false),
    });
    let tag = |role| SegmentProcess { lead: Some(role), ..SegmentProcess::default() };
    let mut all: Vec<_> =
        entry.into_iter().map(|curve| PreparedSegment::new(curve, tag(LeadRole::Entry))).collect();
    all.extend(segments);
    all.extend(exit.into_iter().map(|curve| PreparedSegment::new(curve, tag(LeadRole::Exit))));
    all
}

/// The closed contour as a polygon, a few points per curve.
fn outline(segments: &[PreparedSegment]) -> Vec<Point> {
    segments
        .iter()
        .flat_map(|s| {
            // About every half millimetre, so an arc stays within a few
            // microns of its chords.
            let steps = if matches!(s.curve, Curve::Line { .. }) {
                1
            } else {
                (16..512u32).find(|n| f64::from(*n) * 0.5 >= s.curve.length()).unwrap_or(512)
            };
            (0..steps).map(move |i| s.curve.point(f64::from(i) / f64::from(steps)))
        })
        .collect()
}

fn inside_polygon(polygon: &[Point], p: Point) -> bool {
    let mut inside = false;
    for (i, a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

fn distance_to_polygon(polygon: &[Point], p: Point) -> f64 {
    let mut best = f64::INFINITY;
    for (i, a) in polygon.iter().enumerate() {
        let b = polygon[(i + 1) % polygon.len()];
        let ab = b - *a;
        let t = if ab.dot(ab) > 0. { ((p - *a).dot(ab) / ab.dot(ab)).clamp(0., 1.) } else { 0. };
        best = best.min((p - (*a + ab * t)).norm());
    }
    best
}

/// How far a lead's free end stays off a hole's contour unless the job says.
const LEAD_CLEARANCE_MM: f64 = 0.5;

/// `lead`, scaled down until every point of it lies in `polygon` and its
/// free end keeps clear of the contour; unchanged when it already fits.
fn fitted(
    lead: Lead,
    polygon: &[Point],
    clearance: f64,
    curves: impl Fn(Lead) -> Vec<Curve>,
) -> Lead {
    if polygon.len() < 3 {
        return lead;
    }
    let scaled = |k: f64| Lead {
        length: Millimeters(lead.length.0 * k),
        radius: Millimeters(lead.radius.0 * k),
        ..lead
    };
    let fits = |k: f64| {
        let all = curves(scaled(k));
        let points: Vec<Point> = all
            .iter()
            .flat_map(|c| {
                (1..8).map(move |i| c.point(f64::from(i) / 8.)).chain([c.start(), c.end()])
            })
            .collect();
        // The free end keeps `clearance` from the contour, or as much as the
        // lead holds off a straight edge where it is shorter: an arc lead
        // runs alongside the contour on purpose.
        let anchor = polygon[0];
        // The tangent at the anchor, across it so a curve does not tilt it.
        let along = polygon[1] - polygon[polygon.len() - 1];
        let free = all.first().map(Curve::start).zip(all.last().map(Curve::end));
        let clear = free.is_none_or(|(a, b)| {
            let far = if (a - anchor).norm() > (b - anchor).norm() { a } else { b };
            let off_edge = if along.norm() > 1e-9 {
                (far - anchor).cross(along).abs() / along.norm()
            } else {
                0.
            };
            distance_to_polygon(polygon, far) >= clearance.min(off_edge * 0.5)
        });
        clear
            && points
                .iter()
                .all(|p| inside_polygon(polygon, *p) || distance_to_polygon(polygon, *p) < 0.02)
    };
    if fits(1.) {
        return lead;
    }
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..20 {
        let mid = f64::midpoint(lo, hi);
        if fits(mid) { lo = mid } else { hi = mid }
    }
    scaled(lo)
}

/// The unit bisector into the waste of a corner where travel turns from
/// `incoming` to `outgoing`; none where the contour runs on smoothly.
fn corner(incoming: Point, outgoing: Point, side: f64) -> Option<Point> {
    let (a, b) = (incoming.norm(), outgoing.norm());
    if a < 1e-12 || b < 1e-12 {
        return None;
    }
    let (incoming, outgoing) = (incoming * (1. / a), outgoing * (1. / b));
    // Below about two degrees the contour runs on smoothly.
    if incoming.cross(outgoing).abs() < 0.035 && incoming.dot(outgoing) > 0. {
        return None;
    }
    let waste = incoming.rotated(side * FRAC_PI_2) + outgoing.rotated(side * FRAC_PI_2);
    let length = waste.norm();
    (length > 1e-6).then(|| waste * (1. / length))
}

/// The curves of one lead at `anchor`, where the contour's tangent is
/// `tangent`; `side` is +1 when the waste is to the left of travel.
fn shape(lead: Lead, anchor: Point, tangent: Point, side: f64, entry: bool) -> Vec<Curve> {
    let angle = lead.angle.0.to_radians();
    let length = lead.length.0;
    let line = |at: Point, direction: Point| {
        if entry {
            Curve::Line { start: at - direction * length, end: at }
        } else {
            Curve::Line { start: at, end: at + direction * length }
        }
    };
    if lead.shape == LeadShape::Line {
        let turn = if entry { -side * angle } else { side * angle };
        return vec![line(anchor, tangent.rotated(turn))];
    }
    let radius = lead.radius.0;
    let center = anchor + tangent.rotated(side * FRAC_PI_2) * radius;
    let at = (anchor - center).angle();
    let sweep = side * angle;
    let arc =
        Curve::Arc { center, radius, start_angle: if entry { at - sweep } else { at }, sweep };
    if lead.shape == LeadShape::Arc {
        return vec![arc];
    }
    let (join, direction) =
        if entry { (arc.start(), arc.tangent(0.)) } else { (arc.end(), arc.tangent(1.)) };
    let straight = line(join, direction);
    if entry { vec![straight, arc] } else { vec![arc, straight] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attached(
        curves: Vec<Curve>,
        processes: Vec<SegmentProcess>,
        leads: Option<&Leads>,
        closed: bool,
        hole: bool,
        counterclockwise: bool,
    ) -> (Vec<Curve>, Vec<SegmentProcess>) {
        let segments =
            curves.into_iter().zip(processes).map(|(c, p)| PreparedSegment::new(c, p)).collect();
        super::attached(segments, leads, closed, hole, counterclockwise)
            .into_iter()
            .map(|s| (s.curve, s.process))
            .unzip()
    }

    use openlaser_core::units::{Degrees, Millimeters};

    fn lead(shape: LeadShape) -> Lead {
        Lead { shape, length: Millimeters(2.), radius: Millimeters(1.), angle: Degrees(90.) }
    }

    fn leads(shape: LeadShape) -> Leads {
        Leads {
            entry: Some(lead(shape)),
            exit: Some(lead(shape)),
            side: Side::Auto,
            closed_only: false,
            clearance: None,
            overrides: Vec::new(),
        }
    }

    /// A line running along +X on a counterclockwise outline gets its
    /// entry from below (the waste is outside, to the right) and its exit
    /// to below; on a hole both go up, into the hole.
    #[test]
    fn line_leads_lie_on_the_waste_side() {
        let along = Curve::Line { start: Point::ORIGIN, end: Point::new(10., 0.) };
        let (curves, tags) = attached(
            vec![along],
            vec![SegmentProcess::default()],
            Some(&leads(LeadShape::Line)),
            true,
            false,
            true,
        );
        assert_eq!(curves.len(), 3);
        assert_eq!(tags[0].lead, Some(LeadRole::Entry));
        assert_eq!(tags[2].lead, Some(LeadRole::Exit));
        assert!(curves[0].start().distance(Point::new(0., -2.)) < 1e-9);
        assert_eq!(curves[0].end(), Point::ORIGIN);
        assert!(curves[2].end().distance(Point::new(10., -2.)) < 1e-9);
        let (hole, _) = attached(
            vec![along],
            vec![SegmentProcess::default()],
            Some(&leads(LeadShape::Line)),
            true,
            true,
            true,
        );
        assert!(hole[0].start().distance(Point::new(0., 2.)) < 1e-9);
        assert!(hole[2].end().distance(Point::new(10., 2.)) < 1e-9);
    }

    /// An arc lead is tangent to the contour where it meets it, and a
    /// line-arc lead runs straight into the arc's far end.
    #[test]
    fn arc_leads_are_tangent_at_the_anchor() {
        let along = Curve::Line { start: Point::ORIGIN, end: Point::new(10., 0.) };
        let (curves, _) = attached(
            vec![along],
            vec![SegmentProcess::default()],
            Some(&leads(LeadShape::Arc)),
            true,
            false,
            true,
        );
        let entry = curves[0];
        assert!(entry.end().distance(Point::ORIGIN) < 1e-9);
        assert!(entry.tangent(1.).distance(Point::new(1., 0.)) < 1e-9);
        assert!(entry.start().distance(Point::new(-1., -1.)) < 1e-9);
        let (curves, _) = attached(
            vec![along],
            vec![SegmentProcess::default()],
            Some(&leads(LeadShape::LineArc)),
            true,
            false,
            true,
        );
        assert_eq!(curves.len(), 5);
        assert!(curves[0].end().distance(curves[1].start()) < 1e-9);
        assert!(curves[0].tangent(1.).distance(curves[1].tangent(0.)) < 1e-9);
        assert!(curves[0].start().distance(Point::new(-1., -3.)) < 1e-9);
    }

    /// Open contours get no leads when only closed ones should, and bad
    /// lead values are refused.
    #[test]
    fn closed_only_and_checks() {
        let along = Curve::Line { start: Point::ORIGIN, end: Point::new(10., 0.) };
        let only = Leads { closed_only: true, ..leads(LeadShape::Line) };
        assert_eq!(
            attached(vec![along], vec![SegmentProcess::default()], Some(&only), false, false, true)
                .0
                .len(),
            1
        );
        assert!(
            check(&Leads {
                entry: Some(Lead { length: Millimeters(0.), ..lead(LeadShape::Line) }),
                ..leads(LeadShape::Line)
            })
            .is_err()
        );
        assert!(
            check(&Leads {
                exit: Some(Lead { angle: Degrees(360.), ..lead(LeadShape::Arc) }),
                ..leads(LeadShape::Arc)
            })
            .is_err()
        );
        assert!(check(&leads(LeadShape::LineArc)).is_ok());
    }

    #[test]
    fn leads_in_a_small_hole_shrink_to_fit() {
        // A 4 mm hole, its leads asked for 5 mm: they must stay inside it.
        let circle = Curve::Arc {
            center: Point::new(0., 0.),
            radius: 2.,
            start_angle: 0.,
            sweep: 2. * std::f64::consts::PI,
        };
        for shape in [LeadShape::Line, LeadShape::LineArc, LeadShape::Arc] {
            let mut leads = leads(shape);
            for l in [leads.entry.as_mut().unwrap(), leads.exit.as_mut().unwrap()] {
                l.length = Millimeters(5.);
                l.radius = Millimeters(3.);
                l.angle = Degrees(45.);
            }
            let (curves, _) = attached(
                vec![circle],
                vec![SegmentProcess::default()],
                Some(&leads),
                true,
                true,
                true,
            );
            for c in &curves[..curves.len()] {
                for i in 0..=8 {
                    let p = c.point(f64::from(i) / 8.);
                    assert!(p.norm() <= 2. + 1e-6, "{shape:?} lead leaves the hole at {p:?}");
                }
            }
            let entry = curves.first().unwrap();
            assert!(entry.start().norm() > 0.1, "{shape:?} lead is not collapsed to nothing");
        }
    }

    #[test]
    fn an_arc_lead_that_fits_a_hole_is_kept() {
        let circle = Curve::Arc {
            center: Point::new(20., 30.),
            radius: 5.,
            start_angle: 0.,
            sweep: 2. * std::f64::consts::PI,
        };
        let mut leads = leads(LeadShape::Arc);
        leads.exit = None;
        leads.entry.as_mut().unwrap().radius = Millimeters(1.5);
        leads.entry.as_mut().unwrap().angle = Degrees(45.);
        let (curves, _) =
            attached(vec![circle], vec![SegmentProcess::default()], Some(&leads), true, true, true);
        assert_eq!(curves.len(), 2);
        let Curve::Arc { radius, .. } = curves[0] else { panic!() };
        assert!((radius - 1.5).abs() < 1e-9, "radius {radius}");
    }
}
