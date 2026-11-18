//! task-86: the valve's `opening`/`open_curve` characteristic scales the flow.

use openflex_core::run_model;
use openflex_model::Model;

fn valve_model(opening: f64, curve: &str) -> Model {
    let json = format!(
        r#"{{
  "format": "openflex-model",
  "version": 1,
  "name": "valve",
  "description": "",
  "time_unit": "seconds",
  "run": {{"stop_time": 20.0, "warmup": 0.0, "seed": 1, "replications": 1}},
  "objects": [
    {{"name": "G", "kind": "fluid_generator", "pos": [0, 0], "params": {{"flow_rate": 10.0}}}},
    {{"name": "V", "kind": "fluid_valve", "pos": [10, 0],
      "params": {{"open": true, "ratio": 1.0, "opening": {opening}, "open_curve": "{curve}"}}}},
    {{"name": "S", "kind": "fluid_sink", "pos": [20, 0], "params": {{}}}}
  ],
  "connections": [
    {{"from": "G", "to": "V"}},
    {{"from": "V", "to": "S"}}
  ]
}}"#
    );
    Model::from_json(&json).expect("model parses")
}

fn valve_throughput(opening: f64, curve: &str) -> f64 {
    let report = run_model(&valve_model(opening, curve)).expect("run succeeds");
    let fluid = report.fluid.expect("fluid report");
    fluid
        .objects
        .iter()
        .find(|object| object.name == "V")
        .map(|object| object.throughput)
        .expect("valve entry")
}

#[test]
fn fully_open_valve_passes_everything() {
    let fully_open = valve_throughput(1.0, "linear");
    assert!(
        (fully_open - 9.95).abs() < 0.2,
        "expected the full 10/s flow, got {fully_open}"
    );
}

#[test]
fn linear_opening_scales_the_flow() {
    let half = valve_throughput(0.5, "linear");
    assert!(
        (half - 5.0).abs() < 0.3,
        "linear opening 0.5 should pass ~5/s, got {half}"
    );
}

#[test]
fn equal_percentage_opening_is_geometric() {
    let half = valve_throughput(0.5, "equal_percentage");
    // R^(x-1) with R = 50 at x = 0.5 → 50^-0.5 ≈ 0.1414 → ~1.4/s.
    assert!(
        (half - 1.4).abs() < 0.2,
        "equal-percentage opening 0.5 should pass ~1.4/s, got {half}"
    );
}

#[test]
fn equal_percentage_is_lower_than_linear_at_the_same_opening() {
    let linear = valve_throughput(0.5, "linear");
    let equal = valve_throughput(0.5, "equal_percentage");
    assert!(
        equal < linear,
        "equal-percentage ({equal}) should be below linear ({linear}) at 50% opening"
    );
}
