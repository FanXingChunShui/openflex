//! Fluid family tests (B5).

use openflex_core::{SimConfig, run_model, simulate_with_checkpoint};
use openflex_model::Model;

fn line_model() -> Model {
    Model::from_json(include_str!("../../../examples/objects/fluid-line.ofm")).expect("model")
}

fn block<'a>(
    report: &'a openflex_core::FluidReport,
    name: &str,
) -> &'a openflex_core::FluidObjectReport {
    report
        .objects
        .iter()
        .find(|object| object.name == name)
        .unwrap_or_else(|| panic!("missing fluid object {name}"))
}

fn assert_near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected} ± {tolerance}, got {actual}"
    );
}

#[test]
fn fluid_line_reports_levels_flows_and_overflow() {
    let report = run_model(&line_model()).expect("run");
    let fluid = report.fluid.as_ref().expect("fluid report");
    // Only the steps strictly before the stop time move fluid, so the rates
    // are 59.9/60 of the nominal values (see the module docs).
    let generator = block(fluid, "Generator1");
    assert_near(generator.throughput, 10.0, 0.05, "generator throughput");
    assert_near(fluid.total_inflow, 10.0, 0.05, "total inflow");

    let tank = block(fluid, "Tank1");
    assert_near(tank.level_max, 100.0, 1e-9, "tank level max");
    assert_near(tank.level_avg, 83.0, 1.5, "tank level avg");
    assert_near(tank.throughput, 5.0, 0.05, "tank outflow");
    assert_near(tank.overflow, 200.0, 1.0, "tank overflow");

    let valve = block(fluid, "Valve1");
    let sink = block(fluid, "Sink1");
    assert_near(valve.throughput, 2.5, 0.05, "valve throughput");
    assert_near(sink.throughput, 2.5, 0.05, "sink throughput");
    assert_near(fluid.total_throughput, 2.5, 0.05, "system throughput");
    assert_near(fluid.total_overflow, 200.0, 1.0, "system overflow");
}

#[test]
fn fluid_link_flow_rate_caps_the_link() {
    let json = include_str!("../../../examples/objects/fluid-line.ofm").replace(
        r#"{ "from": "Valve1", "to": "Sink1" }"#,
        r#"{ "from": "Valve1", "to": "Sink1", "flow_rate": 1.0 }"#,
    );
    let model = Model::from_json(&json).expect("model");
    let report = run_model(&model).expect("run");
    let fluid = report.fluid.as_ref().expect("fluid report");
    // The valve offers 2.5/s but the link only carries 1/s.
    assert_near(block(fluid, "Sink1").throughput, 1.0, 0.05, "capped link");
    assert_near(block(fluid, "Valve1").throughput, 1.0, 0.05, "valve sent");
}

#[test]
fn fluid_and_discrete_objects_coexist() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "fluid-mixed",
      "run": { "stop_time": 20.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "fluid_step": 0.5,
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "max_arrivals": 2 } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "G", "kind": "fluid_generator", "params": { "flow_rate": 2.0 } },
        { "name": "T", "kind": "fluid_tank", "params": { "capacity": 10.0 } },
        { "name": "FS", "kind": "fluid_sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "K" },
        { "from": "G", "to": "T" },
        { "from": "T", "to": "FS" }
      ]
    }"#;
    let report = run_model(&Model::from_json(json).expect("model")).expect("run");
    assert_eq!(report.objects["K"].input_count.mean, 2.0);
    let fluid = report.fluid.as_ref().expect("fluid report");
    assert_eq!(fluid.objects.len(), 3);
}

#[test]
fn fluid_checkpoint_resume_matches_a_direct_run() {
    let model = line_model();
    let first = SimConfig {
        stop_time: 25.0,
        warmup: 0.0,
        seed: 1,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let second = SimConfig {
        stop_time: 60.0,
        ..first
    };
    let (resumed, _) =
        simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint)).expect("resume");
    let (direct, _) = simulate_with_checkpoint(&model, &second, 0, None).expect("direct");
    assert_eq!(
        resumed, direct,
        "fluid resume must reproduce the direct run"
    );
}

fn fluid_model(blocks: &str, connections: &str, fluid_step: &str) -> String {
    format!(
        r#"{{
          "format": "openflex-model", "version": 1, "name": "fluid-validation",
          "fluid_step": {fluid_step},
          "objects": [{blocks}],
          "connections": [{connections}]
        }}"#
    )
}

