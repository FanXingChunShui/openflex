//! task-86 safety net: the default fluid behaviour is pinned before the Darcy
//! branch is added. The Darcy/ρ(T)/valve-opening work must keep these exact.

use openflex_core::{SimConfig, run_model, simulate};
use openflex_model::Model;

fn line_model() -> Model {
    Model::from_json(include_str!("../../../examples/objects/fluid-line.ofm")).expect("model")
}

/// Two runs of the same model produce the identical fluid report (the engine is
/// deterministic, so any later change must keep this true).
#[test]
fn fluid_runs_are_deterministic() {
    let first = run_model(&line_model()).expect("first");
    let second = run_model(&line_model()).expect("second");
    assert_eq!(first, second, "the fluid report must be deterministic");
}

/// The default path (`flow_rate` calibration, no Darcy parameters) keeps the
/// documented rates of the shipped example.
#[test]
fn fluid_line_default_rates_are_pinned() {
    let report = run_model(&line_model()).expect("run");
    let fluid = report.fluid.as_ref().expect("fluid report");
    let rate = |name: &str| {
        fluid
            .objects
            .iter()
            .find(|object| object.name == name)
            .unwrap_or_else(|| panic!("missing {name}"))
            .throughput
    };
    assert!((rate("Generator1") - 10.0).abs() < 0.05, "generator");
    assert!((rate("Tank1") - 5.0).abs() < 0.05, "tank outflow");
    assert!((rate("Valve1") - 2.5).abs() < 0.05, "valve");
    assert!((rate("Sink1") - 2.5).abs() < 0.05, "sink");
    assert!(
        (fluid.total_throughput - 2.5).abs() < 0.05,
        "system throughput"
    );
}

/// A single replication is bit-identical before and after a save/restore-free
/// re-run; this is the baseline the Darcy branch must not disturb.
#[test]
fn fluid_single_replication_is_stable() {
    let model = line_model();
    let config = SimConfig {
        stop_time: model.run.stop_time,
        warmup: model.run.warmup,
        seed: model.run.seed,
    };
    let first = simulate(&model, &config, 0).expect("first");
    let second = simulate(&model, &config, 0).expect("second");
    assert_eq!(first, second);
    assert!(first.fluid.is_some(), "the example is a fluid model");
}
