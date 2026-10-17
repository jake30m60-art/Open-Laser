// SPDX-License-Identifier: GPL-3.0-or-later

use crate::polygon::{GUARD, MAX_TOTAL_VERTICES, contains, polygon};
use crate::{Error, Placement, Request, Sheet, SheetSolution, Solution};
use jagua_rs::entities::{Container, Item, Layout, PlacedItem};
use jagua_rs::geometry::DTransformation;
use jagua_rs::geometry::shape_modification::ShapeModifyConfig;
use jagua_rs::io::export::int_to_ext_transformation;
use jagua_rs::io::ext_repr::{ExtContainer, ExtItem, ExtQualityZone, ExtSPolygon, ExtShape};
use jagua_rs::io::import::{Importer, ext_to_int_transformation};
use jagua_rs::probs::spp::entities::{SPInstance, SPProblem, SPSolution, Strip};
use openlaser_core::geometry::{Point, Transform};
use openlaser_core::nesting::{NestRotation, NestSettings};
use rand::rngs::Xoshiro256PlusPlus;
use rand::{SeedableRng, seq::SliceRandom};
use sparrow::config::DEFAULT_SPARROW_CONFIG;
use sparrow::eval::lbf_evaluator::LBFEvaluator;
use sparrow::eval::sample_eval::{SampleEval, SampleEvaluator};
use sparrow::sample::search::{SampleConfig, search_placement};
use sparrow::util::listener::DummySolListener;
use sparrow::util::listener::{ReportType, SolutionListener};
use sparrow::util::terminator::Terminator;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Most copies one search places on its own, across every item and sheet;
/// a part wanted more often goes in grids.
pub(crate) const SEARCH_COPIES: usize = 500;
/// Most copies one multi-sheet request may ask for, across every item and
/// sheet: several sheets' worth of small parts, laid out in grids.
pub(crate) const MAX_COPIES: usize = 100_000;
/// Longest search a request may ask for, and the separate bound on shape
/// preparation before it.
const MAX_SEARCH_TIME: Duration = Duration::from_secs(30);
/// Largest machining clearance, in millimetres, a request may add around
/// each part.
const MAX_MACHINING_CLEARANCE: f64 = 1000.;
/// Samples tried for each copy of a first fit across sheets. Sparrow's own
/// construction uses 1000 (`sparrow::consts::LBF_SAMPLE_CONFIG`); fewer keep
/// a many-sheet search within its time.
const SHEET_FIT_SAMPLES: SampleConfig =
    SampleConfig { n_container_samples: 350, n_focussed_samples: 0, n_coord_descents: 3 };
/// Samples tried for each copy of one sheet's randomized first fits, which
/// are repeated many times, so fewer again.
const REPEATED_FIT_SAMPLES: SampleConfig =
    SampleConfig { n_container_samples: 250, n_focussed_samples: 0, n_coord_descents: 3 };
/// Share of the remaining time Sparrow explores before compressing: its own
/// default split (`sparrow::consts::DEFAULT_EXPLORE_TIME_RATIO`, 0.8/0.2).
const EXPLORE_SHARE: f64 = 0.8;
/// Share of the remaining time Sparrow compresses; see [`EXPLORE_SHARE`].
const COMPRESS_SHARE: f64 = 0.2;

/// How often a running search shows where it stands.
pub const LIVE_INTERVAL: Duration = Duration::from_millis(200);

/// Shows a search's sheets as they stand and which one just changed, at
/// most every [`LIVE_INTERVAL`].
pub(crate) struct Live<'a> {
    show: &'a (dyn Fn(&[SheetSolution], usize) + Sync),
    last: Option<Instant>,
}

impl<'a> Live<'a> {
    pub(crate) fn new(show: &'a (dyn Fn(&[SheetSolution], usize) + Sync)) -> Self {
        Self { show, last: None }
    }

    /// Whether the last showing is old enough for another.
    fn due(&mut self) -> bool {
        let now = Instant::now();
        if self.last.is_some_and(|last| now - last < LIVE_INTERVAL) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

struct Budget<'a> {
    cancel: &'a AtomicBool,
    deadline: Instant,
    phase: Instant,
}

impl Terminator for Budget<'_> {
    fn kill(&self) -> bool {
        self.cancel.load(Ordering::Relaxed) || Instant::now() >= self.phase.min(self.deadline)
    }
    fn new_timeout(&mut self, timeout: Duration) {
        self.phase = (Instant::now() + timeout).min(self.deadline);
    }
    fn timeout_at(&self) -> Option<jagua_rs::Instant> {
        Some(self.phase.min(self.deadline))
    }
}

struct Evaluation<'a> {
    inner: LBFEvaluator<'a>,
    budget: &'a Budget<'a>,
}

impl SampleEvaluator for Evaluation<'_> {
    fn evaluate_sample(&mut self, dt: DTransformation, upper: Option<SampleEval>) -> SampleEval {
        if self.budget.kill() { SampleEval::Invalid } else { self.inner.evaluate_sample(dt, upper) }
    }
    fn n_evals(&self) -> usize {
        self.inner.n_evals()
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "finite millimetre inputs are bounded to 100 m and padded for f32 conversion"
)]
fn ext(points: &[[f64; 2]], origin: Point) -> ExtShape {
    ExtShape::SimplePolygon(ExtSPolygon(
        points.iter().map(|p| ((p[0] - origin.x) as f32, (p[1] - origin.y) as f32)).collect(),
    ))
}

