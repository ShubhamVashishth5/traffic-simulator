//! Simulation core of the marg lane-free mixed-traffic simulator.
//!
//! The step loop follows `plan/02-architecture.md` §4: demand → perceive →
//! decide → move → apply → observe → commands. Perceive, decide and move only
//! read the previous state and write per-agent buffers, so results are
//! bitwise identical for any number of threads (ADR-0003).

pub mod rng;
pub mod sim;

pub use sim::{RunSummary, SimError, Simulation};

/// Version of the simulator core, recorded in every run manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
