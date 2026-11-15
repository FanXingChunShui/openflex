//! Engine integration of the newer model-format features.
//!
//! * **S connections** (`Connection.kind = "s"`) are center-port resource
//!   links: they never become an output/input port and never carry a flow
//!   item, but they still connect task executers to their dispatcher.
//! * **Global tables** are readable from expressions (step 2).
//! * **New triggers** (`OnContentChange`, `OnStateChange`, `OnReset`, …) run
//!   their actions (step 3).

use openflex_core::{SimReport, run_model};
use openflex_model::Model;

fn model(json: &str) -> Model {
    Model::from_json(json).expect("model should load")
}

fn run(json: &str) -> SimReport {
    run_model(&model(json)).expect("run")
}

fn out(report: &SimReport, name: &str) -> f64 {
    report.objects[name].output_count.mean
}

fn input(report: &SimReport, name: &str) -> f64 {
    report.objects[name].input_count.mean
}

// ---------------------------------------------------------------------------
// S connections
// ---------------------------------------------------------------------------

/// An S connection between two output ports must not consume a port number:
/// `by_label: port = 1` still reaches the second *flow* connection.
#[test]
fn s_connection_does_not_consume_an_output_port() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "s-ports",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0,
            "triggers": { "on_create": [
              { "action": "set_label", "name": "port", "value": "1" } ] } } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 1.0 } } },
        { "name": "A", "kind": "sink", "params": {} },
        { "name": "B", "kind": "sink", "params": {} },
        { "name": "Dispatcher1", "kind": "dispatcher", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "P" },
        { "from": "P", "to": "A", "strategy": { "type": "by_label", "label": "port" } },
        { "from": "P", "to": "Dispatcher1", "kind": "s" },
        { "from": "P", "to": "B" }
      ]
    }"#);
    assert_eq!(out(&report, "S"), 10.0);
    // Port 1 is the second A connection (B); the S link is not a port at all.
    assert_eq!(input(&report, "B"), 10.0);
    assert_eq!(input(&report, "A"), 0.0);
    assert_eq!(
        input(&report, "Dispatcher1"),
        0.0,
        "an S link carries no items"
    );
}

/// A processor linked to its dispatcher with an S connection still routes its
/// finished items along the A connection to the sink.
#[test]
fn s_connection_keeps_the_operator_team_and_skips_routing() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "s-team",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 4.0 },
            "process_operator": "Dispatcher1" } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Dispatcher1", "kind": "dispatcher", "pos": [10, 0], "params": {} },
        { "name": "Transporter1", "kind": "transporter", "pos": [10, 0], "params": {
            "capacity": 1, "speed": 10.0, "home": "Dispatcher1" } }
      ],
      "connections": [
        { "from": "S", "to": "P" },
        { "from": "P", "to": "K" },
        { "from": "P", "to": "Dispatcher1", "kind": "s" },
        { "from": "Dispatcher1", "to": "Transporter1" }
      ]
    }"#);
    // The operator team still resolves through the plain connection and the
    // processor's only flow port is the sink.
    assert_eq!(out(&report, "P"), 10.0);
    assert_eq!(input(&report, "K"), 10.0);
    assert_eq!(
        input(&report, "Dispatcher1"),
        0.0,
        "the S link must not receive items"
    );
}

/// An S link alone is not a delivery path: a processor with only an S
/// connection blocks instead of pushing items into its dispatcher.
#[test]
fn s_connection_alone_is_not_a_delivery_path() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "s-only",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 4.0 } } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Dispatcher1", "kind": "dispatcher", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "P" },
        { "from": "P", "to": "Dispatcher1", "kind": "s" }
      ]
    }"#);
    assert_eq!(out(&report, "P"), 0.0, "the S link cannot deliver the item");
    assert_eq!(input(&report, "Dispatcher1"), 0.0);
    assert!(report.replications[0].items_in_system > 0);
}

// ---------------------------------------------------------------------------
// Global tables
// ---------------------------------------------------------------------------

