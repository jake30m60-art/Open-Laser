// SPDX-License-Identifier: GPL-3.0-or-later

//! What is left of a sheet once its cut areas are gone. A cut area inside
//! the sheet stays a hole with its exact curves; one that reaches the edge,
//! such as a strip cut off, is taken away from the sheet's own outline.

use crate::Error;
use crate::polygon::{CHECK_REACH, contains, polygon};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::{IntOverlayOptions, Overlay};
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::core::solver::Solver;
use i_overlay::i_float::int::point::IntPoint;
use openlaser_core::geometry::{Contour, Curve, Point};

/// Grid steps per millimetre for the area difference: one micrometre, far
/// below the outline's own chord error.
const SCALE: f64 = 1000.;

/// The remaining sheet: its outline less every cut area reaching its edge,
/// then the cut areas inside it as holes. Where a cut divides the sheet,
/// the largest piece remains.
pub fn remaining(outline: &Contour, cutouts: &[Contour]) -> Result<Vec<Contour>, Error> {
    let boundary: Vec<Point> = polygon(outline)?.into_iter().map(Into::into).collect();
    let mut holes = Vec::new();
    let mut edge = Vec::new();
    for cutout in cutouts {
        let points = polygon(cutout)?;
        let inside = points.iter().all(|p| {
            let p = Point::from(*p);
            contains(&boundary, p) && distance(&boundary, p) > CHECK_REACH
        });
        if inside { holes.push(cutout.clone()) } else { edge.push(points) }
    }
    if edge.is_empty() {
        return Ok(std::iter::once(outline.clone()).chain(holes).collect());
    }
    let subject = vec![grid(&boundary.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>())];
    let clip: Vec<_> = edge.iter().map(|points| grid(points)).collect();
    let shapes = Overlay::with_contours_custom(
        &subject,
        &clip,
        IntOverlayOptions::default(),
        Solver::default(),
    )
    .overlay(OverlayRule::Difference, FillRule::NonZero);
    let piece = shapes
        .into_iter()
        .filter(|shape| !shape.is_empty())
        .max_by(|a, b| area(&a[0]).total_cmp(&area(&b[0])))
        .ok_or_else(|| Error::new("the cut areas leave nothing of this sheet".into()))?;
    let mut rings = piece.into_iter().map(|ring| contour(&outline.layer, &ring));
    let outer = rings.next().ok_or_else(|| Error::new("empty remaining sheet".into()))?;
    let kept: Vec<Point> = polygon(&outer)?.into_iter().map(Into::into).collect();
    // Holes in a piece that was cut away are gone with it.
    holes.retain(|hole| hole.start().is_some_and(|p| contains(&kept, p)));
    Ok(std::iter::once(outer).chain(rings).chain(holes).collect())
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "nesting coordinates are bounded to 100 m, well within i64 micrometres"
)]
fn grid(points: &[[f64; 2]]) -> Vec<IntPoint<i64>> {
    points
        .iter()
        .map(|p| IntPoint::new((p[0] * SCALE).round() as i64, (p[1] * SCALE).round() as i64))
        .collect()
}

#[allow(clippy::cast_precision_loss, reason = "micrometre grid coordinates within 100 m")]
fn area(ring: &[IntPoint<i64>]) -> f64 {
    ring.iter()
        .zip(ring.iter().cycle().skip(1))
        .map(|(a, b)| (a.x as f64) * (b.y as f64) - (b.x as f64) * (a.y as f64))
        .sum::<f64>()
        .abs()
}

#[allow(clippy::cast_precision_loss, reason = "micrometre grid coordinates within 100 m")]
fn contour(layer: &str, ring: &[IntPoint<i64>]) -> Contour {
    let points: Vec<Point> =
        ring.iter().map(|p| Point::new(p.x as f64 / SCALE, p.y as f64 / SCALE)).collect();
    Contour {
        layer: layer.into(),
        curves: points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(&start, &end)| Curve::Line { start, end })
            .collect(),
    }
}

fn distance(boundary: &[Point], p: Point) -> f64 {
    boundary
        .iter()
        .zip(boundary.iter().cycle().skip(1))
        .map(|(&a, &b)| {
            let d = b - a;
            let length = d.x * d.x + d.y * d.y;
            let t = if length > 0. {
                (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / length).clamp(0., 1.)
            } else {
                0.
            };
            p.distance(a.lerp(b, t))
        })
        .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::many_single_char_names, reason = "known geometry fixtures")]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Contour {
        let p = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
        Contour {
            layer: "Stock".into(),
            curves: (0..4)
                .map(|i| Curve::Line { start: p[i].into(), end: p[(i + 1) % 4].into() })
                .collect(),
        }
    }

    #[test]
    fn a_strip_cut_off_the_edge_shrinks_the_sheet_and_inner_cuts_stay_holes() {
        let sheet = rect(0., 0., 1000., 500.);
        let strip = rect(-1., -1., 202., 502.);
        let hole = rect(500., 200., 50., 50.);
        let left = remaining(&sheet, &[strip, hole.clone()]).unwrap();
        assert_eq!(left.len(), 2);
        let b = left[0].bounds().unwrap();
        assert!((b.min.x - 201.).abs() < 1e-6 && (b.max.x - 1000.).abs() < 1e-6, "{b:?}");
        assert!((left[0].signed_area().abs() - 799. * 500.).abs() < 1e-3);
        assert_eq!(left[1], hole);
        // Nothing reaching the edge leaves the sheet exactly as it was.
        let same = remaining(&sheet, std::slice::from_ref(&hole)).unwrap();
        assert_eq!(same, vec![sheet.clone(), hole]);
        assert!(remaining(&sheet, &[rect(-1., -1., 1002., 502.)]).is_err());
    }
}
