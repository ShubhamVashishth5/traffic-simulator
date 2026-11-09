//! Run folders, manifests and Parquet output.

pub mod batch;
pub mod collect;
pub mod live;
pub mod manifest;
pub mod outputs;
pub mod params_table;
pub mod run;
pub mod table;

pub use batch::{BatchOptions, BatchSummary, RunEntry, Selection, run_batch};
pub use collect::{CollectReport, collect};
pub use live::{CommandRecord, LiveRun, LiveSource, ReplayReport, replay};
pub use manifest::{RunManifest, RunMeta};
pub use run::{Inputs, RunError, RunOptions, find_params_dir, run_scenario};
