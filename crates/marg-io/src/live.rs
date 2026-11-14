//! Live sessions (plan P4.1): a run driven step by step by timed commands.
//!
//! A live session is saved as **scenario + seed + timed command log**
//! (`commands.parquet`), plus a snapshot of the register, so anything seen in
//! the viewer can be re-run headless and bitwise identical ([`replay`]).
//!
//! Every command is stamped with the number of steps completed when it took
//! effect (`step`) and the simulated time (`t`): it acts between step `step`
//! and step `step + 1`. Only these commands change the simulation, and only
//! they are re-applied on replay:
//!
//! - `set_param(path, value)`: the `live` layer of the resolver (register
//!   entries with `live: true` only). Population-level values act on every
//!   agent from the next step; per-agent draws apply to agents that have not
//!   entered yet.
//! - `resample(path)`: re-draw that per-agent parameter for every agent in
//!   the network, from each agent's own random numbers.
//! - `add_object(object)` and `remove_object(id)`: a road object, in ribbon
//!   coordinates, is on the road (or gone) from the next step on.
//!
//! Everything else (play, pause, speed, select, breakpoints, …) is logged for
//! the record but does not change the run.

use crate::manifest::{RNG_SCHEME, RUN_FORMAT_VERSION, RunManifest, RunStatus};
use crate::outputs::{ParquetTrajectorySink, write_outputs};
use crate::params_table::params_batch;
use crate::run::{Inputs, RunError};
use crate::table::{TableError, write_batch};
use arrow::array::{Array, ArrayRef, BooleanArray, Float64Array, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use marg_core::Simulation;
use marg_params::spec::ObjectSpec;
use marg_params::{ParamValue, ResolvedSpec, Scenario};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Commands that change the simulation (re-applied by [`replay`]).
pub const STATE_COMMANDS: [&str; 4] = ["set_param", "resample", "add_object", "remove_object"];

/// One logged command (a row of `commands.parquet`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandRecord {
    /// Position in the log (0-based).
    pub seq: u64,
    /// Steps completed when the command took effect: it acts before step `step + 1`.
    pub step: u64,
    /// Simulated time at that point (s).
    pub t: f64,
    /// Wall-clock seconds since the session started (informational only).
    pub wall_s: f64,
    pub cmd: String,
    /// Arguments as JSON.
    pub args: String,
    pub accepted: bool,
    /// What happened, or why the command was refused.
    pub message: String,
}

impl CommandRecord {
    /// Whether replay must re-apply this command.
    pub fn changes_state(&self) -> bool {
        self.accepted && STATE_COMMANDS.contains(&self.cmd.as_str())
    }
}

/// One accepted parameter change (the viewer's change history, for undo).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParamChange {
    pub seq: u64,
    pub step: u64,
    pub t: f64,
    pub path: String,
    /// Value before the change, in the register unit (`None` = unset).
    pub before: Option<ParamValue>,
    pub after: ParamValue,
}

/// Live-session facts recorded in the manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveMeta {
    pub session: String,
    /// Run hash of the scenario and seed alone (no commands).
    pub initial_run_hash: String,
    pub commands: u64,
    pub state_commands: u64,
    /// Set by [`replay`]: whether the headless replay reproduced `output_hash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_verified: Option<bool>,
}

/// The run identity of a live session: the plain run hash when no command
/// changed the simulation (so an untouched live session is the same run as
/// `marg run`), else blake3 of that hash and every state-changing command.
pub fn live_run_hash(initial: &str, log: &[CommandRecord]) -> String {
    let state: Vec<_> = log.iter().filter(|c| c.changes_state()).collect();
    if state.is_empty() {
        return initial.to_string();
    }
    let mut h = blake3::Hasher::new();
    h.update(initial.as_bytes());
    for c in state {
        h.update(&c.step.to_le_bytes());
        h.update(c.cmd.as_bytes());
        h.update(&[0]);
        h.update(c.args.as_bytes());
        h.update(&[0]);
    }
    h.finalize().to_hex()[..16].to_string()
}

/// Where a live session's inputs come from (snapshotted by [`LiveRun::save`]).
#[derive(Debug, Clone)]
pub struct LiveSource {
    pub params_dir: PathBuf,
    pub calibration_dir: Option<PathBuf>,
}

