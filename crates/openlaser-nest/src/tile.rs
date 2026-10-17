// SPDX-License-Identifier: GPL-3.0-or-later

//! Many copies of one part in rows and columns. A searched layout places
//! each copy on its own, which takes too long for thousands; a grid of the
//! part's exact extent is computed at once and repeated across sheets. At
//! zero spacing the copies touch, so straight edges can be cut once.

use crate::{Error, Item, Placement, Request, Sheet, SheetSolution, Solution};
use openlaser_core::geometry::{Bounds, Point, Transform};
use openlaser_core::nesting::NestRotation;

/// Copies from which a part is laid out in a grid rather than searched,
/// when it fills its extent at least to [`SQUARE`]. A grid of such parts
/// is close to what a search finds, in a fraction of the time.
pub(crate) const TILE_FROM: usize = 50;
/// Copies from which any other part is laid out in a grid: a search of
/// this many would rarely finish in its time.
pub(crate) const TILE_FROM_IRREGULAR: usize = 150;
/// Share of its extent a part must fill to count as square-cornered.
const SQUARE: f64 = 0.85;
/// Share of its extent a rectangle fills, allowing for rounding.
const RECTANGULAR: f64 = 0.9999;
/// Keeps a copy at the margin from failing the containment check by a
/// rounding error.
const EDGE: f64 = 1e-6;

/// One copy's place in a grid: its extent's lower corner and quarter turns.
#[derive(Clone, Copy, Debug)]
struct Cell {
    x: f64,
    y: f64,
    turns: u8,
}

/// The quarter turns a rotation setting allows.
fn turns(rotation: NestRotation) -> &'static [u8] {
    match rotation {
        NestRotation::Any => &[0, 1, 2, 3],
        NestRotation::HalfTurn => &[0, 2],
        NestRotation::Across => &[1, 3],
        NestRotation::Fixed => &[0],
    }
}

/// The exact rotation by `turns` quarter turns about the origin.
fn turn(turns: u8) -> Transform {
    let (sin, cos) = match turns % 4 {
        0 => (0., 1.),
        1 => (1., 0.),
        2 => (0., -1.),
        _ => (-1., 0.),
    };
    Transform([cos, sin, -sin, cos, 0., 0.])
}

/// The part's exact extent after `turns` quarter turns.
fn extent(item: &Item, turns: u8) -> Option<Bounds> {
    let rotate = turn(turns);
    item.contours.iter().filter_map(|c| rotate.contour(c).bounds()).reduce(Bounds::union)
}

/// The outer area of a part: its largest closed contour.
pub(crate) fn area(item: &Item) -> f64 {
    item.contours.iter().filter(|c| c.is_closed()).map(|c| c.signed_area().abs()).fold(0., f64::max)
}

/// The largest share of its extent the part fills in an allowed orientation.
fn filled(item: &Item, rotation: NestRotation) -> f64 {
    turns(rotation)
        .iter()
        .filter_map(|&t| extent(item, t))
        .map(|b| area(item) / ((b.max.x - b.min.x) * (b.max.y - b.min.y)).max(f64::MIN_POSITIVE))
        .fold(0., f64::max)
}

/// Whether copies of a rectangular part are wanted edge to edge: at zero
/// spacing only a grid lays them flush for a common cut, so they never go
/// to the search.
pub(crate) fn flush(item: &Item, request: &Request) -> bool {
    request.settings.spacing == 0.
        && item.quantity > 1
        && filled(item, request.settings.rotation) >= RECTANGULAR
}

/// Whether this part's copies go in a grid rather than through a search:
/// many of them, more than a search places in its time altogether, or
/// rectangles wanted edge to edge.
pub(crate) fn bulk(item: &Item, request: &Request, total: usize, search: usize) -> bool {
    if total > search || flush(item, request) {
        return true;
    }
    let from = if filled(item, request.settings.rotation) >= SQUARE {
        TILE_FROM
    } else {
        TILE_FROM_IRREGULAR
    };
    item.quantity >= from
}

/// What a grid needs to know about the sheet and the part.
pub(crate) struct Grid {
    /// The usable rectangle: the sheet less its margin and machining reach.
    min: Point,
    size: [f64; 2],
    /// The part's extent in each allowed orientation.
    shapes: Vec<(u8, Bounds)>,
    /// Distance between neighbouring extents.
    gap: f64,
}

impl Grid {
    /// A grid for `item` on `sheet`, or `None` when the sheet is not a
    /// plain rectangle or the part fits on it in no orientation.
    pub(crate) fn new(request: &Request, sheet: &Sheet, item: &Item) -> Option<Self> {
        if !sheet.rectangular || !sheet.cutouts.is_empty() {
            return None;
        }
        let bounds = sheet.stock.bounds()?;
        let inset = sheet.margin + request.machining_clearance + crate::polygon::CHECK_REACH + EDGE;
        let min = Point::new(bounds.min.x + inset, bounds.min.y + inset);
        let size =
            [bounds.max.x - bounds.min.x - 2. * inset, bounds.max.y - bounds.min.y - 2. * inset];
        // At zero spacing the copies touch: a common cut runs between them.
        let gap = if request.settings.spacing > 0. {
            request.settings.spacing + 2. * request.machining_clearance + EDGE
        } else {
            0.
        };
        let shapes: Vec<_> = turns(request.settings.rotation)
            .iter()
            .filter_map(|&t| Some((t, extent(item, t)?)))
            .collect();
        let grid = Self { min, size, shapes, gap };
        (grid.capacity() > 0).then_some(grid)
    }

