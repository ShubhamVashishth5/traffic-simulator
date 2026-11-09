//! `marg collect`: validate an experiment's run folders and write
//! `index.parquet`, one row per run-list entry (plan P3.5, P3.6).
//!
//! Each entry gets a status:
//!
//! - `ok`: manifest `ok`, run hash matches the run list, and every Parquet
//!   table in the folder is readable (`kpis.parquet` must exist);
//! - `failed` / `timeout`: as recorded by the runner;
//! - `corrupt`: manifest `ok` but a table is missing or unreadable;
//! - `stale`: a manifest with a different run hash (expanded against other inputs);
//! - `missing`: no manifest yet.
//!
//! It also reports folders the run list does not know about (`orphans`) and
//! leftovers of interrupted runs (`partials`), which `clean_partials` removes.

use crate::batch::{RunEntry, read_run_list};
use crate::manifest::{RunManifest, RunStatus};
use crate::table::{TableError, write_batch};
use arrow::array::{ArrayRef, Float64Array, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::file::reader::{FileReader, SerializedFileReader};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Schema version of `index.parquet`.
pub const INDEX_SCHEMA_VERSION: u32 = 1;

/// One row of `index.parquet`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexRow {
    pub entry: RunEntry,
    pub status: String,
    pub error: Option<String>,
    pub manifest: Option<RunManifest>,
    /// Tables present, sorted (e.g. `agents,detectors,kpis`).
    pub tables: String,
}

/// What `collect` found.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectReport {
    pub total: usize,
    pub ok: usize,
    pub failed: usize,
    pub timeout: usize,
    pub corrupt: usize,
    pub stale: usize,
    pub missing: usize,
    /// Run folders (`run=*`) not in the run list.
    pub orphans: Vec<String>,
    /// Leftover `*.partial-*` folders of interrupted runs.
    pub partials: Vec<String>,
    /// Run-list paths that appear more than once.
    pub duplicate_paths: Vec<String>,
}

impl CollectReport {
    /// Every entry finished `ok` and nothing is duplicated.
    pub fn complete(&self) -> bool {
        self.ok == self.total && self.duplicate_paths.is_empty()
    }
}

/// Errors collecting.
#[derive(Debug, thiserror::Error)]
pub enum CollectError {
    #[error(transparent)]
    Batch(#[from] crate::batch::BatchError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Table(#[from] TableError),
    #[error("arrow: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
}

/// Inspect one run folder.
pub fn inspect(root: &Path, e: &RunEntry) -> IndexRow {
    let dir = root.join(&e.path);
    let tables = list_tables(&dir);
    let tables_s = tables.iter().cloned().collect::<Vec<_>>().join(",");
    let row = |status: &str, error: Option<String>, manifest: Option<RunManifest>| IndexRow {
        entry: e.clone(),
        status: status.to_string(),
        error,
        manifest,
        tables: tables_s.clone(),
    };
    let m = match RunManifest::read(&dir) {
        Ok(m) => m,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return row("missing", None, None);
        }
        Err(err) => return row("corrupt", Some(format!("manifest: {err}")), None),
    };
    if m.run_hash != e.run_hash {
        let msg = format!("manifest hash {} != run list {}", m.run_hash, e.run_hash);
        return row("stale", Some(msg), Some(m));
    }
    match m.status {
        RunStatus::Failed => {
            let err = m.error.clone();
            return row("failed", err, Some(m));
        }
        RunStatus::Timeout => return row("timeout", m.error.clone(), Some(m)),
        RunStatus::Ok => {}
    }
    if !tables.contains("kpis") {
        return row("corrupt", Some("kpis.parquet missing".into()), Some(m));
    }
    for t in &tables {
        let f = dir.join(format!("{t}.parquet"));
        if let Err(err) = check_parquet(&f) {
            return row("corrupt", Some(format!("{t}.parquet: {err}")), Some(m));
        }
    }
    row("ok", None, Some(m))
}

fn list_tables(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|d| {
            let name = d.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".parquet").map(str::to_string)
        })
        .collect()
}

fn check_parquet(path: &Path) -> Result<(), String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let r = SerializedFileReader::new(f).map_err(|e| e.to_string())?;
    let _ = r.metadata().file_metadata().num_rows();
    Ok(())
}

