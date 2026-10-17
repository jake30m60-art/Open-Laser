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
//!
//! Flying circles and contours take whole closed contours instead: each
//! opens from a laser-off approach at cutting speed, runs once round and a
//! little past its start, and leaves on a laser-off run-out to the next. A
//! circle is entered on the tangent that points along its row, so the
//! circles of a row join in one smooth movement, row by row in a snake.

use crate::{EPS, Error, Result, feature, float};
use openlaser_core::features::{Fill, FlyCut, ImageFill, ImageMode};
use openlaser_core::geometry::{Bounds, Curve, Point};
use openlaser_core::raster::Raster;
use openlaser_core::toolpath::{PreparedContour, PreparedSegment, SegmentProcess};
use std::collections::HashMap;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::sync::Arc;

/// The smallest uncut web between two cuts on one line, in millimetres.
/// Below it the beam's switching at speed could cut through.
pub const MIN_WEB: f64 = 0.5;
/// The most edges a flying cut gathers.
const MAX_EDGES: usize = 50_000;
/// The most scan lines one fill makes.
const MAX_SCAN_LINES: usize = 100_000;
/// The most segments one fill makes.
const MAX_FILL_SEGMENTS: usize = 400_000;
/// The most image cells one fill engraves, all passes together.
const MAX_IMAGE_CELLS: u32 = 60_000_000;
/// The most scan lines joined into one path. Every turn costs the motion
/// planner two of its 2048 profile builds per contour.
const LINES_PER_PATH: usize = 250;
/// Chord tolerance when an arc is flattened for scanning, in millimetres.
const FLATTEN: f64 = 0.005;
/// The most segments one path of flying circles or contours holds, so the
/// motion planner's 2048 profile builds per contour hold.
const LOOP_SEGMENTS: usize = 400;
/// The widest turn, in radians, a laser-off link between two flying loops
/// takes straight from one to the next without its own run-out and run-up.
const SMOOTH_TURN: f64 = 0.5;

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
    /// An engraved image's power along each span, from its start; empty
    /// when the spans are cut at the layer's power.
    levels: Vec<Arc<[u8]>>,
}

impl Line {
    fn at(&self, t: f64) -> Point {
        self.origin + self.direction * t
    }

