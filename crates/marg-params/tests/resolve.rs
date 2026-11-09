//! Resolver behaviour on the shipped register and scenarios (plan P1.4, P1.5).

use indexmap::IndexMap;
use marg_params::{Layer, ParamValue, Register, ResolveError, ResolveOptions, Scenario, resolve};
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn register() -> Register {
    Register::load_dir(&repo().join("params/base")).unwrap()
}
fn scenario(name: &str) -> Scenario {
    Scenario::from_yaml(&std::fs::read_to_string(repo().join("scenarios").join(name)).unwrap())
        .unwrap()
}
fn none() -> IndexMap<String, ParamValue> {
    IndexMap::new()
}

#[test]
fn shipped_scenarios_resolve() {
    let reg = register();
    for s in ["corridor_2km.yaml", "free_flow_early_morning.yaml"] {
        let spec = resolve(&reg, &scenario(s), &none(), ResolveOptions::default()).unwrap();
        assert_eq!(spec.params.len(), reg.len());
    }
}

#[test]
fn units_are_converted_to_si() {
    let spec = resolve(
        &register(),
        &scenario("corridor_2km.yaml"),
        &none(),
        ResolveOptions::default(),
    )
    .unwrap();
    // 120.9 cm → 1.209 m
    assert!(
        (spec
            .scalar_si("interaction.lateral.clearance.intercept")
            .unwrap()
            - 1.209)
            .abs()
            < 1e-12
    );
    // free-flow speed 49.19 km/h → 13.664 m/s; truncation at 10 km/h (3.2 SD below)
    // raises the mean by about 0.035 km/h.
    let d = spec.dist_si("vehicles.free_flow_speed.motorcycle").unwrap();
    assert!((d.mean() - 49.19 / 3.6).abs() < 0.02, "mean {}", d.mean());
    let marg_params::Dist::TruncNormal { mean, sd, .. } = d else {
        panic!("expected trunc_normal")
    };
    assert!((mean - 49.19 / 3.6).abs() < 1e-12 && (sd - 12.43 / 3.6).abs() < 1e-12);
}

#[test]
fn overrides_apply_in_layer_order_with_provenance() {
    let reg = register();
    let mut sc = scenario("corridor_2km.yaml");
    sc.overrides.insert(
        "interaction.lateral.politeness".into(),
        ParamValue::Scalar(0.3),
    );
    sc.overrides.insert(
        "interaction.lateral.steering_scale".into(),
        ParamValue::Scalar(0.4),
    );
    let mut exp = IndexMap::new();
    exp.insert(
        "interaction.lateral.politeness".to_string(),
        ParamValue::Scalar(0.5),
    );
    let spec = resolve(&reg, &sc, &exp, ResolveOptions::default()).unwrap();
    let p = spec.get("interaction.lateral.politeness").unwrap();
    assert_eq!(p.value, Some(ParamValue::Scalar(0.5)));
    assert_eq!(p.set_by, Layer::Experiment);
    assert_eq!(
        spec.get("interaction.lateral.steering_scale")
            .unwrap()
            .set_by,
        Layer::Scenario
    );
    assert_eq!(
        spec.get("interaction.lateral.max_heading").unwrap().set_by,
        Layer::Base
    );
    assert!(
        spec.scenario.overrides.is_empty(),
        "overrides are folded into params"
    );
}

#[test]
fn rejects_unknown_out_of_range_and_fix_overrides() {
    let reg = register();
    let sc = scenario("corridor_2km.yaml");
    let one = |k: &str, v: f64| {
        let mut m = IndexMap::new();
        m.insert(k.to_string(), ParamValue::Scalar(v));
        m
    };
    let opts = ResolveOptions::default();
    assert!(matches!(
        resolve(&reg, &sc, &one("no.such.param", 1.0), opts),
        Err(ResolveError::Unknown(_))
    ));
    assert!(matches!(
        resolve(&reg, &sc, &one("interaction.lateral.politeness", 5.0), opts),
        Err(ResolveError::Invalid { .. })
    ));
    assert!(matches!(
        resolve(
            &reg,
            &sc,
            &one("interaction.following.emergency_decel", 8.0),
            opts
        ),
        Err(ResolveError::FixOverride(_))
    ));
    let allowed = resolve(
        &reg,
        &sc,
        &one("interaction.following.emergency_decel", 8.0),
        ResolveOptions {
            allow_fix_override: true,
        },
    )
    .unwrap();
    assert!(
        allowed
            .get("interaction.following.emergency_decel")
            .unwrap()
            .fix_overridden
    );
}

