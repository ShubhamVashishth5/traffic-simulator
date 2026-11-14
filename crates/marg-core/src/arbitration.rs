//! Who goes first between two vehicles whose paths through a junction meet
//! (ADR-0016 part 4, decision D of 2026-11-13).
//!
//! Once per step, before the move phase, every pair of vehicles at a
//! junction whose movement ribbons overlap is compared along the paths they
//! will drive: each vehicle's centre, at its present lateral offset, along
//! the ribbons inside the junction it will take. Where those two paths come
//! within the two half-widths and the pair's minimum clearance is the
//! *contact* of the pair. Then:
//!
//! 1. it is decided who goes first as soon as either vehicle comes within
//!    its stopping distance and desired gap of the contact, while the one
//!    that gives way can still stop short of it;
//! 2. the decision is kept until the contact is gone (one of them is
//!    through), so the two cannot change their minds into each other;
//! 3. of two vehicles from one arm whose paths cross, the one behind waits
//!    at the stop line.
//!
//! The one that gives way stops short of the contact (paths that cross) or
//! follows the other as if it were already ahead on the shared path (paths
//! that merge). Rule 1 of decision D (nobody drives into a body lying
//! across its path) is in the move phase ([`crate::junction_scan`]).
//!
//! This pass runs on one thread in a fixed order and writes only its own
//! state, so the move phase that reads it stays deterministic (ADR-0003).

use crate::agents::AgentStore;
use crate::junction_scan::{arrive, body_ahead};
use crate::junctions::{LIGHT_GREEN, Member, onward};
use crate::params::CoreParams;
use marg_net::{ConflictKind, Control, Network};
use marg_params::spec::JunctionKind;

/// Spacing of the points at which two paths are compared (m). A numerical
/// resolution, not behaviour: it is finer than any vehicle's width.
const PATH_STEP: f64 = 0.5;
/// Two paths from one arm that start apart count as meeting only if they
/// come this much closer than they start (m): numerical slack, so that
/// side-by-side vehicles whose paths part are never taken to meet.
const PARTING_SLACK: f64 = 1e-6;
/// Ribbons inside one junction that a vehicle's path is followed through.
const PATH_RIBBONS: usize = 4;
/// Marker: the agent is not at any junction's arbitration.
pub const NO_SCOPE: u32 = u32::MAX;
/// [`Yield::goer`] of a hold short of a priority stream's carriageway: the
/// vehicle waits there for a gap in the stream, not for one vehicle.
pub const STREAM: u32 = u32::MAX;
const NO_CONFLICT: u32 = u32::MAX;

/// What one vehicle must do for another this step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Yield {
    /// The agent that gives way, and the one it gives way to (indices;
    /// [`STREAM`]: a priority stream it waits for a gap in).
    pub i: u32,
    pub goer: u32,
    /// Gap (m) from its front to where it must stop, or, at a merge, to the
    /// other's rear placed on its own path.
    pub gap: f64,
    /// Speed of what it brakes for: 0 for a place to stop at, the other
    /// vehicle's speed at a merge.
    pub v_lead: f64,
}

/// A decision taken at an earlier step and still in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Decision {
    yielder: u64,
    goer: u64,
}

/// A vehicle taking part in a junction's arbitration this step.
#[derive(Debug, Clone, Copy)]
struct Active {
    i: u32,
    id: u64,
    junction: u32,
    /// The arm it arrives by.
    arm: u32,
    /// Distance from its centre to its stop line (negative: past it).
    d_line: f64,
    /// The ribbons inside the junction ahead of it: `(ribbon, station its
    /// path starts at on it, path distance of that station, first sample,
    /// number of samples)`.
    path: [(u32, f64, f64, u32, u32); PATH_RIBBONS],
    n: usize,
    half_l: f64,
    half_w: f64,
    /// Its first ribbon inside the junction (its movement).
    first: u32,
    /// Its body as vehicles on other ribbons see it (`chord_pose`):
    /// centre, unit heading, half-length and half-width.
    pose: [f64; 4],
    body: (f64, f64),
    /// Samples of its path on the ribbon it leaves the junction by:
    /// `(first, count)` into the points.
    exit: (u32, u32),
    /// Under a signal, it makes a permitted movement and has not taken a
    /// gap yet: it gives way to the movements with priority over it.
    gives_way: bool,
    /// Under a signal, it makes a permitted movement and has taken its gap.
    took_gap: bool,
    /// When it committed to the junction (NaN: not yet).
    ticket: f64,
}

/// Where two vehicles' paths meet: how far along each path (distance of its
/// centre from where it is now) the two first come closer than their
/// half-widths and clearance, and how the ribbons meet there.
#[derive(Debug, Clone, Copy)]
struct Contact {
    e_x: f64,
    e_z: f64,
    kind: ConflictKind,
}

/// Run-time state of the arbitration of every junction.
#[derive(Debug, Clone, Default)]
pub struct Arbiter {
    /// Per agent index: the junction whose arbitration it is in this step
    /// (on an arriving ribbon on its way to it, or inside it).
    pub scope: Vec<u32>,
    /// This step's constraints, sorted by the index of the agent that gives way.
    pub yields: Vec<Yield>,
    kept: Vec<Decision>,
    next: Vec<Decision>,
    active: Vec<Active>,
    /// This junction's decisions before cycles are broken, and scratch for
    /// finding the cycles.
    pending: Vec<Pending>,
    out_edges: Vec<Vec<usize>>,
    mark: Vec<u8>,
    stack: Vec<(usize, usize)>,
    path: Vec<usize>,
    /// Sampled path points `(x, y, path distance, station)`.
    points: Vec<[f64; 4]>,
    /// Per ribbon: its index among its junction's ribbons.
    local: Vec<u32>,
    /// Per junction: `table[a * n + b]`, the conflict between its ribbons
    /// `a` and `b` (local indices), as an index into its conflict list.
    table: Vec<Vec<u32>>,
    size: Vec<usize>,
}

