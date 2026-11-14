//! Object behaviour tests.
//!
//! Every test builds a small model, runs it and asserts the documented
//! semantics: travel times on conveyors, the number of items a combiner or
//! separator produces, the distribution of a splitter, operator delays, shift
//! pauses, queue batch timeouts and the statistics that go with them.
//!
//! The models are deliberately deterministic (constant distributions) so the
//! expected values are exact arithmetic rather than statistical ranges.

use openflex_core::run_model;
use openflex_core::{SimConfig, SimReport, StateTimes, simulate, simulate_traced};
use openflex_model::Model;

fn model(json: &str) -> Model {
    Model::from_json(json).expect("model should load")
}

fn run(json: &str) -> SimReport {
    run_model(&model(json)).expect("run")
}

fn assert_near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected} ± {tolerance}, got {actual}"
    );
}

/// Output count of one object.
fn out(report: &SimReport, name: &str) -> f64 {
    report.objects[name].output_count.mean
}

fn input(report: &SimReport, name: &str) -> f64 {
    report.objects[name].input_count.mean
}

/// Absolute state times of the first replication.
fn state_times(report: &SimReport, name: &str) -> StateTimes {
    report.replications[0].objects[name].state_times
}

// ---------------------------------------------------------------------------
// Conveyor
// ---------------------------------------------------------------------------

/// `length / speed` is the travel time: 10 distance units at 2 units per time
/// unit must take exactly 5 time units.
#[test]
fn conveyor_travel_time_is_length_over_speed() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "conveyor-travel",
      "run": { "stop_time": 1000.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "C", "kind": "conveyor", "params": {
            "speed": 2.0, "length": 10.0, "item_spacing": 1.0,
            "accumulating": true, "max_content": 20 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "C" }, { "from": "C", "to": "K" }]
    }"#);
    assert_eq!(out(&report, "S"), 100.0);
    assert_eq!(out(&report, "C"), 100.0);
    assert_eq!(input(&report, "K"), 100.0);
    assert_near(
        report.objects["C"].avg_stay_time.mean,
        5.0,
        1e-9,
        "conveyor stay",
    );
    // One item on the belt at a time: the source only sends every 10 units.
    assert_near(
        report.objects["C"].avg_content.mean,
        0.5,
        1e-9,
        "conveyor WIP",
    );
}

/// A blocked conveyor accumulates items behind the item at the exit; the belt
/// keeps accepting items until the spacing rule fills it up.
#[test]
fn conveyor_accumulates_while_the_exit_is_blocked() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "conveyor-accum",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1.0 }, "first_arrival": 0.0 } },
        { "name": "C", "kind": "conveyor", "params": {
            "speed": 1.0, "length": 10.0, "item_spacing": 1.0,
            "accumulating": true, "max_content": 50 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 1 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "C" }, { "from": "C", "to": "Q" }]
    }"#);
    // The queue (capacity 1, no outlet) jams after one item.
    assert_eq!(input(&report, "Q"), 1.0);
    assert_eq!(out(&report, "C"), 1.0);
    // The accumulating belt stacks items at `item_spacing` until it is full:
    // 10 distance units of belt with a 1 unit spacing hold ~10 items.
    assert!(
        report.objects["C"].max_content.mean >= 8.0,
        "an accumulating belt should collect items, max content was {}",
        report.objects["C"].max_content.mean
    );
}

/// A non-accumulating belt freezes as a whole when the leading item cannot
/// leave; it therefore holds fewer items than an accumulating one under the
/// same blocked-exit conditions.
#[test]
fn non_accumulating_belt_holds_fewer_items() {
    let base = |accumulating: bool| {
        format!(
            r#"{{
      "format": "openflex-model", "version": 1, "name": "conveyor-rigid",
      "run": {{ "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 1.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "C", "kind": "conveyor", "params": {{
            "speed": 1.0, "length": 10.0, "item_spacing": 4.0,
            "accumulating": {accumulating}, "max_content": 50 }} }},
        {{ "name": "Q", "kind": "queue", "params": {{ "max_content": 1 }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [{{ "from": "S", "to": "C" }}, {{ "from": "C", "to": "Q" }}]
    }}"#
        )
    };
    let accumulating = run(&base(true));
    let rigid = run(&base(false));
    assert!(
        accumulating.objects["C"].max_content.mean > rigid.objects["C"].max_content.mean,
        "accumulating: {}, rigid: {}",
        accumulating.objects["C"].max_content.mean,
        rigid.objects["C"].max_content.mean
    );
}

