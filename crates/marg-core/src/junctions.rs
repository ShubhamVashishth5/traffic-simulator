//! Junctions at run time (plan/phases/P07-junctions-splits.md, ADR-0016):
//! where each vehicle goes, what the signals show, which vehicles are near
//! each junction (for interaction across ribbons), and the record of every
//! signal cycle.
//!
//! Nothing here scales capacity or flow (ADR-0005). A signal closes a stop
//! line; a curve caps speed; a priority rule makes a vehicle wait for a gap.

use crate::agents::AgentStore;
use crate::arbitration::Arbiter;
use crate::output::SignalCycleRecord;
use crate::params::CoreParams;
use crate::rng::{StreamKind, StreamRng};
use marg_net::{Control, Network, Phase, Turn};
use marg_params::spec::{ControlState, CountdownState, JunctionKind};

/// What the signal shows the movement that starts on a ribbon.
pub const LIGHT_NONE: u8 = 0;
pub const LIGHT_GREEN: u8 = 1;
pub const LIGHT_AMBER: u8 = 2;
pub const LIGHT_RED: u8 = 3;

/// How a junction decides who goes first this step (P8.7, ADR-0019).
/// Ranked: an uncontrolled junction with its ranks (all equal: nobody waits
/// for a gap), or a blinker with major arms.
pub const MODE_RANKED: u8 = 0;
/// A signal or a traffic officer shows the lights.
pub const MODE_SIGNAL: u8 = 1;
/// Dark, or a blinker with no major arm: every movement gives way to every
/// movement it conflicts with (G§6.6: nobody knows who should yield).
pub const MODE_MUTUAL: u8 = 2;

/// Counters of a junction's outage draws in the `Environment` stream:
/// outage `n` starts after a gap at `3n` and lasts for a draw at `3n + 1`.
const OUTAGE_DRAWS: u64 = 3;
/// Crossings on red are counted by third of the red (plan/04).
const RED_BINS: usize = 3;
const HOURS_PER_DAY: f64 = 24.0;

/// Queued vehicles left out of a saturation-flow measurement at the start
/// of green, and the fewest it needs after them (the usual field method:
/// headways from the fourth queued vehicle on, HCM 2010 ch. 31).
const SATURATION_SKIP: usize = 3;
const SATURATION_MIN: usize = 5;

/// The leaving arm agent `id` takes at `junction`, arriving by `arm`.
#[inline]
pub fn draw_turn(seed: u64, id: u64, net: &Network, junction: usize, arm: usize) -> u8 {
    let counter = (junction as u64) << 8 | arm as u64;
    let u = StreamRng::at(seed, StreamKind::Routes, id, counter).uniform();
    net.junctions[junction].draw_out(arm, u) as u8
}

/// The ribbon after `ribbon` for agent `id`, whose current choice is
/// `turn = (junction, leaving arm)`; `turn` is updated when the walk
/// reaches another junction.
#[inline]
pub fn onward(
    net: &Network,
    seed: u64,
    id: u64,
    ribbon: usize,
    turn: &mut (u32, u8),
) -> Option<usize> {
    let rib = net.ribbon(ribbon);
    if let Some(n) = rib.next {
        return Some(n);
    }
    if rib.branch.is_empty() {
        return None;
    }
    branch(net, seed, id, ribbon, turn)
}

#[cold]
fn branch(net: &Network, seed: u64, id: u64, ribbon: usize, turn: &mut (u32, u8)) -> Option<usize> {
    let rib = net.ribbon(ribbon);
    let j = rib.junction.expect("a branching ribbon has a junction");
    if turn.0 as usize != j {
        let arm = rib.arm_in?;
        *turn = (j as u32, draw_turn(seed, id, net, j, arm));
    }
    rib.branch[usize::from(turn.1)]
}

/// The body of agent `i` at `(s, y)` on its ribbon as vehicles on other
/// ribbons see it (ADR-0016 part 4): the chord from the point of its path
/// half a length behind its centre to the point half a length ahead, at its
/// lateral offset, lying along that chord. On a straight ribbon that is the
/// footprint the vehicle has on its own ribbon; on a curve it lies inside
/// the arc, as a vehicle's rear follows inside its front, instead of its
/// ends swinging out. As on its own ribbon, a turned body is not rotated
/// but made wider across its path (`body_half_width`): rotating a long body
/// that creeps sideways would swing its ends by metres. `tan` is unused and
/// kept for the callers. Returns the centre and the unit heading
/// `(x, y, hx, hy)`.
#[allow(clippy::too_many_arguments)]
pub fn chord_pose(
    net: &Network,
    seed: u64,
    prev: &[Option<usize>],
    a: &AgentStore,
    i: usize,
    s: f64,
    y: f64,
    tan: f64,
) -> (f64, f64, f64, f64) {
    let r = a.ribbon[i] as usize;
    let rib = net.ribbon(r);
    let half = 0.5 * a.length[i];
    let len = rib.length();
    let (back, ahead) = (s - half, s + half);
    let rear = if back >= 0.0 {
        rib.to_world(back, y)
    } else {
        let came = a.came_from[i];
        match (came != crate::agents::NO_RIBBON)
            .then_some(came as usize)
            .or(prev[r])
        {
            Some(q) => net.ribbon(q).to_world(net.ribbon(q).length() + back, y),
            None => rib.to_world(back, y),
        }
    };
    let front = if ahead <= len {
        rib.to_world(ahead, y)
    } else {
        let mut turn = (a.turn_junction[i], a.turn_out[i]);
        match onward(net, seed, a.id[i], r, &mut turn) {
            Some(n) => net.ribbon(n).to_world(ahead - len, y),
            None => rib.to_world(ahead, y),
        }
    };
    let (dx, dy) = (front.0 - rear.0, front.1 - rear.1);
    let norm = libm::hypot(dx, dy);
    let d = if norm > 0.0 {
        [dx / norm, dy / norm]
    } else {
        rib.direction(s)
    };
    let _ = tan;
    (
        0.5 * (rear.0 + front.0),
        0.5 * (rear.1 + front.1),
        d[0],
        d[1],
    )
}