    /// The run-up, the cuts and gaps, and the run-out, forwards or reversed.
    fn segments(&self, run_up: f64, reverse: bool) -> Vec<PreparedSegment> {
        // Each piece: from, to, and the span it cuts, if any.
        let mut pieces: Vec<(f64, f64, Option<usize>)> =
            Vec::with_capacity(self.spans.len() * 2 + 1);
        let (first, last) = (self.spans[0].0, self.spans[self.spans.len() - 1].1);
        if run_up > 0. {
            pieces.push((first - run_up, first, None));
        }
        for (i, &(a, b)) in self.spans.iter().enumerate() {
            if i > 0 {
                pieces.push((self.spans[i - 1].1, a, None));
            }
            pieces.push((a, b, Some(i)));
        }
        if run_up > 0. {
            pieces.push((last, last + run_up, None));
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
            .map(|(a, b, span)| {
                let curve = Curve::Line { start: self.at(a), end: self.at(b) };
                let process = match span {
                    None => off(),
                    Some(i) => SegmentProcess {
                        levels: self.levels.get(i).map(|levels| {
                            if reverse {
                                levels.iter().rev().copied().collect()
                            } else {
                                levels.clone()
                            }
                        }),
                        ..SegmentProcess::default()
                    },
                };
                PreparedSegment::new(curve, process)
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
    if !(fly.overlap.is_finite() && (0. ..=5.).contains(&fly.overlap)) {
        return Err(feature("flying cut", "the overlap must be 0 to 5 mm"));
    }
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
        let line =
            Line { origin: group.origin, direction: group.direction, spans, levels: Vec::new() };
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

/// The cut segments of a closed contour, its leads left out; none when it
/// does not close.
fn cut_loop(contour: &PreparedContour) -> Option<Vec<PreparedSegment>> {
    let segments: Vec<PreparedSegment> =
        contour.segments.iter().filter(|s| s.process.lead.is_none()).cloned().collect();
    let (first, last) = (segments.first()?, segments.last()?);
    let closes = first.curve.start().distance(last.curve.end()) <= 1e-3;
    (contour.closed && closes).then_some(segments)
}

/// The centre, radius and turning sense of a contour that is one full
/// circle, whole or in arcs, with no joints.
fn circle_of(segments: &[PreparedSegment]) -> Option<(Point, f64, f64)> {
    let Curve::Arc { center, radius, sweep, .. } = segments.first()?.curve else {
        return None;
    };
    let mut total = 0.;
    for segment in segments {
        let Curve::Arc { center: c, radius: r, sweep: s, .. } = segment.curve else {
            return None;
        };
        if segment.process.joint
            || c.distance(center) > 1e-4
            || (r - radius).abs() > 1e-4
            || s.signum() != sweep.signum()
        {
            return None;
        }
        total += s;
    }
    ((total.abs() - TAU).abs() < 1e-6).then_some((center, radius, sweep.signum()))
}

/// A selected closed contour cut flying.
struct Loop {
    index: usize,
    /// Where it sits for ordering into rows, and half its height.
    middle: Point,
    half_height: f64,
    /// A circle's centre, radius and sense; none for any other contour.
    circle: Option<(Point, f64, f64)>,
    segments: Vec<PreparedSegment>,
}

impl Loop {
    /// The cut, entering a circle where it runs along `along` and any other
    /// contour at its own start, once round and on by `overlap`.
    fn cut(&self, along: Point, overlap: f64) -> Vec<PreparedSegment> {
        if let Some((center, radius, sense)) = self.circle {
            let process = SegmentProcess { repierce: false, ..self.segments[0].process.clone() };
            let start_angle = along.angle() - sense * FRAC_PI_2;
            let round = Curve::Arc { center, radius, start_angle, sweep: sense * TAU };
            let mut cut = vec![PreparedSegment::new(round, process.clone())];
            // At most a quarter turn, so the overlap never retraces much.
            let extra = overlap.min(radius * FRAC_PI_2) / radius;
            if extra * radius > EPS {
                let on = Curve::Arc { center, radius, start_angle, sweep: sense * extra };
                cut.push(PreparedSegment::new(on, process));
            }
            return cut;
        }
        let mut cut = self.segments.clone();
        let mut left = overlap;
        for segment in &self.segments {
            if left <= EPS {
                break;
            }
            let length = segment.curve.length();
            if length <= EPS {
                continue;
            }
            cut.push(if length <= left {
                segment.clone()
            } else {
                segment.slice(0., left / length)
            });
            left -= length;
        }
        if let Some(first) = cut.first_mut() {
            first.process.repierce = false;
        }
        cut
    }
}

/// The loops in rows across Y, each row along X, alternating unless every
/// pass should run the same way, so the head snakes from row to row.
/// Returns each loop with the direction its row runs.
fn rows(mut loops: Vec<Loop>, fly: &FlyCut) -> Vec<(Loop, Point)> {
    loops.sort_by(|a, b| a.middle.y.total_cmp(&b.middle.y));
    if fly.from_far_side {
        loops.reverse();
    }
    let mut rows: Vec<Vec<Loop>> = Vec::new();
    for item in loops {
        match rows.last_mut() {
            Some(row)
                if (item.middle.y - row[0].middle.y).abs()
                    <= item.half_height.min(row[0].half_height) =>
            {
                row.push(item);
            }
            _ => rows.push(vec![item]),
        }
    }
    let mut out = Vec::new();
    for (i, mut row) in rows.into_iter().enumerate() {
        let forward = (fly.one_way || i % 2 == 0) != fly.directions_reversed;
        row.sort_by(|a, b| a.middle.x.total_cmp(&b.middle.x));
        if !forward {
            row.reverse();
        }
        let along = Point::new(if forward { 1. } else { -1. }, 0.);
        out.extend(row.into_iter().map(|item| (item, along)));
    }
    out
}

/// Appends a laser-off line at cutting speed, if it has length.
fn off_line(segments: &mut Vec<PreparedSegment>, start: Point, end: Point) {
    if start.distance(end) > EPS {
        segments.push(PreparedSegment::new(Curve::Line { start, end }, off()));
    }
}

/// The angle between two directions, in radians.
fn turn(a: Point, b: Point) -> f64 {
    a.cross(b).atan2(a.dot(b)).abs()
}

/// One flying path under construction.
#[derive(Default)]
struct Path {
    segments: Vec<PreparedSegment>,
    film: Vec<Curve>,
    members: Vec<usize>,
}

impl Path {
    /// The finished path, with its run-out; none when it is empty.
    fn finish(&mut self, contours: &[PreparedContour], run_up: f64) -> Option<PreparedContour> {
        let last = self.segments.last()?;
        let (end, tangent) = (last.curve.end(), last.curve.tangent(1.));
        off_line(&mut self.segments, end, end + tangent * run_up);
        let mut sources: Vec<usize> =
            self.members.iter().flat_map(|&i| contours[i].sources.iter().copied()).collect();
        sources.sort_unstable();
        sources.dedup();
        let first = &contours[self.members[0]];
        let path = std::mem::take(self);
        Some(PreparedContour {
            sources,
            lead_target: None,
            layer: first.layer.clone(),
            closed: false,
            depth: first.depth,
            segments: path.segments,
            film: path.film,
        })
    }
}

/// Flying paths through the loops in order: a run-up into the first, each
/// cut joined to the next by a laser-off link, straight when the turns are
/// gentle and through a run-out and a run-up otherwise, and a run-out after
/// the last. A path holds at most [`LOOP_SEGMENTS`] segments.
fn loop_paths(
    contours: &[PreparedContour],
    ordered: Vec<(Loop, Point)>,
    fly: &FlyCut,
) -> Vec<PreparedContour> {
    let mut paths = Vec::new();
    let mut path = Path::default();
    for (item, along) in ordered {
        let cut = item.cut(along, fly.overlap);
        if path.segments.len() + cut.len() + 4 > LOOP_SEGMENTS {
            paths.extend(path.finish(contours, fly.run_up));
        }
        let (entry, heading) = (cut[0].curve.start(), cut[0].curve.tangent(0.));
        match path.segments.last() {
            None => off_line(&mut path.segments, entry - heading * fly.run_up, entry),
            Some(last) => {
                let (exit, leaving) = (last.curve.end(), last.curve.tangent(1.));
                let link = entry - exit;
                let gentle = exit.distance(entry) > EPS
                    && turn(leaving, link) <= SMOOTH_TURN
                    && turn(link, heading) <= SMOOTH_TURN;
                if gentle {
                    off_line(&mut path.segments, exit, entry);
                } else {
                    let out = exit + leaving * fly.run_up;
                    let into = entry - heading * fly.run_up;
                    off_line(&mut path.segments, exit, out);
                    off_line(&mut path.segments, out, into);
                    off_line(&mut path.segments, into, entry);
                }
            }
        }
        path.film.extend(item.segments.iter().map(|s| s.curve));
        path.segments.extend(cut);
        path.members.push(item.index);
    }
    paths.extend(path.finish(contours, fly.run_up));
    paths
}

/// What a selected contour becomes in a flying cut.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fate {
    /// Not selected.
    Stays,
    /// Selected, but cut as usual.
    Curved,
    Line,
    Loop,
}

/// Replaces the selected straight contours by one flying pass per line,
/// and, when chosen, the selected circles and other closed contours by
/// flying paths that never stop to pierce.
pub(crate) fn fly(
    contours: Vec<PreparedContour>,
    fly: &FlyCut,
    marked: &[&str],
    warnings: &mut Vec<String>,
) -> Result<Vec<PreparedContour>> {
    check_fly(fly)?;
    let mut loops = Vec::new();
    let fates: Vec<Fate> = contours
        .iter()
        .enumerate()
        .map(|(index, contour)| {
            // Marked layers engrave or trace: a flying cut leaves them as they are.
            let selected = !contour.sources.is_empty()
                && !marked.contains(&contour.layer.as_str())
                && contour.sources.iter().all(|s| fly.contours.contains(s));
            if !selected {
                return Fate::Stays;
            }
            let whole = cut_loop(contour);
            let circle = whole.as_deref().and_then(circle_of);
            let takes = if circle.is_some() {
                fly.circles || fly.outlines
            } else {
                fly.outlines && !straight(contour)
            };
            match whole {
                Some(segments) if takes => {
                    let bounds = segments
                        .iter()
                        .map(|s| s.curve.bounds())
                        .reduce(Bounds::union)
                        .unwrap_or(Bounds::of(Point::new(0., 0.)));
                    loops.push(Loop {
                        index,
                        middle: bounds.center(),
                        half_height: bounds.height() / 2.,
                        circle,
                        segments,
                    });
                    Fate::Loop
                }
                _ if straight(contour) => Fate::Line,
                _ => Fate::Curved,
            }
        })
        .collect();
    let curved = fates.iter().filter(|&&f| f == Fate::Curved).count();
    if curved > 0 {
        warnings.push(format!(
            "{curved} selected contour{} with arcs cut as usual: \
             switch on flying circles or contours to fly {}",
            if curved == 1 { "" } else { "s" },
            if curved == 1 { "it" } else { "them" },
        ));
    }
    let Some(first) = fates.iter().position(|&f| matches!(f, Fate::Line | Fate::Loop)) else {
        return Err(feature(
            "flying cut",
            "none of the selected contours can fly: \
             switch on flying circles or contours for shapes with arcs",
        ));
    };
    let lining: Vec<bool> = fates.iter().map(|&f| f == Fate::Line).collect();
    let groups = gather(&contours, &lining, fly.tolerance)?;
    let mut paths = passes(&contours, groups, fly)?;
    // Each layer and depth flies on its own path.
    while let Some(item) = loops.first() {
        let key = (contours[item.index].layer.clone(), contours[item.index].depth);
        let (these, rest): (Vec<Loop>, Vec<Loop>) = loops.into_iter().partition(|item| {
            contours[item.index].layer == key.0 && contours[item.index].depth == key.1
        });
        loops = rest;
        paths.extend(loop_paths(&contours, rows(these, fly), fly));
    }
    // Holes before the outlines round them, so no part drops early.
    paths.sort_by_key(|p| std::cmp::Reverse(p.depth));

    let mut out = Vec::with_capacity(contours.len() + paths.len());
    for (index, contour) in contours.into_iter().enumerate() {
        if index == first {
            out.append(&mut paths);
        }
        if matches!(fates[index], Fate::Stays | Fate::Curved) {
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
                levels: Vec::new(),
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
            levels: line.levels.clone(),
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

fn check_image(image: &ImageFill) -> Result<()> {
    let percent = |v: f64| v.is_finite() && (0. ..=100.).contains(&v);
    if !percent(image.min_power) || !percent(image.max_power) || image.min_power > image.max_power {
        return Err(feature(
            "image",
            "the powers must be 0 to 100 %, the least no more than the most",
        ));
    }
    if !(image.gamma.is_finite() && (0.1..=10.).contains(&image.gamma)) {
        return Err(feature("image", "the gamma must be 0.1 to 10"));
    }
    Ok(())
}

/// How dark the image is at a gray level, 0 to 1, after inversion and
/// gamma: what the beam should do there.
fn darkness(gray: f64, image: &ImageFill) -> f64 {
    let dark = if image.invert { gray } else { 1. - gray };
    dark.clamp(0., 1.).powf(image.gamma)
}

/// A power level in whole percent.
fn percent(value: f64) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped")]
    let level = value.round().clamp(0., 100.) as u8;
    level
}

/// Turns the lines' spans into image cells `pitch` long, aligned across
/// the lines, each with the power the image calls for under its middle.
/// Cells with no power at either end of a span are dropped, and so is a span
/// with none at all. Dithering carries each cell's error to its right and
/// to the next line, the lines taken in order. Returns how many cells.
fn shade(lines: &mut [Line], raster: &Raster, image: &ImageFill, pitch: f64) -> usize {
    let (low, high) = (image.min_power, image.max_power);
    let mut cells = 0;
    let mut carry: HashMap<i64, f64> = HashMap::new();
    for line in lines.iter_mut() {
        let mut next: HashMap<i64, f64> = HashMap::new();
        let mut spans = Vec::with_capacity(line.spans.len());
        let mut levels = Vec::with_capacity(line.spans.len());
        for &(a, b) in &line.spans {
            #[allow(clippy::cast_possible_truncation, reason = "bounded by the scan's length")]
            let (first, end) = ((a / pitch).ceil() as i64, (b / pitch).floor() as i64);
            if end <= first {
                continue;
            }
            let mut row: Vec<u8> = Vec::new();
            for k in first..end {
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_wrap,
                    reason = "cell indices stay small"
                )]
                let middle = line.at((k as f64 + 0.5) * pitch);
                let dark = darkness(raster.gray_at(middle).unwrap_or(1.), image);
                row.push(match image.mode {
                    ImageMode::Grayscale if dark < 0.01 => 0,
                    ImageMode::Grayscale => percent(low + (high - low) * dark),
                    ImageMode::Threshold => {
                        if dark >= 0.5 {
                            percent(high)
                        } else {
                            0
                        }
                    }
                    ImageMode::Dither => {
                        let value = dark + carry.remove(&k).unwrap_or(0.);
                        let on = value >= 0.5;
                        let error = value - if on { 1. } else { 0. };
                        *carry.entry(k + 1).or_default() += error * 7. / 16.;
                        *next.entry(k - 1).or_default() += error * 3. / 16.;
                        *next.entry(k).or_default() += error * 5. / 16.;
                        *next.entry(k + 1).or_default() += error / 16.;
                        if on { percent(high) } else { 0 }
                    }
                });
            }
            let (Some(from), Some(to)) =
                (row.iter().position(|&l| l > 0), row.iter().rposition(|&l| l > 0))
            else {
                continue;
            };
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_wrap,
                reason = "cell indices stay small"
            )]
            let at = |cell: usize| (first + cell as i64) as f64 * pitch;
            spans.push((at(from), at(to + 1)));
            cells += to + 1 - from;
            levels.push(Arc::from(&row[from..=to]));
        }
        line.spans = spans;
        line.levels = levels;
        carry = next;
    }
    cells
}

/// Fills the closed contours with scan lines, the beam on inside them, once
/// per pass, each pass turned and moved across from the one before. The
/// lines are joined into paths of at most [`LINES_PER_PATH`] lines, so the
/// motion planner's per-contour budget holds; every path enters at speed
/// with the beam off. Empty when no line crosses the shapes.
///
pub(crate) fn fill(shapes: &[&PreparedContour], fill: &Fill) -> Result<Vec<PreparedContour>> {
    fill_framing(shapes, fill, None)
}

/// [`fill`], the shapes framing `image` when there is one: every line is
/// shaded cell by cell, the cells as long as the lines are apart, and only
/// where the image calls for power.
pub(crate) fn fill_framing(
    shapes: &[&PreparedContour],
    fill: &Fill,
    image: Option<&Raster>,
) -> Result<Vec<PreparedContour>> {
    check_fill(fill)?;
    let shading = fill.image.unwrap_or_default();
    if image.is_some() {
        check_image(&shading)?;
    }
    let mut cells = 0.;
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
        let mut lines = scan(&polygons, fill.spacing, angle, shift)?;
        if let Some(raster) = image {
            let requested: f64 = lines
                .iter()
                .flat_map(|l| &l.spans)
                .map(|&(a, b)| ((b / fill.spacing).floor() - (a / fill.spacing).ceil()).max(0.))
                .sum();
            if requested + cells > f64::from(MAX_IMAGE_CELLS) {
                return Err(Error::Limit(format!(
                    "the image needs more than {MAX_IMAGE_CELLS} cells; widen the line spacing"
                )));
            }
            cells += requested;
            shade(&mut lines, raster, &shading, fill.spacing);
            lines.retain(|line| !line.spans.is_empty());
        }
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

    #[allow(clippy::many_single_char_names, reason = "a rectangle is x, y, w, h")]
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
            circles: false,
            outlines: false,
            overlap: 0.3,
        }
    }

