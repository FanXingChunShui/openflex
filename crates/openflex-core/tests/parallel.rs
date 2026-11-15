//! task-59: parallel replications must produce the serial report bit for bit.

use openflex_core::{run_model, run_model_parallel};
use openflex_model::Model;

fn model(replications: u32) -> Model {
    Model::from_json(&format!(
        r#"{{
      "format": "openflex-model", "version": 1, "name": "parallel",
      "run": {{ "stop_time": 200.0, "warmup": 20.0, "seed": 11, "replications": {replications} }},
      "objects": [
        {{ "name": "S", "kind": "source", "params": {{
            "interarrival": {{ "distribution": "exponential", "mean": 5.0 }} }} }},
        {{ "name": "Q", "kind": "queue", "params": {{ "max_content": 1000 }} }},
        {{ "name": "P", "kind": "processor", "params": {{
            "process_time": {{ "distribution": "exponential", "mean": 4.0 }} }} }},
        {{ "name": "K", "kind": "sink", "params": {{}} }}
      ],
      "connections": [
        {{ "from": "S", "to": "Q" }}, {{ "from": "Q", "to": "P" }}, {{ "from": "P", "to": "K" }}
      ]
    }}"#
    ))
    .expect("model")
}

/// Parallel execution reproduces the serial report exactly, including the
/// replication order.
#[test]
fn parallel_matches_serial() {
    let model = model(5);
    let serial = run_model(&model).expect("serial");
    let parallel = run_model_parallel(&model, 3).expect("parallel");
    assert_eq!(parallel, serial);
    assert_eq!(parallel.replications.len(), 5);
    for (index, replication) in parallel.replications.iter().enumerate() {
        assert_eq!(replication.index, index as u32);
    }
}

/// One worker is the serial path; more workers than replications is allowed.
#[test]
fn worker_counts_are_robust() {
    let model = model(4);
    let serial = run_model(&model).expect("serial");
    assert_eq!(run_model_parallel(&model, 1).expect("1 worker"), serial);
    assert_eq!(run_model_parallel(&model, 0).expect("0 workers"), serial);
    assert_eq!(run_model_parallel(&model, 16).expect("16 workers"), serial);
}

/// Repeated parallel runs are deterministic.
#[test]
fn parallel_is_deterministic() {
    let model = model(6);
    let first = run_model_parallel(&model, 4).expect("first");
    let second = run_model_parallel(&model, 4).expect("second");
    assert_eq!(first, second);
}
