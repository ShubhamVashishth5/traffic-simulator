//! Struct-of-arrays agent store (plan/02-architecture.md §2).
//!
//! Agents are kept densely; exits are removed by a stable compaction at the
//! end of each step, so the store's order is deterministic.

/// Marker for "no agent" where an agent id is stored.
pub const NO_AGENT: u64 = u64::MAX;
/// `decision_leader` of an agent that has not decided yet (forces a decision).
pub const UNDECIDED: u64 = u64::MAX - 1;

/// Column-oriented storage of all vehicles in the network.
#[derive(Debug, Default, Clone)]
pub struct AgentStore {
    pub id: Vec<u64>,
    pub vtype: Vec<u8>,
    pub class: Vec<u8>,
    pub entry: Vec<u16>,
    pub ribbon: Vec<u32>,
    pub s: Vec<f64>,
    pub s_prev: Vec<f64>,
    pub y: Vec<f64>,
    pub y_prev: Vec<f64>,
    pub v: Vec<f64>,
    /// Lateral speed (m/s, positive to the left).
    pub vy: Vec<f64>,
    pub acc: Vec<f64>,
    /// Tangent of the heading relative to the ribbon axis, within the heading
    /// limit (the tangent, so the step loop needs no trigonometry).
    pub heading_tan: Vec<f64>,
    pub heading_tan_prev: Vec<f64>,
    /// Half of the body's lateral extent at its heading:
    /// `(w cos ψ + l sin|ψ|) / 2` (ADR-0010 §4).
    pub half_width: Vec<f64>,
    /// Lateral target `y*` of the last decision.
    pub y_target: Vec<f64>,
    /// Most constraining leader (agent id) at the last decision.
    pub decision_leader: Vec<u64>,
    pub length: Vec<f64>,
    pub width: Vec<f64>,
    /// Desired speed in force: `v0_base`, plus `v0_bonus` inside the median band.
    pub v0: Vec<f64>,
    /// Desired speed drawn at entry.
    pub v0_base: Vec<f64>,
    /// The agent's median-band speed bonus (applies while it is in the band).
    pub v0_bonus: Vec<f64>,
    pub time_gap: Vec<f64>,
    pub standstill_gap: Vec<f64>,
    pub max_accel: Vec<f64>,
    pub comfortable_decel: Vec<f64>,
    /// Aggressiveness κ (placeholder until personas, P8).
    pub kappa: Vec<f64>,
    /// `exp(−γ_c κ)`: the agent's scale on lateral clearances.
    pub clearance_scale: Vec<f64>,
    /// The agent's cruising draw (see [`crate::demand::Draws::cruising`]).
    pub cruising: Vec<f64>,
    /// Stop-anywhere state (ADR-0014): one of the `STOP_*` values.
    pub stop_state: Vec<u8>,
    /// Exposure left until the next stop-anywhere event (m of travel, an
    /// attractor's multiplier applied); infinite for a class that never stops.
    pub stop_exposure: Vec<f64>,
    /// Travelled distance at which a stopping vehicle aims to stand.
    pub stop_point: Vec<f64>,
    /// While stopping: the dwell drawn (s). While standing: when it ends.
    pub stop_until: Vec<f64>,
    /// Stop-anywhere events so far.
    pub stop_count: Vec<u32>,
    /// How far from the kerb a vehicle that pulls over aims to stand (m).
    pub stop_offset: Vec<f64>,
    /// The bus stop it served last (index), or `u32::MAX`.
    pub stop_last: Vec<u32>,
    /// The speed it takes a hump at (m/s).
    pub hump_speed: Vec<f64>,
    pub t_arrival: Vec<f64>,
    pub t_entry: Vec<f64>,
    pub entry_y: Vec<f64>,
    pub distance: Vec<f64>,
    /// Time the distance so far would have taken at the desired speed in
    /// force along the way (s).
    pub free_flow_time: Vec<f64>,
    /// Sum of |lateral displacement| (m).
    pub lateral_distance: Vec<f64>,
    /// Number of times the agent took a new lateral target.
    pub lateral_moves: Vec<u32>,
    pub stops: Vec<u32>,
    pub stop_armed: Vec<bool>,
    /// Open conflict episode: the other agent's id (or [`NO_AGENT`]), when it
    /// started, its minimum TTC, maximum DRAC and type so far, and where the
    /// minimum occurred.
    pub conflict_other: Vec<u64>,
    pub conflict_t0: Vec<f64>,
    pub conflict_min_ttc: Vec<f64>,
    pub conflict_max_drac: Vec<f64>,
    pub conflict_kind: Vec<u8>,
    pub conflict_s: Vec<f64>,
    pub conflict_y: Vec<f64>,
    /// The ribbon it was on before this one ([`NO_RIBBON`] at its entry).
    pub came_from: Vec<u32>,
    /// The junction it is at or will reach next ([`NO_RIBBON`]: none), and
    /// the arm it leaves that junction by.
    pub turn_junction: Vec<u32>,
    pub turn_out: Vec<u8>,
    /// It may pass the stop line ahead whatever the line shows: it took a
    /// gap, or could not stop when the signal changed.
    pub gate_open: Vec<bool>,
    /// When it began to wait at the stop line ahead (NaN: not waiting).
    pub wait_since: Vec<f64>,
    /// The conflicting vehicle that ends the gap it is looking at.
    pub gap_ref: Vec<u64>,
    /// Largest gap it turned down at this stop line (s; NaN: none) and how many.
    pub gap_rejected: Vec<f64>,
    pub gap_rejects: Vec<u32>,
    /// Its critical gap (s) and its factor against heavy vehicles.
    pub critical_gap: Vec<f64>,
    pub heavy_factor: Vec<f64>,
    /// How far before a split it learns which branch it needs (m), whether
    /// it is a late realiser, and whether it knows now.
    pub aware_distance: Vec<f64>,
    pub late_realiser: Vec<bool>,
    pub aware: Vec<bool>,
    /// At a split: one of the `SPLIT_*` values.
    pub split_state: Vec<u8>,
    /// Lateral position it makes for on the ribbon it is on (NaN: none).
    pub land_y: Vec<f64>,
    /// It stood at a closed signal on this approach, and when it moved off.
    pub queued: Vec<bool>,
    pub moved_off: Vec<f64>,
    /// Set during the step when the agent leaves the network.
    pub exited: Vec<bool>,
}

