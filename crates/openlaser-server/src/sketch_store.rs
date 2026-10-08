// SPDX-License-Identifier: GPL-3.0-or-later

//! Sketches kept on the server, so every screen sees the same ones and they
//! are backed up with the library. The sketch itself is the browser's
//! editing model, kept as it is sent; the server only checks its shape and
//! keeps a small outline for the list.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The most bytes one sketch may take.
const MAX_BYTES: usize = 16 * 1024 * 1024;
/// The most sketches kept, removed ones included.
const MAX_SKETCHES: usize = 5000;

/// One sketch as saved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SketchDocument {
    /// Chosen by the browser that started it.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The editing model: points, entities, constraints.
    pub sketch: Value,
    /// What saving it to Parts does: a new part, added to the job, or replacing one.
    #[serde(default)]
    pub target: String,
    /// The library part it was opened from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
    /// The working job it was opened from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_key: Option<String>,
    /// Polylines of its curves for the list's preview, in millimetres.
    #[serde(default)]
    pub outline: Vec<Vec<[f64; 2]>>,
    /// When it was last saved, in seconds since the epoch.
    #[serde(default)]
    pub updated: u64,
}

/// One sketch in the list, without its editing model.
#[derive(Clone, Debug, Serialize)]
pub struct SketchSummary {
    /// Its id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// What saving it to Parts does.
    pub target: String,
    /// The library part it was opened from, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
    /// Polylines for the preview.
    pub outline: Vec<Vec<[f64; 2]>>,
    /// When it was last saved.
    pub updated: u64,
}

/// The sketches folder and its removed sketches, which can come back.
#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}

fn io(path: &Path, error: impl std::fmt::Display) -> Error {
    Error::Refused(format!("{}: {error}", path.display()))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Store {
    /// The store under the data folder, made when missing.
    pub fn open(data: &Path) -> Result<Self> {
        let dir = data.join("sketches");
        for d in [dir.clone(), dir.join("removed")] {
            std::fs::create_dir_all(&d).map_err(|e| io(&d, e))?;
        }
        Ok(Self { dir })
    }

    fn path(&self, id: &str, removed: bool) -> Result<PathBuf> {
        if !valid_id(id) {
            return Err(Error::Request("a sketch id is letters, digits, - and _".into()));
        }
        let dir = if removed { self.dir.join("removed") } else { self.dir.clone() };
        Ok(dir.join(format!("{id}.json")))
    }

    fn read(path: &Path) -> Option<SketchDocument> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn list_in(&self, removed: bool) -> Vec<SketchSummary> {
        let dir = if removed { self.dir.join("removed") } else { self.dir.clone() };
        let mut out: Vec<SketchSummary> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| Self::read(&e.path()))
            .map(|d| SketchSummary {
                id: d.id,
                name: d.name,
                target: d.target,
                source_part: d.source_part,
                outline: d.outline,
                updated: d.updated,
            })
            .collect();
        out.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.name.cmp(&b.name)));
        out
    }

    /// The sketches, latest first.
    #[must_use]
    pub fn list(&self) -> Vec<SketchSummary> {
        self.list_in(false)
    }

    /// The removed sketches, latest first.
    #[must_use]
    pub fn removed(&self) -> Vec<SketchSummary> {
        self.list_in(true)
    }

    /// One sketch.
    pub fn get(&self, id: &str) -> Result<SketchDocument> {
        let path = self.path(id, false)?;
        Self::read(&path).ok_or_else(|| Error::Refused("that sketch is not there".into()))
    }

    /// Saves a sketch under its id, replacing what was there.
    pub fn save(&self, mut document: SketchDocument) -> Result<SketchDocument> {
        let path = self.path(&document.id, false)?;
        let sketch = &document.sketch;
        let shaped = ["points", "entities", "constraints"]
            .iter()
            .all(|k| sketch.get(k).is_some_and(Value::is_array));
        if !shaped {
            return Err(Error::Request("a sketch has points, entities and constraints".into()));
        }
        if !path.exists() && self.list().len() + self.removed().len() >= MAX_SKETCHES {
            return Err(Error::Refused(format!(
                "at most {MAX_SKETCHES} sketches are kept; remove some first"
            )));
        }
        document.name = document.name.trim().chars().take(200).collect();
        if document.name.is_empty() {
            document.name = "Sketch".into();
        }
        document.updated = now();
        let bytes = serde_json::to_vec(&document).map_err(|e| Error::Request(e.to_string()))?;
        if bytes.len() > MAX_BYTES {
            return Err(Error::Request("the sketch is too large to keep".into()));
        }
        openlaser_library::atomic_write(&path, &bytes).map_err(|e| io(&path, e))?;
        Ok(document)
    }

    /// Moves a sketch to the removed ones, where it can be restored.
    pub fn remove(&self, id: &str) -> Result<()> {
        let from = self.path(id, false)?;
        let to = self.path(id, true)?;
        std::fs::rename(&from, &to).map_err(|e| io(&from, e))
    }

    /// Brings a removed sketch back.
    pub fn restore(&self, id: &str) -> Result<SketchDocument> {
        let from = self.path(id, true)?;
        let to = self.path(id, false)?;
        std::fs::rename(&from, &to).map_err(|e| io(&from, e))?;
        self.get(id)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "test fixtures")]
    use super::*;

    fn doc(id: &str) -> SketchDocument {
        SketchDocument {
            id: id.into(),
            name: " Bracket ".into(),
            sketch: serde_json::json!({ "points": [], "entities": [], "constraints": [], "next": 1 }),
            target: "new".into(),
            source_part: None,
            job_key: None,
            outline: vec![vec![[0., 0.], [10., 0.]]],
            updated: 0,
        }
    }

    #[test]
    fn saves_lists_removes_and_restores() {
        let root = std::env::temp_dir().join(format!("ol-sketches-{}", std::process::id()));
        let store = Store::open(&root).unwrap();
        let saved = store.save(doc("sketch-a")).unwrap();
        assert_eq!(saved.name, "Bracket");
        assert!(saved.updated > 0);
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get("sketch-a").unwrap().outline.len(), 1);
        store.remove("sketch-a").unwrap();
        assert!(store.list().is_empty());
        assert_eq!(store.removed().len(), 1);
        store.restore("sketch-a").unwrap();
        assert_eq!(store.list().len(), 1);
        assert!(
            store.save(SketchDocument { sketch: serde_json::json!({}), ..doc("bad") }).is_err()
        );
        assert!(store.get("../x").is_err());
        drop(std::fs::remove_dir_all(&root));
    }
}
