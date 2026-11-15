//! Probability distributions for parameters (G§: "draw every parameter from a
//! distribution, not a constant").
//!
//! Sampling uses only `libm` transcendental functions, so draws are bitwise
//! identical on every platform given the same random stream.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Source of uniform and normal variates. Implemented by the simulator's
/// counter-based streams (`marg_core::rng::StreamRng`); defined here so this
/// crate has no dependency on the core.
pub trait Rng64 {
    /// Uniform in `[0, 1)`.
    fn uniform(&mut self) -> f64;
    /// Uniform in `(0, 1)`.
    fn uniform_open(&mut self) -> f64;
    /// Standard normal.
    fn standard_normal(&mut self) -> f64;
}

/// Maximum rejection-sampling attempts for a truncated normal before falling
/// back to clamping (keeps sampling bounded and deterministic).
const TRUNC_MAX_TRIES: usize = 64;

/// A univariate distribution.
///
/// Several lognormal forms are accepted for readability in the register; they
/// are converted to `(mu, sigma)` of `ln X` by [`Dist::canonical`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "dist", rename_all = "snake_case", deny_unknown_fields)]
pub enum Dist {
    /// Always the same value.
    Constant { value: f64 },
    /// Normal with mean and standard deviation.
    Normal { mean: f64, sd: f64 },
    /// Normal restricted to `[min, max]` (rejection sampling).
    TruncNormal {
        mean: f64,
        sd: f64,
        min: f64,
        max: f64,
    },
    /// Lognormal by the mean and SD of `ln X`.
    LogNormal { mu: f64, sigma: f64 },
    /// Lognormal by its median and the SD of `ln X`.
    LogNormalMedian { median: f64, sigma: f64 },
    /// Lognormal by the mean and SD of `X` itself.
    LogNormalMoments { mean: f64, sd: f64 },
    /// Lognormal through two quantiles: `P(X ≤ x_lo) = p_lo`, `P(X ≤ x_hi) = p_hi`.
    LogNormalQuantiles {
        p_lo: f64,
        x_lo: f64,
        p_hi: f64,
        x_hi: f64,
    },
    /// Gamma with shape `k` and scale `θ` (mean `kθ`).
    Gamma { shape: f64, scale: f64 },
    /// Exponential with the given mean.
    Exponential { mean: f64 },
    /// `shift + Exponential`, parameterised by the shift and the overall mean.
    ShiftedExponential { shift: f64, mean: f64 },
    /// Uniform on `[min, max]`.
    Uniform { min: f64, max: f64 },
    /// Piecewise-linear inverse CDF through `[p, x]` points, `p` rising from 0 to 1.
    Empirical { points: Vec<[f64; 2]> },
    /// Finite mixture; weights need not sum to 1 (they are normalised).
    Mixture { components: Vec<MixtureComponent> },
}

/// One weighted component of a [`Dist::Mixture`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MixtureComponent {
    pub weight: f64,
    pub dist: Box<Dist>,
}

/// An invalid distribution definition.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("invalid distribution: {0}")]
pub struct DistError(pub String);

fn bad(msg: impl Into<String>) -> Result<(), DistError> {
    Err(DistError(msg.into()))
}

