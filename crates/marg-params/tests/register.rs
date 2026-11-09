//! The shipped register (`params/base`) loads, validates, and covers every row
//! of the guide's master register (G§9) — plan P1.3.

use marg_params::{Register, RegisterFilter, Role};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn register() -> Register {
    Register::load_dir(&repo().join("params/base")).expect("params/base loads and validates")
}

fn yaml(path: &Path) -> serde_norway::Value {
    serde_norway::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn register_loads_and_validates() {
    let reg = register();
    assert!(
        reg.len() > 200,
        "expected the full register, got {} entries",
        reg.len()
    );
}

#[test]
fn every_fit_entry_has_a_stage_and_every_entry_a_row() {
    let reg = register();
    for (path, e) in reg.iter() {
        assert!(e.register_row.is_some(), "{path} has no register_row");
        if e.role == Role::Fit {
            assert!(e.stage.is_some(), "{path} is fit without a stage");
        }
    }
}

#[test]
fn guide_rows_are_covered() {
    let reg = register();
    let rows = yaml(&repo().join("params/guide_rows.yaml"));
    let mut validation_rows = BTreeSet::new();
    for entry in std::fs::read_dir(repo().join("validation/targets")).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_some_and(|x| x == "yaml") {
            for t in yaml(&p)["targets"].as_sequence().unwrap() {
                validation_rows.insert(t["register_row"].as_str().unwrap().to_string());
            }
        }
    }
    let register_rows: BTreeSet<String> = reg
        .iter()
        .filter_map(|(_, e)| e.register_row.clone())
        .collect();
    let mut known = BTreeSet::new();
    let mut missing = Vec::new();
    for row in rows["rows"].as_sequence().unwrap() {
        let id = row["id"].as_str().unwrap().to_string();
        let by = row["covered_by"].as_str().unwrap();
        let ok = match by {
            "register" => register_rows.contains(&id),
            "validation" => validation_rows.contains(&id),
            "scenario" | "bundle" => row["where"].as_str().is_some(),
            other => panic!("unknown covered_by `{other}` for {id}"),
        };
        if !ok {
            missing.push(format!("{id} ({by})"));
        }
        known.insert(id);
    }
    assert!(missing.is_empty(), "guide rows not covered: {missing:?}");
    let unknown: Vec<_> = register_rows
        .union(&validation_rows)
        .filter(|r| !known.contains(*r))
        .collect();
    assert!(
        unknown.is_empty(),
        "register_row values not in guide_rows.yaml: {unknown:?}"
    );
    // 13 + 12 + 8 + 11 + 8 + 15 + 8 + 8 rows in guide §9.1–9.8.
    let guide_rows = known.iter().filter(|r| r.starts_with("9.")).count();
    assert_eq!(guide_rows, 83);
}

#[test]
fn every_vehicle_type_and_class_has_its_entries() {
    let reg = register();
    for t in marg_params::VehicleType::ALL {
        for p in ["length", "width", "free_flow_speed"] {
            let path = format!("vehicles.{p}.{}", t.key());
            assert!(reg.get(&path).is_some(), "missing {path}");
        }
    }
    for c in marg_params::VehicleClass::ALL {
        for p in [
            "interaction.following.time_gap",
            "interaction.following.standstill_gap",
            "interaction.following.max_accel",
            "interaction.following.comfortable_decel",
            "interaction.gap_acceptance.critical_gap",
            "demand.min_headway",
            "demand.entry_lateral_position",
        ] {
            let path = format!("{p}.{}", c.key());
            assert!(reg.get(&path).is_some(), "missing {path}");
        }
    }
}

#[test]
fn filter_by_grade_and_role() {
    let reg = register();
    let f = RegisterFilter {
        grade: Some(marg_params::Grade::C),
        role: Some(Role::Sweep),
        ..Default::default()
    };
    let n = reg.filter(&f).count();
    assert!(n > 20, "expected many grade-C sweep parameters, got {n}");
    let f = RegisterFilter {
        stage: Some(2),
        ..Default::default()
    };
    assert!(reg.filter(&f).all(|(_, e)| e.role == Role::Fit));
}
