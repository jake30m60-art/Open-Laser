// SPDX-License-Identifier: GPL-3.0-or-later
//! Nesting through stock selection, preview, Apply, history and persistence.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "known offline fixtures")]
#![allow(clippy::float_cmp, reason = "sheet sizes are set exactly")]
mod common;

use common::start;
use openlaser_core::LaserMode;
use openlaser_core::features::Features;
use openlaser_core::geometry::{Contour, Curve, Drawing, Point, Transform};
use openlaser_core::nesting::{NestRotation, NestSettings};
use openlaser_library::placement::Placement;
use openlaser_server::coordinator::{NewRecipe, Shared, Values};
use openlaser_server::nesting::{
    self, NestRequest, NestView, PartQuantity, StockChoice, StockSource,
};
use openlaser_server::{Coordinator, machine};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Contour {
    let points = [[x, y], [x + width, y], [x + width, y + height], [x, y + height]];
    Contour {
        layer: "Cut".into(),
        curves: (0..4)
            .map(|i| Curve::Line { start: points[i].into(), end: points[(i + 1) % 4].into() })
            .collect(),
    }
}
async fn setup(shared: &Shared) {
    let mut c = shared.lock().await;
    let drawing = Drawing {
        contours: vec![
            rect(0., 0., 240., 160.),
            rect(20., 20., 30., 20.),
            Contour {
                layer: "Hole".into(),
                curves: vec![Curve::Arc {
                    center: Point::new(30., 30.),
                    radius: 3.,
                    start_angle: 0.,
                    sweep: std::f64::consts::TAU,
                }],
            },
            rect(100., 50., 20., 20.),
        ],
    };
    // Synthetic geometry enters the same library path as a parsed DXF.
    let part = c.library.add_part("nesting.dxf", b"nesting fixture geometry", drawing).unwrap();
    let recipe = c
        .add_recipe(&NewRecipe {
            name: "Nest steel".into(),
            laser: LaserMode::Fiber,
            thickness_mm: 1.,
            values: Values::Bank(1),
            gas: None,
        })
        .unwrap();
    c.open_part(&part.id).unwrap();
    c.set_recipe(&recipe.id).unwrap();
    c.set_features(Features::default()).unwrap();
    drop(c);
    machine::prepare(shared).await.unwrap();
    shared.lock().await.set_stock(StockChoice::Outline { contour: 0 }).unwrap();
    machine::prepare(shared).await.unwrap();
}
fn request() -> NestRequest {
    NestRequest {
        contours: vec![0],
        quantity: 4,
        settings: NestSettings {
            remnant_clearance: 0.,
            spacing: 4.,
            margin: 2.,
            rotation: NestRotation::HalfTurn,
        },
        seconds: 1,
        stock: vec![],
        quantities: Vec::new(),
        common_edges: false,
    }
}

