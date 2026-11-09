//! `marg`: headless command-line runner.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "marg", version, about = "Lane-free mixed-traffic simulator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print version and build information.
    Version,
    /// Schema utilities.
    Schema {
        #[command(subcommand)]
        command: SchemaCommand,
    },
    /// Run one scenario headless and write a run folder.
    Run {
        /// Scenario YAML file.
        scenario: PathBuf,
        /// Master random seed.
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Output directory for the run folder.
        #[arg(long, default_value = "runs/adhoc")]
        out: PathBuf,
        /// Worker threads (0 = all cores).
        #[arg(long, default_value_t = 1)]
        threads: usize,
    },
}

#[derive(Subcommand)]
enum SchemaCommand {
    /// Write the JSON Schema of all input types to a directory.
    Export {
        #[arg(long, default_value = "schema")]
        out_dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => {
            let (version, sha, dirty, profile, _) = marg_io::RunManifest::build_info();
            println!(
                "marg {version} ({sha}{}, {profile})",
                if dirty { "-dirty" } else { "" }
            );
        }
        Command::Schema {
            command: SchemaCommand::Export { out_dir },
        } => {
            std::fs::create_dir_all(&out_dir)?;
            let schema = schemars::schema_for!(marg_params::Scenario);
            let path = out_dir.join("scenario.schema.json");
            std::fs::write(&path, serde_json::to_string_pretty(&schema)? + "\n")?;
            println!("wrote {}", path.display());
        }
        Command::Run {
            scenario,
            seed,
            out,
            threads,
        } => {
            let text = std::fs::read_to_string(&scenario)
                .with_context(|| format!("reading {}", scenario.display()))?;
            let summary = marg_io::run::run_scenario_text(&text, seed, threads, &out)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
    }
    Ok(())
}