impl Arbiter {
    pub fn new(net: &Network) -> Self {
        let mut local = vec![NO_CONFLICT; net.len()];
        let mut table = Vec::new();
        let mut size = Vec::new();
        for j in &net.junctions {
            let n = j.ribbons.len();
            for (k, &r) in j.ribbons.iter().enumerate() {
                local[r] = k as u32;
            }
            let mut t = vec![NO_CONFLICT; n * n];
            for (c, area) in j.conflicts.iter().enumerate() {
                let (a, b) = (local[area.a] as usize, local[area.b] as usize);
                t[a * n + b] = c as u32;
                t[b * n + a] = c as u32;
            }
            table.push(t);
            size.push(n);
        }
        Self {
            local,
            table,
            size,
            ..Self::default()
        }
    }

    /// The constraints on agent `i` this step.
    #[inline]
    pub fn yields_of(&self, i: usize) -> &[Yield] {
        let i = i as u32;
        let from = self.yields.partition_point(|y| y.i < i);
        let to = from + self.yields[from..].partition_point(|y| y.i == i);
        &self.yields[from..to]
    }

    /// Whether the vehicle with id `yielder` gives way to the one with id
    /// `goer` this step.
    #[inline]
    pub fn gives_way_to(&self, yielder: u64, goer: u64) -> bool {
        self.kept.binary_search(&Decision { yielder, goer }).is_ok()
    }

    /// Whether agents `i` and `k` are both at junction arbitration, at the
    /// same junction: who goes first between them is decided here, not in
    /// the move phase.
    #[inline]
    pub fn same_scope(&self, i: usize, k: usize) -> bool {
        let s = self.scope[i];
        s != NO_SCOPE && s == self.scope[k]
    }

    /// The junction whose arbitration agent `i` takes part in, if any.
    #[inline]
    pub fn scope_of(&self, i: usize) -> Option<usize> {
        let s = self.scope[i];
        (s != NO_SCOPE).then_some(s as usize)
    }

    /// Decide who goes first at every junction this step. `members[j]` are
    /// the vehicles near junction `j`, in ribbon and position order.
    pub fn run(
        &mut self,
        a: &AgentStore,
        members: &[Vec<Member>],
        net: &Network,
        p: &CoreParams,
        seed: u64,
        light: &[u8],
    ) {
        self.scope.clear();
        self.scope.resize(a.len(), NO_SCOPE);
        self.yields.clear();
        self.next.clear();
        for (j, group) in members.iter().enumerate().take(net.junctions.len()) {
            self.gather(j, group, a, net, p, seed, light);
            self.order_tickets(a, net.junctions[j].kind == JunctionKind::Roundabout);
            let n = self.active.len();
            for x in 0..n {
                for z in x + 1..n {
                    self.pair(x, z, a, net, p, light);
                }
            }
            self.break_cycles();
        }
        self.yields.sort_by_key(|y| y.i);
        std::mem::swap(&mut self.kept, &mut self.next);
        self.kept.sort_unstable();
    }

