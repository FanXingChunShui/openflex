//! P0 engine semantics that the v0.1.0 engine did not implement yet.
//!
//! One section per backlog item:
//!
//! * A3 pull mode (downstream asks upstream for items, with a requirement and
//!   a port strategy),
//! * A4 `route() == -1`: hold a finished item until it is released,
//! * A5 in-transit items count against capacity,
//! * A8 `first_failure_time` and up-time counted only in selected states,
//! * A9 overlapping downtime requests,
//! * A10 operator counts, priorities and preemption,
//! * A12 state-time detail, t-distribution intervals and Little's law check.
//!
//! The models are deterministic (constant distributions) unless the test is
//! explicitly about a random effect.

use openflex_core::{SimReport, StateTimes, run_model};
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

fn state_times(report: &SimReport, name: &str) -> StateTimes {
    report.replications[0].objects[name].state_times
}

fn out(report: &SimReport, name: &str) -> f64 {
    report.objects[name].output_count.mean
}

fn input(report: &SimReport, name: &str) -> f64 {
    report.objects[name].input_count.mean
}

// ---------------------------------------------------------------------------
// A12. State-time detail, t intervals and Little's law
// ---------------------------------------------------------------------------

/// `setup` and `processing` subdivide `busy`: 10 items × (2 setup + 4 process).
#[test]
fn a12_state_detail_splits_busy_into_setup_and_processing() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "state-detail",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "setup_time": { "distribution": "constant", "value": 2.0 },
            "process_time": { "distribution": "constant", "value": 4.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    let states = state_times(&report, "P");
    assert_eq!(input(&report, "K"), 10.0);
    assert_near(states.busy, 60.0, 1e-9, "busy time");
    assert_near(states.setup, 20.0, 1e-9, "setup detail");
    assert_near(states.processing, 40.0, 1e-9, "processing detail");
    // The details are a partition of busy, not extra time.
    assert_near(
        states.setup + states.processing,
        states.busy,
        1e-9,
        "detail",
    );
    assert_near(states.idle, 40.0, 1e-9, "idle time");
    // The legacy total is unchanged by the detail fields.
    assert_near(states.total(), 100.0, 1e-9, "state total");
}

/// A processor waiting for a transporter records `waiting_for_transport`.
#[test]
fn a12_waiting_for_transport_is_a_detail_of_blocked() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "wait-transport",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 20.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 5.0 },
            "process_operator": "Dispatcher1" } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Dispatcher1", "kind": "dispatcher", "pos": [100, 0], "params": {} },
        { "name": "Transporter1", "kind": "transporter", "pos": [100, 0], "params": {
            "capacity": 1, "speed": 10.0, "home": "Dispatcher1" } }
      ],
      "connections": [
        { "from": "S", "to": "P" }, { "from": "P", "to": "K" },
        { "from": "Dispatcher1", "to": "Transporter1" }
      ]
    }"#);
    let states = state_times(&report, "P");
    assert!(states.waiting_for_transport > 0.0, "{states:?}");
    // The detail never exceeds the state it explains.
    assert!(
        states.waiting_for_transport <= states.blocked + 1e-9,
        "{states:?}"
    );
    // This model has no resource pool, so nothing waits for a plain operator.
    assert_near(states.waiting_for_operator, 0.0, 1e-9, "no pool wait");
}

/// A processor waiting for a resource pool records `waiting_for_operator`.
#[test]
fn a12_waiting_for_operator_is_a_detail_of_blocked() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "wait-operator",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 4.0 },
            "process_operator": "Resource1" } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Resource1", "kind": "resource", "params": {
            "capacity": 1, "downtimes": [{ "start": 0.0, "end": 100.0 }] } }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    let states = state_times(&report, "P");
    assert!(
        states.waiting_for_operator > 90.0,
        "the pool is down for 100 time units: {states:?}"
    );
    assert!(
        states.waiting_for_operator <= states.blocked + 1e-9,
        "{states:?}"
    );
    assert_near(states.waiting_for_transport, 0.0, 1e-9, "no transport wait");
}

