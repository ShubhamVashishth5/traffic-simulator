//! Junctions: the ribbons inside them, where two of those overlap, and who
//! goes first (plan/phases/P07-junctions-splits.md, ADR-0009, ADR-0016).
//!
//! A junction adds ribbons to the network and tells each arriving ribbon
//! which ribbon follows it for each leaving arm. Nothing here is behaviour:
//! it is geometry and the rule book.

use crate::network::NetError;
use crate::ribbon::Ribbon;
use indexmap::IndexMap;
use marg_params::Dist;
use marg_params::spec::{
    ControlSpec, CountdownState, JunctionKind, JunctionSpec, RibbonSpec, WidthPoint,
};
use std::f64::consts::{FRAC_PI_4, PI, TAU};

/// Segments of a generated movement ribbon's reference line.
const MOVEMENT_SEGMENTS: usize = 16;
/// Largest angle one segment of a ring arc spans (rad).
const ARC_STEP: f64 = PI / 36.0;
/// Spacing of the stations at which two ribbons are tested for overlap (m).
const OVERLAP_STEP: f64 = 0.5;

/// One arm of a built junction.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub id: String,
    pub inbound: Option<usize>,
    pub outbound: Option<usize>,
}

/// Which way a movement turns (left-hand traffic: a left turn is the near one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    Through,
    Left,
    Right,
    UTurn,
}

impl Turn {
    pub fn key(self) -> &'static str {
        match self {
            Turn::Through => "through",
            Turn::Left => "left",
            Turn::Right => "right",
            Turn::UTurn => "u_turn",
        }
    }
}

/// One movement: from an arriving arm to a leaving arm.
#[derive(Debug, Clone, PartialEq)]
pub struct Movement {
    pub from: usize,
    pub to: usize,
    /// The ribbon a vehicle is on after the arriving ribbon.
    pub first: usize,
    pub turn: Turn,
    /// The part of the arriving ribbon's width, at its end, from which the
    /// movement can be made (`None`: all of it).
    pub window: Option<(f64, f64)>,
}

/// How two ribbons inside a junction meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    Crossing,
    Merge,
    Diverge,
}

impl ConflictKind {
    pub fn key(self) -> &'static str {
        match self {
            ConflictKind::Crossing => "crossing",
            ConflictKind::Merge => "merge",
            ConflictKind::Diverge => "diverge",
        }
    }
}

/// Where two ribbons inside a junction overlap: the stretch of each that
/// lies within the other's width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConflictArea {
    pub a: usize,
    pub b: usize,
    pub a_s: (f64, f64),
    pub b_s: (f64, f64),
    pub kind: ConflictKind,
}

/// One stage of a fixed-time plan, with the ribbons that have green in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    pub green: Vec<usize>,
    /// Arms any of whose movements has green in this stage.
    pub arms: Vec<usize>,
    pub green_s: f64,
    pub amber_s: f64,
    pub all_red_s: f64,
}

/// Who goes first.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    Uncontrolled,
    Signal {
        phases: Vec<Phase>,
        cycle_s: f64,
        offset_s: f64,
        countdown: CountdownState,
    },
}

/// A built junction.
#[derive(Debug, Clone, PartialEq)]
pub struct Junction {
    pub id: String,
    pub kind: JunctionKind,
    pub arms: Vec<Arm>,
    /// Cumulative turning shares by arriving arm over leaving arms.
    pub turns: Vec<Vec<f64>>,
    pub movements: Vec<Movement>,
    /// `movement_of[from][to]`: index into `movements`.
    pub movement_of: Vec<Vec<Option<usize>>>,
    /// Every ribbon the junction built.
    pub ribbons: Vec<usize>,
    pub conflicts: Vec<ConflictArea>,
    pub control: Control,
    pub sign_visibility: Option<f64>,
    pub u_turn_path: Option<Dist>,
}

impl Junction {
    /// The leaving arm for a uniform draw `u` in `[0, 1)` at arriving arm `from`.
    pub fn draw_out(&self, from: usize, u: f64) -> usize {
        let cum = &self.turns[from];
        cum.iter()
            .position(|&c| u < c)
            .unwrap_or_else(|| cum.iter().rposition(|&c| c > 0.0).unwrap_or(0))
    }

    /// The areas in which ribbon `r` overlaps another, as `(other, own
    /// stretch, other's stretch, kind)`.
    pub fn conflicts_of(
        &self,
        r: usize,
    ) -> impl Iterator<Item = (usize, (f64, f64), (f64, f64), ConflictKind)> + '_ {
        self.conflicts.iter().filter_map(move |c| {
            if c.a == r {
                Some((c.b, c.a_s, c.b_s, c.kind))
            } else if c.b == r {
                Some((c.a, c.b_s, c.a_s, c.kind))
            } else {
                None
            }
        })
    }
}