/// Marker for "no ribbon" / "no junction".
pub const NO_RIBBON: u32 = u32::MAX;
/// Split states: not decided; it is in its branch's part of the road or
/// still moving there; it stopped to wait for room; it gave up.
pub const SPLIT_NONE: u8 = 0;
pub const SPLIT_WAIT: u8 = 1;
pub const SPLIT_GAVE_UP: u8 = 2;
pub const SPLIT_DECIDED: u8 = 3;

/// Everything needed to add one agent.
#[derive(Debug, Clone, Copy)]
pub struct NewAgent {
    pub id: u64,
    pub vtype: u8,
    pub class: u8,
    pub entry: u16,
    pub ribbon: u32,
    pub s: f64,
    pub y: f64,
    pub v: f64,
    pub length: f64,
    pub width: f64,
    pub v0: f64,
    pub v0_base: f64,
    pub v0_bonus: f64,
    pub time_gap: f64,
    pub standstill_gap: f64,
    pub max_accel: f64,
    pub comfortable_decel: f64,
    pub kappa: f64,
    pub clearance_scale: f64,
    pub cruising: f64,
    pub hump_speed: f64,
    pub t_arrival: f64,
    pub t_entry: f64,
    pub critical_gap: f64,
    pub heavy_factor: f64,
    pub aware_distance: f64,
    pub late_realiser: bool,
}

/// Stop-anywhere states: driving; braking for a stop while making for the
/// kerb; braking for a stop where it is; standing.
pub const STOP_NONE: u8 = 0;
pub const STOP_PULLING_OVER: u8 = 1;
pub const STOP_IN_PLACE: u8 = 2;
pub const STOP_STANDING: u8 = 3;

/// Marker for a removed agent in a remap table.
pub const REMOVED: u32 = u32::MAX;

