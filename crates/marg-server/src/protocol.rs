//! The viewer protocol (plan P4.2), version [`PROTOCOL_VERSION`].
//!
//! - **Client → server:** JSON text messages, [`ClientMessage`], tagged by `cmd`.
//! - **Server → client:** JSON text messages, [`ServerMessage`], tagged by
//!   `type`, and binary messages: one agent frame each, as an Arrow IPC
//!   stream (schema + one record batch) with the columns in
//!   [`FRAME_COLUMNS`] (see `crate::frames`).
//!
//! The WebSocket URL carries the client's version (`?protocol=1`); the
//! server refuses other versions, and its first message is [`Hello`] with its
//! own. Agent ids are decimal strings in JSON (they use all 64 bits; a
//! JavaScript number holds 53) and `UInt64` in frames.
//!
//! The TypeScript types (`web/src/protocol.gen.ts`) are generated from these
//! types by `scripts/gen-schema.sh`.

use marg_params::spec::ObjectSpec;
use marg_params::{ParamValue, Scenario};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Bump on any change to the messages or frame columns.
pub const PROTOCOL_VERSION: u32 = 3;

/// Columns of a binary agent frame, in order.
pub const FRAME_COLUMNS: [&str; 8] = [
    "id", "x", "y", "heading", "length", "width", "class", "colour",
];

/// What the per-agent `colour` value of a frame holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColourBy {
    /// Vehicle class index (`VehicleClass::ALL`).
    #[default]
    Class,
    /// Speed (m/s).
    Speed,
    /// Speed over desired speed.
    SpeedRatio,
    /// Acceleration (m/s²).
    Accel,
    /// Lateral position in the ribbon frame (m, + left of travel).
    Lateral,
    /// Desired speed v0 (m/s).
    DesiredSpeed,
    /// Persona (P8).
    Persona,
    /// Aggressiveness κ (P8).
    Kappa,
    /// Violation state (P8).
    Violation,
    /// Zone (P9).
    Zone,
}

/// When to pause automatically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "on", rename_all = "snake_case")]
pub enum BreakpointOn {
    /// The first event of this kind (`events.parquet` `kind`, e.g. `collision_guard`).
    Event { kind: String },
    /// Simulated time reaches `t` (s).
    Time { t: f64 },
}

/// Viewer settings: not commands, not logged, no effect on the run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewSettings {
    /// Visible world rectangle `[x0, y0, x1, y1]`, relative to `Hello.origin`
    /// (agents outside are not sent).
    pub viewport: Option<[f64; 4]>,
    /// Frames per second to send (default 30).
    pub fps: Option<f64>,
    pub colour: Option<ColourBy>,
    /// Ribbon to sample for the time–space diagram ("" = none).
    pub ts_ribbon: Option<String>,
}

/// A message from the viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ClientMessage {
    Play,
    Pause,
    /// Advance `n` steps, then pause.
    Step {
        #[serde(default = "one")]
        n: u64,
    },
    /// Time warp: simulated seconds per wall second (0 = as fast as possible).
    Speed {
        x: f64,
    },
    /// Change a `live: true` register entry, in its register unit. With
    /// `resample`, also re-draw it for agents already in the network.
    SetParam {
        path: String,
        value: ParamValue,
        #[serde(default)]
        resample: bool,
    },
    /// Re-draw a per-agent parameter for every agent in the network.
    Resample {
        path: String,
    },
    /// Revert the latest parameter change not yet undone.
    Undo,
    /// Inspect an agent (`null` to stop).
    SelectAgent {
        id: Option<String>,
    },
    Breakpoint {
        on: BreakpointOn,
    },
    ClearBreakpoints,
    /// Signals arrive in P8; refused until then.
    ToggleSignalState {
        junction: String,
        state: String,
    },
    /// Put a road object on the road from the next step on (ribbon
    /// coordinates, as in a scenario's `network.objects`).
    AddObject {
        object: ObjectSpec,
    },
    /// Take a road object off the road from the next step on.
    RemoveObject {
        id: String,
    },
    /// Write the session folder and verify it by a headless replay.
    Save,
    /// Viewer settings ([`ViewSettings`]): not logged, no effect on the run.
    View {
        #[serde(default)]
        viewport: Option<[f64; 4]>,
        #[serde(default)]
        fps: Option<f64>,
        #[serde(default)]
        colour: Option<ColourBy>,
        #[serde(default)]
        ts_ribbon: Option<String>,
    },
}

fn one() -> u64 {
    1
}

impl ClientMessage {
    /// The command name, as logged in `commands.parquet`.
    pub fn name(&self) -> &'static str {
        match self {
            ClientMessage::Play => "play",
            ClientMessage::Pause => "pause",
            ClientMessage::Step { .. } => "step",
            ClientMessage::Speed { .. } => "speed",
            ClientMessage::SetParam { .. } => "set_param",
            ClientMessage::Resample { .. } => "resample",
            ClientMessage::Undo => "undo",
            ClientMessage::SelectAgent { .. } => "select_agent",
            ClientMessage::Breakpoint { .. } => "breakpoint",
            ClientMessage::ClearBreakpoints => "clear_breakpoints",
            ClientMessage::ToggleSignalState { .. } => "toggle_signal_state",
            ClientMessage::AddObject { .. } => "add_object",
            ClientMessage::RemoveObject { .. } => "remove_object",
            ClientMessage::Save => "save",
            ClientMessage::View { .. } => "view",
        }
    }
}

