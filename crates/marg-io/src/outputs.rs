//! Parquet tables for run outputs (plan/04-data-outputs-analytics.md §2).
//! Column names and units are documented in `docs/data-dictionary.md`,
//! generated from [`data_dictionary`].

use crate::table::{TableError, write_batch, writer_properties};
use arrow::array::{
    ArrayRef, BooleanArray, Float64Array, ListBuilder, StringArray, UInt32Array, UInt32Builder,
    UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use marg_core::{AgentRecord, DetectorRecord, EventRecord, TrajectoryChunk, TrajectorySink};
use parquet::arrow::ArrowWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;

/// Schema version of the output tables; bump on any column change.
pub const OUTPUT_SCHEMA_VERSION: u32 = 1;

fn f64s<T>(rows: &[T], f: impl Fn(&T) -> f64) -> ArrayRef {
    Arc::new(Float64Array::from_iter_values(rows.iter().map(f)))
}
fn strs<T>(rows: &[T], f: impl Fn(&T) -> &str) -> ArrayRef {
    Arc::new(StringArray::from_iter_values(rows.iter().map(f)))
}

/// `agents.parquet`: one row per agent.
pub fn agents_batch(rows: &[AgentRecord]) -> Result<RecordBatch, arrow::error::ArrowError> {
    let schema = Schema::new(vec![
        Field::new("agent_id", DataType::UInt64, false),
        Field::new("vehicle_type", DataType::Utf8, false),
        Field::new("class", DataType::Utf8, false),
        Field::new("entry", DataType::Utf8, false),
        Field::new("t_arrival", DataType::Float64, false),
        Field::new("t_entry", DataType::Float64, false),
        Field::new("t_exit", DataType::Float64, true),
        Field::new("spawn_delay", DataType::Float64, false),
        Field::new("distance", DataType::Float64, false),
        Field::new("travel_time", DataType::Float64, false),
        Field::new("free_flow_time", DataType::Float64, false),
        Field::new("delay", DataType::Float64, false),
        Field::new("stops", DataType::UInt32, false),
        Field::new("exited", DataType::Boolean, false),
        Field::new("entry_y", DataType::Float64, false),
        Field::new("length", DataType::Float64, false),
        Field::new("width", DataType::Float64, false),
        Field::new("v0", DataType::Float64, false),
        Field::new("time_gap", DataType::Float64, false),
        Field::new("standstill_gap", DataType::Float64, false),
        Field::new("max_accel", DataType::Float64, false),
        Field::new("comfortable_decel", DataType::Float64, false),
    ]);
    let cols: Vec<ArrayRef> = vec![
        Arc::new(UInt64Array::from_iter_values(
            rows.iter().map(|r| r.agent_id),
        )),
        strs(rows, |r| r.vehicle_type),
        strs(rows, |r| r.class),
        strs(rows, |r| &r.entry),
        f64s(rows, |r| r.t_arrival),
        f64s(rows, |r| r.t_entry),
        Arc::new(Float64Array::from(
            rows.iter().map(|r| r.t_exit).collect::<Vec<_>>(),
        )),
        f64s(rows, |r| r.spawn_delay),
        f64s(rows, |r| r.distance),
        f64s(rows, |r| r.travel_time),
        f64s(rows, |r| r.free_flow_time),
        f64s(rows, |r| r.delay),
        Arc::new(UInt32Array::from_iter_values(rows.iter().map(|r| r.stops))),
        Arc::new(BooleanArray::from(
            rows.iter().map(|r| r.exited).collect::<Vec<_>>(),
        )),
        f64s(rows, |r| r.entry_y),
        f64s(rows, |r| r.length),
        f64s(rows, |r| r.width),
        f64s(rows, |r| r.v0),
        f64s(rows, |r| r.time_gap),
        f64s(rows, |r| r.standstill_gap),
        f64s(rows, |r| r.max_accel),
        f64s(rows, |r| r.comfortable_decel),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
}

fn opt(x: f64) -> Option<f64> {
    if x.is_nan() { None } else { Some(x) }
}

/// `detectors.parquet`: one row per detector × interval × vehicle type.
pub fn detectors_batch(rows: &[DetectorRecord]) -> Result<RecordBatch, arrow::error::ArrowError> {
    let nullable = |rows: &[DetectorRecord], f: fn(&DetectorRecord) -> f64| -> ArrayRef {
        Arc::new(Float64Array::from(
            rows.iter().map(|r| opt(f(r))).collect::<Vec<_>>(),
        ))
    };
    let mut hist = ListBuilder::new(UInt32Builder::new());
    for r in rows {
        if r.lateral_hist.is_empty() {
            hist.append_null();
        } else {
            hist.values().append_slice(&r.lateral_hist);
            hist.append(true);
        }
    }
    let item = Field::new("item", DataType::UInt32, true);
    let schema = Schema::new(vec![
        Field::new("detector_id", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("t_start", DataType::Float64, false),
        Field::new("t_end", DataType::Float64, false),
        Field::new("in_warmup", DataType::Boolean, false),
        Field::new("vehicle_type", DataType::Utf8, false),
        Field::new("class", DataType::Utf8, false),
        Field::new("count", DataType::UInt64, false),
        Field::new("time_mean_speed", DataType::Float64, true),
        Field::new("space_mean_speed", DataType::Float64, true),
        Field::new("density", DataType::Float64, true),
        Field::new("flow", DataType::Float64, true),
        Field::new("tts", DataType::Float64, true),
        Field::new("ttd", DataType::Float64, true),
        Field::new("lateral_mean", DataType::Float64, true),
        Field::new("lateral_sd", DataType::Float64, true),
        Field::new("lateral_y0", DataType::Float64, true),
        Field::new("lateral_bin", DataType::Float64, true),
        Field::new("lateral_hist", DataType::List(Arc::new(item)), true),
    ]);
    let cols: Vec<ArrayRef> = vec![
        strs(rows, |r| &r.detector_id),
        strs(rows, |r| r.kind),
        f64s(rows, |r| r.t_start),
        f64s(rows, |r| r.t_end),
        Arc::new(BooleanArray::from(
            rows.iter().map(|r| r.in_warmup).collect::<Vec<_>>(),
        )),
        strs(rows, |r| r.vehicle_type),
        strs(rows, |r| r.class),
        Arc::new(UInt64Array::from_iter_values(rows.iter().map(|r| r.count))),
        nullable(rows, |r| r.time_mean_speed),
        nullable(rows, |r| r.space_mean_speed),
        nullable(rows, |r| r.density),
        nullable(rows, |r| r.flow),
        nullable(rows, |r| r.tts),
        nullable(rows, |r| r.ttd),
        nullable(rows, |r| r.lateral_mean),
        nullable(rows, |r| r.lateral_sd),
        nullable(rows, |r| r.lateral_y0),
        nullable(rows, |r| r.lateral_bin),
        Arc::new(hist.finish()),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
}

/// `events.parquet`: one row per event.
pub fn events_batch(rows: &[EventRecord]) -> Result<RecordBatch, arrow::error::ArrowError> {
    let schema = Schema::new(vec![
        Field::new("t", DataType::Float64, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("agent_id", DataType::UInt64, false),
        Field::new("ribbon", DataType::Utf8, false),
        Field::new("s", DataType::Float64, false),
        Field::new("y", DataType::Float64, false),
        Field::new("attrs", DataType::Utf8, false),
    ]);
    let cols: Vec<ArrayRef> = vec![
        f64s(rows, |r| r.t),
        strs(rows, |r| r.kind),
        Arc::new(UInt64Array::from_iter_values(
            rows.iter().map(|r| r.agent_id),
        )),
        strs(rows, |r| &r.ribbon),
        f64s(rows, |r| r.s),
        f64s(rows, |r| r.y),
        strs(rows, |r| &r.attrs),
    ];
    RecordBatch::try_new(Arc::new(schema), cols)
}

/// `kpis.parquet`: one row, one column per KPI, plus the run hash and seed.
pub fn kpis_batch(
    kpis: &[(&'static str, f64)],
    run_hash: &str,
    seed: u64,
) -> Result<RecordBatch, arrow::error::ArrowError> {
    let mut fields = vec![
        Field::new("run_hash", DataType::Utf8, false),
        Field::new("seed", DataType::UInt64, false),
    ];
    let mut cols: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec![run_hash])),
        Arc::new(UInt64Array::from(vec![seed])),
    ];
    for (name, v) in kpis {
        fields.push(Field::new(*name, DataType::Float64, true));
        cols.push(Arc::new(Float64Array::from(vec![opt(*v)])));
    }
    RecordBatch::try_new(Arc::new(Schema::new(fields)), cols)
}

fn trajectory_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("t", DataType::Float64, false),
        Field::new("agent_id", DataType::UInt64, false),
        Field::new("ribbon", DataType::Utf8, false),
        Field::new("s", DataType::Float64, false),
        Field::new("y", DataType::Float64, false),
        Field::new("x_world", DataType::Float64, false),
        Field::new("y_world", DataType::Float64, false),
        Field::new("heading", DataType::Float64, false),
        Field::new("v", DataType::Float64, false),
        Field::new("a", DataType::Float64, false),
    ]))
}

