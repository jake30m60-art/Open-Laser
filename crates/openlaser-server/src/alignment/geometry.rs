// SPDX-License-Identifier: GPL-3.0-or-later
//! Offline metrology. Edge hits constrain a curve, not the clicked point along it.
use crate::{Error, Result};
use openlaser_core::geometry::{Contour, Curve, Drawing, Point, Transform};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

fn fail(s: &str) -> Error {
    Error::Refused(s.into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Shape {
    Rectangle { width: f64, height: f64 },
    Disc { diameter: f64 },
    Special { drawing: Drawing },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pick {
    pub contour: usize,
    pub curve: usize,
    pub t: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Template {
    #[serde(default)]
    pub remnant: Option<super::remnants::RemnantRef>,
    pub shape: Shape,
    pub picks: Vec<Pick>,
    pub drawing_point: [f64; 2],
    pub machine_point: [f64; 2],
    /// Independent +/- uncertainty in X and Y, not radial distance.
    pub search_mm: f64,
    pub angle_deg: f64,
    pub tolerance_mm: f64,
    pub every_job: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Pose {
    pub translation: [f64; 2],
    pub angle: f64,
}
impl Pose {
    pub fn matrix(self) -> Transform {
        let (s, c) = self.angle.sin_cos();
        Transform([c, s, -s, c, self.translation[0], self.translation[1]])
    }
    pub fn apply(self, p: Point) -> Point {
        self.matrix().apply(p)
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Search {
    pub pick: Pick,
    pub point: [f64; 2],
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub normal: [f64; 2],
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Plan {
    pub searches: Vec<Search>,
    pub outline: Vec<Vec<[f64; 2]>>,
    pub nominal: Pose,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Fit {
    pub pose: Pose,
    pub max_error_mm: f64,
    pub hits: Vec<[f64; 2]>,
}

impl Shape {
    pub fn drawing(&self) -> Result<Drawing> {
        let dimension = |x: f64| x.is_finite() && (0.1..=10000.).contains(&x);
        let drawing = match self {
            Self::Rectangle { width, height } => {
                if !dimension(*width) || !dimension(*height) {
                    return Err(fail("Invalid workpiece dimensions"));
                }
                let p = [
                    Point::ORIGIN,
                    Point::new(*width, 0.),
                    Point::new(*width, *height),
                    Point::new(0., *height),
                ];
                Drawing {
                    contours: vec![Contour {
                        layer: "Reference only".into(),
                        curves: (0..4)
                            .map(|i| Curve::Line { start: p[i], end: p[(i + 1) % 4] })
                            .collect(),
                    }],
                }
            }
            Self::Disc { diameter } => {
                if !dimension(*diameter) {
                    return Err(fail("Invalid workpiece dimensions"));
                }
                Drawing {
                    contours: vec![Contour {
                        layer: "Reference only".into(),
                        curves: vec![Curve::circle(Point::ORIGIN, *diameter / 2.)],
                    }],
                }
            }
            Self::Special { drawing } => drawing.clone(),
        };
        let curves = drawing.contours.iter().map(|c| c.curves.len()).sum::<usize>();
        if curves == 0
            || curves > 2048
            || drawing.contours.iter().any(|c| {
                !c.is_closed() || c.curves.iter().any(|c| !c.is_valid() || c.length() < 1e-5)
            })
        {
            return Err(fail("Reference needs closed, valid contours (maximum 2048 curves)"));
        }
        if drawing.contours.iter().flat_map(|c| &c.curves).any(|c| {
            let b = c.bounds();
            [b.min.x, b.min.y, b.max.x, b.max.y].iter().any(|v| !v.is_finite() || v.abs() > 100000.)
        }) {
            return Err(fail("Invalid reference coordinates"));
        }
        let outer =
            openlaser_prep::reference_outline(&drawing).map_err(|e| fail(&e.to_string()))?;
        if outer != 0 {
            return Err(fail("The first reference contour must be the outer boundary"));
        }
        Ok(drawing)
    }
}

fn parameter(curve: Curve, p: Point) -> f64 {
    match curve {
        Curve::Line { start, end } => (p - start).dot(end - start) / (end - start).dot(end - start),
        Curve::Arc { center, start_angle, sweep, .. } => {
            let a = (p.y - center.y).atan2(p.x - center.x);
            if sweep > 0. {
                (a - start_angle).rem_euclid(TAU) / sweep
            } else {
                (start_angle - a).rem_euclid(TAU) / -sweep
            }
        }
    }
}

fn distance(curve: Curve, p: Point) -> f64 {
    let t = parameter(curve, p);
    if (0. ..=1.).contains(&t) {
        p.distance(curve.point(t))
    } else {
        p.distance(curve.start()).min(p.distance(curve.end()))
    }
}

/// Exact ray/line and ray/arc intersections, in ray distance units.
fn crossings(curve: Curve, p: Point, v: Point) -> Vec<f64> {
    let roots = match curve {
        Curve::Line { start, end } => {
            let d = end - start;
            let det = v.cross(d);
            if det.abs() < 1e-10 { vec![] } else { vec![(start - p).cross(d) / det] }
        }
        Curve::Arc { center, radius, .. } => {
            let q = p - center;
            let b = q.dot(v);
            let a = v.dot(v);
            let disc = b * b - a * (q.dot(q) - radius * radius);
            if disc <= 1e-10 {
                vec![]
            } else {
                vec![(-b - disc.sqrt()) / a, (-b + disc.sqrt()) / a]
            }
        }
    };
    roots
        .into_iter()
        .filter(|&r| {
            let t = parameter(curve, p + v * r);
            (-1e-8..=1. + 1e-8).contains(&t)
        })
        .collect()
}

fn material(d: &Drawing, p: Point) -> bool {
    // Non-axis-aligned ray avoids the common horizontal/vertical vertex case.
    let v = Point::direction(0.3712345);
    let mut hits: Vec<f64> = d
        .contours
        .iter()
        .flat_map(|c| &c.curves)
        .flat_map(|c| crossings(*c, p, v))
        .filter(|r| *r > 1e-8)
        .collect();
    hits.sort_by(f64::total_cmp);
    hits.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    hits.len() % 2 == 1
}

fn selected(d: &Drawing, pick: Pick) -> Result<Curve> {
    if !pick.t.is_finite() || !(0. ..=1.).contains(&pick.t) {
        return Err(fail("Invalid measurement point"));
    }
    d.contours
        .get(pick.contour)
        .and_then(|c| c.curves.get(pick.curve))
        .copied()
        .ok_or_else(|| fail("Invalid measurement point"))
}

impl Template {
    pub fn nominal(&self) -> Pose {
        Pose {
            translation: [
                self.machine_point[0] - self.drawing_point[0],
                self.machine_point[1] - self.drawing_point[1],
            ],
            angle: 0.,
        }
    }
    pub fn plan(&self) -> Result<Plan> {
        let d = self.shape.drawing()?;
        if self
            .drawing_point
            .iter()
            .chain(&self.machine_point)
            .any(|x| !x.is_finite() || x.abs() > 100000.)
            || !self.search_mm.is_finite()
            || !(0.1..=50.).contains(&self.search_mm)
            || !self.angle_deg.is_finite()
            || !(0. ..=10.).contains(&self.angle_deg)
            || !self.tolerance_mm.is_finite()
            || !(0.005..=1.).contains(&self.tolerance_mm)
        {
            return Err(fail("Invalid search limits"));
        }
        let disc = matches!(self.shape, Shape::Disc { .. });
        let picks = if disc {
            (0..8).map(|i| Pick { contour: 0, curve: 0, t: f64::from(i) / 8. }).collect()
        } else {
            self.picks.clone()
        };
        if !(4..=32).contains(&picks.len()) {
            return Err(fail("Choose 4 to 32 distributed measurement points"));
        }
        if matches!(self.shape, Shape::Rectangle { .. })
            && (0..4).any(|edge| !picks.iter().any(|p| p.contour == 0 && p.curve == edge))
        {
            return Err(fail(
                "Rectangle measurements must cover all four edges to check its dimensions",
            ));
        }
        let nominal = self.nominal();
        let mut searches = Vec::new();
        let rot = if disc { 0. } else { 2. * (self.angle_deg.to_radians() / 2.).sin() };
        let reference = Point::from(self.drawing_point);
        for pick in picks {
            if pick.contour != 0 {
                return Err(fail("Choose measurement points on the outside boundary, not a hole"));
            }
            let curve = selected(&d, pick)?;
            let p = curve.point(pick.t);
            if !matches!(curve,Curve::Arc { sweep, .. } if sweep.abs()>=TAU-1e-8)
                && (pick.t.min(1. - pick.t) * curve.length() < 2.)
            {
                return Err(fail("Keep measurement points away from corners"));
            }
            let mut n = curve.tangent(pick.t).perpendicular();
            if material(&d, p + n * 0.01) {
                n = -n;
            }
            if !material(&d, p - n * 0.01) || material(&d, p + n * 0.01) {
                return Err(fail("Ambiguous material boundary"));
            }
            let inset =
                (self.search_mm * 2_f64.sqrt() + rot * p.distance(reference) + 2.) / (1. - rot);
            let start = p - n * inset;
            let end = p + n * inset;
            let uncertainty = self.search_mm * 2_f64.sqrt() + rot * start.distance(reference);
            let clearance = d
                .contours
                .iter()
                .flat_map(|c| &c.curves)
                .map(|c| distance(*c, start))
                .fold(f64::INFINITY, f64::min);
            if !material(&d, start) || clearance < uncertainty + 1. {
                return Err(fail(
                    "Search start is not guaranteed on material; reduce uncertainty or choose another point",
                ));
            }
            searches.push(Search {
                pick,
                point: p.into(),
                start: nominal.apply(start).into(),
                end: nominal.apply(end).into(),
                normal: n.into(),
            });
        }
        // Rank of normal constraints; a circular special part cannot determine rotation.
        let points =
            searches.iter().map(|s| nominal.apply(s.point.into()).into()).collect::<Vec<_>>();
        solve(self, &d, &searches, &points)?;
        Ok(Plan { searches, outline: crate::draft::outline(&d, 96), nominal })
    }
}

fn residual(curve: Curve, pose: Pose, hit: Point) -> f64 {
    let p = pose.matrix().inverse().apply(hit);
    match curve {
        Curve::Line { start, end } => (p - start).cross(end - start) / start.distance(end),
        Curve::Arc { center, radius, .. } => p.distance(center) - radius,
    }
}

fn linear(mut a: [[f64; 3]; 3], mut b: [f64; 3], n: usize) -> Result<[f64; 3]> {
    for i in 0..n {
        let pivot = (i..n).max_by(|&j, &k| a[j][i].abs().total_cmp(&a[k][i].abs())).unwrap_or(i);
        a.swap(i, pivot);
        b.swap(i, pivot);
        if a[i][i].abs() < 1e-5 {
            return Err(fail(
                "Measurement points cannot determine a unique stable position and angle",
            ));
        }
        let v = a[i][i];
        for k in i..n {
            a[i][k] /= v;
        }
        b[i] /= v;
        for j in 0..n {
            if j != i {
                let v = a[j][i];
                for k in i..n {
                    a[j][k] -= v * a[i][k];
                }
                b[j] -= v * b[i];
            }
        }
    }
    Ok(b)
}

fn solve(t: &Template, d: &Drawing, searches: &[Search], hits: &[[f64; 2]]) -> Result<Pose> {
    let n = if matches!(t.shape, Shape::Disc { .. }) { 2 } else { 3 };
    let center = Point::from(t.drawing_point);
    let lever = searches.iter().map(|s| Point::from(s.point).distance(center)).fold(1., f64::max);
    let mut x = [t.machine_point[0], t.machine_point[1], 0.];
    let pose = |x: [f64; 3]| {
        let a = x[2] / lever;
        let r = Pose { translation: [0., 0.], angle: a }.apply(center);
        Pose { translation: [x[0] - r.x, x[1] - r.y], angle: a }
    };
    for _ in 0..30 {
        let mut a = [[0.; 3]; 3];
        let mut b = [0.; 3];
        for (s, hit) in searches.iter().zip(hits) {
            let c = selected(d, s.pick)?;
            let r = residual(c, pose(x), (*hit).into());
            let mut j = [0.; 3];
            for k in 0..n {
                let mut xx = x;
                xx[k] += 0.001;
                j[k] = (residual(c, pose(xx), (*hit).into()) - r) / 0.001;
            }
            for i in 0..n {
                b[i] -= j[i] * r;
                for k in 0..n {
                    a[i][k] += j[i] * j[k];
                }
            }
        }
        let delta = linear(a, b, n)?;
        for i in 0..n {
            x[i] += delta[i];
        }
        if x.iter().any(|v| !v.is_finite()) {
            return Err(fail("Registration did not converge"));
        }
        if delta.iter().take(n).all(|v| v.abs() < 1e-8) {
            return Ok(pose(x));
        }
    }
    Err(fail("Registration did not converge"))
}

pub(crate) fn fit(t: &Template, plan: &Plan, hits: Vec<[f64; 2]>) -> Result<Fit> {
    if hits.len() != plan.searches.len() || hits.iter().flatten().any(|v| !v.is_finite()) {
        return Err(fail("Incomplete measurement"));
    }
    for (s, h) in plan.searches.iter().zip(&hits) {
        let a = Point::from(s.start);
        let b = Point::from(s.end);
        let h = Point::from(*h);
        let v = b - a;
        let u = (h - a).dot(v) / v.dot(v);
        if !(0. ..=1.).contains(&u) || h.distance(a + v * u) > t.tolerance_mm {
            return Err(fail("Edge outside search range"));
        }
    }
    let d = t.shape.drawing()?;
    let pose = solve(t, &d, &plan.searches, &hits)?;
    let mapped = pose.apply(t.drawing_point.into());
    if (mapped.x - t.machine_point[0]).abs() > t.search_mm + 1e-6
        || (mapped.y - t.machine_point[1]).abs() > t.search_mm + 1e-6
        || pose.angle.abs() > t.angle_deg.to_radians() + 1e-6
    {
        return Err(fail("Measured position exceeds the allowed uncertainty"));
    }
    let mut max_error: f64 = 0.;
    for (s, h) in plan.searches.iter().zip(&hits) {
        let c = selected(&d, s.pick)?;
        let local = pose.matrix().inverse().apply((*h).into());
        max_error = max_error.max(distance(c, local));
        let u = parameter(c, local);
        if !(0. ..=1. + 1e-7).contains(&u) {
            return Err(fail("Measured edge belongs to another contour region"));
        }
    }
    if max_error > t.tolerance_mm {
        return Err(fail("Workpiece dimensions or edge measurements do not match"));
    }
    Ok(Fit { pose, max_error_mm: max_error, hits })
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Fault {
    None,
    Missing,
    Hole,
    WrongSize,
    Outside,
    Outlier,
}

pub(crate) fn simulate(
    t: &Template,
    plan: &Plan,
    delta: [f64; 2],
    angle_deg: f64,
    fault: Fault,
) -> Result<Fit> {
    if delta.iter().any(|v| !v.is_finite() || v.abs() > 1000.)
        || !angle_deg.is_finite()
        || angle_deg.abs() > 30.
    {
        return Err(fail("Invalid simulation input"));
    }
    if matches!(fault, Fault::Missing) {
        return Err(fail("No material at search start"));
    }
    let mut d = t.shape.drawing()?;
    if matches!(fault, Fault::WrongSize) {
        let scale = Transform([1.05, 0., 0., 1.05, 0., 0.]);
        for c in &mut d.contours {
            *c = scale.contour(c);
        }
    }
    let angle = if matches!(t.shape, Shape::Disc { .. }) { 0. } else { angle_deg.to_radians() };
    let r = Pose { translation: [0., 0.], angle }.apply(t.drawing_point.into());
    let actual = Pose {
        translation: [t.machine_point[0] + delta[0] - r.x, t.machine_point[1] + delta[1] - r.y],
        angle,
    };
    if matches!(fault, Fault::Hole) {
        let s = &plan.searches[0];
        let mid = Point::from(s.start).lerp(plan.nominal.apply(s.point.into()), 0.5);
        d.contours.push(Contour {
            layer: "Simulated hole".into(),
            curves: vec![Curve::circle(actual.matrix().inverse().apply(mid), 2.)],
        });
    }
    let mut hits = Vec::new();
    for s in &plan.searches {
        let inv = actual.matrix().inverse();
        let start = inv.apply(s.start.into());
        let end = inv.apply(s.end.into());
        let v = end - start;
        if !material(&d, start) {
            return Err(fail("No material at search start"));
        }
        let mut crossings: Vec<_> = d
            .contours
            .iter()
            .enumerate()
            .flat_map(|(ci, c)| {
                c.curves.iter().enumerate().flat_map(move |(ei, c)| {
                    crossings(*c, start, v).into_iter().map(move |u| (u, ci, ei))
                })
            })
            .filter(|(u, _, _)| *u > 1e-8 && *u <= 1.)
            .collect();
        crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
        let &(u, ci, ei) = crossings.first().ok_or_else(|| fail("Edge outside search range"))?;
        if (ci, ei) != (s.pick.contour, s.pick.curve) {
            return Err(fail("Hole or unexpected edge in search corridor"));
        }
        hits.push(actual.apply(start + v * u).into());
    }
    if matches!(fault, Fault::Outside) {
        hits[0] = [plan.searches[0].end[0] + 1000., plan.searches[0].end[1] + 1000.];
    }
    if matches!(fault, Fault::Outlier) {
        let n = plan.searches[0].normal;
        hits[0][0] += n[0] * 3.;
        hits[0][1] += n[1] * 3.;
    }
    fit(t, plan, hits)
}

/// Transform a copy of the prepared machining paths, including leads and film.
/// The reference drawing is deliberately absent from this interface.
pub(crate) fn prepare(p: &crate::draft::Prepared, pose: Pose) -> crate::draft::Prepared {
    use openlaser_compiler::cut::Segment;
    let transform = |c: &openlaser_compiler::cut::Contour| {
        let mut c = c.clone();
        for s in &mut c.segments {
            s.curve = match s.curve {
                Segment::Line { start, end } => Segment::Line {
                    start: pose.apply(start.into()).into(),
                    end: pose.apply(end.into()).into(),
                },
                Segment::Arc { center, radius, start_angle, sweep } => Segment::Arc {
                    center: pose.apply(center.into()).into(),
                    radius,
                    start_angle: start_angle + pose.angle,
                    sweep,
                },
            };
        }
        c
    };
    let mut preview = (*p.preview).clone();
    let display = |c: &mut crate::document::PreviewContour| {
        for path in &mut c.paths {
            for q in &mut path.points {
                *q = pose.apply((*q).into()).into();
            }
        }
        for q in &mut c.cooling {
            *q = pose.apply((*q).into()).into();
        }
        c.start = pose.apply(c.start.into()).into();
    };
    for c in &mut preview.contours {
        display(c);
    }
    for c in &mut preview.engraving_regions {
        display(c);
    }
    if let Some(source) = &mut preview.source_contours {
        for c in source {
            display(c);
        }
    }
    preview.bounds = preview
        .contours
        .iter()
        .flat_map(|c| &c.paths)
        .flat_map(|p| &p.points)
        .map(|p| openlaser_core::geometry::Bounds::of((*p).into()))
        .reduce(openlaser_core::geometry::Bounds::union);
    for raster in &mut preview.rasters {
        let origin = pose.apply(raster.origin.into());
        raster.across =
            (pose.apply((Point::from(raster.origin) + Point::from(raster.across)).into()) - origin)
                .into();
        raster.down = (pose.apply((Point::from(raster.origin) + Point::from(raster.down)).into())
            - origin)
            .into();
        raster.origin = origin.into();
    }
    crate::draft::Prepared {
        contours: p.contours.iter().map(transform).collect(),
        film: p.film.iter().map(|v| v.iter().map(transform).collect()).collect(),
        preview: std::sync::Arc::new(preview),
        ..p.clone()
    }
}
