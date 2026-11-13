//! Lateral behaviour (ADR-0010): pair clearances, the lateral weight of a
//! leader, the choice of a lateral target, and the motion towards it.
//!
//! Two clearances come from Budhkar & Maurya (2017): the **desired** one
//! (their mean line), which agents prefer to keep, and the **minimum** (the
//! lower threshold of their residual), which braking and squeezing use.

use crate::objects::EdgeAhead;

/// Most leaders, followers or side neighbours one decision considers; the
/// nearest are kept.
pub const MAX_NEIGHBOURS: usize = 32;

/// Desired and minimum lateral clearance (m) between two vehicles at mean
/// pair speed `v_mean`: `desired = intercept + slope · v̄` and
/// `minimum = max(floor, desired − offset)`, never above the desired one.
#[inline]
pub fn clearances(
    intercept: f64,
    slope: f64,
    min_offset: f64,
    min_floor: f64,
    v_mean: f64,
) -> (f64, f64) {
    let desired = intercept + slope * v_mean;
    (desired, (desired - min_offset).max(min_floor).min(desired))
}

/// Lateral weight of a leader: 1 when the lateral gap is within the minimum
/// clearance, decaying as `exp(−d/λ)` beyond it (Kanagaraj & Treiber 2018).
#[inline]
pub fn leader_weight(excess: f64, braking_scale: f64) -> f64 {
    if excess <= 0.0 {
        1.0
    } else {
        libm::exp(-excess / braking_scale)
    }
}

/// How a vehicle sees a standing body ahead when it may steer round it
/// (ADR-0012 part 2, option B: the swing-out gap).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwingOut {
    /// The gap to brake for: the bumper gap less the distance the vehicle
    /// still needs, at the heading limit, to move clear of the body.
    pub gap: f64,
    /// Lateral gap beyond the minimum clearance when its front reaches the
    /// body (negative: still in its way). The leader weight is taken from it.
    /// When it is zero or more the vehicle will pass beside the body, and
    /// the caller keeps no standstill gap to it: that gap is for waiting
    /// behind something, not for passing it. A vehicle in the body's way
    /// that means to drive on must also never come so close that it could
    /// not steer clear moving forward: the caller brakes for the body at
    /// the gap left after the run-up it needs from where it is now (this
    /// function with the vehicle holding its line), with no standstill gap.
    pub excess: f64,
}

/// The swing-out gap of a vehicle centred at `y` with half width `half` and
/// lateral speed `vy`, tracking lateral target `target` with time constant
/// `time_constant`, `gap` m behind a standing body centred at `body_y` with
/// half width `body_half`, at speed `v` with maximum acceleration `max_accel`.
///
/// Where the vehicle will be across the road when its front reaches the
/// body: its critically damped tracking of the target ([`track`], without
/// the steering response) over the shortest time it could take to get there
/// (at full acceleration), no further than the heading limit allows over
/// that distance, within this
/// step's side limits `limits` and the carriageway `road` (bounds of its
/// centre). If that is clear of the body by the minimum clearance `c_min`,
/// the body is braked for by that lateral gap. If not, the vehicle keeps
/// room to swing out: it brakes for the body as if it were nearer by the
/// distance it would need to move clear on the nearer side that has room,
/// `Δy / tan θ_max`. If neither side has room it brakes for the body itself.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn swing_out(
    y: f64,
    half: f64,
    vy: f64,
    target: f64,
    limits: (f64, f64),
    road: (f64, f64),
    body_y: f64,
    body_half: f64,
    c_min: f64,
    gap: f64,
    v: f64,
    max_accel: f64,
    heading_tan: f64,
    time_constant: f64,
) -> SwingOut {
    let run = gap.max(0.0);
    let time = ((v * v + 2.0 * max_accel * run).sqrt() - v) / max_accel;
    // e(t) = (e0 + (vy0 + e0/τ)·t)·exp(−t/τ), with e = y − target.
    let (toward, x) = (target - y, time / time_constant);
    let decay = libm::exp(-x);
    let tracked = toward * (1.0 - (1.0 + x) * decay) + vy * time * decay;
    let shift = tracked.clamp(-run * heading_tan, run * heading_tan);
    let lo = limits.0.max(road.0).min(y);
    let hi = limits.1.min(road.1).max(y);
    let arrive = (y + shift).clamp(lo, hi);
    let excess = edge_gap(arrive, half, body_y, body_half) - c_min;
    let left = body_y + body_half + c_min + half;
    let right = body_y - body_half - c_min - half;
    let mut need = f64::INFINITY;
    if left <= road.1 {
        need = need.min((left - arrive).max(0.0));
    }
    if right >= road.0 {
        need = need.min((arrive - right).max(0.0));
    }
    if need.is_infinite() {
        need = 0.0;
    }
    SwingOut {
        gap: gap - need / heading_tan,
        excess,
    }
}

/// Edge-to-edge lateral gap between two bodies centred at `ya`, `yb` with
/// widths `wa`, `wb` (negative when they overlap laterally).
#[inline]
pub fn lateral_gap(ya: f64, wa: f64, yb: f64, wb: f64) -> f64 {
    (ya - yb).abs() - 0.5 * (wa + wb)
}

/// [`lateral_gap`] from half-widths.
#[inline]
pub fn edge_gap(ya: f64, half_a: f64, yb: f64, half_b: f64) -> f64 {
    (ya - yb).abs() - half_a - half_b
}

/// Half of a body's lateral extent when turned by a heading with tangent
/// `heading_tan`: `(w cos ψ + l sin|ψ|) / 2`.
#[inline]
pub fn body_half_width(width: f64, length: f64, heading_tan: f64) -> f64 {
    0.5 * (width + length * heading_tan.abs()) / (1.0 + heading_tan * heading_tan).sqrt()
}

/// How much further a body reaches along the road, at each end, when turned
/// by a heading with tangent `heading_tan`: `(l cos ψ + w sin|ψ|) / 2 − l / 2`
/// (negative for a slender body at a large angle).
#[inline]
pub fn body_stretch(width: f64, length: f64, heading_tan: f64) -> f64 {
    0.5 * (length + width * heading_tan.abs()) / (1.0 + heading_tan * heading_tan).sqrt()
        - 0.5 * length
}

/// How far a lateral gap `gap` falls short of the desired clearance, from 0
/// (at or beyond the desired clearance) to 1 (at or below the minimum).
#[inline]
pub fn shortfall(desired: f64, minimum: f64, gap: f64) -> f64 {
    if gap >= desired {
        0.0
    } else if gap <= minimum {
        1.0
    } else {
        (desired - gap) / (desired - minimum)
    }
}

