// SPDX-License-Identifier: GPL-3.0-or-later

//! Flying cuts and scan fills: straight runs cut at speed with the laser
//! switched off over the gaps between them.
//!
//! Both build the same thing, a line of spans: a laser-off run-up, the
//! cuts with laser-off gaps between them, and a laser-off run-out. The gaps
//! are joints at zero power and the cutting speed, so the compiler keeps
//! the head moving and gates the beam step by step; a contour that opens in
//! a joint enters without a stationary pierce. Nothing here touches the
//! compiler: it sees ordinary prepared segments.
//!
//! A flying cut gathers the straight edges of the selected contours that
//! lie on one line and replaces those contours with one pass per line,
//! alternating direction. A scan fill crosses a marked layer's closed
//! shapes with parallel lines, inside by the even-odd rule, joined into a
//! single zigzag.

use crate::{EPS, Error, Result, feature, float};
use openlaser_core::features::{Fill, FlyCut};
use openlaser_core::geometry::{Curve, Point};
use openlaser_core::toolpath::{PreparedContour, PreparedSegment, SegmentProcess};

/// The smallest uncut web between two cuts on one line, in millimetres.
/// Below it the beam's switching at speed could cut through.
pub const MIN_WEB: f64 = 0.5;
/// The most edges a flying cut gathers.
const MAX_EDGES: usize = 50_000;
/// The most scan lines one fill makes.
const MAX_SCAN_LINES: usize = 100_000;
/// The most segments one fill makes.
const MAX_FILL_SEGMENTS: usize = 400_000;
/// The most scan lines joined into one path. Every turn costs the motion
/// planner two of its 2048 profile builds per contour.
const LINES_PER_PATH: usize = 250;
/// Chord tolerance when an arc is flattened for scanning, in millimetres.
const FLATTEN: f64 = 0.005;

/// A stretch at cutting speed with the beam off.
fn off() -> SegmentProcess {
    SegmentProcess { joint: true, power: Some(0.), run_up: true, ..SegmentProcess::default() }
}

/// A line through `origin` along the unit `direction`, and the intervals
/// along it that are cut, sorted and apart.
struct Line {
    origin: Point,
    direction: Point,
    spans: Vec<(f64, f64)>,
}

impl Line {
    fn at(&self, t: f64) -> Point {
        self.origin + self.direction * t
    }

    /// The run-up, the cuts and gaps, and the run-out, forwards or reversed.
    fn segments(&self, run_up: f64, reverse: bool) -> Vec<PreparedSegment> {
        let mut pieces: Vec<(f64, f64, bool)> = Vec::with_capacity(self.spans.len() * 2 + 1);
        let (first, last) = (self.spans[0].0, self.spans[self.spans.len() - 1].1);
        if run_up > 0. {
            pieces.push((first - run_up, first, false));
        }
        for (i, &(a, b)) in self.spans.iter().enumerate() {
            if i > 0 {
                pieces.push((self.spans[i - 1].1, a, false));
            }
            pieces.push((a, b, true));
        }
        if run_up > 0. {
            pieces.push((last, last + run_up, false));
        }
        if reverse {
            pieces.reverse();
            for piece in &mut pieces {
                *piece = (piece.1, piece.0, piece.2);
            }
        }
        pieces
            .into_iter()
            .filter(|(a, b, _)| (b - a).abs() > EPS)
            .map(|(a, b, cut)| {
                let curve = Curve::Line { start: self.at(a), end: self.at(b) };
                PreparedSegment::new(curve, if cut { SegmentProcess::default() } else { off() })
            })
            .collect()
    }

    /// The cuts alone, as a film pass traces them.
    fn cuts(&self, reverse: bool) -> Vec<Curve> {
        let mut curves: Vec<Curve> = self
            .spans
            .iter()
            .map(|&(a, b)| Curve::Line { start: self.at(a), end: self.at(b) })
            .collect();
        if reverse {
            curves.reverse();
            curves = curves.into_iter().map(|c| c.reversed()).collect();
        }
        curves
    }
}

