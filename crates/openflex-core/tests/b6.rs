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

#[test]
fn elevator_call_order_prefers_same_direction() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "elevator-calls",
      "run": { "stop_time": 60.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S3", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 },
            "max_arrivals": 1, "first_arrival": 0.0 } },
        { "name": "S2", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 },
            "max_arrivals": 1, "first_arrival": 20.0 } },
        { "name": "Sm", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 },
            "max_arrivals": 1, "first_arrival": 20.0 } },
        { "name": "P3", "kind": "processor", "z": 3.0, "params": {
            "process_time": { "distribution": "constant", "value": 2.0 },
            "process_operator": "D1" } },
        { "name": "P2", "kind": "processor", "z": 2.0, "params": {
            "process_time": { "distribution": "constant", "value": 2.0 },
            "process_operator": "D1" } },
        { "name": "Pm", "kind": "processor", "z": -1.0, "params": {
            "process_time": { "distribution": "constant", "value": 2.0 },
            "process_operator": "D1" } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "D1", "kind": "dispatcher", "z": 0.0, "params": {} },
        { "name": "E1", "kind": "elevator", "z": 0.0, "params": {
            "capacity": 1, "speed": 1.0, "home": "D1" } }
      ],
      "connections": [
        { "from": "S3", "to": "P3" }, { "from": "P3", "to": "K" },
        { "from": "S2", "to": "P2" }, { "from": "P2", "to": "K" },
        { "from": "Sm", "to": "Pm" }, { "from": "Pm", "to": "K" },
        { "from": "D1", "to": "E1" }
      ]
    }"#;
    let model = model(json);
    let config = SimConfig {
        stop_time: 60.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("run");
    // The elevator goes up to floor 3 first and keeps direction +1; when the
    // floor 2 and floor -1 calls arrive together, the same-direction floor 2
    // wins even though floor -1 is closer.
    let names: Vec<String> = trace
        .events
        .iter()
        .filter(|event| event.event == "operator_loaded")
        .map(|event| {
            let index = event.data["requester"] as usize;
            model.objects[index].name.clone()
        })
        .collect();
    println!("call order: {names:?}");
    assert_eq!(names, vec!["P3", "P2", "Pm"]);
    assert_eq!(result.items_completed, 3);
}

#[test]
fn agv_battery_charges_after_each_task() {
    let battery_model = model(include_str!("../../../examples/objects/agv-battery.ofm"));
    let config = SimConfig {
        stop_time: 60.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&battery_model, &config, 0).expect("run");
    // The pack covers exactly one 20-unit task (pickup 0 + 10 + return 10).
    assert_eq!(result.items_completed, 2);
    let charges = event_times(&trace, "Agv1", "agv_charge");
    println!("charges: {charges:?}");
    assert_eq!(charges.len(), 2);
    assert_near(charges[0], 7.0, 1e-9, "first recharge start");

    // Without the battery the same AGV serves more items.
    // Git may check the file out with CRLF on Windows; normalise before the
    // textual replacement, otherwise the match fails and both models keep the
    // battery (making the comparison meaningless).
    let source = include_str!("../../../examples/objects/agv-battery.ofm").replace("\r\n", "\n");
    let json = source.replace(
        r#""capacity": 1, "speed": 5.0, "home": "Dispatcher1",
        "battery_capacity": 20.0, "consumption_per_distance": 1.0,
        "charge_time": { "distribution": "constant", "value": 30.0 },
        "charger": "Dispatcher1" } }"#,
        r#""capacity": 1, "speed": 5.0, "home": "Dispatcher1" } }"#,
    );
    let plain = model(&json);
    let report = run_model(&plain).expect("run");
    assert!(
        report.objects["Sink1"].input_count.mean > result.items_completed as f64,
        "battery should reduce throughput: {} vs {}",
        report.objects["Sink1"].input_count.mean,
        result.items_completed
    );
}

#[test]
fn invalid_agv_battery_params_are_rejected() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-agv",
      "objects": [
        { "name": "D", "kind": "dispatcher", "params": {} },
        { "name": "A", "kind": "agv", "params": { "battery_capacity": -1.0 } }
      ],
      "connections": [{ "from": "D", "to": "A" }]
    }"#;
    assert!(Model::from_json(json).is_err(), "negative battery");

    let unknown_charger = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-charger",
      "objects": [
        { "name": "D", "kind": "dispatcher", "params": {} },
        { "name": "A", "kind": "agv", "params": {
            "battery_capacity": 10.0, "charger": "Ghost" } }
      ],
      "connections": [{ "from": "D", "to": "A" }]
    }"#;
    let error = Model::from_json(unknown_charger).expect_err("unknown charger");
    assert!(error.to_string().contains("charger"), "{error}");
}