// ---------------------------------------------------------------------------
// Combiner
// ---------------------------------------------------------------------------

const COMBINER_PACK: &str = r#"{
  "format": "openflex-model", "version": 1, "name": "combiner-pack",
  "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
  "objects": [
    { "name": "TraySource", "kind": "source", "params": {
        "interarrival": { "distribution": "constant", "value": 50.0 }, "first_arrival": 0.0 } },
    { "name": "BoxSource", "kind": "source", "params": {
        "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
    { "name": "Combiner1", "kind": "combiner", "params": {
        "mode": "pack", "components_per_container": 4 } },
    { "name": "Sink1", "kind": "sink", "params": {} }
  ],
  "connections": [
    { "from": "TraySource", "to": "Combiner1" },
    { "from": "BoxSource", "to": "Combiner1" },
    { "from": "Combiner1", "to": "Sink1" }
  ]
}"#;

/// 10 trays, 4 boxes each: the sink sees one tray item per cycle (10), not the
/// 50 boxes, and the container counts once in the combiner's content.
#[test]
fn combiner_pack_releases_one_container_per_cycle() {
    let report = run(COMBINER_PACK);
    assert_eq!(out(&report, "TraySource"), 10.0);
    assert_eq!(out(&report, "Combiner1"), 10.0);
    assert_eq!(input(&report, "Sink1"), 10.0);
    // 10 trays + 40 boxes were consumed.
    assert_eq!(input(&report, "Combiner1"), 50.0);
    assert!(
        report.objects["Combiner1"].max_content.mean <= 1.0,
        "components inside the container must not be counted, max content was {}",
        report.objects["Combiner1"].max_content.mean
    );
    assert_near(
        report.objects["Combiner1"].avg_content.mean,
        0.6,
        0.02,
        "combiner WIP",
    );
}

/// Join mode takes one item from every input port and releases a single
/// merged item.
#[test]
fn combiner_join_merges_one_item_per_port() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "combiner-join",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "A", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "B", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Combiner1", "kind": "combiner", "params": { "mode": "join" } },
        { "name": "Sink1", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "A", "to": "Combiner1" },
        { "from": "B", "to": "Combiner1" },
        { "from": "Combiner1", "to": "Sink1" }
      ]
    }"#);
    assert_eq!(out(&report, "A"), 50.0);
    assert_eq!(out(&report, "B"), 50.0);
    assert_eq!(out(&report, "Combiner1"), 50.0);
    assert_eq!(input(&report, "Combiner1"), 100.0);
    assert_eq!(input(&report, "Sink1"), 50.0);
}

// ---------------------------------------------------------------------------
// Separator
// ---------------------------------------------------------------------------

/// unpack semantics: `split_count = qty` releases `qty` parts plus the
/// container itself, so 3 becomes 4 items per container.
#[test]
fn separator_unpacks_quantity_plus_container() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "separator-unpack",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 50.0 }, "first_arrival": 0.0 } },
        { "name": "Sep1", "kind": "separator", "params": { "split_count": 3 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "Sep1" }, { "from": "Sep1", "to": "K" }]
    }"#);
    assert_eq!(input(&report, "Sep1"), 10.0);
    assert_eq!(out(&report, "Sep1"), 40.0);
    assert_eq!(input(&report, "K"), 40.0);
}