fn err(error: impl std::fmt::Display) -> Error {
    Error::new(error.to_string())
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "validated dimensions and at most 500 copies fit these conversions"
)]
pub(crate) fn run(
    request: &Request,
    cancel: &AtomicBool,
    progress: &(impl Fn(usize, usize) + Sync),
) -> Result<Solution, Error> {
    let total = validate(request, SEARCH_COPIES)?;
    let whole = request
        .items
        .iter()
        .fold([0, 0], |sum, item| (0..item.quantity).fold(sum, |sum, _| plus(sum, load(item))));
    if !fits(&request.sheet_limit, whole) {
        return Err(Error::new(format!(
            "these copies hold {} contours and {} lines and arcs; one sheet can prepare {} and {}",
            whole[0], whole[1], request.sheet_limit.contours, request.sheet_limit.curves
        )));
    }
    check_cancelled(cancel)?;
    let geometry = geometry(request, cancel)?;
    // Shape preparation has its own bound; the requested duration is search time.
    let deadline = Instant::now() + request.time_limit;
    let mut budget = Budget { cancel, deadline, phase: deadline };
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(request.seed);
    let mut progress = Progress { callback: progress, most_placed: 0, total };
    let best = initial_search(request, &geometry, &budget, &mut rng, &mut progress);
    check_cancelled(cancel)?;
    let mut best = best.ok_or_else(|| Error::new(format!("no complete layout found ({} of {total} parts placed); try fewer copies, a larger container, less spacing, or different rotations", progress.most_placed)))?;
    if request.rectangular && request.cutouts.is_empty() && !budget.kill() {
        let mut quiet = DummySolListener;
        best = compress(best, &geometry.items, &geometry.stock, &mut budget, &mut rng, &mut quiet)?;
    }
    check_cancelled(cancel)?;
    if !best.is_feasible() || best.placed_items.len() != total {
        return Err(Error::new("nesting result failed its collision check".into()));
    }
    solution(request, &best, &geometry.items, geometry.origin, &geometry.item_origins, true)
}

fn check_cancelled(cancel: &AtomicBool) -> Result<(), Error> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::new("nesting cancelled".into()))
    } else {
        Ok(())
    }
}

/// Many copies of a part first fill whole sheets in grids; what is left,
/// with every other part, is searched on the sheets still free. When that
/// search fails for a single part, its rest is laid out in a grid too.
pub(crate) fn run_sheets(
    request: &Request,
    sheets: &[Sheet],
    cancel: &AtomicBool,
    progress: &(impl Fn(usize, usize) + Sync),
    live: &mut Live<'_>,
) -> Result<Vec<SheetSolution>, Error> {
    if sheets.is_empty() {
        return Err(Error::new("choose at least one sheet to nest on".into()));
    }
    let total = validate(request, MAX_COPIES)?;
    check_cancelled(cancel)?;
    let mut tiling = Tiling {
        request,
        sheets,
        left: sheets.iter().map(|s| s.count).collect(),
        remaining: request.items.iter().map(|i| i.quantity).collect(),
        done: Vec::new(),
        total,
    };
    let mut bulk: Vec<usize> = (0..request.items.len())
        .filter(|&i| crate::tile::bulk(&request.items[i], request, total, SEARCH_COPIES))
        .collect();
    bulk.sort_by(|a, b| {
        crate::tile::area(&request.items[*b]).total_cmp(&crate::tile::area(&request.items[*a]))
    });
    // Copies the search takes: every other part, then each grid's rest.
    let mut searched: usize = (0..request.items.len())
        .filter(|i| !bulk.contains(i))
        .map(|i| request.items[i].quantity)
        .sum();
    for &id in &bulk {
        // Rectangles wanted edge to edge stay in grids to the last copy.
        let shared = (!crate::tile::flush(&request.items[id], request)).then_some(searched);
        searched += tiling.fill(id, shared, cancel, progress, live)?;
    }
    if searched > SEARCH_COPIES {
        return Err(if bulk.iter().any(|&id| tiling.grid(id).is_some()) {
            Error::sheets_full(tiling.placed(), total)
        } else {
            Error::new(format!(
                "more than {SEARCH_COPIES} copies need a rectangular sheet without cutouts that holds the part"
            ))
        });
    }
    if searched == 0 {
        return Ok(tiling.done);
    }
    // The rest as a request of its own, its items numbered anew.
    let ids: Vec<usize> = (0..request.items.len()).filter(|&i| tiling.remaining[i] > 0).collect();
    let rest = Request {
        items: ids
            .iter()
            .map(|&i| crate::Item { quantity: tiling.remaining[i], ..request.items[i].clone() })
            .collect(),
        ..request.clone()
    };
    let free: Vec<Sheet> = sheets
        .iter()
        .zip(&tiling.left)
        .map(|(sheet, left)| Sheet { count: *left, ..sheet.clone() })
        .collect();
    let before = tiling.placed();
    let reached = AtomicUsize::new(before);
    let found = {
        let shown = &tiling.done;
        let outer = live.show;
        let show = |found: &[SheetSolution], changed: usize| {
            let mut all = shown.clone();
            all.extend(found.iter().cloned().map(|mut s| {
                renumber(&mut s, &ids);
                s
            }));
            outer(&all, shown.len() + changed);
        };
        let mut inner = Live::new(&show);
        let counted = |placed: usize, _: usize| {
            reached.fetch_max(before + placed, Ordering::Relaxed);
            progress(before + placed, total);
        };
        search_sheets(&rest, &free, cancel, &counted, &mut inner)
    };
    match found {
        Ok(found) => {
            for mut sheet in found {
                renumber(&mut sheet, &ids);
                tiling.done.push(sheet);
            }
            Ok(tiling.done)
        }
        Err(_) if ids.len() == 1 && bulk.contains(&ids[0]) && !cancel.load(Ordering::Relaxed) => {
            tiling.fill(ids[0], None, cancel, progress, live)?;
            if tiling.remaining[ids[0]] > 0 {
                return Err(Error::sheets_full(tiling.placed(), total));
            }
            Ok(tiling.done)
        }
        Err(e) if e.sheets_full => Err(Error::sheets_full(reached.into_inner(), total)),
        Err(e) => Err(e),
    }
}