/// A run driven by commands between steps.
pub struct LiveRun {
    source: LiveSource,
    inputs: Inputs,
    scenario: Scenario,
    initial: ResolvedSpec,
    spec: ResolvedSpec,
    seed: u64,
    sim: Simulation,
    log: Vec<CommandRecord>,
    history: Vec<ParamChange>,
}

fn args_path(args: &serde_json::Value) -> Result<String, String> {
    args.get("path")
        .and_then(|p| p.as_str())
        .map(str::to_string)
        .ok_or_else(|| "missing `path`".to_string())
}

impl LiveRun {
    /// Resolve `scenario` and build its simulation. `threads` as for `marg run`.
    pub fn new(
        source: LiveSource,
        scenario: Scenario,
        seed: u64,
        threads: usize,
    ) -> Result<Self, RunError> {
        let inputs = Inputs::load(Some(&source.params_dir), source.calibration_dir.as_deref())?;
        let initial = inputs.resolve(&scenario, &Default::default(), false)?;
        let sim = Simulation::new(&initial, seed, threads)?;
        Ok(Self {
            source,
            spec: initial.clone(),
            initial,
            inputs,
            scenario,
            seed,
            sim,
            log: Vec::new(),
            history: Vec::new(),
        })
    }

    pub fn sim(&self) -> &Simulation {
        &self.sim
    }