#[tokio::test]
async fn numbered_sheets_switch_persist_and_save_as_one_history_edit() {
    let (_sim, shared) = start("numbered-sheets").await;
    setup(&shared).await;
    let revision = shared.lock().await.document().draft_revision;
    let mut request = request();
    request.quantity = 65;
    request.seconds = 5;
    // The drawing's outline is one sheet; no more open unasked.
    let task = nesting::start(&shared, revision, request.clone()).await.unwrap();
    let full = finished(&shared, task.id).await;
    assert!(full.needs_sheets, "{:?}", full.error);
    assert!(full.placed < 66 && full.error.as_deref().is_some_and(|e| e.contains("add sheets")));
    // The operator chooses the next sheets.
    request.stock = vec![StockSource::Sheet { width: 240., height: 160., count: 3 }];
    let task = nesting::start(&shared, revision, request).await.unwrap();
    let result = finished(&shared, task.id).await;
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(!result.needs_sheets);
    assert!(result.sheets.len() > 1);
    assert_eq!(result.sheets.iter().map(|p| p.parts).sum::<usize>(), 66);
    let other = nesting::sheet_preview(&shared, task.id, 1).await.unwrap();
    assert!(other.preview.is_some());
    nesting::apply(&shared, task.id).await.unwrap();
    let count = result.sheets.len();
    let first = shared.lock().await.draft.as_ref().unwrap().current.placed.clone();
    shared.lock().await.select_sheet(1).unwrap();
    machine::prepare(&shared).await.unwrap();
    let second = shared.lock().await.draft.as_ref().unwrap().current.placed.clone();
    assert_ne!(first, second);
    shared.lock().await.select_sheet(0).unwrap();
    machine::prepare(&shared).await.unwrap();
    assert_eq!(shared.lock().await.draft.as_ref().unwrap().current.placed, first);
    shared.lock().await.select_sheet(1).unwrap();
    machine::prepare(&shared).await.unwrap();
    let mut c = shared.lock().await;
    let saved = c.save_job("Bracket batch").unwrap();
    assert_eq!(saved.sheet.as_ref().unwrap().number, 2);
    assert_eq!(c.library.jobs().count(), count);
    assert!(
        c.library
            .jobs()
            .all(|j| j.folder == saved.folder && j.sheet.as_ref().unwrap().total as usize == count)
    );
    assert_eq!(c.document().draft.unwrap().sheets.as_ref().unwrap().active, 1);
    let root = c.config.data_dir.clone();
    let history = c.library.edit_history().unwrap();
    c.library.undo_saved(true, history.revision).unwrap();
    assert_eq!(c.library.jobs().count(), 0);
    let revision = c.library.edit_history().unwrap().revision;
    c.library.undo_saved(false, revision).unwrap();
    let reopened = openlaser_library::Library::open(root).unwrap();
    assert_eq!(reopened.jobs().count(), count);
    assert_eq!(reopened.job(&saved.id).unwrap().placed, second);
}
async fn finished(shared: &Shared, id: u64) -> NestView {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let view = nesting::status(shared, id).await.unwrap();
        if !view.running {
            return view;
        }
        assert!(tokio::time::Instant::now() < deadline, "nesting worker did not finish");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn stock_quantity_one_undo_save_and_restart() {
    let (_sim, shared) = start("nest-history").await;
    setup(&shared).await;
    let (before, past, revision) = {
        let c = shared.lock().await;
        let d = c.draft.as_ref().unwrap();
        assert_eq!(d.groups.len(), 2, "removed stock must not keep all enclosed parts grouped");
        assert_eq!(d.current.placed.len(), 3);
        assert!(d.current.placed.iter().all(|p| p.source != 0));
        (d.current.placed.clone(), d.view().past, c.document().draft_revision)
    };
    let work = nesting::start(&shared, revision, request()).await.unwrap();
    let view = finished(&shared, work.id).await;
    assert!(view.error.is_none(), "{:?}", view.error);
    assert_eq!(view.total, 5);
    assert_eq!(
        shared.lock().await.draft.as_ref().unwrap().current.placed,
        before,
        "preview must not mutate the draft"
    );
    nesting::apply(&shared, work.id).await.unwrap();
    let (placed, stock, config) = {
        let c = shared.lock().await;
        let d = c.draft.as_ref().unwrap();
        assert_eq!(d.current.placed.len(), 9);
        assert_eq!(d.groups.len(), 5);
        assert_eq!(d.view().past, past + 1);
        assert_eq!(d.current.placed.iter().filter(|p| p.source == 2).count(), 4);
        assert!(d.current.placed.iter().all(|p| p.source != 0));
        assert!(d.prepared.is_some());
        (d.current.placed.clone(), d.current.nesting.clone(), c.config.clone())
    };
    shared.lock().await.undo().unwrap();
    machine::prepare(&shared).await.unwrap();
    assert_eq!(shared.lock().await.draft.as_ref().unwrap().current.placed, before);
    shared.lock().await.redo().unwrap();
    machine::prepare(&shared).await.unwrap();
    let job = shared.lock().await.save_job("Nested sheet").unwrap();
    machine::compile(&shared, true).await.unwrap();
    assert_eq!(
        shared.lock().await.draft.as_ref().unwrap().compiled.as_ref().unwrap().view.plan.len(),
        9
    );
    openlaser_server::shutdown(&shared).await.unwrap();
    let reopened = Coordinator::start(config).unwrap();
    common::until(&reopened, 5, |d| {
        d.draft.as_ref().is_some_and(|draft| draft.preview.is_some() && draft.error.is_none())
    })
    .await;
    let c = reopened.lock().await;
    let d = c.draft.as_ref().unwrap();
    assert_eq!(d.current.placed, placed);
    assert_eq!(d.current.nesting, stock);
    assert_eq!(c.library.job(&job.id).unwrap().nesting, stock);
    drop(c);
    openlaser_server::shutdown(&reopened).await.unwrap();
}

#[tokio::test]
async fn edited_and_cancelled_results_cannot_apply() {
    let (_sim, shared) = start("nest-stale").await;
    setup(&shared).await;
    let revision = shared.lock().await.document().draft_revision;
    let work = nesting::start(&shared, revision, request()).await.unwrap();
    let view = finished(&shared, work.id).await;
    assert!(view.error.is_none(), "{:?}", view.error);
    shared.lock().await.transform(&[0], Some(Transform::translation(Point::new(1., 0.)))).unwrap();
    assert!(nesting::apply(&shared, work.id).await.is_err());
    machine::prepare(&shared).await.unwrap();
    let revision = shared.lock().await.document().draft_revision;
    let before = shared.lock().await.draft.as_ref().unwrap().current.placed.clone();
    let work = nesting::start(&shared, revision, request()).await.unwrap();
    nesting::cancel(&shared, work.id).await.unwrap();
    let view = finished(&shared, work.id).await;
    assert!(view.error.is_some());
    assert!(nesting::apply(&shared, work.id).await.is_err());
    assert_eq!(shared.lock().await.draft.as_ref().unwrap().current.placed, before);
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn rectangle_keeps_the_fixture_position_after_nesting() {
    let (_sim, shared) = start("nest-rectangle-origin").await;
    setup(&shared).await;
    {
        let mut c = shared.lock().await;
        // The sheet has its own origin on the bed, apart from the job origin.
        c.set_origin([146., 673.]).unwrap();
        c.set_sheet_corner(Some([146., 673.]), false, None).unwrap();
        c.set_stock(StockChoice::Rectangle { width: 240., height: 160., min: None }).unwrap();
    }
    machine::prepare(&shared).await.unwrap();
    let revision = shared.lock().await.document().draft_revision;
    let work = nesting::start(&shared, revision, request()).await.unwrap();
    let view = finished(&shared, work.id).await;
    assert!(view.error.is_none(), "{:?}", view.error);
    nesting::apply(&shared, work.id).await.unwrap();
    let c = shared.lock().await;
    let d = c.draft.as_ref().unwrap();
    let drawing = d.drawing().unwrap();
    let bounds =
        nesting::stock(drawing, d.current.nesting.as_ref().unwrap()).unwrap().bounds().unwrap();
    assert!((bounds.min.x + d.sheet_offset().unwrap()[0] - 146.).abs() < 1e-9);
    assert!((bounds.min.y + d.sheet_offset().unwrap()[1] - 673.).abs() < 1e-9);
    assert!(
        matches!(d.current.placement, Some(Placement::Fixed { .. })),
        "nesting on a sheet lying on the bed fixes the job origin"
    );
    drop(c);
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn every_part_keeps_its_own_quantity() {
    let (_sim, shared) = start("nest-quantities").await;
    setup(&shared).await;
    let groups = shared.lock().await.draft.as_ref().unwrap().groups.clone();
    assert_eq!(groups.len(), 2);
    let quantities = vec![
        PartQuantity { contours: groups[0].clone(), quantity: 3 },
        PartQuantity { contours: groups[1].clone(), quantity: 2 },
    ];
    // Without a selection, and with one: each part keeps its own count.
    for contours in [vec![], groups[1].clone()] {
        let wanted =
            NestRequest { contours, quantity: 1, quantities: quantities.clone(), ..request() };
        let revision = shared.lock().await.document().draft_revision;
        let job = nesting::start(&shared, revision, wanted).await.unwrap();
        let view = finished(&shared, job.id).await;
        assert!(view.error.is_none(), "{:?}", view.error);
        assert_eq!(view.total, 5);
    }
    let zero = NestRequest {
        contours: vec![],
        quantity: 1,
        quantities: vec![PartQuantity { contours: groups[0].clone(), quantity: 0 }],
        ..request()
    };
    let revision = shared.lock().await.document().draft_revision;
    assert!(nesting::start(&shared, revision, zero).await.is_err());
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn repeat_without_selection_and_retry_after_no_fit_preserve_counts() {
    let (_sim, shared) = start("nest-repeat").await;
    setup(&shared).await;
    let revision = shared.lock().await.document().draft_revision;
    let first = nesting::start(&shared, revision, request()).await.unwrap();
    assert!(finished(&shared, first.id).await.error.is_none());
    nesting::apply(&shared, first.id).await.unwrap();
    let again = NestRequest { contours: vec![], quantity: 1, ..request() };
    let revision = shared.lock().await.document().draft_revision;
    let second = nesting::start(&shared, revision, again.clone()).await.unwrap();
    let view = finished(&shared, second.id).await;
    assert!(view.error.is_none(), "{:?}", view.error);
    assert_eq!(view.total, 5, "no selection means all current parts at their existing counts");
    nesting::apply(&shared, second.id).await.unwrap();
    let before = shared.lock().await.draft.as_ref().unwrap().current.placed.clone();
    shared
        .lock()
        .await
        .set_stock(StockChoice::Rectangle { width: 15., height: 15., min: None })
        .unwrap();
    machine::prepare(&shared).await.unwrap();
    let revision = shared.lock().await.document().draft_revision;
    let failed = nesting::start(&shared, revision, again.clone()).await.unwrap();
    assert!(finished(&shared, failed.id).await.error.unwrap().contains("add a larger sheet"));
    assert!(nesting::apply(&shared, failed.id).await.is_err());
    assert_eq!(shared.lock().await.draft.as_ref().unwrap().current.placed, before);
    shared
        .lock()
        .await
        .set_stock(StockChoice::Rectangle { width: 240., height: 160., min: None })
        .unwrap();
    machine::prepare(&shared).await.unwrap();
    let revision = shared.lock().await.document().draft_revision;
    let retry = nesting::start(&shared, revision, again).await.unwrap();
    let view = finished(&shared, retry.id).await;
    assert!(view.error.is_none(), "{:?}", view.error);
    assert_eq!(view.total, 5);
    nesting::apply(&shared, retry.id).await.unwrap();
    openlaser_server::shutdown(&shared).await.unwrap();
}

#[tokio::test]
async fn chosen_sheets_fill_in_order_within_what_is_on_hand() {
    use openlaser_server::inventory::NewStock;
    use openlaser_server::nesting::StockSource;
    let (_sim, shared) = start("nest-stock").await;
    setup(&shared).await;
    let rack = |material: &str, quantity| NewStock {
        material: material.into(),
        thickness_mm: 1.,
        laser: LaserMode::Fiber,
        width_mm: 100.,
        height_mm: 60.,
        quantity,
        folder: None,
    };
    let (revision, steel, other) = {
        let mut c = shared.lock().await;
        let steel = c.add_stock(rack("nest STEEL", 2)).unwrap();
        let turned = NewStock { width_mm: 60., height_mm: 100., ..rack("Nest steel", 1) };
        assert_eq!(c.add_stock(turned).unwrap(), steel, "the same size either way round adds up");
        let other = c.add_stock(rack("Brass", 5)).unwrap();
        (c.document().draft_revision, steel, other)
    };
    let asking = |stock| NestRequest { quantity: 12, stock, ..request() };
    let refused = [
        vec![StockSource::Stock { id: steel.clone(), count: 4 }],
        vec![
            StockSource::Stock { id: steel.clone(), count: 2 },
            StockSource::Stock { id: steel.clone(), count: 2 },
        ],
        vec![StockSource::Stock { id: other, count: 1 }],
    ];
    for stock in refused {
        assert!(nesting::start(&shared, revision, asking(stock)).await.is_err());
    }
    let stock = vec![
        StockSource::Stock { id: steel.clone(), count: 1 },
        StockSource::Sheet { width: 80., height: 50., count: 5 },
    ];
    let task = nesting::start(&shared, revision, asking(stock)).await.unwrap();
    let result = finished(&shared, task.id).await;
    assert!(result.error.is_none(), "{:?}", result.error);
    let sources: Vec<_> = result.sheets.iter().map(|s| s.source).collect();
    assert_eq!(sources[0], 0, "the rack sheet fills first");
    assert!(sources.len() > 1 && sources[1..].iter().all(|&s| s == 1), "{sources:?}");
    let [width, height] = result.sheets[0].size;
    assert!((width - 100.).abs() < 1e-9 && (height - 60.).abs() < 1e-9);
    assert_eq!(result.sheets.iter().map(|s| s.parts).sum::<usize>(), 13);
    nesting::apply(&shared, task.id).await.unwrap();
    machine::prepare(&shared).await.unwrap();
    let mut c = shared.lock().await;
    let job = c.save_job("Rack batch").unwrap();
    c.report_cut_sheet(&job.id).unwrap();
    let left = c.inventory.iter().find(|i| i.id == steel).unwrap().quantity;
    assert_eq!(left, 3, "a cut marked afterwards left the rack before it was counted");
    assert_eq!(c.document().library.stock.len(), 2);
}

#[tokio::test]
async fn new_sheets_on_auto_take_as_many_as_the_parts_need() {
    let (_sim, shared) = start("auto-sheets").await;
    setup(&shared).await;
    let revision = shared.lock().await.document().draft_revision;
    let mut request = request();
    request.quantity = 65;
    request.seconds = 5;
    // 0: no count chosen, the search opens sheets until every part sits.
    request.stock = vec![StockSource::Sheet { width: 240., height: 160., count: 0 }];
    let task = nesting::start(&shared, revision, request).await.unwrap();
    let result = finished(&shared, task.id).await;
    assert!(result.error.is_none(), "{:?}", result.error);
    assert!(!result.needs_sheets);
    assert!(result.sheets.len() > 1, "more than one sheet was needed");
    assert_eq!(result.sheets.iter().map(|p| p.parts).sum::<usize>(), 66);
    assert!(result.sheets.iter().all(|p| p.parts > 0), "no empty sheet is opened");
}

#[tokio::test]
async fn a_sheet_is_placed_where_it_was_moved_and_turned_about_its_corner() {
    let (_sim, shared) = start("sheet-place").await;
    setup(&shared).await;
    let mut c = shared.lock().await;
    let bounds =
        |c: &openlaser_server::Coordinator| match &c.draft.as_ref().unwrap().current.nesting {
            Some(n) => match n.stock {
                openlaser_core::nesting::NestStock::Rectangle { bounds } => {
                    [bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y]
                }
                _ => panic!("not a rectangle"),
            },
            None => panic!("no sheet"),
        };
    c.set_stock(StockChoice::Rectangle { width: 300., height: 200., min: Some([-10., 5.]) })
        .unwrap();
    assert_eq!(bounds(&c), [-10., 5., 290., 205.]);
    // A quarter turn: the sizes swap about the same front-left corner.
    c.set_stock(StockChoice::Rectangle { width: 200., height: 300., min: Some([-10., 5.]) })
        .unwrap();
    assert_eq!(bounds(&c), [-10., 5., 190., 305.]);
    assert!(c.set_stock(StockChoice::Rectangle { width: 0., height: 300., min: None }).is_err());
}

/// Thousands of copies fill sheets in grids within moments; at zero
/// spacing they touch and their shared edges are cut once.
#[tokio::test]
async fn thousands_of_copies_fill_sheets_in_grids_with_common_edges() {
    let (_sim, shared) = start("nest-thousands").await;
    setup(&shared).await;
    let groups = shared.lock().await.draft.as_ref().unwrap().groups.clone();
    let square = groups.iter().find(|g| g.len() == 1).unwrap().clone();
    let revision = shared.lock().await.document().draft_revision;
    let wanted = NestRequest {
        contours: square,
        quantity: 2000,
        settings: NestSettings {
            remnant_clearance: 0.,
            spacing: 0.,
            margin: 2.,
            rotation: NestRotation::Fixed,
        },
        seconds: 5,
        stock: vec![StockSource::Sheet { width: 500., height: 300., count: 0 }],
        common_edges: true,
        ..request()
    };
    let started = std::time::Instant::now();
    let task = nesting::start(&shared, revision, wanted).await.unwrap();
    let view = finished(&shared, task.id).await;
    assert!(view.error.is_none(), "{:?}", view.error);
    assert!(view.notice.is_none(), "{:?}", view.notice);
    assert!(started.elapsed() < std::time::Duration::from_secs(30), "{:?}", started.elapsed());
    assert_eq!(view.sheets.iter().map(|s| s.parts).sum::<usize>(), 2001);
    // 24 columns and 14 rows of 20 mm squares within the 2 mm margin.
    assert_eq!(view.sheets[0].parts, 336);
    nesting::apply(&shared, task.id).await.unwrap();
    let c = shared.lock().await;
    let draft = c.draft.as_ref().unwrap();
    let common = draft.current.features.common.as_ref().expect("common edges taken");
    assert_eq!(common.contours.len(), 336);
    assert!(draft.error.is_none(), "{:?}", draft.error);
    drop(c);
    openlaser_server::shutdown(&shared).await.unwrap();
}

/// Machining chosen for one part's contours applies to every nested copy.
#[tokio::test]
async fn a_parts_own_machining_follows_every_copy() {
    let (_sim, shared) = start("nest-own-machining").await;
    setup(&shared).await;
    let groups = shared.lock().await.draft.as_ref().unwrap().groups.clone();
    let part = groups.iter().find(|g| g.len() == 2).unwrap().clone();
    {
        let mut c = shared.lock().await;
        let mut features = c.draft.as_ref().unwrap().current.features.clone();
        features.contour_machining = vec![openlaser_core::features::ContourMachining {
            contours: part.clone(),
            own: vec![openlaser_core::features::MachiningPart::Start],
            machining: openlaser_core::features::Machining::default(),
        }];
        c.set_features(features).unwrap();
    }
    machine::prepare(&shared).await.unwrap();
    let revision = shared.lock().await.document().draft_revision;
    let wanted = NestRequest { contours: part, quantity: 3, ..request() };
    let task = nesting::start(&shared, revision, wanted).await.unwrap();
    let view = finished(&shared, task.id).await;
    assert!(view.error.is_none(), "{:?}", view.error);
    nesting::apply(&shared, task.id).await.unwrap();
    let c = shared.lock().await;
    let draft = c.draft.as_ref().unwrap();
    let own = &draft.current.features.contour_machining;
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].contours.len(), 6, "both contours of all three copies");
    drop(c);
    openlaser_server::shutdown(&shared).await.unwrap();
}