/// Expand `$m!(column)` for every column of the store.
macro_rules! each_column {
    ($s:ident, $m:ident) => {
        $m!($s.id);
        $m!($s.vtype);
        $m!($s.class);
        $m!($s.entry);
        $m!($s.ribbon);
        $m!($s.s);
        $m!($s.s_prev);
        $m!($s.y);
        $m!($s.y_prev);
        $m!($s.v);
        $m!($s.vy);
        $m!($s.acc);
        $m!($s.heading_tan);
        $m!($s.heading_tan_prev);
        $m!($s.half_width);
        $m!($s.y_target);
        $m!($s.decision_leader);
        $m!($s.length);
        $m!($s.width);
        $m!($s.v0);
        $m!($s.v0_base);
        $m!($s.v0_bonus);
        $m!($s.time_gap);
        $m!($s.standstill_gap);
        $m!($s.max_accel);
        $m!($s.comfortable_decel);
        $m!($s.kappa);
        $m!($s.clearance_scale);
        $m!($s.cruising);
        $m!($s.stop_state);
        $m!($s.stop_exposure);
        $m!($s.stop_point);
        $m!($s.stop_until);
        $m!($s.stop_count);
        $m!($s.stop_offset);
        $m!($s.stop_last);
        $m!($s.hump_speed);
        $m!($s.t_arrival);
        $m!($s.t_entry);
        $m!($s.entry_y);
        $m!($s.distance);
        $m!($s.free_flow_time);
        $m!($s.lateral_distance);
        $m!($s.lateral_moves);
        $m!($s.stops);
        $m!($s.stop_armed);
        $m!($s.conflict_other);
        $m!($s.conflict_t0);
        $m!($s.conflict_min_ttc);
        $m!($s.conflict_max_drac);
        $m!($s.conflict_kind);
        $m!($s.conflict_s);
        $m!($s.conflict_y);
        $m!($s.came_from);
        $m!($s.turn_junction);
        $m!($s.turn_out);
        $m!($s.gate_open);
        $m!($s.wait_since);
        $m!($s.gap_ref);
        $m!($s.gap_rejected);
        $m!($s.gap_rejects);
        $m!($s.critical_gap);
        $m!($s.heavy_factor);
        $m!($s.aware_distance);
        $m!($s.late_realiser);
        $m!($s.aware);
        $m!($s.split_state);
        $m!($s.land_y);
        $m!($s.queued);
        $m!($s.moved_off);
        $m!($s.exited);
    };
}

impl AgentStore {
    pub fn len(&self) -> usize {
        self.id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_empty()
    }

    /// Heading of agent `i` relative to the ribbon axis (rad).
    pub fn heading(&self, i: usize) -> f64 {
        libm::atan(self.heading_tan[i])
    }

    /// Append an agent; returns its index.
    pub fn push(&mut self, a: NewAgent) -> u32 {
        let i = self.len() as u32;
        self.id.push(a.id);
        self.vtype.push(a.vtype);
        self.class.push(a.class);
        self.entry.push(a.entry);
        self.ribbon.push(a.ribbon);
        self.s.push(a.s);
        self.s_prev.push(a.s);
        self.y.push(a.y);
        self.y_prev.push(a.y);
        self.v.push(a.v);
        self.vy.push(0.0);
        self.acc.push(0.0);
        self.heading_tan.push(0.0);
        self.heading_tan_prev.push(0.0);
        self.half_width.push(0.5 * a.width);
        self.y_target.push(a.y);
        self.decision_leader.push(UNDECIDED);
        self.length.push(a.length);
        self.width.push(a.width);
        self.v0.push(a.v0);
        self.v0_base.push(a.v0_base);
        self.v0_bonus.push(a.v0_bonus);
        self.time_gap.push(a.time_gap);
        self.standstill_gap.push(a.standstill_gap);
        self.max_accel.push(a.max_accel);
        self.comfortable_decel.push(a.comfortable_decel);
        self.kappa.push(a.kappa);
        self.clearance_scale.push(a.clearance_scale);
        self.cruising.push(a.cruising);
        self.stop_state.push(STOP_NONE);
        self.stop_exposure.push(f64::INFINITY);
        self.stop_point.push(0.0);
        self.stop_until.push(0.0);
        self.stop_count.push(0);
        self.stop_offset.push(0.0);
        self.stop_last.push(u32::MAX);
        self.hump_speed.push(a.hump_speed);
        self.t_arrival.push(a.t_arrival);
        self.t_entry.push(a.t_entry);
        self.entry_y.push(a.y);
        self.distance.push(0.0);
        self.free_flow_time.push(0.0);
        self.lateral_distance.push(0.0);
        self.lateral_moves.push(0);
        self.stops.push(0);
        self.stop_armed.push(true);
        self.conflict_other.push(NO_AGENT);
        self.conflict_t0.push(0.0);
        self.conflict_min_ttc.push(f64::INFINITY);
        self.conflict_max_drac.push(0.0);
        self.conflict_kind.push(0);
        self.conflict_s.push(0.0);
        self.conflict_y.push(0.0);
        self.came_from.push(NO_RIBBON);
        self.turn_junction.push(NO_RIBBON);
        self.turn_out.push(0);
        self.gate_open.push(false);
        self.wait_since.push(f64::NAN);
        self.gap_ref.push(NO_AGENT);
        self.gap_rejected.push(f64::NAN);
        self.gap_rejects.push(0);
        self.critical_gap.push(a.critical_gap);
        self.heavy_factor.push(a.heavy_factor);
        self.aware_distance.push(a.aware_distance);
        self.late_realiser.push(a.late_realiser);
        self.aware.push(false);
        self.split_state.push(SPLIT_NONE);
        self.land_y.push(f64::NAN);
        self.queued.push(false);
        self.moved_off.push(f64::NAN);
        self.exited.push(false);
        i
    }

