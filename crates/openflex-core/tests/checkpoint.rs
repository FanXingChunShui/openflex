//! Checkpoint / resume tests (B12).

use openflex_core::{
    Checkpoint, SimConfig, SimReport, simulate, simulate_with_checkpoint,
    simulate_with_checkpoint_traced,
};
use openflex_model::Model;

fn mm1_model() -> Model {
    Model::from_json(
        r#"{
      "format": "openflex-model", "version": 1, "name": "checkpoint-mm1",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 7, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "exponential", "mean": 5.0 } } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 1000 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "exponential", "mean": 4.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" }, { "from": "Q", "to": "P" }, { "from": "P", "to": "K" }
      ]
    }"#,
    )
    .expect("model")
}

#[test]
fn resume_without_json_matches_a_direct_run() {
    let model = mm1_model();
    let first = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 7,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let second = SimConfig {
        stop_time: 200.0,
        ..first
    };
    let (resumed, _) =
        simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint)).expect("resume");
    let direct = simulate(&model, &second, 0).expect("direct");
    assert_eq!(resumed, direct, "resume must reproduce the direct run");
}

#[test]
fn resume_after_json_roundtrip_matches_a_direct_run() {
    let model = mm1_model();
    let first = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 7,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let json = checkpoint.to_json().expect("json");
    let checkpoint = Checkpoint::from_json(&json).expect("load");
    let second = SimConfig {
        stop_time: 200.0,
        ..first
    };
    let (resumed, _) =
        simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint)).expect("resume");
    let direct = simulate(&model, &second, 0).expect("direct");
    assert_eq!(
        resumed, direct,
        "JSON roundtrip resume must reproduce the direct run"
    );
    let report_a = SimReport::from_replications(&model, vec![resumed]);
    let report_b = SimReport::from_replications(&model, vec![direct]);
    let json_a = serde_json::to_string(&report_a).expect("json");
    let json_b = serde_json::to_string(&report_b).expect("json");
    assert_eq!(json_a, json_b, "report JSON must be identical");
}

#[test]
fn resume_rejects_a_mismatched_seed() {
    let model = mm1_model();
    let first = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 7,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let other_seed = SimConfig {
        stop_time: 200.0,
        seed: 8,
        ..first
    };
    let error = simulate_with_checkpoint(&model, &other_seed, 0, Some(&checkpoint))
        .expect_err("seed mismatch must fail");
    assert!(error.to_string().contains("seed"), "{error}");
}

#[test]
fn resume_rejects_a_stop_time_before_the_checkpoint() {
    let model = mm1_model();
    let first = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 7,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let earlier = SimConfig {
        stop_time: 50.0,
        ..first
    };
    let error = simulate_with_checkpoint(&model, &earlier, 0, Some(&checkpoint))
        .expect_err("earlier stop time must fail");
    assert!(error.to_string().contains("cannot resume"), "{error}");
}

#[test]
fn tracing_a_resumed_run_is_rejected() {
    let model = mm1_model();
    let first = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 7,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let error = simulate_with_checkpoint_traced(&model, &first, 0, Some(&checkpoint))
        .expect_err("resume + trace must fail");
    assert!(error.to_string().contains("tracing"), "{error}");
}

fn complex_model() -> Model {
    Model::from_json(
        r#"{
      "format": "openflex-model", "version": 1, "name": "checkpoint-complex",
      "run": { "stop_time": 150.0, "warmup": 0.0, "seed": 3, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "exponential", "mean": 7.0 } } },
        { "name": "C", "kind": "conveyor", "params": {
            "speed": 2.0, "length": 10.0, "item_spacing": 1.0, "accumulating": true,
            "photo_eyes": [{ "position": 4.0, "name": "eye" }],
            "stations": [{ "position": 7.0,
                           "process_time": { "distribution": "constant", "value": 1.5 } }],
            "triggers": { "on_photo_eye": [
              { "action": "increment_label", "name": "seen", "by": "1" }
            ] } } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 100 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "exponential", "mean": 3.0 },
            "process_operator": "Dispatcher1",
            "shifts": [{ "start": 0.0, "duration": 60.0, "repeat_every": 120.0 }] } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Dispatcher1", "kind": "dispatcher", "pos": [10, 0], "params": {} },
        { "name": "T", "kind": "transporter", "pos": [10, 0], "params": {
            "capacity": 1, "speed": 5.0,
            "load_time": { "distribution": "constant", "value": 0.5 },
            "unload_time": { "distribution": "constant", "value": 0.5 },
            "home": "Dispatcher1",
            "network": [{ "from": "Dispatcher1", "to": "P", "distance": 30.0 }] } }
      ],
      "connections": [
        { "from": "S", "to": "C" },
        { "from": "C", "to": "Q" },
        { "from": "Q", "to": "P" },
        { "from": "P", "to": "K" },
        { "from": "Dispatcher1", "to": "T" }
      ]
    }"#,
    )
    .expect("model")
}

/// The snapshot must cover belt positions/stops, shift pauses, operator tasks,
/// network travel and the counter/statistics state.
#[test]
fn resume_matches_a_direct_run_for_a_complex_model() {
    let model = complex_model();
    let first = SimConfig {
        stop_time: 70.0,
        warmup: 0.0,
        seed: 3,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    assert_eq!(checkpoint.model_name(), "checkpoint-complex");
    assert!((checkpoint.now() - 70.0).abs() < 1e-9);
    assert_eq!(checkpoint.warmup(), 0.0);
    let json = checkpoint.to_json().expect("json");
    let checkpoint = Checkpoint::from_json(&json).expect("load");

    let second = SimConfig {
        stop_time: 150.0,
        ..first
    };
    let (resumed, _) =
        simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint)).expect("resume");
    let direct = simulate(&model, &second, 0).expect("direct");
    assert_eq!(
        resumed, direct,
        "complex resume must reproduce the direct run"
    );

    let report_a = SimReport::from_replications(&model, vec![resumed]);
    let report_b = SimReport::from_replications(&model, vec![direct]);
    assert_eq!(
        serde_json::to_string(&report_a).expect("json"),
        serde_json::to_string(&report_b).expect("json"),
        "report JSON must be identical"
    );
}