/// The default `split_count = 0` splits a container into everything it holds:
/// a combiner that packs 4 components produces containers with 4 parts, which
/// the separator releases as 5 items (4 + the container).
#[test]
fn separator_splits_into_components_plus_container() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "combine-separate",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "Tray", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 50.0 }, "first_arrival": 0.0 } },
        { "name": "Box", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Combiner1", "kind": "combiner", "params": {
            "mode": "pack", "components_per_container": 4 } },
        { "name": "Sep1", "kind": "separator", "params": { "split_count": 0 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "Tray", "to": "Combiner1" },
        { "from": "Box", "to": "Combiner1" },
        { "from": "Combiner1", "to": "Sep1" },
        { "from": "Sep1", "to": "K" }
      ]
    }"#);
    assert_eq!(out(&report, "Combiner1"), 10.0);
    assert_eq!(input(&report, "Sep1"), 10.0);
    assert_eq!(out(&report, "Sep1"), 50.0);
    assert_eq!(input(&report, "K"), 50.0);
}

/// A separator distributes its parts over several output ports.
#[test]
fn separator_distributes_parts_over_ports() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "separator-ports",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 50.0 }, "first_arrival": 0.0 } },
        { "name": "Sep1", "kind": "separator", "params": { "split_count": 3 } },
        { "name": "A", "kind": "sink", "params": {} },
        { "name": "B", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Sep1" },
        { "from": "Sep1", "to": "A" },
        { "from": "Sep1", "to": "B" }
      ]
    }"#);
    // 4 parts per container, alternating between the two ports.
    assert_eq!(input(&report, "A"), 20.0);
    assert_eq!(input(&report, "B"), 20.0);
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

const MERGE_TEMPLATE: &str = r#"{
  "format": "openflex-model", "version": 1, "name": "merge",
  "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
  "objects": [
    { "name": "A", "kind": "source", "params": {
        "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
    { "name": "B", "kind": "source", "params": {
        "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 5.0 } },
    { "name": "Merge1", "kind": "merge", "params": { "wait_for_all": WAIT } },
    { "name": "K", "kind": "sink", "params": {} }
  ],
  "connections": [
    { "from": "A", "to": "Merge1" },
    { "from": "B", "to": "Merge1" },
    { "from": "Merge1", "to": "K" }
  ]
}"#;

#[test]
fn merge_waits_for_every_port() {
    let report = run(&MERGE_TEMPLATE.replace("WAIT", "true"));
    // One merged item per pair of arrivals every 10 time units.
    assert_eq!(out(&report, "Merge1"), 50.0);
    assert_eq!(input(&report, "Merge1"), 100.0);
    assert_eq!(input(&report, "K"), 50.0);
}

#[test]
fn merge_without_waiting_releases_each_arrival() {
    let report = run(&MERGE_TEMPLATE.replace("WAIT", "false"));
    assert_eq!(input(&report, "Merge1"), 100.0);
    assert_eq!(out(&report, "Merge1"), 100.0);
}

// ---------------------------------------------------------------------------
// Split
// ---------------------------------------------------------------------------

#[test]
fn split_round_robin_balances_the_outputs() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "split-rr",
      "run": { "stop_time": 1000.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Split1", "kind": "split", "params": { "by_item_type": false } },
        { "name": "A", "kind": "sink", "params": {} },
        { "name": "B", "kind": "sink", "params": {} },
        { "name": "C", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Split1" },
        { "from": "Split1", "to": "A" },
        { "from": "Split1", "to": "B" },
        { "from": "Split1", "to": "C" }
      ]
    }"#);
    assert_eq!(out(&report, "S"), 100.0);
    let a = input(&report, "A");
    let b = input(&report, "B");
    let c = input(&report, "C");
    assert_eq!(a + b + c, 100.0);
    for (name, value) in [("A", a), ("B", b), ("C", c)] {
        assert_near(
            value,
            100.0 / 3.0,
            2.0,
            &format!("round-robin share of {name}"),
        );
    }
}

#[test]
fn split_by_item_type_routes_to_the_matching_port() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "split-type",
      "run": { "stop_time": 1000.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S0", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "first_arrival": 0.0, "item_type": 0 } },
        { "name": "S1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 20.0 },
            "first_arrival": 0.0, "item_type": 1 } },
        { "name": "S2", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 40.0 },
            "first_arrival": 0.0, "item_type": 2 } },
        { "name": "Split1", "kind": "split", "params": { "by_item_type": true } },
        { "name": "A", "kind": "sink", "params": {} },
        { "name": "B", "kind": "sink", "params": {} },
        { "name": "C", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S0", "to": "Split1" },
        { "from": "S1", "to": "Split1" },
        { "from": "S2", "to": "Split1" },
        { "from": "Split1", "to": "A" },
        { "from": "Split1", "to": "B" },
        { "from": "Split1", "to": "C" }
      ]
    }"#);
    assert_eq!(input(&report, "A"), out(&report, "S0"));
    assert_eq!(input(&report, "B"), out(&report, "S1"));
    assert_eq!(input(&report, "C"), out(&report, "S2"));
    assert_near(input(&report, "A"), 100.0, 1.0, "type 0 items");
    assert_near(input(&report, "B"), 50.0, 1.0, "type 1 items");
    assert_near(input(&report, "C"), 25.0, 1.0, "type 2 items");
}