/// Desired lateral speed away from a side neighbour whose lateral gap is
/// `excess` above the minimum clearance: `strength · exp(−excess/λ_steer)`.
#[inline]
pub fn steering_speed(excess: f64, strength: f64, steering_scale: f64) -> f64 {
    strength * libm::exp(-excess.max(0.0) / steering_scale)
}

/// Lateral speed after one step of critically damped tracking of `target`
/// with time constant `tau`, plus the steering response `repulsion`:
/// `dv_y/dt = (w0 − v_y)/(τ/2)`, `w0 = (y* − y)/(2τ) + repulsion`
/// (implicit Euler, stable for any `dt`).
#[inline]
pub fn track(vy: f64, y: f64, target: f64, repulsion: f64, tau: f64, dt: f64) -> f64 {
    let w0 = (target - y) / (2.0 * tau) + repulsion;
    let k = 2.0 * dt / tau;
    (vy + k * w0) / (1.0 + k)
}

/// A leader ahead of the deciding agent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LeaderTerm {
    pub y: f64,
    pub half_width: f64,
    /// The agent's acceleration if it followed this leader alone (bounded).
    pub accel: f64,
    /// The agent's clearances to this leader.
    pub c_min: f64,
    pub c_des: f64,
    /// Within the agent's desired gap: counts for comfort.
    pub near: bool,
}

/// A follower behind the deciding agent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FollowerTerm {
    pub y: f64,
    pub half_width: f64,
    /// The follower's free acceleration, its acceleration if it followed the
    /// deciding agent alone (bounded), and its acceleration now.
    pub free: f64,
    pub imposed: f64,
    pub acc_now: f64,
    /// The follower's minimum clearance to the agent (for its braking weight).
    pub c_min: f64,
    /// The agent's own clearances to the follower (for comfort).
    pub c_min_own: f64,
    pub c_des_own: f64,
    /// Within the follower's desired gap: counts for comfort.
    pub near: bool,
}

/// A vehicle level with the deciding agent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SideTerm {
    pub y: f64,
    pub half_width: f64,
    pub c_min: f64,
    pub c_des: f64,
}

/// The neighbours one decision considers (fixed capacity, no allocation).
#[derive(Debug, Clone)]
pub struct Neighbours {
    pub leaders: [LeaderTerm; MAX_NEIGHBOURS],
    pub n_leaders: usize,
    pub followers: [FollowerTerm; MAX_NEIGHBOURS],
    pub n_followers: usize,
    pub sides: [SideTerm; MAX_NEIGHBOURS],
    pub n_sides: usize,
}

impl Default for Neighbours {
    fn default() -> Self {
        Self {
            leaders: [LeaderTerm::default(); MAX_NEIGHBOURS],
            n_leaders: 0,
            followers: [FollowerTerm::default(); MAX_NEIGHBOURS],
            n_followers: 0,
            sides: [SideTerm::default(); MAX_NEIGHBOURS],
            n_sides: 0,
        }
    }
}

impl Neighbours {
    /// Remove every neighbour.
    pub fn clear(&mut self) {
        self.n_leaders = 0;
        self.n_followers = 0;
        self.n_sides = 0;
    }

    /// Add a leader; returns `false` when full.
    pub fn push_leader(&mut self, l: LeaderTerm) -> bool {
        if self.n_leaders == MAX_NEIGHBOURS {
            return false;
        }
        self.leaders[self.n_leaders] = l;
        self.n_leaders += 1;
        true
    }

    /// Add a follower; returns `false` when full.
    pub fn push_follower(&mut self, f: FollowerTerm) -> bool {
        if self.n_followers == MAX_NEIGHBOURS {
            return false;
        }
        self.followers[self.n_followers] = f;
        self.n_followers += 1;
        true
    }

    /// Add a side neighbour (ignored when full).
    pub fn push_side(&mut self, s: SideTerm) {
        if self.n_sides < MAX_NEIGHBOURS {
            self.sides[self.n_sides] = s;
            self.n_sides += 1;
        }
    }
}

/// The deciding agent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Subject {
    pub y: f64,
    /// Half of its width (heading ignored: candidates are where it would
    /// travel straight).
    pub half_width: f64,
    /// Feasible interval of its centre: the carriageway, cut at the nearest
    /// side neighbour on each side by the squeezing clearance.
    pub lo: f64,
    pub hi: f64,
    /// Edges of the carriageway.
    pub edge_lo: f64,
    pub edge_hi: f64,
    /// Free acceleration inside the median band (`y < band_y`) and outside.
    pub free_band: f64,
    pub free_out: f64,
    pub band_y: f64,
    /// Acceleration allowed by constraints that do not depend on `y` (a
    /// closed barrier ahead).
    pub cap: f64,
    /// Its current lateral target.
    pub target: f64,
    /// The carriageway ahead, where it narrows within the lookahead.
    pub wall: Option<Wall>,
}

/// Slack on the lines compared with the carriageway's edges (m): a body
/// that touches the kerb is not across it.
const EDGE_SLACK: f64 = 1e-6; // allow-literal: rounding tolerance, not behaviour

/// The carriageway ahead of the deciding agent, with what it needs to work
/// out the acceleration its edge allows on a given line: that of following a
/// standing obstacle at the station where the line leaves the carriageway.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wall {
    pub edges: EdgeAhead,
    /// The agent's desired gap `s*` to a standing obstacle at its speed.
    pub desired_gap: f64,
    pub max_accel: f64,
    /// Lowest acceleration (`−b_emergency`).
    pub floor: f64,
    /// Lookahead distance (m).
    pub reach: f64,
}

impl Wall {
    /// The acceleration the edge allows a body spanning `[lo, hi]` laterally
    /// whose free acceleration is `free`.
    #[inline]
    pub fn accel(&self, free: f64, lo: f64, hi: f64) -> f64 {
        match self.edges.contact(lo + EDGE_SLACK, hi - EDGE_SLACK) {
            Some(d) if d <= self.reach => {
                if d > 0.0 {
                    let q = self.desired_gap / d;
                    (free - self.max_accel * q * q).max(self.floor)
                } else {
                    self.floor
                }
            }
            _ => f64::INFINITY,
        }
    }
}

/// Population-level constants of the choice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChoiceParams {
    pub braking_scale: f64,
    /// Lateral distance beyond the minimum clearance at which a leader is ignored.
    pub cutoff: f64,
    pub politeness: f64,
    pub safe_decel: f64,
    pub comfort_weight: f64,
    pub move_cost: f64,
    pub edge_clearance: f64,
    pub switch_threshold: f64,
    pub step: f64,
    pub range: f64,
}