    /// Mutable access for viewer-only settings (live detectors, trajectory
    /// sink). Do not step through it: use [`Self::step`].
    pub fn sim_mut(&mut self) -> &mut Simulation {
        &mut self.sim
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The scenario as given (with its own overrides).
    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// The spec in use now (initial spec plus the `live` layer).
    pub fn spec(&self) -> &ResolvedSpec {
        &self.spec
    }

    pub fn register(&self) -> &marg_params::Register {
        &self.inputs.register
    }

    pub fn source(&self) -> &LiveSource {
        &self.source
    }

    pub fn commands(&self) -> &[CommandRecord] {
        &self.log
    }

    /// Accepted parameter changes, oldest first.
    pub fn history(&self) -> &[ParamChange] {
        &self.history
    }

    /// Run hash of the scenario and seed before any command.
    pub fn initial_run_hash(&self) -> String {
        self.initial.run_hash(self.seed, marg_core::VERSION)
    }

    /// Run identity including the commands so far ([`live_run_hash`]).
    pub fn run_hash(&self) -> String {
        live_run_hash(&self.initial_run_hash(), &self.log)
    }

    /// Advance one step.
    pub fn step(&mut self) {
        self.sim.step();
    }

    fn push(
        &mut self,
        cmd: &str,
        args: &serde_json::Value,
        result: &Result<String, String>,
        wall_s: f64,
    ) -> u64 {
        let seq = self.log.len() as u64;
        self.log.push(CommandRecord {
            seq,
            step: self.sim.step_index(),
            t: self.sim.time(),
            wall_s,
            cmd: cmd.to_string(),
            args: args.to_string(),
            accepted: result.is_ok(),
            message: match result {
                Ok(m) | Err(m) => m.clone(),
            },
        });
        seq
    }

    /// Log a command that does not change the simulation (play, pause, …).
    pub fn log(
        &mut self,
        cmd: &str,
        args: &serde_json::Value,
        result: &Result<String, String>,
        wall_s: f64,
    ) -> u64 {
        self.push(cmd, args, result, wall_s)
    }

    /// Apply a state-changing command ([`STATE_COMMANDS`]) with JSON
    /// arguments, and log it. Returns the log position and the outcome.
    pub fn command(
        &mut self,
        cmd: &str,
        args: &serde_json::Value,
        wall_s: f64,
    ) -> (u64, Result<String, String>) {
        let (result, change) = match cmd {
            "set_param" => match self.apply_set(args) {
                Ok((message, change)) => (Ok(message), Some(change)),
                Err(e) => (Err(e), None),
            },
            "resample" => (self.apply_resample(args), None),
            "add_object" => (self.apply_add_object(args), None),
            "remove_object" => (self.apply_remove_object(args), None),
            other => (Err(format!("`{other}` is not a state command")), None),
        };
        let seq = self.push(cmd, args, &result, wall_s);
        if let Some(mut change) = change {
            change.seq = seq;
            self.history.push(change);
        }
        (seq, result)
    }

    fn apply_set(&mut self, args: &serde_json::Value) -> Result<(String, ParamChange), String> {
        let path = args_path(args)?;
        let value: ParamValue = serde_json::from_value(
            args.get("value")
                .cloned()
                .ok_or_else(|| "missing `value`".to_string())?,
        )
        .map_err(|e| format!("bad value: {e}"))?;
        let before = self.spec.get(&path).and_then(|p| p.value.clone());
        let mut spec = self.spec.clone();
        spec.set_live(&self.inputs.register, &path, &value)
            .map_err(|e| e.to_string())?;
        self.sim.update_params(&spec).map_err(|e| e.to_string())?;
        self.spec = spec;
        let read = self.spec.accessed().contains(&path);
        let message = if read {
            format!("{path} = {}", value.describe())
        } else {
            format!(
                "{path} = {} (not read by the simulator yet: no effect)",
                value.describe()
            )
        };
        let change = ParamChange {
            seq: 0, // the log position, set by `command`
            step: self.sim.step_index(),
            t: self.sim.time(),
            path,
            before,
            after: value,
        };
        Ok((message, change))
    }

    fn apply_resample(&mut self, args: &serde_json::Value) -> Result<String, String> {
        let path = args_path(args)?;
        let entry = self
            .inputs
            .register
            .get(&path)
            .ok_or_else(|| format!("unknown parameter `{path}`"))?;
        if !entry.live {
            return Err(format!("parameter `{path}` is not live"));
        }
        let n = self.sim.resample(&path).map_err(|e| e.to_string())?;
        Ok(format!("resampled {path} for {n} agents"))
    }

    /// `add_object {object}`: a road object (`ObjectSpec`, in ribbon
    /// coordinates) on the road from the next step on.
    fn apply_add_object(&mut self, args: &serde_json::Value) -> Result<String, String> {
        let object: ObjectSpec = serde_json::from_value(
            args.get("object")
                .cloned()
                .ok_or_else(|| "missing `object`".to_string())?,
        )
        .map_err(|e| format!("bad object: {e}"))?;
        self.sim.add_object(&object).map_err(|e| e.to_string())?;
        Ok(format!(
            "object `{}` on `{}` at s = {:.1} m, y = {:.2} m ({:.1} × {:.1} m)",
            object.id, object.ribbon, object.s, object.y, object.length, object.width
        ))
    }

    /// `remove_object {id}`: the object is gone from the next step on.
    fn apply_remove_object(&mut self, args: &serde_json::Value) -> Result<String, String> {
        let id = args
            .get("id")
            .and_then(|p| p.as_str())
            .ok_or_else(|| "missing `id`".to_string())?;
        self.sim.remove_object(id).map_err(|e| e.to_string())?;
        Ok(format!("object `{id}` removed"))
    }

    /// Write the session folder: `scenario.yaml`, `params/` (register
    /// snapshot), `calibration/` (if used), `spec.resolved.yaml` and
    /// `params.parquet` (the spec before any command), `commands.parquet`
    /// and `manifest.json`. The outputs themselves are produced by [`replay`].
    pub fn save(&self, dir: &Path, session: &str) -> Result<RunManifest, RunError> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(
            dir.join("scenario.yaml"),
            serde_norway::to_string(&self.scenario)?,
        )?;
        copy_yaml(&self.source.params_dir, &dir.join("params"), |_| true)?;
        if let Some(c) = &self.source.calibration_dir {
            copy_yaml(c, &dir.join("calibration"), |n| n.starts_with("stage_"))?;
        }
        std::fs::write(dir.join("spec.resolved.yaml"), self.initial.to_yaml()?)?;
        write_batch(&dir.join("params.parquet"), &params_batch(&self.initial)?)?;
        write_batch(&dir.join("commands.parquet"), &commands_batch(&self.log)?)?;
        let summary = self.sim.summary();
        let (marg_version, git_sha, git_dirty, build_profile, host) = RunManifest::build_info();
        let manifest = RunManifest {
            run_format_version: RUN_FORMAT_VERSION,
            run_hash: self.run_hash(),
            scenario: self.scenario.name.clone(),
            seed: self.seed,
            rng_scheme: RNG_SCHEME.into(),
            marg_version,
            git_sha,
            git_dirty,
            build_profile,
            threads: 1,
            host,
            wall_time_s: self.log.last().map_or(0.0, |c| c.wall_s),
            status: RunStatus::Ok,
            output_hash: summary.output_hash,
            steps: summary.steps,
            agent_steps: summary.agent_steps,
            experiment: None,
            calibration_stages: self.inputs.calibration.stages.clone(),
            error: None,
            live: Some(LiveMeta {
                session: session.to_string(),
                initial_run_hash: self.initial_run_hash(),
                commands: self.log.len() as u64,
                state_commands: self.log.iter().filter(|c| c.changes_state()).count() as u64,
                replay_verified: None,
            }),
        };
        manifest.write(dir)?;
        Ok(manifest)
    }
}

