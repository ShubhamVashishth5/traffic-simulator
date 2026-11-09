//! Parameter register, distributions and scenario schema.
//!
//! This crate is the single source of truth for every input type of the
//! simulator. Its types derive `serde` and `schemars`; the JSON Schema exported
//! from them generates the Python (pydantic) and TypeScript types, so the three
//! sides cannot drift apart (see `plan/02-architecture.md`).

pub mod dist;
pub mod spec;

pub use spec::Scenario;