/// The outcome of one decision, with the utility terms of the chosen target
/// relative to staying (m/s²).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub target: f64,
    /// Whether a candidate beat the switching threshold, and the target moved
    /// by at least half a candidate step (a new move, not the same one again).
    pub switched: bool,
    pub utility: f64,
    pub own: f64,
    pub polite: f64,
    pub comfort: f64,
    pub cost: f64,
}

/// Candidates scored together. Longer candidate lists (a fine step over a
/// wide range) are scored in consecutive batches of this size.
const BATCH: usize = 32;

/// Margin on the utility bound used to drop candidates before they are fully
/// scored (m/s²); far above rounding error, far below any utility difference
/// that matters.
const PRUNE_SLACK: f64 = 1e-9; // allow-literal: rounding tolerance, not behaviour

/// What being at each of up to [`BATCH`] lateral positions is worth: the
/// agent's own acceleration, the sum of its followers' accelerations, its
/// clearance shortfalls, and whether every follower stays within the safe
/// braking limit.
///
/// Positions are scored neighbour by neighbour over the whole batch, in two
/// stages: the leaders' effect on the agent's own acceleration first, then
/// everything else, only for positions that the first stage leaves able to
/// beat the switching threshold. Each position sees the same operations in
/// the same order as if it were scored alone, so the result does not depend
/// on the batch it is in (tested against one-at-a-time scoring).
struct Batch {
    y: [f64; BATCH],
    free: [f64; BATCH],
    own: [f64; BATCH],
    followers: [f64; BATCH],
    shortfall: [f64; BATCH],
    safe: [bool; BATCH],
    n: usize,
}

impl Batch {
    fn new() -> Self {
        Self {
            y: [0.0; BATCH],
            free: [0.0; BATCH],
            own: [0.0; BATCH],
            followers: [0.0; BATCH],
            shortfall: [0.0; BATCH],
            safe: [true; BATCH],
            n: 0,
        }
    }

    fn push(&mut self, y: f64) {
        self.y[self.n] = y;
        self.n += 1;
    }

    fn full(&self) -> bool {
        self.n == BATCH
    }
}

/// The neighbours of one decision, with what each follower's other leaders
/// allow it (`others`) and what the agent imposes on it now (`imposed_now`).
struct Scene<'a> {
    p: &'a ChoiceParams,
    me: &'a Subject,
    leaders: &'a [LeaderTerm],
    followers: &'a [FollowerTerm],
    sides: &'a [SideTerm],
    others: &'a [f64],
    imposed_now: &'a [f64],
}

impl Scene<'_> {
    /// Score every position in `b`.
    fn evaluate(&self, b: &mut Batch) {
        self.own_terms(b);
        self.other_terms(b);
    }

    /// First stage: the free and own accelerations, and the shortfall to
    /// the road edges.
    fn own_terms(&self, b: &mut Batch) {
        let (p, me, n) = (self.p, self.me, b.n);
        for m in 0..n {
            let y = b.y[m];
            let free = if y < me.band_y {
                me.free_band
            } else {
                me.free_out
            };
            b.free[m] = free;
            b.own[m] = free.min(me.cap);
            if let Some(w) = &me.wall {
                b.own[m] = b.own[m].min(w.accel(free, y - me.half_width, y + me.half_width));
            }
            b.shortfall[m] = shortfall(p.edge_clearance, 0.0, y - me.half_width - me.edge_lo)
                + shortfall(p.edge_clearance, 0.0, me.edge_hi - y - me.half_width);
            b.followers[m] = 0.0;
            b.safe[m] = true;
        }
        for l in self.leaders {
            for m in 0..n {
                if l.accel < b.own[m] {
                    let excess = edge_gap(b.y[m], me.half_width, l.y, l.half_width) - l.c_min;
                    if excess <= p.cutoff {
                        let free = b.free[m];
                        b.own[m] = b.own[m]
                            .min(free + leader_weight(excess, p.braking_scale) * (l.accel - free));
                    }
                }
            }
        }
    }

    /// Second stage: shortfalls to neighbours, and the followers.
    fn other_terms(&self, b: &mut Batch) {
        let (p, me, n) = (self.p, self.me, b.n);
        for l in self.leaders.iter().filter(|l| l.near) {
            for m in 0..n {
                let gap = edge_gap(b.y[m], me.half_width, l.y, l.half_width);
                b.shortfall[m] += shortfall(l.c_des, l.c_min, gap);
            }
        }
        for (k, f) in self.followers.iter().enumerate() {
            let (other, now) = (self.others[k], self.imposed_now[k]);
            for m in 0..n {
                let gap = edge_gap(b.y[m], me.half_width, f.y, f.half_width);
                if f.near {
                    b.shortfall[m] += shortfall(f.c_des_own, f.c_min_own, gap);
                }
                let excess = gap - f.c_min;
                let imposed = if excess > p.cutoff {
                    f.free
                } else {
                    f.free + leader_weight(excess, p.braking_scale) * (f.imposed - f.free)
                };
                b.followers[m] += other.min(imposed);
                if imposed < -p.safe_decel && imposed < now {
                    b.safe[m] = false;
                }
            }
        }
        for s in self.sides {
            for m in 0..n {
                b.shortfall[m] += shortfall(
                    s.c_des,
                    s.c_min,
                    edge_gap(b.y[m], me.half_width, s.y, s.half_width),
                );
            }
        }
    }

    /// The most the politeness term can add at any position: every
    /// follower at the most it could be allowed (or, with negative
    /// politeness, the least). `None` if the comfort term has no upper bound.
    fn polite_bound(&self, base: &Batch) -> Option<f64> {
        let p = self.p;
        if p.comfort_weight.is_nan() || p.comfort_weight < 0.0 {
            return None;
        }
        let (mut hi, mut lo) = (0.0, 0.0);
        for (f, &other) in self.followers.iter().zip(self.others) {
            hi += other.min(f.free.max(f.imposed));
            lo += other.min(f.free.min(f.imposed));
        }
        let sum = if p.politeness >= 0.0 { hi } else { lo };
        Some(p.politeness * (sum - base.followers[0]))
    }

    /// Drop the positions of a batch scored by [`Self::own_terms`] whose
    /// utility cannot exceed the switching threshold whatever the second
    /// stage adds: shortfalls only grow, and the politeness term is at most
    /// `polite`. Positions keep their order.
    fn prune(&self, b: &mut Batch, base: &Batch, polite: f64) {
        let (p, me) = (self.p, self.me);
        let mut k = 0;
        for m in 0..b.n {
            let bound = (b.own[m] - base.own[0])
                + polite
                + p.comfort_weight * (base.shortfall[0] - b.shortfall[m])
                - p.move_cost * (b.y[m] - me.y).abs();
            if bound + PRUNE_SLACK > p.switch_threshold {
                b.y[k] = b.y[m];
                b.free[k] = b.free[m];
                b.own[k] = b.own[m];
                b.shortfall[k] = b.shortfall[m];
                k += 1;
            }
        }
        b.n = k;
    }

    /// The utility of position `m` of a scored batch relative to `base`
    /// (position 0 of a batch holding only the current position), or `None`
    /// if a follower would have to brake harder than the safe limit.
    fn choice(&self, b: &Batch, m: usize, base: &Batch) -> Option<Choice> {
        if !b.safe[m] {
            return None;
        }
        let (p, y) = (self.p, b.y[m]);
        let own = b.own[m] - base.own[0];
        let polite = p.politeness * (b.followers[m] - base.followers[0]);
        let comfort = -p.comfort_weight * (b.shortfall[m] - base.shortfall[0]);
        let cost = -p.move_cost * (y - self.me.y).abs();
        Some(Choice {
            target: y,
            switched: true,
            utility: own + polite + comfort + cost,
            own,
            polite,
            comfort,
            cost,
        })
    }

    /// Score the batch and keep the first position, in batch order, whose
    /// utility beats both the switching threshold and `best`.
    fn best_of(&self, b: &mut Batch, base: &Batch, polite: Option<f64>, best: &mut Option<Choice>) {
        if b.n == 0 {
            return;
        }
        self.own_terms(b);
        if let Some(polite) = polite {
            self.prune(b, base, polite);
        }
        self.other_terms(b);
        for m in 0..b.n {
            if let Some(c) = self.choice(b, m, base)
                && c.utility > best.map_or(self.p.switch_threshold, |x| x.utility)
            {
                *best = Some(c);
            }
        }
        b.n = 0;
    }
}

