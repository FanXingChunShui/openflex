//! The `person-reserve` example exercises `grid_step` + `grid_reserve` end to end.

use openflex_core::run_model;
use openflex_model::Model;

#[test]
fn person_reserve_example_runs_and_both_people_work() {
    let model = Model::from_json(include_str!("../../../examples/objects/person-reserve.ofm"))
        .expect("example parses");
    let report = run_model(&model).expect("example runs");

    assert!(
        report.completed.mean > 0.0,
        "the corridor scenario should complete items, got {}",
        report.completed.mean
    );
    for name in ["Person1", "Person2"] {
        let stats = report.objects.get(name).expect("person entry");
        assert!(
            stats.input_count.mean > 0.0,
            "{name} should take part in the run"
        );
    }
    // The reservations must not dead-lock: the run finishes at the stop time.
    assert!(report.completed.mean >= 1.0);
}