fn bad(id: &str, m: impl Into<String>) -> NetError {
    NetError::Junction(id.to_string(), m.into())
}

fn wrap(a: f64) -> f64 {
    let a = a.rem_euclid(TAU);
    if a > PI { a - TAU } else { a }
}

/// A cubic Hermite curve from one pose to another, as a polyline.
fn hermite(p0: (f64, f64, f64), p1: (f64, f64, f64)) -> Vec<[f64; 2]> {
    let d = (p1.0 - p0.0).hypot(p1.1 - p0.1);
    let (t0, t1) = (
        [d * p0.2.cos(), d * p0.2.sin()],
        [d * p1.2.cos(), d * p1.2.sin()],
    );
    (0..=MOVEMENT_SEGMENTS)
        .map(|k| {
            let t = k as f64 / MOVEMENT_SEGMENTS as f64;
            let (t2, t3) = (t * t, t * t * t);
            let (h00, h10, h01, h11) = (
                2.0 * t3 - 3.0 * t2 + 1.0,
                t3 - 2.0 * t2 + t,
                -2.0 * t3 + 3.0 * t2,
                t3 - t2,
            );
            [
                h00 * p0.0 + h10 * t0[0] + h01 * p1.0 + h11 * t1[0],
                h00 * p0.1 + h10 * t0[1] + h01 * p1.1 + h11 * t1[1],
            ]
        })
        .collect()
}

/// A generated ribbon whose width runs linearly from `w0` to `w1`.
fn generated(id: String, polyline: Vec<[f64; 2]>, w0: (f64, f64), w1: (f64, f64)) -> Ribbon {
    let len: f64 = polyline
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum();
    let mut r = Ribbon::from_spec(&RibbonSpec {
        id,
        polyline,
        width: vec![
            WidthPoint {
                s: 0.0,
                y_min: w0.0,
                y_max: w0.1,
            },
            WidthPoint {
                s: len,
                y_min: w1.0,
                y_max: w1.1,
            },
        ],
        next: None,
    });
    r.internal = true;
    r.measure_radius();
    r
}

/// The stretches of `a` and `b` that lie within each other's width, if any.
fn overlap(a: &Ribbon, b: &Ribbon) -> Option<((f64, f64), (f64, f64))> {
    let scan = |p: &Ribbon, q: &Ribbon| -> Option<((f64, f64), (f64, f64))> {
        let (mut own, mut other) = (
            (f64::INFINITY, f64::NEG_INFINITY),
            (f64::INFINITY, f64::NEG_INFINITY),
        );
        let n = (p.length() / OVERLAP_STEP).ceil().max(1.0) as usize;
        for k in 0..=n {
            let s = p.length() * k as f64 / n as f64;
            let (lo, hi) = p.y_bounds(s);
            for y in [lo, 0.5 * (lo + hi), hi] {
                let (x, yw, _) = p.to_world(s, y);
                let (sq, yq, _) = q.nearest(x, yw);
                let (qlo, qhi) = q.y_bounds(sq);
                // Strictly inside the other's length: touching ends are a handover, not an overlap.
                if sq > 0.0 && sq < q.length() && yq > qlo && yq < qhi {
                    own = (own.0.min(s), own.1.max(s));
                    other = (other.0.min(sq), other.1.max(sq));
                }
            }
        }
        (own.0 <= own.1).then_some((own, other))
    };
    match (scan(a, b), scan(b, a)) {
        (None, None) => None,
        (x, y) => {
            let (mut sa, mut sb) = (
                (f64::INFINITY, f64::NEG_INFINITY),
                (f64::INFINITY, f64::NEG_INFINITY),
            );
            if let Some((own, other)) = x {
                sa = (sa.0.min(own.0), sa.1.max(own.1));
                sb = (sb.0.min(other.0), sb.1.max(other.1));
            }
            if let Some((own, other)) = y {
                sb = (sb.0.min(own.0), sb.1.max(own.1));
                sa = (sa.0.min(other.0), sa.1.max(other.1));
            }
            Some((sa, sb))
        }
    }
}

