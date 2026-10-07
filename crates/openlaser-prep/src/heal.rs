// SPDX-License-Identifier: GPL-3.0-or-later

//! Contour repair. A drawing that was traced, exported or sketched carelessly
//! has small gaps where contours should meet, contours drawn twice, and
//! contours that cross themselves. Gaps up to the tolerance are closed and
//! repeats dropped, as import does; a contour that crosses itself is split at
//! its crossings into simple loops, and a loop smaller than the tolerance, a
//! glitch of the tracing, is dropped. A contour this cannot make simple is
//! kept as drawn and reported, so nothing is repaired by guessing.

use crate::{Result, feature, topology};
use openlaser_core::fit::{MAX_TOLERANCE, MIN_TOLERANCE};
use openlaser_core::geometry::{Bounds, CONTINUITY_TOLERANCE, Contour, Curve, Drawing, Point};
use openlaser_core::repair::{chain, without_duplicates};
use std::collections::HashMap;
use std::f64::consts::TAU;

/// The most curves in a crossing contour that are searched for crossings.
const MAX_SEARCHED: usize = 4000;

/// The most crossings one contour may have to be split; a contour that
/// crosses itself more often is a scribble, not a glitch, and is reported.
const MAX_HITS: usize = 2000;

/// A parameter within this of an end of a curve is that end.
const EPS_T: f64 = 1e-9;

/// A repaired drawing and what was done.
#[derive(Clone, Debug, PartialEq)]
pub struct Healed {
    /// The repaired drawing.
    pub drawing: Drawing,
    /// Whether it differs from the drawing given.
    pub changed: bool,
    /// Gaps closed between ends that nearly met.
    pub gaps: usize,
    /// Contours dropped because another on the same layer cuts the same path.
    pub duplicates: usize,
    /// Contours that crossed themselves and were split into simple loops.
    pub split: usize,
    /// Loops dropped for being smaller than the tolerance.
    pub specks: usize,
    /// Contours of the repaired drawing that still cross themselves, or
    /// cross or touch another contour.
    pub unresolved: Vec<usize>,
}

/// The indices of the closed contours that cross themselves, or cross or
/// touch another contour, so the drawing cannot be prepared as it is.
#[must_use]
pub fn faulty(drawing: &Drawing) -> Vec<usize> {
    let mut found: Vec<usize> = drawing
        .contours
        .iter()
        .enumerate()
        .filter(|(_, contour)| crate::crosses_itself(contour))
        .map(|(i, _)| i)
        .collect();
    found.extend(topology::tangled(&drawing.contours));
    found.sort_unstable();
    found.dedup();
    found
}

/// Repairs `drawing`, moving no path further than `tolerance`.
pub fn heal(drawing: &Drawing, tolerance: f64) -> Result<Healed> {
    if !(MIN_TOLERANCE..=MAX_TOLERANCE).contains(&tolerance) {
        return Err(feature(
            "repair",
            format!("the tolerance must be {MIN_TOLERANCE} to {MAX_TOLERANCE} mm"),
        ));
    }
    let (chained, joins) = chain(&drawing.contours, tolerance);
    let (kept, repeats) = without_duplicates(chained, tolerance);
    let (mut split, mut specks) = (0, 0);
    let mut contours = Vec::with_capacity(kept.len());
    for contour in kept {
        if !crate::crosses_itself(&contour) {
            contours.push(contour);
            continue;
        }
        match loops(&contour, tolerance) {
            Some((simple, dropped)) => {
                split += 1;
                specks += dropped;
                contours.extend(simple);
            }
            None => contours.push(contour),
        }
    }
    // An import splits a tracing glitch off as its own tiny loop that touches
    // the contour it came from; that loop is no part of the shape.
    let tiny: Vec<usize> = topology::tangled(&contours)
        .into_iter()
        .filter(|&i| contours[i].bounds().is_some_and(|b| b.extent() <= tolerance))
        .collect();
    specks += tiny.len();
    let contours: Vec<Contour> = contours
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !tiny.contains(i))
        .map(|(_, contour)| contour)
        .collect();
    let repaired = Drawing { contours };
    Ok(Healed {
        changed: &repaired != drawing,
        unresolved: faulty(&repaired),
        drawing: repaired,
        gaps: joins.len(),
        duplicates: repeats.len(),
        split,
        specks,
    })
}

// Splitting ----------------------------------------------------------------------

/// Where two curves of a contour meet: the curve and parameter on each side.
struct Hit {
    first: (usize, f64),
    second: (usize, f64),
    at: Point,
}