#[test]
fn person_grid_astar_detours_around_obstacles() {
    let grid_model = model(include_str!("../../../examples/objects/person-grid.ofm"));
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&grid_model, &config, 0).expect("grid run");
    // 8-neighbour A* detours diagonally around the wall: 4.414 distance units
    // at speed 2 (euclidean would be 2).
    let loaded = event_times(&trace, "Person1", "operator_loaded");
    assert_near(loaded[0], 4.414, 1e-3, "A* walking time");
    assert_eq!(result.items_completed, 1);

    let plain = include_str!("../../../examples/objects/person-grid.ofm")
        .replace("\"grid\"", "\"unused_grid\"");
    let error = Model::from_json(&plain).expect_err("unknown key");
    assert!(error.to_string().contains("unknown"), "{error}");
}

#[test]
fn person_congestion_slows_down_parallel_walkers() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "person-congestion",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "grid": { "width": 5, "height": 5, "cell_size": 1.0,
        "obstacles": [[2, 0], [2, 1], [2, 2], [2, 4]] },
      "objects": [
        { "name": "S1", "kind": "source", "pos": [4, 0], "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 },
            "max_arrivals": 1, "first_arrival": 0.0 } },
        { "name": "S2", "kind": "source", "pos": [4, 0], "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 },
            "max_arrivals": 1, "first_arrival": 0.0 } },
        { "name": "P1", "kind": "processor", "pos": [4, 0], "params": {
            "process_time": { "distribution": "constant", "value": 2.0 },
            "process_operator": "D1" } },
        { "name": "P2", "kind": "processor", "pos": [4, 0], "params": {
            "process_time": { "distribution": "constant", "value": 2.0 },
            "process_operator": "D1" } },
        { "name": "K1", "kind": "sink", "pos": [4, 4], "params": {} },
        { "name": "K2", "kind": "sink", "pos": [4, 4], "params": {} },
        { "name": "D1", "kind": "dispatcher", "pos": [0, 0], "params": {} },
        { "name": "Person1", "kind": "person", "pos": [0, 0], "params": {
            "speed": 2.0, "home": "D1" } },
        { "name": "Person2", "kind": "person", "pos": [0, 0], "params": {
            "speed": 2.0, "home": "D1" } }
      ],
      "connections": [
        { "from": "S1", "to": "P1" }, { "from": "P1", "to": "K1" },
        { "from": "S2", "to": "P2" }, { "from": "P2", "to": "K2" },
        { "from": "D1", "to": "Person1" },
        { "from": "D1", "to": "Person2" }
      ]
    }"#;
    let model = model(json);
    let (_, trace) = simulate_traced(
        &model,
        &SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        },
        0,
    )
    .expect("two-person run");
    // Both walkers are busy (10-cell detour / (2 / 1.1)).
    let loaded = event_times(&trace, "Person1", "operator_loaded");
    assert_near(loaded[0], 7.2831, 1e-3, "congested walking time");
}

#[test]
fn person_avoidance_uses_diagonal_moves() {
    let model = model(include_str!(
        "../../../examples/objects/person-avoidance.ofm"
    ));
    let (_, trace) = simulate_traced(
        &model,
        &SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        },
        0,
    )
    .expect("run");
    let loaded = event_times(&trace, "Walker", "operator_loaded");
    // 4 diagonal steps of sqrt(2) at speed 1.
    assert_near(
        loaded[0],
        4.0 * std::f64::consts::SQRT_2,
        1e-2,
        "diagonal walk",
    );
}

#[test]
fn agv_node_occupancy_makes_the_second_wait() {
    let model = model(include_str!("../../../examples/objects/agv-traffic.ofm"));
    let (_, trace) = simulate_traced(
        &model,
        &SimConfig {
            stop_time: 60.0,
            warmup: 0.0,
            seed: 1,
        },
        0,
    )
    .expect("run");
    let a1 = event_times(&trace, "A1", "operator_loaded");
    let a2 = event_times(&trace, "A2", "operator_loaded");
    assert_eq!(a1.len(), 1);
    assert_eq!(a2.len(), 1);
    assert!(a2[0] > a1[0] + 0.4, "second AGV should wait: {a1:?} {a2:?}");
}

#[test]
fn agv_directed_network_and_full_leg_drain() {
    let model = model(include_str!("../../../examples/objects/agv-network.ofm"));
    let (_, trace) = simulate_traced(
        &model,
        &SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        },
        0,
    )
    .expect("run");
    let loaded = event_times(&trace, "A", "operator_loaded");
    let home = event_times(&trace, "A", "transport_home");
    // Outbound directed edge 10 / speed 5 = 2; return directed edge 30 / 5 = 6.
    assert_near(loaded[0], 2.0, 1e-9, "directed outbound");
    assert_near(home[0], 9.0, 1e-9, "directed return");
}
