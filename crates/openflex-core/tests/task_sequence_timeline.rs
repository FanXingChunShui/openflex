//! task-80: safety net for the dispatcher task sequence timeline.
//!
//! The current implementation runs the extra task-sequence legs at the end of a
//! task (`settle_task`). Before moving them between `load` and `unload`, these
//! tests pin down:
//!
//! * a task sequence that contains only the standard legs behaves exactly like
//!   no `task_sequence` at all (bit for bit),
//! * the current numbers for a model with extra legs (event counts and
//!   statistics), so the later change can be compared against them.

use openflex_core::SimConfig;
use openflex_core::{SimulationHandle, simulate};
use openflex_model::Model;

fn model(sequence: &str) -> Model {
    Model::from_json(&format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "task-timeline",
      "run": {{ "stop_time": 300.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "pos": [10, 0], "params": {{
            "interarrival": {{ "distribution": "constant", "value": 20.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "P", "kind": "processor", "pos": [10, 0], "params": {{
            "process_time": {{ "distribution": "constant", "value": 4.0 }},
            "process_operator": "Dispatcher1" }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }},
        {{ "name": "Dispatcher1", "kind": "dispatcher", "pos": [0, 0], "params": {{ {sequence} }} }},
        {{ "name": "E", "kind": "transporter", "pos": [0, 0], "params": {{
            "capacity": 1, "speed": 1.0, "home": "Dispatcher1" }} }}
      ],
      "connections": [
        {{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }},
        {{ "from": "Dispatcher1", "to": "E" }}
      ]
    }}"#
    ))
    .expect("model")
}

fn config() -> SimConfig {
    SimConfig {
        stop_time: 300.0,
        warmup: 0.0,
        seed: 1,
    }
}

/// A standard-leg sequence must not change anything: it behaves exactly like a
/// dispatcher without a `task_sequence`.
#[test]
fn standard_legs_match_no_task_sequence() {
    let plain = simulate(&model(""), &config(), 0).expect("plain");
    let standard = simulate(
        &model(r#""task_sequence": ["travel", "travel", "load", "unload"]"#),
        &config(),
        0,
    )
    .expect("standard");
    assert_eq!(standard, plain, "standard legs must be a no-op");
}

/// Snapshot of the current behaviour with two extra legs (10 s travel and 5 s
/// load). The numbers are the pre-change reference for task-80 step 2.
#[test]
fn extra_legs_current_behaviour_snapshot() {
    let model = model(
        r#""task_sequence": ["travel", "travel", "load", "unload",
            { "kind": "travel", "duration": { "distribution": "constant", "value": 10.0 } },
            { "kind": "load", "duration": { "distribution": "constant", "value": 5.0 } }]"#,
    );
    let result = simulate(&model, &config(), 0).expect("run");
    let processor = &result.objects["P"];
    // Snapshot: the extra 15 s per task limit the throughput of this model.
    assert_eq!(processor.output_count, 8, "processor output count");
    assert_eq!(result.items_completed, 8, "completed items");
    assert!(
        (processor.avg_stay_time - 35.125).abs() < 1e-9,
        "processor stay time: {}",
        processor.avg_stay_time
    );

    // The two extra legs are visible as individual events.
    let mut handle = SimulationHandle::new(&model, &config(), 0).expect("handle");
    let mut step_events = 0usize;
    while let Some(preview) = handle.next_event() {
        if preview.kind == "task_step_done" {
            step_events += 1;
        }
        handle.step_event().expect("step");
    }
    // 8 completed tasks x 2 legs, minus the legs of the last task that run
    // past the stop time.
    assert_eq!(step_events, 14, "task_step_done events");
    handle.step_until(300.0).expect("advance to stop");
    assert_eq!(handle.result(), result, "stepping must not change the run");
}