/// A vehicle near a junction, in world coordinates.
#[derive(Debug, Clone, Copy)]
pub struct Member {
    pub i: u32,
    pub ribbon: u32,
    pub x: f64,
    pub y: f64,
    /// Unit vector of its body's heading.
    pub hx: f64,
    pub hy: f64,
}

/// One arm's part of a signal cycle, being measured.
#[derive(Debug, Clone, Default)]
struct ArmCycle {
    open: bool,
    cycle: u32,
    phase: usize,
    t_green: f64,
    green_s: f64,
    amber_s: f64,
    queue_at_green: u32,
    /// Stop-line crossings since green began: time, and whether the
    /// vehicle had stood in the queue.
    /// Also whether it crossed on red (a red-light runner, a seeper).
    crossings: Vec<(f64, bool, bool)>,
    /// Who showed the lights (fixed plan or officer).
    control: ControlState,
    /// Crossings on red, by third of the red (plan/04).
    red_bins: [u32; RED_BINS],
    /// A cycle has been opened before.
    started: bool,
}

#[derive(Debug, Clone)]
struct Signal {
    junction: usize,
    /// Stage shown at the last step: `(phase, 0 green | 1 amber | 2 all red)`.
    shown: Option<(usize, u8)>,
    arms: Vec<ArmCycle>,
    /// The state set by the scenario, its schedule or a live toggle; the
    /// next scripted change.
    base: ControlState,
    next_scripted: usize,
    /// Outage overlay: 0 none, 1 dark, 2 an officer has arrived.
    outage: u8,
    outages: u64,
    outage_next: f64,
    outage_end: f64,
    officer_at: f64,
    /// On the night blinker schedule and blinking now.
    blinking: bool,
    /// The state in force and when it began.
    state: ControlState,
    since: f64,
    /// The officer's plan (phases, cycle), if the register has one.
    police: Option<(Vec<Phase>, f64)>,
}

/// A change of a signal controller's state, for the event log.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlChange {
    pub t: f64,
    pub junction: usize,
    pub from: ControlState,
    pub to: ControlState,
    /// `"schedule"`, `"live"`, `"outage"`, `"officer"`, `"repaired"`,
    /// `"blinker_on"` or `"blinker_off"`.
    pub cause: &'static str,
}

/// A vehicle in the option zone at amber onset: it can still stop at the
/// safe braking limit and would reach the line within the amber at its
/// speed (the one-shot `amber_go` decision, ADR-0019).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmberOption {
    pub i: u32,
    pub junction: u32,
    /// Its movement's first ribbon.
    pub first: u32,
    /// Gap from its front to the line (m), amber and red to come (s).
    pub gap: f64,
    pub amber_s: f64,
    pub red_s: f64,
}

/// A change of a signal, for the event log.
#[derive(Debug, Clone, Copy)]
pub struct SignalChange {
    pub junction: usize,
    pub phase: usize,
    /// `"green"`, `"amber"` or `"all_red"`.
    pub state: &'static str,
    /// Seconds the stage lasts.
    pub lasts: f64,
    /// Who shows it.
    pub control: ControlState,
}

/// Run-time state of every junction.
#[derive(Debug, Clone, Default)]
pub struct Junctions {
    /// Whether the network has any junction or merge at all.
    pub enabled: bool,
    /// Per ribbon: what the signal shows the movement starting on it.
    pub light: Vec<u8>,
    /// Per ribbon: the speed its curvature allows (m/s).
    pub speed_cap: Vec<f64>,
    pub any_caps: bool,
    /// Per ribbon: the stop line reached by following `next`: the ribbon it
    /// ends, and the distance from this ribbon's start to the line.
    pub line: Vec<Option<(u32, f64)>>,
    /// Per ribbon: the groups its vehicles belong to, with the stretch of
    /// the ribbon that counts.
    zones: Vec<Vec<(u32, f64, f64)>>,
    /// Per group: its vehicles this step, in ribbon and position order.
    pub members: Vec<Vec<Member>>,
    /// `related[a * n + b]`: vehicles on ribbons `a` and `b` can be in
    /// each other's way though neither ribbon follows the other for them.
    related: Vec<bool>,
    n: usize,
    /// Per junction and movement: the part of the arriving ribbon's width
    /// the movement is made from.
    pub window: Vec<Vec<Option<(f64, f64)>>>,
    /// Whether the movement can only be made from its window (a split, a
    /// U-turn) or is merely made from that side by habit (a turn at a box).
    hard: Vec<Vec<bool>>,
    signals: Vec<Signal>,
    pub cycles: Vec<SignalCycleRecord>,
    /// Who goes first between vehicles whose paths meet (ADR-0016 part 4).
    pub arbiter: Arbiter,
    /// Per junction: one of the `MODE_*` values.
    pub mode: Vec<u8>,
    /// Per ribbon: its priority rank now (the network's, or the blinker's).
    pub rank: Vec<u8>,
    /// Per movement ribbon under a signal: when its last amber and its red
    /// began, when its next green begins (NaN: green, or no signal), and
    /// the change interval (amber and all-red) just before that green.
    pub amber_start: Vec<f64>,
    pub red_start: Vec<f64>,
    pub next_green: Vec<f64>,
    pub change_before_green: Vec<f64>,
    /// This step's amber-onset decisions to take (filled only when
    /// `amber_go` runs), and the controller changes to log.
    pub amber_options: Vec<AmberOption>,
    pub control_log: Vec<ControlChange>,
    /// Live requests, applied at the start of the next step.
    requests: Vec<(usize, ControlState)>,
    /// Hour of day at time 0, for the blinker schedule.
    start_hour: Option<f64>,
    /// Per movement ribbon under a signal: the station on it where the
    /// first area it shares with a movement from another arm starts (NaN:
    /// none).
    /// A vehicle creeping past its red line stands short of it
    /// (`stopline_seepage`, ADR-0020 part 5).
    pub seep_hold: Vec<f64>,
    /// Per ribbon inside a junction: `±1 / 8ρ`, so that a body of length
    /// `L` sweeps `L² · sweep_coef` towards the inside of the turn
    /// (positive: the inside is to the left; ADR-0016 open problem 3).
    pub sweep_coef: Vec<f64>,
    /// Dark junctions take turns, first come first served (ADR-0019 D1).
    pub first_come: bool,
    /// Per junction, under first come first served: the vehicles waiting at
    /// its lines this step, `(when they got there, id, first ribbon)`.
    pub waiting_at_lines: Vec<Vec<(f64, u64, u32)>>,
}