/// What a walk along the contour passes: a stretch of curve or a crossing.
enum Token {
    Piece(Curve),
    Node(usize),
}

/// The simple closed loops a self-crossing contour falls apart into, and how
/// many of them were smaller than `tolerance` and dropped; `None` when the
/// crossings cannot be resolved into closed, simple loops.
fn loops(contour: &Contour, tolerance: f64) -> Option<(Vec<Contour>, usize)> {
    if !contour.is_closed() || contour.curves.len() > MAX_SEARCHED {
        return None;
    }
    let hits = hits(&contour.curves)?;
    if hits.is_empty() {
        return None;
    }
    // Each place the contour crosses itself is one node, however many
    // curves pass through it.
    let mut nodes: Vec<Point> = Vec::new();
    let mut cuts: Vec<Vec<(f64, usize)>> = vec![Vec::new(); contour.curves.len()];
    for hit in &hits {
        let node = nodes.iter().position(|p| p.distance(hit.at) <= 10. * CONTINUITY_TOLERANCE);
        let node = node.unwrap_or_else(|| {
            nodes.push(hit.at);
            nodes.len() - 1
        });
        for (curve, t) in [hit.first, hit.second] {
            cuts[curve].push((t, node));
        }
    }
    let mut tokens = Vec::new();
    for (curve, cut) in contour.curves.iter().zip(&mut cuts) {
        cut.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut from = 0.;
        for &(t, node) in &*cut {
            if t <= EPS_T {
                tokens.push(Token::Node(node));
            } else if t >= 1. - EPS_T {
                continue;
            } else {
                tokens.push(Token::Piece(curve.slice(from, t)));
                tokens.push(Token::Node(node));
                from = t;
            }
        }
        tokens.push(Token::Piece(curve.slice(from, 1.)));
        for &(t, node) in &*cut {
            if t >= 1. - EPS_T {
                tokens.push(Token::Node(node));
            }
        }
    }
    let mut path: Vec<Curve> = Vec::new();
    let mut seen: HashMap<usize, usize> = HashMap::new();
    let mut found: Vec<Vec<Curve>> = Vec::new();
    for token in tokens {
        match token {
            Token::Piece(piece) => {
                if piece.length() > CONTINUITY_TOLERANCE {
                    path.push(piece);
                }
            }
            Token::Node(node) => {
                if let Some(&at) = seen.get(&node) {
                    let lap: Vec<Curve> = path.drain(at..).collect();
                    if !lap.is_empty() {
                        found.push(lap);
                    }
                    seen.retain(|_, position| *position <= at);
                }
                seen.insert(node, path.len());
            }
        }
    }
    if !path.is_empty() {
        found.push(path);
    }
    let mut simple = Vec::new();
    let mut specks = 0;
    for curves in found {
        let lap = Contour { layer: contour.layer.clone(), curves };
        if !lap.is_closed() {
            return None;
        }
        if lap.bounds().is_none_or(|b| b.extent() <= tolerance) {
            specks += 1;
        } else if topology::polyline(&lap).is_ok() {
            simple.push(lap);
        } else {
            return None;
        }
    }
    (!simple.is_empty()).then_some((simple, specks))
}

/// Every place two curves of a closed contour cross or touch, not counting
/// where neighbours join; `None` past [`MAX_HITS`].
fn hits(curves: &[Curve]) -> Option<Vec<Hit>> {
    let boxes: Vec<Bounds> = curves.iter().map(Curve::bounds).collect();
    let mut found = Vec::new();
    for i in 0..curves.len() {
        for j in i + 1..curves.len() {
            let (a, b) = (boxes[i], boxes[j]);
            let slack = CONTINUITY_TOLERANCE;
            if a.max.x + slack < b.min.x
                || b.max.x + slack < a.min.x
                || a.max.y + slack < b.min.y
                || b.max.y + slack < a.min.y
            {
                continue;
            }
            for (ti, tj) in crossings(&curves[i], &curves[j]) {
                let at = curves[i].point(ti);
                let joined = (j == i + 1
                    && at.distance(curves[i].end()) <= 10. * CONTINUITY_TOLERANCE)
                    || (i == 0
                        && j == curves.len() - 1
                        && at.distance(curves[i].start()) <= 10. * CONTINUITY_TOLERANCE);
                if !joined {
                    if found.len() == MAX_HITS {
                        return None;
                    }
                    found.push(Hit { first: (i, ti), second: (j, tj), at });
                }
            }
        }
    }
    Some(found)
}

