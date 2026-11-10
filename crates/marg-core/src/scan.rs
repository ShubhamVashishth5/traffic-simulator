//! The move phase for one agent (plan/02-architecture.md §4, ADR-0010).
//!
//! Everything here reads the snapshot of the previous step and writes
//! nothing, so agents can be processed in any order or in parallel with the
//! same result. For one agent it gives the longitudinal acceleration, the
//! lateral speed, the limits on this step's lateral position, the lateral
//! target if a decision was due, and the most critical conflict.

use crate::agents::{AgentStore, NO_AGENT};
use crate::model::coverage::Coverage;
use crate::model::geometry::{Approach, Body, time_to_contact};
use crate::model::lateral::{
    Choice, ChoiceParams, FollowerTerm, LeaderTerm, Neighbours, SideTerm, Subject, choose_target,
    edge_gap, leader_weight, steering_speed, track,
};
use crate::model::longitudinal::Idm;
use crate::params::CoreParams;
use marg_net::Network;

/// Conflict types (ADR-0010 §6), as stored in `AgentStore::conflict_kind`.
pub const CONFLICT_REAR_END: u8 = 0;
pub const CONFLICT_SIDESWIPE: u8 = 1;
pub const CONFLICT_HEAD_ON: u8 = 2;

/// Name of a conflict type in the output tables.
pub fn conflict_kind_name(kind: u8) -> &'static str {
    match kind {
        CONFLICT_REAR_END => "rear_end",
        CONFLICT_SIDESWIPE => "sideswipe",
        _ => "head_on",
    }
}

/// One other agent considered by an agent's scan: the terms of
/// plan/03-model-design.md §2, for the live inspector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interaction {
    /// Store index of the other agent.
    pub j: u32,
    /// Distance between centres along the ribbon chain (m); negative for a
    /// side neighbour whose centre is behind.
    pub ds: f64,
    /// Bumper-to-bumper gap (m); ≤ 0 means level with the agent (a side neighbour).
    pub gap_s: f64,
    /// Edge-to-edge lateral gap (m); < 0 means the footprints overlap laterally.
    pub lateral_gap: f64,
    /// The agent's minimum lateral clearance to it, `c_min(v̄)` (m).
    pub clearance: f64,
    /// Leader weight ω; NaN for side neighbours.
    pub weight: f64,
    /// IDM acceleration toward this leader alone (m/s²); NaN for side neighbours.
    pub leader_accel: f64,
}

/// A time to contact under the conflict threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conflict {
    /// Store index of the other agent.
    pub other: u32,
    pub ttc: f64,
    /// Deceleration along the approach axis that would just avoid contact
    /// (m/s²); NaN when the bodies are already level on that axis.
    pub drac: f64,
    pub kind: u8,
}

/// What the move phase decided for one agent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveOut {
    pub acc: f64,
    /// Lateral speed for this step, before the heading limit.
    pub vy: f64,
    /// This step's lateral position must stay within these limits: half of
    /// the free gap to each side neighbour.
    pub y_lo: f64,
    pub y_hi: f64,
    /// Id of the most constraining leader, or `NO_AGENT`.
    pub leader: u64,
    /// The lateral decision, if one was due this step.
    pub choice: Option<Choice>,
    pub conflict: Option<Conflict>,
}

/// A stop line and whether it is closed this step.
#[derive(Debug, Clone, PartialEq)]
pub struct Barrier {
    pub id: String,
    pub ribbon: usize,
    pub s: f64,
    pub closed: Vec<(f64, f64)>,
    pub is_closed: bool,
}

