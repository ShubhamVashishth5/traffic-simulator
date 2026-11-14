//! The violation decision rule (G§4.1, plan/03-model-design.md §7, ADR-0018).
//!
//! One linear predictor serves every violation type `v`:
//!
//! ```text
//! η = β0 + βκ·κ + βt·time_saved + βw·waiting_time
//!   + βs⁺·ln(1 + N_violating) − βs⁻·ln(1 + N_complying_waiting)
//!   − βe·perceived_enforcement − βr·perceived_risk
//! ```
//!
//! A probability evaluated at every decision tick would pile up with the
//! resolution of the simulation, so a trigger is one of two kinds:
//!
//! - **one-shot** (arriving at amber, approaching a split): `P = σ(η)`,
//!   decided once per episode against one uniform;
//! - **continuous** (waiting at red, waiting at a junction): a hazard
//!   `λ = λ_max · σ(η)` per second. An episode draws one threshold
//!   `E ~ Exp(1)` when it starts and acts when the integrated hazard
//!   `Λ = ∫ λ dt` reaches it ([`HazardClock`]). `Λ` is integrated by the
//!   trapezoid rule between evaluations, so `P(acted by t) = 1 − exp(−Λ(t))`
//!   does not depend on how often the features are read (exact for a hazard
//!   linear in time), and under common random numbers the same agent acts at
//!   nearly the same time whatever the decision interval.
//!
//! Every term is kept apart so the event that records a decision can log
//! each one (CLAUDE.md rule 9); they add up to the logged `η`.

/// Number of terms in the linear predictor.
pub const N_TERMS: usize = 8;

/// Names of the terms, in the order they are summed and logged.
pub const TERM_NAMES: [&str; N_TERMS] = [
    "intercept",
    "aggressiveness",
    "time_saved",
    "waiting_time",
    "imitation_violators",
    "imitation_compliers",
    "enforcement",
    "risk",
];

/// Coefficients of one violation type for one agent. Each is the register's
/// value (plan/03 §7); signs follow the predictor above, so a positive
/// `enforcement` coefficient lowers the probability.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coefficients {
    /// `β0`.
    pub intercept: f64,
    /// `βκ`.
    pub aggressiveness: f64,
    /// `βt`, per second saved.
    pub time_saved: f64,
    /// `βw`, per second waited.
    pub waiting_time: f64,
    /// `βs⁺`, on `ln(1 + violators moving in view)`.
    pub imitation_violators: f64,
    /// `βs⁻`, on `ln(1 + compliers waiting in view)`.
    pub imitation_compliers: f64,
    /// `βe`, on perceived enforcement.
    pub enforcement: f64,
    /// `βr`, on perceived risk.
    pub risk: f64,
}

/// What an agent perceives when it decides.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Features {
    /// Aggressiveness κ.
    pub kappa: f64,
    /// Time the violation would save (s).
    pub time_saved: f64,
    /// Time spent in the trigger so far (s).
    pub waiting_time: f64,
    /// Violators moving inside the field of view.
    pub n_violating: u32,
    /// Compliers waiting inside the field of view.
    pub n_complying: u32,
    /// Perceived enforcement (P9's detection field; 0 until then).
    pub enforcement: f64,
    /// Perceived risk of acting now.
    pub risk: f64,
}

/// The terms of the linear predictor, in [`TERM_NAMES`] order.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Terms(pub [f64; N_TERMS]);

impl Terms {
    /// The terms for an agent with coefficients `c` that perceives `f`.
    pub fn new(c: &Coefficients, f: &Features) -> Self {
        Self([
            c.intercept,
            c.aggressiveness * f.kappa,
            c.time_saved * f.time_saved,
            c.waiting_time * f.waiting_time,
            c.imitation_violators * libm::log1p(f64::from(f.n_violating)),
            -c.imitation_compliers * libm::log1p(f64::from(f.n_complying)),
            -c.enforcement * f.enforcement,
            -c.risk * f.risk,
        ])
    }

    /// `η`: the terms summed in a fixed order.
    pub fn eta(&self) -> f64 {
        self.0.iter().sum()
    }

    /// `σ(η)`.
    pub fn probability(&self) -> f64 {
        sigmoid(self.eta())
    }

    /// `(name, value)` pairs for logging.
    pub fn named(&self) -> impl Iterator<Item = (&'static str, f64)> + '_ {
        TERM_NAMES.iter().copied().zip(self.0.iter().copied())
    }
}

/// The logistic function, without overflow for large `|x|`.
pub fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + libm::exp(-x))
    } else {
        let e = libm::exp(x);
        e / (1.0 + e)
    }
}

/// Hazard of a continuous trigger (per second): `λ_max · σ(η)`.
pub fn hazard(max_rate: f64, eta: f64) -> f64 {
    max_rate * sigmoid(eta)
}