    fn fill_of(spacing: f64, angle: f64, run_up: f64) -> Fill {
        Fill {
            spacing,
            angle,
            run_up,
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
        let out = fly(row, &settings((0..5).collect()), &[], &mut warnings).unwrap();
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
        let out = fly(grid, &settings(vec![0, 1]), &[], &mut Vec::new()).unwrap();
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
        let out = fly(contours, &settings(vec![0, 2]), &[], &mut warnings).unwrap();
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
        assert!(fly(contours, &settings(vec![0, 1]), &[], &mut Vec::new()).is_err());
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
        let out = fly(cells, &settings((0..9).collect()), &[], &mut Vec::new()).unwrap();
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
        let out = fly(grid(), &one_way, &[], &mut Vec::new()).unwrap();
        assert!(out.iter().filter(|c| horizontal(c)).all(|c| dx(c) > 0.));

        let far = FlyCut { from_far_side: true, ..settings((0..3).collect()) };
        let out = fly(grid(), &far, &[], &mut Vec::new()).unwrap();
        assert!((out[0].segments[0].curve.start().y - 40.).abs() < 1e-9, "the top line first");

        let flipped = FlyCut { directions_reversed: true, ..settings((0..3).collect()) };
        let out = fly(grid(), &flipped, &[], &mut Vec::new()).unwrap();
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

    #[test]
    fn marked_layers_stay_out_of_a_flying_cut() {
        let mut cells: Vec<_> = (0..3).map(|i| rect(i, 0., float(i) * 15., 10., 10.)).collect();
        for c in &mut cells {
            c.layer = "Mark".into();
        }
        // Every selected contour is on a marked layer: nothing can fly.
        assert!(fly(cells, &settings(vec![0, 1]), &["Mark"], &mut Vec::new()).is_err());
    }

    /// A 20 by 10 mm image, black on the left half and white on the right.
    fn halves() -> Raster {
        Raster::upright(2, 1, vec![0, 255], [20., 10.]).unwrap()
    }

    /// The lit segments of the paths, with their levels.
    fn lit(paths: &[PreparedContour]) -> Vec<&PreparedSegment> {
        paths.iter().flat_map(|p| &p.segments).filter(|s| !s.process.joint).collect()
    }

    #[test]
    fn an_image_shades_its_lines_and_skips_the_white() {
        let frame = rect(0, 0., 0., 20., 10.);
        let paths = fill_framing(&[&frame], &fill_of(1., 0., 2.), Some(&halves())).unwrap();
        let lit = lit(&paths);
        assert_eq!(lit.len(), 10, "one lit stretch per line");
        for segment in &lit {
            let levels = segment.process.levels.as_ref().expect("levels");
            let (a, b) = (segment.curve.start().x, segment.curve.end().x);
            let (left, right) = (a.min(b), a.max(b));
            assert!(left.abs() < 1e-9, "starts at the black edge");
            // Bilinear gray reaches white at the right pixel's middle, 15 mm.
            assert!((14. ..=15.).contains(&right), "ends before the white: {right}");
            #[allow(clippy::cast_precision_loss, reason = "a short list")]
            let cells = levels.len() as f64;
            assert!((cells - (right - left)).abs() < 1e-9, "one cell per millimetre");
            let first = if a < b { levels[0] } else { levels[levels.len() - 1] };
            assert_eq!(first, 100, "black is full power");
            assert!(levels.iter().all(|&l| (10..=100).contains(&l)));
        }
    }

    #[test]
    fn a_mirrored_image_engraves_mirrored() {
        let frame = rect(0, -20., 0., 20., 10.);
        let mirrored =
            halves().transformed(&openlaser_core::geometry::Transform([-1., 0., 0., 1., 0., 0.]));
        let paths = fill_framing(&[&frame], &fill_of(1., 0., 2.), Some(&mirrored)).unwrap();
        for segment in lit(&paths) {
            let (a, b) = (segment.curve.start().x, segment.curve.end().x);
            assert!(a.max(b).abs() < 1e-9, "the black now lies on the right");
        }
    }

    #[test]
    fn dithering_spreads_dots_for_a_mid_gray() {
        let frame = rect(0, 0., 0., 20., 20.);
        let gray = Raster::upright(1, 1, vec![128], [20., 20.]).unwrap();
        let settings = Fill {
            image: Some(ImageFill { mode: ImageMode::Dither, ..ImageFill::default() }),
            ..fill_of(0.5, 0., 2.)
        };
        let paths = fill_framing(&[&frame], &settings, Some(&gray)).unwrap();
        let levels: Vec<u8> =
            lit(&paths).iter().flat_map(|s| s.process.levels.as_ref().unwrap().to_vec()).collect();
        assert!(levels.iter().all(|&l| l == 0 || l == 100), "dots are on or off");
        #[allow(clippy::naive_bytecount, reason = "a test count, not a hot path")]
        let on = levels.iter().filter(|&&l| l == 100).count();
        // About half of the 1600 cells; trimming drops some off cells.
        assert!((700..=900).contains(&on), "{on} dots on");
    }

    #[test]
    fn bad_image_settings_are_refused() {
        let frame = rect(0, 0., 0., 20., 10.);
        let image = |min_power, max_power, gamma| Fill {
            image: Some(ImageFill { min_power, max_power, gamma, ..ImageFill::default() }),
            ..fill_of(1., 0., 2.)
        };
        for settings in [image(60., 40., 1.), image(0., 120., 1.), image(0., 100., 0.)] {
            assert!(fill_framing(&[&frame], &settings, Some(&halves())).is_err());
        }
    }

    fn hole(source: usize, center: Point, radius: f64, curves: Vec<Curve>) -> PreparedContour {
        let curves = if curves.is_empty() { vec![Curve::circle(center, radius)] } else { curves };
        PreparedContour {
            sources: vec![source],
            lead_target: None,
            layer: "0".into(),
            closed: true,
            depth: 1,
            segments: curves
                .into_iter()
                .map(|c| PreparedSegment::new(c, SegmentProcess::default()))
                .collect(),
            film: Vec::new(),
        }
    }

    fn grid_of_holes(columns: usize, rows: usize, radius: f64) -> Vec<PreparedContour> {
        let mut holes = Vec::new();
        for j in 0..rows {
            for i in 0..columns {
                let center = Point::new(float(i) * 10., float(j) * 10.);
                holes.push(hole(holes.len(), center, radius, Vec::new()));
            }
        }
        holes
    }

    fn circles(contours: Vec<usize>) -> FlyCut {
        FlyCut { circles: true, ..settings(contours) }
    }

    /// The path runs on without a gap and only ever enters a cut from a
    /// laser-off stretch or another cut, never from a standstill.
    fn flies_through(path: &PreparedContour) {
        assert!(!path.closed);
        assert!(path.segments.first().unwrap().process.joint, "enters at speed");
        assert!(path.segments.last().unwrap().process.joint, "leaves at speed");
        for pair in path.segments.windows(2) {
            assert!(pair[0].curve.end().distance(pair[1].curve.start()) < 1e-9, "continuous");
        }
    }

    #[test]
    fn a_grid_of_holes_flies_in_one_snake() {
        let radius = 2.;
        let out =
            fly(grid_of_holes(3, 3, radius), &circles((0..9).collect()), &[], &mut Vec::new())
                .unwrap();
        assert_eq!(out.len(), 1, "one path through all nine holes");
        let path = &out[0];
        flies_through(path);
        let cut = cut_length(path);
        assert!(
            (cut - 9. * (TAU * radius + 0.3)).abs() < 1e-6,
            "once round and the overlap: {cut}"
        );
        let arcs: Vec<_> = path.segments.iter().filter(|s| !s.process.joint).collect();
        assert!(arcs.iter().all(|s| matches!(s.curve, Curve::Arc { .. })));
        // Each circle opens on the tangent along its row: the first row
        // runs +X along the bottoms, the second -X along the tops.
        let entries: Vec<_> = arcs.iter().step_by(2).collect();
        assert!((entries[0].curve.tangent(0.).x - 1.).abs() < 1e-9);
        assert!((entries[0].curve.start().y + radius).abs() < 1e-9);
        assert!((entries[3].curve.tangent(0.).x + 1.).abs() < 1e-9);
        assert!((entries[3].curve.start().y - (10. + radius)).abs() < 1e-9);
        assert!(entries[3].curve.start().x > entries[4].curve.start().x, "snakes back");
        // Within a row the head goes straight on from one hole to the next.
        let links = path.segments.iter().filter(|s| s.process.joint).count();
        assert_eq!(links, 2 + 6 + 2 * 3, "run-up, six links, two turns of three, run-out");
    }

    #[test]
    fn a_hole_drawn_in_arcs_flies_as_a_circle() {
        let center = Point::new(5., 5.);
        let halves = vec![
            Curve::Arc { center, radius: 3., start_angle: 0., sweep: std::f64::consts::PI },
            Curve::Arc {
                center,
                radius: 3.,
                start_angle: std::f64::consts::PI,
                sweep: std::f64::consts::PI,
            },
        ];
        let out = fly(vec![hole(0, center, 3., halves)], &circles(vec![0]), &[], &mut Vec::new())
            .unwrap();
        assert_eq!(out.len(), 1);
        flies_through(&out[0]);
        assert!((cut_length(&out[0]) - (TAU * 3. + 0.3)).abs() < 1e-6);
    }

    #[test]
    fn circles_stay_unless_chosen() {
        let mut warnings = Vec::new();
        assert!(fly(grid_of_holes(2, 1, 2.), &settings(vec![0, 1]), &[], &mut warnings).is_err());
    }

    /// A 20 by 10 mm slot with round ends, starting on its bottom edge.
    fn slot(source: usize) -> PreparedContour {
        let (a, b) = (Point::new(0., 0.), Point::new(20., 0.));
        let curves = vec![
            Curve::Line { start: a, end: b },
            Curve::Arc {
                center: Point::new(20., 5.),
                radius: 5.,
                start_angle: -FRAC_PI_2,
                sweep: std::f64::consts::PI,
            },
            Curve::Line { start: Point::new(20., 10.), end: Point::new(0., 10.) },
            Curve::Arc {
                center: Point::new(0., 5.),
                radius: 5.,
                start_angle: FRAC_PI_2,
                sweep: std::f64::consts::PI,
            },
        ];
        hole(source, Point::new(10., 5.), 5., curves)
    }

    #[test]
    fn contours_with_arcs_fly_with_an_overlap() {
        let fly_settings = FlyCut { outlines: true, overlap: 1., ..settings(vec![0, 1]) };
        let mut second = slot(1);
        for s in &mut second.segments {
            s.curve = s.curve.translated(Point::new(0., 30.));
        }
        let mut warnings = Vec::new();
        let out = fly(vec![slot(0), second], &fly_settings, &[], &mut warnings).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(out.len(), 1);
        flies_through(&out[0]);
        let perimeter = 40. + TAU * 5.;
        assert!((cut_length(&out[0]) - 2. * (perimeter + 1.)).abs() < 1e-6);
        // The overlap runs on along the first edge past the start.
        let lit: Vec<_> = out[0].segments.iter().filter(|s| !s.process.joint).collect();
        assert!((lit[4].curve.end().distance(Point::new(1., 0.))) < 1e-9);
        assert!((lit[0].curve.start().distance(Point::new(0., 0.))) < 1e-9);
    }

    #[test]
    fn holes_fly_before_the_outline_round_them() {
        let mut outer = hole(0, Point::new(0., 0.), 20., Vec::new());
        outer.depth = 0;
        let inner = hole(1, Point::new(0., 0.), 3., Vec::new());
        let both = FlyCut { outlines: true, ..settings(vec![0, 1]) };
        let out = fly(vec![outer, inner], &both, &[], &mut Vec::new()).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].sources, vec![1]);
        assert_eq!(out[1].sources, vec![0]);
    }

    #[test]
    fn many_holes_split_into_paths_the_planner_can_take() {
        let out =
            fly(grid_of_holes(20, 20, 1.), &circles((0..400).collect()), &[], &mut Vec::new())
                .unwrap();
        assert!(out.len() > 1);
        for path in &out {
            assert!(path.segments.len() <= LOOP_SEGMENTS);
            flies_through(path);
        }
        let total: f64 = out.iter().map(cut_length).sum();
        assert!((total - 400. * (TAU + 0.3)).abs() < 1e-6);
    }
}
