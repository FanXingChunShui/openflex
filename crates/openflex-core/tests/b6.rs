//! Elevator / Person / AGV tests (B6).

use openflex_core::{SimConfig, TraceLog, run_model, simulate_traced};
use openflex_model::Model;

fn model(text: &str) -> Model {
    Model::from_json(text).expect("model")
}

fn event_times(trace: &TraceLog, object: &str, label: &str) -> Vec<f64> {
    trace
        .object(object)
        .filter(|event| event.event == label)
        .map(|event| event.time)
        .collect()
}

fn assert_near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected} ± {tolerance}, got {actual}"
    );
}

#[test]
fn elevator_travels_between_floors_with_dwell() {
    let model = model(include_str!("../../../examples/objects/elevator.ofm"));
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("run");
    // Floor 0 -> floor 3 at speed 1, 1 unit dwell at pickup and at delivery:
    // pickup@0, arrive@4, operator ready@5, processing 5..7, back home@10.
    assert_eq!(
        event_times(&trace, "Elevator1", "transport_pickup"),
        vec![0.0, 20.0, 40.0, 60.0, 80.0]
    );
    let arrive = event_times(&trace, "Elevator1", "transport_arrive");
    assert_near(arrive[0], 4.0, 1e-9, "first arrival");
    let loaded = event_times(&trace, "Elevator1", "operator_loaded");
    assert_near(loaded[0], 5.0, 1e-9, "first operator delivery");
    let home = event_times(&trace, "Elevator1", "transport_home");
    assert_near(home[0], 10.0, 1e-9, "first return home");

    assert_eq!(result.items_completed, 5);
    // The elevator is busy 10 time units per task (5 tasks / 100 units).
    assert_near(
        result.objects["Elevator1"].state_times.busy,
        50.0,
        1e-9,
        "elevator busy time",
    );
    assert_near(
        result.objects["Elevator1"].utilization,
        0.5,
        1e-9,
        "elevator utilization",
    );
}

fn elevator_capacity_model(capacity: u32) -> Model {
    model(&format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "elevator-capacity",
      "run": {{ "stop_time": 60.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S1", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }},
            "max_arrivals": 6, "first_arrival": 0.0 }} }},
        {{ "name": "S2", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }},
            "max_arrivals": 6, "first_arrival": 5.0 }} }},
        {{ "name": "P1", "kind": "processor", "z": 3.0, "params": {{
            "process_time": {{ "distribution": "constant", "value": 2.0 }},
            "process_operator": "D1" }} }},
        {{ "name": "P2", "kind": "processor", "z": 3.0, "params": {{
            "process_time": {{ "distribution": "constant", "value": 2.0 }},
            "process_operator": "D1" }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }},
        {{ "name": "D1", "kind": "dispatcher", "z": 0.0, "params": {{}} }},
        {{ "name": "E1", "kind": "elevator", "z": 0.0, "params": {{
            "capacity": {capacity}, "speed": 1.0,
            "stop_time": {{ "distribution": "constant", "value": 1.0 }},
            "home": "D1" }} }}
      ],
      "connections": [
        {{ "from": "S1", "to": "P1" }}, {{ "from": "P1", "to": "K" }},
        {{ "from": "S2", "to": "P2" }}, {{ "from": "P2", "to": "K" }},
        {{ "from": "D1", "to": "E1" }}
      ]
    }}"#
    ))
}

#[test]
fn elevator_capacity_serves_more_tasks() {
    let single = run_model(&elevator_capacity_model(1)).expect("run");
    let double = run_model(&elevator_capacity_model(2)).expect("run");
    assert!(
        double.objects["K"].input_count.mean > single.objects["K"].input_count.mean,
        "capacity 2: {} vs capacity 1: {}",
        double.objects["K"].input_count.mean,
        single.objects["K"].input_count.mean
    );
}

#[test]
fn person_walks_and_serves_operator_tasks() {
    let model = model(include_str!("../../../examples/objects/person.ofm"));
    let config = SimConfig {
        stop_time: 60.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("run");
    // Load 0.5s + unload 0.5s with no travel distance.
    let loaded = event_times(&trace, "Person1", "operator_loaded");
    assert_near(loaded[0], 1.0, 1e-9, "first operator delivery");
    assert_eq!(result.items_completed, 6);
    assert!(result.objects["Person1"].state_times.busy > 0.0);
}

#[test]
fn agv_alias_uses_transporter_timing() {
    let model = model(include_str!("../../../examples/objects/agv.ofm"));
    let config = SimConfig {
        stop_time: 50.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("run");
    // 10 distance units at speed 5 -> 2 time units per task leg.
    let loaded = event_times(&trace, "Agv1", "operator_loaded");
    assert_near(loaded[0], 2.0, 1e-9, "first operator delivery");
    assert_eq!(result.items_completed, 3);
}

#[test]
fn invalid_executor_params_are_rejected() {
    // A person always has capacity 1.
    let person = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-person",
      "objects": [
        { "name": "S", "kind": "source", "params": {} },
        { "name": "P", "kind": "person", "params": { "capacity": 2 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "K" }]
    }"#;
    let error = Model::from_json(person).expect_err("person capacity");
    assert!(error.to_string().contains("capacity 1"), "{error}");

    // An elevator needs a positive speed.
    let elevator = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-elevator",
      "objects": [
        { "name": "D", "kind": "dispatcher", "params": {} },
        { "name": "E", "kind": "elevator", "params": { "speed": 0.0 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "D", "to": "E" }]
    }"#;
    let error = Model::from_json(elevator).expect_err("elevator speed");
    assert!(error.to_string().contains("speed"), "{error}");
}