/// A one-shot decision: act with probability `p`, against a uniform `u` in
/// `[0, 1)` drawn once for the episode.
pub fn one_shot(p: f64, u: f64) -> bool {
    u < p
}

/// The integrated-hazard clock of one continuous-trigger episode.
///
/// The episode draws `E = −ln(1 − u)` once. Each evaluation adds the
/// trapezoid `½(λ_prev + λ)·(t − t_prev)` to `Λ`, and the agent acts at the
/// first evaluation with `Λ ≥ E`. Because `P(E ≤ Λ) = 1 − exp(−Λ)`, the
/// probability of having acted by `t` depends on the hazard's path, not on
/// the times it was read at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HazardClock {
    threshold: f64,
    integrated: f64,
    t_last: f64,
    rate_last: f64,
}

impl HazardClock {
    /// Start an episode at `t` with hazard `rate`; `u` is a uniform in
    /// `[0, 1)` from the agent's decision stream, one per episode.
    pub fn start(t: f64, rate: f64, u: f64) -> Self {
        Self {
            threshold: -libm::log1p(-u),
            integrated: 0.0,
            t_last: t,
            rate_last: rate,
        }
    }

    /// Read the hazard again at `t`: integrate it since the last reading and
    /// say whether the agent acts now.
    pub fn advance(&mut self, t: f64, rate: f64) -> bool {
        self.integrated += 0.5 * (self.rate_last + rate) * (t - self.t_last);
        self.t_last = t;
        self.rate_last = rate;
        self.fired()
    }

    /// Whether the integrated hazard has reached the threshold.
    pub fn fired(&self) -> bool {
        self.integrated >= self.threshold
    }

    /// `Λ`: the hazard integrated since the episode started.
    pub fn integrated(&self) -> f64 {
        self.integrated
    }