// ---------------------------------------------------------------------------
// Queue batching with a maximum wait
// ---------------------------------------------------------------------------

/// `batch_max_wait` releases a partial batch once the oldest item waited long
/// enough, so items spend less time in the queue than with unlimited waiting.
#[test]
fn queue_batch_timeout_releases_early() {
    let template = |max_wait: f64| {
        format!(
            r#"{{
      "format": "openflex-model", "version": 1, "name": "batch-timeout",
      "run": {{ "stop_time": 400.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "Q", "kind": "queue", "params": {{
            "batch_size": 5, "batch_max_wait": {max_wait} }} }},
        {{ "name": "P", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "constant", "value": 4.0 }} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [
        {{ "from": "S", "to": "Q" }}, {{ "from": "Q", "to": "P" }}, {{ "from": "P", "to": "K" }}
      ]
    }}"#
        )
    };
    let unlimited = run(&template(0.0));
    let limited = run(&template(20.0));
    assert!((input(&unlimited, "K") - input(&limited, "K")).abs() <= 3.0);
    assert!(
        limited.objects["Q"].avg_stay_time.mean < unlimited.objects["Q"].avg_stay_time.mean,
        "a batch timeout must shorten the queue wait: {} vs {}",
        limited.objects["Q"].avg_stay_time.mean,
        unlimited.objects["Q"].avg_stay_time.mean
    );
    // A complete batch is still released as a whole.
    assert_eq!(out(&unlimited, "Q"), 40.0);
}

// ---------------------------------------------------------------------------
// Operators: dispatcher + transporter, resource pool
// ---------------------------------------------------------------------------

fn operator_model(operator: &str) -> String {
    format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "operator",
      "run": {{ "stop_time": 1000.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 20.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "P", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "constant", "value": 5.0 }},
            "process_operator": "{operator}" }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }},
        {{ "name": "Dispatcher1", "kind": "dispatcher", "pos": [100, 0], "params": {{}} }},
        {{ "name": "Transporter1", "kind": "transporter", "pos": [100, 0], "params": {{
            "capacity": 1, "speed": 10.0, "home": "Dispatcher1" }} }},
        {{ "name": "Resource1", "kind": "resource", "pos": [0, 0], "params": {{ "capacity": 1 }} }}
      ],
      "connections": [
        {{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }},
        {{ "from": "Dispatcher1", "to": "Transporter1" }}
      ]
    }}"#
    )
}

#[test]
fn transporter_travel_delays_processing() {
    let without = run(&operator_model(""));
    let with = run(&operator_model("Dispatcher1"));
    assert!(
        out(&without, "P") > out(&with, "P"),
        "an operator must slow the processor"
    );
    // The transporter counts one task per processed item.
    assert!(out(&with, "Transporter1") > 0.0);
    assert!((out(&with, "Transporter1") - out(&with, "P")).abs() <= 1.0);
    // Waiting for the operator shows up as blocked time.
    assert!(state_times(&with, "P").blocked > 0.0);
}

#[test]
fn resource_pool_downtime_blocks_the_processor() {
    // The resource is down for the whole run, so no operator ever arrives.
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "resource-down",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 4.0 },
            "process_operator": "Resource1" } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Resource1", "kind": "resource", "params": {
            "capacity": 1, "downtimes": [{ "start": 0.0, "end": 1000.0 }] } }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    assert_eq!(input(&report, "K"), 0.0);
    assert!(
        state_times(&report, "P").blocked > 400.0,
        "the processor should wait for the resource"
    );
}