/// `table("Rates", row, "rate")` reads a cell with an expression row number:
/// the label set at creation decides which sink the item reaches.
#[test]
fn global_table_function_selects_a_row_with_an_expression() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "table-function",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "global_tables": {
        "Rates": { "columns": ["rate"], "rows": [[5], [20]] }
      },
      "objects": [
        { "name": "S0", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "first_arrival": 0.0, "item_type": 0,
            "triggers": { "on_create": [
              { "action": "set_label", "name": "rate",
                "value": "table(\"Rates\", item.type, \"rate\")" } ] } } },
        { "name": "S1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "first_arrival": 0.0, "item_type": 1,
            "triggers": { "on_create": [
              { "action": "set_label", "name": "rate",
                "value": "table(\"Rates\", item.type, \"rate\")" } ] } } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 100 } },
        { "name": "A", "kind": "sink", "params": {} },
        { "name": "B", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S0", "to": "Q" },
        { "from": "S1", "to": "Q" },
        { "from": "Q", "to": "A", "strategy": { "type": "by_condition", "conditions": [
            { "condition": "item.label.rate == 5", "port": 0 },
            { "condition": "item.label.rate == 20", "port": 1 } ] } },
        { "from": "Q", "to": "B" }
      ]
    }"#);
    assert_eq!(input(&report, "A"), 10.0, "type 0 maps to rate 5");
    assert_eq!(input(&report, "B"), 10.0, "type 1 maps to rate 20");
}

/// The plain path form reads the first row, which is enough for
/// configuration-style lookups inside a routing condition.
#[test]
fn global_table_path_reads_the_first_row() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "table-path",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "global_tables": {
        "Config": { "columns": ["enabled", "name"], "rows": [[1, "primary"], [0, "backup"]] }
      },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "A", "kind": "sink", "params": {} },
        { "name": "B", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "A", "strategy": { "type": "by_condition", "conditions": [
            { "condition": "table.Config.enabled == 1", "port": 0 } ] } },
        { "from": "S", "to": "B" }
      ]
    }"#);
    assert_eq!(input(&report, "A"), 10.0);
    assert_eq!(input(&report, "B"), 0.0);
}

// ---------------------------------------------------------------------------
// Lifecycle triggers
// ---------------------------------------------------------------------------

/// `on_state_change` fires when the queue becomes busy; the message releases
/// the item that the `hold` output strategy is keeping.
#[test]
fn on_state_change_trigger_runs() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "on-state-change",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 100,
            "triggers": {
              "on_state_change": [
                { "action": "send_message", "target": "Q", "delay": "0" } ],
              "on_message": [ { "action": "release_item" } ] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" },
        { "from": "Q", "to": "K", "strategy": { "type": "hold" } }
      ]
    }"#);
    assert_eq!(input(&report, "K"), 10.0, "every arrival is released");
}

/// `on_content_change` fires when an item enters the object.
#[test]
fn on_content_change_trigger_runs() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "on-content-change",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 100,
            "triggers": {
              "on_content_change": [
                { "action": "send_message", "target": "Q", "delay": "0" } ],
              "on_message": [ { "action": "release_item" } ] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" },
        { "from": "Q", "to": "K", "strategy": { "type": "hold" } }
      ]
    }"#);
    assert_eq!(input(&report, "K"), 10.0);
}

/// `on_reset` fires at the warm-up boundary and can release the item that was
/// held during the transient phase.
#[test]
fn on_reset_trigger_runs() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "on-reset",
      "run": { "stop_time": 100.0, "warmup": 50.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 100.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 100,
            "triggers": {
              "on_reset": [ { "action": "send_message", "target": "Q", "delay": "0" } ],
              "on_message": [ { "action": "release_item" } ] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" },
        { "from": "Q", "to": "K", "strategy": { "type": "hold" } }
      ]
    }"#);
    // The item arrives at t = 0 and is held; the reset at t = 50 releases it,
    // so the post-warm-up statistics count exactly one delivery.
    assert_eq!(input(&report, "K"), 1.0);
    assert_eq!(out(&report, "Q"), 1.0);
}