#[test]
fn invalid_fluid_models_are_rejected() {
    // Fluid and discrete objects cannot be connected.
    let mixed = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-mixed",
      "objects": [
        { "name": "S", "kind": "source", "params": {} },
        { "name": "G", "kind": "fluid_generator", "params": {} },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "G" }, { "from": "G", "to": "K" }]
    }"#;
    let error = Model::from_json(mixed).expect_err("mixed connection");
    assert!(error.to_string().contains("fluid and discrete"), "{error}");

    // A pipe cycle is rejected.
    let cycle = fluid_model(
        r#"{"name":"G","kind":"fluid_generator"},
           {"name":"T1","kind":"fluid_tank"},
           {"name":"T2","kind":"fluid_tank"},
           {"name":"S","kind":"fluid_sink"}"#,
        r#"{"from":"G","to":"T1"},{"from":"T1","to":"T2"},
           {"from":"T2","to":"T1"},{"from":"T2","to":"S"}"#,
        "0.1",
    );
    let error = Model::from_json(&cycle).expect_err("cycle");
    assert!(error.to_string().contains("acyclic"), "{error}");

    // A bad valve ratio, tank level and step are rejected.
    for (blocks, step) in [
        (
            r#"{"name":"G","kind":"fluid_generator"},
            {"name":"V","kind":"fluid_valve","params":{"ratio":2.0}},
            {"name":"S","kind":"fluid_sink"}"#,
            "0.1",
        ),
        (
            r#"{"name":"G","kind":"fluid_generator"},
            {"name":"T","kind":"fluid_tank","params":{"capacity":10.0,"initial_level":20.0}},
            {"name":"S","kind":"fluid_sink"}"#,
            "0.1",
        ),
    ] {
        let json = fluid_model(blocks, r#"{"from":"G","to":"S"}"#, step);
        assert!(
            Model::from_json(&json).is_err(),
            "expected validation error"
        );
    }
    let bad_step = fluid_model(
        r#"{"name":"G","kind":"fluid_generator"},{"name":"S","kind":"fluid_sink"}"#,
        r#"{"from":"G","to":"S"}"#,
        "-1",
    );
    assert!(Model::from_json(&bad_step).is_err(), "negative step");
}

#[test]
fn fluid_pipe_buffers_and_delays_volume() {
    let model =
        Model::from_json(include_str!("../../../examples/objects/fluid-pipe.ofm")).expect("model");
    let report = run_model(&model).expect("run");
    let fluid = report.fluid.as_ref().expect("fluid report");
    let pipe = block(fluid, "Pipe1");
    // 10 volume units per second stay in transit for 5 seconds.
    assert_near(pipe.level_max, 50.0, 0.5, "pipe in-transit volume");
    assert_near(pipe.level_avg, 47.9, 1.0, "pipe average volume");
    assert_near(pipe.throughput, 9.15, 0.05, "pipe throughput");
    let sink = block(fluid, "Sink1");
    assert_near(sink.throughput, 9.15, 0.05, "sink throughput after delay");
    let tank = block(fluid, "Tank1");
    assert_near(tank.throughput, 10.0, 0.05, "tank outflow");
    assert_near(fluid.total_inflow, 10.0, 0.05, "total inflow");
}

#[test]
fn valve_pressure_threshold_closes_the_line() {
    let json = include_str!("../../../examples/objects/fluid-pipe.ofm").replace(
        "\"pressure_threshold\": 5.0",
        "\"pressure_threshold\": 15.0",
    );
    let model = Model::from_json(&json).expect("model");
    let report = run_model(&model).expect("run");
    let fluid = report.fluid.as_ref().expect("fluid report");
    // The valve latches closed after the first quantised pressure measurement,
    // so at most one step of volume passes.
    assert!(block(fluid, "Sink1").throughput <= 0.02);
    assert!(block(fluid, "Valve1").throughput <= 0.02);
    // Backpressure: the pipe buffers the arriving volume instead of dropping it.
    assert!(block(fluid, "Pipe1").level_max > 100.0);
}

#[test]
fn invalid_pipe_and_threshold_params_are_rejected() {
    let bad_pipe = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-pipe",
      "objects": [
        { "name": "G", "kind": "fluid_generator", "params": {} },
        { "name": "P", "kind": "fluid_pipe", "params": { "length": 0.0 } },
        { "name": "S", "kind": "fluid_sink", "params": {} }
      ],
      "connections": [
        { "from": "G", "to": "P" }, { "from": "P", "to": "S" }
      ]
    }"#;
    assert!(Model::from_json(bad_pipe).is_err(), "zero length");

    let bad_threshold = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-threshold",
      "objects": [
        { "name": "G", "kind": "fluid_generator", "params": {} },
        { "name": "V", "kind": "fluid_valve", "params": { "pressure_threshold": -1.0 } },
        { "name": "S", "kind": "fluid_sink", "params": {} }
      ],
      "connections": [
        { "from": "G", "to": "V" }, { "from": "V", "to": "S" }
      ]
    }"#;
    assert!(
        Model::from_json(bad_threshold).is_err(),
        "negative threshold"
    );
}

#[test]
fn closed_valve_backpressures_the_tank() {
    let model = Model::from_json(include_str!(
        "../../../examples/objects/fluid-backpressure.ofm"
    ))
    .expect("model");
    let report = run_model(&model).expect("run");
    let fluid = report.fluid.as_ref().expect("fluid report");
    let tank = block(fluid, "Tank1");
    // The tank fills to capacity while the closed valve blocks the release,
    // then the generator inflow becomes overflow.
    assert_near(tank.level_max, 100.0, 0.5, "tank level");
    assert!(tank.overflow > 450.0, "overflow {}", tank.overflow);
    assert!(tank.throughput <= 0.02, "release {}", tank.throughput);
    assert!(block(fluid, "Valve1").pressure >= 0.0);
    assert!(block(fluid, "Sink1").throughput <= 0.02);
}
