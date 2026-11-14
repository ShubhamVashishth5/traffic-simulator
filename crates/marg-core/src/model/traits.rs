//! Trait processes (G§3.1, ADR-0018 parts 5–6): the copula that links κ to
//! drawn parameters, the within-trip Ornstein–Uhlenbeck process on κ, and
//! who uses a phone.

use marg_params::dist::norm_ppf;

/// The normal score an agent draws a κ-linked parameter at: its own score
/// `eps` mixed with κ's score `z` at correlation `r`. Both standard normal
/// and independent ⇒ the result is standard normal (Gaussian copula), so the
/// parameter keeps its register marginal.
pub fn linked_score(r: f64, z: f64, eps: f64) -> f64 {
    r * z + libm::sqrt((1.0 - r * r).max(0.0)) * eps
}

/// The exact transition of an Ornstein–Uhlenbeck process over a fixed
/// interval: `x' = μ + (x − μ)·e^{−θΔ} + σ_stat·√(1 − e^{−2θΔ})·N(0, 1)`.
/// Exact for any interval, so the process does not depend on how often it is
/// advanced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OuStep {
    pub decay: f64,
    pub noise: f64,
}

impl OuStep {
    /// Reversion rate `θ` (1/s), stationary SD, interval (s).
    pub fn new(rate: f64, stationary_sd: f64, interval: f64) -> Self {
        let decay = libm::exp(-rate * interval);
        Self {
            decay,
            noise: stationary_sd * libm::sqrt((1.0 - decay * decay).max(0.0)),
        }
    }

    /// Advance `x` about `mean` with the standard normal `n`.
    pub fn step(&self, x: f64, mean: f64, n: f64) -> f64 {
        mean + (x - mean) * self.decay + self.noise * n
    }
}

/// Whether an agent whose distraction trait has normal score `z` (relative to
/// the population) uses a phone, when `share` of the population does: the
/// top `share` of the distribution.
pub fn phone_user(z: f64, share: f64) -> bool {
    if share <= 0.0 {
        false
    } else if share >= 1.0 {
        true
    } else {
        z > norm_ppf(1.0 - share)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::{StreamKind, StreamRng};

    #[test]
    fn linked_scores_stay_standard_normal_with_the_asked_correlation() {
        let n = 50_000;
        let r = 0.7;
        let (mut sw, mut sww, mut swz) = (0.0, 0.0, 0.0);
        for id in 0..n {
            let z = StreamRng::at(3, StreamKind::AgentDraws, id, 0).standard_normal();
            let e = StreamRng::at(3, StreamKind::AgentDraws, id, 1).standard_normal();
            let w = linked_score(r, z, e);
            sw += w;
            sww += w * w;
            swz += w * z;
        }
        let nf = n as f64;
        assert!((sw / nf).abs() < 0.02);
        assert!((sww / nf - 1.0).abs() < 0.03);
        assert!((swz / nf - r).abs() < 0.02);
        assert_eq!(linked_score(0.0, 5.0, 0.25), 0.25);
    }

    /// P8.2: κ's autocorrelation matches the OU parameters. A long path at
    /// the decision interval has the stationary SD asked for and lag-k
    /// autocorrelation `exp(−θ k Δ)`, whatever the interval.
    #[test]
    fn kappa_autocorrelation_matches_the_ou_parameters() {
        let rate = 1.0 / 120.0; // reverts over two minutes
        let sd = 0.4;
        for interval in [0.5, 2.0] {
            let step = OuStep::new(rate, sd, interval);
            let n = 400_000;
            let mut x = 0.0;
            let mut path = Vec::with_capacity(n);
            for k in 0..n as u64 {
                let z = StreamRng::at(11, StreamKind::Decisions, 1, k).standard_normal();
                x = step.step(x, 0.0, z);
                path.push(x);
            }
            let var = path.iter().map(|x| x * x).sum::<f64>() / n as f64;
            assert!(
                (var.sqrt() - sd).abs() < 0.03 * sd,
                "dt {interval}: sd {}",
                var.sqrt()
            );
            for lag_s in [30.0, 120.0] {
                let k = (lag_s / interval) as usize;
                let c =
                    path.windows(k + 1).map(|w| w[0] * w[k]).sum::<f64>() / (n - k) as f64 / var;
                let want = (-rate * lag_s).exp();
                assert!(
                    (c - want).abs() < 0.05,
                    "dt {interval} lag {lag_s}: {c} vs {want}"
                );
            }
        }
    }

    #[test]
    fn phone_users_are_the_top_share_of_the_distraction_trait() {
        let n = 40_000;
        for share in [0.0, 0.2, 0.5, 1.0] {
            let users = (0..n)
                .filter(|&id| {
                    let z = StreamRng::at(4, StreamKind::AgentDraws, id, 0).standard_normal();
                    phone_user(z, share)
                })
                .count() as f64
                / n as f64;
            assert!((users - share).abs() < 0.01, "share {share}: {users}");
        }
        // A persona one SD more distracted has more phone users.
        assert!(phone_user(0.9, 0.2) && !phone_user(0.8, 0.2));
    }
}