impl Junctions {
    pub fn new(
        net: &Network,
        p: &CoreParams,
        seed: u64,
        start_hour: Option<f64>,
    ) -> Result<Self, String> {
        let n = net.len();
        let merges = (0..n).any(|r| net.ribbon(r).preds > 1);
        let enabled = !net.junctions.is_empty() || merges;
        let mut line = vec![None; n];
        for (r, slot) in line.iter_mut().enumerate() {
            let (mut at, mut dist) = (r, 0.0);
            *slot = loop {
                let rib = net.ribbon(at);
                dist += rib.length();
                if rib.arm_in.is_some() {
                    break Some((at as u32, dist));
                }
                match rib.next {
                    Some(nx) => at = nx,
                    None => break None,
                }
            };
        }
        let mut speed_cap = vec![f64::INFINITY; n];
        if let Some(a_lat) = p.junction.turn_accel {
            for (r, cap) in speed_cap.iter_mut().enumerate() {
                let rib = net.ribbon(r);
                if rib.internal && rib.min_radius.is_finite() {
                    *cap = (a_lat * rib.min_radius).sqrt();
                }
            }
        }
        let any_caps = speed_cap.iter().any(|c| c.is_finite());
        // Groups: one per junction, one per ribbon that several lead onto.
        let reach = p.lookahead;
        let mut zones = vec![Vec::new(); n];
        let mut groups = 0u32;
        for j in &net.junctions {
            for &r in &j.ribbons {
                zones[r].push((groups, f64::NEG_INFINITY, f64::INFINITY));
            }
            for arm in &j.arms {
                if let Some(r) = arm.inbound {
                    zones[r].push((groups, net.ribbon(r).length() - reach, f64::INFINITY));
                }
                if let Some(r) = arm.outbound {
                    zones[r].push((groups, f64::NEG_INFINITY, reach));
                }
            }
            groups += 1;
        }
        for r in 0..n {
            if net.ribbon(r).preds > 1 && !net.ribbon(r).internal {
                let feeders: Vec<usize> = net
                    .predecessors(r)
                    .filter(|&q| !net.ribbon(q).internal)
                    .collect();
                if feeders.len() > 1 {
                    zones[r].push((groups, f64::NEG_INFINITY, reach));
                    for q in feeders {
                        zones[q].push((groups, net.ribbon(q).length() - reach, f64::INFINITY));
                    }
                    groups += 1;
                }
            }
        }
        // Ribbons whose vehicles can meet: any ribbon inside a junction with
        // any other, and ribbons that share a successor or a predecessor.
        let succ = |r: usize| -> Vec<usize> {
            let mut v: Vec<usize> = net.ribbon(r).branch.iter().flatten().copied().collect();
            v.extend(net.ribbon(r).next);
            v
        };
        let succs: Vec<Vec<usize>> = (0..n).map(succ).collect();
        let preds: Vec<Vec<usize>> = (0..n).map(|r| net.predecessors(r).collect()).collect();
        let mut related = vec![false; n * n];
        for a in 0..n {
            for b in 0..n {
                if a == b {
                    continue;
                }
                let share = |x: &[usize], y: &[usize]| x.iter().any(|q| y.contains(q));
                related[a * n + b] = net.ribbon(a).internal
                    || net.ribbon(b).internal
                    || share(&succs[a], &succs[b])
                    || share(&preds[a], &preds[b])
                    || succs[a].contains(&b)
                    || succs[b].contains(&a);
            }
        }
        // Where each movement can be made from.
        let lane = p.junction.six_lane_width / 3.0;
        let mut window = Vec::new();
        let mut hard = Vec::new();
        let mut needs_split = false;
        for j in &net.junctions {
            let mut w = Vec::new();
            let mut h = Vec::new();
            for m in &j.movements {
                let rin = j.arms[m.from]
                    .inbound
                    .expect("a movement has an arriving ribbon");
                let (lo, hi) = net.ribbon(rin).y_bounds(net.ribbon(rin).length());
                let win = match (m.window, m.turn, j.kind) {
                    (Some(win), _, _) if win.0 > lo || win.1 < hi => Some(win),
                    // A U-turn is made from the median side: one notional lane.
                    (None, Turn::UTurn, JunctionKind::Box) if hi - lo > lane => {
                        Some((lo, lo + lane))
                    }
                    _ => None,
                };
                needs_split |= win.is_some();
                h.push(win.is_some());
                // A turn at a box is made from its own side of the road
                // where the approach has room for more than two notional
                // lanes abreast; nothing forces it (G§7.5).
                let win = win.or(match (m.turn, j.kind) {
                    (Turn::Right, JunctionKind::Box) if hi - lo >= 2.0 * lane => {
                        Some((lo, lo + lane))
                    }
                    (Turn::Left, JunctionKind::Box) if hi - lo >= 2.0 * lane => {
                        Some((hi - lane, hi))
                    }
                    _ => None,
                });
                w.push(win);
            }
            window.push(w);
            hard.push(h);
        }
        if needs_split {
            let missing = [
                (
                    "road.split.stop_and_wait_probability",
                    p.junction.stop_and_wait.is_none(),
                ),
                ("road.split.route_weight", p.junction.route_weight.is_none()),
                (
                    "road.split.urgency_clearance_link",
                    p.junction.urgency_link.is_none(),
                ),
            ];
            if let Some((path, _)) = missing.iter().find(|(_, unset)| *unset) {
                return Err(format!(
                    "a split or a U-turn needs `{path}`, which the register leaves unset: give it in the scenario's overrides"
                ));
            }
        }
        for j in &net.junctions {
            if j.kind == JunctionKind::Roundabout
                && (p.junction.ring_gap_factor.is_none() || p.junction.ring_compliance.is_none())
            {
                return Err(
                    "a roundabout needs `road.junction.roundabout.entry_critical_gap_factor` and `road.junction.roundabout.priority_compliance`, which the register leaves unset: give them in the scenario's overrides".into(),
                );
            }
            if j.movements.iter().any(|m| m.turn == Turn::UTurn)
                && j.kind == JunctionKind::Box
                && p.junction.u_turn_gap_factor.is_none()
            {
                return Err(
                    "a U-turn needs `interaction.gap_acceptance.u_turn.critical_gap_factor`, which the register leaves unset: give it in the scenario's overrides".into(),
                );
            }
        }
        let mut signals = Vec::new();
        for (k, j) in net.junctions.iter().enumerate() {
            let Control::Signal {
                phases,
                state,
                schedule,
                police_major,
                night_blinker,
                ..
            } = &j.control
            else {
                continue;
            };
            let sp = &p.signal;
            let uses_police = *state == ControlState::Police
                || schedule.iter().any(|c| c.1 == ControlState::Police)
                || (sp.outage_rate.is_some() && sp.officer_delay.is_some());
            let police = match (sp.officer_cycle, sp.officer_bias) {
                (Some(c), Some(b)) => Some(police_plan(j, phases, police_major, c, b)?),
                _ if uses_police => {
                    return Err(format!(
                        "junction `{}`: police control needs `road.signal.officer.cycle_length` and `road.signal.officer.major_bias`, which the register leaves unset: give them in the scenario's overrides",
                        j.id
                    ));
                }
                _ => None,
            };
            if *night_blinker && start_hour.is_none() {
                return Err(format!(
                    "junction `{}`: `night_blinker` needs the scenario's `start_time_h`",
                    j.id
                ));
            }
            let mut sg = Signal {
                junction: k,
                shown: None,
                arms: vec![ArmCycle::default(); j.arms.len()],
                base: *state,
                next_scripted: 0,
                outage: 0,
                outages: 0,
                outage_next: f64::INFINITY,
                outage_end: f64::INFINITY,
                officer_at: f64::INFINITY,
                blinking: false,
                state: *state,
                since: 0.0,
                police,
            };
            if let Some(rate) = sp.outage_rate {
                sg.outage_next = outage_gap(seed, k, 0, rate);
            }
            signals.push(sg);
        }
        let mut mode = vec![MODE_RANKED; net.junctions.len()];
        for sg in &signals {
            mode[sg.junction] = MODE_SIGNAL;
        }
        let rank = (0..n).map(|r| net.ribbon(r).rank).collect();
        let mut out = Self {
            enabled,
            light: vec![LIGHT_NONE; n],
            speed_cap,
            any_caps,
            line,
            zones,
            members: vec![Vec::new(); groups as usize],
            related,
            n,
            window,
            hard,
            signals,
            cycles: Vec::new(),
            arbiter: Arbiter::new(net),
            mode,
            rank,
            amber_start: vec![f64::NAN; n],
            red_start: vec![f64::NAN; n],
            next_green: vec![f64::NAN; n],
            change_before_green: vec![f64::NAN; n],
            amber_options: Vec::new(),
            control_log: Vec::new(),
            requests: Vec::new(),
            start_hour,
            seep_hold: seep_holds(net),
            sweep_coef: sweep_coefs(net),
            first_come: p.signal.dark_first_come,
            waiting_at_lines: vec![Vec::new(); net.junctions.len()],
        };
        // A junction that starts other than fixed takes its regime now.
        for s in 0..out.signals.len() {
            let state = out.signals[s].state;
            out.apply_mode(s, state, net);
        }
        Ok(out)
    }