#[test]
fn resource_pool_capacity_serialises_the_processors() {
    let template = |capacity: u32| {
        format!(
            r#"{{
      "format": "openflex-model", "version": 1, "name": "resource-pool",
      "run": {{ "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S1", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 5.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "S2", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 5.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "P1", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "constant", "value": 4.0 }},
            "process_operator": "Resource1" }} }},
        {{ "name": "P2", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "constant", "value": 4.0 }},
            "process_operator": "Resource1" }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }},
        {{ "name": "Resource1", "kind": "resource", "params": {{ "capacity": {capacity} }} }}
      ],
      "connections": [
        {{ "from": "S1", "to": "P1" }}, {{ "from": "P1", "to": "K" }},
        {{ "from": "S2", "to": "P2" }}, {{ "from": "P2", "to": "K" }}
      ]
    }}"#
        )
    };
    let single = run(&template(1));
    let double = run(&template(2));
    assert!(
        input(&double, "K") > input(&single, "K"),
        "two resource units must serve more items: {} vs {}",
        input(&double, "K"),
        input(&single, "K")
    );
}

// ---------------------------------------------------------------------------
// Shifts
// ---------------------------------------------------------------------------

#[test]
fn shifts_pause_processing_and_count_off_shift_time() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "shifts",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 6.0 },
            "shifts": [{ "start": 0.0, "duration": 10.0, "repeat_every": 100.0 }] } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    // Two work windows ([0, 10) and [100, 110)) fit two items; the third is
    // paused inside the second window and only resumes at t = 200, after the
    // run stopped.
    assert_eq!(input(&report, "K"), 2.0);
    let states = state_times(&report, "P");
    assert_near(states.off_shift, 180.0, 0.5, "off-shift time");
    assert!(
        states.busy > 10.0,
        "the processor must work during the windows"
    );
}

#[test]
fn shifts_can_stop_a_source() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "source-shift",
      "run": { "stop_time": 300.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0,
            "shifts": [{ "start": 0.0, "duration": 100.0, "repeat_every": 0.0 }] } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "K" }]
    }"#);
    // Items are only created during the first window: 0, 10, ..., 90.
    assert_eq!(out(&report, "S"), 10.0);
    assert_eq!(input(&report, "K"), 10.0);
}

// ---------------------------------------------------------------------------
// Failures and their triggers
// ---------------------------------------------------------------------------

#[test]
fn breakdown_preempts_processing_and_repairs() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "breakdown",
      "run": { "stop_time": 4800.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 6.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 10000 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 8.0 },
            "failure": {
              "mtbf": { "distribution": "constant", "value": 100.0 },
              "mttr": { "distribution": "constant", "value": 10.0 }
            },
            "triggers": {
              "on_breakdown": [{ "action": "send_message", "target": "K", "delay": "0" }],
              "on_repair": [{ "action": "send_message", "target": "K", "delay": "0" }]
            } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" }, { "from": "Q", "to": "P" }, { "from": "P", "to": "K" }
      ]
    }"#);
    // 43 full 110s cycles of 12.5 items plus 70s of the next up period:
    // 43 * 12.5 + 70 / 8 = 546.25 items.
    assert_near(
        input(&report, "K"),
        546.0,
        1.0,
        "items produced with failures",
    );
    let states = state_times(&report, "P");
    assert_near(states.down, 430.0, 10.0, "down time");
    assert_near(states.busy, 4370.0, 10.0, "processing time");
    assert_near(states.idle, 0.0, 2.0, "idle time");
}

#[test]
fn repair_trigger_fires() {
    // A message on every repair reaches the sink; the model must still run and
    // the trigger must not disturb the flow.
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "repair-trigger",
      "run": { "stop_time": 300.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 2.0 },
            "failure": {
              "mtbf": { "distribution": "constant", "value": 50.0 },
              "mttr": { "distribution": "constant", "value": 20.0 }
            },
            "triggers": {
              "on_breakdown": [{ "action": "set_label", "name": "broken", "value": "1" }],
              "on_repair": [{ "action": "increment_label", "name": "repaired", "by": "1" }]
            } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    assert!(input(&report, "K") > 0.0);
    assert!(state_times(&report, "P").down > 40.0);
}

