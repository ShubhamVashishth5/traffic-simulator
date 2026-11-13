//! The part of the move phase that junctions add (ADR-0016, ADR-0017):
//! the stop line ahead (a signal, a gap to wait for, a split to be on the
//! right side of), the speed a curve ahead allows, and vehicles on other
//! ribbons that are in the way.
//!
//! Like the rest of the move phase this reads the snapshot and writes
//! nothing; what it decides is returned and applied afterwards in a fixed
//! order.

use crate::agents::{NO_AGENT, NO_RIBBON, SPLIT_DECIDED, SPLIT_GAVE_UP, SPLIT_NONE, SPLIT_WAIT};
use crate::junctions::{LIGHT_AMBER, LIGHT_GREEN, LIGHT_NONE, LIGHT_RED, chord_pose, onward};
use crate::model::lateral::{edge_gap, leader_weight, steering_speed};
use crate::model::longitudinal::Idm;
use crate::rng::{StreamKind, StreamRng};
use crate::scan::{MoveCtx, approach};
use marg_net::{ConflictKind, Turn};
use marg_params::VehicleClass;
use marg_params::spec::JunctionKind;

/// Counter blocks of the per-agent decisions made here (`StreamKind::Decisions`).
const SPLIT_BASE: u64 = 3 << 40;
const RING_BASE: u64 = 4 << 40;

/// Ribbons of an agent's own path that other vehicles are projected onto.
const CHAIN: usize = 6;
/// Ribbons upstream of a conflict area searched for vehicles coming to it.
const UPSTREAM: usize = 8;

/// What the junction part of the move phase decided for one agent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JunctionOut {
    /// The gap it looked at this step (s; NaN: none), the vehicle that
    /// ends it, that vehicle's class, and whether it took the gap.
    pub gap: f64,
    pub gap_ref: u64,
    pub gap_class: u8,
    pub accepted: bool,
    /// The critical gap it compared with (s).
    pub needed: f64,
    /// A closed stop line ahead holds it.
    pub held: bool,
    /// It knows which branch it needs.
    pub aware: bool,
    /// Decided at a split this step: one of `SPLIT_*` (`SPLIT_NONE`: nothing).
    pub split: u8,
    /// The probability of stopping to wait that the decision used, the
    /// urgency and the lateral move still needed (m).
    pub split_p: f64,
    pub urgency: f64,
    pub needed_move: f64,
    /// It waits inside the junction for a gap in a priority stream.
    pub waiting: bool,
}

impl JunctionOut {
    pub const NONE: Self = Self {
        gap: f64::NAN,
        gap_ref: NO_AGENT,
        gap_class: 0,
        accepted: false,
        needed: f64::NAN,
        held: false,
        aware: false,
        split: SPLIT_NONE,
        split_p: f64::NAN,
        urgency: 0.0,
        needed_move: 0.0,
        waiting: false,
    };
}

/// The pull of a branch on the lateral choice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Route {
    /// Where the vehicle's centre must be.
    pub lo: f64,
    pub hi: f64,
    /// Utility per metre nearer (m/s² per m), urgency applied.
    pub weight: f64,
    /// Factor on the clearance kept to side neighbours, urgency applied.
    pub squeeze: f64,
}

/// What the stop line ahead means for this step.
#[derive(Debug, Clone, Copy)]
pub struct LineView {
    /// Acceleration the line allows (infinite: it is open).
    pub cap: f64,
    pub route: Option<Route>,
    /// A lateral position to make for directly.
    pub track: Option<f64>,
    pub out: JunctionOut,
}

/// Time to cover `d` m from speed `v` at constant acceleration `a`.
#[inline]
pub(crate) fn arrive(v: f64, a: f64, d: f64) -> f64 {
    if d <= 0.0 {
        0.0
    } else {
        (-v + (v * v + 2.0 * a * d).sqrt()) / a
    }
}