/// Item numbers of a rest request's own back to the original request's.
fn renumber(sheet: &mut SheetSolution, ids: &[usize]) {
    for placement in &mut sheet.layout.placements {
        placement.item = ids[placement.item];
    }
}

/// Sheets laid out in grids so far.
struct Tiling<'a> {
    request: &'a Request,
    sheets: &'a [Sheet],
    /// Sheets of each kind still free, `None` for as many as needed.
    left: Vec<Option<usize>>,
    /// Copies of each item still to place.
    remaining: Vec<usize>,
    done: Vec<SheetSolution>,
    total: usize,
}

impl Tiling<'_> {
    fn placed(&self) -> usize {
        self.total - self.remaining.iter().sum::<usize>()
    }

    /// The first kind of sheet with one left that holds a grid of item
    /// `id`, its grid and how many copies one sheet takes.
    fn grid(&self, id: usize) -> Option<(usize, crate::tile::Grid, usize)> {
        let item = &self.request.items[id];
        let copy = load(item);
        let limit = &self.request.sheet_limit;
        let by_load = (limit.contours / copy[0].max(1)).min(limit.curves / copy[1].max(1));
        self.sheets.iter().enumerate().filter(|(k, _)| self.left[*k] != Some(0)).find_map(
            |(kind, sheet)| {
                let grid = crate::tile::Grid::new(self.request, sheet, item)?;
                let capacity = grid.capacity().min(by_load);
                (capacity > 0).then_some((kind, grid, capacity))
            },
        )
    }

    /// Fills sheets with grids of item `id` and returns the copies left.
    /// With `searched`, the copies a search already takes, a rest smaller
    /// than one sheet stays for that search while it keeps within
    /// [`SEARCH_COPIES`].
    fn fill(
        &mut self,
        id: usize,
        searched: Option<usize>,
        cancel: &AtomicBool,
        progress: &(impl Fn(usize, usize) + Sync),
        live: &mut Live<'_>,
    ) -> Result<usize, Error> {
        while self.remaining[id] > 0 {
            check_cancelled(cancel)?;
            let Some((kind, grid, capacity)) = self.grid(id) else { break };
            let rest = self.remaining[id];
            if rest < capacity && searched.is_some_and(|s| s + rest <= SEARCH_COPIES) {
                break;
            }
            let count = rest.min(capacity);
            let placements = grid.place(&self.request.items[id], id, count);
            if placements.len() != count {
                return Err(Error::new("a grid lost a requested copy".into()));
            }
            self.done.push(crate::tile::sheet(self.request, &self.sheets[kind], kind, placements)?);
            if let Some(left) = &mut self.left[kind] {
                *left -= 1;
            }
            self.remaining[id] -= count;
            progress(self.placed(), self.total);
            if live.due() {
                (live.show)(&self.done, self.done.len() - 1);
            }
        }
        Ok(self.remaining[id])
    }
}