/// A ribbon, for drawing.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct RibbonView {
    pub id: String,
    pub length: f64,
    /// Reference line, relative to `origin`.
    pub centre: Vec<[f64; 2]>,
    /// Carriageway polygon (right edge forward, left edge back), relative to `origin`.
    pub outline: Vec<[f64; 2]>,
    /// Median-side band of width `vehicles.median_band.width` (polygon).
    pub median_band: Vec<[f64; 2]>,
    pub next: Option<String>,
}

/// A detector, for drawing.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct DetectorView {
    pub id: String,
    /// `screenline` or `area`.
    pub kind: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Line (screenline) or polygon (area), relative to `origin`.
    pub shape: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct TypeView {
    pub key: String,
    pub class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ColourMode {
    pub key: ColourBy,
    pub label: String,
    pub unit: String,
    /// `true` if the value is categorical (an index into `classes`).
    pub categorical: bool,
    pub available: bool,
    pub note: String,
}

/// First message of every connection.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Hello {
    pub protocol: u32,
    pub marg_version: String,
    pub session: String,
    pub seed: u64,
    pub scenario: Scenario,
    /// Run hash of the scenario and seed before any command.
    pub initial_run_hash: String,
    /// World coordinates of the frame origin: every position sent is relative to it.
    pub origin: [f64; 2],
    /// Network bounds `[x0, y0, x1, y1]`, relative to `origin`.
    pub bounds: [f64; 4],
    pub ribbons: Vec<RibbonView>,
    pub detectors: Vec<DetectorView>,
    /// Class keys in index order.
    pub classes: Vec<String>,
    /// Vehicle types in index order.
    pub vehicle_types: Vec<TypeView>,
    pub colour_modes: Vec<ColourMode>,
    /// Interval of the live detector windows in `Kpi.windows` (s).
    pub live_window_s: f64,
    pub dt_s: f64,
    pub duration_s: f64,
    pub warmup_s: f64,
    pub frame_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Status {
    pub playing: bool,
    /// Simulated seconds per wall second (0 = as fast as possible).
    pub speed: f64,
    pub t: f64,
    pub step: u64,
    pub finished: bool,
    pub agents: u64,
    /// Measured simulated seconds per wall second over the last second.
    pub real_time_factor: f64,
    pub breakpoints: Vec<BreakpointOn>,
    /// Run identity including the commands so far.
    pub run_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct KpiValue {
    pub name: String,
    /// `null` = not measured yet.
    pub value: Option<f64>,
}

/// One row of `detectors.parquet` (see the data dictionary); `null` = not measured.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct DetectorRow {
    pub detector_id: String,
    pub kind: String,
    pub t_start: f64,
    pub t_end: f64,
    pub in_warmup: bool,
    pub vehicle_type: String,
    pub class: String,
    pub count: u64,
    pub time_mean_speed: Option<f64>,
    pub space_mean_speed: Option<f64>,
    pub density: Option<f64>,
    pub flow: Option<f64>,
    pub tts: Option<f64>,
    pub ttd: Option<f64>,
    pub lateral_mean: Option<f64>,
    pub lateral_sd: Option<f64>,
    pub lateral_y0: Option<f64>,
    pub lateral_bin: Option<f64>,
    pub lateral_hist: Option<Vec<u32>>,
}

/// Speeds of the agents in the network now, by class.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ClassHist {
    pub class: String,
    /// Bin width (m/s); bin `k` is `[k·w, (k+1)·w)`.
    pub bin_width: f64,
    pub counts: Vec<u32>,
}

/// Time–space samples of one ribbon (positions rounded to 1 cm, speeds to 0.01 m/s).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct TsSamples {
    pub ribbon: String,
    pub t: Vec<f64>,
    pub s: Vec<f64>,
    pub v: Vec<f64>,
}

/// Sent once per wall second (and after steps while paused).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Kpi {
    pub t: f64,
    pub step: u64,
    /// Headline KPIs if the run ended now (the computation of `kpis.parquet`).
    pub kpis: Vec<KpiValue>,
    /// Live detector windows that closed since the last message.
    pub windows: Vec<DetectorRow>,
    pub speed_hist: Vec<ClassHist>,
    pub ts: Option<TsSamples>,
    pub agents: u64,
    /// Vehicles waiting at entries.
    pub waiting: u64,
}

