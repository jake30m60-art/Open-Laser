// SPDX-License-Identifier: GPL-3.0-or-later

//! A drawing's curves as the sketch editor keeps them: lines, arcs and
//! circles as drawn, ellipses and elliptical arcs as ellipses, and clamped
//! splines as splines over their control points, all in millimetres with
//! block copies placed. What the editor has no curve for, a spline through
//! fit points, an unclamped spline or a curve-fit polyline, comes as the
//! lines and arcs [`crate::import_with`] would cut, and text as its outlines.

use crate::entities::Piece;
use crate::entities::curves::{Ellipse, Nurbs, Parametric};
use crate::{Error, Options, Result, fit};
use openlaser_core::geometry::{Contour, Curve, Point, Transform};
use std::collections::BTreeSet;
use std::f64::consts::{FRAC_PI_2, TAU};

/// One curve as the sketch editor keeps it.
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive {
    /// A straight line.
    Line {
        /// Where it starts.
        start: Point,
        /// Where it ends.
        end: Point,
    },
    /// A circular arc, a whole circle when it sweeps a full turn.
    Arc(Curve),
    /// An ellipse, or the part of one from `start` to `end`, counter-clockwise:
    /// `center + major·cos t + minor·sin t`, where `minor` is `major` turned
    /// a quarter turn left and shortened to `minor_radius`. Whole when the
    /// parameters span a full turn.
    Ellipse {
        /// The centre.
        center: Point,
        /// From the centre to the end of the major axis.
        major: Point,
        /// The minor semi-axis, no longer than the major one.
        minor_radius: f64,
        /// Where it starts, as a parameter.
        start: f64,
        /// Where it ends, above `start`.
        end: f64,
    },
    /// A clamped B-spline: it starts at its first pole and ends at its last.
    Spline {
        /// Its degree.
        degree: usize,
        /// Poles + degree + 1 knots.
        knots: Vec<f64>,
        /// Its control points.
        poles: Vec<Point>,
        /// One weight per pole; all ones unless rational.
        weights: Vec<f64>,
    },
}

/// A drawing read for the sketch editor.
#[derive(Clone, Debug, PartialEq)]
pub struct SketchImport {
    /// Each curve with its layer, as the cutting import names layers.
    pub curves: Vec<(String, Primitive)>,
}

/// Reads an ASCII DXF file into the sketch editor's curves, with the layers
/// and units [`crate::import_with`] would use for the same options.
pub fn sketch_with(bytes: &[u8], options: &Options) -> Result<SketchImport> {
    if bytes.len() > crate::MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let text = crate::source::text(bytes)?;
    let pairs = crate::pairs::tokenize(text.trim_start_matches('\u{feff}'))?;
    let sections = crate::Sections::read(&pairs)?;
    let scale = sections.units.millimeters();
    let placed = crate::blocks::expand(&sections.entities, &sections.blocks)?;
    let layers = crate::layers(&placed, &sections.layers, options.layers.as_deref());
    let chosen: BTreeSet<String> =
        layers.iter().filter(|l| l.imported).map(|l| l.name.to_uppercase()).collect();
    let mut curves = Vec::new();
    for (entity, transform, layer, own) in &placed.entities {
        if !chosen.contains(&layer.to_uppercase()) {
            continue;
        }
        let by_layer = sections.colors.get(&layer.to_uppercase()).copied();
        let layer = match own {
            Some(rgb) if Some(*rgb) != by_layer => {
                format!("{layer} {}", crate::colors::name(*rgb))
            }
            _ => layer.clone(),
        };
        let m = Transform([scale, 0., 0., scale, 0., 0.]).after(transform);
        for piece in &entity.pieces {
            for primitive in primitives(piece, &m, options.tolerance) {
                curves.push((layer.clone(), primitive));
            }
        }
    }
    let mut outlines: Vec<Contour> = Vec::new();
    let mut notes = Vec::new();
    crate::outline_text(&placed, &chosen, scale, &mut outlines, &mut notes)?;
    for contour in outlines {
        for curve in contour.curves {
            curves.push((contour.layer.clone(), as_drawn(curve)));
        }
    }
    Ok(SketchImport { curves })
}

/// A line or arc as the editor keeps it.
fn as_drawn(curve: Curve) -> Primitive {
    match curve {
        Curve::Line { start, end } => Primitive::Line { start, end },
        arc @ Curve::Arc { .. } => Primitive::Arc(arc),
    }
}

/// One piece of an entity under `m`.
fn primitives(piece: &Piece, m: &Transform, tolerance: f64) -> Vec<Primitive> {
    let fitted = |path: &Parametric| -> Vec<Primitive> {
        let (from, to) = path.domain();
        fit::approximate(|t| m.apply(path.at(t)), from, to, tolerance, path.closed())
            .unwrap_or_default()
            .into_iter()
            .map(as_drawn)
            .collect()
    };
    match piece {
        Piece::Curve(line @ Curve::Line { .. }) => vec![as_drawn(m.curve(line))],
        Piece::Curve(arc) if m.is_similarity() => vec![as_drawn(m.curve(arc))],
        Piece::Curve(arc) | Piece::Path(Parametric::Arc(arc)) => {
            stretched(arc, m).map_or_else(|| fitted(&Parametric::Arc(*arc)), |e| vec![e])
        }
        Piece::Path(Parametric::Ellipse(ellipse)) => ellipse_under(ellipse, m)
            .map_or_else(|| fitted(&Parametric::Ellipse(ellipse.clone())), |e| vec![e]),
        Piece::Path(Parametric::Nurbs(nurbs)) => spline_under(nurbs, m)
            .map_or_else(|| fitted(&Parametric::Nurbs(nurbs.clone())), |s| vec![s]),
        Piece::Path(path) => fitted(path),
        Piece::Points(points) => {
            let points: Vec<Point> = points.iter().map(|p| m.apply(*p)).collect();
            let mut curves = Vec::new();
            fit::fit_points(&points, tolerance, &mut curves);
            curves.into_iter().map(as_drawn).collect()
        }
    }
}