fn trajectory_batch(
    c: TrajectoryChunk,
    ribbons: &[String],
) -> Result<RecordBatch, arrow::error::ArrowError> {
    let ribbon: ArrayRef = Arc::new(StringArray::from_iter_values(
        c.ribbon.iter().map(|&r| ribbons[r as usize].as_str()),
    ));
    let f = |v: Vec<f64>| -> ArrayRef { Arc::new(Float64Array::from(v)) };
    RecordBatch::try_new(
        trajectory_schema(),
        vec![
            f(c.t),
            Arc::new(UInt64Array::from(c.agent_id)),
            ribbon,
            f(c.s),
            f(c.y),
            f(c.x_world),
            f(c.y_world),
            f(c.heading),
            f(c.v),
            f(c.a),
        ],
    )
}

/// Writes `trajectories.parquet` on a background thread, one row group per chunk.
pub struct ParquetTrajectorySink {
    tx: Option<mpsc::SyncSender<TrajectoryChunk>>,
    handle: Option<JoinHandle<Result<(), String>>>,
}

impl ParquetTrajectorySink {
    /// Start a writer for `path`; `ribbons` maps ribbon indices to ids.
    pub fn new(path: PathBuf, ribbons: Vec<String>) -> Result<Self, TableError> {
        let file = std::fs::File::create(&path)?;
        let mut writer =
            ArrowWriter::try_new(file, trajectory_schema(), Some(writer_properties()))?;
        let (tx, rx) = mpsc::sync_channel::<TrajectoryChunk>(4);
        let handle = std::thread::spawn(move || {
            for chunk in rx {
                let batch = trajectory_batch(chunk, &ribbons).map_err(|e| e.to_string())?;
                writer.write(&batch).map_err(|e| e.to_string())?;
                writer.flush().map_err(|e| e.to_string())?;
            }
            writer.close().map(|_| ()).map_err(|e| e.to_string())
        });
        Ok(Self {
            tx: Some(tx),
            handle: Some(handle),
        })
    }
}