// ---------------------------------------------------------------------------
// Rack minimum dwell (task-36)
// ---------------------------------------------------------------------------

/// `min_dwell` keeps every item in the rack for at least the given time, even
/// when the sink could take it immediately.
#[test]
fn rack_min_dwell_delays_release() {
    let template = |min_dwell: f64| {
        format!(
            r#"{{
      "format": "openflex-model", "version": 1, "name": "rack-dwell",
      "run": {{ "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "R", "kind": "rack", "params": {{
            "capacity": 10, "levels": 1, "min_dwell": {min_dwell} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [ {{ "from": "S", "to": "R" }}, {{ "from": "R", "to": "K" }} ]
    }}"#
        )
    };
    let held = run(&template(10.0));
    let free = run(&template(0.0));
    // Every item waits exactly ten time units before it is released.
    assert!(
        (held.objects["R"].avg_stay_time.mean - 10.0).abs() < 1e-9,
        "dwell time: {}",
        held.objects["R"].avg_stay_time.mean
    );
    assert!(
        free.objects["R"].avg_stay_time.mean < 1e-9,
        "without a dwell the rack releases at once"
    );
    // The dwell only delays delivery; every item still arrives eventually.
    assert_eq!(input(&held, "K"), 19.0);
    assert_eq!(input(&free, "K"), 20.0);
}

// ---------------------------------------------------------------------------
// Elevator doors, overload and shaft conflicts (task-36)
// ---------------------------------------------------------------------------

