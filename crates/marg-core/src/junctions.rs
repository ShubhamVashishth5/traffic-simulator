//! Junctions at run time (plan/phases/P07-junctions-splits.md, ADR-0016):
//! where each vehicle goes, what the signals show, which vehicles are near
//! each junction (for interaction across ribbons), and the record of every
//! signal cycle.
//!
//! Nothing here scales capacity or flow (ADR-0005). A signal closes a stop
//! line; a curve caps speed; a priority rule makes a vehicle wait for a gap.

use crate::agents::AgentStore;
use crate::output::SignalCycleRecord;
use crate::params::CoreParams;
use crate::rng::{StreamKind, StreamRng};
use marg_net::{Control, Network, Turn};
use marg_params::spec::{CountdownState, JunctionKind};

/// What the signal shows the movement that starts on a ribbon.
pub const LIGHT_NONE: u8 = 0;
pub const LIGHT_GREEN: u8 = 1;
pub const LIGHT_AMBER: u8 = 2;
pub const LIGHT_RED: u8 = 3;

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
    crossings: Vec<(f64, bool)>,
}

#[derive(Debug, Clone)]
struct Signal {
    junction: usize,
    /// Stage shown at the last step: `(phase, 0 green | 1 amber | 2 all red)`.
    shown: Option<(usize, u8)>,
    arms: Vec<ArmCycle>,
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
}

impl Junctions {
    pub fn new(net: &Network, p: &CoreParams) -> Result<Self, String> {
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
        let signals = net
            .junctions
            .iter()
            .enumerate()
            .filter(|(_, j)| matches!(j.control, Control::Signal { .. }))
            .map(|(k, j)| Signal {
                junction: k,
                shown: None,
                arms: vec![ArmCycle::default(); j.arms.len()],
            })
            .collect();
        Ok(Self {
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
        })
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

    /// Bring the signals to time `t` and list the vehicles near each
    /// junction. `on_change` is called for every signal that changed; at the
    /// start of amber, vehicles too close to stop are let through
    /// (`gate_open`).
    pub fn step(
        &mut self,
        t: f64,
        a: &mut AgentStore,
        orders: &[Vec<u32>],
        net: &Network,
        p: &CoreParams,
        mut on_change: impl FnMut(SignalChange),
    ) {
        if !self.enabled {
            return;
        }
        for sg in &mut self.signals {
            let j = &net.junctions[sg.junction];
            let Control::Signal {
                phases,
                cycle_s,
                offset_s,
                ..
            } = &j.control
            else {
                continue;
            };
            let mut into = (t + offset_s).rem_euclid(*cycle_s);
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
            for m in &j.movements {
                self.light[m.first] = LIGHT_RED;
            }
            let (light, state, lasts) = match now.1 {
                0 => (LIGHT_GREEN, "green", ph.green_s),
                1 => (LIGHT_AMBER, "amber", ph.amber_s),
                _ => (LIGHT_RED, "all_red", ph.all_red_s),
            };
            for &r in &ph.green {
                self.light[r] = light;
            }
            on_change(SignalChange {
                junction: sg.junction,
                phase: now.0,
                state,
                lasts,
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
                            cycle: cyc.cycle + u32::from(cyc.open),
                            phase: now.0,
                            t_green: t,
                            green_s: ph.green_s,
                            amber_s: ph.amber_s,
                            queue_at_green: queue,
                            crossings: Vec::new(),
                        };
                    }
                    1 => {
                        // Amber: a vehicle that cannot stop before the
                        // line at its comfortable deceleration goes on.
                        for &i in &orders[rin] {
                            let i = i as usize;
                            let gap = len - a.s[i] - 0.5 * a.length[i];
                            let green_for_it = ph.green.contains(
                                &net.ribbon(rin).branch[usize::from(a.turn_out[i])]
                                    .unwrap_or(usize::MAX),
                            );
                            if green_for_it
                                && (gap <= 0.0
                                    || a.v[i] * a.v[i] > 2.0 * a.comfortable_decel[i] * gap)
                                && a.v[i] >= p.stop_speed_below
                            {
                                a.gate_open[i] = true;
                            }
                        }
                    }
                    _ => {}
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
            let rib = net.ribbon(r);
            for &i in order {
                let (s, y) = (a.s[i as usize], a.y[i as usize]);
                let mut pose = None;
                for &(g, from, to) in &self.zones[r] {
                    if s < from || s > to {
                        continue;
                    }
                    let (x, yw, hx, hy) = *pose.get_or_insert_with(|| {
                        let (x, yw, _) = rib.to_world(s, y);
                        let d = rib.direction(s);
                        // The body's own heading: the ribbon's, turned by
                        // the vehicle's heading on it.
                        let tan = a.heading_tan[i as usize];
                        let cos = 1.0 / (1.0 + tan * tan).sqrt();
                        let sin = tan * cos;
                        (x, yw, d[0] * cos - d[1] * sin, d[1] * cos + d[0] * sin)
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
    }

    /// A vehicle crossed the stop line of `arm` at `junction` at time `t`.
    pub fn crossed(&mut self, junction: usize, arm: usize, t: f64, queued: bool) {
        if let Some(sg) = self.signals.iter_mut().find(|s| s.junction == junction) {
            let cyc = &mut sg.arms[arm];
            if cyc.open {
                cyc.crossings.push((t, queued));
            }
        }
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
    // time from the last of those few to the last queued vehicle.
    let queued: Vec<f64> = cyc.crossings.iter().filter(|c| c.1).map(|c| c.0).collect();
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
        discharged_from_queue: queued.len() as u32,
        saturation_vehicles: n_sat,
        saturation_headway: headway,
        saturation_flow: flow,
        approach_width: hi - lo,
        saturation_flow_per_m: flow / (hi - lo),
    }
}