/// Build junction `spec` (number `index`): append its ribbons to `ribbons`
/// and set the branch tables of the ribbons that arrive at it.
#[allow(clippy::needless_range_loop)]
pub(crate) fn build(
    spec: &JunctionSpec,
    index: usize,
    ribbons: &mut Vec<Ribbon>,
    by_id: &mut IndexMap<String, usize>,
) -> Result<Junction, NetError> {
    let id = spec.id.as_str();
    if spec.arms.is_empty() {
        return Err(bad(id, "needs at least one arm"));
    }
    let mut arms = Vec::new();
    for a in &spec.arms {
        if spec.arms.iter().filter(|b| b.id == a.id).count() > 1 {
            return Err(bad(id, format!("duplicate arm `{}`", a.id)));
        }
        let find = |name: &Option<String>| -> Result<Option<usize>, NetError> {
            match name {
                None => Ok(None),
                Some(n) => by_id
                    .get(n)
                    .copied()
                    .map(Some)
                    .ok_or_else(|| bad(id, format!("arm `{}`: unknown ribbon `{n}`", a.id))),
            }
        };
        let (inbound, outbound) = (find(&a.inbound)?, find(&a.out)?);
        if let Some(r) = inbound
            && (ribbons[r].next.is_some() || !ribbons[r].branch.is_empty())
        {
            return Err(bad(
                id,
                format!(
                    "arm `{}`: ribbon `{}` already continues elsewhere (remove its `next`)",
                    a.id, ribbons[r].id
                ),
            ));
        }
        arms.push(Arm {
            id: a.id.clone(),
            inbound,
            outbound,
        });
    }
    let n = arms.len();
    let arm_of = |name: &str| arms.iter().position(|a| a.id == name);
    // Turning shares.
    let mut share = vec![vec![0.0; n]; n];
    for (from, tos) in &spec.turns {
        let f = arm_of(from).ok_or_else(|| bad(id, format!("turns: unknown arm `{from}`")))?;
        if arms[f].inbound.is_none() {
            return Err(bad(id, format!("turns: arm `{from}` has no `in` ribbon")));
        }
        for (to, &x) in tos {
            let t = arm_of(to).ok_or_else(|| bad(id, format!("turns: unknown arm `{to}`")))?;
            if !(x >= 0.0 && x.is_finite()) {
                return Err(bad(id, format!("turns: `{from}` to `{to}` must be >= 0")));
            }
            if x > 0.0 && arms[t].outbound.is_none() {
                return Err(bad(id, format!("turns: arm `{to}` has no `out` ribbon")));
            }
            share[f][t] = x;
        }
    }
    let mut turns = vec![Vec::new(); n];
    for f in 0..n {
        if arms[f].inbound.is_none() {
            continue;
        }
        let total: f64 = share[f].iter().sum();
        if total <= 0.0 {
            return Err(bad(
                id,
                format!("turns: arm `{}` needs a positive share", arms[f].id),
            ));
        }
        let mut cum = 0.0;
        turns[f] = share[f]
            .iter()
            .map(|x| {
                cum += x / total;
                if *x > 0.0 { cum } else { 0.0 }
            })
            .collect();
        // Guard the last positive share against rounding.
        if let Some(k) = turns[f].iter().rposition(|&c| c > 0.0) {
            turns[f][k] = 1.0;
        }
    }
    let mut j = Junction {
        id: id.to_string(),
        kind: spec.kind,
        arms,
        turns,
        movements: Vec::new(),
        movement_of: vec![vec![None; n]; n],
        ribbons: Vec::new(),
        conflicts: Vec::new(),
        control: Control::Uncontrolled,
        sign_visibility: spec.sign_visibility_distance,
        u_turn_path: spec.u_turn_path.clone(),
    };
    let mut add =
        |ribbons: &mut Vec<Ribbon>, j: &mut Junction, mut r: Ribbon| -> Result<usize, NetError> {
            let k = ribbons.len();
            r.junction = Some(index);
            if by_id.insert(r.id.clone(), k).is_some() {
                return Err(NetError::Duplicate(r.id.clone()));
            }
            ribbons.push(r);
            j.ribbons.push(k);
            Ok(k)
        };
    let turn_of = |h_in: f64, h_out: f64, same_arm: bool| {
        let d = wrap(h_out - h_in);
        if same_arm || d.abs() > 3.0 * FRAC_PI_4 {
            Turn::UTurn
        } else if d > FRAC_PI_4 {
            Turn::Left
        } else if d < -FRAC_PI_4 {
            Turn::Right
        } else {
            Turn::Through
        }
    };
    let served = |f: usize, t: usize| share[f][t] > 0.0;
    match spec.kind {
        JunctionKind::Box => {
            for f in 0..n {
                let Some(rin) = j.arms[f].inbound else {
                    continue;
                };
                let mut branch = vec![None; n];
                for t in 0..n {
                    if !served(f, t) {
                        continue;
                    }
                    let rout = j.arms[t].outbound.expect("checked above");
                    let (p0, p1) = (ribbons[rin].end_pose(), ribbons[rout].start_pose());
                    let (w0, w1) = (
                        ribbons[rin].y_bounds(ribbons[rin].length()),
                        ribbons[rout].y_bounds(0.0),
                    );
                    let mut m = generated(
                        format!("{id}:{}>{}", j.arms[f].id, j.arms[t].id),
                        hermite(p0, p1),
                        w0,
                        w1,
                    );
                    m.next = Some(rout);
                    let turn = turn_of(p0.2, p1.2, f == t);
                    let k = add(ribbons, &mut j, m)?;
                    branch[t] = Some(k);
                    // A U-turn is made from the median side: the strip a
                    // vehicle of any width can be in is set by the core.
                    j.movement_of[f][t] = Some(j.movements.len());
                    j.movements.push(Movement {
                        from: f,
                        to: t,
                        first: k,
                        turn,
                        window: None,
                    });
                }
                ribbons[rin].branch = branch;
                ribbons[rin].junction = Some(index);
                ribbons[rin].arm_in = Some(f);
            }
        }
        JunctionKind::Split => {
            for f in 0..n {
                let Some(rin) = j.arms[f].inbound else {
                    continue;
                };
                let end = ribbons[rin].end_pose();
                let (in_lo, in_hi) = ribbons[rin].y_bounds(ribbons[rin].length());
                let mut branch = vec![None; n];
                for t in 0..n {
                    if !served(f, t) {
                        continue;
                    }
                    let rout = j.arms[t].outbound.expect("checked above");
                    let start = ribbons[rout].start_pose();
                    if (start.0 - end.0).hypot(start.1 - end.1) > 1e-6 {
                        return Err(bad(
                            id,
                            format!(
                                "split: ribbon `{}` must start where `{}` ends (give it the part of the width it takes)",
                                ribbons[rout].id, ribbons[rin].id
                            ),
                        ));
                    }
                    let (lo, hi) = ribbons[rout].y_bounds(0.0);
                    branch[t] = Some(rout);
                    j.movement_of[f][t] = Some(j.movements.len());
                    j.movements.push(Movement {
                        from: f,
                        to: t,
                        first: rout,
                        turn: turn_of(end.2, start.2, false),
                        window: Some((lo.max(in_lo), hi.min(in_hi))),
                    });
                }
                ribbons[rin].branch = branch;
                ribbons[rin].junction = Some(index);
                ribbons[rin].arm_in = Some(f);
            }
        }
        JunctionKind::Roundabout => build_ring(spec, index, ribbons, &mut j, &mut add, &served)?,
    }
    // Priorities and the signal plan.
    let first_of = |j: &Junction, text: &str| -> Result<usize, NetError> {
        let (from, to) = text
            .split_once('>')
            .ok_or_else(|| bad(id, format!("movement `{text}` must be written `from>to`")))?;
        let (f, t) = (
            arm_of_in(&j.arms, from.trim())
                .ok_or_else(|| bad(id, format!("unknown arm `{from}`")))?,
            arm_of_in(&j.arms, to.trim()).ok_or_else(|| bad(id, format!("unknown arm `{to}`")))?,
        );
        j.movement_of[f][t]
            .map(|m| j.movements[m].first)
            .ok_or_else(|| bad(id, format!("no movement `{text}`")))
    };
    match &spec.control {
        ControlSpec::Uncontrolled { major } => {
            if spec.kind == JunctionKind::Box {
                let mut is_major = vec![false; n];
                for m in major {
                    let a = arm_of_in(&j.arms, m)
                        .ok_or_else(|| bad(id, format!("control.major: unknown arm `{m}`")))?;
                    is_major[a] = true;
                }
                if !major.is_empty() {
                    // The priority ranks of two-way stop control (HCM 2010,
                    // ch. 19), mirrored for left-hand traffic.
                    for m in &j.movements {
                        ribbons[m.first].rank = match (is_major[m.from], m.turn) {
                            (true, Turn::Through | Turn::Left) => 1,
                            (true, Turn::Right | Turn::UTurn) | (false, Turn::Left) => 2,
                            (false, Turn::Through) => 3,
                            (false, Turn::Right | Turn::UTurn) => 4,
                        };
                    }
                }
            } else if !major.is_empty() {
                return Err(bad(id, "control.major applies to a box junction"));
            }
        }
        ControlSpec::Signal {
            phases,
            offset_s,
            countdown,
        } => {
            if spec.kind != JunctionKind::Box {
                return Err(bad(id, "a signal needs a box junction"));
            }
            if phases.is_empty() {
                return Err(bad(id, "a signal needs at least one phase"));
            }
            let mut built = Vec::new();
            for p in phases {
                if !(p.green_s > 0.0 && p.amber_s >= 0.0 && p.all_red_s >= 0.0) {
                    return Err(bad(
                        id,
                        "phase times: green_s > 0, amber_s and all_red_s >= 0",
                    ));
                }
                let mut green = Vec::new();
                let mut phase_arms = Vec::new();
                for a in &p.arms {
                    let f = arm_of_in(&j.arms, a)
                        .ok_or_else(|| bad(id, format!("phase: unknown arm `{a}`")))?;
                    green.extend(j.movements.iter().filter(|m| m.from == f).map(|m| m.first));
                    phase_arms.push(f);
                }
                for m in &p.movements {
                    let first = first_of(&j, m)?;
                    green.push(first);
                    let from = j
                        .movements
                        .iter()
                        .find(|x| x.first == first)
                        .map(|x| x.from);
                    phase_arms.extend(from);
                }
                green.sort_unstable();
                green.dedup();
                phase_arms.sort_unstable();
                phase_arms.dedup();
                if green.is_empty() {
                    return Err(bad(id, "a phase gives green to no movement"));
                }
                built.push(Phase {
                    green,
                    arms: phase_arms,
                    green_s: p.green_s,
                    amber_s: p.amber_s,
                    all_red_s: p.all_red_s,
                });
            }
            for m in &j.movements {
                if !built.iter().any(|p| p.green.contains(&m.first)) {
                    return Err(bad(
                        id,
                        format!(
                            "movement `{}>{}` never has green",
                            j.arms[m.from].id, j.arms[m.to].id
                        ),
                    ));
                }
                // With green together, a turn across the opposing stream
                // waits for a gap in it (a permitted turn).
                ribbons[m.first].rank = match m.turn {
                    Turn::Through | Turn::Left => 1,
                    Turn::Right | Turn::UTurn => 2,
                };
            }
            let cycle_s = built
                .iter()
                .map(|p| p.green_s + p.amber_s + p.all_red_s)
                .sum();
            j.control = Control::Signal {
                phases: built,
                cycle_s,
                offset_s: *offset_s,
                countdown: *countdown,
            };
        }
    }
    // Where the ribbons inside the junction overlap.
    if spec.kind != JunctionKind::Split {
        let own = j.ribbons.clone();
        // Ribbons that hand over to one another do not conflict.
        let follows =
            |a: usize, b: usize| ribbons[a].next == Some(b) || ribbons[a].branch.contains(&Some(b));
        let feeders =
            |r: usize| -> Vec<usize> { (0..ribbons.len()).filter(|&q| follows(q, r)).collect() };
        let successors = |r: usize| -> Vec<usize> {
            let mut v: Vec<usize> = ribbons[r].branch.iter().flatten().copied().collect();
            v.extend(ribbons[r].next);
            v.sort_unstable();
            v.dedup();
            v
        };
        for (x, &a) in own.iter().enumerate() {
            for &b in &own[x + 1..] {
                if follows(a, b) || follows(b, a) {
                    continue;
                }
                let Some((a_s, b_s)) = overlap(&ribbons[a], &ribbons[b]) else {
                    continue;
                };
                let (fa, fb) = (feeders(a), feeders(b));
                let (sa, sb) = (successors(a), successors(b));
                let kind = if fa.iter().any(|q| fb.contains(q)) {
                    ConflictKind::Diverge
                } else if sa.iter().any(|q| sb.contains(q)) {
                    ConflictKind::Merge
                } else {
                    ConflictKind::Crossing
                };
                j.conflicts.push(ConflictArea {
                    a,
                    b,
                    a_s,
                    b_s,
                    kind,
                });
            }
        }
    }
    Ok(j)
}

