//! task-44: controlled runs (`on_stop` / `on_resume`).
//!
//! [`simulate_until`] pauses at an exact time and fires `on_stop`; the returned
//! checkpoint can be resumed with [`resume_from_checkpoint`], which fires
//! `on_resume` before the run continues.

use openflex_core::{SimConfig, resume_from_checkpoint, run_model, simulate, simulate_until};
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

/// A queue whose `on_stop` releases the item held by the `hold` output.
fn hold_model(trigger: &str) -> String {
    format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "controlled",
      "run": {{ "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 20.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "Q", "kind": "queue", "params": {{
            "max_content": 100,
            "triggers": {{
              "{trigger}": [ {{ "action": "send_message", "target": "Q", "delay": "0" }} ],
              "on_message": [ {{ "action": "release_item" }} ] }} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [
        {{ "from": "S", "to": "Q" }},
        {{ "from": "Q", "to": "K", "strategy": {{ "type": "hold" }} }}
      ]
    }}"#
    )
}

/// `simulate_until` stops exactly at `stop_at` and fires `on_stop`.
#[test]
fn on_stop_fires_at_the_pause_point() {
    let model = model(&hold_model("on_stop"));
    let (paused, checkpoint) = simulate_until(&model, &config(200.0), 0, 50.0).expect("run");
    assert_eq!(paused.final_time, 50.0);
    // The item that arrived at t = 0 was held until the pause point released it.
    assert_eq!(
        paused.objects["K"].input_count, 1,
        "on_stop must release the held item"
    );
    assert_eq!(checkpoint.now, 50.0);
}

/// The same run without an `on_stop` action produces no delivery at the pause
/// point.
#[test]
fn without_on_stop_nothing_happens_at_the_pause_point() {
    let model = model(&hold_model("on_message"));
    let (paused, _) = simulate_until(&model, &config(200.0), 0, 50.0).expect("run");
    assert_eq!(paused.objects["K"].input_count, 0);
}

/// `resume_from_checkpoint` fires `on_resume` and continues from the same
/// state; without side effects the result equals a single run.
#[test]
fn resume_matches_a_single_run() {
    let source = r#"{
      "format": "openflex-model", "version": 1, "name": "resume",
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
    }"#;
    let model = model(source);
    let config = config(200.0);
    let (_, checkpoint) = simulate_until(&model, &config, 0, 120.0).expect("first segment");
    let resumed = resume_from_checkpoint(&model, &config, 0, &checkpoint, 200.0).expect("resume");
    let direct = simulate(&model, &config, 0).expect("direct");
    assert_eq!(resumed, direct, "pause/resume must not change the run");
}

/// `on_resume` runs before the continuation, so a held item is released by the
/// resume itself.
#[test]
fn on_resume_fires_when_the_run_continues() {
    let model = model(&hold_model("on_resume"));
    let config = config(200.0);
    let (paused, checkpoint) = simulate_until(&model, &config, 0, 50.0).expect("first segment");
    // `on_stop` is not defined, so nothing was released yet.
    assert_eq!(paused.objects["K"].input_count, 0);
    let resumed = resume_from_checkpoint(&model, &config, 0, &checkpoint, 100.0).expect("resume");
    assert_eq!(
        resumed.objects["K"].input_count, 1,
        "on_resume must release one held item"
    );
}

/// The regular (checkpoint) API keeps its old behaviour: no `on_stop` /
/// `on_resume` triggers.
#[test]
fn plain_checkpoint_api_does_not_fire_run_triggers() {
    let model = model(&hold_model("on_stop"));
    let report = run_model(&model).expect("run");
    assert_eq!(
        report.objects["K"].input_count.mean, 0.0,
        "a normal run never fires on_stop"
    );
}
