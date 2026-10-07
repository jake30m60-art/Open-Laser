// SPDX-License-Identifier: GPL-3.0-or-later
//! A sheet separation line staged through the normal Setup and Run workflow.
use crate::draft::Draft;
use crate::{Coordinator, Error, Result};
use openlaser_core::geometry::{Contour, Curve, Drawing, Point};
use openlaser_library::{Id, placement::Placement};
use serde::Deserialize;

/// Machine coordinates for a straight separation cut and its cutting recipe.
#[cfg_attr(feature = "typescript", derive(ts_rs::TS), ts(export))]
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualCut {
    /// Cutting recipe, including power, gas and piercing.
    pub recipe: Id,
    /// First point in machine millimetres.
    pub start: [f64; 2],
    /// Last point in machine millimetres.
    pub end: [f64; 2],
}

impl Coordinator {
    /// Stages a separation cut; never starts machine motion or turns on the beam.
    pub fn manual_cut(&mut self, cut: ManualCut) -> Result<()> {
        let bed = crate::correction::bed(self)?;
        for point in [cut.start, cut.end] {
            if point.iter().any(|v| !v.is_finite())
                || point[0] < bed.min.x
                || point[0] > bed.max.x
                || point[1] < bed.min.y
                || point[1] > bed.max.y
            {
                return Err(Error::Request(
                    "manual cut endpoints must lie inside the machine travel".into(),
                ));
            }
        }
        let start = Point::new(cut.start[0], cut.start[1]);
        let end = Point::new(cut.end[0], cut.end[1]);
        if start.distance(end) < 0.01 {
            return Err(Error::Request("a manual cut must be at least 0.01 mm long".into()));
        }
        let recipe =
            self.library.recipes().find(|r| r.id == cut.recipe).cloned().ok_or_else(|| {
                Error::Request("choose a cutting recipe for the manual cut".into())
            })?;
        if Some(recipe.laser) != self.mode
            || recipe.attributes.get("OpenLaserProcess").is_some_and(|v| v == "engrave")
        {
            return Err(Error::Request(
                "manual cut needs a cutting recipe for the active laser".into(),
            ));
        }
        let drawing = Drawing {
            contours: vec![Contour {
                layer: "Manual cut".into(),
                curves: vec![Curve::Line { start, end }],
            }],
        };
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"{} {} {} {}\"><line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"black\"/></svg>",
            bed.width(),
            bed.height(),
            bed.min.x,
            bed.min.y,
            bed.width(),
            bed.height(),
            start.x,
            start.y,
            end.x,
            end.y
        );
        self.queue_draft();
        let part = self.library.add_part("Manueller Trennschnitt.svg", svg.as_bytes(), drawing)?;
        let source = self.library.job_drawing(std::slice::from_ref(&part.id))?;
        let mut draft = Draft::new(std::sync::Arc::new(source));
        draft.current.recipe = Some(recipe);
        draft.current.placement = Some(Placement::Fixed { origin: bed.min.into() });
        draft.current.sheet_offset = Some([0., 0.]);
        self.draft_generation += 1;
        self.draft = Some(draft);
        self.library_changed();
        self.reprepare();
        Ok(())
    }
}
