// SPDX-License-Identifier: GPL-3.0-or-later

//! One file holding everything the operator made: parts, jobs, recipes,
//! sketches, sheets, settings and the machine files. It is a plain ZIP
//! (stored, not compressed) so Windows opens it without OpenLaser, and
//! restoring is unpacking it over the data folder while OpenLaser is closed.
//! A copy is also kept once a day under `data/backups`, the newest
//! [`KEEP_DAILY`] of them.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Where the daily copies go, inside the data folder.
pub const DAILY_DIR: &str = "backups";
/// How many daily copies are kept.
pub const KEEP_DAILY: usize = 14;
/// Folders and files of the data folder that are not the operator's work:
/// the browser's cache, locks, the log and the copies themselves.
const SKIPPED: [&str; 5] = ["webview", DAILY_DIR, "server.lock", "desktop.lock", "server.log"];

/// The archive's entries: path inside it, file on disk.
pub fn entries(
    data_dir: &Path,
    machine_files: &[PathBuf],
) -> std::io::Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    walk(data_dir, "data", &mut out)?;
    for file in machine_files {
        if let Some(name) = file.file_name().and_then(|n| n.to_str())
            && file.is_file()
            && !file.starts_with(data_dir)
        {
            out.push((format!("machine/{name}"), file.clone()));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
    let mut items = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    items.sort_by_key(std::fs::DirEntry::file_name);
    for item in items {
        let Some(name) = item.file_name().to_str().map(str::to_owned) else { continue };
        if prefix == "data" && SKIPPED.contains(&name.as_str()) {
            continue;
        }
        let path = item.path();
        let kind = item.file_type()?;
        if kind.is_dir() {
            walk(&path, &format!("{prefix}/{name}"), out)?;
        } else if kind.is_file() {
            out.push((format!("{prefix}/{name}"), path));
        }
    }
    Ok(())
}

/// The archive in memory. Files that vanish while it is written are left out.
pub fn archive(data_dir: &Path, machine_files: &[PathBuf]) -> std::io::Result<Vec<u8>> {
    let mut zip = Zip::default();
    for (name, path) in entries(data_dir, machine_files)? {
        match std::fs::read(&path) {
            Ok(bytes) => zip.add(&name, &bytes, modified(&path)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(zip.finish())
}

/// The archive's file name for a day.
#[must_use]
pub fn file_name(now: std::time::SystemTime) -> String {
    let (y, m, d) = civil(seconds(now) / 86_400);
    format!("OpenLaser-Sicherung-{y:04}-{m:02}-{d:02}.zip")
}

/// Writes today's copy unless there is one, then drops the oldest beyond
/// [`KEEP_DAILY`]. Returns the copy written, if any.
pub fn daily(data_dir: &Path, machine_files: &[PathBuf]) -> std::io::Result<Option<PathBuf>> {
    let dir = data_dir.join(DAILY_DIR);
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(file_name(std::time::SystemTime::now()));
    let written = if target.exists() {
        None
    } else {
        let bytes = archive(data_dir, machine_files)?;
        let partial = target.with_extension("zip.part");
        let mut file = std::fs::File::create(&partial)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&partial, &target)?;
        Some(target)
    };
    let mut copies = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("OpenLaser-Sicherung-")
                    && std::path::Path::new(n)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            })
        })
        .collect::<Vec<_>>();
    copies.sort();
    let excess = copies.len().saturating_sub(KEEP_DAILY);
    for old in &copies[..excess] {
        std::fs::remove_file(old)?;
    }
    Ok(written)
}