/// Sorts intervals and joins those that overlap or touch.
fn merged(mut spans: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        match out.last_mut() {
            Some(last) if a <= last.1 + EPS => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Lengthens every cut by up to `overcut` at both ends, never leaving less
/// than [`MIN_WEB`] between neighbours.
fn overcut(spans: &mut [(f64, f64)], overcut: f64) {
    if overcut <= 0. {
        return;
    }
    let gaps: Vec<f64> = spans.windows(2).map(|w| w[1].0 - w[0].1).collect();
    let reach = |gap: f64| overcut.min(((gap - MIN_WEB) / 2.).max(0.));
    for i in 0..spans.len() {
        spans[i].0 -= if i == 0 { overcut } else { reach(gaps[i - 1]) };
        spans[i].1 += if i + 1 == spans.len() { overcut } else { reach(gaps[i]) };
    }
}

fn check_fly(fly: &FlyCut) -> Result<()> {
    if !(fly.run_up.is_finite() && (0. ..=50.).contains(&fly.run_up)) {
        return Err(feature("flying cut", "the run-up must be 0 to 50 mm"));
    }
    if !(fly.overcut.is_finite() && (0. ..=2.).contains(&fly.overcut)) {
        return Err(feature("flying cut", "the overcut must be 0 to 2 mm"));
    }
    if !(fly.tolerance.is_finite() && fly.tolerance > 0. && fly.tolerance <= 1.) {
        return Err(feature("flying cut", "the tolerance must be above 0 and at most 1 mm"));
    }
    if fly.contours.is_empty() {
        return Err(feature("flying cut", "select the contours to cut flying"));
    }
    Ok(())
}

/// Whether a contour can fly: every segment apart from its leads is a line.
fn straight(contour: &PreparedContour) -> bool {
    contour
        .segments
        .iter()
        .filter(|s| s.process.lead.is_none())
        .all(|s| matches!(s.curve, Curve::Line { .. }))
}

/// One gathered line under construction.
struct Group {
    layer: String,
    origin: Point,
    direction: Point,
    spans: Vec<(f64, f64)>,
    members: Vec<usize>,
}

/// The straight edges of the flying contours gathered into lines, by layer.
fn gather(contours: &[PreparedContour], flying: &[bool], tolerance: f64) -> Result<Vec<Group>> {
    let mut groups: Vec<Group> = Vec::new();
    let mut edges = 0usize;
    for (index, contour) in contours.iter().enumerate().filter(|(i, _)| flying[*i]) {
        for segment in &contour.segments {
            // Leads and joints stay uncut: a joint is a web the part needs.
            if segment.process.lead.is_some() || segment.process.joint {
                continue;
            }
            let Curve::Line { start, end } = segment.curve else { continue };
            let length = start.distance(end);
            if length <= EPS {
                continue;
            }
            edges += 1;
            if edges > MAX_EDGES {
                return Err(Error::Limit(format!(
                    "a flying cut gathers at most {MAX_EDGES} edges; select fewer contours"
                )));
            }
            let found = groups.iter().position(|g| {
                g.layer == contour.layer
                    && g.direction.cross(start - g.origin).abs() <= tolerance
                    && g.direction.cross(end - g.origin).abs() <= tolerance
            });
            let at = found.unwrap_or_else(|| {
                // One direction per line, in [0, π), so opposite edges share it.
                let mut direction = (end - start) * (1. / length);
                if direction.y < -EPS || (direction.y.abs() <= EPS && direction.x < 0.) {
                    direction = direction * -1.;
                }
                groups.push(Group {
                    layer: contour.layer.clone(),
                    origin: start,
                    direction,
                    spans: Vec::new(),
                    members: Vec::new(),
                });
                groups.len() - 1
            });
            let group = &mut groups[at];
            let (a, b) = (
                group.direction.dot(start - group.origin),
                group.direction.dot(end - group.origin),
            );
            group.spans.push((a.min(b), a.max(b)));
            if !group.members.contains(&index) {
                group.members.push(index);
            }
        }
    }
    Ok(groups)
}

/// One flying pass per gathered line: the lines of one direction side by
/// side, alternating unless every pass should run the same way, in the
/// order and from the side the settings choose.
fn passes(
    contours: &[PreparedContour],
    mut groups: Vec<Group>,
    fly: &FlyCut,
) -> Result<Vec<PreparedContour>> {
    let family = |g: &Group| (g.direction.angle() * 1000.).round();
    let offset = |g: &Group| g.direction.perpendicular().dot(g.origin);
    groups.sort_by(|a, b| {
        let by_family = family(a).total_cmp(&family(b));
        let by_family = if fly.directions_reversed { by_family.reverse() } else { by_family };
        let by_offset = offset(a).total_cmp(&offset(b));
        by_family.then(if fly.from_far_side { by_offset.reverse() } else { by_offset })
    });

    let mut lines = Vec::with_capacity(groups.len());
    let mut previous_family = None;
    let mut reverse = false;
    for mut group in groups {
        let f = family(&group);
        let mut spans = merged(std::mem::take(&mut group.spans));
        if let Some(web) = spans.windows(2).map(|w| w[1].0 - w[0].1).find(|gap| *gap < MIN_WEB) {
            return Err(feature(
                "flying cut",
                format!(
                    "two cuts on one line are only {web:.2} mm apart; \
                     a flying cut needs webs of at least {MIN_WEB} mm"
                ),
            ));
        }
        overcut(&mut spans, fly.overcut);
        if previous_family != Some(f) {
            reverse = false;
            previous_family = Some(f);
        }
        let line = Line { origin: group.origin, direction: group.direction, spans };
        let mut sources: Vec<usize> =
            group.members.iter().flat_map(|&i| contours[i].sources.iter().copied()).collect();
        sources.sort_unstable();
        sources.dedup();
        let depth = group.members.first().map_or(0, |&i| contours[i].depth);
        lines.push(PreparedContour {
            sources,
            lead_target: None,
            layer: group.layer,
            closed: false,
            depth,
            segments: line.segments(fly.run_up, reverse),
            film: line.cuts(reverse),
        });
        if !fly.one_way {
            reverse = !reverse;
        }
    }
    Ok(lines)
}

/// Replaces the selected straight contours by one flying pass per line.
pub(crate) fn fly(
    contours: Vec<PreparedContour>,
    fly: &FlyCut,
    warnings: &mut Vec<String>,
) -> Result<Vec<PreparedContour>> {
    check_fly(fly)?;
    let selected: Vec<bool> = contours
        .iter()
        .map(|c| !c.sources.is_empty() && c.sources.iter().all(|s| fly.contours.contains(s)))
        .collect();
    let flying: Vec<bool> =
        contours.iter().zip(&selected).map(|(c, &s)| s && straight(c)).collect();
    let curved = selected.iter().zip(&flying).filter(|(s, f)| **s && !**f).count();
    if curved > 0 {
        warnings.push(format!(
            "{curved} selected contour{} with arcs cut as usual: a flying cut takes straight edges only",
            if curved == 1 { "" } else { "s" }
        ));
    }
    let Some(first) = flying.iter().position(|&f| f) else {
        return Err(feature(
            "flying cut",
            "none of the selected contours is made of straight lines",
        ));
    };
    let groups = gather(&contours, &flying, fly.tolerance)?;
    let mut lines = passes(&contours, groups, fly)?;

    let mut out = Vec::with_capacity(contours.len() + lines.len());
    for (index, contour) in contours.into_iter().enumerate() {
        if index == first {
            out.append(&mut lines);
        }
        if !flying[index] {
            out.push(contour);
        }
    }
    Ok(out)
}

fn check_fill(fill: &Fill) -> Result<()> {
    if !(fill.spacing.is_finite() && (0.01..=20.).contains(&fill.spacing)) {
        return Err(feature("fill", "the line spacing must be 0.01 to 20 mm"));
    }
    if !fill.angle.is_finite() {
        return Err(feature("fill", "the angle must be a number"));
    }
    if !(fill.run_up.is_finite() && (0. ..=1000.).contains(&fill.run_up)) {
        return Err(feature("fill", "the overscan must be 0 to 1000 mm"));
    }
    if !(fill.scan_offset.is_finite() && (-2. ..=2.).contains(&fill.scan_offset)) {
        return Err(feature("fill", "the scan offset must be -2 to 2 mm"));
    }
    if !(1..=20).contains(&fill.passes) {
        return Err(feature("fill", "a fill has 1 to 20 passes"));
    }
    if !fill.pass_angle.is_finite() {
        return Err(feature("fill", "the angle between passes must be a number"));
    }
    if !(fill.pass_shift.is_finite() && (0. ..1.).contains(&fill.pass_shift)) {
        return Err(feature("fill", "the shift between passes must be 0 to under 100 %"));
    }
    Ok(())
}

/// The closed contour as a polygon, arcs flattened within [`FLATTEN`].
fn polygon(contour: &PreparedContour) -> Vec<Point> {
    let mut points = Vec::new();
    for segment in contour.segments.iter().filter(|s| s.process.lead.is_none()) {
        match segment.curve {
            Curve::Line { start, .. } => points.push(start),
            Curve::Arc { radius, sweep, .. } => {
                let step = if radius > FLATTEN {
                    2. * (1. - FLATTEN / radius).acos()
                } else {
                    sweep.abs()
                };
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "bounded below"
                )]
                let count = ((sweep.abs() / step.max(1e-3)).ceil() as usize).clamp(1, 4096);
                for k in 0..count {
                    points.push(segment.curve.point(float(k) / float(count)));
                }
            }
        }
    }
    points
}