/// Bounded first-fit placement across sheets, largest parts first. Every sheet
/// shares the same quantity accounting and the same cancellation/deadline.
fn search_sheets(
    request: &Request,
    sheets: &[Sheet],
    cancel: &AtomicBool,
    progress: &(impl Fn(usize, usize) + Sync),
    live: &mut Live<'_>,
) -> Result<Vec<SheetSolution>, Error> {
    let total = validate(request, SEARCH_COPIES)?;
    let inputs: Vec<Request> = sheets
        .iter()
        .map(|sheet| Request {
            stock: sheet.stock.clone(),
            cutouts: sheet.cutouts.clone(),
            rectangular: sheet.rectangular,
            settings: NestSettings { margin: sheet.margin, ..request.settings },
            ..request.clone()
        })
        .collect();
    let geometries =
        inputs.iter().map(|input| geometry(input, cancel)).collect::<Result<Vec<_>, _>>()?;
    let stocks = Stocks { sheets, inputs: &inputs, geometries: &geometries };
    let deadline = Instant::now() + request.time_limit;
    let mut budget = Budget { cancel, deadline, phase: deadline };
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(request.seed);
    let order = largest_first(&geometries[0]);
    let mut open = FirstFit { layouts: Vec::new(), kinds: Vec::new(), loads: Vec::new() };
    for (placed, id) in order.into_iter().enumerate() {
        check_cancelled(cancel)?;
        if budget.kill() {
            return Err(time_limit_reached(placed, total));
        }
        let fitted =
            open.place(&stocks, request, id, &budget, &mut rng).map_err(
                |unplaced| match unplaced {
                    Unplaced::OutOfTime => time_limit_reached(placed, total),
                    Unplaced::TooLarge => Error::new(format!(
                        "part {} fits on none of the sheets left with this spacing and rotation; add a larger sheet",
                        id + 1
                    )),
                    Unplaced::OutOfSheets => Error::sheets_full(placed, total),
                },
            )?;
        progress(placed + 1, total);
        if live.due() {
            (live.show)(&stocks.standing(&open), fitted);
        }
    }
    check_cancelled(cancel)?;
    let last = open.layouts.len() - 1;
    let kind = open.kinds[last];
    // Every sheet before the last stays as it is while the last compacts.
    let earlier = stocks.standing_before(&open, last);
    let (geometry, input) = (&geometries[kind], &inputs[kind]);
    let mut compacting = Compacting { earlier, input, geometry, sheet: kind, live };
    compress_last(&mut open.layouts[last], geometry, input, &mut budget, &mut rng, &mut compacting);
    check_cancelled(cancel)?;
    stocks.finished(open)
}

/// Every requested copy, as its item index, largest area first.
fn largest_first(geometry: &Geometry) -> Vec<usize> {
    let mut order: Vec<usize> = geometry
        .items
        .iter()
        .enumerate()
        .flat_map(|(i, (_, q))| std::iter::repeat_n(i, *q))
        .collect();
    order.sort_by(|a, b| {
        geometry.items[*b].0.shape_cd.area.total_cmp(&geometry.items[*a].0.shape_cd.area)
    });
    order
}

fn time_limit_reached(placed: usize, total: usize) -> Error {
    Error::new(format!(
        "nesting time limit reached ({placed} of {total} parts); increase search time"
    ))
}

/// Why a copy found no sheet.
enum Unplaced {
    /// The search ran out of time.
    OutOfTime,
    /// It does not fit even on an empty sheet.
    TooLarge,
    /// Every chosen sheet is in use and none has room.
    OutOfSheets,
}

/// The sheets of a first-fit placement so far.
struct FirstFit {
    layouts: Vec<Layout>,
    /// Which requested sheet each layout is.
    kinds: Vec<usize>,
    /// Each sheet's contours and curves, kept within what preparation accepts.
    loads: Vec<[usize; 2]>,
}

impl FirstFit {
    /// Places one copy of item `id` on the chosen sheets in their order: for
    /// each kind, its open sheets first, then a new one while any are left.
    /// A remnant listed first therefore fills before a full sheet is started.
    /// Returns the index of the sheet it went on.
    fn place(
        &mut self,
        stocks: &Stocks<'_>,
        request: &Request,
        id: usize,
        budget: &Budget<'_>,
        rng: &mut Xoshiro256PlusPlus,
    ) -> Result<usize, Unplaced> {
        let copy = load(&request.items[id]);
        let mut left = false;
        for (kind, sheet) in stocks.sheets.iter().enumerate() {
            let item = &stocks.geometries[kind].items[id].0;
            for index in (0..self.layouts.len()).filter(|i| self.kinds[*i] == kind) {
                if fits(&request.sheet_limit, plus(self.loads[index], copy))
                    && place_one(&mut self.layouts[index], item, budget, rng)
                {
                    self.loads[index] = plus(self.loads[index], copy);
                    return Ok(index);
                }
            }
            let opened = self.kinds.iter().filter(|k| **k == kind).count();
            if sheet.count.is_some_and(|count| opened >= count) {
                continue;
            }
            left = true;
            if budget.kill() {
                return Err(Unplaced::OutOfTime);
            }
            let mut layout = Layout::new(stocks.geometries[kind].stock.clone());
            if place_one(&mut layout, item, budget, rng) {
                self.layouts.push(layout);
                self.kinds.push(kind);
                self.loads.push(copy);
                return Ok(self.layouts.len() - 1);
            }
        }
        Err(if budget.kill() {
            Unplaced::OutOfTime
        } else if left {
            Unplaced::TooLarge
        } else {
            Unplaced::OutOfSheets
        })
    }
}

/// Each requested sheet's request and geometry.
struct Stocks<'a> {
    sheets: &'a [Sheet],
    inputs: &'a [Request],
    geometries: &'a [Geometry],
}