#[test]
fn unset_parameters_error_only_when_read() {
    let spec = resolve(
        &register(),
        &scenario("corridor_2km.yaml"),
        &none(),
        ResolveOptions::default(),
    )
    .unwrap();
    assert!(matches!(
        spec.scalar_si("violations.red_light.intercept"),
        Err(ResolveError::Unset { .. })
    ));
}

#[test]
fn run_hash_is_stable_and_sensitive() {
    let reg = register();
    let sc = scenario("corridor_2km.yaml");
    let a = resolve(&reg, &sc, &none(), ResolveOptions::default()).unwrap();
    let b = resolve(&reg, &sc, &none(), ResolveOptions::default()).unwrap();
    assert_eq!(a.run_hash(1, "0.1.0"), b.run_hash(1, "0.1.0"));
    assert_ne!(a.run_hash(1, "0.1.0"), a.run_hash(2, "0.1.0"));
    // The same value set through a different layer is the same run.
    let mut sc2 = sc.clone();
    sc2.overrides.insert(
        "interaction.lateral.politeness".into(),
        ParamValue::Scalar(0.2),
    );
    let c = resolve(&reg, &sc2, &none(), ResolveOptions::default()).unwrap();
    assert_eq!(a.run_hash(1, "x"), c.run_hash(1, "x"));
    let mut sc3 = sc.clone();
    sc3.overrides.insert(
        "interaction.lateral.politeness".into(),
        ParamValue::Scalar(0.21),
    );
    let d = resolve(&reg, &sc3, &none(), ResolveOptions::default()).unwrap();
    assert_ne!(a.run_hash(1, "x"), d.run_hash(1, "x"));
}

#[test]
fn run_hash_ignores_register_file_order() {
    // Load the register files in reverse order: same entries, same hash.
    let dir = repo().join("params/base");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files.reverse();
    let mut rev = Register::default();
    for f in files {
        rev.add_yaml(
            &std::fs::read_to_string(&f).unwrap(),
            &f.display().to_string(),
        )
        .unwrap();
    }
    let sc = scenario("corridor_2km.yaml");
    let a = resolve(&register(), &sc, &none(), ResolveOptions::default()).unwrap();
    let b = resolve(&rev, &sc, &none(), ResolveOptions::default()).unwrap();
    assert_eq!(a.run_hash(3, "v"), b.run_hash(3, "v"));
}

#[test]
fn resolved_spec_round_trips_through_yaml() {
    let spec = resolve(
        &register(),
        &scenario("corridor_2km.yaml"),
        &none(),
        ResolveOptions::default(),
    )
    .unwrap();
    let yaml = spec.to_yaml().unwrap();
    let back: marg_params::ResolvedSpec = serde_norway::from_str(&yaml).unwrap();
    assert_eq!(back.params, spec.params);
    assert_eq!(back.run_hash(1, "v"), spec.run_hash(1, "v"));
}

#[test]
fn calibration_layer_sits_between_register_and_scenario() {
    let reg = register();
    let path = "interaction.following.time_gap.car";
    let mut cal = none();
    cal.insert(path.to_string(), ParamValue::Scalar(1.1));
    cal.insert(
        "interaction.following.time_gap.bus".to_string(),
        ParamValue::Scalar(1.3),
    );
    let mut sc = scenario("corridor_2km.yaml");
    sc.overrides
        .insert(path.to_string(), ParamValue::Scalar(0.9));
    let spec =
        marg_params::resolve_layers(&reg, &cal, &sc, &none(), ResolveOptions::default()).unwrap();
    // The scenario wins over calibration; untouched calibrated values stay.
    assert_eq!(spec.params[path].set_by, Layer::Scenario);
    assert_eq!(spec.params[path].si, Some(ParamValue::Scalar(0.9)));
    let bus = &spec.params["interaction.following.time_gap.bus"];
    assert_eq!(bus.set_by, Layer::Calibration);
    assert_eq!(bus.si, Some(ParamValue::Scalar(1.3)));
    // Calibration changes the run's identity.
    let plain = resolve(&reg, &sc, &none(), ResolveOptions::default()).unwrap();
    assert_ne!(plain.run_hash(1, "v"), spec.run_hash(1, "v"));
}