impl Dist {
    /// Check the parameters are well formed.
    pub fn validate(&self) -> Result<(), DistError> {
        let finite = |xs: &[f64]| xs.iter().all(|x| x.is_finite());
        match self {
            Dist::Constant { value } if !value.is_finite() => bad("constant must be finite"),
            Dist::Normal { mean, sd } if !finite(&[*mean, *sd]) || *sd < 0.0 => {
                bad("normal needs finite mean and sd >= 0")
            }
            Dist::TruncNormal { mean, sd, min, max }
                if !finite(&[*mean, *sd, *min, *max]) || *sd < 0.0 || min >= max =>
            {
                bad("trunc_normal needs sd >= 0 and min < max")
            }
            Dist::LogNormal { mu, sigma } if !finite(&[*mu, *sigma]) || *sigma < 0.0 => {
                bad("log_normal needs sigma >= 0")
            }
            Dist::LogNormalMedian { median, sigma }
                if !finite(&[*median, *sigma]) || *median <= 0.0 || *sigma < 0.0 =>
            {
                bad("log_normal_median needs median > 0 and sigma >= 0")
            }
            Dist::LogNormalMoments { mean, sd }
                if !finite(&[*mean, *sd]) || *mean <= 0.0 || *sd < 0.0 =>
            {
                bad("log_normal_moments needs mean > 0 and sd >= 0")
            }
            Dist::LogNormalQuantiles {
                p_lo,
                x_lo,
                p_hi,
                x_hi,
            } if !(0.0 < *p_lo && p_lo < p_hi && *p_hi < 1.0 && 0.0 < *x_lo && x_lo < x_hi) => {
                bad("log_normal_quantiles needs 0 < p_lo < p_hi < 1 and 0 < x_lo < x_hi")
            }
            Dist::Gamma { shape, scale }
                if !finite(&[*shape, *scale]) || *shape <= 0.0 || *scale <= 0.0 =>
            {
                bad("gamma needs shape > 0 and scale > 0")
            }
            Dist::Exponential { mean } if !mean.is_finite() || *mean <= 0.0 => {
                bad("exponential needs mean > 0")
            }
            Dist::ShiftedExponential { shift, mean }
                if !finite(&[*shift, *mean]) || *shift < 0.0 || mean <= shift =>
            {
                bad("shifted_exponential needs 0 <= shift < mean")
            }
            Dist::Uniform { min, max } if !finite(&[*min, *max]) || min > max => {
                bad("uniform needs min <= max")
            }
            Dist::Empirical { points } => {
                if points.len() < 2 {
                    return bad("empirical needs at least two points");
                }
                if points[0][0] != 0.0 || points[points.len() - 1][0] != 1.0 {
                    return bad("empirical p must start at 0 and end at 1");
                }
                for w in points.windows(2) {
                    if !(w[1][0] > w[0][0] && w[1][1] >= w[0][1]) {
                        return bad("empirical points must rise in p and not fall in x");
                    }
                }
                Ok(())
            }
            Dist::Mixture { components } => {
                if components.is_empty() {
                    return bad("mixture needs at least one component");
                }
                for c in components {
                    if !(c.weight.is_finite() && c.weight > 0.0) {
                        return bad("mixture weights must be > 0");
                    }
                    c.dist.validate()?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The same distribution with every lognormal form converted to
    /// [`Dist::LogNormal`] (recursively inside mixtures).
    pub fn canonical(&self) -> Dist {
        match self {
            Dist::LogNormalMedian { median, sigma } => Dist::LogNormal {
                mu: libm::log(*median),
                sigma: *sigma,
            },
            Dist::LogNormalMoments { mean, sd } => {
                let s2 = libm::log1p((sd / mean) * (sd / mean));
                Dist::LogNormal {
                    mu: libm::log(*mean) - 0.5 * s2,
                    sigma: libm::sqrt(s2),
                }
            }
            Dist::LogNormalQuantiles {
                p_lo,
                x_lo,
                p_hi,
                x_hi,
            } => {
                let (z_lo, z_hi) = (norm_ppf(*p_lo), norm_ppf(*p_hi));
                let (l_lo, l_hi) = (libm::log(*x_lo), libm::log(*x_hi));
                let sigma = (l_hi - l_lo) / (z_hi - z_lo);
                Dist::LogNormal {
                    mu: l_lo - sigma * z_lo,
                    sigma,
                }
            }
            Dist::Mixture { components } => Dist::Mixture {
                components: components
                    .iter()
                    .map(|c| MixtureComponent {
                        weight: c.weight,
                        dist: Box::new(c.dist.canonical()),
                    })
                    .collect(),
            },
            other => other.clone(),
        }
    }

    /// The distribution of `factor · X` (used for unit conversion; `factor > 0`).
    pub fn scaled(&self, factor: f64) -> Dist {
        let f = factor;
        match self.canonical() {
            Dist::Constant { value } => Dist::Constant { value: value * f },
            Dist::Normal { mean, sd } => Dist::Normal {
                mean: mean * f,
                sd: sd * f,
            },
            Dist::TruncNormal { mean, sd, min, max } => Dist::TruncNormal {
                mean: mean * f,
                sd: sd * f,
                min: min * f,
                max: max * f,
            },
            Dist::LogNormal { mu, sigma } => Dist::LogNormal {
                mu: mu + libm::log(f),
                sigma,
            },
            Dist::Gamma { shape, scale } => Dist::Gamma {
                shape,
                scale: scale * f,
            },
            Dist::Exponential { mean } => Dist::Exponential { mean: mean * f },
            Dist::ShiftedExponential { shift, mean } => Dist::ShiftedExponential {
                shift: shift * f,
                mean: mean * f,
            },
            Dist::Uniform { min, max } => Dist::Uniform {
                min: min * f,
                max: max * f,
            },
            Dist::Empirical { points } => Dist::Empirical {
                points: points.iter().map(|[p, x]| [*p, x * f]).collect(),
            },
            Dist::Mixture { components } => Dist::Mixture {
                components: components
                    .into_iter()
                    .map(|c| MixtureComponent {
                        weight: c.weight,
                        dist: Box::new(c.dist.scaled(f)),
                    })
                    .collect(),
            },
            Dist::LogNormalMedian { .. }
            | Dist::LogNormalMoments { .. }
            | Dist::LogNormalQuantiles { .. } => unreachable!("canonical() removes these"),
        }
    }

    /// Draw one value. Call on a [`Dist::canonical`] distribution in hot paths
    /// to avoid re-deriving lognormal parameters on every draw.
    pub fn sample<R: Rng64 + ?Sized>(&self, rng: &mut R) -> f64 {
        match self {
            Dist::Constant { value } => *value,
            Dist::Normal { mean, sd } => mean + sd * rng.standard_normal(),
            Dist::TruncNormal { mean, sd, min, max } => {
                for _ in 0..TRUNC_MAX_TRIES {
                    let x = mean + sd * rng.standard_normal();
                    if (*min..=*max).contains(&x) {
                        return x;
                    }
                }
                mean.clamp(*min, *max)
            }
            Dist::LogNormal { mu, sigma } => libm::exp(mu + sigma * rng.standard_normal()),
            Dist::LogNormalMedian { .. }
            | Dist::LogNormalMoments { .. }
            | Dist::LogNormalQuantiles { .. } => self.canonical().sample(rng),
            Dist::Gamma { shape, scale } => scale * sample_gamma(*shape, rng),
            Dist::Exponential { mean } => -mean * libm::log(rng.uniform_open()),
            Dist::ShiftedExponential { shift, mean } => {
                shift - (mean - shift) * libm::log(rng.uniform_open())
            }
            Dist::Uniform { min, max } => min + (max - min) * rng.uniform(),
            Dist::Empirical { points } => empirical_ppf(points, rng.uniform()),
            Dist::Mixture { components } => {
                let total: f64 = components.iter().map(|c| c.weight).sum();
                let mut u = rng.uniform() * total;
                for c in components {
                    if u < c.weight {
                        return c.dist.sample(rng);
                    }
                    u -= c.weight;
                }
                components[components.len() - 1].dist.sample(rng)
            }
        }
    }

    /// The mean, where it exists in closed form.
    pub fn mean(&self) -> f64 {
        match self.canonical() {
            Dist::Constant { value } => value,
            Dist::Normal { mean, .. } => mean,
            Dist::TruncNormal { mean, sd, min, max } => {
                if sd == 0.0 {
                    return mean.clamp(min, max);
                }
                let (a, b) = ((min - mean) / sd, (max - mean) / sd);
                let z = norm_cdf(b) - norm_cdf(a);
                mean + sd * (norm_pdf(a) - norm_pdf(b)) / z
            }
            Dist::LogNormal { mu, sigma } => libm::exp(mu + 0.5 * sigma * sigma),
            Dist::Gamma { shape, scale } => shape * scale,
            Dist::Exponential { mean } | Dist::ShiftedExponential { mean, .. } => mean,
            Dist::Uniform { min, max } => 0.5 * (min + max),
            Dist::Empirical { points } => points
                .windows(2)
                .map(|w| (w[1][0] - w[0][0]) * 0.5 * (w[0][1] + w[1][1]))
                .sum(),
            Dist::Mixture { components } => {
                let total: f64 = components.iter().map(|c| c.weight).sum();
                components
                    .iter()
                    .map(|c| c.weight * c.dist.mean())
                    .sum::<f64>()
                    / total
            }
            Dist::LogNormalMedian { .. }
            | Dist::LogNormalMoments { .. }
            | Dist::LogNormalQuantiles { .. } => unreachable!("canonical() removes these"),
        }
    }

    /// The `p`-quantile, for the forms where it has a closed form.
    pub fn quantile(&self, p: f64) -> Option<f64> {
        match self.canonical() {
            Dist::Constant { value } => Some(value),
            Dist::Normal { mean, sd } => Some(mean + sd * norm_ppf(p)),
            Dist::TruncNormal { mean, sd, min, max } => {
                Some(trunc_normal_ppf(mean, sd, min, max, p))
            }
            Dist::LogNormal { mu, sigma } => Some(libm::exp(mu + sigma * norm_ppf(p))),
            Dist::Exponential { mean } => Some(-mean * libm::log1p(-p)),
            Dist::ShiftedExponential { shift, mean } => {
                Some(shift - (mean - shift) * libm::log1p(-p))
            }
            Dist::Uniform { min, max } => Some(min + (max - min) * p),
            Dist::Empirical { points } => Some(empirical_ppf(&points, p)),
            _ => None,
        }
    }
}

impl Dist {
    /// `F⁻¹(Φ(w))`: the value at normal score `w`, so a standard normal `w`
    /// gives a draw from this distribution (a Gaussian copula's marginal,
    /// ADR-0018 part 5). Exact without the round trip through `Φ` for normal
    /// and lognormal forms; `None` where there is no closed-form quantile.
    pub fn from_normal_score(&self, w: f64) -> Option<f64> {
        match self.canonical() {
            Dist::Normal { mean, sd } => Some(mean + sd * w),
            Dist::LogNormal { mu, sigma } => Some(libm::exp(mu + sigma * w)),
            Dist::TruncNormal { mean, sd, min, max } => {
                if sd == 0.0 {
                    return Some(mean.clamp(min, max));
                }
                let (lo, hi) = (norm_cdf((min - mean) / sd), norm_cdf((max - mean) / sd));
                Some(trunc_normal_ppf_bounds(
                    mean,
                    sd,
                    min,
                    max,
                    lo,
                    hi,
                    norm_cdf(w),
                ))
            }
            d => {
                let u = norm_cdf(w);
                if u <= 0.0 || u >= 1.0 {
                    let edge = if u <= 0.0 {
                        f64::MIN_POSITIVE
                    } else {
                        1.0 - f64::EPSILON
                    };
                    d.quantile(edge)
                } else {
                    d.quantile(u)
                }
            }
        }
    }
}

/// Quantile of a normal truncated to `[min, max]`.
fn trunc_normal_ppf(mean: f64, sd: f64, min: f64, max: f64, p: f64) -> f64 {
    if sd == 0.0 {
        return mean.clamp(min, max);
    }
    let (lo, hi) = (norm_cdf((min - mean) / sd), norm_cdf((max - mean) / sd));
    trunc_normal_ppf_bounds(mean, sd, min, max, lo, hi, p)
}

fn trunc_normal_ppf_bounds(
    mean: f64,
    sd: f64,
    min: f64,
    max: f64,
    lo: f64,
    hi: f64,
    p: f64,
) -> f64 {
    let q = lo + p * (hi - lo);
    if q <= 0.0 {
        min
    } else if q >= 1.0 {
        max
    } else {
        (mean + sd * norm_ppf(q)).clamp(min, max)
    }
}

/// Marsaglia & Tsang (2000) gamma sampler with unit scale.
fn sample_gamma<R: Rng64 + ?Sized>(shape: f64, rng: &mut R) -> f64 {
    if shape < 1.0 {
        // Boost: Gamma(k) = Gamma(k + 1) · U^(1/k).
        let g = sample_gamma(shape + 1.0, rng);
        return g * libm::pow(rng.uniform_open(), 1.0 / shape);
    }
    let d = shape - 1.0 / 3.0;
    let c = 1.0 / libm::sqrt(9.0 * d);
    loop {
        let x = rng.standard_normal();
        let v = 1.0 + c * x;
        if v <= 0.0 {
            continue;
        }
        let v3 = v * v * v;
        let u = rng.uniform_open();
        if libm::log(u) < 0.5 * x * x + d - d * v3 + d * libm::log(v3) {
            return d * v3;
        }
    }
}

fn empirical_ppf(points: &[[f64; 2]], u: f64) -> f64 {
    let i = points
        .partition_point(|pt| pt[0] <= u)
        .clamp(1, points.len() - 1);
    let ([p0, x0], [p1, x1]) = (points[i - 1], points[i]);
    x0 + (x1 - x0) * (u - p0) / (p1 - p0)
}

/// Standard normal density.
pub fn norm_pdf(x: f64) -> f64 {
    libm::exp(-0.5 * x * x) / libm::sqrt(std::f64::consts::TAU)
}

/// Standard normal CDF.
pub fn norm_cdf(x: f64) -> f64 {
    0.5 * libm::erfc(-x / std::f64::consts::SQRT_2)
}

/// Standard normal quantile: Acklam's rational approximation refined by one
/// Halley step (accurate to about 1e-15).
pub fn norm_ppf(p: f64) -> f64 {
    assert!(p > 0.0 && p < 1.0, "norm_ppf needs 0 < p < 1, got {p}");
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    const P_LOW: f64 = 0.02425;
    let x = if p < P_LOW {
        let q = libm::sqrt(-2.0 * libm::log(p));
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = libm::sqrt(-2.0 * libm::log1p(-p));
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    // One Halley refinement step.
    let e = norm_cdf(x) - p;
    let u = e * libm::sqrt(std::f64::consts::TAU) * libm::exp(0.5 * x * x);
    x - u / (1.0 + 0.5 * x * u)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny deterministic generator for tests (SplitMix64), so this crate's
    /// tests don't depend on the core's Philox streams.
    struct TestRng(u64);
    impl TestRng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
    }
    impl Rng64 for TestRng {
        fn uniform(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1u64 << 53) as f64
        }
        fn uniform_open(&mut self) -> f64 {
            ((self.next() >> 12) as f64 + 0.5) / (1u64 << 52) as f64
        }
        fn standard_normal(&mut self) -> f64 {
            let (u1, u2) = (self.uniform_open(), self.uniform());
            libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(std::f64::consts::TAU * u2)
        }
    }

    fn moments(d: &Dist, n: usize) -> (f64, f64) {
        let d = d.canonical();
        let mut r = TestRng(12345);
        let (mut s, mut s2) = (0.0, 0.0);
        for _ in 0..n {
            let x = d.sample(&mut r);
            s += x;
            s2 += x * x;
        }
        let m = s / n as f64;
        (m, (s2 / n as f64 - m * m).sqrt())
    }

    #[test]
    fn ppf_matches_known_values() {
        assert!((norm_ppf(0.5)).abs() < 1e-12);
        assert!((norm_ppf(0.975) - 1.959_963_984_540_054).abs() < 1e-9);
        assert!((norm_ppf(0.15) + 1.036_433_389_493_79).abs() < 1e-9);
        assert!((norm_ppf(1e-6) + 4.753_424_308_822_899).abs() < 1e-8);
        for p in [0.001, 0.02, 0.3, 0.7, 0.99, 0.9999] {
            assert!((norm_cdf(norm_ppf(p)) - p).abs() < 1e-12);
        }
    }

    #[test]
    fn sample_means_match_analytic() {
        let cases = [
            Dist::Normal {
                mean: 49.19,
                sd: 12.43,
            },
            Dist::TruncNormal {
                mean: 21.0,
                sd: 4.0,
                min: 5.0,
                max: 25.0,
            },
            Dist::LogNormalMedian {
                median: 0.383,
                sigma: 0.3,
            },
            Dist::LogNormalMoments {
                mean: 30.0,
                sd: 12.0,
            },
            Dist::Gamma {
                shape: 2.5,
                scale: 1.5,
            },
            Dist::Gamma {
                shape: 0.6,
                scale: 2.0,
            },
            Dist::Exponential { mean: 3.0 },
            Dist::ShiftedExponential {
                shift: 0.5,
                mean: 2.0,
            },
            Dist::Uniform {
                min: 4.35,
                max: 5.78,
            },
            Dist::Empirical {
                points: vec![[0.0, 0.0], [0.5, 1.0], [1.0, 4.0]],
            },
            Dist::Mixture {
                components: vec![
                    MixtureComponent {
                        weight: 0.9,
                        dist: Box::new(Dist::Uniform {
                            min: 300.0,
                            max: 1000.0,
                        }),
                    },
                    MixtureComponent {
                        weight: 0.1,
                        dist: Box::new(Dist::Uniform {
                            min: 0.0,
                            max: 100.0,
                        }),
                    },
                ],
            },
        ];
        for d in cases {
            d.validate().unwrap();
            let (m, sd) = moments(&d, 200_000);
            let target = d.mean();
            let tol = 0.01 * target.abs().max(sd) + 0.01;
            assert!(
                (m - target).abs() < tol,
                "{d:?}: sample mean {m}, analytic {target}"
            );
        }
    }

    #[test]
    fn lognormal_moments_round_trip() {
        let d = Dist::LogNormalMoments {
            mean: 30.0,
            sd: 12.0,
        };
        let (m, sd) = moments(&d, 400_000);
        assert!((m - 30.0).abs() < 0.2, "mean {m}");
        assert!((sd - 12.0).abs() < 0.2, "sd {sd}");
    }

    /// Wong & Goh reaction times: p15 = 0.68 s, mean 1.02 s, p85 = 1.28 s.
    /// A two-parameter lognormal through the two quantiles reproduces them
    /// exactly; its mean is 0.977 s, 0.043 s below the reported mean. The gap
    /// is recorded in the register notes (see plan P1.2).
    #[test]
    fn wong_goh_reaction_time_quantiles() {
        let d = Dist::LogNormalQuantiles {
            p_lo: 0.15,
            x_lo: 0.68,
            p_hi: 0.85,
            x_hi: 1.28,
        };
        assert!((d.quantile(0.15).unwrap() - 0.68).abs() < 1e-9);
        assert!((d.quantile(0.85).unwrap() - 1.28).abs() < 1e-9);
        let mean = d.mean();
        assert!((mean - 0.977).abs() < 0.002, "mean {mean}");
        assert!((mean - 1.02).abs() < 0.05);
    }

    #[test]
    fn scaling_converts_units() {
        let kmh = Dist::Normal {
            mean: 36.0,
            sd: 7.2,
        };
        let ms = kmh.scaled(1.0 / 3.6);
        assert_eq!(
            ms,
            Dist::Normal {
                mean: 10.0,
                sd: 2.0
            }
        );
        let ln = Dist::LogNormalMedian {
            median: 2.0,
            sigma: 0.3,
        }
        .scaled(0.5);
        assert!((ln.quantile(0.5).unwrap() - 1.0).abs() < 1e-12);
        let tn = Dist::TruncNormal {
            mean: 1.0,
            sd: 1.0,
            min: 0.0,
            max: 2.0,
        }
        .scaled(2.0);
        assert!((tn.mean() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn rejects_bad_definitions() {
        assert!(
            Dist::Normal {
                mean: 1.0,
                sd: -1.0
            }
            .validate()
            .is_err()
        );
        assert!(Dist::Uniform { min: 2.0, max: 1.0 }.validate().is_err());
        assert!(
            Dist::ShiftedExponential {
                shift: 2.0,
                mean: 1.0
            }
            .validate()
            .is_err()
        );
        assert!(
            Dist::Empirical {
                points: vec![[0.0, 1.0], [0.9, 2.0]]
            }
            .validate()
            .is_err()
        );
        assert!(
            Dist::LogNormalQuantiles {
                p_lo: 0.9,
                x_lo: 1.0,
                p_hi: 0.1,
                x_hi: 2.0
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn yaml_forms_parse() {
        let d: Dist =
            serde_norway::from_str("{dist: log_normal_median, median: 0.383, sigma: 0.3}").unwrap();
        assert_eq!(
            d,
            Dist::LogNormalMedian {
                median: 0.383,
                sigma: 0.3
            }
        );
        let m: Dist = serde_norway::from_str(
            "dist: mixture\ncomponents:\n  - {weight: 1, dist: {dist: constant, value: 2}}\n",
        )
        .unwrap();
        assert_eq!(m.mean(), 2.0);
    }

    proptest::proptest! {
        #[test]
        fn trunc_normal_stays_in_bounds(mean in -10.0f64..10.0, sd in 0.0f64..20.0, lo in -5.0f64..0.0, width in 0.1f64..10.0) {
            let d = Dist::TruncNormal { mean, sd, min: lo, max: lo + width };
            let mut r = TestRng(7);
            for _ in 0..100 {
                let x = d.sample(&mut r);
                proptest::prop_assert!(x >= lo && x <= lo + width);
            }
        }
    }

    #[test]
    fn normal_scores_map_onto_each_marginal() {
        // A standard normal score gives this distribution: compare quantiles.
        let cases = [
            Dist::Normal {
                mean: 10.0,
                sd: 2.0,
            },
            Dist::TruncNormal {
                mean: 12.0,
                sd: 3.0,
                min: 8.0,
                max: 20.0,
            },
            Dist::LogNormalMedian {
                median: 1.5,
                sigma: 0.4,
            },
            Dist::Uniform {
                min: -1.0,
                max: 3.0,
            },
        ];
        for d in &cases {
            for p in [0.01, 0.1, 0.5, 0.9, 0.99] {
                let x = d.from_normal_score(norm_ppf(p)).unwrap();
                let q = d.quantile(p).unwrap();
                assert!(
                    (x - q).abs() < 1e-9 * q.abs().max(1.0),
                    "{d:?} p {p}: {x} vs {q}"
                );
            }
            // Far tails stay finite and inside the support.
            for w in [-40.0, 40.0] {
                assert!(d.from_normal_score(w).unwrap().is_finite());
            }
        }
        let t = Dist::TruncNormal {
            mean: 12.0,
            sd: 3.0,
            min: 8.0,
            max: 20.0,
        };
        assert!((t.from_normal_score(-40.0).unwrap() - 8.0).abs() < 1e-9);
        assert!((t.from_normal_score(40.0).unwrap() - 20.0).abs() < 1e-9);
        // The quantile matches rejection sampling.
        let mut rng = TestRng(7);
        let mut xs: Vec<f64> = (0..20_000).map(|_| t.sample(&mut rng)).collect();
        xs.sort_by(f64::total_cmp);
        for p in [0.1, 0.5, 0.9] {
            let emp = xs[(p * xs.len() as f64) as usize];
            assert!((emp - t.quantile(p).unwrap()).abs() < 0.1, "p {p}: {emp}");
        }
        assert!(
            Dist::Gamma {
                shape: 2.0,
                scale: 1.0
            }
            .from_normal_score(0.0)
            .is_none()
        );
    }
}
