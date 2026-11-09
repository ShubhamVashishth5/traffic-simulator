//! `marg`: headless command-line runner.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use marg_params::{Grade, ParamValue, Register, RegisterFilter, Role};
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
    /// Inspect the parameter register.
    Params {
        /// Register directory (default: $MARG_PARAMS_DIR or ./params/base).
        #[arg(long, global = true)]
        params: Option<PathBuf>,
        #[command(subcommand)]
        command: ParamsCommand,
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
        /// Register directory (default: $MARG_PARAMS_DIR or ./params/base).
        #[arg(long)]
        params: Option<PathBuf>,
        /// Override a parameter, in its register unit: `--set path=value` (repeatable).
        #[arg(long = "set", value_name = "PATH=VALUE")]
        set: Vec<String>,
        /// Allow overriding `fix` (measured) parameters.
        #[arg(long)]
        allow_fix_override: bool,
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

#[derive(Subcommand)]
enum ParamsCommand {
    /// List parameters, optionally filtered.
    List {
        /// Grade: A, B, C, A/B, B/C.
        #[arg(long)]
        grade: Option<String>,
        /// Role: fix, fit, sweep, scenario.
        #[arg(long)]
        role: Option<String>,
        /// Calibration stage (0-7).
        #[arg(long)]
        stage: Option<u8>,
        /// Path prefix, e.g. `interaction.following`.
        #[arg(long)]
        group: Option<String>,
    },
    /// Show everything known about one parameter.
    Explain {
        path: String,
        /// Also show how this scenario overrides it.
        #[arg(long)]
        scenario: Option<PathBuf>,
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
            for (name, schema) in [
                ("scenario", schemars::schema_for!(marg_params::Scenario)),
                (
                    "register_file",
                    schemars::schema_for!(marg_params::RegisterFile),
                ),
            ] {
                let path = out_dir.join(format!("{name}.schema.json"));
                std::fs::write(&path, serde_json::to_string_pretty(&schema)? + "\n")?;
                println!("wrote {}", path.display());
            }
        }
        Command::Params { params, command } => {
            let dir = params
                .or_else(marg_io::find_params_dir)
                .context("no register found: pass --params or set MARG_PARAMS_DIR")?;
            let reg = Register::load_dir(&dir)?;
            match command {
                ParamsCommand::List {
                    grade,
                    role,
                    stage,
                    group,
                } => {
                    let filter = RegisterFilter {
                        grade: grade.map(|g| parse_grade(&g)).transpose()?,
                        role: role.map(|r| parse_role(&r)).transpose()?,
                        stage,
                        prefix: group,
                    };
                    list(&reg, &filter);
                }
                ParamsCommand::Explain { path, scenario } => explain(&reg, &path, scenario)?,
            }
        }
        Command::Run {
            scenario,
            seed,
            out,
            threads,
            params,
            set,
            allow_fix_override,
        } => {
            let text = std::fs::read_to_string(&scenario)
                .with_context(|| format!("reading {}", scenario.display()))?;
            let mut opts = marg_io::RunOptions {
                seed,
                threads,
                params_dir: params,
                allow_fix_override,
                ..Default::default()
            };
            for s in set {
                let (k, v) = s.split_once('=').context("--set expects PATH=VALUE")?;
                let value: ParamValue =
                    serde_norway::from_str(v).with_context(|| format!("parsing value for {k}"))?;
                opts.overrides.insert(k.to_string(), value);
            }
            let manifest = marg_io::run::run_scenario_text(&text, &opts, &out)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
    }
    Ok(())
}

fn parse_grade(s: &str) -> Result<Grade> {
    Ok(match s.to_ascii_uppercase().as_str() {
        "A" => Grade::A,
        "B" => Grade::B,
        "C" => Grade::C,
        "A/B" | "AB" => Grade::AB,
        "B/C" | "BC" => Grade::BC,
        _ => bail!("unknown grade `{s}` (A, B, C, A/B, B/C)"),
    })
}

fn parse_role(s: &str) -> Result<Role> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "fix" => Role::Fix,
        "fit" => Role::Fit,
        "sweep" => Role::Sweep,
        "scenario" => Role::Scenario,
        _ => bail!("unknown role `{s}` (fix, fit, sweep, scenario)"),
    })
}

fn list(reg: &Register, filter: &RegisterFilter) {
    let rows: Vec<_> = reg.filter(filter).collect();
    let w = rows.iter().map(|(p, _)| p.len()).max().unwrap_or(4).max(4);
    println!(
        "{:w$}  {:42}  {:11}  {:5}  {:8}  source",
        "path", "value", "unit", "grade", "role"
    );
    for (path, e) in &rows {
        let value = e
            .value
            .as_ref()
            .map_or_else(|| "(unset)".to_string(), ParamValue::describe);
        let role = match e.stage {
            Some(s) => format!("fit/{s}"),
            None => e.role.label().to_string(),
        };
        println!(
            "{path:w$}  {:42}  {:11}  {:5}  {:8}  {}",
            truncate(&value, 42),
            e.unit.label(),
            e.grade.label(),
            role,
            truncate(&e.source, 60)
        );
    }
    println!("{} of {} parameters", rows.len(), reg.len());
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

fn explain(reg: &Register, path: &str, scenario: Option<PathBuf>) -> Result<()> {
    let e = reg
        .get(path)
        .with_context(|| format!("`{path}` is not in the register"))?;
    println!("{path}");
    let value = e
        .value
        .as_ref()
        .map_or_else(|| "(unset)".to_string(), ParamValue::describe);
    println!("  value     {value} {}", e.unit.label());
    if let Some(v) = &e.value {
        println!(
            "  in SI     {} {} (mean {:.6})",
            v.to_si(e.unit).describe(),
            e.unit.si_label(),
            v.to_si(e.unit).mean()
        );
    }
    println!("  grade     {}", e.grade.label());
    let stage = e.stage.map(|s| format!(" (stage {s})")).unwrap_or_default();
    println!("  role      {}{stage}", e.role.label());
    if let Some([lo, hi]) = e.range {
        println!("  range     [{lo}, {hi}] {}", e.unit.label());
    }
    println!("  live      {}", e.live);
    println!("  source    {}", e.source);
    println!("  guide     {}", e.guide);
    if let Some(r) = &e.register_row {
        println!("  row       {r}");
    }
    if !e.notes.is_empty() {
        println!("  notes     {}", e.notes);
    }
    if let Some(sc) = scenario {
        let s = marg_params::Scenario::from_yaml(&std::fs::read_to_string(&sc)?)?;
        match s.overrides.get(path) {
            Some(v) => println!("  scenario  {} overrides it with {}", s.name, v.describe()),
            None => println!("  scenario  {} does not override it", s.name),
        }
    }
    Ok(())
}