fn seconds(t: std::time::SystemTime) -> i64 {
    t.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

fn modified(path: &Path) -> i64 {
    std::fs::metadata(path).and_then(|m| m.modified()).map_or(0, seconds)
}

/// Year, month, day of a day count since 1970 (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A ZIP writer for stored entries: enough for a backup, nothing to depend on.
#[derive(Default)]
struct Zip {
    body: Vec<u8>,
    directory: Vec<u8>,
    count: u16,
}

impl Zip {
    fn add(&mut self, name: &str, bytes: &[u8], modified: i64) {
        let crc = crc32(bytes);
        let (time, date) = dos_time(modified);
        let offset = u32::try_from(self.body.len()).unwrap_or(u32::MAX);
        let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let name_len = u16::try_from(name.len()).unwrap_or(u16::MAX);
        // Bit 11: the name is UTF-8.
        let common = |out: &mut Vec<u8>| {
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0x0800u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&time.to_le_bytes());
            out.extend_from_slice(&date.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
        };
        self.body.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        common(&mut self.body);
        self.body.extend_from_slice(name.as_bytes());
        self.body.extend_from_slice(bytes);

        self.directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        self.directory.extend_from_slice(&20u16.to_le_bytes());
        common(&mut self.directory);
        self.directory.extend_from_slice(&[0; 6]); // comment length, disk, internal attributes
        self.directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        self.directory.extend_from_slice(&offset.to_le_bytes());
        self.directory.extend_from_slice(name.as_bytes());
        self.count = self.count.saturating_add(1);
    }

    fn finish(mut self) -> Vec<u8> {
        let start = u32::try_from(self.body.len()).unwrap_or(u32::MAX);
        let size = u32::try_from(self.directory.len()).unwrap_or(u32::MAX);
        self.body.append(&mut self.directory);
        self.body.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.body.extend_from_slice(&[0; 4]);
        self.body.extend_from_slice(&self.count.to_le_bytes());
        self.body.extend_from_slice(&self.count.to_le_bytes());
        self.body.extend_from_slice(&size.to_le_bytes());
        self.body.extend_from_slice(&start.to_le_bytes());
        self.body.extend_from_slice(&0u16.to_le_bytes());
        self.body
    }
}

fn dos_time(seconds: i64) -> (u16, u16) {
    let (y, m, d) = civil(seconds.div_euclid(86_400));
    let s = seconds.rem_euclid(86_400);
    let time =
        u16::try_from(((s / 3600) << 11) | ((s % 3600 / 60) << 5) | ((s % 60) / 2)).unwrap_or(0);
    let year = u16::try_from((y - 1980).clamp(0, 127)).unwrap_or(0);
    let date = year << 9 | u16::try_from(m).unwrap_or(1) << 5 | u16::try_from(d).unwrap_or(1);
    (time, date)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { crc >> 1 ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_checksums_are_right() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_734), (2026, 10, 8));
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn the_archive_holds_the_work_but_not_the_cache() {
        let dir = std::env::temp_dir().join(format!("openlaser-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data/recipes")).unwrap();
        std::fs::create_dir_all(dir.join("data/webview")).unwrap();
        std::fs::write(dir.join("data/recipes/steel.json"), "{}").unwrap();
        std::fs::write(dir.join("data/webview/cache"), "x").unwrap();
        std::fs::write(dir.join("data/server.lock"), "1").unwrap();
        std::fs::write(dir.join("backup.xml"), "<x/>").unwrap();
        let names = entries(&dir.join("data"), &[dir.join("backup.xml")])
            .unwrap()
            .into_iter()
            .map(|e| e.0)
            .collect::<Vec<_>>();
        assert_eq!(names, ["data/recipes/steel.json", "machine/backup.xml"]);
        let zip = archive(&dir.join("data"), &[dir.join("backup.xml")]).unwrap();
        // The end record points at the directory, whose entries point at the files.
        let end = zip.len() - 22;
        assert_eq!(&zip[end..end + 4], b"PK\x05\x06");
        let at = |i: usize| u32::from_le_bytes(zip[i..i + 4].try_into().unwrap()) as usize;
        let short = |i: usize| usize::from(u16::from_le_bytes(zip[i..i + 2].try_into().unwrap()));
        let (size, mut entry) = (at(end + 12), at(end + 16));
        assert_eq!(entry + size, end);
        for name in ["data/recipes/steel.json", "machine/backup.xml"] {
            assert_eq!(&zip[entry..entry + 4], b"PK\x01\x02");
            assert_eq!(&zip[entry + 46..entry + 46 + short(entry + 28)], name.as_bytes());
            assert_eq!(&zip[at(entry + 42)..at(entry + 42) + 4], b"PK\x03\x04");
            entry += 46 + short(entry + 28);
        }
        assert_eq!(entry, end);

        for _ in 0..2 {
            daily(&dir.join("data"), &[]).unwrap();
        }
        for day in 1..=20 {
            std::fs::write(
                dir.join(format!("data/backups/OpenLaser-Sicherung-2000-01-{day:02}.zip")),
                "",
            )
            .unwrap();
        }
        daily(&dir.join("data"), &[]).unwrap();
        let kept = std::fs::read_dir(dir.join("data/backups")).unwrap().count();
        assert_eq!(kept, KEEP_DAILY);
        assert!(
            dir.join("data/backups").join(file_name(std::time::SystemTime::now())).exists(),
            "today's copy is the newest and stays"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