/// Little's law holds for every object of a deterministic line, so the report
/// carries a clean check.
#[test]
fn a12_little_check_passes_for_a_stable_line() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "little",
      "run": { "stop_time": 5000.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 100 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 6.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" }, { "from": "Q", "to": "P" }, { "from": "P", "to": "K" }
      ]
    }"#);
    let check = report.little_check.as_ref().expect("little check present");
    assert_eq!(check.tolerance, 0.10);
    assert!(
        check.is_ok(),
        "unexpected mismatches: {:?}",
        check.mismatches
    );
    for (name, entry) in &check.objects {
        assert_near(
            entry.predicted_content,
            entry.throughput * entry.stay_time,
            1e-12,
            &format!("{name} predicted content"),
        );
    }
    // The queue hands each item over at once: L = W = 0, so λW = 0 too.
    let queue = &check.objects["Q"];
    assert_near(queue.content_avg, 0.0, 1e-9, "queue content");
    assert_near(queue.predicted_content, 0.0, 1e-9, "queue λW");
    // The processor holds an item for exactly its 6 time units of service, so
    // L = λW = 0.1 × 6 = 0.6.
    let processor = &check.objects["P"];
    assert_near(processor.content_avg, 0.6, 1e-9, "processor content");
    assert_near(processor.predicted_content, 0.6, 1e-9, "processor λW");
}

// ---------------------------------------------------------------------------
// A8. Failure clock and A9. Downtime request stack
// ---------------------------------------------------------------------------

/// A breakdown and a shift break overlap:
///
/// * t = 0..30 processing,
/// * t = 30 breakdown (`first_failure_time`, so the first failure is exact),
/// * t = 40 the shift window ends while the machine is still broken,
/// * t = 50 the repair finishes, but the shift request still holds the object,
/// * t = 60 the shift starts again, the stack drains and processing resumes.
#[test]
fn a09_overlapping_downtime_sources_recover_in_order() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "downtime-stack",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 1000.0 },
            "shifts": [{ "start": 0.0, "duration": 40.0, "repeat_every": 60.0 }],
            "failure": {
              "mtbf": { "distribution": "constant", "value": 10000.0 },
              "mttr": { "distribution": "constant", "value": 20.0 },
              "first_failure_time": 30.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    let states = state_times(&report, "P");
    // The repair at t = 50 must not resume processing: the shift request is
    // still on the stack, so the object waits until the shift starts at t = 60.
    assert_near(states.busy, 70.0, 1e-9, "busy: 0..30 plus 60..100");
    assert_near(states.down, 20.0, 1e-9, "down: 30..50");
    assert_near(states.off_shift, 10.0, 1e-9, "off shift: 50..60");
    assert_near(states.total(), 100.0, 1e-9, "state total");
}