    /// `E`: the integrated hazard at which the agent acts.
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// `1 − exp(−Λ)`: the probability, before the draw, of having acted by
    /// the last reading.
    pub fn probability(&self) -> f64 {
        -libm::expm1(-self.integrated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::{StreamKind, StreamRng};

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn terms_add_up_to_the_logged_probability() {
        let c = Coefficients {
            intercept: -3.0,
            aggressiveness: 0.8,
            time_saved: 0.05,
            waiting_time: 0.02,
            imitation_violators: 1.2,
            imitation_compliers: 0.6,
            enforcement: 2.0,
            risk: 1.5,
        };
        let f = Features {
            kappa: 1.25,
            time_saved: 30.0,
            waiting_time: 12.0,
            n_violating: 3,
            n_complying: 7,
            enforcement: 0.1,
            risk: 0.4,
        };
        let t = Terms::new(&c, &f);
        let expected = [
            -3.0,
            0.8 * 1.25,
            0.05 * 30.0,
            0.02 * 12.0,
            1.2 * 4f64.ln(),
            -0.6 * 8f64.ln(),
            -2.0 * 0.1,
            -1.5 * 0.4,
        ];
        for (k, (got, want)) in t.0.iter().zip(expected).enumerate() {
            assert!(
                close(*got, want, 1e-12),
                "{}: {got} vs {want}",
                TERM_NAMES[k]
            );
        }
        let eta: f64 = expected.iter().sum();
        assert!(close(t.eta(), eta, 1e-12));
        assert!(close(t.probability(), 1.0 / (1.0 + (-eta).exp()), 1e-12));
        assert_eq!(t.named().count(), N_TERMS);
        // Nobody in view: both imitation terms vanish.
        let alone = Terms::new(
            &c,
            &Features {
                n_violating: 0,
                n_complying: 0,
                ..f
            },
        );
        assert_eq!((alone.0[4], alone.0[5]), (0.0, 0.0));
    }

    #[test]
    fn sigmoid_is_stable_and_symmetric() {
        for x in [-800.0, -40.0, -3.0, -1e-9, 0.0, 1e-9, 3.0, 40.0, 800.0] {
            let s = sigmoid(x);
            assert!(s.is_finite() && (0.0..=1.0).contains(&s));
            assert!(close(s + sigmoid(-x), 1.0, 1e-15));
        }
        assert_eq!(sigmoid(0.0), 0.5);
    }

    #[test]
    fn a_constant_hazard_integrates_exactly_at_any_interval() {
        let rate = 0.037;
        for dt in [0.1, 0.25, 0.5, 1.0, 2.0] {
            let mut c = HazardClock::start(3.0, rate, 0.5);
            let n = (60.0 / dt) as usize;
            for k in 1..=n {
                c.advance(3.0 + k as f64 * dt, rate);
            }
            assert!(close(c.integrated(), rate * 60.0, 1e-9), "dt {dt}");
            assert!(close(c.probability(), 1.0 - (-rate * 60.0f64).exp(), 1e-9));
        }
    }

    /// The P8.1 check. Agents wait 60 s at a red light. The hazard rises
    /// with waiting time and falls with the red still to come:
    /// `η(t) = β0 + βw·t + βt·(60 − t)`, `λ = λ_max σ(η)`, about 1 in 15
    /// running it. The share that has run by the end of red is the same at
    /// every decision interval (within Monte-Carlo error, and the integrated
    /// hazard within 0.1%), and with the same random numbers the same agents
    /// run at nearly the same time. Evaluating `σ(η)` as a probability at
    /// every tick instead would give a share that moves with the interval.
    #[test]
    fn cumulative_probability_is_invariant_to_the_decision_interval() {
        const N: u64 = 20_000;
        const RED: f64 = 60.0;
        let c = Coefficients {
            intercept: -9.0,
            time_saved: 0.02,
            waiting_time: 0.05,
            ..Coefficients::default()
        };
        let max_rate = 1.0;
        let rate = |t: f64| {
            let f = Features {
                waiting_time: t,
                time_saved: RED - t,
                ..Features::default()
            };
            hazard(max_rate, Terms::new(&c, &f).eta())
        };
        // Reference: the integral by a fine midpoint rule.
        let fine = 1e-3;
        let lambda_ref: f64 = (0..(RED / fine) as usize)
            .map(|k| rate((k as f64 + 0.5) * fine) * fine)
            .sum();
        let p_ref = 1.0 - (-lambda_ref).exp();
        let se = (p_ref * (1.0 - p_ref) / N as f64).sqrt();

        // One uniform per agent and episode, as the core will draw it.
        let u: Vec<f64> = (0..N)
            .map(|id| StreamRng::at(17, StreamKind::Decisions, id, 0).uniform())
            .collect();
        // Act times by agent (None: waited out the red) at one interval.
        let act_times = |dt: f64| -> Vec<Option<f64>> {
            let n = (RED / dt).round() as usize;
            u.iter()
                .map(|&u| {
                    let mut clock = HazardClock::start(0.0, rate(0.0), u);
                    (1..=n)
                        .map(|k| k as f64 * dt)
                        .find(|&t| clock.advance(t, rate(t)))
                })
                .collect()
        };
        let intervals = [0.1, 0.25, 0.5, 1.0, 2.0];
        let base = act_times(intervals[0]);
        let mut naive = Vec::new();
        for dt in intervals {
            let times = act_times(dt);
            let share = times.iter().filter(|t| t.is_some()).count() as f64 / N as f64;
            assert!(
                close(share, p_ref, 4.0 * se),
                "dt {dt}: share {share:.4} vs {p_ref:.4} ± {se:.4}"
            );
            // The integrated hazard itself, without Monte-Carlo error.
            let mut clock = HazardClock::start(0.0, rate(0.0), 0.0);
            let n = (RED / dt).round() as usize;
            for k in 1..=n {
                clock.advance(k as f64 * dt, rate(k as f64 * dt));
            }
            assert!(
                close(clock.integrated(), lambda_ref, 1e-3 * lambda_ref),
                "dt {dt}: Λ {} vs {lambda_ref}",
                clock.integrated()
            );
            // Common random numbers: the same agents run, at nearly the
            // same time.
            let mut same = 0;
            for (a, b) in base.iter().zip(&times) {
                match (a, b) {
                    (Some(a), Some(b)) => {
                        assert!((a - b).abs() <= dt + 1e-9, "dt {dt}: {a} vs {b}");
                        same += 1;
                    }
                    (None, None) => same += 1,
                    _ => {}
                }
            }
            assert!(same as f64 / N as f64 > 0.999, "dt {dt}: {same} agree");
            // The naive rule: σ(η) as a probability at every tick.
            let p_naive = 1.0
                - (1..=n)
                    .map(|k| 1.0 - rate(k as f64 * dt) / max_rate)
                    .product::<f64>();
            naive.push(p_naive);
        }
        assert!(
            naive[0] > 5.0 * naive[intervals.len() - 1],
            "the naive rule should move with the interval: {naive:?}"
        );
    }

    #[test]
    fn a_one_shot_is_decided_once_whatever_the_interval() {
        // Amber starts between ticks; each agent decides at the first tick
        // of its episode and never again.
        const N: u64 = 20_000;
        let p = 0.2;
        for dt in [0.1, 0.5, 2.0] {
            let mut go = 0;
            for id in 0..N {
                let mut decided: Option<bool> = None;
                let onset = 3.03;
                for k in 0..200 {
                    let t = k as f64 * dt;
                    if t >= onset && decided.is_none() {
                        let u = StreamRng::at(5, StreamKind::Decisions, id, 0).uniform();
                        decided = Some(one_shot(p, u));
                    }
                }
                if decided == Some(true) {
                    go += 1;
                }
            }
            let share = go as f64 / N as f64;
            let se = (p * (1.0 - p) / N as f64).sqrt();
            assert!(close(share, p, 4.0 * se), "dt {dt}: {share}");
        }
    }
}