/// Read-only context for the move phase.
pub struct MoveCtx<'a> {
    pub a: &'a AgentStore,
    pub orders: &'a [Vec<u32>],
    pub net: &'a Network,
    pub p: &'a CoreParams,
    /// A predecessor of each ribbon, for looking behind across a boundary.
    pub prev: &'a [Option<usize>],
    /// `chain_offset[from][to]`: distance from the start of ribbon `from` to
    /// the start of `to` along the chain; NaN if `to` is not downstream.
    pub chain_offset: &'a [Vec<f64>],
    pub barriers: &'a [Barrier],
    /// Lateral distance beyond the minimum clearance at which a leader's
    /// weight, and a side neighbour's steering response, are cut off.
    pub cut: f64,
    pub steer_cut: f64,
    /// Longest vehicle in the network: agents are sorted by centre, so a
    /// long vehicle whose centre is further away can still be nearer.
    pub l_max: f64,
    pub step: u64,
    /// Steps between an agent's scheduled decisions.
    pub decision_steps: u64,
    pub dt: f64,
    pub choice: ChoiceParams,
    /// Largest lateral speed in the network, and an upper bound on any
    /// minimum clearance at the current speeds: with them, a vehicle too far
    /// to the side to matter is skipped after one comparison.
    pub vy_max: f64,
    pub c_min_cap: f64,
    /// Highest speed in the network.
    pub v_max: f64,
}

/// Limits and steering response collected from side neighbours.
struct Sides {
    repulsion: f64,
    y_lo: f64,
    y_hi: f64,
}

impl Sides {
    /// A neighbour level with the agent, `dy` to its left (negative: right),
    /// with edge gap `gy`, `excess` above the minimum clearance.
    #[inline]
    fn add(&mut self, left: bool, y: f64, gy: f64, excess: f64, strength: f64, scale: f64) {
        let push = steering_speed(excess, strength, scale);
        if left {
            self.repulsion -= push;
        } else {
            self.repulsion += push;
        }
        self.limit(left, y, gy);
    }

    /// Each of two vehicles may take half of the free gap between them.
    #[inline]
    fn limit(&mut self, left: bool, y: f64, gy: f64) {
        let room = 0.5 * gy.max(0.0);
        if left {
            self.y_hi = self.y_hi.min(y + room);
        } else {
            self.y_lo = self.y_lo.max(y - room);
        }
    }
}

