//! Run folders, manifests and Parquet output.

pub mod manifest;
pub mod params_table;
pub mod run;
pub mod table;

pub use manifest::RunManifest;
pub use run::{RunError, RunOptions, find_params_dir, run_scenario};