/// The parameters on `a` and on `b` where they meet. Curves that run along
/// each other, with no single crossing, meet nowhere.
fn crossings(a: &Curve, b: &Curve) -> Vec<(f64, f64)> {
    match (*a, *b) {
        (Curve::Line { start: p, end: p2 }, Curve::Line { start: q, end: q2 }) => {
            line_line(p, p2, q, q2).into_iter().collect()
        }
        (Curve::Line { start, end }, Curve::Arc { center, radius, start_angle, sweep }) => {
            line_arc(start, end, center, radius, start_angle, sweep)
        }
        (Curve::Arc { center, radius, start_angle, sweep }, Curve::Line { start, end }) => {
            line_arc(start, end, center, radius, start_angle, sweep)
                .into_iter()
                .map(|(line, arc)| (arc, line))
                .collect()
        }
        (
            Curve::Arc { center: c1, radius: r1, start_angle: s1, sweep: w1 },
            Curve::Arc { center: c2, radius: r2, start_angle: s2, sweep: w2 },
        ) => arc_arc((c1, r1, s1, w1), (c2, r2, s2, w2)),
    }
}

fn in_range(t: f64) -> Option<f64> {
    (-EPS_T..=1. + EPS_T).contains(&t).then(|| t.clamp(0., 1.))
}

fn line_line(p: Point, p2: Point, q: Point, q2: Point) -> Option<(f64, f64)> {
    let (r, s) = (p2 - p, q2 - q);
    let denominator = r.cross(s);
    if denominator.abs() <= 1e-12 * r.norm() * s.norm() {
        return None;
    }
    let offset = q - p;
    Some((in_range(offset.cross(s) / denominator)?, in_range(offset.cross(r) / denominator)?))
}

/// Where a point on a circle lies along an arc, if it lies on the arc.
fn along(center: Point, start_angle: f64, sweep: f64, point: Point) -> Option<f64> {
    let angle = (point - center).angle();
    let travelled =
        if sweep >= 0. { angle - start_angle } else { start_angle - angle }.rem_euclid(TAU);
    let t = travelled / sweep.abs();
    if t <= 1. + EPS_T {
        Some(t.min(1.))
    } else if TAU - travelled <= 1e-9 {
        Some(0.)
    } else {
        None
    }
}

fn line_arc(
    start: Point,
    end: Point,
    center: Point,
    radius: f64,
    start_angle: f64,
    sweep: f64,
) -> Vec<(f64, f64)> {
    let direction = end - start;
    let from_center = start - center;
    let a = direction.dot(direction);
    let b = 2. * from_center.dot(direction);
    let c = from_center.dot(from_center) - radius * radius;
    let discriminant = b * b - 4. * a * c;
    if a == 0. || discriminant < -1e-12 * (b * b).max(1e-30) {
        return Vec::new();
    }
    let root = discriminant.max(0.).sqrt();
    let mut roots = vec![(-b - root) / (2. * a)];
    if root > 0. {
        roots.push((-b + root) / (2. * a));
    }
    roots
        .into_iter()
        .filter_map(|t| {
            let t = in_range(t)?;
            let u = along(center, start_angle, sweep, start + direction * t)?;
            Some((t, u))
        })
        .collect()
}

type Arc = (Point, f64, f64, f64);

