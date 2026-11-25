//! Behaviour tests for shipped example models.
//!
//! The matrix marks some rows ✅ on the strength of an example model rather than
//! an engine unit test ("First Model": three varieties through a shared test
//! station with rework). Those claims are only evidence if something asserts
//! them, so this file turns the prose into checks.

use openflex_core::run_model;
use openflex_model::Model;

fn load(text: &str) -> Model {
    Model::from_json(text).expect("example model must validate")
}

fn three_varieties() -> Model {
    load(include_str!("../../../examples/three-varieties.ofm"))
}

#[test]
fn three_varieties_model_keeps_its_shape() {
    let model = three_varieties();
    // Three dedicated machines fed by a shared buffer, chosen by item type.
    let typed: Vec<&str> = model
        .connections
        .iter()
        .filter(|connection| {
            connection.from == "EntryBuffer"
                && matches!(
                    &connection.strategy,
                    Some(openflex_model::PortStrategy::ByItemType { .. })
                )
        })
        .map(|connection| connection.to.as_str())
        .collect();
    assert_eq!(
        typed.len(),
        3,
        "the buffer must route by item type to three machines: {typed:?}"
    );
    // Rework: the tester sends failures back to the entry buffer.
    let rework = model
        .connections
        .iter()
        .any(|connection| connection.from == "Tester" && connection.to == "EntryBuffer");
    assert!(rework, "the tester must route failures back to the buffer");
}

#[test]
fn three_varieties_runs_five_replications_and_the_tester_is_the_bottleneck() {
    let model = three_varieties();
    // The model declares five replications; `run_model` honours that.
    let report = run_model(&model).expect("five replications must run");
    assert_eq!(report.replications.len(), 5);

    // The claim in the matrix: the shared test station is the constraint.
    let tester = report.objects.get("Tester").expect("Tester row");
    let tester_utilisation = tester.utilization.mean;
    let busiest = report
        .objects
        .iter()
        .filter(|(name, _)| name.as_str() != "Tester")
        .map(|(_, stats)| stats.utilization.mean)
        .fold(0.0_f64, f64::max);
    assert!(
        tester_utilisation > busiest,
        "Tester must be the busiest object: {tester_utilisation:.3} vs {busiest:.3}"
    );
    // And it should be heavily loaded — the documented measurement was 0.763,
    // so hold a wide band around it rather than pinning an exact number.
    assert!(
        (0.7..0.85).contains(&tester_utilisation),
        "Tester utilisation {tester_utilisation:.3} should be near the documented 0.763"
    );

    // Rework means more items enter the machines than one per source arrival.
    let completed = report.completed.mean;
    assert!(completed > 0.0, "items must complete");
}

#[test]
fn three_varieties_is_deterministic_across_repeated_runs() {
    let model = three_varieties();
    let first = run_model(&model).expect("first run");
    let second = run_model(&model).expect("second run");
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap(),
        "the same seed must produce the same report"
    );
}