/// Build an elevator model: `processors` operator requests at different
/// heights, served by `elevators` cabins.
fn elevator_model(
    processors: usize,
    elevators: usize,
    door_open: f64,
    door_close: f64,
    max_load: f64,
    shafts: &[&str],
) -> String {
    let mut objects = Vec::new();
    let mut connections = Vec::new();
    for index in 0..processors {
        let z = index as f64 * 40.0;
        objects.push(format!(
            r#"{{ "name": "S{index}", "kind": "source", "z": {z}, "params": {{
                "interarrival": {{ "distribution": "constant", "value": 10.0 }}, "first_arrival": 0.0 }} }}"#
        ));
        objects.push(format!(
            r#"{{ "name": "P{index}", "kind": "processor", "z": {z}, "params": {{
                "process_time": {{ "distribution": "constant", "value": 4.0 }},
                "process_operator": "Dispatcher1" }} }}"#
        ));
        connections.push(format!(r#"{{ "from": "S{index}", "to": "P{index}" }}"#));
        connections.push(format!(r#"{{ "from": "P{index}", "to": "K" }}"#));
    }
    objects.push(r#"{ "name": "K", "kind": "sink", "params": {} }"#.to_string());
    objects.push(r#"{ "name": "Dispatcher1", "kind": "dispatcher", "params": {} }"#.to_string());
    for index in 0..elevators {
        let shaft = shafts
            .get(index)
            .copied()
            .filter(|shaft| !shaft.is_empty())
            .map(|shaft| format!(r#", "shaft": "{shaft}""#))
            .unwrap_or_default();
        objects.push(format!(
            r#"{{ "name": "E{index}", "kind": "elevator", "z": 0, "params": {{
                "capacity": 4, "speed": 2.0, "home": "Dispatcher1",
                "door_open_time": {door_open}, "door_close_time": {door_close},
                "max_load": {max_load}, "load_per_task": 80.0{shaft} }} }}"#
        ));
        connections.push(format!(r#"{{ "from": "Dispatcher1", "to": "E{index}" }}"#));
    }
    format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "elevator",
      "run": {{ "stop_time": 300.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [ {} ],
      "connections": [ {} ]
    }}"#,
        objects.join(",\n"),
        connections.join(",\n")
    )
}

fn elevator_total(report: &SimReport, processors: usize) -> f64 {
    (0..processors)
        .map(|index| out(report, &format!("P{index}")))
        .sum()
}

/// Opening and closing the doors costs time on every stop, so the cabin
/// completes fewer tasks.
#[test]
fn elevator_door_times_slow_the_cabin() {
    let fast = run(&elevator_model(2, 1, 0.0, 0.0, 0.0, &[]));
    let doors = run(&elevator_model(2, 1, 10.0, 10.0, 0.0, &[]));
    assert!(
        elevator_total(&doors, 2) < elevator_total(&fast, 2),
        "door times must cost throughput: {} vs {}",
        elevator_total(&doors, 2),
        elevator_total(&fast, 2),
    );
    assert!(elevator_total(&doors, 2) > 0.0, "the cabin still works");
}

/// `max_load = 160` with 80 per task admits two passengers although the cabin
/// could take four.
#[test]
fn elevator_overload_rejects_passengers() {
    let unlimited = run(&elevator_model(4, 1, 0.0, 0.0, 0.0, &[]));
    let overload = run(&elevator_model(4, 1, 0.0, 0.0, 160.0, &[]));
    assert!(
        elevator_total(&overload, 4) < elevator_total(&unlimited, 4),
        "the load limit must reduce throughput: {} vs {}",
        elevator_total(&overload, 4),
        elevator_total(&unlimited, 4),
    );
}

/// Two cabins in the same shaft cannot travel at the same time, while two
/// shafts serve in parallel.
#[test]
fn elevators_in_one_shaft_serialise() {
    let shared = run(&elevator_model(4, 2, 0.0, 0.0, 0.0, &["A", "A"]));
    let separate = run(&elevator_model(4, 2, 0.0, 0.0, 0.0, &["A", "B"]));
    assert!(
        elevator_total(&shared, 4) < elevator_total(&separate, 4),
        "one shaft must serialise the cabins: {} vs {}",
        elevator_total(&shared, 4),
        elevator_total(&separate, 4),
    );
}

// ---------------------------------------------------------------------------
// Emptier (task-36, `basic_fr` + `emptier`)
// ---------------------------------------------------------------------------

/// The Emptier sweeps its upstream queue every `interval` and forwards the
/// items to its own output.
#[test]
fn emptier_flushes_upstream_on_a_schedule() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "emptier",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 100 } },
        { "name": "E", "kind": "basic_fr", "params": {
            "emptier": { "interval": { "distribution": "constant", "value": 50.0 } } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" },
        { "from": "Q", "to": "E" },
        { "from": "E", "to": "K" }
      ]
    }"#);
    // Sweeps at t = 50, 100 and 150 move the queue to the sink; the sweep at
    // t = 200 coincides with the stop time and does not run.
    assert_eq!(input(&report, "K"), 15.0);
    assert_eq!(input(&report, "E"), 15.0);
    // The queue only drains in batches, so items wait there.
    assert!(
        report.objects["Q"].avg_stay_time.mean > 0.0,
        "the queue holds items between sweeps"
    );
}

/// `destroy: true` removes the items instead of forwarding them.
#[test]
fn emptier_can_destroy_items() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "emptier-destroy",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 100 } },
        { "name": "E", "kind": "basic_fr", "params": {
            "emptier": { "interval": { "distribution": "constant", "value": 50.0 },
                         "destroy": true } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" },
        { "from": "Q", "to": "E" }
      ]
    }"#);
    assert_eq!(report.completed.mean, 0.0, "destroyed items never complete");
    // Every accepted item is destroyed; only the arrivals after the last sweep
    // are still in the queue.
    assert!(
        report.replications[0].items_in_system <= 5,
        "sweeps must empty the queue: {} items left",
        report.replications[0].items_in_system
    );
}

/// `max_items` caps how many items one sweep moves.
#[test]
fn emptier_max_items_limits_a_sweep() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "emptier-max",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 5.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 100 } },
        { "name": "E", "kind": "basic_fr", "params": {
            "emptier": { "interval": { "distribution": "constant", "value": 50.0 },
                         "max_items": 2 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" },
        { "from": "Q", "to": "E" },
        { "from": "E", "to": "K" }
      ]
    }"#);
    // Three sweeps (t = 50, 100, 150) of at most two items each: six arrive.
    assert_eq!(input(&report, "K"), 6.0);
    assert!(report.replications[0].items_in_system > 0);
}