fn copy_yaml(from: &Path, to: &Path, keep: impl Fn(&str) -> bool) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    let mut files: Vec<_> = std::fs::read_dir(from)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml" || x == "yml"))
        .collect();
    files.sort();
    for f in files {
        let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if keep(name) {
            std::fs::copy(&f, to.join(name))?;
        }
    }
    Ok(())
}

/// `commands.parquet`: one row per command.
pub fn commands_batch(rows: &[CommandRecord]) -> Result<RecordBatch, arrow::error::ArrowError> {
    let schema = Schema::new(vec![
        Field::new("seq", DataType::UInt64, false),
        Field::new("step", DataType::UInt64, false),
        Field::new("t", DataType::Float64, false),
        Field::new("wall_s", DataType::Float64, false),
        Field::new("cmd", DataType::Utf8, false),
        Field::new("args", DataType::Utf8, false),
        Field::new("accepted", DataType::Boolean, false),
        Field::new("message", DataType::Utf8, false),
    ]);
    let u = |f: fn(&CommandRecord) -> u64| -> ArrayRef {
        Arc::new(UInt64Array::from_iter_values(rows.iter().map(f)))
    };
    let x = |f: fn(&CommandRecord) -> f64| -> ArrayRef {
        Arc::new(Float64Array::from_iter_values(rows.iter().map(f)))
    };
    let s = |f: fn(&CommandRecord) -> &str| -> ArrayRef {
        Arc::new(StringArray::from_iter_values(rows.iter().map(f)))
    };
    RecordBatch::try_new(
        Arc::new(schema),
        vec![
            u(|r| r.seq),
            u(|r| r.step),
            x(|r| r.t),
            x(|r| r.wall_s),
            s(|r| &r.cmd),
            s(|r| &r.args),
            Arc::new(BooleanArray::from(
                rows.iter().map(|r| r.accepted).collect::<Vec<_>>(),
            )),
            s(|r| &r.message),
        ],
    )
}

/// Read `commands.parquet`.
pub fn read_commands(path: &Path) -> Result<Vec<CommandRecord>, TableError> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let file = std::fs::File::open(path)?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?.build()?;
    let mut out = Vec::new();
    let bad = |c: &str| arrow::error::ArrowError::SchemaError(format!("column `{c}`"));
    for batch in reader {
        let b = batch?;
        macro_rules! col {
            ($name:literal, $ty:ty) => {
                b.column_by_name($name)
                    .and_then(|c| c.as_any().downcast_ref::<$ty>())
                    .ok_or_else(|| bad($name))?
            };
        }
        let seq = col!("seq", UInt64Array);
        let step = col!("step", UInt64Array);
        let t = col!("t", Float64Array);
        let wall = col!("wall_s", Float64Array);
        let cmd = col!("cmd", StringArray);
        let args = col!("args", StringArray);
        let acc = col!("accepted", BooleanArray);
        let msg = col!("message", StringArray);
        for i in 0..b.num_rows() {
            out.push(CommandRecord {
                seq: seq.value(i),
                step: step.value(i),
                t: t.value(i),
                wall_s: wall.value(i),
                cmd: cmd.value(i).to_string(),
                args: args.value(i).to_string(),
                accepted: !acc.is_null(i) && acc.value(i),
                message: msg.value(i).to_string(),
            });
        }
    }
    Ok(out)
}