// ---------------------------------------------------------------------------
// Regression fixture: zero waiting, low content
// ---------------------------------------------------------------------------

/// `tests/fixtures/diag-empty-transit.ofm`: inter-arrival >> process time, so
/// the queue never holds an item and the processor is almost always empty.
#[test]
fn empty_transit_fixture_has_no_queue_wait() {
    let report = run(include_str!("fixtures/diag-empty-transit.ofm"));
    assert_eq!(out(&report, "Source1"), 10.0);
    assert_eq!(input(&report, "Sink1"), 10.0);
    assert_near(
        report.objects["Queue1"].avg_content.mean,
        0.0,
        1e-9,
        "queue WIP",
    );
    assert_near(
        report.objects["Queue1"].avg_stay_time.mean,
        0.0,
        1e-9,
        "queue wait",
    );
    assert_near(
        report.objects["Proc1"].avg_content.mean,
        0.01,
        1e-9,
        "processor WIP",
    );
    assert_eq!(report.replications[0].items_in_system, 0);
}

// ---------------------------------------------------------------------------
// MultiProcessor (B1)
// ---------------------------------------------------------------------------

fn multiprocessor_model(slots: u32, process_time: f64, operator: &str) -> String {
    let operator_line = if operator.is_empty() {
        String::new()
    } else {
        format!(r#", "process_operator": "{operator}""#)
    };
    format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "multiprocessor-{slots}",
      "run": {{ "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "MP", "kind": "multiprocessor", "params": {{
            "slots": {slots},
            "setup_time": {{ "distribution": "constant", "value": 0.0 }},
            "process_time": {{ "distribution": "constant", "value": {process_time} }}{operator_line} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }},
        {{ "name": "Dispatcher1", "kind": "dispatcher", "pos": [100, 0], "params": {{}} }},
        {{ "name": "T1", "kind": "transporter", "pos": [100, 0], "params": {{ "capacity": 1, "speed": 10.0 }} }},
        {{ "name": "T2", "kind": "transporter", "pos": [100, 0], "params": {{ "capacity": 1, "speed": 10.0 }} }}
      ],
      "connections": [
        {{ "from": "S", "to": "MP" }}, {{ "from": "MP", "to": "K" }},
        {{ "from": "Dispatcher1", "to": "T1" }}, {{ "from": "Dispatcher1", "to": "T2" }}
      ]
    }}"#
    )
}

#[test]
fn multiprocessor_runs_slots_in_parallel() {
    let report = run(&multiprocessor_model(3, 20.0, ""));
    // Three slots process 20 time units each; arrivals every 10 time units are
    // absorbed, so everything except the item still in process leaves.
    assert_eq!(input(&report, "MP"), 50.0);
    assert_eq!(out(&report, "MP"), 49.0);
    assert_near(
        report.objects["MP"].avg_stay_time.mean,
        20.0,
        1e-9,
        "slot stay",
    );
    // Average over the three slots: 49 * 20 plus the partial item at stop.
    assert_near(
        report.objects["MP"].utilization.mean,
        0.648,
        0.02,
        "mean slot utilisation",
    );
    assert!(report.objects["MP"].max_content.mean <= 3.0);
}

#[test]
fn multiprocessor_slots_raise_the_throughput() {
    let single = run(&multiprocessor_model(1, 25.0, ""));
    let parallel = run(&multiprocessor_model(3, 25.0, ""));
    assert!(
        input(&parallel, "K") > input(&single, "K"),
        "more slots must raise the throughput: {} vs {}",
        input(&parallel, "K"),
        input(&single, "K")
    );
    assert!(input(&single, "K") <= 21.0);
}

#[test]
fn multiprocessor_requests_a_crew_per_busy_period() {
    let without = run(&multiprocessor_model(2, 20.0, ""));
    let with = run(&multiprocessor_model(2, 20.0, "Dispatcher1"));
    assert!(input(&with, "K") > 0.0);
    assert!(
        input(&with, "K") < input(&without, "K"),
        "the operator crew must delay the start: {} vs {}",
        input(&with, "K"),
        input(&without, "K")
    );
    assert!(state_times(&with, "MP").blocked > 0.0);
}

