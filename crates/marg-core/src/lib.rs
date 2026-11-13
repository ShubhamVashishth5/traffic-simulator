//! Simulation core of the marg lane-free mixed-traffic simulator.
//!
//! The step loop follows `plan/02-architecture.md` §4. Only the move phase is
//! parallel, and it reads the previous state and writes per-agent results in
//! a fixed order, so results are bitwise identical for any number of threads
//! (ADR-0003).

pub mod agents;
pub mod demand;
pub mod junction_scan;
pub mod junctions;
pub mod model;
pub mod objects;
pub mod observe;
pub mod output;
pub mod params;
pub mod parking;
pub mod pedestrians;
pub mod rng;
pub mod scan;
pub mod sim;
pub mod stopping;
pub mod surface;
pub mod width;

pub use output::{
    AgentRecord, ConflictRecord, DetectorRecord, EffectiveWidthRecord, EventRecord, RunOutputs,
    SignalCycleRecord, TrajectoryChunk, TrajectorySink,
};
pub use params::CoreParams;
pub use sim::{RunSummary, SimError, Simulation};

/// Version of the simulator core, recorded in every run manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
