//! In-memory run outputs handed to `marg-io` for writing
//! (plan/04-data-outputs-analytics.md §2). The core has no Arrow dependency.

/// One finished (or unfinished) trip.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRecord {
    pub agent_id: u64,
    pub vehicle_type: &'static str,
    pub class: &'static str,
    pub entry: String,
    pub t_arrival: f64,
    pub t_entry: f64,
    /// `None` if still in the network at the end.
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
}

/// Aggregates for one detector, interval and vehicle type.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectorRecord {
    pub detector_id: String,
    pub kind: &'static str,
    pub t_start: f64,
    pub t_end: f64,
    pub in_warmup: bool,
    pub vehicle_type: &'static str,
    pub class: &'static str,
    /// Screenline: crossings. Area: vehicles that entered the area.
    pub count: u64,
    /// Screenline: mean spot speed (m/s).
    pub time_mean_speed: f64,
    /// Screenline: harmonic mean of spot speeds; area: TTD / TTS (m/s).
    pub space_mean_speed: f64,
    /// Area (Edie): TTS / (L · Δt), veh/m.
    pub density: f64,
    /// Screenline: count / Δt; area (Edie): TTD / (L · Δt), veh/s.
    pub flow: f64,
    /// Area: total time spent (veh·s).
    pub tts: f64,
    /// Area: total distance travelled (veh·m).
    pub ttd: f64,
    /// Screenline: mean and SD of lateral crossing position (m).
    pub lateral_mean: f64,
    pub lateral_sd: f64,
    /// Screenline: lateral histogram over `[lateral_y0, lateral_y0 + bin · n)`.
    pub lateral_y0: f64,
    pub lateral_bin: f64,
    pub lateral_hist: Vec<u32>,
}

/// A discrete event.
#[derive(Debug, Clone, PartialEq)]
pub struct EventRecord {
    pub t: f64,
    pub kind: &'static str,
    pub agent_id: u64,
    pub ribbon: String,
    pub s: f64,
    pub y: f64,
    /// Type-specific attributes as JSON.
    pub attrs: String,
}

/// A chunk of trajectory rows, columnar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrajectoryChunk {
    pub t: Vec<f64>,
    pub agent_id: Vec<u64>,
    pub ribbon: Vec<u32>,
    pub s: Vec<f64>,
    pub y: Vec<f64>,
    pub x_world: Vec<f64>,
    pub y_world: Vec<f64>,
    pub heading: Vec<f64>,
    pub v: Vec<f64>,
    pub a: Vec<f64>,
}

impl TrajectoryChunk {
    pub fn len(&self) -> usize {
        self.t.len()
    }
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }
    pub fn clear(&mut self) {
        self.t.clear();
        self.agent_id.clear();
        self.ribbon.clear();
        self.s.clear();
        self.y.clear();
        self.x_world.clear();
        self.y_world.clear();
        self.heading.clear();
        self.v.clear();
        self.a.clear();
    }
}

/// Receives trajectory chunks as the run progresses (e.g. a Parquet writer
/// on a background thread).
pub trait TrajectorySink: Send {
    fn write(&mut self, chunk: TrajectoryChunk);
    fn finish(&mut self) -> Result<(), String>;
}

/// Everything a finished run produced, apart from streamed trajectories.
#[derive(Debug, Clone, Default)]
pub struct RunOutputs {
    pub agents: Vec<AgentRecord>,
    pub detectors: Vec<DetectorRecord>,
    pub events: Vec<EventRecord>,
    /// Headline KPIs as `(name, value)`, in a fixed order.
    pub kpis: Vec<(&'static str, f64)>,
    /// Ribbon ids by index (for decoding `TrajectoryChunk::ribbon`).
    pub ribbons: Vec<String>,
}