/// What [`replay`] found.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReplayReport {
    pub steps: u64,
    pub commands_applied: usize,
    /// Output hash of the live session (from its manifest).
    pub expected_output_hash: String,
    /// Output hash of the headless replay.
    pub output_hash: String,
    pub matches: bool,
    pub run_hash: String,
}

/// Replay a saved live session headless: the same scenario, seed and
/// state-changing commands at the same steps, for as many steps as the
/// session ran. With `write_outputs`, the run's tables (agents, detectors,
/// events, KPIs, and trajectories if the scenario asks for them) are written
/// into the session folder and the manifest records the verification.
pub fn replay(
    dir: &Path,
    threads: usize,
    write_outputs_too: bool,
) -> Result<ReplayReport, RunError> {
    let mut manifest = RunManifest::read(dir)?;
    let live = manifest
        .live
        .clone()
        .ok_or_else(|| RunError::Live(format!("{} is not a live session", dir.display())))?;
    let scenario = Scenario::from_yaml(&std::fs::read_to_string(dir.join("scenario.yaml"))?)?;
    let calibration = dir.join("calibration");
    let source = LiveSource {
        params_dir: dir.join("params"),
        calibration_dir: calibration.is_dir().then_some(calibration),
    };
    let log = read_commands(&dir.join("commands.parquet"))?;
    let mut run = LiveRun::new(source, scenario, manifest.seed, threads)?;
    if run.initial_run_hash() != live.initial_run_hash {
        return Err(RunError::Live(format!(
            "initial run hash {} differs from the session's {}: the register, calibration or \
             simulator version changed",
            run.initial_run_hash(),
            live.initial_run_hash
        )));
    }
    if write_outputs_too && run.scenario().outputs.trajectories.is_some() {
        // The built network's ribbons: a junction adds its own after the
        // scenario's.
        let ribbons = run
            .sim()
            .network()
            .ribbons
            .iter()
            .map(|r| r.id.clone())
            .collect();
        run.sim_mut()
            .set_trajectory_sink(Box::new(ParquetTrajectorySink::new(
                dir.join("trajectories.parquet"),
                ribbons,
            )?));
    }
    let state: Vec<&CommandRecord> = log.iter().filter(|c| c.changes_state()).collect();
    let mut next = 0;
    let mut apply_due = |run: &mut LiveRun| -> Result<usize, RunError> {
        let mut n = 0;
        while next < state.len() && state[next].step == run.sim().step_index() {
            let c = state[next];
            let args: serde_json::Value = serde_json::from_str(&c.args)
                .map_err(|e| RunError::Live(format!("command {}: {e}", c.seq)))?;
            if let (_, Err(e)) = run.command(&c.cmd, &args, c.wall_s) {
                return Err(RunError::Live(format!(
                    "command {} ({}) was accepted live but is refused now: {e}",
                    c.seq, c.cmd
                )));
            }
            next += 1;
            n += 1;
        }
        Ok(n)
    };
    let mut applied = 0;
    while run.sim().step_index() < manifest.steps {
        applied += apply_due(&mut run)?;
        run.step();
    }
    applied += apply_due(&mut run)?;
    let summary = run.sim().summary();
    let report = ReplayReport {
        steps: summary.steps,
        commands_applied: applied,
        expected_output_hash: manifest.output_hash.clone(),
        matches: summary.output_hash == manifest.output_hash && applied == state.len(),
        output_hash: summary.output_hash,
        run_hash: run.run_hash(),
    };
    if write_outputs_too {
        let LiveRun { sim, .. } = run;
        let (_, outputs) = sim.finish().map_err(RunError::Trajectories)?;
        write_outputs(dir, &outputs, &manifest.run_hash, manifest.seed)?;
        if let Some(l) = &mut manifest.live {
            l.replay_verified = Some(report.matches);
        }
        if !report.matches {
            manifest.status = RunStatus::Failed;
            manifest.error = Some(format!(
                "replay gave output hash {}, the live session {}",
                report.output_hash, report.expected_output_hash
            ));
        }
        manifest.write(dir)?;
    }
    Ok(report)
}
