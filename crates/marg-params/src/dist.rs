//! Probability distributions for parameters.

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
