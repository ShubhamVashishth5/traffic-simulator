//! Parameter register, distributions and scenario schema.
//!
//! This crate is the single source of truth for every input type of the
//! simulator. Its types derive `serde` and `schemars`; the JSON Schema exported
//! from them generates the Python (pydantic) and TypeScript types, so the three
//! sides cannot drift apart (see `plan/02-architecture.md`).

pub mod dist;
pub mod param;
pub mod register;
pub mod resolve;
pub mod spec;
pub mod units;
pub mod vehicle;

pub use dist::Dist;
pub use param::{Grade, ParamEntry, ParamValue, Role};
pub use register::{Register, RegisterFile, RegisterFilter};
pub use resolve::{Layer, ResolveError, ResolveOptions, ResolvedParam, ResolvedSpec, resolve};
pub use spec::Scenario;
pub use units::Unit;
pub use vehicle::{VehicleClass, VehicleType};