impl Stocks<'_> {
    fn show(&self, layout: &Layout, kind: usize) -> SheetSolution {
        so_far(&self.inputs[kind], layout.placed_items.values(), &self.geometries[kind], kind)
    }

    /// Every sheet as it stands, for showing.
    fn standing(&self, open: &FirstFit) -> Vec<SheetSolution> {
        self.standing_before(open, open.layouts.len())
    }

    fn standing_before(&self, open: &FirstFit, end: usize) -> Vec<SheetSolution> {
        (0..end).map(|i| self.show(&open.layouts[i], open.kinds[i])).collect()
    }

    /// The finished sheets, each checked, with every requested copy placed.
    fn finished(&self, open: FirstFit) -> Result<Vec<SheetSolution>, Error> {
        let items = &self.inputs[0].items;
        let mut counts = vec![0usize; items.len()];
        let mut sheets = Vec::new();
        for (layout, kind) in open.layouts.into_iter().zip(open.kinds) {
            if layout.placed_items.is_empty() {
                continue;
            }
            if !layout.is_feasible() {
                return Err(Error::new("a sheet failed its collision check".into()));
            }
            let geometry = &self.geometries[kind];
            let layout = solution(
                &self.inputs[kind],
                &layout,
                &geometry.items,
                geometry.origin,
                &geometry.item_origins,
                false,
            )?;
            for placement in &layout.placements {
                counts[placement.item] += 1;
            }
            sheets.push(SheetSolution { sheet: kind, layout });
        }
        if counts.iter().zip(items).any(|(count, item)| *count != item.quantity) {
            return Err(Error::new("multi-sheet nesting lost a requested copy".into()));
        }
        Ok(sheets)
    }
}

/// Every sheet before the last is full. Compress the last one towards its
/// start with the remaining time, so its offcut is left in one piece; a
/// failed compression keeps the first-fit layout.
fn compress_last(
    layout: &mut Layout,
    geometry: &Geometry,
    input: &Request,
    budget: &mut Budget<'_>,
    rng: &mut Xoshiro256PlusPlus,
    listener: &mut impl SolutionListener,
) {
    if !input.rectangular || !input.cutouts.is_empty() || budget.kill() {
        return;
    }
    let mut copies: Vec<(Item, usize)> =
        geometry.items.iter().map(|(item, _)| (item.clone(), 0)).collect();
    for placed in layout.placed_items.values() {
        copies[placed.item_id].1 += 1;
    }
    if let Ok(compressed) =
        compress(layout.clone(), &copies, &geometry.stock, budget, rng, listener)
    {
        *layout = compressed;
    }
}

/// Shows the last sheet each time compaction finds a tighter arrangement
/// without overlaps, beside the sheets already full.
struct Compacting<'a, 'b> {
    earlier: Vec<SheetSolution>,
    input: &'b Request,
    geometry: &'b Geometry,
    sheet: usize,
    live: &'b mut Live<'a>,
}

impl SolutionListener for Compacting<'_, '_> {
    fn report(&mut self, report: ReportType, solution: &SPSolution, _: &SPInstance) {
        if matches!(report, ReportType::ExplFeas | ReportType::CmprFeas) && self.live.due() {
            let placed = solution.layout_snapshot.placed_items.values();
            let mut sheets = self.earlier.clone();
            sheets.push(so_far(self.input, placed, self.geometry, self.sheet));
            (self.live.show)(&sheets, sheets.len() - 1);
        }
    }
}

fn place_one(
    layout: &mut Layout,
    item: &Item,
    budget: &Budget<'_>,
    rng: &mut Xoshiro256PlusPlus,
) -> bool {
    if budget.kill() {
        return false;
    }
    let evaluator = Evaluation { inner: LBFEvaluator::new(layout, item), budget };
    let (sample, _) = search_placement(layout, item, None, evaluator, SHEET_FIT_SAMPLES, rng);
    if let Some((dt, SampleEval::Clear { .. })) = sample {
        layout.place_item(item, dt);
        true
    } else {
        false
    }
}

struct Progress<'a> {
    callback: &'a (dyn Fn(usize, usize) + Sync),
    most_placed: usize,
    total: usize,
}

impl Progress<'_> {
    fn placed(&mut self, count: usize) {
        self.most_placed = self.most_placed.max(count);
        (self.callback)(self.most_placed, self.total);
    }
}

fn initial_search(
    request: &Request,
    geometry: &Geometry,
    budget: &Budget<'_>,
    rng: &mut Xoshiro256PlusPlus,
    progress: &mut Progress<'_>,
) -> Option<Layout> {
    let mut order = largest_first(geometry);
    let mut best = current_layout(
        &geometry.stock,
        &geometry.items,
        &geometry.item_origins,
        geometry.origin,
        request.settings.rotation,
    );
    let mut best_width = best.as_ref().map_or(f32::INFINITY, occupied_width);
    progress.placed(if best.is_some() { progress.total } else { 0 });
    let mut attempt = 0;
    while !budget.kill() {
        if best.is_some() && request.rectangular {
            break;
        }
        let mut layout = Layout::new(geometry.stock.clone());
        if attempt > 0 {
            order.shuffle(rng);
        }
        fill_layout(&mut layout, &geometry.items, &order, budget, rng, progress);
        if layout.placed_items.len() == progress.total && layout.is_feasible() {
            let width = occupied_width(&layout);
            if width < best_width {
                best_width = width;
                best = Some(layout);
            }
            if request.rectangular {
                break;
            }
        }
        attempt += 1;
    }
    best
}