    /// Whether junction `j`'s lights are in force this step (a working
    /// signal or an officer).
    #[inline]
    pub fn signalised(&self, j: usize) -> bool {
        self.mode[j] == MODE_SIGNAL
    }

    /// The state of junction `j`'s controller (`None`: it has no signal).
    pub fn control_state(&self, j: usize) -> Option<ControlState> {
        self.signals
            .iter()
            .find(|s| s.junction == j)
            .map(|s| s.state)
    }

    /// Ask for junction `j`'s controller to be put in `state` from the next
    /// step on (the viewer's live toggle). The state stays until the next
    /// scripted change or toggle; an outage still overlays it.
    pub fn request(&mut self, j: usize, state: ControlState) -> Result<(), String> {
        let Some(sg) = self.signals.iter().find(|s| s.junction == j) else {
            return Err("the junction has no signal".into());
        };
        if state == ControlState::Police && sg.police.is_none() {
            return Err("police control needs `road.signal.officer.cycle_length` and `road.signal.officer.major_bias`".into());
        }
        self.requests.push((j, state));
        Ok(())
    }

    /// Set the decision mode and the ranks of signal `s`'s junction for
    /// `state`.
    fn apply_mode(&mut self, s: usize, state: ControlState, net: &Network) {
        let j = self.signals[s].junction;
        let jn = &net.junctions[j];
        let Control::Signal { blinker_ranks, .. } = &jn.control else {
            return;
        };
        self.mode[j] = match (state, blinker_ranks) {
            (ControlState::Fixed | ControlState::Police, _) => MODE_SIGNAL,
            (ControlState::Blinker, Some(_)) => MODE_RANKED,
            _ => MODE_MUTUAL,
        };
        for (k, m) in jn.movements.iter().enumerate() {
            self.rank[m.first] = match (state, blinker_ranks) {
                (ControlState::Blinker, Some(r)) => r[k],
                _ => net.ribbon(m.first).rank,
            };
        }
    }

