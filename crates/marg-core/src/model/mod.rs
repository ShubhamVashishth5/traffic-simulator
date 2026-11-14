//! Behaviour models (plan/03-model-design.md). Pure functions over plain
//! numbers: every constant comes in as an argument from `CoreParams`.
//!
//! A test (`tests/no_magic_numbers.rs`) rejects float literals in this
//! directory other than 0, 1, 2 and 0.5 used as arithmetic, unless the line
//! says `allow-literal` with a reason.

pub mod coverage;
pub mod decision;
pub mod geometry;
pub mod lateral;
pub mod longitudinal;
pub mod traits;