fn fill_layout(
    layout: &mut Layout,
    items: &[(Item, usize)],
    order: &[usize],
    budget: &Budget<'_>,
    rng: &mut Xoshiro256PlusPlus,
    progress: &mut Progress<'_>,
) {
    for &id in order {
        if budget.kill() {
            break;
        }
        let item = &items[id].0;
        let evaluator = Evaluation { inner: LBFEvaluator::new(layout, item), budget };
        let (sample, _) =
            search_placement(layout, item, None, evaluator, REPEATED_FIT_SAMPLES, rng);
        if let Some((dt, SampleEval::Clear { .. })) = sample {
            layout.place_item(item, dt);
        } else {
            break;
        }
        progress.placed(layout.placed_items.len());
    }
}

fn solution(
    request: &Request,
    layout: &Layout,
    items: &[(Item, usize)],
    origin: Point,
    item_origins: &[Point],
    complete: bool,
) -> Result<Solution, Error> {
    let mut counts = vec![0; items.len()];
    let mut placements = Vec::new();
    for placed in layout.placed_items.values() {
        let placement = placement(request, placed, items, origin, item_origins);
        let original = &request.items[placement.item];
        let moved: Vec<_> =
            original.contours.iter().map(|c| placement.transform.contour(c)).collect();
        crate::check_region(&request.stock, &request.cutouts, &moved, request.settings.margin)?;
        counts[placement.item] += 1;
        placements.push(placement);
    }
    if complete && counts.iter().zip(items).any(|(a, (_, b))| a != b) {
        return Err(Error::new("nesting result lost a requested copy".into()));
    }
    Ok(Solution { placements, coverage: coverage(request, items, &counts) })
}

/// A sheet as a running search has it, for showing only: every copy clear
/// of the others, not yet checked against the stock's own curves.
fn so_far<'a>(
    request: &Request,
    placed: impl Iterator<Item = &'a PlacedItem>,
    geometry: &Geometry,
    sheet: usize,
) -> SheetSolution {
    let items = &geometry.items;
    let placements: Vec<_> = placed
        .map(|p| placement(request, p, items, geometry.origin, &geometry.item_origins))
        .collect();
    let mut counts = vec![0; items.len()];
    for placement in &placements {
        counts[placement.item] += 1;
    }
    SheetSolution {
        sheet,
        layout: Solution { coverage: coverage(request, items, &counts), placements },
    }
}

/// Where a placed copy goes: a rigid transform of its item's contours.
fn placement(
    request: &Request,
    placed: &PlacedItem,
    items: &[(Item, usize)],
    origin: Point,
    item_origins: &[Point],
) -> Placement {
    let item = &items[placed.item_id].0;
    let dt = int_to_ext_transformation(&placed.d_transf, &item.shape_orig.pre_transform);
    let (sin, cos) = rotation(dt.rotation(), request.settings.rotation);
    let item_origin = item_origins[placed.item_id];
    let (x, y) = dt.translation();
    let transform = Transform([
        cos,
        sin,
        -sin,
        cos,
        origin.x + f64::from(x) - cos * item_origin.x + sin * item_origin.y,
        origin.y + f64::from(y) - sin * item_origin.x - cos * item_origin.y,
    ]);
    Placement { item: placed.item_id, transform }
}

/// The share of the usable stock that `counts` copies of each item cover.
#[allow(
    clippy::cast_precision_loss,
    reason = "at most 500 item quantities are exactly representable"
)]
fn coverage(request: &Request, items: &[(Item, usize)], counts: &[usize]) -> f64 {
    let area: f64 =
        items.iter().zip(counts).map(|((i, _), q)| f64::from(i.area()) * *q as f64).sum();
    let available = (request.stock.signed_area().abs()
        - request.cutouts.iter().map(|c| c.signed_area().abs()).sum::<f64>())
    .max(area);
    (area / available).clamp(0., 1.)
}

fn validate(request: &Request, most: usize) -> Result<usize, Error> {
    request.settings.validate().map_err(Error::new)?;
    let copies = || Error::new(format!("nest between 1 and {most} complete parts at a time"));
    let total = request
        .items
        .iter()
        .try_fold(0usize, |sum, item| sum.checked_add(item.quantity))
        .ok_or_else(copies)?;
    if total == 0
        || total > most
        || request.items.iter().any(|i| i.quantity == 0 || i.contours.is_empty())
    {
        return Err(copies());
    }
    if request.time_limit.is_zero()
        || request.time_limit > MAX_SEARCH_TIME
        || !request.machining_clearance.is_finite()
        || !(0. ..=MAX_MACHINING_CLEARANCE).contains(&request.machining_clearance)
    {
        return Err(Error::new("invalid nesting time or machining clearance".into()));
    }
    if request.items.iter().any(|item| !fits(&request.sheet_limit, load(item))) {
        return Err(Error::new(format!(
            "a part holds more than one sheet can prepare ({} contours, {} lines and arcs)",
            request.sheet_limit.contours, request.sheet_limit.curves
        )));
    }
    Ok(total)
}

/// The contours and curves one copy of `item` adds to a sheet.
fn load(item: &crate::Item) -> [usize; 2] {
    [item.contours.len(), item.contours.iter().map(|c| c.curves.len()).sum()]
}

fn fits(limit: &crate::SheetLimit, load: [usize; 2]) -> bool {
    load[0] <= limit.contours && load[1] <= limit.curves
}