/// A downtime request never ends early: the object is down until the repair.
#[test]
fn a08_first_failure_time_sets_the_first_breakdown() {
    let with_first = run(r#"{
      "format": "openflex-model", "version": 1, "name": "first-failure",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 1000.0 },
            "failure": {
              "mtbf": { "distribution": "constant", "value": 1000.0 },
              "mttr": { "distribution": "constant", "value": 10.0 },
              "first_failure_time": 20.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    // The 1000 MTBF would not fire inside the run; the explicit first failure
    // at t = 20 does.
    assert_near(state_times(&with_first, "P").down, 10.0, 1e-9, "down time");

    // A negative `first_failure_time` means "ignore" and falls back to the MTBF.
    let ignored = run(r#"{
      "format": "openflex-model", "version": 1, "name": "first-failure-ignored",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 1000.0 },
            "failure": {
              "mtbf": { "distribution": "constant", "value": 1000.0 },
              "mttr": { "distribution": "constant", "value": 10.0 },
              "first_failure_time": -1.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    assert_near(state_times(&ignored, "P").down, 0.0, 1e-9, "no failure");
}

/// `count_time_in_states: ["processing"]` keeps the MTBF clock paused while the
/// processor is starved, so the same run sees fewer failures and more output.
#[test]
fn a08_up_time_can_be_counted_only_in_processing() {
    let template = |count_states: &str| {
        format!(
            r#"{{
      "format": "openflex-model", "version": 1, "name": "up-time",
      "run": {{ "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 5.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "P", "kind": "processor", "params": {{
            "setup_time": {{ "distribution": "constant", "value": 10.0 }},
            "process_time": {{ "distribution": "constant", "value": 10.0 }},
            "failure": {{
              "mtbf": {{ "distribution": "constant", "value": 25.0 }},
              "mttr": {{ "distribution": "constant", "value": 10.0 }},
              "count_time_in_states": {count_states} }} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [{{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }}]
    }}"#
        )
    };
    let wall_clock = run(&template("[]"));
    let processing_only = run(&template(r#"["processing"]"#));
    // Wall clock: a failure every 25 + 10 time units, i.e. ~14 of them.
    // Processing only: 25 time units of *work*, which take ~50 wall-clock
    // units at 50% utilisation, so roughly half as many failures.
    assert!(
        state_times(&processing_only, "P").down < state_times(&wall_clock, "P").down - 30.0,
        "processing-only counting must fail less often: {:?} vs {:?}",
        state_times(&processing_only, "P"),
        state_times(&wall_clock, "P"),
    );
    assert!(
        input(&processing_only, "K") > input(&wall_clock, "K"),
        "fewer failures must produce more items: {} vs {}",
        input(&processing_only, "K"),
        input(&wall_clock, "K"),
    );
    // The processing clock is still a real clock: it does fire.
    assert!(state_times(&processing_only, "P").down > 0.0);
}

// ---------------------------------------------------------------------------
// A12. Small-sample intervals
// ---------------------------------------------------------------------------

/// With three replications the half-width uses the t quantile for df = 2.
#[test]
fn a12_small_sample_ci_uses_the_t_table() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "t-interval",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 7, "replications": 3 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "exponential", "mean": 5.0 } } },
        { "name": "P", "kind": "processor", "params": {
            "process_time": { "distribution": "exponential", "mean": 4.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
    }"#);
    let completed = &report.completed;
    assert_eq!(completed.n, 3);
    // sd is the sample standard deviation, so se = sd / sqrt(3); t(2) = 4.303.
    let expected = 4.303 * completed.std_dev / 3.0f64.sqrt();
    assert_near(completed.ci95, expected, 1e-9, "ci95 half-width");
    // Wider than the normal approximation: that is the point of the t table.
    assert!(completed.ci95 > 1.96 * completed.std_dev / 3.0f64.sqrt());
}

// ---------------------------------------------------------------------------
// A3. Pull mode
// ---------------------------------------------------------------------------

/// The queue pulls instead of being pushed into: the source never hands an
/// item over on its own, yet every item reaches the sink.
#[test]
fn a03_pull_moves_items_without_push() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "pull-basic",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 100, "pull": { "enabled": true } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "Q" }, { "from": "Q", "to": "K" }
      ]
    }"#);
    assert_eq!(out(&report, "S"), 10.0);
    assert_eq!(input(&report, "Q"), 10.0);
    assert_eq!(input(&report, "K"), 10.0);
}

/// `requirement` rejects items that do not match: type 1 is pulled, type 0 is
/// left in its source.
#[test]
fn a03_pull_requirement_skips_items() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "pull-requirement",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S0", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "first_arrival": 0.0, "item_type": 0 } },
        { "name": "S1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "first_arrival": 0.0, "item_type": 1 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 100,
            "pull": { "enabled": true, "requirement": "item.type == 1" } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S0", "to": "Q" }, { "from": "S1", "to": "Q" }, { "from": "Q", "to": "K" }
      ]
    }"#);
    assert_eq!(input(&report, "K"), 10.0, "only type 1 items pass");
    assert_eq!(out(&report, "S1"), 10.0);
    assert_eq!(out(&report, "S0"), 0.0, "type 0 items stay in their source");
}

