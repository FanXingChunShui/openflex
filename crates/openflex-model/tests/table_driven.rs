//! Writable global tables, table-driven arrival parameters and OnDraw.

use openflex_model::{ArrivalStyle, Model, TableCell};

const ARRIVALS_TABLE: &str = r#""Arrivals": {
    "columns": ["time", "quantity", "item_type", "priority"],
    "rows": [
        [0, 1, 2, 5],
        [10, 2, 2, 7]
    ]
}"#;

fn model(objects: &str, connections: &str, tables: Option<&str>) -> String {
    let tables = tables
        .map(|tables| format!(r#""global_tables": {{ {tables} }}, "#))
        .unwrap_or_default();
    format!(
        r#"{{
  "format": "openflex-model", "version": 1, "name": "table-driven",
  "run": {{ "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
  {tables}"objects": [ {objects} ],
  "connections": [ {connections} ]
}}"#
    )
}

fn source(params: &str) -> String {
    format!(r#"{{ "name": "S", "kind": "source", "params": {{ {params} }} }}"#)
}

fn sink() -> &'static str {
    r#"{ "name": "K", "kind": "sink", "params": {} }"#
}

fn table_model(source_params: &str) -> String {
    model(
        &format!("{}, {}", source(source_params), sink()),
        r#"{ "from": "S", "to": "K" }"#,
        Some(ARRIVALS_TABLE),
    )
}

fn load_error(json: &str) -> String {
    Model::from_json(json)
        .expect_err("model should be rejected")
        .to_string()
}

fn validation_error(json: &str) -> String {
    let model = Model::parse_json(json).expect("model should parse");
    model
        .validate()
        .expect_err("model should fail validation")
        .to_string()
}

// ---------------------------------------------------------------------------
// Writable table API
// ---------------------------------------------------------------------------

fn table_model_for_edit() -> Model {
    Model::from_json(&model(
        &format!("{}, {}", source(""), sink()),
        r#"{ "from": "S", "to": "K" }"#,
        Some(ARRIVALS_TABLE),
    ))
    .expect("model")
}

#[test]
fn set_cell_updates_a_table_in_memory() {
    let mut model = table_model_for_edit();
    model
        .set_table_cell("Arrivals", 0, 1, TableCell::Number(9.0))
        .expect("set cell");
    let table = model.table("Arrivals").expect("table");
    assert_eq!(table.cell(0, "quantity"), Some(&TableCell::Number(9.0)));
    // The rest of the model still validates after the in-memory edit.
    model.validate().expect("edited model validates");
}

#[test]
fn set_cell_rejects_out_of_range_coordinates() {
    let mut model = table_model_for_edit();
    let error = model
        .set_table_cell("Arrivals", 9, 0, TableCell::Number(1.0))
        .expect_err("row out of range");
    assert!(error.to_string().contains("out of range"), "{error}");
    let error = model
        .set_table_cell("Arrivals", 0, 9, TableCell::Number(1.0))
        .expect_err("column out of range");
    assert!(error.to_string().contains("out of range"), "{error}");
}

#[test]
fn set_table_cell_rejects_unknown_tables() {
    let mut model = table_model_for_edit();
    let error = model
        .set_table_cell("Ghost", 0, 0, TableCell::Number(1.0))
        .expect_err("unknown table");
    assert!(
        error.to_string().contains("unknown global table"),
        "{error}"
    );
}

#[test]
fn insert_and_remove_rows() {
    let mut model = table_model_for_edit();
    let table = model.table_mut("Arrivals").expect("table");
    table
        .insert_row(vec![
            TableCell::Number(20.0),
            TableCell::Number(1.0),
            TableCell::Number(9.0),
            TableCell::Number(1.0),
        ])
        .expect("insert");
    assert_eq!(table.len(), 3);
    let error = table
        .insert_row(vec![TableCell::Number(1.0)])
        .expect_err("wrong width");
    assert!(error.contains("columns"), "{error}");
    let removed = table.remove_row(0).expect("remove");
    assert_eq!(removed.len(), 4);
    assert_eq!(table.len(), 2);
    assert!(table.remove_row(99).is_none());
    // The edited table still validates (row order is not checked here because
    // this test does not use the table for arrivals).
    model.validate().expect("edited model validates");
}

#[test]
fn set_cell_on_plain_table_api() {
    let mut table = openflex_model::GlobalTable {
        columns: vec!["a".into(), "b".into()],
        rows: vec![vec![TableCell::Number(0.0), TableCell::Number(0.0)]],
    };
    table
        .set_cell(0, 1, TableCell::Str("text".into()))
        .expect("set");
    assert_eq!(table.cell(0, "b"), Some(&TableCell::Str("text".into())));
    assert!(table.set_cell(0, 2, TableCell::Bool(true)).is_err());
    assert!(table.set_cell(3, 0, TableCell::Bool(true)).is_err());
}

// ---------------------------------------------------------------------------
// Table-driven arrival parameter
// ---------------------------------------------------------------------------

#[test]
fn source_can_load_its_arrival_schedule_from_a_table() {
    let json = table_model(r#""arrival_schedule_from_table": "Arrivals""#);
    let model = Model::from_json(&json).expect("table-driven source loads");
    let params = model.object("S").unwrap().source().expect("source params");
    assert_eq!(
        params.effective_arrival_style(),
        ArrivalStyle::ArrivalSchedule
    );
    assert_eq!(params.arrival_schedule_table(), Some("Arrivals"));
    assert!(
        params.arrival_schedule().is_none(),
        "data lives in the table"
    );

    let schedule = model.apply_table_reference("Arrivals").expect("resolve");
    assert_eq!(schedule.rows.len(), 2);
    assert_eq!(schedule.rows[0].time, 0.0);
    assert_eq!(schedule.rows[0].quantity, 1);
    assert_eq!(schedule.rows[0].item_type, Some(2));
    assert_eq!(schedule.rows[0].labels.get("priority"), Some(&5.0));
    assert_eq!(schedule.rows[1].time, 10.0);
    assert_eq!(schedule.rows[1].quantity, 2);
    assert_eq!(schedule.rows[1].labels.get("priority"), Some(&7.0));
    assert!(!schedule.repeat);
}

#[test]
fn table_driven_source_needs_an_existing_table() {
    let json = table_model(r#""arrival_schedule_from_table": "Missing""#);
    let error = validation_error(&json);
    assert!(error.contains("unknown global table 'Missing'"), "{error}");
}

#[test]
fn table_needs_a_time_column() {
    let json = model(
        &format!(
            "{}, {}",
            source(r#""arrival_schedule_from_table": "Arrivals""#),
            sink()
        ),
        r#"{ "from": "S", "to": "K" }"#,
        Some(r#""Arrivals": { "columns": ["when", "quantity"], "rows": [[0, 1]] }"#),
    );
    let error = validation_error(&json);
    assert!(
        error.contains("needs a 'time' or 'arrival_time' column"),
        "{error}"
    );
}

#[test]
fn table_quantity_must_be_numeric() {
    let json = model(
        &format!(
            "{}, {}",
            source(r#""arrival_schedule_from_table": "Arrivals""#),
            sink()
        ),
        r#"{ "from": "S", "to": "K" }"#,
        Some(r#""Arrivals": { "columns": ["time", "quantity"], "rows": [[0, "many"]] }"#),
    );
    let error = validation_error(&json);
    assert!(error.contains("quantity must be numeric"), "{error}");
}

#[test]
fn table_driven_arrivals_must_be_non_decreasing() {
    let json = model(
        &format!(
            "{}, {}",
            source(r#""arrival_schedule_from_table": "Arrivals""#),
            sink()
        ),
        r#"{ "from": "S", "to": "K" }"#,
        Some(r#""Arrivals": { "columns": ["time"], "rows": [[10], [5]] }"#),
    );
    let error = validation_error(&json);
    assert!(error.contains("non-decreasing"), "{error}");
}

#[test]
fn table_driven_arrivals_are_mutually_exclusive_with_inline_modes() {
    let with_interarrival = table_model(
        r#""interarrival": { "distribution": "constant", "value": 5.0 },
           "arrival_schedule_from_table": "Arrivals""#,
    );
    let error = validation_error(&with_interarrival);
    assert!(error.contains("cannot be combined"), "{error}");

    let with_inline = table_model(
        r#""arrival_schedule_from_table": "Arrivals",
           "arrival_schedule": { "rows": [ { "time": 0.0 } ] }"#,
    );
    let error = validation_error(&with_inline);
    assert!(
        error.contains("mutually exclusive") || error.contains("cannot be combined"),
        "{error}"
    );

    let wrong_style = table_model(
        r#""arrival_style": "inter_arrival_time",
           "arrival_schedule_from_table": "Arrivals""#,
    );
    let error = validation_error(&wrong_style);
    assert!(error.contains("arrival_style"), "{error}");
}

// ---------------------------------------------------------------------------
// OnDraw trigger
// ---------------------------------------------------------------------------

#[test]
fn on_draw_parses_and_defaults_to_empty() {
    let json = model(
        &format!(
            r#"{{ "name": "S", "kind": "source", "params": {{
                "triggers": {{ "on_draw": [ {{ "action": "script", "source": "log(\"draw\");" }} ] }}
            }} }}, {}"#,
            sink()
        ),
        r#"{ "from": "S", "to": "K" }"#,
        None,
    );
    let parsed = Model::from_json(&json).expect("on_draw parses");
    let triggers = parsed.object("S").unwrap().params.triggers();
    assert_eq!(triggers.on_draw.len(), 1);
    assert!(triggers.on_resume.is_empty());
    // Round-trips through the JSON representation.
    let serialised = serde_json::to_string(&parsed).expect("serialise");
    assert!(serialised.contains("on_draw"));
    let reparsed = Model::from_json(&serialised).expect("reparse");
    assert_eq!(parsed, reparsed);
}

#[test]
fn on_draw_script_syntax_is_validated() {
    let json = model(
        &format!(
            r#"{{ "name": "S", "kind": "source", "params": {{
                "triggers": {{ "on_draw": [ {{ "action": "script", "source": "let = ;" }} ] }}
            }} }}, {}"#,
            sink()
        ),
        r#"{ "from": "S", "to": "K" }"#,
        None,
    );
    let error = validation_error(&json);
    assert!(error.contains("triggers.on_draw[0].source"), "{error}");
    assert!(error.contains("does not compile"), "{error}");
}

#[test]
fn unknown_trigger_points_are_still_rejected() {
    let json = model(
        &format!(
            r#"{{ "name": "S", "kind": "source", "params": {{
                "triggers": {{ "on_drw": [] }}
            }} }}, {}"#,
            sink()
        ),
        r#"{ "from": "S", "to": "K" }"#,
        None,
    );
    let error = load_error(&json);
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("on_drw"), "{error}");
}