    /// Remove every agent with `exited` set, keeping the others in order.
    /// Writes `remap[old] = new index or REMOVED` and returns whether anything moved.
    pub fn compact(&mut self, remap: &mut Vec<u32>) -> bool {
        let n = self.len();
        remap.clear();
        remap.reserve(n);
        let mut next = 0u32;
        for i in 0..n {
            if self.exited[i] {
                remap.push(REMOVED);
            } else {
                remap.push(next);
                next += 1;
            }
        }
        if next as usize == n {
            return false;
        }
        macro_rules! retain_col {
            ($col:expr) => {{
                let mut k = remap.iter();
                $col.retain(|_| *k.next().unwrap() != REMOVED);
            }};
        }
        each_column!(self, retain_col);
        true
    }

    /// Drop all agents.
    pub fn clear(&mut self) {
        macro_rules! clear_col {
            ($col:expr) => {
                $col.clear()
            };
        }
        each_column!(self, clear_col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: u64, s: f64) -> NewAgent {
        NewAgent {
            id,
            vtype: 0,
            class: 0,
            entry: 0,
            ribbon: 0,
            s,
            y: 0.0,
            v: 1.0,
            length: 4.0,
            width: 1.8,
            v0: 10.0,
            v0_base: 10.0,
            v0_bonus: 0.0,
            time_gap: 1.0,
            standstill_gap: 2.0,
            max_accel: 1.0,
            comfortable_decel: 1.0,
            kappa: 0.0,
            clearance_scale: 1.0,
            cruising: 0.0,
            hump_speed: 5.0,
            t_arrival: 0.0,
            t_entry: 0.0,
            critical_gap: 5.0,
            heavy_factor: 1.0,
            aware_distance: f64::INFINITY,
            late_realiser: false,
        }
    }

    #[test]
    fn compaction_is_stable() {
        let mut st = AgentStore::default();
        for i in 0..6 {
            st.push(agent(i, i as f64));
        }
        st.exited[1] = true;
        st.exited[4] = true;
        let mut remap = Vec::new();
        assert!(st.compact(&mut remap));
        assert_eq!(st.id, vec![0, 2, 3, 5]);
        assert_eq!(st.s, vec![0.0, 2.0, 3.0, 5.0]);
        assert_eq!(remap, vec![0, REMOVED, 1, 2, REMOVED, 3]);
        assert_eq!(st.exited.len(), 4);
    }

    #[test]
    fn many_cycles_do_not_leak() {
        let mut st = AgentStore::default();
        let mut remap = Vec::new();
        for round in 0..1000u64 {
            for k in 0..10 {
                st.push(agent(round * 10 + k, 0.0));
            }
            for e in st.exited.iter_mut() {
                *e = true;
            }
            st.compact(&mut remap);
            assert!(st.is_empty());
        }
        st.clear();
        assert!(st.id.capacity() < 1000);
    }
}