/// Round-robin pull takes from both upstream objects.
#[test]
fn a03_pull_strategy_round_robin_uses_every_upstream() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "pull-rr",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S0", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "S1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 1,
            "pull": { "enabled": true, "strategy": { "type": "round_robin" } } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S0", "to": "Q" }, { "from": "S1", "to": "Q" }, { "from": "Q", "to": "K" }
      ]
    }"#);
    assert_eq!(input(&report, "K"), 20.0);
    assert_eq!(out(&report, "S0"), 10.0);
    assert_eq!(out(&report, "S1"), 10.0);
}

// ---------------------------------------------------------------------------
// A4. SendToPort = -1 (hold) and release_item
// ---------------------------------------------------------------------------

/// The queue holds its items instead of sending them; a message fires
/// `release_item`, which sends the held item and lets it flow to the sink.
#[test]
fn a04_hold_keeps_items_until_release() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "hold",
      "run": { "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 50.0 }, "first_arrival": 0.0 } },
        { "name": "Q", "kind": "queue", "params": {
            "max_content": 100,
            "triggers": { "on_message": [{ "action": "release_item" }] } } },
        { "name": "S2", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 100.0 }, "first_arrival": 0.0,
            "triggers": { "on_create": [{ "action": "send_message", "target": "Q", "delay": "0" }] } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S1", "to": "Q" },
        { "from": "Q", "to": "K", "strategy": { "type": "hold" } },
        { "from": "S2", "to": "K" }
      ]
    }"#);
    // Messages arrive at t = 0 and t = 100, releasing one item each time; the
    // third item (t = 150) stays held until the run ends. S2 only exists to
    // schedule the messages, so its two items also reach the sink.
    assert_eq!(out(&report, "Q"), 2.0);
    assert_eq!(input(&report, "K"), 4.0);
    assert!(
        state_times(&report, "Q").blocked > 0.0,
        "the hold blocks the queue"
    );
}

// ---------------------------------------------------------------------------
// A10. Operator counts, SameAsSetup and priority
// ---------------------------------------------------------------------------

/// Two process operators are required; a pool with only one unit can never
/// satisfy the request, so nothing is produced.
#[test]
fn a10_operator_count_gates_processing() {
    let template = |capacity: u32| {
        format!(
            r#"{{
      "format": "openflex-model", "version": 1, "name": "operator-count",
      "run": {{ "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "P", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "constant", "value": 6.0 }},
            "process_operator": "Resource1", "num_process_operators": 2 }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }},
        {{ "name": "Resource1", "kind": "resource", "params": {{ "capacity": {capacity} }} }}
      ],
      "connections": [ {{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }} ]
    }}"#
        )
    };
    let one_operator = run(&template(1));
    let two_operators = run(&template(2));
    assert_eq!(input(&one_operator, "K"), 0.0, "one unit cannot serve two");
    assert!(input(&two_operators, "K") > 0.0, "two units can");
}