    /// Bring signal `s`'s controller state to time `t`: scripted changes,
    /// live requests, the outage chain and the officer, the night blinker.
    fn update_state(&mut self, s: usize, t: f64, net: &Network, p: &CoreParams, seed: u64) {
        let j = self.signals[s].junction;
        let mut cause = "";
        {
            let sg = &mut self.signals[s];
            let Control::Signal { schedule, .. } = &net.junctions[j].control else {
                return;
            };
            while sg.next_scripted < schedule.len() && schedule[sg.next_scripted].0 <= t {
                sg.base = schedule[sg.next_scripted].1;
                sg.next_scripted += 1;
                cause = "schedule";
            }
        }
        for k in 0..self.requests.len() {
            if self.requests[k].0 == j {
                self.signals[s].base = self.requests[k].1;
                cause = "live";
            }
        }
        let sg = &mut self.signals[s];
        if let Some(rate) = p.signal.outage_rate {
            if sg.outage == 0 && t >= sg.outage_next {
                let n = sg.outages;
                let dur = p
                    .signal
                    .outage_duration
                    .as_ref()
                    .expect("checked at start-up")
                    .sample(&mut StreamRng::at(
                        seed,
                        StreamKind::Environment,
                        j as u64,
                        OUTAGE_DRAWS * n + 1,
                    ))
                    .max(0.0);
                sg.outage = 1;
                sg.outage_end = sg.outage_next + dur;
                sg.officer_at = p
                    .signal
                    .officer_delay
                    .map_or(f64::INFINITY, |d| sg.outage_next + d);
                cause = "outage";
            }
            if sg.outage == 1 && t >= sg.officer_at && t < sg.outage_end {
                sg.outage = 2;
                cause = "officer";
            }
            if sg.outage != 0 && t >= sg.outage_end {
                sg.outage = 0;
                sg.outages += 1;
                sg.outage_next = sg.outage_end + outage_gap(seed, j, sg.outages, rate);
                cause = "repaired";
            }
        }
        let Control::Signal { night_blinker, .. } = &net.junctions[j].control else {
            return;
        };
        if *night_blinker && let Some(h0) = self.start_hour {
            let hour = (h0 + t / crate::demand::SECONDS_PER_HOUR).rem_euclid(HOURS_PER_DAY);
            let (on, off) = (p.signal.blinker_start, p.signal.blinker_end);
            let blink = if on <= off {
                hour >= on && hour < off
            } else {
                hour >= on || hour < off
            };
            if blink != sg.blinking {
                sg.blinking = blink;
                cause = if blink { "blinker_on" } else { "blinker_off" };
            }
        }
        let state = match sg.outage {
            2 => ControlState::Police,
            1 => ControlState::Dark,
            _ if sg.blinking => ControlState::Blinker,
            _ => sg.base,
        };
        if state != sg.state {
            let from = sg.state;
            sg.state = state;
            sg.since = t;
            // The plan in force restarts its display.
            sg.shown = None;
            self.control_log.push(ControlChange {
                t,
                junction: j,
                from,
                to: state,
                cause,
            });
            self.apply_mode(s, state, net);
        }
    }

    /// Whether vehicles on ribbons `a` and `b` can be in each other's way.
    #[inline]
    pub fn related(&self, a: usize, b: usize) -> bool {
        self.related[a * self.n + b]
    }