fn plus(a: [usize; 2], b: [usize; 2]) -> [usize; 2] {
    [a[0].saturating_add(b[0]), a[1].saturating_add(b[1])]
}

struct Geometry {
    stock: Container,
    items: Vec<(Item, usize)>,
    origin: Point,
    item_origins: Vec<Point>,
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "validated coordinates are padded for f32 conversion"
)]
fn geometry(request: &Request, cancel: &AtomicBool) -> Result<Geometry, Error> {
    let deadline = Instant::now() + MAX_SEARCH_TIME;
    let stock_points = polygon(&request.stock)?;
    let bounds = request.stock.bounds().ok_or_else(|| Error::new("empty sheet".into()))?;
    let origin = bounds.min;
    let config = DEFAULT_SPARROW_CONFIG;
    let item_offset = request.settings.spacing / 2. + request.machining_clearance + GUARD;
    let stock_offset = request.settings.margin + GUARD;
    let item_importer =
        Importer::new(config.cde_config, None, Some((2. * item_offset) as f32), None);
    let stock_importer =
        Importer::new(config.cde_config, None, Some((2. * stock_offset) as f32), None);
    let holes = request.cutouts.iter().map(polygon).collect::<Result<Vec<_>, Error>>()?;
    let hole_vertices = holes.iter().map(Vec::len).sum::<usize>();
    if hole_vertices + stock_points.len() > MAX_TOTAL_VERTICES {
        return Err(Error::new(format!("nesting geometry exceeds {MAX_TOTAL_VERTICES} vertices")));
    }
    let stock = stock_importer
        .import_container(&ExtContainer {
            id: 0,
            shape: ext(&stock_points, origin),
            zones: holes
                .iter()
                .map(|p| ExtQualityZone { quality: 0, shape: ext(p, origin) })
                .collect(),
        })
        .map_err(err)?;
    let mut shapes = Shapes {
        importer: item_importer,
        orientations: orientations(request.settings.rotation),
        cache: BTreeMap::new(),
    };
    let mut vertices = stock_points.len() + hole_vertices;
    let mut items = Vec::new();
    let mut item_origins = Vec::new();
    for (id, input) in request.items.iter().enumerate() {
        check_cancelled(cancel)?;
        if Instant::now() >= deadline {
            return Err(Error::new(
                "shape preparation took too long; use fewer or simpler parts".into(),
            ));
        }
        let points = outer_polygon(input, id)?;
        vertices += points.len();
        if vertices > MAX_TOTAL_VERTICES {
            return Err(Error::new(format!(
                "nesting geometry exceeds {MAX_TOTAL_VERTICES} vertices"
            )));
        }
        let item_origin = Point::new(
            points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min),
            points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min),
        );
        let item = shapes.import(id, &points, item_origin)?;
        items.push((item, input.quantity));
        item_origins.push(item_origin);
    }
    Ok(Geometry { stock, items, origin, item_origins })
}

fn orientations(rotation: NestRotation) -> Option<Vec<f32>> {
    match rotation {
        NestRotation::Any => None,
        NestRotation::HalfTurn => Some(vec![0., 180.]),
        NestRotation::Across => Some(vec![90., 270.]),
        NestRotation::Fixed => Some(vec![0.]),
    }
}

struct Shapes {
    importer: Importer,
    orientations: Option<Vec<f32>>,
    cache: BTreeMap<Vec<(i64, i64)>, Item>,
}

impl Shapes {
    fn import(&mut self, id: usize, points: &[[f64; 2]], origin: Point) -> Result<Item, Error> {
        let (key, normalized) = canonical(points, origin);
        if let Some(item) = self.cache.get(&key) {
            let mut item = item.clone();
            item.id = id;
            return Ok(item);
        }
        let item = self
            .importer
            .import_item(&ExtItem {
                id: id.try_into().map_err(err)?,
                shape: ext(&normalized, Point::ORIGIN),
                allowed_orientations: self.orientations.clone(),
                min_quality: None,
            })
            .map_err(err)?;
        self.cache.insert(key, item.clone());
        Ok(item)
    }
}

fn outer_polygon(input: &crate::Item, id: usize) -> Result<Vec<[f64; 2]>, Error> {
    let outer = input
        .contours
        .iter()
        .filter(|c| c.is_closed())
        .max_by(|a, b| a.signed_area().abs().total_cmp(&b.signed_area().abs()))
        .ok_or_else(|| Error::new(format!("part {} has no closed outer contour", id + 1)))?;
    let points = polygon(outer)?;
    let poly: Vec<Point> = points.iter().copied().map(Into::into).collect();
    // The original outer arc can sit outside its own inscribed polygon.
    // Test only the other contours, once, against that boundary.
    for other in &input.contours {
        if std::ptr::eq(other, outer) {
            continue;
        }
        if other
            .curves
            .iter()
            .any(|c| (0..=8).any(|k| !contains(&poly, c.point(f64::from(k) / 8.))))
        {
            return Err(Error::new(format!(
                "part {} contains separate outer shapes; remove their connection before nesting",
                id + 1
            )));
        }
    }
    Ok(points)
}

