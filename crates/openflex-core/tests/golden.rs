//! Golden tests: validate the engine against analytic results and against the
//! definitions in the model format.
//!
//! These are deliberately numerical rather than snapshot-based: a change to
//! the engine that breaks a semantic contract should fail here even when it
//! makes the output look plausible.

use openflex_core::run_model;
use openflex_model::Model;

fn model(json: &str) -> Model {
    Model::from_json(json).expect("model should load")
}

/// M/M/1 with λ = 0.2, μ = 0.25 (ρ = 0.8). Analytic values:
/// utilization 0.8, Lq = ρ²/(1−ρ) = 3.2, Wq = ρ/(μ−λ) = 16, W = 20.
fn mm1() -> Model {
    model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "mm1-golden",
      "time_unit": "minutes",
      "run": { "stop_time": 100000.0, "warmup": 10000.0, "seed": 42, "replications": 10 },
      "objects": [
        { "name": "Arrivals", "kind": "source", "params": {
            "interarrival": { "distribution": "exponential", "mean": 5.0 } } },
        { "name": "Queue", "kind": "queue", "params": { "max_content": 100000 } },
        { "name": "Server", "kind": "processor", "params": {
            "process_time": { "distribution": "exponential", "mean": 4.0 } } },
        { "name": "Exit", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "Arrivals", "to": "Queue" },
        { "from": "Queue", "to": "Server" },
        { "from": "Server", "to": "Exit" }
      ]
    }"#,
    )
}

fn assert_near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected} ± {tolerance}, got {actual}"
    );
}

#[test]
fn mm1_matches_analytic_utilization() {
    let report = run_model(&mm1()).expect("run");
    let server = &report.objects["Server"];
    assert_near(server.utilization.mean, 0.8, 0.05, "server utilization");
}

#[test]
fn mm1_matches_analytic_queue_length() {
    let report = run_model(&mm1()).expect("run");
    let queue = &report.objects["Queue"];
    // Lq = ρ² / (1 − ρ) = 3.2; a finite run has variance, allow 30%.
    assert_near(queue.avg_content.mean, 3.2, 1.0, "queue length");
}

#[test]
fn mm1_matches_analytic_waiting_time() {
    let report = run_model(&mm1()).expect("run");
    let queue = &report.objects["Queue"];
    // Wq = ρ / (μ − λ) = 16.
    assert_near(queue.avg_stay_time.mean, 16.0, 4.0, "queue waiting time");
    let server = &report.objects["Server"];
    // W_service = 1 / μ = 4.
    assert_near(server.avg_stay_time.mean, 4.0, 0.6, "service time");
}

#[test]
fn mm1_throughput_matches_arrival_rate() {
    let report = run_model(&mm1()).expect("run");
    let server = &report.objects["Server"];
    // λ = 0.2 completions per time unit.
    assert_near(server.throughput.mean, 0.2, 0.01, "throughput");
    // Everything created after the warm-up either left the model or is still
    // inside it (plus items that entered the system before the warm-up).
    assert!(
        report.completed.mean <= report.created.mean,
        "completed {} cannot exceed created {}",
        report.completed.mean,
        report.created.mean
    );
}

#[test]
fn same_seed_reproduces_exactly() {
    let first = run_model(&mm1()).expect("run");
    let second = run_model(&mm1()).expect("run");
    assert_eq!(
        first, second,
        "the same model and seed must reproduce bit for bit"
    );
}

#[test]
fn different_seed_changes_the_run() {
    let mut model = mm1();
    model.run.replications = 1;
    let first = run_model(&model).expect("run");
    model.run.seed = 999;
    let second = run_model(&model).expect("run");
    assert_ne!(
        first, second,
        "a different seed must produce a different run"
    );
}

#[test]
fn source_respects_max_arrivals() {
    let report = run_model(&model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "exact",
      "run": { "stop_time": 1000.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 5.0 },
            "max_arrivals": 10 } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "K" }]
    }"#,
    ))
    .expect("run");
    assert_eq!(report.created.mean, 10.0);
    assert_eq!(report.completed.mean, 10.0);
    assert_eq!(report.replications[0].items_in_system, 0);
}

#[test]
fn percentage_routing_is_respected() {
    let report = run_model(&model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "split",
      "run": { "stop_time": 200000.0, "warmup": 0.0, "seed": 3, "replications": 1 },
      "objects": [
        { "name": "Arrivals", "kind": "source", "params": {
            "interarrival": { "distribution": "exponential", "mean": 1.0 } } },
        { "name": "A", "kind": "sink", "params": {} },
        { "name": "B", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "Arrivals", "to": "A",
          "strategy": { "type": "by_percentage", "percentages": [40.0, 60.0] } },
        { "from": "Arrivals", "to": "B" }
      ]
    }"#,
    ))
    .expect("run");
    let total = report.completed.mean;
    let a = report.objects["A"].output_count.mean;
    assert_near(a / total, 0.4, 0.02, "routing share of port 0");
}

#[test]
fn warmup_excludes_initial_transient_from_counts() {
    let mut base = model(
        r#"{
      "format": "openflex-model", "version": 1, "name": "warm",
      "run": { "stop_time": 20000.0, "warmup": 0.0, "seed": 5, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 2.0 } } },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "K" }]
    }"#,
    );
    let no_warmup = run_model(&base).expect("run");
    base.run.warmup = 10000.0;
    let with_warmup = run_model(&base).expect("run");
    // The same amount of simulated time elapses, but the counted window is half.
    let counted_no_warmup = no_warmup.completed.mean;
    let counted_with_warmup = with_warmup.completed.mean;
    let ratio = counted_with_warmup / counted_no_warmup;
    assert_near(ratio, 0.5, 0.05, "warm-up halves the counted window");
}

#[test]
fn model_with_invalid_routing_is_rejected() {
    let err = Model::from_json(
        r#"{
      "format": "openflex-model", "version": 1, "name": "bad",
      "objects": [
        { "name": "S", "kind": "source", "params": {} },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S", "to": "K",
          "strategy": { "type": "by_percentage", "percentages": [10.0] } }
      ]
    }"#,
    );
    assert!(
        err.is_err(),
        "percentages that do not sum to 100 must be rejected"
    );
}