    /// The groups a vehicle at station `s` of ribbon `r` belongs to.
    #[inline]
    pub fn groups(&self, r: usize, s: f64) -> impl Iterator<Item = usize> + '_ {
        self.zones[r]
            .iter()
            .filter(move |&&(_, from, to)| s >= from && s <= to)
            .map(|&(g, ..)| g as usize)
    }

    /// The part of the arriving ribbon's width from which a vehicle that
    /// arrives at `junction` by `arm` and leaves by `out` makes its movement.
    #[inline]
    pub fn window_of(
        &self,
        net: &Network,
        junction: usize,
        arm: usize,
        out: usize,
    ) -> Option<(f64, f64)> {
        net.junctions[junction].movement_of[arm][out].and_then(|m| self.window[junction][m])
    }

    /// Whether that window binds (a split, a U-turn) or is only where
    /// drivers place themselves for a turn.
    #[inline]
    pub fn hard_window(&self, net: &Network, junction: usize, arm: usize, out: usize) -> bool {
        net.junctions[junction].movement_of[arm][out].is_some_and(|m| self.hard[junction][m])
    }

    /// Bring the signals to time `t`, list the vehicles near each junction
    /// and decide who goes first where their paths meet. `on_change` is
    /// called for every signal that changed; at the start of amber, vehicles
    /// too close to stop are let through (`gate_open`).
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        t: f64,
        a: &mut AgentStore,
        orders: &[Vec<u32>],
        net: &Network,
        p: &CoreParams,
        seed: u64,
        prev: &[Option<usize>],
        mut on_change: impl FnMut(SignalChange),
    ) {
        if !self.enabled {
            return;
        }
        self.amber_options.clear();
        for s in 0..self.signals.len() {
            self.update_state(s, t, net, p, seed);
        }
        self.requests.clear();
        let amber_go = p.violations.active(crate::violations::Kind::AmberGo);
        for sg in &mut self.signals {
            let j = &net.junctions[sg.junction];
            let Control::Signal {
                phases: fixed,
                cycle_s: fixed_cycle,
                offset_s,
                ..
            } = &j.control
            else {
                continue;
            };
            let (phases, cycle_s, offset) = match sg.state {
                ControlState::Fixed => (fixed.as_slice(), *fixed_cycle, *offset_s),
                ControlState::Police => {
                    let (ph, c) = sg.police.as_ref().expect("checked at start-up");
                    (ph.as_slice(), *c, -sg.since)
                }
                ControlState::Dark | ControlState::Blinker => {
                    // The lights are off: the junction is uncontrolled.
                    if sg.shown.take().is_some() || self.light[j.movements[0].first] != LIGHT_NONE {
                        for m in &j.movements {
                            self.light[m.first] = LIGHT_NONE;
                            self.amber_start[m.first] = f64::NAN;
                            self.red_start[m.first] = f64::NAN;
                            self.next_green[m.first] = f64::NAN;
                            self.change_before_green[m.first] = f64::NAN;
                        }
                        for (arm, cyc) in sg.arms.iter_mut().enumerate() {
                            if cyc.open {
                                self.cycles.push(close_cycle(cyc, j, arm, net, t));
                                cyc.open = false;
                            }
                        }
                    }
                    continue;
                }
            };
            let pos = (t + offset).rem_euclid(cycle_s);
            let mut into = pos;
            let mut now = (phases.len() - 1, 2u8);
            for (k, ph) in phases.iter().enumerate() {
                let total = ph.green_s + ph.amber_s + ph.all_red_s;
                if into < total {
                    now = (
                        k,
                        if into < ph.green_s {
                            0
                        } else if into < ph.green_s + ph.amber_s {
                            1
                        } else {
                            2
                        },
                    );
                    break;
                }
                into -= total;
            }
            if sg.shown == Some(now) {
                continue;
            }
            sg.shown = Some(now);
            let ph = &phases[now.0];
            let (light, state, lasts) = match now.1 {
                0 => (LIGHT_GREEN, "green", ph.green_s),
                1 => (LIGHT_AMBER, "amber", ph.amber_s),
                _ => (LIGHT_RED, "all_red", ph.all_red_s),
            };
            for m in &j.movements {
                let r = m.first;
                let old = self.light[r];
                let new = if ph.green.contains(&r) {
                    light
                } else {
                    LIGHT_RED
                };
                self.light[r] = new;
                if new == LIGHT_AMBER && old != LIGHT_AMBER {
                    self.amber_start[r] = t;
                }
                if new == LIGHT_RED && old != LIGHT_RED {
                    self.red_start[r] = t;
                }
                if new == LIGHT_GREEN {
                    self.next_green[r] = f64::NAN;
                    self.change_before_green[r] = f64::NAN;
                } else if let Some((dt, change)) = next_green(phases, cycle_s, pos, r) {
                    self.next_green[r] = t + dt;
                    self.change_before_green[r] = change;
                }
            }
            on_change(SignalChange {
                junction: sg.junction,
                phase: now.0,
                state,
                lasts,
                control: sg.state,
            });
            for &arm in &ph.arms {
                let Some(rin) = j.arms[arm].inbound else {
                    continue;
                };
                let len = net.ribbon(rin).length();
                match now.1 {
                    0 => {
                        // A new cycle for this arm: close the last one.
                        let cyc = &mut sg.arms[arm];
                        if cyc.open {
                            self.cycles.push(close_cycle(cyc, j, arm, net, t));
                        }
                        let queue = orders[rin]
                            .iter()
                            .filter(|&&i| a.queued[i as usize])
                            .count() as u32;
                        *cyc = ArmCycle {
                            open: true,
                            cycle: cyc.cycle + u32::from(cyc.open || cyc.started),
                            phase: now.0,
                            t_green: t,
                            green_s: ph.green_s,
                            amber_s: ph.amber_s,
                            queue_at_green: queue,
                            crossings: Vec::new(),
                            control: sg.state,
                            red_bins: [0; RED_BINS],
                            started: true,
                        };
                    }
                    1 => {
                        // Amber: a vehicle that cannot stop before the
                        // line even at the safe braking limit goes on; it
                        // reaches the line within v / 2b_safe, before red.
                        // One that can stop and would reach the line within
                        // the amber decides whether to go (`amber_go`).
                        for &i in &orders[rin] {
                            let i = i as usize;
                            let gap = len - a.s[i] - 0.5 * a.length[i];
                            let first = net.ribbon(rin).branch[usize::from(a.turn_out[i])];
                            let green_for_it = first.is_some_and(|f| ph.green.contains(&f));
                            if !green_for_it {
                                continue;
                            }
                            if gap <= p.lookahead {
                                a.saw_amber[i] = t;
                            }
                            if a.v[i] < p.stop_speed_below {
                                continue;
                            }
                            if gap <= 0.0 || a.v[i] * a.v[i] > 2.0 * p.safe_decel * gap {
                                a.gate_open[i] = true;
                                continue;
                            }
                            // One that can stop does, whatever let it on before:
                            // a gap taken upstream for a wait inside the junction
                            // does not open a line that is turning red.
                            a.gate_open[i] = false;
                            if amber_go && gap <= a.v[i] * ph.amber_s {
                                let f = first.expect("green for it");
                                self.amber_options.push(AmberOption {
                                    i: i as u32,
                                    junction: sg.junction as u32,
                                    first: f as u32,
                                    gap,
                                    amber_s: ph.amber_s,
                                    red_s: self.next_green[f] - t - ph.amber_s,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        self.sweeps(a, orders, net);
        if self.first_come {
            for (j, w) in self.waiting_at_lines.iter_mut().enumerate() {
                w.clear();
                if self.mode[j] != MODE_MUTUAL {
                    continue;
                }
                for arm in &net.junctions[j].arms {
                    let Some(rin) = arm.inbound else { continue };
                    for &i in &orders[rin] {
                        let i = i as usize;
                        if a.line_arrival[i].is_nan() || a.turn_junction[i] as usize != j {
                            continue;
                        }
                        if let Some(f) = net.ribbon(rin).branch[usize::from(a.turn_out[i])] {
                            w.push((a.line_arrival[i], a.id[i], f as u32));
                        }
                    }
                }
            }
        }
        // Vehicles near each junction, in world coordinates.
        for m in &mut self.members {
            m.clear();
        }
        for (r, order) in orders.iter().enumerate() {
            if self.zones[r].is_empty() {
                continue;
            }
            for &i in order {
                let (s, y) = (a.s[i as usize], a.y[i as usize]);
                let mut pose = None;
                for &(g, from, to) in &self.zones[r] {
                    if s < from || s > to {
                        continue;
                    }
                    let (x, yw, hx, hy) = *pose.get_or_insert_with(|| {
                        chord_pose(
                            net,
                            seed,
                            prev,
                            a,
                            i as usize,
                            s,
                            y,
                            a.heading_tan[i as usize],
                        )
                    });
                    self.members[g as usize].push(Member {
                        i,
                        ribbon: r as u32,
                        x,
                        y: yw,
                        hx,
                        hy,
                    });
                }
            }
        }
        self.arbiter.run(
            a,
            &self.members,
            net,
            p,
            seed,
            &self.light,
            &self.rank,
            &self.mode,
        );
    }

    /// Each vehicle's sweep for the turn it is about to make or is making
    /// (`AgentStore::sweep`): from when its front is within its own length
    /// of its line until it leaves the junction's curved ribbons.
    fn sweeps(&self, a: &mut AgentStore, orders: &[Vec<u32>], net: &Network) {
        for (r, order) in orders.iter().enumerate() {
            let rib = net.ribbon(r);
            let own = self.sweep_coef[r];
            let line = self.line[r];
            if own == 0.0 && line.is_none() {
                for &i in order {
                    a.sweep[i as usize] = 0.0;
                }
                continue;
            }
            for &i in order {
                let i = i as usize;
                let l = a.length[i];
                let coef = if rib.internal {
                    own
                } else {
                    match line {
                        Some((lr, dist))
                            if dist - a.s[i] - 0.5 * l <= l
                                && net.ribbon(lr as usize).junction
                                    == Some(a.turn_junction[i] as usize) =>
                        {
                            net.ribbon(lr as usize)
                                .branch
                                .get(usize::from(a.turn_out[i]))
                                .copied()
                                .flatten()
                                .map_or(0.0, |f| self.sweep_coef[f])
                        }
                        _ => 0.0,
                    }
                };
                a.sweep[i] = l * l * coef;
            }
        }
    }

    /// Whether, under first come first served, a vehicle (`id`, at its line
    /// since `since`; NaN: not yet) into `first` at junction `j` must let a
    /// vehicle waiting at a conflicting line go first: one that got there
    /// earlier (ties by id).
    pub fn earlier_at_lines(
        &self,
        net: &Network,
        j: usize,
        first: usize,
        since: f64,
        id: u64,
    ) -> bool {
        let mine = if since.is_nan() { f64::INFINITY } else { since };
        let jn = &net.junctions[j];
        self.waiting_at_lines[j].iter().any(|&(at, other, f)| {
            other != id
                && (at < mine || (at == mine && other < id))
                && jn.conflicts_of(first).any(|(c, _, _, kind)| {
                    c == f as usize && kind != marg_net::ConflictKind::Diverge
                })
        })
    }

    /// A vehicle crossed the stop line of `arm` at `junction` at time `t`,
    /// into the movement that starts on ribbon `first`. Returns, if it
    /// crossed on red, the time since its red began and the red's length.
    /// A vehicle creeping past its line (`seeping`) is not counted among
    /// the red crossings of the cycle.
    #[allow(clippy::too_many_arguments)]
    pub fn crossed(
        &mut self,
        junction: usize,
        arm: usize,
        first: usize,
        t: f64,
        queued: bool,
        seeping: bool,
    ) -> Option<(f64, f64)> {
        let on_red = self.light[first] == LIGHT_RED;
        let red = on_red.then(|| {
            (
                t - self.red_start[first],
                self.next_green[first] - self.red_start[first],
            )
        });
        if let Some(sg) = self.signals.iter_mut().find(|s| s.junction == junction) {
            let cyc = &mut sg.arms[arm];
            if cyc.open {
                cyc.crossings.push((t, queued, on_red));
                if let Some((into, length)) = red
                    && length > 0.0
                    && !seeping
                {
                    let bin = ((RED_BINS as f64 * into / length) as usize).min(RED_BINS - 1);
                    cyc.red_bins[bin] += 1;
                }
            }
        }
        red
    }

    /// Whether `junction` has a signal, and its countdown display.
    pub fn countdown(net: &Network, junction: usize) -> Option<CountdownState> {
        match &net.junctions[junction].control {
            Control::Signal { countdown, .. } => Some(*countdown),
            Control::Uncontrolled => None,
        }
    }

    /// Close the cycles still being measured (the end of the run).
    pub fn finish(&mut self, net: &Network, t: f64) {
        for sg in &mut self.signals {
            let j = &net.junctions[sg.junction];
            for (arm, cyc) in sg.arms.iter_mut().enumerate() {
                if cyc.open {
                    self.cycles.push(close_cycle(cyc, j, arm, net, t));
                    cyc.open = false;
                }
            }
        }
    }
}

fn close_cycle(
    cyc: &ArmCycle,
    j: &marg_net::Junction,
    arm: usize,
    net: &Network,
    t_end: f64,
) -> SignalCycleRecord {
    let rin = j.arms[arm]
        .inbound
        .expect("a measured arm has an arriving ribbon");
    let (lo, hi) = net.ribbon(rin).y_bounds(net.ribbon(rin).length());
    // Saturation flow: the queued vehicles after the first few, over the
    // time from the last of those few to the last queued vehicle, of those
    // that crossed while the movement was not red (a red-light runner or a
    // vehicle that crept over the line belongs to no discharge).
    let all_queued = cyc.crossings.iter().filter(|c| c.1).count();
    let queued: Vec<f64> = cyc
        .crossings
        .iter()
        .filter(|c| c.1 && !c.2)
        .map(|c| c.0)
        .collect();
    let (mut headway, mut flow, mut n_sat) = (f64::NAN, f64::NAN, 0u32);
    if queued.len() >= SATURATION_SKIP + SATURATION_MIN {
        let span = queued[queued.len() - 1] - queued[SATURATION_SKIP - 1];
        n_sat = (queued.len() - SATURATION_SKIP) as u32;
        if span > 0.0 {
            headway = span / f64::from(n_sat);
            flow = crate::demand::SECONDS_PER_HOUR / headway;
        }
    }
    SignalCycleRecord {
        junction: j.id.clone(),
        arm: j.arms[arm].id.clone(),
        ribbon: net.ribbon(rin).id.clone(),
        cycle: cyc.cycle,
        phase: cyc.phase as u32,
        t_green: cyc.t_green,
        green_s: cyc.green_s,
        amber_s: cyc.amber_s,
        red_s: (t_end - cyc.t_green - cyc.green_s - cyc.amber_s).max(0.0),
        queue_at_green: cyc.queue_at_green,
        discharged: cyc.crossings.len() as u32,
        discharged_from_queue: all_queued as u32,
        saturation_vehicles: n_sat,
        saturation_headway: headway,
        saturation_flow: flow,
        approach_width: hi - lo,
        saturation_flow_per_m: flow / (hi - lo),
        control: cyc.control.key().to_string(),
        countdown: match &j.control {
            Control::Signal { countdown, .. } => countdown.key(),
            Control::Uncontrolled => "none",
        }
        .to_string(),
        red_crossings_early: cyc.red_bins[0],
        red_crossings_middle: cyc.red_bins[1],
        red_crossings_late: cyc.red_bins[2],
    }
}

/// Seconds from cycle position `pos` to the next start of a stage that gives
/// the movement on ribbon `r` green, and the change interval (amber and
/// all-red) of the stage just before it.
fn next_green(phases: &[Phase], cycle: f64, pos: f64, r: usize) -> Option<(f64, f64)> {
    let n = phases.len();
    let mut start = 0.0;
    for k in 0..2 * n {
        let ph = &phases[k % n];
        if start > pos && ph.green.contains(&r) {
            let before = &phases[(k + n - 1) % n];
            return Some((start - pos, before.amber_s + before.all_red_s));
        }
        start += ph.green_s + ph.amber_s + ph.all_red_s;
        if k + 1 == n {
            start = cycle;
        }
    }
    None
}

/// The time to junction `j`'s outage after the `n`-th (s): exponential at
/// `rate` per second.
fn outage_gap(seed: u64, j: usize, n: u64, rate: f64) -> f64 {
    let u = StreamRng::at(seed, StreamKind::Environment, j as u64, OUTAGE_DRAWS * n).uniform();
    -libm::log1p(-u) / rate
}

/// A traffic officer's plan (G§6.6, ADR-0019): the fixed plan's stages in
/// order with their amber and all-red, over a cycle of `cycle` s. The green
/// is shared out as in the fixed plan, then a share `bias` of the minor
/// stages' green moves to the stages that serve a major arm (`major`, or
/// the arms of the stage with the longest green).
fn police_plan(
    j: &marg_net::Junction,
    phases: &[Phase],
    major: &[usize],
    cycle: f64,
    bias: f64,
) -> Result<(Vec<Phase>, f64), String> {
    let lost: f64 = phases.iter().map(|p| p.amber_s + p.all_red_s).sum();
    let green = cycle - lost;
    if green <= 0.0 {
        return Err(format!(
            "junction `{}`: the officer's cycle ({cycle} s) leaves no green after the amber and all-red ({lost} s)",
            j.id
        ));
    }
    if !(0.0..1.0).contains(&bias) {
        return Err(format!(
            "junction `{}`: `road.signal.officer.major_bias` must be in [0, 1) (1 leaves the minor approaches no green)",
            j.id
        ));
    }
    let longest = phases
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.green_s.total_cmp(&b.1.green_s).then(b.0.cmp(&a.0)))
        .map(|(k, _)| k)
        .unwrap_or(0);
    let major: Vec<usize> = if major.is_empty() {
        phases[longest].arms.clone()
    } else {
        major.to_vec()
    };
    let is_major: Vec<bool> = phases
        .iter()
        .map(|p| p.arms.iter().any(|a| major.contains(a)))
        .collect();
    let total: f64 = phases.iter().map(|p| p.green_s).sum();
    let major_fixed: f64 = phases
        .iter()
        .zip(&is_major)
        .filter(|(_, m)| **m)
        .map(|(p, _)| p.green_s)
        .sum();
    let share_fixed = major_fixed / total;
    let share = if is_major.iter().all(|m| *m) {
        1.0
    } else {
        share_fixed + bias * (1.0 - share_fixed)
    };
    let built = phases
        .iter()
        .zip(&is_major)
        .map(|(p, &m)| {
            let part = if m {
                share * p.green_s / major_fixed
            } else {
                (1.0 - share) * p.green_s / (total - major_fixed)
            };
            Phase {
                green_s: green * part,
                ..p.clone()
            }
        })
        .collect();
    Ok((built, cycle))
}

/// Per movement ribbon: where the first area it shares with a movement
/// from another arm (crossing or merging) starts on it (NaN: none).
fn seep_holds(net: &Network) -> Vec<f64> {
    let mut hold = vec![f64::NAN; net.len()];
    for j in &net.junctions {
        let arm_of = |r: usize| j.movements.iter().find(|m| m.first == r).map(|m| m.from);
        for m in &j.movements {
            let start = j
                .conflicts_of(m.first)
                .filter(|c| c.3 != marg_net::ConflictKind::Diverge && arm_of(c.0) != Some(m.from))
                .map(|c| c.1.0)
                .fold(f64::INFINITY, f64::min);
            if start.is_finite() {
                hold[m.first] = start;
            }
        }
    }
    hold
}

/// Per ribbon inside a junction: `±1 / 8ρ` towards the inside of its turn
/// (0 for straight ribbons and ribbons outside junctions).
fn sweep_coefs(net: &Network) -> Vec<f64> {
    (0..net.len())
        .map(|r| {
            let rib = net.ribbon(r);
            if !rib.internal || !rib.min_radius.is_finite() || rib.min_radius <= 0.0 {
                return 0.0;
            }
            // Which way it turns: the sum of the turns at its vertices.
            let pts = rib.points();
            let mut turn = 0.0;
            for w in pts.windows(3) {
                let (ax, ay) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
                let (bx, by) = (w[2][0] - w[1][0], w[2][1] - w[1][1]);
                turn += libm::atan2(ax * by - ay * bx, ax * bx + ay * by);
            }
            if turn == 0.0 {
                0.0
            } else {
                turn.signum() / (8.0 * rib.min_radius)
            }
        })
        .collect()
}
