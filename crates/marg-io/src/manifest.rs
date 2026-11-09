//! `manifest.json`: run identity and provenance (`plan/04-data-outputs-analytics.md` §1).

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Version of the run-folder layout and manifest schema.
pub const RUN_FORMAT_VERSION: u32 = 2;

/// Identifier of the RNG scheme (ADR-0003).
pub const RNG_SCHEME: &str = "philox4x32-10/v1";

/// Git commit the binary was built from.
pub const GIT_SHA: &str = env!("MARG_GIT_SHA");
/// Whether the working tree had uncommitted changes at build time.
pub const GIT_DIRTY: &str = env!("MARG_GIT_DIRTY");

/// Outcome of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Ok,
    Failed,
    Timeout,
}

/// Where a run sits in an experiment (plan P3.1): the Hive partition keys of
/// its folder, `scenario=<s>/point=<p>/arm=<a>/rep=<r>/run=<hash>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunMeta {
    pub experiment: String,
    pub run_id: u64,
    pub point: u64,
    pub arm: String,
    pub rep: u64,
}

/// Identity and provenance of one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_format_version: u32,
    /// blake3 of (resolved spec, seed, marg version): the run's identity.
    pub run_hash: String,
    pub scenario: String,
    pub seed: u64,
    pub rng_scheme: String,
    pub marg_version: String,
    pub git_sha: String,
    pub git_dirty: bool,
    pub build_profile: String,
    pub threads: usize,
    pub host: String,
    pub wall_time_s: f64,
    pub status: RunStatus,
    /// blake3 over every observed state, in step order: equal hashes mean
    /// bitwise-identical outputs.
    pub output_hash: String,
    pub steps: u64,
    pub agent_steps: u64,
    /// Experiment, design point, arm and replication (absent for ad-hoc runs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment: Option<RunMeta>,
    /// Frozen calibration stages applied (`calibration` layer).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calibration_stages: Vec<u8>,
    /// Why the run failed (status `failed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RunManifest {
    /// Fill the build- and host-dependent fields.
    pub fn build_info() -> (String, String, bool, String, String) {
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let host = std::env::var("HOSTNAME")
            .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
            .unwrap_or_else(|_| "unknown".into());
        (
            marg_core::VERSION.to_string(),
            GIT_SHA.to_string(),
            GIT_DIRTY == "true",
            profile.to_string(),
            host,
        )
    }

    /// A manifest for a run that failed before it produced outputs.
    pub fn failed(
        run_hash: &str,
        scenario: &str,
        seed: u64,
        meta: Option<RunMeta>,
        error: String,
    ) -> Self {
        let (marg_version, git_sha, git_dirty, build_profile, host) = Self::build_info();
        Self {
            run_format_version: RUN_FORMAT_VERSION,
            run_hash: run_hash.to_string(),
            scenario: scenario.to_string(),
            seed,
            rng_scheme: RNG_SCHEME.into(),
            marg_version,
            git_sha,
            git_dirty,
            build_profile,
            threads: 0,
            host,
            wall_time_s: 0.0,
            status: RunStatus::Failed,
            output_hash: String::new(),
            steps: 0,
            agent_steps: 0,
            experiment: meta,
            calibration_stages: Vec::new(),
            error: Some(error),
        }
    }

    /// Write as pretty JSON to `dir/manifest.json`.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(dir.join("manifest.json"), json + "\n")
    }

    /// Read `dir/manifest.json`.
    pub fn read(dir: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(dir.join("manifest.json"))?;
        serde_json::from_str(&text).map_err(std::io::Error::other)
    }
}

/// Serialise a manifest as compact JSON.
pub fn to_json(m: &RunManifest) -> String {
    serde_json::to_string(m).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}