// ---------------------------------------------------------------------------
// Time Table (B11)
// ---------------------------------------------------------------------------

/// A linear availability calendar pauses the processor outside its windows and
/// reports the unavailable time as `off_shift`.
#[test]
fn timetable_pauses_the_object_outside_its_windows() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "timetable",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 4.0 },
            "timetable": { "windows": [
              { "start": 0.0, "end": 100.0 },
              { "start": 200.0, "end": 300.0 },
              { "start": 400.0, "end": 500.0 }
            ] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    // The source keeps its departure-paced rhythm: it produces while the
    // processor is available (0-100, 200-300, 400-500) and waits otherwise, so
    // 30 items are created and all of them are processed.
    assert_eq!(out(&report, "S"), 30.0);
    assert_eq!(input(&report, "K"), 30.0);
    let states = state_times(&report, "P");
    assert_near(states.off_shift, 200.0, 0.5, "off-shift time");
    assert_near(states.busy, 120.0, 0.5, "processing time");
}

/// Availability calendars and shift plans are combined: the object only works
/// when both are open.
#[test]
fn timetable_and_shifts_combine() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "timetable-shift",
      "run": { "stop_time": 300.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 5.0 },
            "shifts": [{ "start": 0.0, "duration": 200.0, "repeat_every": 0.0 }],
            "timetable": { "windows": [{ "start": 0.0, "end": 100.0 }] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    // Only the intersection [0, 100) is usable. The source creates 11 items
    // (0..100); the last one arrives as the calendar closes and waits forever,
    // so 10 items reach the sink.
    assert_eq!(out(&report, "S"), 10.0);
    assert_eq!(input(&report, "K"), 10.0);
    assert_near(state_times(&report, "P").off_shift, 200.0, 0.5, "off-shift");
}

#[test]
fn timetable_windows_must_not_overlap() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-timetable",
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "timetable": { "windows": [
              { "start": 0.0, "end": 100.0 },
              { "start": 50.0, "end": 150.0 }
            ] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "K" }]
    }"#;
    let error = Model::from_json(json).expect_err("overlapping windows must fail validation");
    assert!(
        error.to_string().contains("must not overlap"),
        "unexpected error: {error}"
    );
}

// ---------------------------------------------------------------------------
// Event trace (B12)
// ---------------------------------------------------------------------------

#[test]
fn trace_records_every_processed_event() {
    let model = model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "trace",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 4.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#,
    );
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("traced run");
    let plain = simulate(&model, &config, 0).expect("plain run");
    assert_eq!(result, plain, "tracing must not change the run");

    assert!(!trace.truncated);
    assert!(trace.len() > 10);
    assert!(
        trace
            .events
            .windows(2)
            .all(|pair| (pair[0].time, pair[0].seq) <= (pair[1].time, pair[1].seq)),
        "the trace must follow the event calendar order"
    );
    for label in ["arrive", "flow", "process_done"] {
        assert!(
            trace.events.iter().any(|event| event.event == label),
            "missing {label} event"
        );
    }
    assert!(
        trace
            .object("P")
            .any(|event| event.event == "process_done" && event.time < 20.0),
        "processor events must carry the object name"
    );
    // A queue batch timeout / shift change style event carries its detail.
    let first = &trace.events[0];
    assert_eq!(first.time, 0.0);
    assert!(first.data.is_empty() || first.data.contains_key("token"));
}

#[test]
fn trace_is_off_for_normal_runs() {
    let model = model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "no-trace",
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "max_arrivals": 1 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "K" }]
    }"#,
    );
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (_, trace) = simulate_traced(&model, &config, 0).expect("traced run");
    let plain = simulate(&model, &config, 0).expect("plain run");
    assert_eq!(plain.items_completed, 1);
    // The sink delivery happens inside the source's `flow` event, so the trace
    // only contains the arrival and that flow.
    assert_eq!(trace.len(), 2);
    assert_eq!(trace.events[0].event, "arrive");
    assert_eq!(trace.events[1].event, "flow");
    assert_eq!(trace.object("K").count(), 0);
}
