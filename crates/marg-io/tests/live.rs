//! P4.1: a live session replayed headless from scenario + seed + command log
//! gives the same output hash.

use marg_io::live::{LiveRun, LiveSource, read_commands, replay};
use marg_params::Scenario;
use serde_json::json;
use std::path::{Path, PathBuf};

const SCENARIO: &str = r#"
name: live_test
duration_s: 120
warmup_s: 10
network:
  ribbons:
    - {id: main, polyline: [[0, 0], [600, 0]], width: [{s: 0, y_min: -5.25, y_max: 5.25}]}
demand:
  entries:
    - {id: w, ribbon: main, flow: [{from_s: 0, veh_per_h: 2400}],
       composition: {hatchback: 0.5, sedan: 0.1, motorcycle: 0.3, auto_rickshaw: 0.1}}
detectors:
  - {kind: screenline, id: s300, ribbon: main, s: 300, interval_s: 30}
  - {kind: area, id: a, ribbon: main, s_from: 100, s_to: 500, interval_s: 30}
kpi: {throughput_detector: s300}
outputs:
  trajectories: {every_s: 1.0}
"#;

fn source() -> LiveSource {
    LiveSource {
        params_dir: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../params/base"),
        calibration_dir: None,
    }
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("marg-live-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A scripted session: steps interleaved with viewer and state commands.
fn session() -> LiveRun {
    let sc = Scenario::from_yaml(SCENARIO).unwrap();
    let mut run = LiveRun::new(source(), sc, 11, 1).unwrap();
    let mut wall = 0.0;
    let steps = |run: &mut LiveRun, n: usize| {
        for _ in 0..n {
            run.step();
        }
    };
    run.log("play", &json!({}), &Ok("playing".into()), wall);
    steps(&mut run, 250);
    wall += 1.0;
    let fast = json!({"dist": "trunc_normal", "mean": 75.0, "sd": 15.52, "min": 15, "max": 120});
    for t in ["hatchback", "sedan"] {
        let path = format!("vehicles.free_flow_speed.{t}");
        let (_, r) = run.command("set_param", &json!({"path": path, "value": fast}), wall);
        r.unwrap();
    }
    run.command(
        "resample",
        &json!({"path": "vehicles.free_flow_speed.hatchback"}),
        wall,
    )
    .1
    .unwrap();
    // Refused commands are logged and not replayed.
    let (_, r) = run.command(
        "set_param",
        &json!({"path": "core.lookahead_distance", "value": 99}),
        wall,
    );
    assert!(r.unwrap_err().contains("not live"));
    run.log(
        "select_agent",
        &json!({"id": 1}),
        &Ok("selected".into()),
        wall,
    );
    steps(&mut run, 137);
    wall += 2.5;
    let clearance = json!({"path": "interaction.lateral.clearance.intercept", "value": 90.0});
    run.command("set_param", &clearance, wall).1.unwrap();
    run.log("pause", &json!({}), &Ok("paused".into()), wall);
    steps(&mut run, 400);
    run
}

#[test]
fn replay_reproduces_the_live_output_hash() {
    let run = session();
    let dir = tmp("replay");
    let m = run.save(&dir, "s1").unwrap();
    assert_eq!(m.steps, 787);
    let live = m.live.as_ref().unwrap();
    assert_eq!(live.commands, 8);
    assert_eq!(live.state_commands, 4);
    assert_ne!(
        m.run_hash, live.initial_run_hash,
        "commands change the run identity"
    );
    let log = read_commands(&dir.join("commands.parquet")).unwrap();
    assert_eq!(log, run.commands());
    assert_eq!(log[1].step, 250);
    assert!((log[1].t - 25.0).abs() < 1e-9);

    // Headless replay (twice: the second also writes the outputs).
    let r = replay(&dir, 1, false).unwrap();
    assert!(r.matches, "{r:?}");
    assert_eq!(r.output_hash, m.output_hash);
    assert_eq!(r.commands_applied, 4);
    assert_eq!(r.run_hash, m.run_hash);
    let r4 = replay(&dir, 4, true).unwrap();
    assert!(r4.matches, "4 threads: {r4:?}");
    for t in ["agents", "detectors", "events", "kpis", "trajectories"] {
        assert!(dir.join(format!("{t}.parquet")).is_file(), "{t}");
    }
    let m2 = marg_io::RunManifest::read(&dir).unwrap();
    assert_eq!(m2.live.unwrap().replay_verified, Some(true));

    // A command moved by one step no longer reproduces the session.
    let mut moved = log.clone();
    moved[1].step += 1;
    marg_io::table::write_batch(
        &dir.join("commands.parquet"),
        &marg_io::live::commands_batch(&moved).unwrap(),
    )
    .unwrap();
    assert!(!replay(&dir, 1, false).unwrap().matches);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_untouched_session_is_the_plain_run() {
    let sc = Scenario::from_yaml(SCENARIO).unwrap();
    let mut run = LiveRun::new(source(), sc.clone(), 3, 1).unwrap();
    run.log("play", &json!({}), &Ok("playing".into()), 0.0);
    while !run.sim().finished() {
        run.step();
    }
    let dir = tmp("plain");
    let m = run.save(&dir, "s2").unwrap();
    let opts = marg_io::RunOptions {
        seed: 3,
        params_dir: Some(source().params_dir),
        ..Default::default()
    };
    let headless = marg_io::run_scenario(&sc, &opts, &dir.join("headless")).unwrap();
    assert_eq!(m.run_hash, headless.run_hash);
    assert_eq!(m.output_hash, headless.output_hash);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn history_records_before_and_after() {
    let run = session();
    let h = run.history();
    assert_eq!(h.len(), 3);
    assert_eq!(h[0].path, "vehicles.free_flow_speed.hatchback");
    let reg = run.register().get(&h[0].path).unwrap();
    assert_eq!(h[0].before, reg.value);
    assert!(h[0].after.describe().contains("mean=75"));
    assert_eq!(h[0].seq, 1);
    assert_eq!(h[2].seq, 6);
}
