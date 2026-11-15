//! The violation decision engine (G§4, ADR-0018, phase P8.3).
//!
//! Each violation type has a trigger (when an agent is in a position to
//! violate), features, and an act. While the trigger holds the agent is in
//! an *episode*. A one-shot type decides once per episode (`P = σ(η)`); a
//! continuous type runs an integrated-hazard clock (`λ = λ_max σ(η)`) read at
//! the start of the episode and at the agent's decision ticks. Every episode
//! ends in one event: `violation` (it acted) or `complied` (the trigger
//! ended first), with its features and each term of the logit (rule 9).
//! `violations.enabled = 0` (the cooperative twin, ADR-0007) starts none.
//!
//! Imitation counts come from the other agents' episodes inside the
//! agent's field of view (ADR-0018 parts 3–4): a *violator* has acted and is
//! moving; a *complier* is in an episode of the same type, has not acted and
//! is standing.

use crate::model::decision::{Coefficients, Features, HazardClock, Terms, hazard, one_shot};
use crate::personas::{N_TRAITS, PersonaParams};
use marg_params::{Persona, ResolveError, ResolvedSpec};
use std::collections::BTreeMap;

/// Counter base of violation decisions in an agent's `Decisions` stream:
/// one uniform per (type, episode), at `VIOLATION_BASE + type·2³⁶ + episode`.
pub const VIOLATION_BASE: u64 = 8 << 40;
const KIND_BLOCK: u64 = 1 << 36;

/// The violation types of plan/03 §7 (stop-anywhere keeps its own module,
/// ADR-0014).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    RedLight = 0,
    EarlyStart = 1,
    AmberGo = 2,
    StoplineSeepage = 3,
    WrongWay = 4,
    Footpath = 5,
    ForcedEntry = 6,
    LateMerge = 7,
    BoxBlocking = 8,
    OppositeEncroachment = 9,
}

pub const N_KINDS: usize = 10;

impl Kind {
    pub const ALL: [Kind; N_KINDS] = [
        Kind::RedLight,
        Kind::EarlyStart,
        Kind::AmberGo,
        Kind::StoplineSeepage,
        Kind::WrongWay,
        Kind::Footpath,
        Kind::ForcedEntry,
        Kind::LateMerge,
        Kind::BoxBlocking,
        Kind::OppositeEncroachment,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Kind::RedLight => "red_light",
            Kind::EarlyStart => "early_start",
            Kind::AmberGo => "amber_go",
            Kind::StoplineSeepage => "stopline_seepage",
            Kind::WrongWay => "wrong_way",
            Kind::Footpath => "footpath",
            Kind::ForcedEntry => "forced_entry",
            Kind::LateMerge => "late_merge",
            Kind::BoxBlocking => "box_blocking",
            Kind::OppositeEncroachment => "opposite_encroachment",
        }
    }

    /// Continuous (a hazard) or one-shot (ADR-0018 part 1).
    pub fn continuous(self) -> bool {
        !matches!(self, Kind::AmberGo | Kind::WrongWay)
    }

    /// Whether the type's trigger and act exist yet (P8.4 adds the rest).
    pub fn built(self) -> bool {
        matches!(self, Kind::ForcedEntry)
    }
}

/// Register paths of the shared coefficients, in [`Coefficients`] order
/// after the intercept.
const COEFFICIENTS: [&str; 7] = [
    "aggressiveness",
    "time_saved",
    "waiting_time",
    "imitation_violators",
    "imitation_compliers",
    "enforcement",
    "risk",
];

/// The rule's parameters as the register has them.
#[derive(Debug, Clone, PartialEq)]
pub struct ViolationParams {
    /// `violations.enabled`: 0 in the cooperative twin.
    pub enabled: bool,
    /// `β0` by type; a type runs only if its intercept is set.
    pub intercept: [Option<f64>; N_KINDS],
    /// The shared coefficients (`None`: unset).
    pub coefficient: [Option<f64>; 7],
    /// Added to every intercept of a delivery rider (G§3.2, stage 6).
    pub delivery_shift: Option<f64>,
    /// `λ_max` (1/s).
    pub max_rate: f64,
    /// Violators of any type count in imitation.
    pub any_type: bool,
    /// Field of view: radius (m) and the cosine of its half-angle.
    pub fov_radius: f64,
    pub fov_cos: f64,
}

