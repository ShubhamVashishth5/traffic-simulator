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
    /// Aggressiveness κ at entry (the mean its within-trip κ reverts to).
    pub kappa: f64,
    /// κ at exit (or at the end): differs from `kappa` only with the OU
    /// process or the jump on delay.
    pub kappa_end: f64,
    /// Persona key; `None` when personas are off.
    pub persona: Option<&'static str>,
    /// Trait vector at entry `(κ, π, ρ, c, η, skill, δ)` (G§3.1).
    pub traits: [f64; crate::personas::N_TRAITS],
    pub phone_user: bool,
    pub distraction_episodes: u32,
    /// Time spent distracted (s).
    pub distracted_time: f64,
    /// Violations committed (every type; `violation` events give each).
    pub violations: u32,
    /// Sum of |lateral displacement| over the trip (m).
    pub lateral_distance: f64,
    /// Number of times the agent took a new lateral target.
    pub lateral_moves: u32,
}

/// One conflict episode: consecutive steps in which the time to contact of
/// two vehicles, at constant velocity, was under the threshold (ADR-0010 §6).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConflictRecord {
    pub t_start: f64,
    pub t_end: f64,
    /// The vehicle behind (earlier in the ribbon order) and the other one.
    pub agent_id: u64,
    pub other_id: u64,
    pub class: &'static str,
    pub other_class: &'static str,
    /// `rear_end`, `sideswipe` or `head_on`.
    pub kind: &'static str,
    /// Minimum time to contact in the episode (s).
    pub min_ttc: f64,
    /// Maximum deceleration rate to avoid the crash in the episode (m/s²).
    pub max_drac: f64,
    /// Where `agent_id` was at the minimum time to contact.
    pub ribbon: String,
    pub s: f64,
    pub y: f64,
}

/// One bin of an effective-width observer over the measured period
/// (G§6.2: `W_eff = W − Σ w_i − c_shy`). NaN where no vehicle passed.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveWidthRecord {
    pub observer_id: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Mean carriageway width over the bin (m).
    pub road_width: f64,
    /// Width road objects took, averaged over the bin and the period:
    /// footprints plus strips too narrow for any vehicle.
    pub blocked_width: f64,
    pub free_width: f64,
    /// The desired and the minimum lateral clearance to the nearest object,
    /// from the clearance model, averaged over the time vehicles spent
    /// beside one.
    pub shy_desired: f64,
    pub shy_minimum: f64,
    /// `free_width − shy_desired`.
    pub effective_width_model: f64,
    /// The gap to the nearest object that vehicles beside one were within
    /// for 5% of their time, and the smallest gap seen.
    pub shy_observed: f64,
    pub shy_observed_min: f64,
    /// `free_width − shy_observed`.
    pub effective_width_observed: f64,
    /// Lateral band that vehicle bodies covered.
    pub used_lo: f64,
    pub used_hi: f64,
    /// Time vehicles spent beside an object, and in the bin (veh·s).
    pub time_beside: f64,
    pub time_spent: f64,
    /// Space-mean speed in the bin (m/s).
    pub mean_speed: f64,
}

/// One arm of a signalised junction over one cycle: from the start of its
/// green to the start of its next green (plan/04 §2, P7.3).
#[derive(Debug, Clone, PartialEq)]
pub struct SignalCycleRecord {
    pub junction: String,
    pub arm: String,
    /// The arriving ribbon; its end is the stop line.
    pub ribbon: String,
    pub cycle: u32,
    /// Index of the stage that gives this arm green.
    pub phase: u32,
    pub t_green: f64,
    pub green_s: f64,
    pub amber_s: f64,
    /// From the end of amber to the next green (to the end of the run for
    /// the last cycle).
    pub red_s: f64,
    /// Vehicles standing on the arriving ribbon when green began.
    pub queue_at_green: u32,
    /// Vehicles that crossed the stop line in the cycle, and those of them
    /// that had stood in the queue.
    pub discharged: u32,
    pub discharged_from_queue: u32,
    /// Saturation flow over the whole width of the approach: queued
    /// vehicles after the third, over the time from the third's crossing
    /// to the last one's. NaN with fewer than eight queued vehicles.
    pub saturation_vehicles: u32,
    pub saturation_headway: f64,
    /// veh/h, and veh/h per metre of approach width (the road is lane-free).
    pub saturation_flow: f64,
    pub approach_width: f64,
    pub saturation_flow_per_m: f64,
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
    /// Heading of the body in world coordinates (rad).
    pub heading: Vec<f64>,
    pub v: Vec<f64>,
    /// Lateral speed (m/s, positive to the left of travel).
    pub v_lat: Vec<f64>,
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
        self.v_lat.clear();
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
    pub conflicts: Vec<ConflictRecord>,
    pub effective_width: Vec<EffectiveWidthRecord>,
    pub signal_cycles: Vec<SignalCycleRecord>,
    /// Headline KPIs as `(name, value)`, in a fixed order.
    pub kpis: Vec<(&'static str, f64)>,
    /// Ribbon ids by index (for decoding `TrajectoryChunk::ribbon`).
    pub ribbons: Vec<String>,
}
