//! task-47: the in-memory [`SimulationHandle`].
//!
//! The handle advances the *same* engine repeatedly instead of rebuilding it,
//! so a host can step, pause, resume, snapshot and inspect a run.

use openflex_core::{SimConfig, SimulationHandle, simulate};
use openflex_model::Model;

fn model(json: &str) -> Model {
    Model::from_json(json).expect("model should load")
}

fn config(stop_time: f64) -> SimConfig {
    SimConfig {
        stop_time,
        warmup: 0.0,
        seed: 1,
    }
}

fn line_model() -> Model {
    model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "handle",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 100 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 4.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" }, { "from": "Q", "to": "P" }, { "from": "P", "to": "K" }
      ]
    }"#,
    )
}

/// Several incremental steps produce exactly the same result as one run.
#[test]
fn stepping_matches_a_single_run() {
    let model = line_model();
    let config = config(200.0);
    let direct = simulate(&model, &config, 0).expect("direct run");

    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");
    for time in [25.0, 50.0, 75.0, 100.0, 150.0, 200.0] {
        handle.step_until(time).expect("step");
    }
    assert_eq!(handle.result(), direct, "stepping must not change the run");
}

/// `state()` reports the current time and the object statistics up to it.
#[test]
fn state_reports_progress() {
    let model = line_model();
    let config = config(200.0);
    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");
    handle.step_until(50.0).expect("step");

    let state = handle.state();
    assert_eq!(state.now, 50.0);
    assert!(!state.paused);
    // Five arrivals at t = 0, 10, 20, 30, 40 (t = 50 belongs to the next step).
    assert_eq!(state.items_created, 5);
    assert_eq!(state.objects["S"].output_count, 5);
    // The statistics window is 50 time units long.
    assert!((state.objects["S"].throughput - 0.1).abs() < 1e-9);
    assert_eq!(handle.now(), 50.0);
}

/// `pause()` / `resume()` fire the run-level triggers and the pause flag is
/// visible in `state()`.
#[test]
fn pause_and_resume_are_observable() {
    let model = model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "handle-pause",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 20.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 100,
            "triggers": {
              "on_stop": [ { "action": "send_message", "target": "Q", "delay": "0" } ],
              "on_message": [ { "action": "release_item" } ] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" },
        { "from": "Q", "to": "K", "strategy": { "type": "hold" } }
      ]
    }"#,
    );
    let config = config(200.0);
    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");
    handle.step_until(50.0).expect("step");
    assert!(!handle.is_paused());

    handle.pause().expect("pause");
    assert!(handle.is_paused());
    assert!(handle.state().paused);
    // `on_stop` released the item that arrived at t = 0.
    assert_eq!(handle.state().objects["K"].input_count, 1);

    handle.resume().expect("resume");
    assert!(!handle.is_paused());
    handle.step_until(200.0).expect("continue");
    assert_eq!(handle.state().now, 200.0);
}

/// A snapshot taken mid-run can be restored into another handle and continued
/// to the same result.
#[test]
fn snapshot_and_restore_roundtrip() {
    let model = line_model();
    let config = config(200.0);

    let mut first = SimulationHandle::new(&model, &config, 0).expect("handle");
    first.step_until(120.0).expect("step");
    let checkpoint = first.snapshot();
    first.step_until(200.0).expect("finish");
    let reference = first.result();

    let mut second = SimulationHandle::new(&model, &config, 0).expect("handle");
    second.restore(&checkpoint).expect("restore");
    assert_eq!(second.now(), 120.0);
    second.step_until(200.0).expect("finish");
    assert_eq!(second.result(), reference);
}

/// `step_until` refuses to move backwards or past the configured stop time.
#[test]
fn step_until_validates_its_target() {
    let model = line_model();
    let config = config(200.0);
    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");
    handle.step_until(50.0).expect("step");
    assert!(handle.step_until(25.0).is_err(), "backwards");
    assert!(handle.step_until(250.0).is_err(), "past stop_time");
}