impl TrajectorySink for ParquetTrajectorySink {
    fn write(&mut self, chunk: TrajectoryChunk) {
        if let Some(tx) = &self.tx {
            // A send error means the writer thread failed; `finish` reports it.
            let _ = tx.send(chunk);
        }
    }

    fn finish(&mut self) -> Result<(), String> {
        drop(self.tx.take());
        match self.handle.take() {
            Some(h) => h
                .join()
                .map_err(|_| "trajectory writer panicked".to_string())?,
            None => Ok(()),
        }
    }
}

/// Write every non-streamed output table of a run into `dir`.
pub fn write_outputs(
    dir: &Path,
    out: &marg_core::RunOutputs,
    run_hash: &str,
    seed: u64,
) -> Result<(), TableError> {
    write_batch(&dir.join("agents.parquet"), &agents_batch(&out.agents)?)?;
    write_batch(
        &dir.join("detectors.parquet"),
        &detectors_batch(&out.detectors)?,
    )?;
    write_batch(&dir.join("events.parquet"), &events_batch(&out.events)?)?;
    write_batch(
        &dir.join("kpis.parquet"),
        &kpis_batch(&out.kpis, run_hash, seed)?,
    )?;
    Ok(())
}

/// `(column, type, unit, meaning)`.
type Column = (&'static str, &'static str, &'static str, &'static str);
/// `(file, description, columns)`.
type TableDoc = (&'static str, &'static str, &'static [Column]);

/// The data dictionary in Markdown: every output table, column, type and meaning.
pub fn data_dictionary() -> String {
    let tables: [TableDoc; 6] = [
        (
            "manifest.json",
            "Run identity and provenance (JSON, not Parquet).",
            &[
                (
                    "run_hash",
                    "string",
                    "",
                    "blake3 of resolved spec + seed + version; the run's identity",
                ),
                (
                    "output_hash",
                    "string",
                    "",
                    "blake3 of every step's state; equal ⇔ bitwise-identical run",
                ),
                (
                    "seed, threads, git_sha, git_dirty, marg_version, wall_time_s, …",
                    "",
                    "",
                    "see `marg_io::RunManifest`",
                ),
            ],
        ),
        (
            "params.parquet",
            "Resolved parameter register, one row per parameter.",
            &[
                ("path", "utf8", "", "register path"),
                (
                    "value / value_si",
                    "utf8 (JSON)",
                    "register unit / SI",
                    "scalar or distribution; null = unset",
                ),
                (
                    "unit / si_unit",
                    "utf8",
                    "",
                    "units of `value` and `value_si`",
                ),
                ("mean_si", "f64", "SI", "mean of the value"),
                ("grade", "utf8", "", "A, B, C, A/B, B/C"),
                (
                    "role, stage",
                    "utf8, i32",
                    "",
                    "fix / fit (stage 0–7) / sweep / scenario",
                ),
                ("source, guide", "utf8", "", "citation and guide section"),
                (
                    "set_by, fix_overridden",
                    "utf8, bool",
                    "",
                    "base / scenario / experiment / live",
                ),
            ],
        ),
        (
            "agents.parquet",
            "One row per agent: draws and trip summary.",
            &[
                (
                    "agent_id",
                    "u64",
                    "",
                    "entry << 48 | class << 40 | sequence",
                ),
                ("vehicle_type, class, entry", "utf8", "", ""),
                (
                    "t_arrival",
                    "f64",
                    "s",
                    "time the agent arrived at the entry (virtual queue)",
                ),
                ("t_entry", "f64", "s", "time it entered the network"),
                (
                    "t_exit",
                    "f64?",
                    "s",
                    "null if still in the network at the end",
                ),
                ("spawn_delay", "f64", "s", "t_entry − t_arrival"),
                ("distance", "f64", "m", "distance travelled in the network"),
                ("travel_time", "f64", "s", "exit (or end) − entry"),
                ("free_flow_time", "f64", "s", "distance / v0"),
                ("delay", "f64", "s", "travel_time − free_flow_time"),
                (
                    "stops",
                    "u32",
                    "",
                    "times speed fell below `observers.stop.speed_below`",
                ),
                ("exited", "bool", "", ""),
                (
                    "entry_y",
                    "f64",
                    "m",
                    "lateral position at entry (+ left of travel)",
                ),
                ("length, width", "f64", "m", "drawn dimensions"),
                (
                    "v0",
                    "f64",
                    "m/s",
                    "desired speed (incl. median-band bonus)",
                ),
                ("time_gap, standstill_gap", "f64", "s, m", "IDM T and s0"),
                ("max_accel, comfortable_decel", "f64", "m/s²", "IDM a and b"),
            ],
        ),
        (
            "detectors.parquet",
            "One row per detector × interval × vehicle type with data.",
            &[
                ("detector_id, kind", "utf8", "", "screenline or area"),
                (
                    "t_start, t_end, in_warmup",
                    "f64, f64, bool",
                    "s",
                    "interval",
                ),
                ("vehicle_type, class", "utf8", "", ""),
                (
                    "count",
                    "u64",
                    "veh",
                    "screenline: crossings; area: vehicles entering the area",
                ),
                (
                    "time_mean_speed",
                    "f64?",
                    "m/s",
                    "screenline: mean spot speed",
                ),
                (
                    "space_mean_speed",
                    "f64?",
                    "m/s",
                    "screenline: harmonic mean; area: TTD/TTS (Edie)",
                ),
                ("density", "f64?", "veh/m", "area: TTS/(L·Δt)"),
                (
                    "flow",
                    "f64?",
                    "veh/s",
                    "screenline: count/Δt; area: TTD/(L·Δt)",
                ),
                (
                    "tts, ttd",
                    "f64?",
                    "veh·s, veh·m",
                    "area: total time spent and distance travelled",
                ),
                (
                    "lateral_mean, lateral_sd",
                    "f64?",
                    "m",
                    "screenline: crossing lateral position",
                ),
                (
                    "lateral_y0, lateral_bin, lateral_hist",
                    "f64?, f64?, list<u32>?",
                    "m",
                    "screenline: histogram from y0 in bins",
                ),
            ],
        ),
        (
            "events.parquet",
            "One row per discrete event.",
            &[
                (
                    "t, kind, agent_id",
                    "f64, utf8, u64",
                    "s",
                    "kinds so far: collision_guard",
                ),
                ("ribbon, s, y", "utf8, f64, f64", "m", "where"),
                ("attrs", "utf8 (JSON)", "", "type-specific attributes"),
            ],
        ),
        (
            "kpis.parquet / trajectories.parquet",
            "Headline KPIs (one row) and sampled trajectories.",
            &[
                (
                    "kpis: run_hash, seed, throughput_veh_h, delay_mean_s, delay_p95_s, …",
                    "f64",
                    "",
                    "see `Simulation::kpis`; measured window excludes warm-up",
                ),
                (
                    "trajectories: t, agent_id, ribbon, s, y",
                    "f64, u64, utf8, f64, f64",
                    "s, m",
                    "ribbon frame",
                ),
                (
                    "trajectories: x_world, y_world, heading",
                    "f64",
                    "m, rad",
                    "world frame",
                ),
                (
                    "trajectories: v, a",
                    "f64",
                    "m/s, m/s²",
                    "speed and acceleration",
                ),
            ],
        ),
    ];
    let mut md = format!(
        "# Data dictionary\n\nGenerated by `marg data-dictionary` (output schema version {OUTPUT_SCHEMA_VERSION}). \
         Do not edit by hand.\n\nEvery run folder holds these files (plan/04-data-outputs-analytics.md).\n"
    );
    for (name, desc, cols) in tables {
        md.push_str(&format!("\n## {name}\n\n{desc}\n\n| Column | Type | Unit | Meaning |\n| --- | --- | --- | --- |\n"));
        for (c, t, u, m) in cols {
            md.push_str(&format!("| {c} | {t} | {u} | {m} |\n"));
        }
    }
    md
}