fn optional(spec: &ResolvedSpec, path: &str) -> Result<Option<f64>, ResolveError> {
    match spec.scalar_si(path) {
        Ok(x) => Ok(Some(x)),
        Err(ResolveError::Unset { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

impl ViolationParams {
    pub fn from_spec(spec: &ResolvedSpec) -> Result<Self, ResolveError> {
        let mut intercept = [None; N_KINDS];
        for k in Kind::ALL {
            intercept[k as usize] = optional(spec, &format!("violations.{}.intercept", k.key()))?;
        }
        let mut coefficient = [None; 7];
        for (c, name) in COEFFICIENTS.iter().enumerate() {
            coefficient[c] = optional(spec, &format!("violations.coefficients.{name}"))?;
        }
        Ok(Self {
            enabled: spec.scalar_si("violations.enabled")? != 0.0,
            intercept,
            coefficient,
            delivery_shift: optional(spec, "personas.delivery_rider.violation_intercept_shift")?,
            max_rate: spec.scalar_si("violations.hazard.max_rate")?,
            any_type: spec.scalar_si("violations.imitation.any_type")? != 0.0,
            fov_radius: spec.scalar_si("violations.imitation.fov_radius")?,
            fov_cos: libm::cos(spec.scalar_si("violations.imitation.fov_half_angle")?),
        })
    }

    /// Refuse a type that is switched on but cannot run.
    pub fn check(&self) -> Result<(), String> {
        for k in Kind::ALL {
            if self.intercept[k as usize].is_none() {
                continue;
            }
            if !k.built() {
                return Err(format!(
                    "violations.{}.intercept is set, but the {} violation is not built yet \
                     (P8.4); unset it",
                    k.key(),
                    k.key()
                ));
            }
            let missing: Vec<&str> = COEFFICIENTS
                .iter()
                .zip(&self.coefficient)
                .filter(|(_, c)| c.is_none())
                .map(|(n, _)| *n)
                .collect();
            if !missing.is_empty() {
                return Err(format!(
                    "violations.{}.intercept is set, so every shared coefficient must be: \
                     set violations.coefficients.{{{}}} (the guide has no numbers; 0 is allowed)",
                    k.key(),
                    missing.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Whether type `k` runs in this scenario.
    pub fn active(&self, k: Kind) -> bool {
        self.enabled && self.intercept[k as usize].is_some()
    }

    pub fn any_active(&self) -> bool {
        Kind::ALL.iter().any(|&k| self.active(k))
    }

    /// The coefficients of type `k` for an agent with persona `persona` and
    /// traits `z`: the shared ones, scaled by the agent's traits (a
    /// random-coefficient logit, ADR-0018 part 2).
    pub fn coefficients(
        &self,
        k: Kind,
        persona: u8,
        z: &[f64; N_TRAITS],
        pp: &PersonaParams,
    ) -> Coefficients {
        let c = |n: usize| self.coefficient[n].unwrap_or(0.0);
        let [g_pi, g_rho, g_c, g_eta] = pp.coefficient_link;
        let (pi, rho, conf, norm) = (z[1], z[2], z[3], z[4]);
        let delivery = usize::from(persona) == Persona::DeliveryRider.index();
        Coefficients {
            intercept: self.intercept[k as usize].unwrap_or(0.0)
                + if delivery {
                    self.delivery_shift.unwrap_or(0.0)
                } else {
                    0.0
                },
            aggressiveness: c(0),
            time_saved: c(1) * libm::exp(g_pi * pi),
            waiting_time: c(2) * libm::exp(g_pi * pi),
            imitation_violators: c(3) * libm::exp(g_c * conf),
            imitation_compliers: c(4) * libm::exp(g_eta * norm),
            enforcement: c(5) * libm::exp(g_eta * norm),
            risk: c(6) * libm::exp(-g_rho * rho),
        }
    }
}

/// What one reading of the rule found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub t: f64,
    /// The agent's own coefficients (traits applied).
    pub coefficients: Coefficients,
    pub features: Features,
    pub terms: Terms,
    /// Hazard (1/s) of a continuous type; the probability of a one-shot.
    pub rate: f64,
}

/// One agent's episode of one type.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub agent: u64,
    /// Store index, kept current through compaction.
    pub idx: u32,
    pub kind: Kind,
    pub serial: u32,
    pub t_start: f64,
    pub clock: HazardClock,
    /// One-shot: the uniform it was decided against.
    pub draw: f64,
    pub acted: bool,
    /// The junction it is at, and the ribbon it acted from.
    pub junction: u32,
    pub ribbon: u32,
    /// Step at which its agent last saw its trigger (or, acted, was still
    /// in the act).
    pub touched: u64,
    pub last: Reading,
}

/// Every open episode, in the order they started.
#[derive(Debug, Clone, Default)]
pub struct Episodes {
    pub list: Vec<Episode>,
    serials: BTreeMap<(u64, Kind), u32>,
    /// Somebody acted at the last step: every episode reads its features
    /// now (`recount`); somebody acted this step (`recount_next`).
    pub recount: bool,
    pub recount_next: bool,
    /// Acts so far by type.
    pub acts: [u64; N_KINDS],
}

impl Episodes {
    pub fn find(&self, agent: u64, kind: Kind) -> Option<usize> {
        self.list
            .iter()
            .position(|e| e.agent == agent && e.kind == kind)
    }

    /// The next episode number of `agent` for `kind`.
    pub fn next_serial(&mut self, agent: u64, kind: Kind) -> u32 {
        let n = self.serials.entry((agent, kind)).or_insert(0);
        let s = *n;
        *n += 1;
        s
    }

    /// Follow the store's compaction; episodes of agents that left go.
    pub fn remap(&mut self, remap: &[u32], removed: u32) {
        for e in &mut self.list {
            e.idx = remap[e.idx as usize];
        }
        self.list.retain(|e| e.idx != removed);
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// The counter of an episode's uniform in its agent's `Decisions` stream.
pub fn episode_counter(kind: Kind, serial: u32) -> u64 {
    VIOLATION_BASE + kind as u64 * KIND_BLOCK + u64::from(serial)
}

/// A world pose: position of the centre, heading (rad), front-centre
/// offset (half the length), speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub half_length: f64,
    pub v: f64,
}

/// Violators moving and compliers waiting of type `kind` inside the field of
/// view of `me` (agent id `agent`): a sector from its front centre, radius
/// `p.fov_radius`, half-angle `acos(p.fov_cos)`, about its heading.
pub fn counts_in_view(
    p: &ViolationParams,
    episodes: &Episodes,
    agent: u64,
    kind: Kind,
    me: Pose,
    pose_of: impl Fn(u32) -> Pose,
    stop_speed: f64,
) -> (u32, u32) {
    let (ux, uy) = (libm::cos(me.heading), libm::sin(me.heading));
    let (fx, fy) = (me.x + me.half_length * ux, me.y + me.half_length * uy);
    let (mut violating, mut complying) = (0, 0);
    for e in &episodes.list {
        if e.agent == agent {
            continue;
        }
        let same = e.kind == kind;
        let counts_as_violator = e.acted && (same || p.any_type);
        let counts_as_complier = !e.acted && same;
        if !counts_as_violator && !counts_as_complier {
            continue;
        }
        let o = pose_of(e.idx);
        if counts_as_violator && o.v < stop_speed {
            continue;
        }
        if counts_as_complier && o.v >= stop_speed {
            continue;
        }
        let (dx, dy) = (o.x - fx, o.y - fy);
        let d = libm::hypot(dx, dy);
        if d > p.fov_radius || (d > 0.0 && (dx * ux + dy * uy) < p.fov_cos * d) {
            continue;
        }
        if counts_as_violator {
            violating += 1;
        } else {
            complying += 1;
        }
    }
    (violating, complying)
}

/// The decision's JSON for `events.attrs`: type, episode, features, every
/// term, `η`, and the hazard and clock (continuous) or probability and draw
/// (one-shot).
pub fn attrs(e: &Episode, junction: &str, max_rate: f64, reason: &str) -> String {
    let f = &e.last.features;
    let c = &e.last.coefficients;
    let mut terms = String::new();
    for (k, (name, x)) in e.last.terms.named().enumerate() {
        if k > 0 {
            terms.push(',');
        }
        terms.push_str(&format!("\"{name}\":{x}"));
    }
    let decision = if e.kind.continuous() {
        format!(
            "\"hazard\":{},\"max_rate\":{max_rate},\"integrated\":{},\"threshold\":{},\"probability\":{}",
            e.last.rate,
            e.clock.integrated(),
            e.clock.threshold(),
            e.clock.probability()
        )
    } else {
        format!("\"probability\":{},\"draw\":{}", e.last.rate, e.draw)
    };
    format!(
        "{{\"type\":\"{}\",\"junction\":{},\"episode\":{},\"t_start\":{},\"reason\":\"{reason}\",\
         \"features\":{{\"kappa\":{},\"time_saved\":{},\"waiting_time\":{},\"n_violating\":{},\
         \"n_complying\":{},\"enforcement\":{},\"risk\":{}}},\"coefficients\":{{\"intercept\":{},\
         \"aggressiveness\":{},\"time_saved\":{},\"waiting_time\":{},\"imitation_violators\":{},\
         \"imitation_compliers\":{},\"enforcement\":{},\"risk\":{}}},\"terms\":{{{terms}}},\"eta\":{},{decision}}}",
        e.kind.key(),
        crate::sim::json_string(junction),
        e.serial,
        e.t_start,
        f.kappa,
        f.time_saved,
        f.waiting_time,
        f.n_violating,
        f.n_complying,
        f.enforcement,
        f.risk,
        c.intercept,
        c.aggressiveness,
        c.time_saved,
        c.waiting_time,
        c.imitation_violators,
        c.imitation_compliers,
        c.enforcement,
        c.risk,
        e.last.terms.eta(),
    )
}

/// One reading of the rule for `kind`.
pub fn read(kind: Kind, t: f64, c: &Coefficients, features: Features, max_rate: f64) -> Reading {
    let terms = Terms::new(c, &features);
    let rate = if kind.continuous() {
        hazard(max_rate, terms.eta())
    } else {
        terms.probability()
    };
    Reading {
        t,
        coefficients: *c,
        features,
        terms,
        rate,
    }
}

/// Decide a one-shot episode.
pub fn decide_once(r: &Reading, u: f64) -> bool {
    one_shot(r.rate, u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ViolationParams {
        ViolationParams {
            enabled: true,
            intercept: [None; N_KINDS],
            coefficient: [Some(0.0); 7],
            delivery_shift: None,
            max_rate: 1.0,
            any_type: false,
            fov_radius: 30.0,
            fov_cos: 0.0,
        }
    }

    fn episode(agent: u64, idx: u32, kind: Kind, acted: bool) -> Episode {
        Episode {
            agent,
            idx,
            kind,
            serial: 0,
            t_start: 0.0,
            clock: HazardClock::start(0.0, 0.0, 0.5),
            draw: 0.0,
            acted,
            junction: 0,
            ribbon: 0,
            touched: 0,
            last: read(
                kind,
                0.0,
                &Coefficients::default(),
                Features::default(),
                1.0,
            ),
        }
    }

    #[test]
    fn the_field_of_view_counts_ahead_within_its_radius() {
        let p = params();
        let mut eps = Episodes::default();
        // Agent 0 looks along +x from (0, 0), front at (2, 0).
        let poses = [
            Pose {
                x: 0.0,
                y: 0.0,
                heading: 0.0,
                half_length: 2.0,
                v: 0.0,
            },
            Pose {
                x: 12.0,
                y: 3.0,
                heading: 0.0,
                half_length: 2.0,
                v: 5.0,
            }, // violator ahead
            Pose {
                x: 8.0,
                y: -2.0,
                heading: 0.0,
                half_length: 2.0,
                v: 0.0,
            }, // complier ahead
            Pose {
                x: -10.0,
                y: 0.0,
                heading: 0.0,
                half_length: 2.0,
                v: 0.0,
            }, // behind
            Pose {
                x: 60.0,
                y: 0.0,
                heading: 0.0,
                half_length: 2.0,
                v: 6.0,
            }, // too far
            Pose {
                x: 15.0,
                y: 0.0,
                heading: 0.0,
                half_length: 2.0,
                v: 0.0,
            }, // acted, standing
        ];
        eps.list.push(episode(1, 1, Kind::ForcedEntry, true));
        eps.list.push(episode(2, 2, Kind::ForcedEntry, false));
        eps.list.push(episode(3, 3, Kind::ForcedEntry, false));
        eps.list.push(episode(4, 4, Kind::ForcedEntry, true));
        eps.list.push(episode(5, 5, Kind::ForcedEntry, true));
        eps.list.push(episode(0, 0, Kind::ForcedEntry, false)); // itself
        let pose = |i: u32| poses[i as usize];
        let c = counts_in_view(&p, &eps, 0, Kind::ForcedEntry, poses[0], pose, 0.1);
        assert_eq!(c, (1, 1));
        // Another type's violator counts only with the any-type switch.
        eps.list.push(Episode {
            agent: 6,
            ..episode(6, 1, Kind::RedLight, true)
        });
        assert_eq!(
            counts_in_view(&p, &eps, 0, Kind::ForcedEntry, poses[0], pose, 0.1),
            (1, 1)
        );
        let any = ViolationParams {
            any_type: true,
            ..params()
        };
        assert_eq!(
            counts_in_view(&any, &eps, 0, Kind::ForcedEntry, poses[0], pose, 0.1),
            (2, 1)
        );
        // A full circle sees behind too.
        let all_round = ViolationParams {
            fov_cos: -1.0,
            ..params()
        };
        let c = counts_in_view(&all_round, &eps, 0, Kind::ForcedEntry, poses[0], pose, 0.1);
        assert_eq!(c, (1, 2));
    }

    #[test]
    fn traits_scale_only_their_coefficients() {
        let mut p = params();
        p.coefficient = [Some(1.0); 7];
        p.intercept[Kind::ForcedEntry as usize] = Some(-4.0);
        p.delivery_shift = Some(0.5);
        let pp_links = [0.5, 0.5, 0.5, 0.5];
        let mut pp = crate::personas::PersonaParams::from_test();
        pp.coefficient_link = pp_links;
        let z = [9.0, 1.0, 2.0, -1.0, 0.0, 9.0, 9.0];
        let c = p.coefficients(
            Kind::ForcedEntry,
            Persona::DeliveryRider.index() as u8,
            &z,
            &pp,
        );
        assert_eq!(c.intercept, -3.5);
        assert_eq!(c.aggressiveness, 1.0);
        assert!((c.time_saved - 0.5f64.exp()).abs() < 1e-12);
        assert!((c.waiting_time - 0.5f64.exp()).abs() < 1e-12);
        assert!((c.risk - (-1.0f64).exp()).abs() < 1e-12);
        assert!((c.imitation_violators - (-0.5f64).exp()).abs() < 1e-12);
        assert_eq!((c.imitation_compliers, c.enforcement), (1.0, 1.0));
        let other = p.coefficients(Kind::ForcedEntry, 0, &z, &pp);
        assert_eq!(other.intercept, -4.0);
    }

    #[test]
    fn a_type_on_without_its_inputs_is_refused() {
        let mut p = params();
        assert!(p.check().is_ok() && !p.any_active());
        p.intercept[Kind::RedLight as usize] = Some(-3.0);
        assert!(p.check().unwrap_err().contains("not built"));
        p.intercept[Kind::RedLight as usize] = None;
        p.intercept[Kind::ForcedEntry as usize] = Some(-3.0);
        p.coefficient[6] = None;
        assert!(p.check().unwrap_err().contains("risk"));
        p.coefficient[6] = Some(0.0);
        assert!(p.check().is_ok() && p.active(Kind::ForcedEntry));
        p.enabled = false;
        assert!(!p.active(Kind::ForcedEntry));
    }
}