// A 0.0001 mm quantization is covered by the 0.05 mm collision guard. Normalize
// translation and vertex order so copies reuse one expensive offset/surrogate.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "coordinates are bounded to 100 m and padded by the collision guard"
)]
fn canonical(points: &[[f64; 2]], origin: Point) -> (Vec<(i64, i64)>, Vec<[f64; 2]>) {
    let mut key: Vec<_> = points
        .iter()
        .map(|p| {
            (
                ((p[0] - origin.x) * 10_000.).round() as i64,
                ((p[1] - origin.y) * 10_000.).round() as i64,
            )
        })
        .collect();
    let first = key.iter().enumerate().min_by_key(|(_, p)| **p).map_or(0, |(i, _)| i);
    key.rotate_left(first);
    let normalized = key.iter().map(|&(x, y)| [x as f64 / 10_000., y as f64 / 10_000.]).collect();
    (key, normalized)
}

fn occupied_width(layout: &Layout) -> f32 {
    layout.placed_items.values().map(|p| p.shape.bbox.x_max).fold(0., f32::max)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "finite input coordinates are bounded and conversion is padded"
)]
fn current_layout(
    stock: &Container,
    items: &[(Item, usize)],
    origins: &[Point],
    stock_origin: Point,
    rotation: NestRotation,
) -> Option<Layout> {
    if rotation == NestRotation::Across || items.iter().any(|(_, quantity)| *quantity != 1) {
        return None;
    }
    let mut layout = Layout::new(stock.clone());
    for ((item, _), origin) in items.iter().zip(origins) {
        let external = DTransformation::new(
            0.,
            ((origin.x - stock_origin.x) as f32, (origin.y - stock_origin.y) as f32),
        );
        let transform = ext_to_int_transformation(&external, &item.shape_orig.pre_transform);
        if !matches!(
            LBFEvaluator::new(&layout, item).evaluate_sample(transform, None),
            SampleEval::Clear { .. }
        ) {
            return None;
        }
        layout.place_item(item, transform);
    }
    layout.is_feasible().then_some(layout)
}

fn rotation(angle: f32, allowed: NestRotation) -> (f64, f64) {
    if allowed == NestRotation::Any {
        return f64::from(angle).sin_cos();
    }
    // jagua stores angles in f32. Preserve the exact requested grain rotations
    // in the authoring geometry instead of accumulating tiny skews each run.
    let turn = (f64::from(angle) / std::f64::consts::FRAC_PI_2).round().rem_euclid(4.);
    if turn < 0.5 {
        (0., 1.)
    } else if turn < 1.5 {
        (1., 0.)
    } else if turn < 2.5 {
        (0., -1.)
    } else {
        (-1., 0.)
    }
}

#[allow(clippy::cast_possible_truncation, reason = "remaining time is bounded to 30 seconds")]
fn compress(
    mut layout: Layout,
    items: &[(Item, usize)],
    stock: &Container,
    budget: &mut Budget<'_>,
    rng: &mut Xoshiro256PlusPlus,
    listener: &mut impl SolutionListener,
) -> Result<Layout, Error> {
    let bbox = stock.outer_orig.shape.bbox;
    let strip = Strip::new(
        bbox.height(),
        DEFAULT_SPARROW_CONFIG.cde_config,
        ShapeModifyConfig {
            offset: stock.outer_orig.modify_config.offset,
            ..ShapeModifyConfig::default()
        },
        bbox.width(),
    )
    .map_err(err)?;
    // Rebuild under the same rectangular container identity as SPProblem.
    layout.swap_container(strip.into());
    if !layout.is_feasible() {
        return Err(Error::new("rectangular warm start failed its collision check".into()));
    }
    let instance = SPInstance::new(items.to_vec(), strip);
    // Start from a strip fitted to the first fit, as Sparrow does after its
    // own construction. From the whole sheet, it shrinks by a tenth of a
    // percent per step and spends the search loosening a tight first fit.
    let mut start = SPProblem::new(instance.clone());
    start.restore(&SPSolution {
        strip,
        layout_snapshot: layout.save(),
        time_stamp: jagua_rs::Instant::now(),
    });
    start.fit_strip();
    let initial = start.save();
    let fitted = initial.strip.width;
    let remaining = budget.deadline.saturating_duration_since(Instant::now());
    let mut config = DEFAULT_SPARROW_CONFIG;
    config.expl_cfg.time_limit = remaining.mul_f64(EXPLORE_SHARE);
    config.cmpr_cfg.time_limit = remaining.mul_f64(COMPRESS_SHARE);
    config.expl_cfg.separator_config.n_workers = crate::SEARCH_THREADS;
    config.cmpr_cfg.separator_config.n_workers = crate::SEARCH_THREADS;
    // Sparrow panics once it shrinks the strip below a part it must place,
    // which a sheet of few parts can start close to. The first fit stands.
    let optimized = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        sparrow::optimizer::optimize(
            instance,
            rng.clone(),
            listener,
            budget,
            &config.expl_cfg,
            &config.cmpr_cfg,
            Some(&initial),
        )
    }));
    let Ok(result) = optimized else { return Ok(layout) };
    let result = result.map_err(err)?;
    let candidate = Layout::from_snapshot(&result.layout_snapshot);
    if candidate.is_feasible() && result.strip.width <= fitted { Ok(candidate) } else { Ok(layout) }
}