/// One row of `agents.parquet`, as it would be if the run ended now.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct AgentRow {
    pub agent_id: String,
    pub vehicle_type: String,
    pub class: String,
    pub entry: String,
    pub t_arrival: f64,
    pub t_entry: f64,
    pub t_exit: Option<f64>,
    pub spawn_delay: f64,
    pub distance: f64,
    pub travel_time: f64,
    pub free_flow_time: f64,
    pub delay: f64,
    pub stops: u32,
    pub exited: bool,
    pub entry_y: f64,
    pub length: f64,
    pub width: f64,
    pub v0: f64,
    pub time_gap: f64,
    pub standstill_gap: f64,
    pub max_accel: f64,
    pub comfortable_decel: f64,
    /// Persona key (null: no persona shares set) and traits at entry (G§3).
    pub persona: Option<String>,
    pub kappa: f64,
    /// κ now (moves within a trip with the OU process or the jump on delay).
    pub kappa_now: f64,
    pub time_pressure: f64,
    pub risk_tolerance: f64,
    pub conformity: f64,
    pub norm_sensitivity: f64,
    pub skill: f64,
    pub distraction: f64,
    pub phone_user: bool,
    pub distraction_episodes: u32,
    pub violations: u32,
}

/// A named number (a feature, a coefficient or a term of a decision).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Named {
    pub name: String,
    pub value: f64,
}

/// An open violation episode of the inspected agent, as its last reading
/// left it (ADR-0018): each term is its coefficient times its feature, and
/// they add up to `eta`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct DecisionView {
    /// Violation type (`forced_entry`, …).
    pub kind: String,
    pub episode: u32,
    /// It has acted (a violation under way).
    pub acted: bool,
    pub t_start: f64,
    /// When the features were last read.
    pub t_read: f64,
    pub features: Vec<Named>,
    pub coefficients: Vec<Named>,
    pub terms: Vec<Named>,
    pub eta: f64,
    /// Continuous types: hazard λ = λ_max σ(η) (1/s), the hazard integrated
    /// so far Λ, and the threshold E it acts at. Null for one-shot types.
    pub hazard: Option<f64>,
    pub integrated: Option<f64>,
    pub threshold: Option<f64>,
    /// 1 − exp(−Λ) (continuous) or σ(η) (one-shot).
    pub probability: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct AgentState {
    pub ribbon: String,
    pub s: f64,
    pub y: f64,
    pub v: f64,
    pub acc: f64,
    /// World position relative to `origin`, and heading (rad).
    pub x_world: f64,
    pub y_world: f64,
    pub heading: f64,
}

/// One agent the inspected agent's leader scan considers (plan/03 §2).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Neighbour {
    pub id: String,
    pub vehicle_type: String,
    /// `leader` or `side` (level with the agent).
    pub role: String,
    pub ds: f64,
    pub gap_s: f64,
    pub lateral_gap: f64,
    pub clearance: f64,
    /// Leader weight ω (null for side neighbours).
    pub weight: Option<f64>,
    /// IDM acceleration toward this leader alone (null for side neighbours).
    pub leader_accel: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Inspector {
    pub id: String,
    /// `false` if the agent is not in the network (not entered yet, or exited).
    pub found: bool,
    pub t: f64,
    pub record: Option<AgentRow>,
    pub state: Option<AgentState>,
    pub neighbours: Vec<Neighbour>,
    /// Free acceleration `a[1 − (v/v0)^δ]` (m/s²).
    pub free_accel: Option<f64>,
    /// Open violation episodes with their logit breakdown (P8.3).
    pub decisions: Vec<DecisionView>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ChangeView {
    /// Position in the command log.
    pub seq: u64,
    pub step: u64,
    pub t: f64,
    pub path: String,
    pub before: Option<ParamValue>,
    pub after: ParamValue,
    pub undone: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct EventRow {
    pub t: f64,
    pub kind: String,
    pub agent_id: String,
    pub ribbon: String,
    pub s: f64,
    pub y: f64,
    pub attrs: String,
}

/// A message from the server: `{"type": …, "data": …}`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello(Box<Hello>),
    Status(Status),
    /// Outcome of a command; `seq` is its position in the command log.
    Ack {
        seq: Option<u64>,
        cmd: String,
        ok: bool,
        message: String,
    },
    Kpi(Kpi),
    Inspector(Box<Inspector>),
    History {
        changes: Vec<ChangeView>,
    },
    /// The session paused at a breakpoint.
    Breakpoint {
        reason: String,
        t: f64,
        step: u64,
        event: Option<EventRow>,
    },
    /// The session folder was written and replayed headless.
    Saved {
        path: String,
        run_hash: String,
        output_hash: String,
        replay_output_hash: String,
        replay_matches: bool,
        steps: u64,
    },
    Error {
        message: String,
    },
    /// Every road object, sent on connecting and whenever one is added or
    /// removed.
    Objects {
        objects: Vec<ObjectView>,
    },
}

/// A road object and when it is on the road.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct ObjectView {
    pub id: String,
    pub ribbon: String,
    pub s: f64,
    pub y: f64,
    pub length: f64,
    pub width: f64,
    /// Corners of the footprint, relative to `Hello.origin`.
    pub shape: Vec<[f64; 2]>,
    /// Periods `[from, to)` on the road; `null` = no bound on that side.
    pub present: Vec<[Option<f64>; 2]>,
}

/// Both directions, for the JSON Schema that generates the TypeScript types.
#[derive(Debug, Clone, JsonSchema)]
pub struct ViewerProtocol {
    pub client: ClientMessage,
    pub server: ServerMessage,
}
