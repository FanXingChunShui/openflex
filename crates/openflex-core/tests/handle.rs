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

// ---------------------------------------------------------------------------
// task-50: event-by-event stepping
// ---------------------------------------------------------------------------

/// Stepping event by event and then advancing to the stop time equals a single
/// run.
#[test]
fn step_event_walks_the_whole_run() {
    let model = line_model();
    let config = config(200.0);
    let direct = simulate(&model, &config, 0).expect("direct run");

    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");
    let mut events = 0usize;
    while handle.step_event().expect("step").is_some() {
        events += 1;
        assert!(events < 100_000, "event loop must terminate");
    }
    assert!(events > 0, "the model must have events");
    handle.step_until(200.0).expect("advance to stop");
    assert_eq!(handle.result(), direct);
}

/// `next_event` previews the head of the calendar and the preview returned by
/// `step_event` matches it; previews never move backwards.
#[test]
fn next_event_previews_the_calendar() {
    let model = line_model();
    let config = config(200.0);
    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");

    let first = handle.next_event().expect("first event");
    assert_eq!(
        first.object.as_deref(),
        Some("S"),
        "the first event is an arrival"
    );
    assert_eq!(first.kind, "arrive");
    assert_eq!(first.time, 0.0);

    let mut previous = (f64::NEG_INFINITY, 0u64);
    let mut seen = 0usize;
    while let Some(preview) = handle.next_event() {
        assert!(
            (preview.time, preview.seq) > previous,
            "preview order must follow (time, seq)"
        );
        previous = (preview.time, preview.seq);
        let handled = handle.step_event().expect("step").expect("event");
        assert_eq!(handled, preview, "the step must handle the previewed event");
        seen += 1;
        if seen >= 50 {
            break;
        }
    }
    assert!(seen > 0);
}

/// After the horizon is reached `next_event` and `step_event` both report
/// "nothing to do".
#[test]
fn stepping_stops_at_the_horizon() {
    let model = line_model();
    let config = config(100.0);
    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");
    handle.step_until(100.0).expect("step");
    assert!(
        handle.next_event().is_none(),
        "no event left before stop_time"
    );
    assert!(handle.step_event().expect("step").is_none());
}

/// A complex model (setup, blocked time, queueing) must produce the same
/// time-weighted statistics whether it is stepped event by event or run once.
#[test]
fn step_event_matches_a_single_run_on_a_complex_model() {
    let model = model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "handle-complex",
      "run": { "stop_time": 300.0, "warmup": 0.0, "seed": 3, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "exponential", "mean": 4.0 } } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 1000 } },
        { "name": "P", "kind": "processor", "params": {
            "setup_time": { "distribution": "constant", "value": 2.0 },
            "process_time": { "distribution": "exponential", "mean": 5.0 },
            "failure": {
              "mtbf": { "distribution": "constant", "value": 60.0 },
              "mttr": { "distribution": "constant", "value": 10.0 } } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" }, { "from": "Q", "to": "P" }, { "from": "P", "to": "K" }
      ]
    }"#,
    );
    let config = config(300.0);
    let direct = simulate(&model, &config, 0).expect("direct run");

    let mut handle = SimulationHandle::new(&model, &config, 0).expect("handle");
    while handle.step_event().expect("step").is_some() {
        // Reading the state must not disturb the run (the studio does this on
        // every frame).
        let _ = handle.state();
    }
    handle.step_until(300.0).expect("advance to stop");
    let stepped = handle.result();
    assert_eq!(stepped.items_created, direct.items_created);
    assert_eq!(stepped.items_completed, direct.items_completed);
    for (name, expected) in &direct.objects {
        let actual = &stepped.objects[name];
        assert!(
            (actual.utilization - expected.utilization).abs() < 1e-9,
            "{name} utilization: {} vs {}",
            actual.utilization,
            expected.utilization
        );
        assert!(
            (actual.avg_content - expected.avg_content).abs() < 1e-9,
            "{name} avg_content: {} vs {}",
            actual.avg_content,
            expected.avg_content
        );
        assert!(
            (actual.state_times.blocked - expected.state_times.blocked).abs() < 1e-9,
            "{name} blocked: {} vs {}",
            actual.state_times.blocked,
            expected.state_times.blocked
        );
    }
    assert_eq!(stepped, direct, "the full result must match");
}