fn arc_arc((c1, r1, s1, w1): Arc, (c2, r2, s2, w2): Arc) -> Vec<(f64, f64)> {
    let between = c2 - c1;
    let d = between.norm();
    if d < 1e-12 || d > r1 + r2 + 1e-9 || d < (r1 - r2).abs() - 1e-9 {
        return Vec::new();
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2. * d);
    let h = (r1 * r1 - a * a).max(0.).sqrt();
    let middle = c1 + between * (a / d);
    let across = between.perpendicular() * (h / d);
    let mut points = vec![middle + across];
    if h > 0. {
        points.push(middle - across);
    }
    points
        .into_iter()
        .filter_map(|p| Some((along(c1, s1, w1, p)?, along(c2, s2, w2, p)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn polygon(points: &[(f64, f64)]) -> Contour {
        let corners: Vec<Point> = points.iter().map(|&(x, y)| at(x, y)).collect();
        Contour {
            layer: "0".into(),
            curves: (0..corners.len())
                .map(|i| Curve::Line { start: corners[i], end: corners[(i + 1) % corners.len()] })
                .collect(),
        }
    }

    fn healed(contours: Vec<Contour>) -> Healed {
        heal(&Drawing { contours }, 0.05).unwrap()
    }

    #[test]
    fn a_bow_tie_falls_apart_into_two_triangles() {
        let result = healed(vec![polygon(&[(0., 0.), (10., 10.), (10., 0.), (0., 10.)])]);
        assert!(result.changed);
        assert_eq!((result.split, result.specks), (1, 0));
        // The triangles meet in a point, which preparation still refuses.
        assert_eq!(result.unresolved, vec![0, 1]);
        assert_eq!(result.drawing.contours.len(), 2);
        for contour in &result.drawing.contours {
            assert!(contour.is_closed() && !crate::crosses_itself(contour));
            assert!((contour.signed_area().abs() - 25.).abs() < 1e-6);
        }
    }

    #[test]
    fn a_small_curl_is_dropped_and_the_outline_kept() {
        // A square with a tiny loop in its bottom edge.
        let result = healed(vec![polygon(&[
            (0., 0.),
            (4., 0.),
            (4.02, 0.02),
            (4.02, -0.02),
            (4., 0.),
            (10., 0.),
            (10., 10.),
            (0., 10.),
        ])]);
        // The loop touches itself at (4, 0); it is the speck.
        assert!(result.unresolved.is_empty(), "{:?}", result.unresolved);
        assert_eq!(result.drawing.contours.len(), 1);
        assert_eq!(result.specks, 1);
        assert!((result.drawing.contours[0].signed_area().abs() - 100.).abs() < 1e-6);
    }

    #[test]
    fn a_tiny_loop_touching_the_outline_is_dropped() {
        // As an import leaves it: the outline, and its glitch as a loop of its own.
        let outline = polygon(&[(0., 0.), (4., 0.), (10., 0.), (10., 10.), (0., 10.)]);
        let glitch = polygon(&[(4., 0.), (4.02, 0.02), (4.02, -0.02)]);
        let big_touching = polygon(&[(10., 0.), (14., 0.), (14., 4.)]);
        let result = healed(vec![outline, glitch, big_touching]);
        // The glitch goes; a real shape touching the outline is left for the operator.
        assert_eq!(result.specks, 1);
        assert_eq!(result.drawing.contours.len(), 2);
        assert_eq!(result.unresolved, vec![0, 1]);
    }

    #[test]
    fn arcs_cross_and_split_as_arcs() {
        // Two half circles of different centres drawn as one closed outline
        // that crosses itself where they meet.
        let a = Curve::Arc {
            center: at(0., 0.),
            radius: 5.,
            start_angle: 0.,
            sweep: std::f64::consts::PI,
        };
        let b = Curve::Arc {
            center: at(4., 0.),
            radius: 5.,
            start_angle: 0.,
            sweep: -std::f64::consts::PI,
        };
        let close = Curve::Line { start: a.end(), end: b.start() };
        let back = Curve::Line { start: b.end(), end: a.start() };
        let contour = Contour { layer: "0".into(), curves: vec![a, close, b, back] };
        assert!(contour.is_closed());
        assert!(crate::crosses_itself(&contour));
        let result = healed(vec![contour]);
        assert!(result.drawing.contours.len() >= 2);
        assert!(result.drawing.contours.iter().all(|c| c.is_closed() && !crate::crosses_itself(c)));
        assert!(
            result
                .drawing
                .contours
                .iter()
                .flat_map(|c| &c.curves)
                .any(|c| matches!(c, Curve::Arc { .. }))
        );
    }

    #[test]
    fn a_gap_is_closed_and_a_clean_drawing_is_untouched() {
        let open = Contour {
            layer: "0".into(),
            curves: vec![
                Curve::Line { start: at(0., 0.), end: at(10., 0.) },
                Curve::Line { start: at(10., 0.), end: at(10., 10.) },
                Curve::Line { start: at(10., 10.), end: at(0., 10.) },
                Curve::Line { start: at(0., 10.), end: at(0., 0.02) },
            ],
        };
        let result = healed(vec![open]);
        assert!(result.changed && result.gaps == 1);
        assert!(result.drawing.contours[0].is_closed());
        let clean = healed(vec![polygon(&[(0., 0.), (10., 0.), (10., 10.), (0., 10.)])]);
        assert!(!clean.changed);
        assert_eq!((clean.gaps, clean.duplicates, clean.split, clean.specks), (0, 0, 0, 0));
    }

    #[test]
    fn a_copy_is_dropped() {
        let square = polygon(&[(0., 0.), (10., 0.), (10., 10.), (0., 10.)]);
        let result = healed(vec![square.clone(), square]);
        assert_eq!((result.duplicates, result.drawing.contours.len()), (1, 1));
    }

    #[test]
    fn the_tolerance_is_checked() {
        assert!(heal(&Drawing::default(), 0.).is_err());
        assert!(heal(&Drawing::default(), 5.).is_err());
    }
}