/// The linear part of a transform applied to a direction.
#[allow(clippy::many_single_char_names, reason = "the matrix entries as the transform names them")]
fn turned(m: &Transform, v: Point) -> Point {
    let [a, b, c, d, ..] = m.0;
    Point::new(a * v.x + c * v.y, b * v.x + d * v.y)
}

/// A circular arc through a transform that stretches it: an elliptical arc.
fn stretched(arc: &Curve, m: &Transform) -> Option<Primitive> {
    let Curve::Arc { center, radius, start_angle, sweep } = *arc else { return None };
    // On the circle, t runs counter-clockwise from +X: c + r(cos t, sin t).
    let (from, to) = if sweep >= 0. {
        (start_angle, start_angle + sweep)
    } else {
        (start_angle + sweep, start_angle)
    };
    conjugate(
        m.apply(center),
        turned(m, Point::new(radius, 0.)),
        turned(m, Point::new(0., radius)),
        from,
        to,
    )
}

/// An elliptical arc through a transform.
fn ellipse_under(e: &Ellipse, m: &Transform) -> Option<Primitive> {
    conjugate(m.apply(e.center), turned(m, e.major), turned(m, e.minor), e.start, e.end)
}

/// The ellipse `center + u·cos t + v·sin t`, t from `from` to `to`, by its
/// principal axes and running counter-clockwise.
fn conjugate(center: Point, u: Point, v: Point, from: f64, to: f64) -> Option<Primitive> {
    if from.is_nan() || to.is_nan() || to <= from {
        return None;
    }
    let full = to - from >= TAU - 1e-9;
    // Where the radius is longest: tan 2t₀ = 2u·v / (|u|² − |v|²).
    let t0 = 0.5 * (2. * u.dot(v)).atan2(u.dot(u) - v.dot(v));
    let (sin, cos) = t0.sin_cos();
    let mut a = u * cos + v * sin;
    let mut b = v * cos - u * sin;
    let (mut from, mut to) = (from - t0, to - t0);
    if b.norm() > a.norm() {
        // a·cos s + b·sin s = b·cos(s − π/2) − a·sin(s − π/2).
        (a, b) = (b, a * -1.);
        (from, to) = (from - FRAC_PI_2, to - FRAC_PI_2);
    }
    if a.cross(b) < 0. {
        // Running clockwise: turn the parameter round.
        b = b * -1.;
        (from, to) = (-to, -from);
    }
    let minor_radius = b.norm();
    if a.norm() < 1e-12 || minor_radius < 1e-12 {
        return None;
    }
    if full {
        (from, to) = (0., TAU);
    }
    Some(Primitive::Ellipse { center, major: a, minor_radius, start: from, end: to })
}

/// A clamped spline through a transform; none for one the editor cannot
/// hold, which is fitted instead.
#[allow(clippy::float_cmp, reason = "a repeated knot is the same number written again")]
fn spline_under(n: &Nurbs, m: &Transform) -> Option<Primitive> {
    let p = n.degree;
    let count = n.controls.len();
    let clamped = n.knots[..=p].iter().all(|&k| k == n.knots[0])
        && n.knots[count..].iter().all(|&k| k == n.knots[count]);
    if !clamped || count > 2000 {
        return None;
    }
    Some(Primitive::Spline {
        degree: p,
        knots: n.knots.clone(),
        poles: n.controls.iter().map(|&c| m.apply(c)).collect(),
        weights: n.weights.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stretched_circle_is_an_ellipse_with_its_long_axis_major() {
        // A unit circle scaled 3 along Y and 2 along X.
        let m = Transform([2., 0., 0., 3., 10., 0.]);
        let Some(Primitive::Ellipse { center, major, minor_radius, start, end }) =
            stretched(&Curve::circle(Point::ORIGIN, 1.), &m)
        else {
            panic!("ellipse expected")
        };
        assert_eq!(center, Point::new(10., 0.));
        assert!((major.norm() - 3.).abs() < 1e-12 && major.x.abs() < 1e-12);
        assert!((minor_radius - 2.).abs() < 1e-12);
        assert!((end - start - TAU).abs() < 1e-12);
    }

    #[test]
    fn a_mirrored_elliptical_arc_runs_counter_clockwise_over_the_same_points() {
        let e = Ellipse {
            center: Point::ORIGIN,
            major: Point::new(4., 0.),
            minor: Point::new(0., 1.),
            start: 0.,
            end: FRAC_PI_2,
        };
        let m = Transform([-1., 0., 0., 1., 0., 0.]);
        let Some(Primitive::Ellipse { center, major, minor_radius, start, end }) =
            ellipse_under(&e, &m)
        else {
            panic!("ellipse expected")
        };
        let minor = Point::new(-major.y, major.x) * (minor_radius / major.norm());
        let at = |t: f64| center + major * t.cos() + minor * t.sin();
        // The original ran from (4, 0) to (0, 1); mirrored, (-4, 0) and (0, 1).
        let ends = [at(start), at(end)];
        assert!(ends.iter().any(|p| p.distance(Point::new(-4., 0.)) < 1e-9), "{ends:?}");
        assert!(ends.iter().any(|p| p.distance(Point::new(0., 1.)) < 1e-9), "{ends:?}");
        assert!(at(f64::midpoint(start, end)).x < 0. && at(f64::midpoint(start, end)).y > 0.);
    }
}
