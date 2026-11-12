//! The move phase for one agent (plan/02-architecture.md §4, ADR-0010).
//!
//! Everything here reads the snapshot of the previous step and writes
//! nothing, so agents can be processed in any order or in parallel with the
//! same result. For one agent it gives the longitudinal acceleration, the
//! lateral speed, the limits on this step's lateral position, the lateral
//! target if a decision was due, and the most critical conflict.

use crate::agents::{
    AgentStore, NO_AGENT, STOP_IN_PLACE, STOP_NONE, STOP_PULLING_OVER, STOP_STANDING,
};
use crate::model::coverage::Coverage;
use crate::model::geometry::{Approach, Body, time_to_contact};
use crate::model::lateral::{
    Choice, ChoiceParams, FollowerTerm, LeaderTerm, Neighbours, SideTerm, Subject, Wall,
    body_half_width, body_stretch, choose_target, edge_gap, leader_weight, steering_speed, track,
};
use crate::model::longitudinal::Idm;
use crate::objects::{EDGE_LEADER, EdgeAhead, Narrowings, Objects};
use crate::params::CoreParams;
use marg_net::Network;
use std::cell::RefCell;

thread_local! {
    /// Neighbour lists for lateral decisions, reused by every decision on a
    /// thread so that none builds (and zeroes) a fresh one.
    static NEIGHBOURS: RefCell<Neighbours> = RefCell::new(Neighbours::default());
}

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
    /// How far the kerb of a narrowing road may move it (within the limits
    /// above; a vehicle coming up behind has no say in this).
    pub kerb_lo: f64,
    pub kerb_hi: f64,
    /// How much further the body may reach along the road, at either end,
    /// when it turns: the gap to the nearest body in front of or behind it
    /// that a turn could bring it against (a turned rectangle is longer
    /// along the road as well as wider across it).
    pub turn_room: f64,
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
    /// Road objects, and where the carriageway narrows.
    pub objects: &'a Objects,
    pub narrow: &'a Narrowings,
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
    /// Where the kerb of a narrowing road may move the vehicle to: the
    /// limits above without those set by vehicles coming up behind, which
    /// give way to the one in front.
    kerb_lo: f64,
    kerb_hi: f64,
    /// Where the kerb of a narrowing road may move the vehicle to without
    /// taking it inside its minimum clearance of a neighbour (within
    /// `y_lo..y_hi`): beyond these it brakes for the kerb.
    clear_lo: f64,
    clear_hi: f64,
    /// The carriageway narrows ahead: the room that vehicles level with
    /// this one leave the kerb is worked out apart from their limits.
    narrowing: bool,
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
        if self.narrowing {
            self.hard(left, y, gy);
        } else {
            self.limit(left, y, gy);
        }
    }

    /// [`Self::limit`] without a say in where the vehicle brakes for the kerb.
    #[inline]
    fn hard(&mut self, left: bool, y: f64, gy: f64) {
        let room = 0.5 * gy.max(0.0);
        if left {
            self.y_hi = self.y_hi.min(y + room);
            self.kerb_hi = self.kerb_hi.min(y + room);
        } else {
            self.y_lo = self.y_lo.max(y - room);
            self.kerb_lo = self.kerb_lo.max(y - room);
        }
    }

    /// [`Self::limit`] for the vehicle's own moves only.
    #[inline]
    fn behind(&mut self, left: bool, y: f64, gy: f64) {
        let room = 0.5 * gy.max(0.0);
        if left {
            self.y_hi = self.y_hi.min(y + room);
        } else {
            self.y_lo = self.y_lo.max(y - room);
        }
    }

    /// Each of two vehicles may take half of the free gap between them.
    #[inline]
    fn limit(&mut self, left: bool, y: f64, gy: f64) {
        self.room(left, y, 0.5 * gy.max(0.0));
    }

    /// A standing body does not move: the whole gap to it can be used.
    #[inline]
    fn wall(&mut self, left: bool, y: f64, gy: f64) {
        self.room(left, y, gy.max(0.0));
    }

    #[inline]
    fn room(&mut self, left: bool, y: f64, room: f64) {
        if left {
            self.y_hi = self.y_hi.min(y + room);
            self.kerb_hi = self.kerb_hi.min(y + room);
        } else {
            self.y_lo = self.y_lo.max(y - room);
            self.kerb_lo = self.kerb_lo.max(y - room);
        }
        self.clear(left, y, room);
    }

    #[inline]
    fn clear(&mut self, left: bool, y: f64, room: f64) {
        if left {
            self.clear_hi = self.clear_hi.min(y + room);
        } else {
            self.clear_lo = self.clear_lo.max(y - room);
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
            kerb_lo: f64::NEG_INFINITY,
            kerb_hi: f64::INFINITY,
            clear_lo: f64::NEG_INFINITY,
            clear_hi: f64::INFINITY,
            narrowing: false,
        };
        let mut conflict: Option<Conflict> = None;
        let strength = p.steering_sensitivity * p.emergency_decel;
        let reach = self.cut.max(self.steer_cut);
        // Beyond this lateral gap a vehicle can neither interact nor conflict.
        let far = (reach + self.c_min_cap).max(p.conflict_ttc * (vy.abs() + self.vy_max));
        // How much two vehicles' speeds can change towards each other in a step.
        let closing_slack = (idm.max_accel + p.emergency_decel) * self.dt;
        // We close on a vehicle ahead at most at our own speed, so beyond
        // this gap none can be cut in on.
        let no_cut_in_ahead = self.no_cut_in_gap(v, closing_slack);
        // Where the carriageway narrows within the lookahead, its edges ahead.
        let half_body = 0.5 * a.width[i];
        let edges = self
            .narrow
            .within(r, s, p.lookahead)
            .then(|| EdgeAhead::gather(self.net, r, s, p.lookahead));
        sides.narrowing = edges.is_some();
        let mut cov = Coverage::new(y - hw, y + hw);
        let mut horizon = p.lookahead;
        // A vehicle that creeps sideways at a standstill turns on the spot
        // (ADR-0012 part 2). Turned to the heading limit its body reaches
        // `stretch` further at each end and `swing` further to each side,
        // which can be more than the side neighbours' range for a long
        // vehicle: bodies within that reach limit the creep.
        let creeping = v < p.stop_speed_below && p.creep_speed[ci] > 0.0;
        let (stretch, swing) = if creeping {
            (
                body_stretch(a.width[i], l, p.max_heading_tan).max(0.0),
                body_half_width(a.width[i], l, p.max_heading_tan) - hw
                    + 2.0 * self.vy_max * self.dt,
            )
        } else {
            (f64::NEG_INFINITY, f64::NEG_INFINITY)
        };
        let mut turn_room = f64::INFINITY;
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
                if let Some(edges) = &edges {
                    self.narrowing_ahead(
                        i, j, ds, gy, edges, &idm, free, &mut acc, &mut lead, &mut sides,
                    );
                }
                let gap_s = ds - 0.5 * (l + a.length[j]);
                if gap_s > 0.0 {
                    let theirs = 0.5 * a.width[j] * p.max_heading_sin;
                    if gap_s <= stretch + theirs
                        && gy <= swing + 0.5 * a.length[j] * p.max_heading_sin
                    {
                        turn_room = turn_room.min(gap_s - theirs);
                    }
                }
                if gap_s <= 0.0 && gy <= swing + 0.5 * a.length[j] * p.max_heading_sin {
                    sides.behind(self.is_left(i, j), y, gy);
                }
                if gy > far {
                    continue;
                }
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
                if gap_s > 0.0
                    && gap_s <= no_cut_in_ahead
                    && gap_s <= self.no_cut_in_gap(dv, closing_slack)
                {
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
                if let Some(edges) = &edges
                    && gap_s <= 0.0
                {
                    // As for a vehicle level with us whose centre is ahead.
                    self.level_in_narrowing(
                        i, j, gy, -ds, edges, &idm, free, &mut acc, &mut lead, &mut sides,
                    );
                }
                if gap_s > 0.0 {
                    let theirs = 0.5 * a.width[j] * p.max_heading_sin;
                    if gap_s <= stretch + theirs
                        && gy <= swing + 0.5 * a.length[j] * p.max_heading_sin
                    {
                        turn_room = turn_room.min(gap_s - theirs);
                    }
                    // A vehicle behind that may draw level during this step, or
                    // could not avoid us at the safe braking rate. Where the
                    // road narrows it is the one behind that gives way: it
                    // does not hold the kerb's push back.
                    if gap_s <= self.no_cut_in_gap(a.v[j] - v, closing_slack) {
                        if edges.is_some() {
                            sides.behind(self.is_left(i, j), y, gy);
                        } else {
                            sides.limit(self.is_left(i, j), y, gy);
                        }
                    }
                    continue;
                }
                if gy <= swing + 0.5 * a.length[j] * p.max_heading_sin {
                    sides.behind(self.is_left(i, j), y, gy);
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
        // Road objects: standing bodies. One ahead is a leader, one level
        // with the vehicle a side neighbour (plan/03-model-design.md §10).
        if self.objects.any_active() {
            // Objects are found by their rear end, vehicles by their centre.
            let ahead = if horizon == p.lookahead {
                horizon
            } else {
                horizon - 0.5 * self.l_max
            };
            let near = |o: &crate::objects::RoadObject, ds: f64| {
                if !o.blocks[ci] {
                    return;
                }
                let half_o = 0.5 * o.width;
                let gy = edge_gap(y, hw, o.y, half_o);
                if gy > far {
                    return;
                }
                let gap_s = ds.abs() - 0.5 * (l + o.length);
                if gap_s > 0.0 && gap_s <= stretch && gy <= swing {
                    turn_room = turn_room.min(gap_s);
                }
                if gap_s > 0.0 && ds < 0.0 {
                    return;
                }
                let left = o.y >= y;
                let c_min = scale * p.object_clearance(ci, o.clearance_class, 0.5 * v).1;
                let excess = gy - c_min;
                if gap_s <= 0.0 {
                    // An object that appeared on top of the vehicle is
                    // ignored until the vehicle has left it.
                    if edge_gap(y, half_body, o.y, half_o) >= 0.0 && excess > reach && gy <= swing {
                        sides.behind(left, y, 2.0 * gy);
                    }
                    if edge_gap(y, half_body, o.y, half_o) >= 0.0 && excess <= reach {
                        let push = steering_speed(excess, strength, p.steering_scale);
                        sides.repulsion += if left { -push } else { push };
                        sides.wall(left, y, gy);
                        sides.clear(left, y, excess.max(0.0));
                    }
                    return;
                }
                // No steering into the path of something that cannot be
                // stopped for at the safe braking rate.
                if gap_s <= no_cut_in_ahead {
                    sides.wall(left, y, gy);
                }
                if excess > self.cut {
                    return;
                }
                let (mt, ms) = p.object_multipliers(ci, o.clearance_class);
                let leader = (free - idm.interaction(v, gap_s, v, mt, ms)).max(-p.emergency_decel);
                let weighted = free + leader_weight(excess, p.braking_scale) * (leader - free);
                if weighted < lead.0 {
                    lead = (weighted, o.leader_id());
                }
                acc = acc.min(weighted);
            };
            self.objects
                .near(self.net, self.prev, r, s, 0.5 * l, ahead, near);
        }
        // The carriageway ahead. The kerb moves a vehicle sideways as the
        // road narrows; where that would take it beyond the room its
        // neighbours leave, the kerb is a standing obstacle to brake for.
        if let Some(edges) = &edges
            && let Some(d) = edges.contact(sides.clear_hi - half_body, sides.clear_lo + half_body)
            && d <= p.lookahead
        {
            let wall = if d > 0.0 {
                (free - idm.interaction(v, d, v, 1.0, 1.0)).max(-p.emergency_decel)
            } else {
                -p.emergency_decel
            };
            if wall < lead.0 {
                lead = (wall, EDGE_LEADER);
            }
            acc = acc.min(wall);
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
        // A vehicle that is stopping to pick up or set down (ADR-0014)
        // brakes for its stop point as for something standing there, and
        // makes for the kerb or holds its line; one that stands, stands.
        let stop = a.stop_state[i];
        if stop == STOP_STANDING {
            return MoveOut {
                acc: -idm.comfortable_decel,
                vy: 0.0,
                y_lo: sides.y_lo,
                y_hi: sides.y_hi,
                kerb_lo: sides.kerb_lo,
                kerb_hi: sides.kerb_hi,
                turn_room,
                leader: lead.1,
                choice: None,
                conflict,
            };
        }
        if stop != STOP_NONE {
            let gap = a.stop_point[i] - a.distance[i];
            acc = acc.min(if gap > 0.0 {
                (free - idm.interaction(v, gap, v, 1.0, 1.0)).max(-p.emergency_decel)
            } else {
                -idm.comfortable_decel
            });
        }
        // Lateral target: on the agent's schedule, or when its leader changed.
        let due = (self.step.wrapping_add(a.id[i])).is_multiple_of(self.decision_steps)
            || lead.1 != a.decision_leader[i];
        let (target, choice) = if stop == STOP_PULLING_OVER {
            (self.net.ribbon(r).y_bounds(s).1 - 0.5 * a.width[i], None)
        } else if stop == STOP_IN_PLACE {
            (a.y_target[i], None)
        } else if due {
            let c = NEIGHBOURS.with_borrow_mut(|nb| self.decide_with(r, k, i, &idm, cap, nb));
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
            kerb_lo: sides.kerb_lo,
            kerb_hi: sides.kerb_hi,
            turn_room,
            leader: lead.1,
            choice,
            conflict,
        }
    }

    /// What vehicle `j`, `ds` m ahead of `i` with edge gap `gy`, means to
    /// `i` where the carriageway narrows ahead (`edges`), however far to the
    /// side it is. Level with `i`: see [`Self::level_in_narrowing`]. Ahead
    /// of `i`, it is a leader, to be followed, if within the lookahead
    ///
    /// - the kerb on `i`'s side closes the space beside `j` to less than
    ///   `i`'s width and minimum clearance before `i` could be past `j`, or
    /// - the road becomes too narrow for the two abreast before then, or
    /// - the kerb on `j`'s side brings `j` into `i`'s line.
    ///
    /// The vehicle in front goes first; the one behind does not draw level.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn narrowing_ahead(
        &self,
        i: usize,
        j: usize,
        ds: f64,
        gy: f64,
        edges: &EdgeAhead,
        idm: &Idm,
        free: f64,
        acc: &mut f64,
        lead: &mut (f64, u64),
        sides: &mut Sides,
    ) {
        let (a, p) = (self.a, self.p);
        let (y, v, width) = (a.y[i], a.v[i], a.width[i]);
        let reach = 0.5 * (a.length[i] + a.length[j]);
        let gap_s = ds - reach;
        if gap_s <= 0.0 {
            self.level_in_narrowing(i, j, gy, ds, edges, idm, free, acc, lead, sides);
            return;
        }
        let (ci, cj) = (usize::from(a.class[i]), usize::from(a.class[j]));
        let c_min = a.clearance_scale[i] * p.pair_clearance(ci, cj, 0.5 * (v + a.v[j])).1;
        let room = width + c_min;
        let (hwj, wj) = (a.half_width[j], a.width[j]);
        let passed = ds + reach;
        let within = |x: Option<f64>, limit: f64| x.is_some_and(|x| x <= limit);
        let closes = if self.is_left(i, j) {
            let (j_edge, my_edge) = (a.y[j] - hwj, y + 0.5 * width);
            within(edges.first(0.0, |lo, _| j_edge - room - lo), passed)
                || within(
                    edges.first(ds, |_, hi| hi - wj - my_edge - c_min),
                    p.lookahead,
                )
        } else {
            let (j_edge, my_edge) = (a.y[j] + hwj, y - 0.5 * width);
            within(edges.first(0.0, |_, hi| hi - j_edge - room), passed)
                || within(
                    edges.first(ds, |lo, _| my_edge - c_min - lo - wj),
                    p.lookahead,
                )
        } || within(edges.narrower(0.0, room + wj), passed);
        if !closes {
            return;
        }
        let leader = (free
            - idm.interaction(
                v,
                gap_s,
                v - a.v[j],
                p.mult_time_gap[ci][cj],
                p.mult_standstill_gap[ci][cj],
            ))
        .max(-p.emergency_decel);
        if leader < lead.0 {
            *lead = (leader, a.id[j]);
        }
        *acc = acc.min(leader);
    }

    /// A vehicle `j` level with `i`, with edge gap `gy`, where the
    /// carriageway narrows ahead, however far to the side it is; `j`'s
    /// centre is `ds` m ahead of `i`'s (negative: behind).
    ///
    /// The kerb may move `i` over by half of the gap, like any move. `i`
    /// brakes for the kerb where that would take it inside its minimum
    /// clearance of `j` (each keeps half of what the gap has above it).
    ///
    /// But if further on the two cannot be abreast (the road is narrower
    /// than both with that clearance, or the kerb leaves no way past the
    /// other where it is now), the one whose front is ahead goes first. It
    /// counts on the other waiting: it brakes for the kerb only where that
    /// would take it into the other's body before it is past. The other
    /// brakes so as not to be level with it where they cannot be abreast.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn level_in_narrowing(
        &self,
        i: usize,
        j: usize,
        gy: f64,
        ds: f64,
        edges: &EdgeAhead,
        idm: &Idm,
        free: f64,
        acc: &mut f64,
        lead: &mut (f64, u64),
        sides: &mut Sides,
    ) {
        let (a, p) = (self.a, self.p);
        let (y, v) = (a.y[i], a.v[i]);
        let (ci, cj) = (usize::from(a.class[i]), usize::from(a.class[j]));
        let c_min = a.clearance_scale[i] * p.pair_clearance(ci, cj, 0.5 * (v + a.v[j])).1;
        let left = self.is_left(i, j);
        sides.hard(left, y, gy);
        let spare = 0.5 * (gy - c_min).max(0.0);
        let (wi, wj) = (a.width[i], a.width[j]);
        let reach = 0.5 * (a.length[i] + a.length[j]);
        let ahead = ds + 0.5 * (a.length[j] - a.length[i]);
        let first = ahead < 0.0 || (ahead == 0.0 && a.id[i] < a.id[j]);
        let (j_edge, my_edge) = if left {
            (a.y[j] - a.half_width[j], y + 0.5 * wi)
        } else {
            (a.y[j] + a.half_width[j], y - 0.5 * wi)
        };
        // The kerb leaves the one that goes first no way past the other.
        let blocked = match (first, left) {
            (true, true) => edges.first(0.0, |lo, _| j_edge - wi - lo),
            (true, false) => edges.first(0.0, |_, hi| hi - j_edge - wi),
            (false, true) => edges.first(0.0, |_, hi| hi - my_edge - wj),
            (false, false) => edges.first(0.0, |lo, _| my_edge - wj - lo),
        };
        let nearest = |x: Option<f64>, z: Option<f64>| match (x, z) {
            (Some(x), Some(z)) => Some(x.min(z)),
            (x, z) => x.or(z),
        };
        let Some(pinch) =
            nearest(edges.narrower(0.0, wi + wj + c_min), blocked).filter(|&x| x <= p.lookahead)
        else {
            sides.clear(left, y, spare);
            return;
        };
        let (wait, leader) = if first {
            // Until we are past it, the kerb must not take us into it.
            let room = gy.max(0.0);
            let touch = if left {
                edges.first(0.0, |lo, _| y + room - 0.5 * wi - lo)
            } else {
                edges.first(0.0, |_, hi| hi - 0.5 * wi - (y - room))
            };
            let Some(touch) = touch.filter(|&x| x <= ds + reach) else {
                return;
            };
            let wait = if touch > 0.0 {
                (free - idm.interaction(v, touch, v, 1.0, 1.0)).max(-p.emergency_decel)
            } else {
                -p.emergency_decel
            };
            (wait, EDGE_LEADER)
        } else {
            sides.clear(left, y, spare);
            let room = pinch - reach;
            let wait = if room > 0.0 {
                (free - idm.interaction(v, room, v, 1.0, 1.0)).max(-p.emergency_decel)
            } else if nearest(edges.narrower(0.0, wi + wj), blocked).is_some_and(|x| x <= reach) {
                -p.emergency_decel
            } else {
                // Only the clearance is short: drop back without alarm.
                -idm.comfortable_decel
            };
            (wait, a.id[j])
        };
        if wait < lead.0 {
            *lead = (wait, leader);
        }
        *acc = acc.min(wait);
    }

    /// Gather the neighbours that matter to agent `i`'s lateral choice and
    /// choose its target (ADR-0010 §3).
    pub fn decide(&self, r: usize, k: usize, i: usize, idm: &Idm, cap: f64) -> Choice {
        self.decide_with(r, k, i, idm, cap, &mut Neighbours::default())
    }

    /// [`Self::decide`] with a neighbour list to fill (emptied first).
    fn decide_with(
        &self,
        r: usize,
        k: usize,
        i: usize,
        idm: &Idm,
        cap: f64,
        nb: &mut Neighbours,
    ) -> Choice {
        nb.clear();
        let (a, p) = (self.a, self.p);
        let ci = usize::from(a.class[i]);
        let (v, y, l, s) = (a.v[i], a.y[i], a.length[i], a.s[i]);
        let hw = 0.5 * a.width[i];
        let scale = a.clearance_scale[i];
        let squeeze = p.squeeze_scale.unwrap_or(scale);
        let free = idm.free_accel(v);
        let range = p.candidate_range;
        // Limits from side neighbours (the squeezing rule).
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        let mut beside =
            |nb: &mut Neighbours, left: bool, yj: f64, hwj: f64, c_des0: f64, c_min0: f64| {
                nb.push_side(SideTerm {
                    y: yj,
                    half_width: hwj,
                    c_min: scale * c_min0,
                    c_des: scale * c_des0,
                });
                let room = hwj + hw + squeeze * c_min0;
                if left {
                    hi = hi.min(yj - room);
                } else {
                    lo = lo.max(yj + room);
                }
            };
        let mut side = |nb: &mut Neighbours, j: usize, c_des0: f64, c_min0: f64| {
            let left = self.is_left(i, j);
            beside(nb, left, a.y[j], a.half_width[j], c_des0, c_min0);
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
                    side(nb, j, c_des0, c_min0);
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
                    side(nb, j, c_des0, c_min0);
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
        // Road objects: leaders ahead, side neighbours when level.
        let mut full = false;
        if self.objects.any_active() {
            let near = |o: &crate::objects::RoadObject, ds: f64| {
                if !o.blocks[ci] || full {
                    return;
                }
                let half_o = 0.5 * o.width;
                let nearest = ((o.y - y).abs() - range).max(0.0) - hw - half_o;
                let (c_des0, c_min0) = p.object_clearance(ci, o.clearance_class, 0.5 * v);
                let (c_des, c_min) = (scale * c_des0, scale * c_min0);
                if nearest > c_des.max(c_min + self.cut) {
                    return;
                }
                let gap_s = ds.abs() - 0.5 * (l + o.length);
                if gap_s <= 0.0 {
                    if edge_gap(y, hw, o.y, half_o) >= 0.0 {
                        beside(nb, o.y >= y, o.y, half_o, c_des0, c_min0);
                    }
                    return;
                }
                if ds < 0.0 {
                    return;
                }
                let (mt, ms) = p.object_multipliers(ci, o.clearance_class);
                let cost = idm.interaction(v, gap_s, v, mt, ms);
                let near = gap_s < idm.standstill_gap * ms + v * idm.time_gap * mt;
                if !near && cost < p.switch_threshold {
                    return;
                }
                full = !nb.push_leader(LeaderTerm {
                    y: o.y,
                    half_width: half_o,
                    accel: (free - cost).max(-p.emergency_decel),
                    c_min,
                    c_des,
                    near,
                });
            };
            self.objects
                .near(self.net, self.prev, r, s, 0.5 * l, p.lookahead, near);
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
            // Where the carriageway narrows ahead, a line that runs into
            // the kerb is worth what following a standing obstacle there is.
            wall: self.narrow.within(r, s, p.lookahead).then(|| Wall {
                edges: EdgeAhead::gather(self.net, r, s, p.lookahead),
                desired_gap: idm.desired_gap(v, v, 1.0, 1.0),
                max_accel: idm.max_accel,
                floor: -p.emergency_decel,
                reach: p.lookahead,
            }),
        };
        choose_target(&self.choice, &me, nb)
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
