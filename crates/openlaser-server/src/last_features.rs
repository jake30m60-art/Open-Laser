// SPDX-License-Identifier: GPL-3.0-or-later

//! The machining the operator set last, kept apart from the library so a
//! new job starts from it even before any job was saved.

use openlaser_core::features::Features;
use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "last-features.json";

/// The last machining set on any job, and when.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Last {
    /// Reusable machining: no picked places or layer choices.
    pub features: Features,
    /// Seconds since the epoch.
    pub at: u64,
}

/// The saved machining, if any could be read.
#[must_use]
pub fn open(root: &Path) -> Option<Last> {
    let bytes = std::fs::read(root.join(FILE)).ok()?;
    serde_json::from_slice(&bytes)
        .map_err(|error| tracing::warn!(%error, "last machining unreadable"))
        .ok()
}

/// Keeps `features` as the last machining set; failures only warn.
pub fn save(root: &Path, features: &Features) -> Last {
    let last = Last { features: features.reusable(), at: openlaser_library::now() };
    match serde_json::to_vec_pretty(&last) {
        Ok(bytes) => {
            if let Err(error) = openlaser_library::atomic_write(&root.join(FILE), &bytes) {
                tracing::warn!(%error, "last machining not saved");
            }
        }
        Err(error) => tracing::warn!(%error, "last machining not saved"),
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_machining_is_kept_without_picked_places() {
        let root = std::env::temp_dir().join(format!("openlaser-last-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(open(&root).is_none());
        let mut features = Features::default();
        features.start.spots.push(openlaser_core::features::Spot { contour: 0, fraction: 0.5 });
        let saved = save(&root, &features);
        assert!(saved.features.start.spots.is_empty());
        assert_eq!(open(&root).unwrap(), saved);
        std::fs::remove_dir_all(root).ok();
    }
}
