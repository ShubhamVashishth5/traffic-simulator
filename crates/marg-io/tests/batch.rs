//! `marg batch` / `marg collect`: resumable, atomic, checked (plan P3.5).

use indexmap::IndexMap;
use marg_io::{BatchOptions, Inputs, RunEntry, Selection, collect, run_batch};
use marg_params::Scenario;
use std::path::{Path, PathBuf};

const TINY: &str = r#"
name: tiny
duration_s: 30
network:
  ribbons:
    - {id: main, polyline: [[0, 0], [300, 0]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
demand:
  entries:
    - {id: w, ribbon: main, flow: [{from_s: 0, veh_per_h: 1200}], composition: {hatchback: 1.0}}
"#;

fn params() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../params/base")
}

fn experiment(name: &str, n: u64) -> PathBuf {
    let root = std::env::temp_dir().join(format!("marg-batch-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("scenarios")).unwrap();
    std::fs::write(root.join("scenarios/tiny.yaml"), TINY).unwrap();
    let inputs = Inputs::load(Some(&params()), None).unwrap();
    let sc = Scenario::from_yaml(TINY).unwrap();
    let spec = inputs.resolve(&sc, &IndexMap::new(), false).unwrap();
    let mut lines = String::new();
    for rep in 0..n {
        let seed = 1 + rep;
        let hash = spec.run_hash(seed, marg_core::VERSION);
        let e = RunEntry {
            run_id: rep,
            run_hash: hash.clone(),
            experiment: name.into(),
            scenario: "tiny".into(),
            scenario_file: "scenarios/tiny.yaml".into(),
            point: 0,
            arm: "base".into(),
            rep,
            seed,
            overrides: IndexMap::new(),
            allow_fix_override: false,
            path: format!("scenario=tiny/point=0/arm=base/rep={rep}/run={hash}"),
        };
        lines += &(serde_json::to_string(&e).unwrap() + "\n");
    }
    std::fs::write(root.join("runs.jsonl"), lines).unwrap();
    root
}

fn opts(selection: Selection) -> BatchOptions {
    BatchOptions {
        params_dir: Some(params()),
        selection,
        ..Default::default()
    }
}

#[test]
fn batch_is_resumable_and_collect_indexes() {
    let root = experiment("resume", 4);
    let s = run_batch(&root, &opts(Selection::Chunk { index: 1, size: 2 })).unwrap();
    assert_eq!((s.selected, s.ran, s.skipped), (2, 2, 0));
    let s = run_batch(&root, &opts(Selection::All)).unwrap();
    assert_eq!((s.ran, s.skipped, s.failed), (2, 2, 0));
    let s = run_batch(&root, &opts(Selection::All)).unwrap();
    assert_eq!((s.ran, s.skipped), (0, 4));

    // An interrupted run leaves only a partial sibling; collect finds and removes it.
    let entries = marg_io::batch::read_run_list(&root.join("runs.jsonl")).unwrap();
    let done = root.join(&entries[3].path);
    let partial = done.with_file_name(format!(
        "{}.partial-host-1",
        done.file_name().unwrap().to_string_lossy()
    ));
    std::fs::create_dir_all(&partial).unwrap();
    std::fs::remove_file(done.join("manifest.json")).unwrap();
    let r = collect(&root, false).unwrap();
    assert_eq!((r.total, r.ok, r.missing, r.partials.len()), (4, 3, 1, 1));
    assert!(!r.complete());
    let s = run_batch(&root, &opts(Selection::All)).unwrap();
    assert_eq!((s.ran, s.skipped), (1, 3));
    let r = collect(&root, true).unwrap();
    assert!(r.complete(), "{r:?}");
    assert!(!partial.exists());
    assert!(r.orphans.is_empty());
    assert!(root.join("index.parquet").exists());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn hash_mismatch_fails_the_run() {
    let root = experiment("mismatch", 1);
    let text = std::fs::read_to_string(root.join("runs.jsonl")).unwrap();
    std::fs::write(
        root.join("runs.jsonl"),
        text.replace("\"seed\":1", "\"seed\":99"),
    )
    .unwrap();
    let s = run_batch(&root, &opts(Selection::All)).unwrap();
    assert_eq!(s.failed, 1);
    let r = collect(&root, false).unwrap();
    assert_eq!(r.failed, 1);
    std::fs::remove_dir_all(&root).unwrap();
}