/// Choose the lateral target (ADR-0010 §3): the feasible candidate with the
/// highest utility, if it beats the switching threshold; otherwise the
/// current target, pulled into the feasible interval (or the current
/// position, if moving on towards the target has become worse than staying).
pub fn choose_target(p: &ChoiceParams, me: &Subject, nb: &mut Neighbours) -> Choice {
    // Leaders in order of the acceleration they allow: a position stops
    // adding weights once none can lower its running minimum.
    nb.leaders[..nb.n_leaders].sort_unstable_by(|a, b| a.accel.total_cmp(&b.accel));
    let (leaders, followers, sides) = (
        &nb.leaders[..nb.n_leaders],
        &nb.followers[..nb.n_followers],
        &nb.sides[..nb.n_sides],
    );
    // What each follower's other leaders allow it, and what the agent
    // imposes on it now.
    let mut others = [0.0; MAX_NEIGHBOURS];
    let mut imposed_now = [0.0; MAX_NEIGHBOURS];
    for (k, f) in followers.iter().enumerate() {
        let excess = edge_gap(me.y, me.half_width, f.y, f.half_width) - f.c_min;
        let now = if excess > p.cutoff {
            f.free
        } else {
            f.free + leader_weight(excess, p.braking_scale) * (f.imposed - f.free)
        };
        imposed_now[k] = now;
        others[k] = if f.acc_now < now { f.acc_now } else { f.free };
    }
    let scene = Scene {
        p,
        me,
        leaders,
        followers,
        sides,
        others: &others[..followers.len()],
        imposed_now: &imposed_now[..followers.len()],
    };
    let mut base = Batch::new();
    base.push(me.y);
    scene.evaluate(&mut base);
    // An upper bound on any candidate's utility: the agent at its best free
    // acceleration, every follower unhindered by it, no shortfall, no cost.
    // If even that cannot beat the threshold, no candidate needs scoring.
    let reach_lo = me.lo.max(me.y - p.range);
    let best_free = if reach_lo < me.band_y {
        me.free_band.max(me.free_out)
    } else {
        me.free_out
    };
    let unhindered: f64 = followers
        .iter()
        .zip(&others)
        .map(|(f, o)| o.min(f.free))
        .sum();
    let bound = (best_free.min(me.cap) - base.own[0])
        + p.politeness * (unhindered - base.followers[0])
        + p.comfort_weight * base.shortfall[0];
    let worth_scoring = bound > p.switch_threshold;
    let polite = scene.polite_bound(&base);
    let mut best: Option<Choice> = None;
    let mut batch = Batch::new();
    let consider = |y: f64, batch: &mut Batch, best: &mut Option<Choice>| {
        if y < me.lo || y > me.hi {
            return;
        }
        batch.push(y);
        if batch.full() {
            scene.best_of(batch, &base, polite, best);
        }
    };
    // Nearest candidates first, so ties go to the smaller move.
    let steps = (p.range / p.step).floor() as i32;
    let (mut lo_done, mut hi_done) = (false, false);
    for m in 1..=if worth_scoring { steps } else { 0 } {
        let d = f64::from(m) * p.step;
        for y in [me.y - d, me.y + d] {
            // The ends of the feasible interval are candidates too: the
            // tightest position beside a neighbour may be the one that fits.
            if y < me.lo && !lo_done {
                lo_done = true;
                if me.lo < me.y {
                    consider(me.lo, &mut batch, &mut best);
                }
            } else if y > me.hi && !hi_done {
                hi_done = true;
                if me.hi > me.y {
                    consider(me.hi, &mut batch, &mut best);
                }
            } else {
                consider(y, &mut batch, &mut best);
            }
        }
    }
    scene.best_of(&mut batch, &base, polite, &mut best);
    if let Some(c) = best {
        return Choice {
            switched: (c.target - me.target).abs() >= 0.5 * p.step,
            ..c
        };
    }
    // Nothing is worth a new move: carry on to the current target unless
    // that has become worse than staying here.
    let target = me.target.clamp(me.lo.min(me.y), me.hi.max(me.y));
    let hold = Choice {
        target: me.y,
        switched: false,
        utility: 0.0,
        own: 0.0,
        polite: 0.0,
        comfort: 0.0,
        cost: 0.0,
    };
    if target == me.y {
        return hold;
    }
    batch.push(target);
    scene.evaluate(&mut batch);
    match scene.choice(&batch, 0, &base) {
        Some(c) if c.utility >= 0.0 => Choice {
            switched: false,
            ..c
        },
        _ => hold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What being at lateral position `y` is worth: the agent's own acceleration,
    /// the sum of its followers' accelerations, its clearance shortfalls, and
    /// whether every follower stays within the safe braking limit.
    #[derive(Debug, Clone, Copy)]
    struct Value {
        own: f64,
        followers: f64,
        shortfall: f64,
        safe: bool,
    }

    /// The choice as first implemented (one candidate at a time, no pruning),
    /// kept to check that batching and pruning give the same answer.
    /// Choose the lateral target (ADR-0010 §3): the feasible candidate with the
    /// highest utility, if it beats the switching threshold; otherwise the
    /// current target, pulled into the feasible interval (or the current
    /// position, if moving on towards the target has become worse than staying).
    fn reference_choose_target(p: &ChoiceParams, me: &Subject, nb: &mut Neighbours) -> Choice {
        // Leaders in order of the acceleration they allow: the scan over them
        // can stop adding weights once none can lower the running minimum.
        nb.leaders[..nb.n_leaders].sort_unstable_by(|a, b| a.accel.total_cmp(&b.accel));
        let (leaders, followers, sides) = (
            &nb.leaders[..nb.n_leaders],
            &nb.followers[..nb.n_followers],
            &nb.sides[..nb.n_sides],
        );
        // What each follower's other leaders allow it, and what the agent
        // imposes on it now.
        let mut others = [0.0; MAX_NEIGHBOURS];
        let mut imposed_now = [0.0; MAX_NEIGHBOURS];
        for (k, f) in followers.iter().enumerate() {
            let excess = edge_gap(me.y, me.half_width, f.y, f.half_width) - f.c_min;
            let now = if excess > p.cutoff {
                f.free
            } else {
                f.free + leader_weight(excess, p.braking_scale) * (f.imposed - f.free)
            };
            imposed_now[k] = now;
            others[k] = if f.acc_now < now { f.acc_now } else { f.free };
        }
        let value = |y: f64| -> Value {
            let free = if y < me.band_y {
                me.free_band
            } else {
                me.free_out
            };
            let mut own = free.min(me.cap);
            let mut short = shortfall(p.edge_clearance, 0.0, y - me.half_width - me.edge_lo)
                + shortfall(p.edge_clearance, 0.0, me.edge_hi - y - me.half_width);
            for l in leaders {
                let gap = edge_gap(y, me.half_width, l.y, l.half_width);
                if l.near {
                    short += shortfall(l.c_des, l.c_min, gap);
                }
                if l.accel < own {
                    let excess = gap - l.c_min;
                    if excess <= p.cutoff {
                        own = own
                            .min(free + leader_weight(excess, p.braking_scale) * (l.accel - free));
                    }
                }
            }
            let mut sum = 0.0;
            let mut safe = true;
            for (k, f) in followers.iter().enumerate() {
                let gap = edge_gap(y, me.half_width, f.y, f.half_width);
                if f.near {
                    short += shortfall(f.c_des_own, f.c_min_own, gap);
                }
                let excess = gap - f.c_min;
                let imposed = if excess > p.cutoff {
                    f.free
                } else {
                    f.free + leader_weight(excess, p.braking_scale) * (f.imposed - f.free)
                };
                sum += others[k].min(imposed);
                if imposed < -p.safe_decel && imposed < imposed_now[k] {
                    safe = false;
                }
            }
            for s in sides {
                short += shortfall(
                    s.c_des,
                    s.c_min,
                    edge_gap(y, me.half_width, s.y, s.half_width),
                );
            }
            Value {
                own,
                followers: sum,
                shortfall: short,
                safe,
            }
        };
        let base = value(me.y);
        // An upper bound on any candidate's utility: the agent at its best free
        // acceleration, every follower unhindered by it, no shortfall, no cost.
        // If even that cannot beat the threshold, no candidate needs scoring.
        let reach_lo = me.lo.max(me.y - p.range);
        let best_free = if reach_lo < me.band_y {
            me.free_band.max(me.free_out)
        } else {
            me.free_out
        };
        let unhindered: f64 = followers
            .iter()
            .zip(&others)
            .map(|(f, o)| o.min(f.free))
            .sum();
        let bound = (best_free.min(me.cap) - base.own)
            + p.politeness * (unhindered - base.followers)
            + p.comfort_weight * base.shortfall;
        let worth_scoring = bound > p.switch_threshold;
        let score = |y: f64| -> Option<Choice> {
            let v = value(y);
            if !v.safe {
                return None;
            }
            let own = v.own - base.own;
            let polite = p.politeness * (v.followers - base.followers);
            let comfort = -p.comfort_weight * (v.shortfall - base.shortfall);
            let cost = -p.move_cost * (y - me.y).abs();
            Some(Choice {
                target: y,
                switched: true,
                utility: own + polite + comfort + cost,
                own,
                polite,
                comfort,
                cost,
            })
        };
        let mut best: Option<Choice> = None;
        let mut consider = |y: f64| {
            if y < me.lo || y > me.hi {
                return;
            }
            if let Some(c) = score(y)
                && c.utility > best.map_or(p.switch_threshold, |b| b.utility)
            {
                best = Some(c);
            }
        };
        // Nearest candidates first, so ties go to the smaller move.
        let steps = (p.range / p.step).floor() as i32;
        let (mut lo_done, mut hi_done) = (false, false);
        for m in 1..=if worth_scoring { steps } else { 0 } {
            let d = f64::from(m) * p.step;
            for y in [me.y - d, me.y + d] {
                // The ends of the feasible interval are candidates too: the
                // tightest position beside a neighbour may be the one that fits.
                if y < me.lo && !lo_done {
                    lo_done = true;
                    if me.lo < me.y {
                        consider(me.lo);
                    }
                } else if y > me.hi && !hi_done {
                    hi_done = true;
                    if me.hi > me.y {
                        consider(me.hi);
                    }
                } else {
                    consider(y);
                }
            }
        }
        if let Some(c) = best {
            return Choice {
                switched: (c.target - me.target).abs() >= 0.5 * p.step,
                ..c
            };
        }
        // Nothing is worth a new move: carry on to the current target unless
        // that has become worse than staying here.
        let target = me.target.clamp(me.lo.min(me.y), me.hi.max(me.y));
        let hold = Choice {
            target: me.y,
            switched: false,
            utility: 0.0,
            own: 0.0,
            polite: 0.0,
            comfort: 0.0,
            cost: 0.0,
        };
        if target == me.y {
            return hold;
        }
        match score(target) {
            Some(c) if c.utility >= 0.0 => Choice {
                switched: false,
                ..c
            },
            _ => hold,
        }
    }

    /// Batched, pruned scoring chooses exactly what scoring each candidate on
    /// its own does, on random scenes.
    /// The swing-out gap: blocked, the vehicle keeps room to pull out;
    /// free to steer, a body it will be clear of on arrival does not count.
    #[test]
    fn swing_out_keeps_room_and_lets_a_free_vehicle_pass() {
        let tan = 0.2_f64.tan();
        let (road, open) = ((-2.7, 2.7), (f64::NEG_INFINITY, f64::INFINITY));
        // A car (half width 0.8, at rest laterally) behind a body at y = 0.5,
        // half width 0.9; minimum clearance 0.05; 1 m/s²; τ = 1 s.
        let at = |y: f64, target: f64, limits, road, body_y: f64, gap: f64, v: f64| {
            swing_out(
                y, 0.8, 0.0, target, limits, road, body_y, 0.9, 0.05, gap, v, 1.0, tan, 1.0,
            )
        };
        // Blocked at y = 0: the nearer clear side is the right, at
        // 0.5 − 0.9 − 0.05 − 0.8 = −1.25.
        let blocked = at(0.0, 0.0, (0.0, 0.0), road, 0.5, 20.0, 5.0);
        assert!(
            (blocked.gap - (20.0 - 1.25 / tan)).abs() < 1e-9,
            "{blocked:?}"
        );
        assert!(blocked.excess < 0.0);
        // Free to steer to y = −1.6, from rest 10 m back: 4.5 s at 1 m/s²,
        // 94% of the way to the target: clear on arrival.
        let free = at(0.0, -1.6, open, road, 0.5, 10.0, 0.0);
        assert_eq!(free.gap, 10.0);
        assert!(free.excess > 0.2 && free.excess < 0.35, "{free:?}");
        // Free to steer but too close: 1 m of run-up allows 0.2 m of shift.
        let close = at(0.0, -1.6, open, road, 0.5, 1.0, 0.0);
        assert!(
            (close.gap - (1.0 - (1.25 - tan) / tan)).abs() < 1e-9,
            "{close:?}"
        );
        // No room on either side: the body itself.
        let wall = at(0.0, 0.0, (0.0, 0.0), (-1.0, 1.0), 0.0, 8.0, 3.0);
        assert_eq!(wall.gap, 8.0);
        // Already clear and holding its line: as before (gap and current excess).
        let past = at(-1.5, -1.5, open, road, 0.5, 8.0, 3.0);
        assert_eq!(past.gap, 8.0);
        assert!((past.excess - 0.25).abs() < 1e-9);
    }

    #[test]
    fn batched_choice_equals_the_reference() {
        use proptest::prelude::*;
        let leader = (
            -4.0..4.0f64,
            0.3..1.3f64,
            -9.0..1.5f64,
            0.0..0.4f64,
            0.0..1.0f64,
            any::<bool>(),
        )
            .prop_map(|(y, half_width, accel, c_min, extra, near)| LeaderTerm {
                y,
                half_width,
                accel,
                c_min,
                c_des: c_min + extra,
                near,
            });
        let follower = (
            (
                -4.0..4.0f64,
                0.3..1.3f64,
                -2.0..1.5f64,
                -9.0..1.5f64,
                -3.0..1.5f64,
            ),
            (0.0..0.4f64, 0.0..0.4f64, 0.0..1.0f64, any::<bool>()),
        )
            .prop_map(
                |((y, half_width, free, imposed, acc_now), (c_min, c_min_own, extra, near))| {
                    FollowerTerm {
                        y,
                        half_width,
                        free,
                        imposed,
                        acc_now,
                        c_min,
                        c_min_own,
                        c_des_own: c_min_own + extra,
                        near,
                    }
                },
            );
        let side = (-3.0..3.0f64, 0.3..1.3f64, 0.0..0.4f64, 0.0..1.0f64).prop_map(
            |(y, half_width, c_min, extra)| SideTerm {
                y,
                half_width,
                c_min,
                c_des: c_min + extra,
            },
        );
        let scene = (
            proptest::collection::vec(leader, 0..8),
            proptest::collection::vec(follower, 0..8),
            proptest::collection::vec(side, 0..3),
            (
                -2.0..2.0f64,
                0.3..1.2f64,
                -5.0..0.0f64,
                0.0..5.0f64,
                -2.0..2.0f64,
            ),
            (
                -1.0..1.5f64,
                -1.0..1.5f64,
                -6.0..6.0f64,
                -2.0..3.0f64,
                0.0..0.3f64,
            ),
        );
        let config = proptest::test_runner::Config {
            cases: 4096,
            ..Default::default()
        };
        let rng = proptest::test_runner::TestRng::deterministic_rng(
            proptest::test_runner::RngAlgorithm::ChaCha,
        );
        let mut runner = proptest::test_runner::TestRunner::new_with_rng(config, rng);
        let switched = std::cell::Cell::new(0);
        runner
            .run(
                &scene,
                |(ls, fs, ss, (y, hw, lo, hi, target), (fb, fo, band, cap, step))| {
                    let mut nb = Neighbours::default();
                    ls.iter().for_each(|&l| assert!(nb.push_leader(l)));
                    fs.iter().for_each(|&f| assert!(nb.push_follower(f)));
                    ss.iter().for_each(|&s| nb.push_side(s));
                    let me = Subject {
                        y,
                        half_width: hw,
                        lo: y + lo,
                        hi: y + hi,
                        edge_lo: -5.25,
                        edge_hi: 5.25,
                        free_band: fb,
                        free_out: fo,
                        band_y: band,
                        cap,
                        target: y + target,
                        wall: None,
                    };
                    let p = ChoiceParams {
                        step: 0.05 + step,
                        ..params()
                    };
                    let got = choose_target(&p, &me, &mut nb.clone());
                    let want = reference_choose_target(&p, &me, &mut nb);
                    prop_assert_eq!(got, want);
                    switched.set(switched.get() + usize::from(got.switched));
                    Ok(())
                },
            )
            .unwrap();
        // The scenes exercise real choices, not only "stay".
        assert!(switched.get() > 4096 / 5, "{}", switched.get());
    }

    const BM: (f64, f64, f64, f64) = (1.209, 0.615 * 0.036, 1.17, 0.0);

    fn pair(intercept_cm: f64, slope: f64, kmh: f64) -> f64 {
        clearances(intercept_cm / 100.0, slope * 0.036, 1.17, 0.0, kmh / 3.6).0
    }

    /// P5.2: the pooled line gives 1.46 m at 40 km/h and 1.33 m at 20 km/h.
    #[test]
    fn desired_clearance_matches_budhkar_maurya() {
        // C[cm] = 0.615 v[km/h] + 120.9.
        let (i, s, off, floor) = BM;
        assert!((clearances(i, s, off, floor, 40.0 / 3.6).0 - 1.455).abs() < 1e-3);
        assert!((clearances(i, s, off, floor, 20.0 / 3.6).0 - 1.332).abs() < 1e-3);
    }

    /// The minimum is the lower threshold of the paper's residual: about
    /// 0.04 m at standstill and 0.29 m at 40 km/h.
    #[test]
    fn minimum_clearance_is_the_lower_threshold() {
        let (i, s, off, floor) = BM;
        assert!((clearances(i, s, off, floor, 0.0).1 - 0.039).abs() < 1e-3);
        assert!((clearances(i, s, off, floor, 40.0 / 3.6).1 - 0.285).abs() < 1e-3);
        // Floored, and never above the desired clearance.
        assert_eq!(clearances(0.5, 0.0, 1.17, 0.1, 5.0).1, 0.1);
        assert_eq!(clearances(0.05, 0.0, 1.17, 0.1, 5.0).1, 0.05);
    }

    /// P5.2 pair ordering with Table 4 of the paper: two-wheeler pairs keep
    /// the least; a mixed pair keeps more than the mean of its two same-type
    /// pairs (the paper's own comparison, its §5.4).
    #[test]
    fn pair_ordering_two_wheelers_same_type_mixed() {
        for kmh in [20.0, 30.0, 40.0] {
            let bike = pair(118.02, 0.224, kmh);
            let car = pair(117.31, 0.681, kmh);
            let auto = pair(114.55, 1.037, kmh);
            let car_bike = pair(125.16, 0.612, kmh);
            let auto_bike = pair(117.27, 0.739, kmh);
            let car_auto = pair(124.12, 0.642, kmh);
            assert!(bike < car && bike < auto, "{kmh}");
            assert!(bike < car_bike && bike < auto_bike && bike < car_auto);
            assert!(car_bike > 0.5 * (car + bike), "{kmh}");
            assert!(auto_bike > 0.5 * (auto + bike), "{kmh}");
        }
        // Car-auto is above the same-type mean up to about 30 km/h and below
        // it at higher speeds, as the paper reports.
        let mean = |kmh| 0.5 * (pair(117.31, 0.681, kmh) + pair(114.55, 1.037, kmh));
        assert!(pair(124.12, 0.642, 20.0) > mean(20.0));
        assert!(pair(124.12, 0.642, 60.0) < mean(60.0));
        // The guide's "+35%" and "−25%" are these slopes against the mean of
        // the two same-type slopes.
        assert!((0.612f64 / (0.5 * (0.681 + 0.224)) - 1.35).abs() < 0.01);
        assert!((0.642f64 / (0.5 * (0.681 + 1.037)) - 0.75).abs() < 0.01);
    }

    #[test]
    fn weight_decays_beyond_clearance() {
        assert_eq!(leader_weight(-0.3, 0.15), 1.0);
        assert_eq!(leader_weight(0.0, 0.15), 1.0);
        assert!((leader_weight(0.15, 0.15) - (-1.0f64).exp()).abs() < 1e-12);
    }

    #[test]
    fn gaps_and_shortfalls() {
        assert_eq!(lateral_gap(0.0, 2.0, 3.0, 2.0), 1.0);
        assert!(lateral_gap(0.0, 2.0, 1.0, 2.0) < 0.0);
        assert_eq!(edge_gap(0.0, 1.0, 3.0, 1.0), 1.0);
        assert_eq!(shortfall(1.2, 0.2, 1.5), 0.0);
        assert_eq!(shortfall(1.2, 0.2, 0.1), 1.0);
        assert!((shortfall(1.2, 0.2, 0.7) - 0.5).abs() < 1e-12);
        assert_eq!(shortfall(0.0, 0.0, 0.0), 0.0);
        assert!((body_half_width(2.0, 10.0, 0.0) - 1.0).abs() < 1e-12);
        let h = 0.2f64;
        let exact = 0.5 * (2.0 * h.cos() + 10.0 * h.sin());
        assert!((body_half_width(2.0, 10.0, h.tan()) - exact).abs() < 1e-12);
        assert!((body_half_width(2.0, 10.0, -h.tan()) - exact).abs() < 1e-12);
    }

    /// Tracking reaches the target without overshoot, 95% of the way in
    /// about 4.7 time constants.
    #[test]
    fn tracking_is_critically_damped() {
        let (tau, dt) = (1.0, 0.01);
        let (mut y, mut vy) = (0.0, 0.0);
        let mut t95 = None;
        for k in 0..2000 {
            vy = track(vy, y, 3.0, 0.0, tau, dt);
            y += vy * dt;
            assert!(y <= 3.0 + 1e-9, "overshoot at step {k}");
            if t95.is_none() && y >= 0.95 * 3.0 {
                t95 = Some(f64::from(k) * dt);
            }
        }
        assert!((y - 3.0).abs() < 1e-3);
        let t95 = t95.unwrap();
        assert!((t95 - 4.74).abs() < 0.15, "{t95}");
    }

    fn params() -> ChoiceParams {
        ChoiceParams {
            braking_scale: 0.15,
            cutoff: 1.0,
            politeness: 0.2,
            safe_decel: 4.0,
            comfort_weight: 0.5,
            move_cost: 0.05,
            edge_clearance: 0.25,
            switch_threshold: 0.1,
            step: 0.25,
            range: 3.5,
        }
    }

    fn subject() -> Subject {
        Subject {
            y: 0.0,
            half_width: 0.9,
            lo: -4.35,
            hi: 4.35,
            edge_lo: -5.25,
            edge_hi: 5.25,
            free_band: 0.5,
            free_out: 0.5,
            band_y: f64::NEG_INFINITY,
            cap: f64::INFINITY,
            target: 0.0,
            wall: None,
        }
    }

    fn leader(y: f64, accel: f64) -> LeaderTerm {
        LeaderTerm {
            y,
            half_width: 0.9,
            accel,
            c_min: 0.1,
            c_des: 1.3,
            near: false,
        }
    }

    #[test]
    fn free_road_keeps_the_position() {
        let c = choose_target(&params(), &subject(), &mut Neighbours::default());
        assert!(!c.switched);
        assert_eq!(c.target, 0.0);
    }

    /// A slow leader straight ahead: the agent picks the nearest position
    /// where the leader's weight has faded, on the side with room.
    #[test]
    fn passes_a_slow_leader_on_the_open_side() {
        let mut nb = Neighbours::default();
        nb.push_leader(leader(0.0, -1.0));
        // A second slow vehicle blocks the right.
        nb.push_leader(leader(-2.5, -1.0));
        let c = choose_target(&params(), &subject(), &mut nb);
        assert!(c.switched);
        assert!(c.target > 2.0 && c.target <= 3.5, "{}", c.target);
        assert!(c.own > 1.0);
        assert!((c.utility - (c.own + c.polite + c.comfort + c.cost)).abs() < 1e-12);
    }

    /// Symmetric room: ties go to the smaller move, then to the lower y.
    #[test]
    fn choice_is_deterministic_on_ties() {
        let mut nb = Neighbours::default();
        nb.push_leader(leader(0.0, -1.0));
        let a = choose_target(&params(), &subject(), &mut nb.clone());
        let b = choose_target(&params(), &subject(), &mut nb);
        assert_eq!(a, b);
        assert!(a.switched && a.target < 0.0);
    }

    /// A hole narrower than the body plus twice the minimum clearance gives
    /// no advantage, so the agent does not aim for it.
    #[test]
    fn does_not_aim_for_a_gap_narrower_than_the_minimum_clearance() {
        let mut me = subject();
        me.half_width = 0.32;
        me.y = -3.0;
        me.target = -3.0;
        // Stopped vehicles ahead: one in front of the agent, and two leaving
        // a hole of `hole` m centred at y = 0. Nothing else is in reach.
        let run = |hole: f64| {
            let mut nb = Neighbours::default();
            nb.push_leader(leader(-3.0, -2.0));
            nb.push_leader(leader(-0.9 - 0.5 * hole, -2.0));
            nb.push_leader(leader(0.9 + 0.5 * hole, -2.0));
            nb.push_leader(leader(2.7 + 0.5 * hole + 0.1, -2.0));
            let mut m = me;
            // The kerb is on its right; a side neighbour limits it on the left.
            m.lo = -3.0;
            m.hi = 0.5;
            choose_target(&params(), &m, &mut nb)
        };
        // 0.64 m body + 2 × 0.1 m minimum clearance = 0.84 m.
        let narrow = run(0.8);
        assert!(!narrow.switched, "{narrow:?}");
        let wide = run(1.8);
        assert!(wide.switched && wide.target.abs() < 0.3, "{wide:?}");
    }

    /// Politeness: with equal room on both sides, the agent avoids the side
    /// where it would make a close follower brake; without politeness the
    /// follower does not matter.
    #[test]
    fn politeness_avoids_cutting_in() {
        let build = || {
            let mut nb = Neighbours::default();
            nb.push_leader(leader(0.0, -1.0));
            // A fast follower on the right, which would have to brake at 2 m/s².
            nb.push_follower(FollowerTerm {
                y: -2.5,
                half_width: 0.9,
                free: 0.5,
                imposed: -2.0,
                acc_now: 0.5,
                c_min: 0.1,
                c_min_own: 0.1,
                c_des_own: 1.3,
                near: false,
            });
            nb
        };
        let polite = choose_target(&params(), &subject(), &mut build());
        assert!(polite.switched && polite.target > 0.0, "{polite:?}");
        let selfish = ChoiceParams {
            politeness: 0.0,
            ..params()
        };
        let c = choose_target(&selfish, &subject(), &mut build());
        assert!(c.switched && c.target < 0.0, "{c:?}");
    }

    /// The safety criterion: a move that would make a follower brake harder
    /// than the safe limit is not available at all.
    #[test]
    fn unsafe_moves_are_infeasible() {
        let mut nb = Neighbours::default();
        nb.push_leader(leader(0.0, -1.0));
        for y in [-2.5, 2.5] {
            nb.push_follower(FollowerTerm {
                y,
                half_width: 0.9,
                free: 0.5,
                imposed: -6.0,
                acc_now: 0.5,
                c_min: 0.1,
                c_min_own: 0.1,
                c_des_own: 1.3,
                near: false,
            });
        }
        let selfish = ChoiceParams {
            politeness: 0.0,
            range: 2.5,
            ..params()
        };
        let c = choose_target(&selfish, &subject(), &mut nb);
        assert!(!c.switched, "{c:?}");
    }

    /// Comfort: on a free road an agent beside another vehicle moves away
    /// until the desired clearance is open, and no further.
    #[test]
    fn comfort_opens_the_desired_clearance() {
        let mut nb = Neighbours::default();
        nb.push_side(SideTerm {
            y: 2.0,
            half_width: 0.9,
            c_min: 0.1,
            c_des: 1.3,
        });
        let c = choose_target(&params(), &subject(), &mut nb);
        // Lateral gap now 0.2 m; the desired 1.3 m needs y = −1.1.
        assert!(c.switched);
        assert!(c.target <= -1.0 && c.target >= -1.25, "{}", c.target);
        assert!(c.comfort > 0.0 && c.own == 0.0);
    }

    /// The ends of the feasible interval are candidates, and nothing outside
    /// it is.
    #[test]
    fn candidates_stay_in_the_feasible_interval() {
        let mut nb = Neighbours::default();
        nb.push_leader(leader(0.0, -1.0));
        let mut me = subject();
        me.lo = 0.0;
        me.hi = 2.6;
        let c = choose_target(&params(), &me, &mut nb);
        assert!(c.switched);
        assert_eq!(c.target, 2.6);
        // Too little room to get clear of the leader: nothing to gain.
        me.hi = 1.9;
        assert!(!choose_target(&params(), &me, &mut nb).switched);
    }

    /// Without a reason to switch, the agent carries on to its target; if the
    /// target has become worse than staying, it stays.
    #[test]
    fn keeps_or_drops_the_current_target() {
        let mut me = subject();
        me.target = 2.0;
        let c = choose_target(&params(), &me, &mut Neighbours::default());
        assert!(!c.switched);
        assert_eq!(c.target, 0.0, "a costly move with no gain is dropped");
        let free_move = ChoiceParams {
            move_cost: 0.0,
            ..params()
        };
        let c = choose_target(&free_move, &me, &mut Neighbours::default());
        assert!(!c.switched);
        assert_eq!(c.target, 2.0);
        // Pulled into the feasible interval.
        me.hi = 1.0;
        let c = choose_target(&free_move, &me, &mut Neighbours::default());
        assert_eq!(c.target, 1.0);
    }

    /// The median band's higher desired speed is an incentive on its own.
    #[test]
    fn band_bonus_pulls_towards_the_band() {
        let mut me = subject();
        me.free_band = 0.9;
        me.band_y = -1.75;
        let c = choose_target(&params(), &me, &mut Neighbours::default());
        assert!(c.switched && c.target < -1.75 && c.target >= -2.0, "{c:?}");
    }
}