/// Whether a vehicle of this class hides a sign from the driver behind it,
/// and counts as heavy for gap acceptance (G§2.4: buses, trucks, tractors).
#[inline]
fn is_heavy(class: usize) -> bool {
    class == VehicleClass::Bus.index()
        || class == VehicleClass::Truck.index()
        || class == VehicleClass::Tractor.index()
}

/// The class of the vehicle with this id, if it is a vehicle's id.
#[inline]
fn class_of_id(id: u64) -> Option<usize> {
    let c = ((id >> 40) & 0xff) as usize;
    (id >> 48 < 0xffff && c < VehicleClass::ALL.len()).then_some(c)
}

impl MoveCtx<'_> {
    /// The ribbon behind `ribbon` for agent `i` on ribbon `r`: the only one
    /// that leads onto it, or the one the agent came by.
    #[inline]
    pub(crate) fn back(&self, i: usize, r: usize, ribbon: usize) -> Option<usize> {
        match self.prev[ribbon] {
            Some(q) => Some(q),
            None if ribbon == r && self.a.came_from[i] != NO_RIBBON => {
                Some(self.a.came_from[i] as usize)
            }
            None => None,
        }
    }

    /// The stop line ahead of agent `i` and what follows from it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn line_ahead(
        &self,
        i: usize,
        r: usize,
        idm: &Idm,
        free: f64,
        leader: u64,
        acc: &mut f64,
    ) -> LineView {
        let (a, p, net) = (self.a, self.p, self.net);
        let mut view = LineView {
            cap: f64::INFINITY,
            route: None,
            track: None,
            out: JunctionOut::NONE,
        };
        let (v, s, y, l) = (a.v[i], a.s[i], a.y[i], a.length[i]);
        // The speed the curves ahead allow.
        if self.junctions.any_caps {
            let mut turn = (a.turn_junction[i], a.turn_out[i]);
            let (mut ribbon, mut dist) = (r, -s);
            for _ in 0..CHAIN {
                dist += net.ribbon(ribbon).length();
                if dist > p.lookahead {
                    break;
                }
                let Some(n) = onward(net, self.seed, a.id[i], ribbon, &mut turn) else {
                    break;
                };
                let cap = self.junctions.speed_cap[n];
                if cap < idm.v0 {
                    *acc = acc.min(approach(idm, v, cap, dist - 0.5 * l).max(-p.emergency_decel));
                }
                ribbon = n;
            }
        }
        // A vehicle inside a junction that has somewhere to be across it.
        if a.land_y[i].is_finite() {
            view.track = Some(a.land_y[i]);
        }
        let Some((line_ribbon, dist)) = self.junctions.line[r] else {
            return view;
        };
        let d_line = dist - s;
        let gap = d_line - 0.5 * l;
        if gap > p.lookahead || a.turn_junction[i] == NO_RIBBON {
            return view;
        }
        let rib = net.ribbon(line_ribbon as usize);
        let (Some(j), Some(arm)) = (rib.junction, rib.arm_in) else {
            return view;
        };
        if a.turn_junction[i] as usize != j {
            return view;
        }
        let out = usize::from(a.turn_out[i]);
        let Some(first) = rib.branch[out] else {
            return view;
        };
        let jn = &net.junctions[j];
        let hw = 0.5 * a.width[i];
        // Being on the right side of the road for the branch (G§6.7).
        if let Some((lo, hi)) = self.junctions.window_of(net, j, arm, out) {
            let centre = if lo + hw <= hi - hw {
                (lo + hw, hi - hw)
            } else {
                (0.5 * (lo + hi), 0.5 * (lo + hi))
            };
            let need = (centre.0 - y).max(y - centre.1).max(0.0);
            let mut aware = a.aware[i] || d_line <= a.aware_distance[i];
            if !aware
                && a.late_realiser[i]
                && let Some(visible) = jn.sign_visibility.or(p.junction.sign_visibility)
            {
                // The sign can be read unless a large vehicle is in front.
                let hidden = class_of_id(leader).is_some_and(is_heavy);
                aware = d_line <= visible && !hidden;
            }
            view.out.aware = aware;
            let hard = self.junctions.hard_window(net, j, arm, out);
            let state = a.split_state[i];
            if aware && need > 0.0 && state != SPLIT_GAVE_UP {
                let u = need / gap.max(a.standstill_gap[i]);
                let urgency = (u / p.max_heading_tan).min(1.0);
                view.out.urgency = urgency;
                view.out.needed_move = need;
                view.route = Some(Route {
                    lo: centre.0,
                    hi: centre.1,
                    weight: p.junction.route_weight.unwrap_or(0.0) * urgency,
                    squeeze: libm::exp(-p.junction.urgency_link.unwrap_or(0.0) * urgency),
                });
            }
            // The deadline: where it could still just stop before the line.
            let stopping = v * v / (2.0 * idm.comfortable_decel) + a.standstill_gap[i];
            if hard && state == SPLIT_NONE && gap <= stopping {
                if need <= 0.0 {
                    view.out.split = SPLIT_DECIDED;
                } else if aware {
                    let p_wait = p.junction.stop_and_wait.unwrap_or(0.0);
                    let draw = StreamRng::at(
                        self.seed,
                        StreamKind::Decisions,
                        a.id[i],
                        SPLIT_BASE + j as u64,
                    )
                    .uniform();
                    view.out.split = if draw < p_wait {
                        SPLIT_WAIT
                    } else {
                        SPLIT_GAVE_UP
                    };
                    view.out.split_p = p_wait;
                }
            }
            let waiting = state == SPLIT_WAIT || view.out.split == SPLIT_WAIT;
            if waiting && need > 0.0 {
                // It stands before the line until it has room to move over.
                view.track = Some(y.clamp(centre.0, centre.1));
                view.route = None;
                if gap > 0.0 {
                    view.cap =
                        (free - idm.interaction(v, gap, v, 1.0, 1.0)).max(-p.emergency_decel);
                }
                view.out.held = true;
            }
        }
        if gap <= 0.0 {
            return view;
        }
        // Keep clear: not onto an area a conflicting movement is still in,
        // if it can still stop before the line at the safe braking limit.
        // One that cannot goes on, and who goes first inside is decided by
        // the junction's arbitration (ADR-0016 part 4).
        let can_stop = v * v <= 2.0 * p.safe_decel * gap;
        if can_stop && self.occupied(i, j, first) {
            view.cap = view
                .cap
                .min((free - idm.interaction(v, gap, v, 1.0, 1.0)).max(-p.emergency_decel));
            view.out.held = true;
            return view;
        }
        // It took a gap or could not stop at amber, and is still on its
        // way: a vehicle standing at a red light is not.
        let light = self.junctions.light[first];
        let shut = light == LIGHT_RED || light == LIGHT_AMBER;
        if a.gate_open[i] && (!shut || v >= p.stop_speed_below) {
            return view;
        }
        // The signal.
        let mut closed = light == LIGHT_RED || light == LIGHT_AMBER;
        // A lower-priority movement waits for a gap (G§2.4).
        let rank = net.ribbon(first).rank;
        if !closed && rank > 0 && self.yields(i, j, first, light) {
            let decide_within = idm.desired_gap(v, v, 1.0, 1.0);
            if gap <= decide_within {
                let movement = jn.movement_of[arm][out].map(|m| jn.movements[m].turn);
                let factor = match (jn.kind, movement) {
                    (JunctionKind::Roundabout, _) => p.junction.ring_gap_factor.unwrap_or(1.0),
                    (_, Some(Turn::UTurn)) => p.junction.u_turn_gap_factor.unwrap_or(1.0),
                    _ => 1.0,
                };
                let t_me = arrive(v, idm.max_accel, gap);
                let offer = self.gap_offer(i, j, first, light, factor, t_me);
                view.out.gap = offer.0;
                view.out.gap_ref = offer.1;
                view.out.gap_class = offer.2;
                view.out.needed = offer.3;
                view.out.accepted = offer.4;
                closed = !offer.4;
            } else {
                closed = true;
            }
        }
        if closed {
            view.cap = view
                .cap
                .min((free - idm.interaction(v, gap, v, 1.0, 1.0)).max(-p.emergency_decel));
            view.out.held = true;
        }
        view
    }

    /// Whether agent `i` gives way at the line into `first` at all. Under a
    /// signal it does not: a permitted movement waits for its gap inside
    /// the junction ([`Self::box_gap`]).
    fn yields(&self, i: usize, j: usize, first: usize, light: u8) -> bool {
        let (net, p) = (self.net, self.p);
        if light != LIGHT_NONE {
            return false;
        }
        let jn = &net.junctions[j];
        let rank = net.ribbon(first).rank;
        let any = jn.conflicts_of(first).any(|(c, _, _, kind)| {
            kind != ConflictKind::Diverge
                && net.ribbon(c).rank < rank
                && (light == LIGHT_NONE || self.junctions.light[c] == LIGHT_GREEN)
        });
        if !any {
            return false;
        }
        if jn.kind == JunctionKind::Roundabout {
            // A share of entrants do not wait for a gap (ADR-0009).
            let draw = StreamRng::at(
                self.seed,
                StreamKind::Decisions,
                self.a.id[i],
                RING_BASE + j as u64,
            )
            .uniform();
            return draw < p.junction.ring_compliance.unwrap_or(1.0);
        }
        true
    }

    /// Whether a vehicle of a crossing movement is in the area it shares
    /// with `first` (ADR-0016). A movement the signal has stopped counts
    /// from its stop line on: its last vehicles are let out of the junction.
    /// Under a signal only such movements count: between movements that
    /// have green together, who goes first is decided inside the junction.
    fn occupied(&self, i: usize, j: usize, first: usize) -> bool {
        let a = self.a;
        let signal = self.junctions.light[first] != LIGHT_NONE;
        self.net.junctions[j]
            .conflicts_of(first)
            .any(|(c, _, c_s, kind)| {
                let light = self.junctions.light[c];
                let leaving = light == LIGHT_RED || light == LIGHT_AMBER;
                kind == ConflictKind::Crossing
                    && !(signal && light == LIGHT_GREEN)
                    && self.orders[c].iter().any(|&k| {
                        let k = k as usize;
                        k != i
                            && a.s[k] - 0.5 * a.length[k] < c_s.1
                            && (leaving || a.s[k] + 0.5 * a.length[k] > c_s.0)
                    })
            })
    }

    /// The gap agent `i` is offered at the line into `first`: `(gap, the
    /// vehicle that ends it, its class, the gap needed, accepted)`. The gap
    /// is the time between the agent's own arrival at the line (`t_me` from
    /// now) and the arrival of the conflicting vehicle that leaves the
    /// least to spare.
    fn gap_offer(
        &self,
        i: usize,
        j: usize,
        first: usize,
        light: u8,
        factor: f64,
        t_me: f64,
    ) -> (f64, u64, u8, f64, bool) {
        let (a, p, net) = (self.a, self.p, self.net);
        let jn = &net.junctions[j];
        let rank = net.ribbon(first).rank;
        let t_c = a.critical_gap[i] * factor;
        let mut worst = (f64::INFINITY, f64::INFINITY, NO_AGENT, 0u8, t_c);
        for (c, _, c_s, kind) in jn.conflicts_of(first) {
            if kind == ConflictKind::Diverge
                || net.ribbon(c).rank >= rank
                || (light != LIGHT_NONE && self.junctions.light[c] != LIGHT_GREEN)
            {
                continue;
            }
            // Vehicles on the conflicting ribbon and coming to it: walk upstream.
            let mut stack = [(0usize, 0.0f64, usize::MAX); UPSTREAM];
            stack[0] = (c, c_s.0, usize::MAX);
            let mut n = 1;
            while n > 0 {
                n -= 1;
                let (q, start, down) = stack[n];
                // `start`: station of the area's start measured on `q`.
                for &k in self.orders[q].iter().rev() {
                    let k = k as usize;
                    if k == i {
                        continue;
                    }
                    let front = a.s[k] + 0.5 * a.length[k];
                    let d = start - front;
                    if d > p.lookahead {
                        break;
                    }
                    if down != usize::MAX {
                        let mut turn = (a.turn_junction[k], a.turn_out[k]);
                        if onward(net, self.seed, a.id[k], q, &mut turn) != Some(down) {
                            continue;
                        }
                    } else if a.s[k] - 0.5 * a.length[k] > c_s.1 {
                        // Past the area.
                        continue;
                    }
                    let moving = a.v[k] >= p.stop_speed_below;
                    // Standing and held (by a closed line, a vehicle in
                    // front, a gap it waits for itself): not coming.
                    if !moving && a.acc[k] <= 0.0 {
                        continue;
                    }
                    let t = if d <= 0.0 {
                        0.0
                    } else if moving {
                        d / a.v[k]
                    } else {
                        arrive(a.v[k], a.max_accel[k], d)
                    };
                    // Gone before the agent gets there.
                    if moving && (d + (c_s.1 - c_s.0) + a.length[k]) / a.v[k] < t_me {
                        continue;
                    }
                    let class = usize::from(a.class[k]);
                    let needed = if is_heavy(class) {
                        t_c * a.heavy_factor[i]
                    } else {
                        t_c
                    };
                    let offered = t - t_me;
                    if offered - needed < worst.0 {
                        worst = (offered - needed, offered, a.id[k], class as u8, needed);
                    }
                }
                if start < p.lookahead {
                    // Further upstream is within reach too.
                    for &up in &self.junctions_preds(q) {
                        if up == usize::MAX || n == UPSTREAM {
                            continue;
                        }
                        stack[n] = (up, start + net.ribbon(up).length(), q);
                        n += 1;
                    }
                }
            }
        }
        (worst.1, worst.2, worst.3, worst.4, worst.0 >= 0.0)
    }

    /// The gap agent `i` is offered where it waits, `hold` m ahead of its
    /// front, for a gap in the priority streams of its movement under a
    /// signal (ADR-0016): as at a stop line, from its own arrival there.
    pub(crate) fn box_gap(
        &self,
        i: usize,
        r: usize,
        idm: &Idm,
        hold: f64,
    ) -> Option<(f64, u64, u8, f64, bool)> {
        let (a, p, net) = (self.a, self.p, self.net);
        let rib = net.ribbon(r);
        let (j, first) = if rib.internal {
            (rib.junction?, r)
        } else {
            let j = rib.junction?;
            (j, rib.branch.get(usize::from(a.turn_out[i])).copied().flatten()?)
        };
        let jn = &net.junctions[j];
        let movement = jn
            .movements
            .iter()
            .find(|m| m.first == first)
            .map(|m| m.turn);
        let factor = match movement {
            Some(Turn::UTurn) => p.junction.u_turn_gap_factor.unwrap_or(1.0),
            _ => 1.0,
        };
        let t_me = arrive(a.v[i], idm.max_accel, hold.max(0.0));
        Some(self.gap_offer(i, j, first, self.junctions.light[first], factor, t_me))
    }

    /// Up to three ribbons that lead onto `q`.
    fn junctions_preds(&self, q: usize) -> [usize; 3] {
        let mut out = [usize::MAX; 3];
        if let Some(pq) = self.prev[q] {
            out[0] = pq;
            return out;
        }
        if self.net.ribbon(q).preds == 0 {
            return out;
        }
        for (slot, up) in out.iter_mut().zip(self.net.predecessors(q)) {
            *slot = up;
        }
        out
    }

    /// Vehicles on other ribbons that are in agent `i`'s way (ADR-0016):
    /// each is placed on the agent's own path and taken as a leader or a
    /// side neighbour there. Of two vehicles whose paths cross, the one
    /// that gets to the crossing first goes first.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn foreign(
        &self,
        i: usize,
        r: usize,
        idm: &Idm,
        free: f64,
        no_cut_in: f64,
        acc: &mut f64,
        lead: &mut (f64, u64),
        mut side: impl FnMut(bool, f64, f64, f64),
    ) {
        let (a, p, net) = (self.a, self.p, self.net);
        let (s, y, v, l) = (a.s[i], a.y[i], a.v[i], a.length[i]);
        let mut groups = self.junctions.groups(r, s).peekable();
        if groups.peek().is_none() {
            return;
        }
        // The agent's own path: the ribbon it came by, its ribbon, and
        // those ahead within the lookahead, with the station of each start
        // measured from its own ribbon's start.
        let mut chain = [(0usize, 0.0f64); CHAIN];
        let mut n = 0;
        if let Some(q) = self.back(i, r, r) {
            chain[n] = (q, -net.ribbon(q).length());
            n += 1;
        }
        chain[n] = (r, 0.0);
        n += 1;
        let mut turn = (a.turn_junction[i], a.turn_out[i]);
        let (mut ribbon, mut off) = (r, 0.0);
        while n < CHAIN {
            off += net.ribbon(ribbon).length();
            if off - s > p.lookahead {
                break;
            }
            let Some(nx) = onward(net, self.seed, a.id[i], ribbon, &mut turn) else {
                break;
            };
            chain[n] = (nx, off);
            n += 1;
            ribbon = nx;
        }
        let chain = &chain[..n];
        let rib = net.ribbon(r);
        let (x0, y0, _) = rib.to_world(s, y);
        // Others see its body as the chord of its path (`chord_pose`): on a
        // curve that lies inside the arc, by this much across its path.
        let (xc, yc, _, _) = chord_pose(
            net,
            self.seed,
            self.prev,
            a,
            i,
            s,
            y,
            a.heading_tan[i],
        );
        let dir0 = rib.direction(s);
        let y = y - dir0[1] * (xc - x0) + dir0[0] * (yc - y0);
        let ci = usize::from(a.class[i]);
        let hw = a.half_width[i];
        let scale = a.clearance_scale[i];
        let radius = idm.desired_gap(v, v, 1.0, 1.0) + l + self.l_max;
        let strength = p.steering_sensitivity * p.emergency_decel;
        for g in groups {
            for m in &self.junctions.members[g] {
                let k = m.i as usize;
                let rk = m.ribbon as usize;
                if k == i
                    || rk == r
                    || !self.junctions.related(r, rk)
                    || chain.iter().any(|&(q, _)| q == rk)
                {
                    continue;
                }
                let (dx, dy) = (m.x - x0, m.y - y0);
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                // Its place on the agent's path.
                let mut best = (f64::INFINITY, 0.0, 0.0, [1.0, 0.0]);
                for &(q, start) in chain {
                    let (sq, yq, d2) = net.ribbon(q).nearest(m.x, m.y);
                    if d2 < best.0 {
                        best = (d2, start + sq, yq, net.ribbon(q).direction(sq));
                    }
                }
                let (ds, yk, dir) = (best.1 - s, best.2, best.3);
                let cos = m.hx * dir[0] + m.hy * dir[1];
                let sin = dir[0] * m.hy - dir[1] * m.hx;
                let (lk, wk) = (a.length[k], a.width[k]);
                let half_l = 0.5 * (lk * cos.abs() + wk * sin.abs());
                let half_w = 0.5 * (lk * sin.abs() + wk * cos.abs());
                let gy = edge_gap(y, hw, yk, half_w);
                let gap_s = ds - 0.5 * l - half_l;
                let along = a.v[k] * cos;
                let cj = usize::from(a.class[k]);
                let c_min = scale * p.pair_clearance(ci, cj, 0.5 * (v + along.abs())).1;
                let excess = gy - c_min;
                let left = yk > y || (yk == y && a.id[k] > a.id[i]);
                // Who goes first between the two is decided by the
                // junction's arbitration (ADR-0016 part 4), not here.
                let arbitrated = self.junctions.arbiter.same_scope(i, k);
                // A body turned across the path beyond the heading limit
                // cannot be a vehicle going the same way: its own outline
                // is what is in the way, not the box around it.
                let across = sin.abs() > p.max_heading_tan * cos.abs();
                let follow = |gap: f64, weight: f64, acc: &mut f64, lead: &mut (f64, u64)| {
                    let leader = (free
                        - idm.interaction(
                            v,
                            gap.max(f64::MIN_POSITIVE),
                            v - along,
                            p.mult_time_gap[ci][cj],
                            p.mult_standstill_gap[ci][cj],
                        ))
                    .max(-p.emergency_decel);
                    let weighted = free + weight * (leader - free);
                    if weighted < lead.0 {
                        *lead = (weighted, a.id[k]);
                    }
                    *acc = acc.min(weighted);
                };
                // Rule 1 of decision D: never drive into a body lying across
                // the path ahead, whoever goes first. The clearance is kept
                // to it unless it gives way to this vehicle: then only its
                // body is to be missed, or one waiting politely beside the
                // stream would stop the stream it waits for.
                let keep = if arbitrated && self.junctions.arbiter.gives_way_to(a.id[k], a.id[i]) {
                    0.0
                } else {
                    c_min
                };
                if across
                    && let Some(gap) = body_ahead(
                        (ds, yk),
                        (cos, sin),
                        (0.5 * lk, 0.5 * wk),
                        (y - hw - keep, y + hw + keep),
                        0.5 * l,
                    )
                {
                    follow(gap, 1.0, acc, lead);
                    continue;
                }
                if gap_s <= 0.0 {
                    if ds + 0.5 * l + half_l <= 0.0 {
                        // Behind: it sees this vehicle.
                        continue;
                    }
                    // Level: no steering into it; steer away from one
                    // going the same way.
                    let push = steering_speed(excess, strength, p.steering_scale) * cos.max(0.0);
                    side(left, gy, excess, push);
                    if arbitrated {
                        continue;
                    }
                    // Their paths close on each other (two ribbons meet
                    // outside any junction): the one that is ahead goes
                    // first, the other drops back.
                    let closing = (yk > y && sin < 0.0) || (yk < y && sin > 0.0);
                    if excess <= 0.0 && (closing || gy < 0.0) {
                        let it_ahead = ds > 0.0;
                        let me_ahead = -(dx * m.hx + dy * m.hy) > 0.0;
                        let it_first = match (it_ahead, me_ahead) {
                            (true, false) => true,
                            (false, true) => false,
                            _ => a.id[k] < a.id[i],
                        };
                        if it_first {
                            *acc = acc.min(-idm.comfortable_decel);
                            if -idm.comfortable_decel < lead.0 {
                                *lead = (-idm.comfortable_decel, a.id[k]);
                            }
                        }
                    }
                    continue;
                }
                // No steering towards a body just ahead.
                if gap_s <= no_cut_in {
                    side(left, gy, excess, 0.0);
                }
                if excess <= 0.0 {
                    // In the way now (rule 1): followed. A body turned
                    // across the path was measured by its outline above.
                    if !across {
                        follow(gap_s, 1.0, acc, lead);
                    }
                    continue;
                }
                if arbitrated {
                    if cos > 0.0 && excess <= self.cut && !across {
                        follow(gap_s, leader_weight(excess, p.braking_scale), acc, lead);
                    }
                    continue;
                }
                // Not in the way now. Is it coming across?
                let towards = (yk > y && sin < 0.0) || (yk < y && sin > 0.0);
                if towards {
                    let d_k = excess / sin.abs();
                    let t_k = arrive(a.v[k], a.max_accel[k], d_k);
                    let t_me = arrive(v, idm.max_accel, gap_s);
                    let t_me_clear = arrive(v, idm.max_accel, gap_s + l + 2.0 * half_l);
                    let t_k_clear = arrive(
                        a.v[k],
                        a.max_accel[k],
                        d_k + (2.0 * hw + 2.0 * c_min) / sin.abs() + lk,
                    );
                    if t_me_clear < t_k || t_k_clear < t_me {
                        continue;
                    }
                    if t_k < t_me || (t_k == t_me && a.id[k] < a.id[i]) {
                        follow(gap_s, 1.0, acc, lead);
                    }
                    continue;
                }
                // Going the same way beside the path: the usual weight.
                if cos > 0.0 && excess <= self.cut {
                    follow(gap_s, leader_weight(excess, p.braking_scale), acc, lead);
                }
            }
        }
    }
}