/// Validate every run of the experiment at `root` and write `root/index.parquet`.
pub fn collect(root: &Path, clean_partials: bool) -> Result<CollectReport, CollectError> {
    let entries = read_run_list(&root.join("runs.jsonl"))?;
    let rows: Vec<IndexRow> = entries.iter().map(|e| inspect(root, e)).collect();
    let mut report = CollectReport {
        total: rows.len(),
        ..Default::default()
    };
    for r in &rows {
        match r.status.as_str() {
            "ok" => report.ok += 1,
            "failed" => report.failed += 1,
            "timeout" => report.timeout += 1,
            "corrupt" => report.corrupt += 1,
            "stale" => report.stale += 1,
            _ => report.missing += 1,
        }
    }
    let mut seen = BTreeSet::new();
    for e in &entries {
        if !seen.insert(e.path.as_str()) {
            report.duplicate_paths.push(e.path.clone());
        }
    }
    let known: BTreeSet<PathBuf> = entries.iter().map(|e| root.join(&e.path)).collect();
    let mut found = Vec::new();
    walk_run_dirs(root, &mut found)?;
    for d in found {
        let rel = d
            .strip_prefix(root)
            .unwrap_or(&d)
            .to_string_lossy()
            .into_owned();
        if rel.contains(".partial-") {
            if clean_partials {
                std::fs::remove_dir_all(&d)?;
            }
            report.partials.push(rel);
        } else if !known.contains(&d) {
            report.orphans.push(rel);
        }
    }
    let tmp = root.join("index.parquet.tmp");
    write_batch(&tmp, &index_batch(&rows)?)?;
    std::fs::rename(&tmp, root.join("index.parquet"))?;
    Ok(report)
}

/// Folders named `run=*` (finished or partial) below `dir`.
fn walk_run_dirs(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut items: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    items.sort_by_key(std::fs::DirEntry::file_name);
    for d in items {
        if !d.file_type()?.is_dir() {
            continue;
        }
        let name = d.file_name().to_string_lossy().into_owned();
        if name.starts_with("run=") {
            out.push(d.path());
        } else if name.contains('=') {
            walk_run_dirs(&d.path(), out)?;
        }
    }
    Ok(())
}

/// Build the `index` table.
pub fn index_batch(rows: &[IndexRow]) -> Result<RecordBatch, arrow::error::ArrowError> {
    let u = |f: &dyn Fn(&IndexRow) -> u64| -> ArrayRef {
        Arc::new(UInt64Array::from_iter_values(rows.iter().map(f)))
    };
    let s = |f: &dyn Fn(&IndexRow) -> String| -> ArrayRef {
        Arc::new(StringArray::from_iter_values(rows.iter().map(f)))
    };
    let so = |f: &dyn Fn(&IndexRow) -> Option<String>| -> ArrayRef {
        Arc::new(StringArray::from(rows.iter().map(f).collect::<Vec<_>>()))
    };
    let schema = Schema::new(vec![
        Field::new("run_id", DataType::UInt64, false),
        Field::new("run_hash", DataType::Utf8, false),
        Field::new("experiment", DataType::Utf8, false),
        Field::new("scenario", DataType::Utf8, false),
        Field::new("point", DataType::UInt64, false),
        Field::new("arm", DataType::Utf8, false),
        Field::new("rep", DataType::UInt64, false),
        Field::new("seed", DataType::UInt64, false),
        Field::new("path", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, false),
        Field::new("error", DataType::Utf8, true),
        Field::new("tables", DataType::Utf8, false),
        Field::new("wall_time_s", DataType::Float64, true),
        Field::new("output_hash", DataType::Utf8, true),
        Field::new("marg_version", DataType::Utf8, true),
        Field::new("git_sha", DataType::Utf8, true),
        Field::new("host", DataType::Utf8, true),
        Field::new("agent_steps", DataType::UInt64, true),
    ]);
    let columns: Vec<ArrayRef> = vec![
        u(&|r| r.entry.run_id),
        s(&|r| r.entry.run_hash.clone()),
        s(&|r| r.entry.experiment.clone()),
        s(&|r| r.entry.scenario.clone()),
        u(&|r| r.entry.point),
        s(&|r| r.entry.arm.clone()),
        u(&|r| r.entry.rep),
        u(&|r| r.entry.seed),
        s(&|r| r.entry.path.clone()),
        s(&|r| r.status.clone()),
        so(&|r| r.error.clone()),
        s(&|r| r.tables.clone()),
        Arc::new(Float64Array::from(
            rows.iter()
                .map(|r| r.manifest.as_ref().map(|m| m.wall_time_s))
                .collect::<Vec<_>>(),
        )),
        so(&|r| r.manifest.as_ref().map(|m| m.output_hash.clone())),
        so(&|r| r.manifest.as_ref().map(|m| m.marg_version.clone())),
        so(&|r| r.manifest.as_ref().map(|m| m.git_sha.clone())),
        so(&|r| r.manifest.as_ref().map(|m| m.host.clone())),
        Arc::new(UInt64Array::from(
            rows.iter()
                .map(|r| r.manifest.as_ref().map(|m| m.agent_steps))
                .collect::<Vec<_>>(),
        )),
    ];
    RecordBatch::try_new(Arc::new(schema), columns)
}