/// The measured offset at a scan angle in degrees: linear between the
/// measured angles around the half turn, so 170° lies between 90° and 0°;
/// none when nothing was measured.
fn calibrated_at(table: &[[f64; 2]], angle: f64) -> f64 {
    let mut points: Vec<[f64; 2]> = table.iter().map(|&[a, o]| [a.rem_euclid(180.), o]).collect();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else {
        return 0.;
    };
    // The angles wrap: before the first comes the last, half a turn back,
    // and after the last the first, half a turn on.
    let mut ring = Vec::with_capacity(points.len() + 2);
    ring.push([last[0] - 180., last[1]]);
    ring.extend(points);
    ring.push([first[0] + 180., first[1]]);
    let a = angle.rem_euclid(180.);
    for pair in ring.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if a <= to[0] {
            let span = to[0] - from[0];
            return if span < 1e-9 {
                to[1]
            } else {
                from[1] + (to[1] - from[1]) * (a - from[0]) / span
            };
        }
    }
    last[1]
}

/// The scan lines across the polygons at `angle` degrees, `spacing` apart,
/// centred in their extent and then moved across by `shift` spacings, each
/// with the spans inside them.
fn scan(polygons: &[Vec<Point>], spacing: f64, angle: f64, shift: f64) -> Result<Vec<Line>> {
    let angle = angle.to_radians();
    // Scan along X in a frame turned by -angle; turn the result back.
    let turned: Vec<Vec<Point>> =
        polygons.iter().map(|p| p.iter().map(|q| q.rotated(-angle)).collect()).collect();
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in turned.iter().flatten() {
        low = low.min(p.y);
        high = high.max(p.y);
    }
    if !(low.is_finite() && high > low) {
        return Ok(Vec::new());
    }
    // Lines centred in the extent, half a spacing in from its edges.
    let count = ((high - low) / spacing).round().max(1.);
    if count > float(MAX_SCAN_LINES) {
        return Err(Error::Limit(format!(
            "the fill needs more than {MAX_SCAN_LINES} lines; widen the line spacing"
        )));
    }
    let first = low + (high - low - (count - 1.) * spacing) / 2. + shift * spacing;
    let mut lines = Vec::new();
    let mut k = 0.;
    while k < count {
        let y = first + k * spacing;
        k += 1.;
        let mut crossings = Vec::new();
        for polygon in &turned {
            for (i, a) in polygon.iter().enumerate() {
                let b = polygon[(i + 1) % polygon.len()];
                // Half-open in Y, so a vertex on the line counts once.
                if (a.y <= y && y < b.y) || (b.y <= y && y < a.y) {
                    crossings.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        let spans: Vec<(f64, f64)> = crossings
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[a, b]| (a, b))
            .filter(|(a, b)| b - a > EPS)
            .collect();
        if !spans.is_empty() {
            lines.push(Line {
                origin: Point::new(0., y).rotated(angle),
                direction: Point::direction(angle),
                spans: merged(spans),
            });
        }
    }
    Ok(lines)
}

/// Appends the lines to one continuous path: alternating when the fill is
/// bidirectional, every line the same way otherwise with the return laser
/// off, each line's marks moved back against its travel by the scan offset.
fn zigzag(
    lines: &[Line],
    fill: &Fill,
    offset: f64,
    segments: &mut Vec<PreparedSegment>,
    traced: &mut Vec<Curve>,
) {
    for (i, line) in lines.iter().enumerate() {
        let reverse = fill.bidirectional && i % 2 == 1;
        let back = if reverse { offset } else { -offset };
        let line = Line {
            origin: line.origin,
            direction: line.direction,
            spans: line.spans.iter().map(|&(a, b)| (a + back, b + back)).collect(),
        };
        let part = line.segments(fill.run_up, reverse);
        if let (Some(last), Some(next)) = (segments.last(), part.first()) {
            let (from, to) = (last.curve.end(), next.curve.start());
            if from.distance(to) > EPS {
                segments.push(PreparedSegment::new(Curve::Line { start: from, end: to }, off()));
            }
        }
        segments.extend(part);
        traced.extend(line.cuts(reverse));
    }
}

/// Fills the closed contours with scan lines, the beam on inside them, once
/// per pass, each pass turned and moved across from the one before. The
/// lines are joined into paths of at most [`LINES_PER_PATH`] lines, so the
/// motion planner's per-contour budget holds; every path enters at speed
/// with the beam off. Empty when no line crosses the shapes.
pub(crate) fn fill(shapes: &[&PreparedContour], fill: &Fill) -> Result<Vec<PreparedContour>> {
    check_fill(fill)?;
    let polygons: Vec<Vec<Point>> =
        shapes.iter().map(|c| polygon(c)).filter(|p| p.len() >= 3).collect();
    let mut sources: Vec<usize> = shapes.iter().flat_map(|c| c.sources.iter().copied()).collect();
    sources.sort_unstable();
    sources.dedup();
    let layer = shapes.first().map(|c| c.layer.clone()).unwrap_or_default();
    let mut paths = Vec::new();
    let mut total = 0usize;
    for pass in 0..fill.passes {
        let k = f64::from(pass);
        let shift = (k * fill.pass_shift).fract();
        let angle = fill.angle + k * fill.pass_angle;
        let offset = fill.scan_offset + calibrated_at(&fill.calibrated, angle);
        let lines = scan(&polygons, fill.spacing, angle, shift)?;
        for chunk in lines.chunks(LINES_PER_PATH) {
            let mut segments = Vec::new();
            let mut traced = Vec::new();
            zigzag(chunk, fill, offset, &mut segments, &mut traced);
            total += segments.len();
            if total > MAX_FILL_SEGMENTS {
                return Err(Error::Limit(format!(
                    "the fill makes more than {MAX_FILL_SEGMENTS} segments; widen the line spacing"
                )));
            }
            paths.push(PreparedContour {
                sources: sources.clone(),
                lead_target: None,
                layer: layer.clone(),
                closed: false,
                depth: 0,
                segments,
                film: traced,
            });
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(source: usize, x: f64, y: f64, w: f64, h: f64) -> PreparedContour {
        let p = [
            Point::new(x, y),
            Point::new(x + w, y),
            Point::new(x + w, y + h),
            Point::new(x, y + h),
        ];
        PreparedContour {
            sources: vec![source],
            lead_target: None,
            layer: "0".into(),
            closed: true,
            depth: 1,
            segments: (0..4)
                .map(|i| {
                    PreparedSegment::new(
                        Curve::Line { start: p[i], end: p[(i + 1) % 4] },
                        SegmentProcess::default(),
                    )
                })
                .collect(),
            film: Vec::new(),
        }
    }

    fn settings(contours: Vec<usize>) -> FlyCut {
        FlyCut {
            contours,
            run_up: 3.,
            overcut: 0.,
            tolerance: 0.01,
            one_way: false,
            from_far_side: false,
            directions_reversed: false,
        }
    }

    fn fill_of(spacing: f64, angle: f64, run_up: f64) -> Fill {
        Fill {
            spacing,
            angle,
            run_up,
            outline: false,
            bidirectional: true,
            scan_offset: 0.,
            passes: 1,
            pass_angle: 0.,
            pass_shift: 0.,
            calibrated: Vec::new(),
        }
    }

    /// The one path a small fill makes.
    fn one_path(mut paths: Vec<PreparedContour>) -> PreparedContour {
        assert_eq!(paths.len(), 1, "a small fill is one path");
        paths.remove(0)
    }

    /// Every path of a fill, one after the other, as one contour; each pass
    /// is a path of its own.
    fn all_paths(mut paths: Vec<PreparedContour>) -> PreparedContour {
        let mut first = paths.remove(0);
        for path in paths {
            first.segments.extend(path.segments);
            first.film.extend(path.film);
        }
        first
    }

    fn cut_length(c: &PreparedContour) -> f64 {
        c.segments.iter().filter(|s| !s.process.joint).map(|s| s.curve.length()).sum()
    }

    #[test]
    fn a_row_of_squares_flies_in_four_lines() {
        let row: Vec<_> = (0..5).map(|i| rect(i, float(i) * 15., 0., 10., 10.)).collect();
        let mut warnings = Vec::new();
        let out = fly(row, &settings((0..5).collect()), &mut warnings).unwrap();
        // Two horizontal lines, and five vertical pairs on ten vertical lines.
        assert_eq!(out.iter().filter(|c| matches!(c.segments[1].curve, Curve::Line { start, end } if (start.y - end.y).abs() < 1e-9)).count(), 2);
        assert_eq!(out.len(), 12);
        let total: f64 = out.iter().map(cut_length).sum();
        assert!((total - 200.).abs() < 1e-9, "every edge is cut once: {total}");
        assert!(warnings.is_empty());
        for line in &out {
            // Every line opens and closes with the beam off, so it never pierces standing.
            assert!(line.segments.first().unwrap().process.joint);
            assert!(line.segments.last().unwrap().process.joint);
            assert!(!line.closed);
        }
        let top = out.iter().find(|c| c.segments.len() == 11).unwrap();
        assert_eq!(top.segments.iter().filter(|s| !s.process.joint).count(), 5);
    }

    #[test]
    fn lines_of_one_direction_alternate() {
        let grid: Vec<_> = (0..2).map(|i| rect(i, 0., float(i) * 15., 10., 10.)).collect();
        let out = fly(grid, &settings(vec![0, 1]), &mut Vec::new()).unwrap();
        let horizontal: Vec<_> = out
            .iter()
            .filter(|c| matches!(c.segments[0].curve, Curve::Line { start, end } if (start.y - end.y).abs() < 1e-9))
            .collect();
        assert_eq!(horizontal.len(), 4);
        let dx = |c: &PreparedContour| c.segments[0].curve.end().x - c.segments[0].curve.start().x;
        assert!(dx(horizontal[0]) * dx(horizontal[1]) < 0.);
        assert!(dx(horizontal[1]) * dx(horizontal[2]) < 0.);
    }

    #[test]
    fn unselected_and_curved_contours_stay() {
        let mut circle = rect(2, 100., 0., 10., 10.);
        circle.segments = vec![PreparedSegment::new(
            Curve::circle(Point::new(105., 5.), 5.),
            SegmentProcess::default(),
        )];
        let contours = vec![rect(0, 0., 0., 10., 10.), rect(1, 50., 0., 10., 10.), circle];
        let mut warnings = Vec::new();
        let out = fly(contours, &settings(vec![0, 2]), &mut warnings).unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(out.iter().any(|c| c.sources == vec![1] && c.closed));
        assert!(out.iter().any(|c| c.sources == vec![2] && c.closed));
        assert_eq!(out.iter().filter(|c| !c.closed).count(), 4);
    }

    #[test]
    fn overcut_keeps_a_web() {
        let mut spans = vec![(0., 10.), (10.8, 20.)];
        overcut(&mut spans, 0.3);
        assert!((spans[0].0 + 0.3).abs() < 1e-12);
        assert!((spans[1].0 - spans[0].1 - MIN_WEB).abs() < 1e-12);
        assert!((spans[1].1 - 20.3).abs() < 1e-12);
    }

    #[test]
    fn webs_too_thin_to_fly_are_refused() {
        let contours = vec![rect(0, 0., 0., 10., 10.), rect(1, 10.2, 0., 10., 10.)];
        assert!(fly(contours, &settings(vec![0, 1]), &mut Vec::new()).is_err());
    }

    #[test]
    fn a_fill_scans_inside_shapes_and_skips_holes() {
        let outer = rect(0, 0., 0., 20., 20.);
        let hole = rect(1, 5., 5., 10., 10.);
        let fill_settings = fill_of(1., 0., 2.);
        let zigzag = fill(&[&outer, &hole], &fill_settings).map(one_path).unwrap();
        assert!(zigzag.segments.first().unwrap().process.joint);
        let cut = cut_length(&zigzag);
        // 20 lines at 0.5, 1.5 … 19.5; ten of them cross the hole.
        assert!((cut - (10. * 20. + 10. * 10.)).abs() < 1e-6, "cut {cut}");
        // One continuous path.
        for pair in zigzag.segments.windows(2) {
            assert!(pair[0].curve.end().distance(pair[1].curve.start()) < 1e-9);
        }
    }

    #[test]
    fn a_fill_turns_with_its_angle() {
        let square = rect(0, 0., 0., 10., 10.);
        let fill_settings = fill_of(0.5, 45., 0.);
        let zigzag = fill(&[&square], &fill_settings).map(one_path).unwrap();
        let s = zigzag.segments.iter().find(|s| !s.process.joint).unwrap();
        let d = s.curve.end() - s.curve.start();
        assert!((d.x.abs() - d.y.abs()).abs() < 1e-9);
        // Coverage: the scanned area over the spacing is the square's area.
        assert!((cut_length(&zigzag) * 0.5 - 100.).abs() < 2.);
    }

    fn hexagon(source: usize, center: Point, radius: f64) -> PreparedContour {
        let p: Vec<Point> = (0..6)
            .map(|k| center + Point::direction(std::f64::consts::FRAC_PI_3 * float(k)) * radius)
            .collect();
        PreparedContour {
            sources: vec![source],
            lead_target: None,
            layer: "0".into(),
            closed: true,
            depth: 1,
            segments: (0..6)
                .map(|i| {
                    PreparedSegment::new(
                        Curve::Line { start: p[i], end: p[(i + 1) % 6] },
                        SegmentProcess::default(),
                    )
                })
                .collect(),
            film: Vec::new(),
        }
    }

    /// A honeycomb of hexagonal holes flies in three directions: every
    /// edge lies on a line with the matching edges of other cells.
    #[test]
    fn a_honeycomb_flies_along_all_three_edge_directions() {
        let radius = 5.;
        let pitch = 3f64.sqrt() * radius + 2.;
        let a = Point::direction(std::f64::consts::FRAC_PI_6) * pitch;
        let b = Point::new(0., pitch);
        let mut cells = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                cells.push(hexagon(cells.len(), a * float(i) + b * float(j), radius));
            }
        }
        let out = fly(cells, &settings((0..9).collect()), &mut Vec::new()).unwrap();
        let total: f64 = out.iter().map(cut_length).sum();
        assert!((total - 9. * 6. * radius).abs() < 1e-6, "every edge once: {total}");
        let mut directions: Vec<i64> = out
            .iter()
            .map(|c| {
                let d = c.segments[0].curve.end() - c.segments[0].curve.start();
                #[allow(clippy::cast_possible_truncation, reason = "whole degrees")]
                let degrees = (d.y.atan2(d.x).to_degrees().rem_euclid(180.)).round() as i64;
                degrees % 180
            })
            .collect();
        directions.sort_unstable();
        directions.dedup();
        assert_eq!(directions, [0, 60, 120]);
        let joined =
            out.iter().filter(|c| c.segments.iter().filter(|s| !s.process.joint).count() > 1);
        assert!(joined.count() >= 6, "lines through several cells");
        assert!(out.len() < 9 * 6, "fewer passes than edges: {}", out.len());
    }

    fn horizontal(c: &PreparedContour) -> bool {
        let d = c.segments[0].curve.end() - c.segments[0].curve.start();
        d.y.abs() < 1e-9
    }

    #[test]
    fn the_order_and_direction_of_passes_can_be_chosen() {
        let grid = || (0..3).map(|i| rect(i, 0., float(i) * 15., 10., 10.)).collect::<Vec<_>>();
        let dx = |c: &PreparedContour| c.segments[0].curve.end().x - c.segments[0].curve.start().x;

        let one_way = FlyCut { one_way: true, ..settings((0..3).collect()) };
        let out = fly(grid(), &one_way, &mut Vec::new()).unwrap();
        assert!(out.iter().filter(|c| horizontal(c)).all(|c| dx(c) > 0.));

        let far = FlyCut { from_far_side: true, ..settings((0..3).collect()) };
        let out = fly(grid(), &far, &mut Vec::new()).unwrap();
        assert!((out[0].segments[0].curve.start().y - 40.).abs() < 1e-9, "the top line first");

        let flipped = FlyCut { directions_reversed: true, ..settings((0..3).collect()) };
        let out = fly(grid(), &flipped, &mut Vec::new()).unwrap();
        assert!(!horizontal(&out[0]), "vertical lines first");
        assert!(horizontal(out.last().unwrap()));
    }

    fn lit_directions(c: &PreparedContour) -> Vec<f64> {
        c.segments
            .iter()
            .filter(|s| !s.process.joint)
            .map(|s| (s.curve.end().x - s.curve.start().x).signum())
            .collect()
    }

    #[test]
    fn a_one_way_fill_engraves_every_line_the_same_way() {
        let square = rect(0, 0., 0., 10., 10.);
        let one_way = Fill { bidirectional: false, ..fill_of(1., 0., 2.) };
        let path = fill(&[&square], &one_way).map(one_path).unwrap();
        assert!(lit_directions(&path).iter().all(|d| *d > 0.));
        let both = fill(&[&square], &fill_of(1., 0., 2.)).map(one_path).unwrap();
        assert!(lit_directions(&both).iter().any(|d| *d < 0.));
    }

    #[test]
    fn a_crosshatch_scans_twice_at_right_angles() {
        let square = rect(0, 0., 0., 10., 10.);
        let single = fill(&[&square], &fill_of(0.5, 0., 1.)).map(one_path).unwrap();
        let cross = Fill { passes: 2, pass_angle: 90., ..fill_of(0.5, 0., 1.) };
        let double = fill(&[&square], &cross).map(all_paths).unwrap();
        assert!((cut_length(&double) - 2. * cut_length(&single)).abs() < 1e-6);
        assert!(double.segments.iter().any(|s| {
            let d = s.curve.end() - s.curve.start();
            !s.process.joint && d.x.abs() < 1e-9
        }));
    }

    #[test]
    fn the_scan_offset_moves_each_line_back_against_its_travel() {
        let square = rect(0, 0., 0., 10., 10.);
        let offset = Fill { scan_offset: 0.1, ..fill_of(1., 0., 2.) };
        let path = fill(&[&square], &offset).map(one_path).unwrap();
        let lit: Vec<_> = path.segments.iter().filter(|s| !s.process.joint).collect();
        // Forward: 0..10 becomes -0.1..9.9; backward: 10..0 becomes 10.1..0.1.
        assert!((lit[0].curve.start().x + 0.1).abs() < 1e-9);
        assert!((lit[0].curve.end().x - 9.9).abs() < 1e-9);
        assert!((lit[1].curve.start().x - 10.1).abs() < 1e-9);
        assert!((lit[1].curve.end().x - 0.1).abs() < 1e-9);
    }

    /// Lit lines' heights, for a fill scanned along X.
    fn heights(c: &PreparedContour) -> Vec<f64> {
        let mut ys: Vec<f64> =
            c.segments.iter().filter(|s| !s.process.joint).map(|s| s.curve.start().y).collect();
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        ys
    }

    #[test]
    fn passes_repeat_the_fill_or_lay_lines_between_the_last_ones() {
        let square = rect(0, 0., 0., 10., 10.);
        let single = fill(&[&square], &fill_of(1., 0., 1.)).map(one_path).unwrap();
        let triple = Fill { passes: 3, ..fill_of(1., 0., 1.) };
        let triple = fill(&[&square], &triple).map(all_paths).unwrap();
        assert!((cut_length(&triple) - 3. * cut_length(&single)).abs() < 1e-6);
        assert_eq!(heights(&triple), heights(&single), "the same lines again");

        let between = Fill { passes: 2, pass_shift: 0.5, ..fill_of(1., 0., 1.) };
        let between = fill(&[&square], &between).map(all_paths).unwrap();
        let ys = heights(&between);
        // 0.5, 1.0, 1.5 … 9.5: the second pass halfway between the first's lines.
        assert_eq!(ys.len(), 19);
        for pair in ys.windows(2) {
            assert!((pair[1] - pair[0] - 0.5).abs() < 1e-9);
        }
    }

    #[test]
    fn a_large_fill_splits_into_paths_that_each_enter_at_speed() {
        let square = rect(0, 0., 0., 100., 100.);
        let paths = fill(&[&square], &fill_of(0.1, 0., 2.)).unwrap();
        assert_eq!(paths.len(), 4, "1000 lines in paths of 250");
        for path in &paths {
            assert!(path.segments[0].process.joint);
            assert!(path.segments.last().unwrap().process.joint);
        }
        let lit: f64 = paths.iter().map(cut_length).sum();
        assert!((lit - 1000. * 100.).abs() < 1e-3);
    }

    #[test]
    fn calibrated_offsets_follow_the_scan_angle_round_the_half_turn() {
        assert!(calibrated_at(&[], 30.).abs() < 1e-12);
        assert!((calibrated_at(&[[0., 0.04]], 77.) - 0.04).abs() < 1e-12);
        let table = [[0., 0.02], [90., 0.06]];
        assert!((calibrated_at(&table, 0.) - 0.02).abs() < 1e-12);
        assert!((calibrated_at(&table, 45.) - 0.04).abs() < 1e-12);
        assert!((calibrated_at(&table, 90.) - 0.06).abs() < 1e-12);
        // 135° lies halfway from 90° back round to 180°, which is 0°.
        assert!((calibrated_at(&table, 135.) - 0.04).abs() < 1e-12);
        assert!((calibrated_at(&table, 270.) - 0.06).abs() < 1e-12);
    }

    #[test]
    fn a_fill_takes_the_offset_measured_for_its_angle() {
        let square = rect(0, 0., 0., 10., 10.);
        let measured = Fill { calibrated: vec![[0., 0.1], [90., 0.3]], ..fill_of(1., 0., 2.) };
        let path = fill(&[&square], &measured).map(one_path).unwrap();
        let lit: Vec<_> = path.segments.iter().filter(|s| !s.process.joint).collect();
        assert!((lit[0].curve.start().x + 0.1).abs() < 1e-9, "0° takes 0.1 mm");
        let vertical = Fill { angle: 90., ..measured };
        let path = fill(&[&square], &vertical).map(one_path).unwrap();
        let lit: Vec<_> = path.segments.iter().filter(|s| !s.process.joint).collect();
        assert!((lit[0].curve.start().y + 0.3).abs() < 1e-9, "90° takes 0.3 mm");
    }
}