    /// How many fit in `length` at `step` per copy and `gap` between.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite, non-negative count of copies along one sheet edge"
    )]
    fn along(&self, length: f64, step: f64) -> usize {
        if step <= 0. || length <= 0. {
            return 0;
        }
        // A tolerance keeps an exact fit from losing its last copy to rounding.
        ((length + self.gap) / (step + self.gap) + 1e-9).floor().clamp(0., 1e6) as usize
    }

    /// The best arrangement: columns of one orientation, then the strip
    /// they leave filled with another, by columns or by rows.
    #[allow(
        clippy::cast_precision_loss,
        clippy::many_single_char_names,
        reason = "copy counts along one sheet edge are at most a million"
    )]
    fn cells(&self) -> Vec<Cell> {
        let [width, height] = self.size;
        let span = |b: &Bounds| [b.max.x - b.min.x, b.max.y - b.min.y];
        let mut best: Option<(usize, bool, usize, usize, usize)> = None;
        for (i, a) in self.shapes.iter().enumerate() {
            for (j, b) in self.shapes.iter().enumerate() {
                let ([wa, ha], [wb, hb]) = (span(&a.1), span(&b.1));
                for across in [false, true] {
                    // Across, the main block takes rows and the strip lies above.
                    let (main, side, step_a, other_a, step_b, other_b) = if across {
                        (height, width, ha, wa, hb, wb)
                    } else {
                        (width, height, wa, ha, wb, hb)
                    };
                    let lines = self.along(main, step_a);
                    let per_line = self.along(side, other_a);
                    for k in 0..=lines {
                        let used = k as f64 * (step_a + self.gap);
                        let strip = self.along(main - used, step_b) * self.along(side, other_b);
                        let count = k * per_line + strip;
                        if best.is_none_or(|(n, ..)| count > n) {
                            best = Some((count, across, i, j, k));
                        }
                    }
                }
            }
        }
        let Some((_, across, i, j, k)) = best else { return Vec::new() };
        let (a, b) = (self.shapes[i], self.shapes[j]);
        let ([wa, ha], [wb, hb]) = (span(&a.1), span(&b.1));
        let mut cells = Vec::new();
        let mut block = |turns: u8, start: Point, step: [f64; 2], count: [usize; 2]| {
            for c in 0..count[0] {
                for r in 0..count[1] {
                    cells.push(Cell {
                        x: start.x + c as f64 * (step[0] + self.gap),
                        y: start.y + r as f64 * (step[1] + self.gap),
                        turns,
                    });
                }
            }
        };
        if across {
            let used = k as f64 * (ha + self.gap);
            block(a.0, self.min, [wa, ha], [self.along(width, wa), k]);
            let start = Point::new(self.min.x, self.min.y + used);
            block(b.0, start, [wb, hb], [self.along(width, wb), self.along(height - used, hb)]);
        } else {
            let used = k as f64 * (wa + self.gap);
            block(a.0, self.min, [wa, ha], [k, self.along(height, ha)]);
            let start = Point::new(self.min.x + used, self.min.y);
            block(b.0, start, [wb, hb], [self.along(width - used, wb), self.along(height, hb)]);
        }
        // Columns from the sheet's start, so a part-filled sheet leaves its
        // offcut in one piece at the far end.
        cells.sort_by(|p, q| p.x.total_cmp(&q.x).then(p.y.total_cmp(&q.y)));
        cells
    }

    /// Copies one empty sheet holds.
    pub(crate) fn capacity(&self) -> usize {
        self.cells().len()
    }

    /// The first `count` copies of item `id`, from the sheet's start.
    pub(crate) fn place(&self, item: &Item, id: usize, count: usize) -> Vec<Placement> {
        self.cells()
            .into_iter()
            .take(count)
            .filter_map(|cell| {
                let rotate = turn(cell.turns);
                let b = extent(item, cell.turns)?;
                let mut transform = rotate;
                transform.0[4] = cell.x - b.min.x;
                transform.0[5] = cell.y - b.min.y;
                Some(Placement { item: id, transform })
            })
            .collect()
    }
}

/// One sheet of a grid, checked against the sheet's own curves.
pub(crate) fn sheet(
    request: &Request,
    sheet: &Sheet,
    kind: usize,
    placements: Vec<Placement>,
) -> Result<SheetSolution, Error> {
    let moved: Vec<_> = placements
        .iter()
        .flat_map(|p| request.items[p.item].contours.iter().map(|c| p.transform.contour(c)))
        .collect();
    crate::check_region(&sheet.stock, &sheet.cutouts, &moved, sheet.margin)?;
    let parts: f64 = placements.iter().map(|p| area(&request.items[p.item])).sum();
    let available = sheet.stock.signed_area().abs().max(parts);
    Ok(SheetSolution {
        sheet: kind,
        layout: Solution { placements, coverage: (parts / available).clamp(0., 1.) },
    })
}