    /// The vehicles at junction `j` that take part this step, each with its
    /// path through the junction sampled.
    #[allow(clippy::too_many_arguments)]
    fn gather(
        &mut self,
        j: usize,
        group: &[Member],
        a: &AgentStore,
        net: &Network,
        p: &CoreParams,
        seed: u64,
        light: &[u8],
    ) {
        self.active.clear();
        self.points.clear();
        let signal = matches!(net.junctions[j].control, Control::Signal { .. });
        for m in group {
            let (i, r) = (m.i as usize, m.ribbon as usize);
            let rib = net.ribbon(r);
            if rib.junction != Some(j) {
                continue;
            }
            let (s, l) = (a.s[i], 0.5 * a.length[i]);
            let mut turn = (a.turn_junction[i], a.turn_out[i]);
            // Its first ribbon inside the junction, the station its path
            // starts at there, and the distance to that station.
            let (first, s0, d0, arm, d_line) = if rib.internal {
                let arm = net.junctions[j]
                    .movements
                    .iter()
                    .find(|mv| mv.first == r)
                    .map_or(u32::MAX, |mv| mv.from as u32);
                let arm = if arm == u32::MAX && a.came_from[i] != u32::MAX {
                    net.ribbon(a.came_from[i] as usize)
                        .arm_in
                        .map_or(u32::MAX, |x| x as u32)
                } else {
                    arm
                };
                (r, s, 0.0, arm, -s)
            } else if let Some(arm) = rib.arm_in {
                if a.turn_junction[i] as usize != j {
                    continue;
                }
                self.scope[i] = j as u32;
                let d_line = rib.length() - s;
                // Not about to enter: held at its line, or too far from it
                // to need to decide anything yet.
                if a.line_held[i] || d_line - l > horizon(a, i) {
                    continue;
                }
                let Some(first) = onward(net, seed, a.id[i], r, &mut turn) else {
                    continue;
                };
                (first, 0.0, d_line, arm as u32, d_line)
            } else {
                continue;
            };
            self.scope[i] = j as u32;
            let mut act = Active {
                i: m.i,
                id: a.id[i],
                junction: j as u32,
                arm,
                d_line,
                path: [(0, 0.0, 0.0, 0, 0); PATH_RIBBONS],
                n: 0,
                half_l: l,
                half_w: a.half_width[i],
                first: first as u32,
                gives_way: false,
                took_gap: false,
                ticket: a.box_ticket[i],
                pose: [m.x, m.y, m.hx, m.hy],
                body: (l, a.half_width[i]),
                exit: (0, 0),
            };
            let (mut ribbon, mut start, mut dist) = (first, s0, d0);
            while act.n < PATH_RIBBONS {
                let rb = net.ribbon(ribbon);
                if !rb.internal || rb.junction != Some(j) {
                    break;
                }
                let from = self.points.len() as u32;
                let len = rb.length();
                let mut st = start;
                while st <= len {
                    let (x, yw, _) = rb.to_world(st, a.y[i]);
                    self.points.push([x, yw, dist + st - start, st]);
                    st += PATH_STEP;
                }
                let count = self.points.len() as u32 - from;
                act.path[act.n] = (ribbon as u32, start, dist, from, count);
                act.n += 1;
                dist += len - start;
                if dist > p.lookahead {
                    break;
                }
                match onward(net, seed, a.id[i], ribbon, &mut turn) {
                    Some(nx) => {
                        ribbon = nx;
                        start = 0.0;
                    }
                    None => break,
                }
            }
            // Its way out: the start of the ribbon it leaves by, as far as
            // its own length and standstill gap (where it would stand if
            // it had to stop just beyond the junction).
            let rb = net.ribbon(ribbon);
            if act.n > 0 && !rb.internal {
                let from = self.points.len() as u32;
                let reach = a.length[i] + a.standstill_gap[i];
                let mut st = 0.0;
                while st <= reach.min(rb.length()) {
                    let (x, yw, _) = rb.to_world(st, a.y[i]);
                    self.points.push([x, yw, dist + st, st]);
                    st += PATH_STEP;
                }
                act.exit = (from, self.points.len() as u32 - from);
            }
            if act.n > 0 {
                // Its body lies inside a curve by up to L^2 / 8 rho (the
                // chord of `chord_pose`): its corridor is that much wider.
                let rho = act.path[..act.n]
                    .iter()
                    .map(|q| net.ribbon(q.0 as usize).min_radius)
                    .fold(f64::INFINITY, f64::min);
                if rho.is_finite() && rho > 0.0 {
                    act.half_w += a.length[i] * a.length[i] / (8.0 * rho);
                }
                // A permitted movement under a signal waits for its gap
                // inside the junction, short of the priority stream's
                // carriageway (ADR-0016, decision of 2026-11-13).
                let mut hold = None;
                if signal && a.gate_open[i] {
                    // A permitted movement that took its gap: whether it
                    // is one is whether anything has priority over it.
                    act.took_gap = net.ribbon(first).rank > 1;
                }
                if signal && !a.gate_open[i] {
                    hold = self.hold(&act, j, net, light);
                    if let Some(d) = hold {
                        act.gives_way = true;
                        if d - act.half_l > 0.0 {
                            self.yields.push(Yield {
                                i: act.i,
                                goer: STREAM,
                                gap: d - act.half_l,
                                v_lead: 0.0,
                            });
                        }
                    }
                }
                // It does not cross its stop line unless its way through
                // the junction is clear: entering to stand in the box is
                // the `box_blocking` violation of P8 (ADR-0016). A vehicle
                // that waits for a gap inside needs its way clear only as
                // far as where it waits. One that can no longer stop
                // before the line goes on.
                let to_line = act.d_line - act.half_l;
                if to_line > 0.0 && a.v[i] * a.v[i] <= 2.0 * p.safe_decel * to_line {
                    let through = hold.unwrap_or(f64::INFINITY);
                    if let Some(b) = self.blocker(&act, group, a, net, p, through) {
                        self.yields.push(Yield {
                            i: act.i,
                            goer: b,
                            gap: to_line,
                            v_lead: 0.0,
                        });
                        continue;
                    }
                }
                self.active.push(act);
            }
        }
    }

    /// A vehicle standing in the junction, or just beyond it on the way
    /// out, whose body lies across `x`'s way within `through` m of `x`'s
    /// centre (its whole way through, if infinite): `x` would have to
    /// stop in the junction behind it.
    fn blocker(
        &self,
        x: &Active,
        group: &[Member],
        a: &AgentStore,
        net: &Network,
        p: &CoreParams,
        through: f64,
    ) -> Option<u32> {
        let i = x.i as usize;
        let ci = usize::from(a.class[i]);
        let j = x.junction as usize;
        for m in group {
            let k = m.i as usize;
            if k == i || a.v[k] >= p.stop_speed_below {
                continue;
            }
            let rk = net.ribbon(m.ribbon as usize);
            // Inside the junction, or on a ribbon that leaves it.
            let inside = rk.internal && rk.junction == Some(j);
            if !inside && (rk.arm_in.is_some() || rk.internal) {
                continue;
            }
            let ck = usize::from(a.class[k]);
            let clear = 0.5
                * (a.clearance_scale[i] + a.clearance_scale[k])
                * p.pair_clearance(ci, ck, 0.5 * a.v[i]).1;
            let (hl, hw) = (0.5 * a.length[k], a.half_width[k]);
            let reach = x.half_w + clear;
            let hit = x.path[..x.n]
                .iter()
                .flat_map(|q| (q.3..q.3 + q.4).map(|n| self.points[n as usize]))
                .chain((x.exit.0..x.exit.0 + x.exit.1).map(|n| self.points[n as usize]))
                .take_while(|q| q[2] <= through)
                .any(|q| {
                    let (dx, dy) = (q[0] - m.x, q[1] - m.y);
                    let u = (dx * m.hx + dy * m.hy).abs() - hl;
                    let w = (m.hx * dy - m.hy * dx).abs() - hw;
                    u.max(0.0).hypot(w.max(0.0)) <= reach
                });
            if hit {
                return Some(m.i);
            }
        }
        None
    }