fn arm_of_in(arms: &[Arm], name: &str) -> Option<usize> {
    arms.iter().position(|a| a.id == name)
}

/// A point of the ring and the heading of clockwise travel there.
fn ring_pose(c: [f64; 2], radius: f64, angle: f64) -> (f64, f64, f64) {
    (
        c[0] + radius * angle.cos(),
        c[1] + radius * angle.sin(),
        angle - 0.5 * PI,
    )
}

/// The ring of a roundabout: arcs between the points where arms leave and
/// join it, a connector onto the ring from each arriving arm and one off it
/// to each leaving arm. Circulating ribbons go first (rank 1); joining
/// connectors wait for a gap (rank 2).
#[allow(clippy::needless_range_loop)]
fn build_ring(
    spec: &JunctionSpec,
    index: usize,
    ribbons: &mut Vec<Ribbon>,
    j: &mut Junction,
    add: &mut impl FnMut(&mut Vec<Ribbon>, &mut Junction, Ribbon) -> Result<usize, NetError>,
    served: &impl Fn(usize, usize) -> bool,
) -> Result<(), NetError> {
    let id = spec.id.as_str();
    let (Some(c), Some(radius), Some(width)) = (spec.centre, spec.ring_radius, spec.ring_width)
    else {
        return Err(bad(
            id,
            "a roundabout needs centre, ring_radius and ring_width",
        ));
    };
    if !(radius > 0.0 && width > 0.0 && width < 2.0 * radius) {
        return Err(bad(
            id,
            "ring_radius > 0 and 0 < ring_width < 2 ring_radius",
        ));
    }
    let n = j.arms.len();
    let half = 0.5 * width;
    let ring_w = (-half, half);
    // Where each arm leaves (`true`) and joins (`false`) the ring: the
    // angle of its ribbon's end, moved along the ring by the distance the
    // connector has to cover, so that it meets the ring at about 45°.
    #[derive(Clone, Copy)]
    struct Node {
        angle: f64,
        arm: usize,
        exit: bool,
    }
    let mut nodes = Vec::new();
    for a in 0..n {
        let reach = |p: (f64, f64, f64)| {
            let d = (p.0 - c[0]).hypot(p.1 - c[1]);
            (
                (p.1 - c[1]).atan2(p.0 - c[0]),
                ((d - radius).abs() / radius).min(FRAC_PI_4),
            )
        };
        if let Some(r) = j.arms[a].inbound {
            let (angle, lead) = reach(ribbons[r].end_pose());
            nodes.push(Node {
                angle: wrap(angle - lead),
                arm: a,
                exit: false,
            });
        }
        if let Some(r) = j.arms[a].outbound
            && (0..n).any(|f| served(f, a))
        {
            let (angle, lead) = reach(ribbons[r].start_pose());
            nodes.push(Node {
                angle: wrap(angle + lead),
                arm: a,
                exit: true,
            });
        }
    }
    if nodes.len() < 2 {
        return Err(bad(
            id,
            "a roundabout needs at least one way in and one way out",
        ));
    }
    // Clockwise: descending angle.
    nodes.sort_by(|x, y| y.angle.total_cmp(&x.angle));
    let m = nodes.len();
    // Arc k runs from node k to node k+1.
    let mut arcs = Vec::with_capacity(m);
    for k in 0..m {
        let (from, to) = (nodes[k], nodes[(k + 1) % m]);
        let mut sweep = (from.angle - to.angle).rem_euclid(TAU);
        if sweep < 1e-9 {
            if m == 2 || from.arm != to.arm {
                sweep = TAU;
            } else {
                return Err(bad(id, "two arms meet the ring at the same point"));
            }
        }
        let steps = (sweep / ARC_STEP).ceil().max(2.0) as usize;
        let line = (0..=steps)
            .map(|q| {
                let p = ring_pose(c, radius, from.angle - sweep * q as f64 / steps as f64);
                [p.0, p.1]
            })
            .collect();
        let mut arc = generated(format!("{id}:ring{k}"), line, ring_w, ring_w);
        arc.rank = 1;
        arcs.push(add(ribbons, j, arc)?);
    }
    let _ = index;
    // Connectors, and what follows each arc.
    let mut entry_of = vec![None; n];
    let mut exit_of = vec![None; n];
    for (k, node) in nodes.iter().enumerate() {
        let on_ring = ring_pose(c, radius, node.angle);
        let a = node.arm;
        if node.exit {
            let rout = j.arms[a].outbound.expect("exit node");
            let mut x = generated(
                format!("{id}:off>{}", j.arms[a].id),
                hermite(on_ring, ribbons[rout].start_pose()),
                ring_w,
                ribbons[rout].y_bounds(0.0),
            );
            x.next = Some(rout);
            x.rank = 1;
            exit_of[a] = Some((add(ribbons, j, x)?, k));
        } else {
            let rin = j.arms[a].inbound.expect("entry node");
            let mut e = generated(
                format!("{id}:{}>on", j.arms[a].id),
                hermite(ribbons[rin].end_pose(), on_ring),
                ribbons[rin].y_bounds(ribbons[rin].length()),
                ring_w,
            );
            e.rank = 2;
            e.branch = vec![Some(arcs[k]); n];
            entry_of[a] = Some(add(ribbons, j, e)?);
        }
    }
    for (k, &arc) in arcs.iter().enumerate() {
        let end = nodes[(k + 1) % m];
        let onward = Some(arcs[(k + 1) % m]);
        let mut branch = vec![onward; n];
        if end.exit {
            branch[end.arm] = exit_of[end.arm].map(|(r, _)| r);
        }
        ribbons[arc].branch = branch;
    }
    for f in 0..n {
        let Some(rin) = j.arms[f].inbound else {
            continue;
        };
        let entry = entry_of[f].expect("an arriving arm has a connector");
        let mut branch = vec![None; n];
        for (t, slot) in branch.iter_mut().enumerate() {
            if served(f, t) {
                *slot = Some(entry);
                j.movement_of[f][t] = Some(j.movements.len());
                j.movements.push(Movement {
                    from: f,
                    to: t,
                    first: entry,
                    turn: if f == t { Turn::UTurn } else { Turn::Through },
                    window: None,
                });
            }
        }
        ribbons[rin].branch = branch;
        ribbons[rin].junction = Some(index);
        ribbons[rin].arm_in = Some(f);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Network;
    use marg_params::Scenario;

    /// A four-arm crossing of two-way roads, 7 m per direction, left-hand
    /// traffic; each arm has a ribbon in and a ribbon out.
    pub(crate) fn four_arm(kind_and_control: &str, turns: &str) -> String {
        let mut ribbons = String::new();
        // Arm axis directions pointing away from the centre.
        for (name, ax, ay) in [
            ("e", 1.0, 0.0),
            ("n", 0.0, 1.0),
            ("w", -1.0, 0.0),
            ("s", 0.0, -1.0),
        ] {
            // Left of travel towards the centre is (ay, -ax) rotated: for
            // the arm east, inbound travels west, its left is south.
            let (lx, ly): (f64, f64) = (-ay, ax);
            // Inbound on the left side seen driving in: offset by -left of outward axis.
            let off = 4.0;
            let (ix, iy) = (-lx * off, -ly * off);
            let (ox, oy) = (lx * off, ly * off);
            ribbons += &format!(
                "    - {{id: {name}_in, polyline: [[{}, {}], [{}, {}]], width: [{{s: 0, y_min: -3.5, y_max: 3.5}}]}}\n",
                ax * 200.0 + ix,
                ay * 200.0 + iy,
                ax * 12.0 + ix,
                ay * 12.0 + iy
            );
            ribbons += &format!(
                "    - {{id: {name}_out, polyline: [[{}, {}], [{}, {}]], width: [{{s: 0, y_min: -3.5, y_max: 3.5}}]}}\n",
                ax * 12.0 + ox,
                ay * 12.0 + oy,
                ax * 200.0 + ox,
                ay * 200.0 + oy
            );
        }
        format!(
            "name: x\nduration_s: 10\nnetwork:\n  ribbons:\n{ribbons}  junctions:\n    - id: j\n{kind_and_control}      arms:\n        - {{id: e, in: e_in, out: e_out}}\n        - {{id: n, in: n_in, out: n_out}}\n        - {{id: w, in: w_in, out: w_out}}\n        - {{id: s, in: s_in, out: s_out}}\n      turns:\n{turns}demand: {{entries: []}}\n"
        )
    }

    pub(crate) const ALL_TURNS: &str = "        e: {n: 1, w: 2, s: 1}\n        n: {w: 1, s: 2, e: 1}\n        w: {s: 1, e: 2, n: 1}\n        s: {e: 1, n: 2, w: 1}\n";

    fn net(yaml: &str) -> Network {
        Network::build(&Scenario::from_yaml(yaml).unwrap().network).unwrap()
    }

    #[test]
    fn four_arm_box_has_every_movement_and_its_conflicts() {
        let n = net(&four_arm("", ALL_TURNS));
        let j = &n.junctions[0];
        assert_eq!(j.movements.len(), 12);
        let count = |t: Turn| j.movements.iter().filter(|m| m.turn == t).count();
        assert_eq!(
            (count(Turn::Through), count(Turn::Left), count(Turn::Right)),
            (4, 4, 4)
        );
        // Left-hand traffic: from the east arm, south is the near (left) turn.
        let e = j.arms.iter().position(|a| a.id == "e").unwrap();
        let s = j.arms.iter().position(|a| a.id == "s").unwrap();
        assert_eq!(j.movements[j.movement_of[e][s].unwrap()].turn, Turn::Left);
        // Every arriving ribbon branches to three movement ribbons, each of
        // which continues onto its leaving ribbon.
        for m in &j.movements {
            let rin = j.arms[m.from].inbound.unwrap();
            assert_eq!(n.ribbon(rin).branch[m.to], Some(m.first));
            assert_eq!(n.ribbon(m.first).next, j.arms[m.to].outbound);
            assert!(n.ribbon(m.first).internal);
        }
        // Movements from one arm diverge (3 pairs per arm), movements to one
        // arm merge (3 pairs per arm). The 16 pairs whose centre lines cross
        // in the classic four-arm diagram are all found; wide ribbons add
        // pairs whose edges overlap though their centre lines do not cross.
        let kind = |k: ConflictKind| j.conflicts.iter().filter(|c| c.kind == k).count();
        assert_eq!(kind(ConflictKind::Diverge), 12);
        assert_eq!(kind(ConflictKind::Merge), 12);
        assert!(
            kind(ConflictKind::Crossing) >= 16,
            "{}",
            kind(ConflictKind::Crossing)
        );
        // Opposing through movements run side by side and do not conflict.
        let w = j.arms.iter().position(|a| a.id == "w").unwrap();
        let (ew, we) = (
            j.movements[j.movement_of[e][w].unwrap()].first,
            j.movements[j.movement_of[w][e].unwrap()].first,
        );
        assert!(!j.conflicts_of(ew).any(|(o, ..)| o == we));
        // A through movement is straight; a turn is not.
        assert!(n.ribbon(ew).min_radius.is_infinite());
        let es = j.movements[j.movement_of[e][s].unwrap()].first;
        assert!(n.ribbon(es).min_radius < 30.0);
        // The stretch of a conflict lies inside both ribbons.
        for c in &j.conflicts {
            assert!(c.a_s.0 >= 0.0 && c.a_s.1 <= n.ribbon(c.a).length() + 1e-9);
            assert!(c.b_s.0 >= 0.0 && c.b_s.1 <= n.ribbon(c.b).length() + 1e-9);
        }
    }

    #[test]
    fn priority_ranks_and_signal_plans() {
        let n = net(&four_arm(
            "      control: {kind: uncontrolled, major: [e, w]}\n",
            ALL_TURNS,
        ));
        let j = &n.junctions[0];
        let rank = |f: &str, t: &str| {
            let (f, t) = (
                j.arms.iter().position(|a| a.id == f).unwrap(),
                j.arms.iter().position(|a| a.id == t).unwrap(),
            );
            n.ribbon(j.movements[j.movement_of[f][t].unwrap()].first)
                .rank
        };
        assert_eq!((rank("e", "w"), rank("e", "s"), rank("e", "n")), (1, 1, 2));
        assert_eq!((rank("n", "e"), rank("n", "s"), rank("n", "w")), (2, 3, 4));
        let sig = "      control:\n        kind: signal\n        phases:\n          - {arms: [e, w], green_s: 30, amber_s: 3, all_red_s: 2}\n          - {arms: [n, s], green_s: 20, amber_s: 3, all_red_s: 2}\n";
        let n = net(&four_arm(sig, ALL_TURNS));
        let Control::Signal {
            phases, cycle_s, ..
        } = &n.junctions[0].control
        else {
            panic!("not a signal")
        };
        assert_eq!(*cycle_s, 60.0);
        assert_eq!((phases[0].green.len(), phases[1].green.len()), (6, 6));
        let bad = "      control:\n        kind: signal\n        phases:\n          - {arms: [e, w], green_s: 30, amber_s: 3}\n";
        let err = Network::build(
            &Scenario::from_yaml(&four_arm(bad, ALL_TURNS))
                .unwrap()
                .network,
        );
        assert!(matches!(err, Err(NetError::Junction(_, m)) if m.contains("never has green")));
    }

    #[test]
    fn roundabout_is_a_ring_closed_only_through_branches() {
        let n = net(&four_arm(
            "      kind: roundabout\n      centre: [0, 0]\n      ring_radius: 14\n      ring_width: 7\n",
            ALL_TURNS,
        ));
        let j = &n.junctions[0];
        // Four ways on, four ways off, eight arcs.
        assert_eq!(j.ribbons.len(), 16);
        // Follow each movement round the ring: it ends on its leaving ribbon
        // within one lap.
        for m in &j.movements {
            let (mut at, mut steps) = (j.arms[m.from].inbound.unwrap(), 0);
            let target = j.arms[m.to].outbound.unwrap();
            while at != target {
                let r = n.ribbon(at);
                at = r.next.or_else(|| r.branch[m.to]).expect("continues");
                steps += 1;
                assert!(
                    steps <= 12,
                    "movement {}>{} does not leave the ring",
                    m.from,
                    m.to
                );
            }
        }
        // Circulating ribbons go first; a joining connector conflicts with
        // the arc that ends where it joins.
        for m in &j.movements {
            assert_eq!(n.ribbon(m.first).rank, 2);
            assert!(
                j.conflicts_of(m.first)
                    .any(|(o, _, _, k)| k == ConflictKind::Merge && n.ribbon(o).rank == 1)
            );
        }
        // No `next` chain cycles.
        for r in &n.ribbons {
            if r.id.contains("ring") {
                assert!(r.next.is_none());
            }
        }
    }

    #[test]
    fn split_hands_over_directly_with_a_window_per_branch() {
        let yaml = "name: x\nduration_s: 10\nnetwork:\n  ribbons:\n    - {id: main, polyline: [[0, 0], [500, 0]], width: [{s: 0, y_min: -7, y_max: 7}]}\n    - {id: left, polyline: [[500, 0], [600, 10], [900, 60]], width: [{s: 0, y_min: 0, y_max: 7}]}\n    - {id: right, polyline: [[500, 0], [600, -10], [900, -60]], width: [{s: 0, y_min: -7, y_max: 0}]}\n  junctions:\n    - id: fork\n      kind: split\n      arms: [{id: a, in: main}, {id: l, out: left}, {id: r, out: right}]\n      turns: {a: {l: 0.3, r: 0.7}}\ndemand: {entries: []}\n";
        let n = net(yaml);
        let j = &n.junctions[0];
        assert!(j.ribbons.is_empty() && j.conflicts.is_empty());
        let main = n.index("main").unwrap();
        assert_eq!(
            n.ribbon(main).branch,
            vec![None, n.index("left").ok(), n.index("right").ok()]
        );
        assert_eq!(j.movements[0].window, Some((0.0, 7.0)));
        assert_eq!(j.movements[1].window, Some((-7.0, 0.0)));
        assert_eq!(j.draw_out(0, 0.1), 1);
        assert_eq!(j.draw_out(0, 0.5), 2);
        assert_eq!(j.draw_out(0, 0.999_999), 2);
        assert_eq!(n.ribbon(main).ahead, Some((0, 0)));
    }
}