/// The gap from a vehicle's front (`front` m ahead of its centre) to the
/// nearest part of another body that lies within the band `band` across
/// its path and ahead of that front, if any part does. The other body is a
/// rectangle with half-length and half-width `half`, centred at `centre`
/// and turned by `(cos, sin)`, all in the vehicle's own frame.
pub(crate) fn body_ahead(
    centre: (f64, f64),
    (cos, sin): (f64, f64),
    half: (f64, f64),
    band: (f64, f64),
    front: f64,
) -> Option<f64> {
    let (mut p, mut q) = ([[0.0; 2]; 8], [[0.0; 2]; 8]);
    for (n, (u, w)) in [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)]
        .into_iter()
        .enumerate()
    {
        p[n] = [
            centre.0 + u * half.0 * cos - w * half.1 * sin,
            centre.1 + u * half.0 * sin + w * half.1 * cos,
        ];
    }
    let n = clip(&p, 4, &mut q, |c| c[1] - band.0);
    let n = clip(&q, n, &mut p, |c| band.1 - c[1]);
    let n = clip(&p, n, &mut q, |c| c[0] - front);
    (n > 0).then(|| q[..n].iter().map(|c| c[0]).fold(f64::INFINITY, f64::min) - front)
}

/// Sutherland–Hodgman: the part of convex polygon `src[..n]` where
/// `inside(point) >= 0`, written to `dst`; returns its number of vertices.
fn clip(
    src: &[[f64; 2]; 8],
    n: usize,
    dst: &mut [[f64; 2]; 8],
    inside: impl Fn(&[f64; 2]) -> f64,
) -> usize {
    let mut m = 0;
    for k in 0..n {
        let (a, b) = (src[k], src[(k + 1) % n]);
        let (fa, fb) = (inside(&a), inside(&b));
        if fa >= 0.0 {
            dst[m] = a;
            m += 1;
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            dst[m] = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
            m += 1;
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::body_ahead;

    #[test]
    fn a_body_turned_across_the_path_is_seen_by_its_outline() {
        // A vehicle whose band is 1 m to each side and whose front is 2 m
        // ahead of its centre; a 4 x 2 m body turned 45 degrees.
        let r = std::f64::consts::FRAC_1_SQRT_2;
        let (half, band, front) = ((2.0, 1.0), (-1.0, 1.0), 2.0);
        // Centred 6 m ahead and 2 m left: its rear-left edge enters the
        // band at s = 7 - 4r.
        let g = body_ahead((6.0, 2.0), (r, r), half, band, front).expect("in the path");
        assert!((g - (5.0 - 4.0 * r)).abs() < 1e-9, "{g}");
        // 3 m ahead it reaches into the band beside the front: no gap.
        let g = body_ahead((3.0, 2.0), (r, r), half, band, front).expect("in the path");
        assert!(g.abs() < 1e-9, "{g}");
        // Turned the other way and further left, it is outside the band.
        assert!(body_ahead((3.0, 3.5), (r, -r), half, band, front).is_none());
        // Behind the front: not ahead.
        assert!(body_ahead((-3.0, 0.0), (r, r), (1.0, 1.0), band, front).is_none());
    }
}
