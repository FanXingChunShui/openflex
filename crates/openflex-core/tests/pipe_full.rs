//! task-153: a pipe with a known diameter can only hold its internal volume.

use openflex_core::run_model;
use openflex_model::Model;

fn pipe_model(diameter: Option<f64>, delay: f64) -> Model {
    let diameter_field = diameter
        .map(|value| format!(r#", "diameter": {value}"#))
        .unwrap_or_default();
    let json = format!(
        r#"{{
  "format": "openflex-model",
  "version": 1,
  "name": "pipe-full",
  "description": "",
  "time_unit": "seconds",
  "run": {{"stop_time": 60.0, "warmup": 0.0, "seed": 1, "replications": 1}},
  "objects": [
    {{"name": "G", "kind": "fluid_generator", "pos": [0, 0], "params": {{"flow_rate": 100.0}}}},
    {{"name": "P", "kind": "fluid_pipe", "pos": [10, 0],
      "params": {{"length": 10.0, "speed": 1.0, "delay": {delay}, "flow_rate": 100.0{diameter_field}}}}},
    {{"name": "S", "kind": "fluid_sink", "pos": [20, 0], "params": {{}}}}
  ],
  "connections": [
    {{"from": "G", "to": "P"}},
    {{"from": "P", "to": "S"}}
  ]
}}"#
    );
    Model::from_json(&json).expect("model parses")
}

fn throughput(model: &Model, name: &str) -> f64 {
    let report = run_model(model).expect("run succeeds");
    report
        .fluid
        .expect("fluid report")
        .objects
        .iter()
        .find(|object| object.name == name)
        .map(|object| object.throughput)
        .expect("entry")
}

#[test]
fn unbounded_pipe_keeps_the_old_throughput() {
    // No diameter: the pipe is loss-free, so the generator's 100/s passes on
    // (minus the fixed-step sampling tolerance).
    let throughput = throughput(&pipe_model(None, 0.0), "P");
    assert!(
        (throughput - 100.0).abs() < 1.0,
        "expected ~100/s through a loss-free pipe, got {throughput}"
    );
}

#[test]
fn full_pipe_throttles_the_flow() {
    // d = 0.1, length = 10 → internal volume ≈ 0.0785; with a 30 s delay the
    // pipe can only forward about volume/delay per second, far below 100/s.
    let throughput = throughput(&pipe_model(Some(0.1), 30.0), "P");
    assert!(
        throughput < 5.0,
        "a full pipe should throttle the flow, got {throughput}/s"
    );
    assert!(
        throughput > 0.0,
        "the throttled flow should still be positive, got {throughput}"
    );
}