/// `use_same_num_operators` makes the process phase reuse the setup crew
/// size, so two processors can no longer run in parallel on a pool of two.
#[test]
fn a10_same_as_setup_reuses_the_setup_crew() {
    let template = |same: bool| {
        // The model layer requires the declared process count to match the
        // setup count when `use_same_num_operators` is on.
        let process_ops = if same { 2 } else { 1 };
        format!(
            r#"{{
      "format": "openflex-model", "version": 1, "name": "same-as-setup",
      "run": {{ "stop_time": 200.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
      "objects": [
        {{ "name": "S1", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "S2", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "constant", "value": 10.0 }}, "first_arrival": 0.0 }} }},
        {{ "name": "P1", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "constant", "value": 10.0 }},
            "setup_operator": "Resource1", "process_operator": "Resource1",
            "num_setup_operators": 2, "num_process_operators": {process_ops},
            "use_same_num_operators": {same} }} }},
        {{ "name": "P2", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "constant", "value": 10.0 }},
            "setup_operator": "Resource1", "process_operator": "Resource1",
            "num_setup_operators": 2, "num_process_operators": {process_ops},
            "use_same_num_operators": {same} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }},
        {{ "name": "Resource1", "kind": "resource", "params": {{ "capacity": 2 }} }}
      ],
      "connections": [
        {{ "from": "S1", "to": "P1" }}, {{ "from": "P1", "to": "K" }},
        {{ "from": "S2", "to": "P2" }}, {{ "from": "P2", "to": "K" }}
      ]
    }}"#
        )
    };
    let shared = run(&template(true));
    let independent = run(&template(false));
    // With the setup crew size reused, each process needs both pool units, so
    // the processors serialise and finish fewer items.
    assert!(
        input(&independent, "K") > input(&shared, "K"),
        "independent crews must run in parallel: {} vs {}",
        input(&independent, "K"),
        input(&shared, "K"),
    );
}

/// The dispatcher serves the higher `operator_priority` first, so the urgent
/// processor gets the single transporter more often.
#[test]
fn a10_priority_decides_who_gets_the_transporter() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "operator-priority",
      "run": { "stop_time": 500.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "S2", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "High", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 5.0 },
            "process_operator": "Dispatcher1", "operator_priority": 10 } },
        { "name": "Low", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 5.0 },
            "process_operator": "Dispatcher1", "operator_priority": 0 } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Dispatcher1", "kind": "dispatcher", "pos": [100, 0], "params": {} },
        { "name": "Transporter1", "kind": "transporter", "pos": [100, 0], "params": {
            "capacity": 1, "speed": 10.0, "home": "Dispatcher1" } }
      ],
      "connections": [
        { "from": "S1", "to": "High" }, { "from": "High", "to": "K" },
        { "from": "S2", "to": "Low" }, { "from": "Low", "to": "K" },
        { "from": "Dispatcher1", "to": "Transporter1" }
      ]
    }"#);
    assert!(
        out(&report, "High") > out(&report, "Low"),
        "priority 10 must win the transporter: {} vs {}",
        out(&report, "High"),
        out(&report, "Low"),
    );
}

// ---------------------------------------------------------------------------
// A5. In-transit items count against capacity
// ---------------------------------------------------------------------------

/// A processor with `max_content` 2 already holds one item and has one
/// operator in transit: the second item is refused until the operator arrives,
/// even though only one item physically sits inside the processor.
#[test]
fn a05_in_transit_items_close_the_input() {
    let report = run(r#"{
      "format": "openflex-model", "version": 1, "name": "in-transit",
      "run": { "stop_time": 50.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 }, "first_arrival": 0.0 } },
        { "name": "P", "kind": "processor", "params": {
            "max_content": 2,
            "process_time": { "distribution": "constant", "value": 100.0 },
            "process_operator": "Dispatcher1" } },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "Dispatcher1", "kind": "dispatcher", "pos": [100, 0], "params": {} },
        { "name": "Transporter1", "kind": "transporter", "pos": [100, 0], "params": {
            "capacity": 1, "speed": 1.0, "home": "Dispatcher1" } }
      ],
      "connections": [
        { "from": "S", "to": "P" }, { "from": "P", "to": "K" },
        { "from": "Dispatcher1", "to": "Transporter1" }
      ]
    }"#);
    // The transporter needs 100 time units to arrive, so at t = 10 the second
    // item finds the processor full (1 item inside + 1 operator in transit).
    assert_eq!(input(&report, "P"), 1.0);
    assert_eq!(
        out(&report, "S"),
        1.0,
        "the second item stays in the source"
    );
    assert_eq!(input(&report, "K"), 0.0);
}