    /// A vehicle cannot go before one ahead of it on its own movement that
    /// is in its way: its ticket is no earlier than that one's, and if that
    /// one has not committed, neither has it. Without this, the order of
    /// tickets and the order of a queue could disagree and form a ring.
    fn order_tickets(&mut self, a: &AgentStore, ring: bool) {
        let n = self.active.len();
        // Inside the junction, front to back along each ribbon.
        self.mark.clear();
        self.stack.clear();
        for x in 0..n {
            if self.active[x].d_line <= 0.0 {
                self.stack.push((x, 0));
            }
        }
        let key = |q: &Active| (q.first, -a.s[q.i as usize]);
        self.stack.sort_by(|p, q| {
            let (kp, kq) = (key(&self.active[p.0]), key(&self.active[q.0]));
            kp.0.cmp(&kq.0).then(kp.1.total_cmp(&kq.1))
        });
        for x in 0..self.stack.len() {
            let ix = self.stack[x].0;
            let mut eff = self.active[ix].ticket;
            for z in 0..x {
                let iz = self.stack[z].0;
                let (ax, az) = (&self.active[ix], &self.active[iz]);
                if az.first != ax.first {
                    continue;
                }
                let (yx, yz) = (a.y[ax.i as usize], a.y[az.i as usize]);
                if (yx - yz).abs() < ax.body.1 + az.body.1 {
                    // Not committed (NaN) is later than any ticket.
                    eff = if eff.is_nan() || az.ticket.is_nan() {
                        f64::NAN
                    } else {
                        eff.max(az.ticket)
                    };
                }
            }
            self.active[ix].ticket = eff;
        }
        // On a ring, vehicles from different entries share its arcs: one is
        // no earlier than any vehicle ahead of it on its path that is in its
        // way, wherever that one came from (chains: to a fixed point).
        if ring {
            let m = self.stack.len();
            for _ in 0..m {
                let mut changed = false;
                for x in 0..m {
                    let ix = self.stack[x].0;
                    for z in 0..m {
                        let iz = self.stack[z].0;
                        if ix == iz {
                            continue;
                        }
                        let (ax, az) = (&self.active[ix], &self.active[iz]);
                        let (i, k) = (ax.i as usize, az.i as usize);
                        let ahead = ax.path[1..ax.n].iter().any(|q| q.0 == az.first)
                            || (ax.first == az.first && (a.s[k], a.id[k]) > (a.s[i], a.id[i]));
                        if !ahead || (a.y[i] - a.y[k]).abs() >= ax.body.1 + az.body.1 {
                            continue;
                        }
                        let eff = if ax.ticket.is_nan() || az.ticket.is_nan() {
                            f64::NAN
                        } else {
                            ax.ticket.max(az.ticket)
                        };
                        if !(eff == ax.ticket || (eff.is_nan() && ax.ticket.is_nan())) {
                            self.active[ix].ticket = eff;
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }
    }

    /// How far along its path vehicle `x`'s body first reaches the
    /// carriageway of a movement that has green and priority over its own
    /// (a lower rank): where it waits for a gap. `None`: it gives way to
    /// nothing.
    fn hold(&self, x: &Active, j: usize, net: &Network, light: &[u8]) -> Option<f64> {
        let n = self.size[j];
        let jn = &net.junctions[j];
        let rank = net.ribbon(x.first as usize).rank;
        let mut best: Option<f64> = None;
        for &(ra, _, _, fa, na) in &x.path[..x.n] {
            let la = self.local[ra as usize] as usize;
            for lb in 0..n {
                let c = self.table[j][la * n + lb];
                if c == NO_CONFLICT {
                    continue;
                }
                let area = &jn.conflicts[c as usize];
                let other = if area.a == ra as usize {
                    area.b
                } else {
                    area.a
                };
                let rc = net.ribbon(other);
                if area.kind == ConflictKind::Diverge
                    || rc.rank >= rank
                    || light[other] != LIGHT_GREEN
                {
                    continue;
                }
                let len = rc.length();
                let pa = &self.points[fa as usize..(fa + na) as usize];
                let first = pa.iter().find(|q| {
                    let (sq, yq, _) = rc.nearest(q[0], q[1]);
                    let (lo, hi) = rc.y_bounds(sq);
                    sq > 0.0 && sq < len && yq > lo - x.half_w && yq < hi + x.half_w
                });
                if let Some(q) = first {
                    best = Some(best.map_or(q[2], |b: f64| b.min(q[2])));
                }
            }
        }
        best
    }

    /// Whether `k`'s body lies across `i`'s way, touching or within `reach`
    /// of its front: the view of rule 1 in the move phase, from the same
    /// poses, so that the two never disagree about who can move.
    fn blocks(k: &Active, i: &Active, reach: f64) -> bool {
        let [xi, yi, hx, hy] = i.pose;
        let [xk, yk, kx, ky] = k.pose;
        let (dx, dy) = (xk - xi, yk - yi);
        let along = dx * hx + dy * hy;
        let across = hx * dy - hy * dx;
        let cos = kx * hx + ky * hy;
        let sin = hx * ky - hy * kx;
        body_ahead(
            (along, across),
            (cos, sin),
            k.body,
            (-i.body.1, i.body.1),
            i.body.0,
        )
        .is_some_and(|g| g <= reach)
    }

    /// Where the paths of active vehicles `x` and `z` meet, if they do.
    fn contact(&self, x: &Active, z: &Active, r2: f64, net: &Network) -> Option<Contact> {
        let j = x.junction as usize;
        let n = self.size[j];
        // A ribbon on both paths: if one is already on a ribbon the other
        // will drive, it is ahead and the other follows it. Otherwise they
        // come together onto it (a merge onto a ring): only what lies before
        // it can bring them into contact.
        let (px, pz) = (&x.path[..x.n], &z.path[..z.n]);
        let shared = |p: &[(u32, f64, f64, u32, u32)], q: &[(u32, f64, f64, u32, u32)]| {
            p.iter()
                .position(|a| q.iter().any(|b| b.0 == a.0))
                .unwrap_or(p.len())
        };
        let (ix, iz) = (shared(px, pz), shared(pz, px));
        if ix == 0 || iz == 0 {
            return None;
        }
        let mut best: Option<Contact> = None;
        for &(ra, _, _, fa, na) in &px[..ix] {
            for &(rb, _, _, fb, nb) in &pz[..iz] {
                let (la, lb) = (self.local[ra as usize], self.local[rb as usize]);
                let c = self.table[j][la as usize * n + lb as usize];
                if c == NO_CONFLICT {
                    continue;
                }
                let area = &net.junctions[j].conflicts[c as usize];
                let (sa, sb) = if area.a == ra as usize {
                    (area.a_s, area.b_s)
                } else {
                    (area.b_s, area.a_s)
                };
                let pa = &self.points[fa as usize..(fa + na) as usize];
                let pb = &self.points[fb as usize..(fb + nb) as usize];
                // Two vehicles from one arm that start apart meet only if
                // their paths come closer than they start.
                let mut lim = r2;
                if area.kind == ConflictKind::Diverge
                    && let (Some(p0), Some(q0)) = (pa.first(), pb.first())
                {
                    let d2 = (p0[0] - q0[0]).powi(2) + (p0[1] - q0[1]).powi(2);
                    let start = (d2.sqrt() - PARTING_SLACK).max(0.0);
                    lim = lim.min(start * start);
                }
                let (mut e_x, mut e_z) = (f64::INFINITY, f64::INFINITY);
                for q in pa.iter().filter(|q| q[3] >= sa.0 && q[3] <= sa.1) {
                    for w in pb.iter().filter(|w| w[3] >= sb.0 && w[3] <= sb.1) {
                        let d2 = (q[0] - w[0]).powi(2) + (q[1] - w[1]).powi(2);
                        if d2 < lim {
                            e_x = e_x.min(q[2]);
                            e_z = e_z.min(w[2]);
                        }
                    }
                }
                if e_x.is_finite() && best.is_none_or(|b| e_x < b.e_x) {
                    best = Some(Contact {
                        e_x,
                        e_z,
                        kind: area.kind,
                    });
                }
            }
        }
        best
    }

    /// Compare active vehicles `x` and `z`, decide who goes first if it is
    /// time to, and record what the other must do.
    fn pair(
        &mut self,
        x: usize,
        z: usize,
        a: &AgentStore,
        net: &Network,
        p: &CoreParams,
        light: &[u8],
    ) {
        let (ax, az) = (self.active[x], self.active[z]);
        let (i, k) = (ax.i as usize, az.i as usize);
        let (ci, ck) = (usize::from(a.class[i]), usize::from(a.class[k]));
        let v_mean = 0.5 * (a.v[i] + a.v[k]);
        let clearance = 0.5
            * (a.clearance_scale[i] + a.clearance_scale[k])
            * p.pair_clearance(ci, ck, v_mean)
                .1
                .min(p.pair_clearance(ck, ci, v_mean).1);
        let r = ax.half_w + az.half_w + clearance;
        let Some(c) = self.contact(&ax, &az, r * r, net) else {
            return;
        };
        // Distance from each front to the contact (<= 0: in it).
        let (dx, dz) = (c.e_x - ax.half_l, c.e_z - az.half_l);
        let stop = |n: usize| a.v[n] * a.v[n] / (2.0 * a.comfortable_decel[n]);
        let (stop_x, stop_z) = (stop(i), stop(k));
        // The same at the safe braking limit: what a driver can still do.
        let hard = |n: usize| a.v[n] * a.v[n] / (2.0 * p.safe_decel);
        let (hard_x, hard_z) = (hard(i), hard(k));
        let find = |y: u64, g: u64| {
            self.kept
                .binary_search(&Decision {
                    yielder: y,
                    goer: g,
                })
                .is_ok()
        };
        let kept = if find(az.id, ax.id) {
            Some(true)
        } else if find(ax.id, az.id) {
            Some(false)
        } else {
            None
        };
        let same_arm = ax.arm == az.arm && ax.arm != u32::MAX;
        // A permitted movement that has not taken a gap gives way to a
        // movement with priority over it, unless it can no longer stop
        // short and the other can.
        let gives_way = |g: &Active, o: &Active| {
            g.gives_way
                && c.kind != ConflictKind::Diverge
                && net.ribbon(g.first as usize).rank > net.ribbon(o.first as usize).rank
                && (light[o.first as usize] == LIGHT_GREEN || o.d_line <= 0.0)
        };
        // ... and once it has taken its gap, the movement it gave way to
        // sees it as a leader and brakes for it (plan/03-model-design.md
        // §4), unless that one can no longer stop short and it can.
        let took_gap = |g: &Active, o: &Active| {
            g.took_gap
                && c.kind != ConflictKind::Diverge
                && net.ribbon(g.first as usize).rank > net.ribbon(o.first as usize).rank
        };
        // `x` goes: `z` gives way to it, or `x` took its gap; and `z` can
        // still stop short or `x` cannot. The same the other way round.
        let x_first = (gives_way(&az, &ax) || took_gap(&ax, &az)) && !(hard_z >= dz && hard_x < dx);
        let z_first = (gives_way(&ax, &az) || took_gap(&az, &ax)) && !(hard_x >= dx && hard_z < dz);
        let by_rank = if z_first {
            Some(false)
        } else if x_first {
            Some(true)
        } else {
            None
        };
        // `Some(true)`: x goes first. Inside the junction the order is the
        // order in which they committed to it (first come, first served):
        // it was decided when the later one could still stop at its line,
        // it never changes, and it cannot run in a circle.
        let (tx, tz) = (ax.ticket, az.ticket);
        // On a ring every vehicle is inside the junction and paths are long:
        // the order of tickets decides a pair only once one of them is near
        // the contact, as two that have not reached their lines are decided.
        let far = dx > horizon(a, i) && dz > horizon(a, k);
        if far && net.junctions[ax.junction as usize].kind == JunctionKind::Roundabout {
            return;
        }
        let physics = |first: bool| {
            // The one that is to wait can no longer stop short and the
            // other still can: the other waits.
            if first && hard_z >= dz && hard_x < dx {
                false
            } else if !first && hard_x >= dx && hard_z < dz {
                true
            } else {
                first
            }
        };
        let x_goes = if !tx.is_nan() || !tz.is_nan() {
            let first = match (tx.is_nan(), tz.is_nan()) {
                (false, false) => tx < tz || (tx == tz && ax.id < az.id),
                (false, true) => true,
                _ => false,
            };
            Some(physics(first))
        } else if kept.is_some() {
            // Neither has committed yet: a decision stands until one of
            // them is through, even one the priority rule would not make.
            kept
        } else if by_rank.is_some() {
            by_rank
        } else if dx > horizon(a, i) && dz > horizon(a, k) {
            None
        } else {
            // Of two from one arm, the one behind at the stop line waits
            // for the other; otherwise the one that would get there first
            // goes ...
            let first = if same_arm && (ax.d_line > 0.0 || az.d_line > 0.0) {
                let (fx, fz) = (ax.d_line - ax.half_l, az.d_line - az.half_l);
                fx < fz || (fx == fz && ax.id < az.id)
            } else {
                let tx = arrive(a.v[i], a.max_accel[i], dx);
                let tz = arrive(a.v[k], a.max_accel[k], dz);
                tx < tz || (tx == tz && ax.id < az.id)
            };
            // ... unless the one that would give way can no longer stop
            // short and the other still can: comfortably, or else at the
            // safe braking limit.
            let (mut can_x, mut can_z) = (stop_x < dx, stop_z < dz);
            if can_x == can_z {
                (can_x, can_z) = (hard_x < dx, hard_z < dz);
            }
            Some(if first && !can_z && can_x {
                false
            } else if !first && !can_x && can_z {
                true
            } else {
                first
            })
        };
        let Some(mut x_goes) = x_goes else {
            return;
        };
        // Never send on a vehicle whose way is blocked by the body of the
        // one that was to wait for it, when that one's way is not.
        // Within the gap it would keep to a leader at its speed.
        let reach = |n: usize, o: usize| {
            let (cn, co) = (usize::from(a.class[n]), usize::from(a.class[o]));
            a.standstill_gap[n] * p.mult_standstill_gap[cn][co]
                + a.v[n] * a.time_gap[n] * p.mult_time_gap[cn][co]
                + PATH_STEP
        };
        // Blocked as the move phase saw it at the last step, too: on a
        // curve the straight view of `blocks` and the move phase's view
        // along the path can differ, and they must not disagree.
        // (Built for the ring of ADR-0009; boxes keep the straight view.)
        let ring = net.junctions[ax.junction as usize].kind == JunctionKind::Roundabout;
        let x_blocked = Self::blocks(&az, &ax, reach(i, k)) || (ring && a.blocker[i] == az.id);
        let z_blocked = Self::blocks(&ax, &az, reach(k, i)) || (ring && a.blocker[k] == ax.id);
        if x_goes && x_blocked && !z_blocked {
            x_goes = false;
        } else if !x_goes && z_blocked && !x_blocked {
            x_goes = true;
        }
        // What each would have to do if it were the one to give way: kept
        // both, so that a cycle of vehicles waiting for one another can be
        // broken by turning one decision round (`break_cycles`).
        let constraint =
            |yi: usize, gi: usize, yd: f64, e_y: f64, e_g: f64, act_y: &Active, half_g: f64| {
                let (gap, v_lead) = match c.kind {
                    ConflictKind::Merge if (e_y - e_g) - act_y.half_l - half_g > 0.0 => {
                        // As if the other were already ahead on the shared path.
                        ((e_y - e_g) - act_y.half_l - half_g, a.v[gi])
                    }
                    _ => {
                        // Short of the contact; one from its own arm waits at the line.
                        let mut gap = yd;
                        if same_arm && act_y.d_line - act_y.half_l > 0.0 {
                            gap = gap.min(act_y.d_line - act_y.half_l);
                        }
                        (gap, 0.0)
                    }
                };
                Yield {
                    i: yi as u32,
                    goer: gi as u32,
                    gap,
                    v_lead,
                }
            };
        let x_yields = constraint(i, k, dx, c.e_x, c.e_z, &ax, az.half_l);
        let z_yields = constraint(k, i, dz, c.e_z, c.e_x, &az, ax.half_l);
        // Priority, then room to the contact: which decision to turn round
        // first if this pair is in a cycle.
        let rank = |q: &Active| net.ribbon(q.first as usize).rank;
        self.pending.push(Pending {
            x: x as u32,
            z: z as u32,
            x_goes,
            fixed: by_rank.is_some(),
            x_yields,
            z_yields,
            release: [(rank(&ax), -dx, ax.id), (rank(&az), -dz, az.id)],
            blocked: [x_blocked, z_blocked],
        });
    }

    /// Turn round one decision in every cycle of vehicles that wait for one
    /// another at this junction, then record what remains. Without this,
    /// decisions kept from step to step could hold such a ring for ever.
    fn break_cycles(&mut self) {
        let n = self.active.len();
        // Who each waits for: adjacency over the pending decisions.
        loop {
            for list in &mut self.out_edges {
                list.clear();
            }
            if self.out_edges.len() < n {
                self.out_edges.resize(n, Vec::new());
            }
            for (e, d) in self.pending.iter().enumerate() {
                let (y, _) = d.ends();
                self.out_edges[y].push(e);
            }
            let Some(cycle) = self.find_cycle(n) else {
                break;
            };
            // Release the yielder that goes first by priority, then by
            // room to its contact, then by id; decisions fixed by priority
            // are turned round only if nothing else in the ring can be.
            let order = |p: &usize, q: &usize| {
                self.pending[*p]
                    .yielder_key()
                    .total_cmp_key(&self.pending[*q].yielder_key())
            };
            let free = |e: &usize| self.pending[*e].releasable();
            let pick = cycle
                .iter()
                .copied()
                .filter(|e| free(e) && !self.pending[*e].fixed)
                .min_by(order)
                .or_else(|| cycle.iter().copied().filter(free).min_by(order))
                .or_else(|| cycle.iter().copied().min_by(order));
            let Some(e) = pick else { break };
            let d = &mut self.pending[e];
            d.x_goes = !d.x_goes;
            d.fixed = true;
        }
        for d in self.pending.drain(..) {
            let (ax, az) = (self.active[d.x as usize], self.active[d.z as usize]);
            let (yielder, goer, y) = if d.x_goes {
                (az.id, ax.id, d.z_yields)
            } else {
                (ax.id, az.id, d.x_yields)
            };
            self.next.push(Decision { yielder, goer });
            self.yields.push(y);
        }
    }

    /// A cycle in the graph of who waits for whom, as the indices of its
    /// decisions, if there is one (depth-first, in a fixed order).
    fn find_cycle(&mut self, n: usize) -> Option<Vec<usize>> {
        // 0: not seen, 1: on the current path, 2: done.
        self.mark.clear();
        self.mark.resize(n, 0);
        for root in 0..n {
            if self.mark[root] != 0 {
                continue;
            }
            // Stack of (vertex, next edge to try); edges taken so far.
            self.stack.clear();
            self.path.clear();
            self.stack.push((root, 0));
            self.mark[root] = 1;
            while let Some(&(v, next)) = self.stack.last() {
                if next < self.out_edges[v].len() {
                    let e = self.out_edges[v][next];
                    if let Some(top) = self.stack.last_mut() {
                        top.1 += 1;
                    }
                    let (_, w) = self.pending[e].ends();
                    match self.mark[w] {
                        0 => {
                            self.mark[w] = 1;
                            self.path.push(e);
                            self.stack.push((w, 0));
                        }
                        1 => {
                            // Back to a vehicle on the path: the ring is the
                            // edges from the one that leaves `w` to `e`.
                            let from = self
                                .path
                                .iter()
                                .position(|&p| self.pending[p].ends().0 == w)
                                .unwrap_or(self.path.len());
                            let mut ring = self.path[from..].to_vec();
                            ring.push(e);
                            return Some(ring);
                        }
                        _ => {}
                    }
                } else {
                    self.mark[v] = 2;
                    self.stack.pop();
                    self.path.pop();
                }
            }
        }
        None
    }
}

/// A decision of this step, before cycles are broken.
#[derive(Debug, Clone, Copy)]
struct Pending {
    /// The pair, as indices into the active list.
    x: u32,
    z: u32,
    x_goes: bool,
    /// Decided by priority (or already turned round once this step).
    fixed: bool,
    x_yields: Yield,
    z_yields: Yield,
    /// Per vehicle: `(rank of its movement, minus its room to the contact, id)`.
    release: [(u8, f64, u64); 2],
    /// Per vehicle: the other's body lies across its way.
    blocked: [bool; 2],
}

impl Pending {
    /// `(the one that gives way, the one it waits for)`, as active indices.
    #[inline]
    fn ends(&self) -> (usize, usize) {
        if self.x_goes {
            (self.z as usize, self.x as usize)
        } else {
            (self.x as usize, self.z as usize)
        }
    }

    /// Whether the one that gives way could move if released: the other's
    /// body is not across its way.
    #[inline]
    fn releasable(&self) -> bool {
        !self.blocked[usize::from(self.x_goes)]
    }

    /// The order in which yielders are released from a cycle: by priority,
    /// then by room to the contact (more first), then by id.
    #[inline]
    fn yielder_key(&self) -> (u8, f64, u64) {
        self.release[usize::from(self.x_goes)]
    }
}

/// Ordering of release keys (`f64` compared totally, so the order is fixed).
trait KeyCmp {
    fn total_cmp_key(&self, other: &Self) -> std::cmp::Ordering;
}

impl KeyCmp for (u8, f64, u64) {
    fn total_cmp_key(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .cmp(&other.0)
            .then(self.1.total_cmp(&other.1))
            .then(self.2.cmp(&other.2))
    }
}

/// How far from a contact a vehicle needs to know who goes first: its
/// stopping distance at its comfortable deceleration plus its desired gap
/// at its speed (the distance within which it decides at a stop line too).
#[inline]
fn horizon(a: &AgentStore, i: usize) -> f64 {
    let v = a.v[i];
    a.standstill_gap[i] + v * a.time_gap[i] + v * v / (2.0 * a.comfortable_decel[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(i: u32) -> Active {
        Active {
            i,
            id: u64::from(i),
            junction: 0,
            arm: u32::MAX,
            d_line: -1.0,
            path: [(0, 0.0, 0.0, 0, 0); PATH_RIBBONS],
            n: 0,
            half_l: 1.0,
            half_w: 0.5,
            first: 0,
            gives_way: false,
            took_gap: false,
            ticket: f64::NAN,
            pose: [0.0, 0.0, 1.0, 0.0],
            body: (1.0, 0.5),
            exit: (0, 0),
        }
    }

    fn decision(x: u32, z: u32, x_goes: bool, room: f64) -> Pending {
        let y = |i, g| Yield {
            i,
            goer: g,
            gap: 1.0,
            v_lead: 0.0,
        };
        Pending {
            x,
            z,
            x_goes,
            fixed: false,
            x_yields: y(x, z),
            z_yields: y(z, x),
            release: [(1, -room, u64::from(x)), (1, -room, u64::from(z))],
            blocked: [false, false],
        }
    }

    /// Three vehicles that each wait for the next: one decision is turned
    /// round, the rest stand, and the result has no cycle.
    #[test]
    fn a_ring_of_waiting_vehicles_is_broken_once() {
        // 0 waits for 1, 1 for 2, 2 for 0; vehicle 1 has the most room.
        let mut arb = Arbiter {
            active: (0..3).map(active).collect(),
            pending: vec![
                decision(0, 1, false, 2.0),
                decision(1, 2, false, 5.0),
                decision(2, 0, false, 3.0),
            ],
            ..Arbiter::default()
        };
        arb.break_cycles();
        assert_eq!(arb.next.len(), 3);
        // Vehicle 1 (most room to its contact) is released: 2 waits for it.
        assert!(arb.next.contains(&Decision {
            yielder: 2,
            goer: 1
        }));
        assert!(arb.next.contains(&Decision {
            yielder: 0,
            goer: 1
        }));
        assert!(arb.next.contains(&Decision {
            yielder: 2,
            goer: 0
        }));
        assert_eq!(arb.yields.len(), 3);
    }

    /// A vehicle whose way is blocked by the one it waits for is not the
    /// one released.
    #[test]
    fn a_blocked_vehicle_is_not_released_from_a_ring() {
        let mut d = decision(1, 2, false, 5.0);
        d.blocked = [true, false];
        let mut arb = Arbiter {
            active: (0..3).map(active).collect(),
            pending: vec![decision(0, 1, false, 2.0), d, decision(2, 0, false, 3.0)],
            ..Arbiter::default()
        };
        arb.break_cycles();
        // Not vehicle 1; of 0 and 2, vehicle 2 has more room.
        assert!(arb.next.contains(&Decision {
            yielder: 1,
            goer: 2
        }));
        assert!(arb.next.contains(&Decision {
            yielder: 0,
            goer: 2
        }));
    }

    /// Without a ring nothing changes.
    #[test]
    fn a_chain_is_left_alone() {
        let mut arb = Arbiter {
            active: (0..3).map(active).collect(),
            pending: vec![decision(0, 1, false, 2.0), decision(1, 2, false, 5.0)],
            ..Arbiter::default()
        };
        arb.break_cycles();
        assert!(arb.next.contains(&Decision {
            yielder: 0,
            goer: 1
        }));
        assert!(arb.next.contains(&Decision {
            yielder: 1,
            goer: 2
        }));
    }
}