impl MoveCtx<'_> {
    #[inline]
    fn idm(&self, i: usize) -> Idm {
        let a = self.a;
        Idm {
            v0: a.v0[i],
            time_gap: a.time_gap[i],
            standstill_gap: a.standstill_gap[i],
            max_accel: a.max_accel[i],
            comfortable_decel: a.comfortable_decel[i],
            exponent: self.p.accel_exponent,
        }
    }

    /// The gap within which a vehicle closing at `dv` cannot be cut in on:
    /// what it covers in one step, plus the distance it needs to shed `dv` at
    /// the safe braking rate (ADR-0010 §3, applied to every step of a move).
    #[inline]
    fn no_cut_in_gap(&self, dv: f64, closing_slack: f64) -> f64 {
        let dv = dv.max(0.0);
        (dv + closing_slack) * self.dt + dv * dv / (2.0 * self.p.safe_decel)
    }

    /// Whether `j` counts as being to the left of `i` (ties by id, so two
    /// agents at the same `y` still steer apart).
    #[inline]
    fn is_left(&self, i: usize, j: usize) -> bool {
        let (a, dy) = (self.a, self.a.y[j] - self.a.y[i]);
        dy > 0.0 || (dy == 0.0 && a.id[j] > a.id[i])
    }

    /// The move of agent `i` at position `k` of ribbon `r`.
    #[inline]
    pub fn agent(&self, r: usize, k: usize, i: usize) -> MoveOut {
        self.agent_with(r, k, i, |_| {})
    }

    /// [`Self::agent`], reporting every agent the scan considers to `seen`
    /// (the live inspector). With a no-op `seen` it compiles to the same code.
    #[inline(always)]
    pub fn agent_with(
        &self,
        r: usize,
        k: usize,
        i: usize,
        mut seen: impl FnMut(Interaction),
    ) -> MoveOut {
        let (a, p) = (self.a, self.p);
        let ci = usize::from(a.class[i]);
        let idm = self.idm(i);
        let (v, y, l, s) = (a.v[i], a.y[i], a.length[i], a.s[i]);
        let (hw, vy, scale) = (a.half_width[i], a.vy[i], a.clearance_scale[i]);
        let free = idm.free_accel(v);
        let mut acc = free;
        // A leader matters for decisions once it costs more than the switching threshold.
        let mut lead = (free - p.switch_threshold, NO_AGENT);
        let mut sides = Sides {
            repulsion: 0.0,
            y_lo: f64::NEG_INFINITY,
            y_hi: f64::INFINITY,
        };
        let mut conflict: Option<Conflict> = None;
        let strength = p.steering_sensitivity * p.emergency_decel;
        let reach = self.cut.max(self.steer_cut);
        // Beyond this lateral gap a vehicle can neither interact nor conflict.
        let far = (reach + self.c_min_cap).max(p.conflict_ttc * (vy.abs() + self.vy_max));
        // How much two vehicles' speeds can change towards each other in a step.
        let closing_slack = (idm.max_accel + p.emergency_decel) * self.dt;
        let mut cov = Coverage::new(y - hw, y + hw);
        let mut horizon = p.lookahead;
        let (mut ribbon, mut pos, mut offset) = (r, k + 1, 0.0);
        'scan: loop {
            let order = &self.orders[ribbon];
            while pos < order.len() {
                let j = order[pos] as usize;
                pos += 1;
                let ds = a.s[j] + offset - s;
                if ds > horizon {
                    break 'scan;
                }
                let hwj = a.half_width[j];
                let gy = edge_gap(y, hw, a.y[j], hwj);
                if gy > far {
                    continue;
                }
                let gap_s = ds - 0.5 * (l + a.length[j]);
                let left = self.is_left(i, j);
                // Conflicts: a cheap bound on when the two could touch, then
                // the exact time to contact of the oriented rectangles.
                let dv = v - a.v[j];
                let reach_s = gap_s - (hw + hwj) * p.max_heading_tan;
                if reach_s <= 0.0 || dv * p.conflict_ttc > reach_s {
                    let closing = if left { vy - a.vy[j] } else { a.vy[j] - vy };
                    if gy <= 0.0 || closing * p.conflict_ttc > gy {
                        let me = Body::from_tan(0.0, y, l, a.width[i], a.heading_tan[i]);
                        let other =
                            Body::from_tan(ds, a.y[j], a.length[j], a.width[j], a.heading_tan[j]);
                        if let Some((ttc, approach)) =
                            time_to_contact(&me, &other, a.v[j] - v, a.vy[j] - vy)
                            && ttc < p.conflict_ttc
                            && conflict.is_none_or(|c| ttc < c.ttc)
                        {
                            let (kind, gap, rate) = match approach {
                                Approach::Lateral => (CONFLICT_SIDESWIPE, gy, closing),
                                Approach::Longitudinal if v * a.v[j] < 0.0 => {
                                    (CONFLICT_HEAD_ON, gap_s, dv)
                                }
                                Approach::Longitudinal => (CONFLICT_REAR_END, gap_s, dv),
                            };
                            conflict = Some(Conflict {
                                other: j as u32,
                                ttc,
                                drac: if gap > 0.0 {
                                    rate * rate / (2.0 * gap)
                                } else {
                                    f64::NAN
                                },
                                kind,
                            });
                        }
                    }
                }
                // A vehicle ahead that we may draw level with during this
                // step, or could not avoid at the safe braking rate, limits
                // the lateral move like a side neighbour: no cutting in.
                if gap_s > 0.0 && gap_s <= self.no_cut_in_gap(dv, closing_slack) {
                    sides.limit(left, y, gy);
                }
                let cj = usize::from(a.class[j]);
                let c_min = scale * p.pair_clearance(ci, cj, 0.5 * (v + a.v[j])).1;
                let excess = gy - c_min;
                if excess > reach {
                    continue;
                }
                if gap_s <= 0.0 {
                    seen(Interaction {
                        j: j as u32,
                        ds,
                        gap_s,
                        lateral_gap: gy,
                        clearance: c_min,
                        weight: f64::NAN,
                        leader_accel: f64::NAN,
                    });
                    // Level with us: a side neighbour, unless the bodies overlap.
                    if edge_gap(y, 0.5 * a.width[i], a.y[j], 0.5 * a.width[j]) < 0.0 {
                        acc = -p.emergency_decel;
                    }
                    sides.add(left, y, gy, excess, strength, p.steering_scale);
                    continue;
                }
                if excess > self.cut {
                    continue;
                }
                let leader = (free
                    - idm.interaction(
                        v,
                        gap_s,
                        dv,
                        p.mult_time_gap[ci][cj],
                        p.mult_standstill_gap[ci][cj],
                    ))
                .max(-p.emergency_decel);
                let weight = leader_weight(excess, p.braking_scale);
                seen(Interaction {
                    j: j as u32,
                    ds,
                    gap_s,
                    lateral_gap: gy,
                    clearance: c_min,
                    weight,
                    leader_accel: leader,
                });
                let weighted = free + weight * (leader - free);
                if weighted < lead.0 {
                    lead = (weighted, a.id[j]);
                }
                acc = acc.min(weighted);
                if excess <= 0.0 && horizon == p.lookahead {
                    cov.add(a.y[j] - hwj - c_min, a.y[j] + hwj + c_min);
                    if cov.full() {
                        // Anything centred beyond ds + l_max/2 has its rear ahead of every covering leader.
                        horizon = horizon.min(ds + 0.5 * self.l_max);
                    }
                }
            }
            match self.net.ribbon(ribbon).next {
                Some(n) => {
                    offset += self.net.ribbon(ribbon).length();
                    ribbon = n;
                    pos = 0;
                }
                None => break,
            }
        }
        // Side neighbours whose centre is behind ours.
        let back_reach = 0.5 * (l + self.l_max) + self.no_cut_in_gap(self.v_max - v, closing_slack);
        let (mut ribbon, mut pos, mut offset) = (r, k, 0.0);
        'back: loop {
            let order = &self.orders[ribbon];
            while pos > 0 {
                pos -= 1;
                let j = order[pos] as usize;
                let ds = s + offset - a.s[j];
                if ds > back_reach {
                    break 'back;
                }
                let gap_s = ds - 0.5 * (l + a.length[j]);
                let gy = edge_gap(y, hw, a.y[j], a.half_width[j]);
                if gap_s > 0.0 {
                    // A vehicle behind that may draw level during this step, or
                    // could not avoid us at the safe braking rate.
                    if gap_s <= self.no_cut_in_gap(a.v[j] - v, closing_slack) {
                        sides.limit(self.is_left(i, j), y, gy);
                    }
                    continue;
                }
                let cj = usize::from(a.class[j]);
                let c_min = scale * p.pair_clearance(ci, cj, 0.5 * (v + a.v[j])).1;
                let excess = gy - c_min;
                if excess > reach {
                    continue;
                }
                seen(Interaction {
                    j: j as u32,
                    ds: -ds,
                    gap_s,
                    lateral_gap: gy,
                    clearance: c_min,
                    weight: f64::NAN,
                    leader_accel: f64::NAN,
                });
                sides.add(
                    self.is_left(i, j),
                    y,
                    gy,
                    excess,
                    strength,
                    p.steering_scale,
                );
            }
            match self.prev[ribbon] {
                Some(q) => {
                    offset += self.net.ribbon(q).length();
                    ribbon = q;
                    pos = self.orders[q].len();
                }
                None => break,
            }
        }
        // Closed barriers ahead: a standing virtual vehicle across the road.
        let mut cap = f64::INFINITY;
        for b in self.barriers {
            if !b.is_closed {
                continue;
            }
            let off = self.chain_offset[r][b.ribbon];
            let gap = b.s + off - s - 0.5 * l;
            if gap > 0.0 && gap <= p.lookahead {
                cap =
                    cap.min((free - idm.interaction(v, gap, v, 1.0, 1.0)).max(-p.emergency_decel));
            }
        }
        acc = acc.min(cap);
        // Lateral target: on the agent's schedule, or when its leader changed.
        let due = (self.step.wrapping_add(a.id[i])).is_multiple_of(self.decision_steps)
            || lead.1 != a.decision_leader[i];
        let (target, choice) = if due {
            let c = self.decide(r, k, i, &idm, cap);
            (c.target, Some(c))
        } else {
            (a.y_target[i], None)
        };
        MoveOut {
            acc: acc.clamp(-p.emergency_decel, idm.max_accel),
            vy: track(
                vy,
                y,
                target,
                sides.repulsion,
                p.lateral_time_constant,
                self.dt,
            ),
            y_lo: sides.y_lo,
            y_hi: sides.y_hi,
            leader: lead.1,
            choice,
            conflict,
        }
    }

    /// Gather the neighbours that matter to agent `i`'s lateral choice and
    /// choose its target (ADR-0010 §3).
    pub fn decide(&self, r: usize, k: usize, i: usize, idm: &Idm, cap: f64) -> Choice {
        let (a, p) = (self.a, self.p);
        let ci = usize::from(a.class[i]);
        let (v, y, l, s) = (a.v[i], a.y[i], a.length[i], a.s[i]);
        let hw = 0.5 * a.width[i];
        let scale = a.clearance_scale[i];
        let squeeze = p.squeeze_scale.unwrap_or(scale);
        let free = idm.free_accel(v);
        let range = p.candidate_range;
        let mut nb = Neighbours::default();
        // Limits from side neighbours (the squeezing rule).
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        let mut side = |nb: &mut Neighbours, j: usize, c_des0: f64, c_min0: f64| {
            let hwj = a.half_width[j];
            nb.push_side(SideTerm {
                y: a.y[j],
                half_width: hwj,
                c_min: scale * c_min0,
                c_des: scale * c_des0,
            });
            let room = hwj + hw + squeeze * c_min0;
            if self.is_left(i, j) {
                hi = hi.min(a.y[j] - room);
            } else {
                lo = lo.max(a.y[j] + room);
            }
        };
        // Ahead.
        let mut cov = Coverage::new(y - range - hw, y + range + hw);
        let mut horizon = p.lookahead;
        let (mut ribbon, mut pos, mut offset) = (r, k + 1, 0.0);
        'ahead: loop {
            let order = &self.orders[ribbon];
            while pos < order.len() {
                let j = order[pos] as usize;
                pos += 1;
                let ds = a.s[j] + offset - s;
                if ds > horizon {
                    break 'ahead;
                }
                let hwj = a.half_width[j];
                // The smallest lateral gap any candidate could have to j.
                let nearest = ((a.y[j] - y).abs() - range).max(0.0) - hw - hwj;
                let cj = usize::from(a.class[j]);
                let (c_des0, c_min0) = p.pair_clearance(ci, cj, 0.5 * (v + a.v[j]));
                let (c_des, c_min) = (scale * c_des0, scale * c_min0);
                if nearest > c_des.max(c_min + self.cut) {
                    continue;
                }
                let gap_s = ds - 0.5 * (l + a.length[j]);
                if gap_s <= 0.0 {
                    side(&mut nb, j, c_des0, c_min0);
                    continue;
                }
                let (mt, ms) = (p.mult_time_gap[ci][cj], p.mult_standstill_gap[ci][cj]);
                let cost = idm.interaction(v, gap_s, v - a.v[j], mt, ms);
                let near = gap_s < idm.standstill_gap * ms + v * idm.time_gap * mt;
                // A leader that costs less than the switching threshold even
                // at full weight cannot decide a move.
                if !near && cost < p.switch_threshold {
                    continue;
                }
                let accel = (free - cost).max(-p.emergency_decel);
                if !nb.push_leader(LeaderTerm {
                    y: a.y[j],
                    half_width: hwj,
                    accel,
                    c_min,
                    c_des,
                    near,
                }) {
                    break 'ahead;
                }
                if horizon == p.lookahead {
                    cov.add(a.y[j] - hwj - c_min, a.y[j] + hwj + c_min);
                    if cov.full() {
                        horizon = horizon.min(ds + 0.5 * self.l_max);
                    }
                }
            }
            match self.net.ribbon(ribbon).next {
                Some(n) => {
                    offset += self.net.ribbon(ribbon).length();
                    ribbon = n;
                    pos = 0;
                }
                None => break,
            }
        }
        // Behind.
        let mut cov = Coverage::new(y - range - hw, y + range + hw);
        let mut horizon = p.lookahead;
        let (mut ribbon, mut pos, mut offset) = (r, k, 0.0);
        'behind: loop {
            let order = &self.orders[ribbon];
            while pos > 0 {
                pos -= 1;
                let j = order[pos] as usize;
                let ds = s + offset - a.s[j];
                if ds > horizon {
                    break 'behind;
                }
                let hwj = a.half_width[j];
                let nearest = ((a.y[j] - y).abs() - range).max(0.0) - hw - hwj;
                let cj = usize::from(a.class[j]);
                let (c_des0, c_min0) = p.pair_clearance(ci, cj, 0.5 * (v + a.v[j]));
                let c_min_f = a.clearance_scale[j] * c_min0;
                if nearest > (scale * c_des0).max(c_min_f + self.cut) {
                    continue;
                }
                let gap_s = ds - 0.5 * (l + a.length[j]);
                if gap_s <= 0.0 {
                    side(&mut nb, j, c_des0, c_min0);
                    continue;
                }
                let follower = self.idm(j);
                let (mt, ms) = (p.mult_time_gap[cj][ci], p.mult_standstill_gap[cj][ci]);
                let vf = a.v[j];
                let cost = follower.interaction(vf, gap_s, vf - v, mt, ms);
                let near = gap_s < follower.standstill_gap * ms + vf * follower.time_gap * mt;
                if !near && cost < p.switch_threshold {
                    continue;
                }
                let free_f = follower.free_accel(vf);
                let imposed = (free_f - cost).max(-p.emergency_decel);
                if !nb.push_follower(FollowerTerm {
                    y: a.y[j],
                    half_width: hwj,
                    free: free_f,
                    imposed,
                    acc_now: a.acc[j],
                    c_min: c_min_f,
                    c_min_own: scale * c_min0,
                    c_des_own: scale * c_des0,
                    near,
                }) {
                    break 'behind;
                }
                if horizon == p.lookahead {
                    cov.add(a.y[j] - hwj - c_min_f, a.y[j] + hwj + c_min_f);
                    if cov.full() {
                        horizon = horizon.min(ds + 0.5 * self.l_max);
                    }
                }
            }
            match self.prev[ribbon] {
                Some(q) => {
                    offset += self.net.ribbon(q).length();
                    ribbon = q;
                    pos = self.orders[q].len();
                }
                None => break,
            }
        }
        // The feasible interval: never across a side neighbour, never off the road.
        let (edge_lo, edge_hi) = self.net.ribbon(r).y_bounds(s);
        let (road_lo, road_hi) = centre_bounds(edge_lo, edge_hi, hw);
        let (mut lo, mut hi) = (lo.min(y).max(road_lo), hi.max(y).min(road_hi));
        if lo > hi {
            lo = y.clamp(road_lo, road_hi);
            hi = lo;
        }
        let with_bonus = Idm {
            v0: a.v0_base[i] + a.v0_bonus[i],
            ..*idm
        };
        let without = Idm {
            v0: a.v0_base[i],
            ..*idm
        };
        let me = Subject {
            y,
            half_width: hw,
            lo,
            hi,
            edge_lo,
            edge_hi,
            free_band: with_bonus.free_accel(v),
            free_out: without.free_accel(v),
            band_y: edge_lo + p.median_band_width,
            cap,
            target: a.y_target[i],
        };
        choose_target(&self.choice, &me, &mut nb)
    }
}

/// The interval a body's centre can take on a carriageway `[edge_lo,
/// edge_hi]`; the middle if the body is wider than the road.
#[inline]
pub fn centre_bounds(edge_lo: f64, edge_hi: f64, half_width: f64) -> (f64, f64) {
    let (lo, hi) = (edge_lo + half_width, edge_hi - half_width);
    if lo <= hi {
        (lo, hi)
    } else {
        let mid = 0.5 * (edge_lo + edge_hi);
        (mid, mid)
    }
}
