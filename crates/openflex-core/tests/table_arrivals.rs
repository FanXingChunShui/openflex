//! Table-driven arrival schedules must actually drive arrivals.
//!
//! The matrix claimed this row ✅ on the strength of a *parser* test
//! (`apply_table_reference`), but the engine only looked at the inline schedule,
//! so a table-driven source created nothing at all. These tests run the model.

use openflex_core::run_model;
use openflex_model::Model;

fn load(json: &str) -> Model {
    Model::from_json(json).expect("table-driven model must validate")
}

#[test]
fn a_table_driven_source_creates_the_rows_items() {
    let model = load(
        r#"{
          "format": "openflex-model", "version": 1, "name": "table-arrivals",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "global_tables": {
            "Schedule": { "columns": ["time", "quantity"],
              "rows": [[10.0, 1.0], [20.0, 2.0], [30.0, 1.0]] }
          },
          "objects": [
            { "name": "S", "kind": "source",
              "params": { "arrival_schedule_from_table": "Schedule" } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let report = run_model(&model).expect("run");
    let created = report.created.mean;
    assert_eq!(
        created, 4.0,
        "rows are 1 + 2 + 1 items; the table must drive arrivals (got {created})"
    );
    assert_eq!(report.completed.mean, 4.0);
}

#[test]
fn a_table_driven_source_respects_stop_time() {
    let model = load(
        r#"{
          "format": "openflex-model", "version": 1, "name": "table-cutoff",
          "run": { "stop_time": 25.0, "seed": 1, "replications": 1 },
          "global_tables": {
            "Schedule": { "columns": ["time", "quantity"],
              "rows": [[10.0, 1.0], [20.0, 1.0], [30.0, 1.0]] }
          },
          "objects": [
            { "name": "S", "kind": "source",
              "params": { "arrival_schedule_from_table": "Schedule" } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let report = run_model(&model).expect("run");
    assert_eq!(
        report.created.mean, 2.0,
        "only the rows at 10 and 20 land before t=25"
    );
}

#[test]
fn editing_the_table_changes_the_arrivals() {
    // This is the GUI table editor's data path: mutate the table, re-run.
    let mut model = load(
        r#"{
          "format": "openflex-model", "version": 1, "name": "table-edit",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "global_tables": {
            "Schedule": { "columns": ["time", "quantity"],
              "rows": [[10.0, 1.0], [20.0, 1.0]] }
          },
          "objects": [
            { "name": "S", "kind": "source",
              "params": { "arrival_schedule_from_table": "Schedule" } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
    );
    let before = run_model(&model).expect("before").created.mean;
    assert_eq!(before, 2.0);

    let table = model.global_tables.get_mut("Schedule").expect("table");
    table.rows[1][1] = openflex_model::TableCell::Number(3.0);
    let after = run_model(&model).expect("after").created.mean;
    assert_eq!(
        after, 4.0,
        "editing a quantity cell must change the arrivals"
    );

    // And the opposite edit: push the second row past the horizon.
    let table = model.global_tables.get_mut("Schedule").expect("table");
    table.rows[1][0] = openflex_model::TableCell::Number(500.0);
    let late = run_model(&model).expect("late").created.mean;
    assert_eq!(late, 1.0, "a row beyond stop_time contributes nothing");
}
