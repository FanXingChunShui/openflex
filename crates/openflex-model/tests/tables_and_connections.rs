//! Global tables, full trigger enum, A/S connections and migration behaviour.

use openflex_model::{ConnectionKind, Model, TableCell};

fn model(objects: &str, connections: &str) -> String {
    format!(
        r#"{{
  "format": "openflex-model", "version": 1, "name": "task38",
  "run": {{ "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
  "objects": [ {objects} ],
  "connections": [ {connections} ]
}}"#
    )
}

fn source() -> &'static str {
    r#"{ "name": "S", "kind": "source", "params": {} }"#
}

fn sink(name: &str) -> String {
    format!(r#"{{ "name": "{name}", "kind": "sink", "params": {{}} }}"#)
}

fn processor(name: &str) -> String {
    format!(r#"{{ "name": "{name}", "kind": "processor", "params": {{}} }}"#)
}

fn dispatcher(name: &str) -> String {
    format!(r#"{{ "name": "{name}", "kind": "dispatcher", "params": {{}} }}"#)
}

fn with_tables(tables: &str) -> String {
    model(
        &format!("{}, {}", source(), sink("K")),
        r#"{ "from": "S", "to": "K" }"#,
    )
    .replace(
        r#""objects""#,
        &format!(r#""global_tables": {{ {tables} }}, "objects""#),
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
// Global tables
// ---------------------------------------------------------------------------

#[test]
fn global_table_parses_and_is_reachable() {
    let json = with_tables(
        r#""Routing": {
            "columns": ["item", "port", "priority"],
            "rows": [
                ["box", 1, 2.5],
                ["tray", 2, true]
            ]
        }"#,
    );
    let model = Model::from_json(&json).expect("table model loads");
    let table = model.table("Routing").expect("routing table");
    assert_eq!(table.columns, vec!["item", "port", "priority"]);
    assert_eq!(table.len(), 2);
    assert_eq!(table.width(), 3);
    assert_eq!(table.cell(0, "item"), Some(&TableCell::Str("box".into())));
    assert_eq!(
        table.cell(0, "port").and_then(TableCell::as_number),
        Some(1.0)
    );
    assert_eq!(table.cell(1, "priority"), Some(&TableCell::Bool(true)));
    assert_eq!(table.cell(5, "item"), None);
    assert_eq!(table.cell(0, "missing"), None);
}

#[test]
fn global_tables_roundtrip_through_json() {
    let json = with_tables(r#""T": { "columns": ["n"], "rows": [ [1], ["two"] ] }"#);
    let model = Model::from_json(&json).expect("loads");
    let serialised = serde_json::to_string(&model).expect("serialise");
    assert!(serialised.contains("global_tables"));
    let reparsed = Model::from_json(&serialised).expect("reparse");
    assert_eq!(model, reparsed);
}

#[test]
fn empty_global_tables_are_omitted_from_json() {
    let model = Model::from_json(&model(
        &format!("{}, {}", source(), sink("K")),
        r#"{ "from": "S", "to": "K" }"#,
    ))
    .expect("loads");
    let serialised = serde_json::to_string(&model).expect("serialise");
    assert!(!serialised.contains("global_tables"), "{serialised}");
}

#[test]
fn table_rows_must_match_the_column_count() {
    let json = with_tables(r#""T": { "columns": ["a", "b"], "rows": [ [1] ] }"#);
    let error = validation_error(&json);
    assert!(error.contains("global_tables.T.rows[0]"), "{error}");
    assert!(error.contains("2 columns"), "{error}");
}

#[test]
fn table_columns_must_be_unique_and_non_empty() {
    let duplicate = with_tables(r#""T": { "columns": ["a", "a"], "rows": [] }"#);
    let error = validation_error(&duplicate);
    assert!(error.contains("duplicate column name"), "{error}");

    let empty = with_tables(r#""T": { "columns": [""], "rows": [] }"#);
    let error = validation_error(&empty);
    assert!(error.contains("cannot be empty"), "{error}");
}

#[test]
fn table_with_rows_needs_columns() {
    let json = with_tables(r#""T": { "rows": [ [1] ] }"#);
    let error = validation_error(&json);
    assert!(error.contains("needs column names"), "{error}");
}

#[test]
fn table_names_must_be_identifiers() {
    let json = with_tables(r#""my table": { "columns": [], "rows": [] }"#);
    let error = validation_error(&json);
    assert!(error.contains("global_tables.my table"), "{error}");
}

#[test]
fn duplicate_table_names_are_rejected() {
    let json =
        with_tables(r#""T": { "columns": [], "rows": [] }, "T": { "columns": [], "rows": [] }"#);
    let error = load_error(&json);
    assert!(error.contains("duplicate global table"), "{error}");
    assert!(error.contains("'T'"), "{error}");
}

// ---------------------------------------------------------------------------
// Full trigger enum
// ---------------------------------------------------------------------------

#[test]
fn new_trigger_points_parse_and_default_to_empty() {
    let json = model(
        &format!(
            r#"{{ "name": "S", "kind": "source", "params": {{
                "triggers": {{
                    "on_content_change": [ {{ "action": "log", "source": "never" }} ]
                }}
            }} }}, {}"#,
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    // `log` is not a structured action (it is a script function), so this must
    // fail to parse — using it proves the new field is actually decoded.
    let error = load_error(&json);
    assert!(error.contains("unknown action 'log'"), "{error}");

    let json = model(
        &format!(
            r#"{{ "name": "S", "kind": "source", "params": {{
                "triggers": {{
                    "on_content_change": [ {{ "action": "script", "source": "log(\"changed\");" }} ],
                    "on_state_change": [ {{ "action": "set_label", "name": "state", "value": "1" }} ],
                    "on_reset": [ {{ "action": "release_item" }} ],
                    "on_stop": [ {{ "action": "send_message", "target": "K" }} ],
                    "on_resume": [ {{ "action": "destroy_item" }} ]
                }}
            }} }}, {}"#,
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let parsed = Model::from_json(&json).expect("new triggers parse");
    let triggers = parsed.object("S").unwrap().params.triggers();
    assert_eq!(triggers.on_content_change.len(), 1);
    assert_eq!(triggers.on_state_change.len(), 1);
    assert_eq!(triggers.on_reset.len(), 1);
    assert_eq!(triggers.on_stop.len(), 1);
    assert_eq!(triggers.on_resume.len(), 1);
    // Points that were not given stay empty (backwards compatible default).
    assert!(triggers.on_entry.is_empty());
    assert!(triggers.on_message.is_empty());
}

#[test]
fn new_trigger_points_validate_script_syntax() {
    let json = model(
        &format!(
            r#"{{ "name": "S", "kind": "source", "params": {{
                "triggers": {{
                    "on_reset": [ {{ "action": "script", "source": "let = ;" }} ]
                }}
            }} }}, {}"#,
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("triggers.on_reset[0].source"), "{error}");
    assert!(error.contains("does not compile"), "{error}");
}

#[test]
fn triggers_roundtrip_keeps_the_new_points() {
    let json = model(
        &format!(
            r#"{{ "name": "S", "kind": "source", "params": {{
                "triggers": {{ "on_resume": [ {{ "action": "release_item" }} ] }}
            }} }}, {}"#,
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let model = Model::from_json(&json).expect("loads");
    let serialised = serde_json::to_string(&model).expect("serialise");
    assert!(serialised.contains("on_resume"), "{serialised}");
    let reparsed = Model::from_json(&serialised).expect("reparse");
    assert_eq!(model, reparsed);
}

// ---------------------------------------------------------------------------
// A / S connections
// ---------------------------------------------------------------------------

#[test]
fn connections_default_to_a_and_omit_kind() {
    let model = Model::from_json(&model(
        &format!("{}, {}", source(), sink("K")),
        r#"{ "from": "S", "to": "K" }"#,
    ))
    .expect("loads");
    assert_eq!(model.connections[0].kind, None);
    let serialised = serde_json::to_value(&model).expect("serialise");
    assert!(
        serialised["connections"][0].get("kind").is_none(),
        "{serialised}"
    );
}

#[test]
fn a_connection_kind_is_explicitly_supported() {
    let model = Model::from_json(&model(
        &format!("{}, {}", source(), sink("K")),
        r#"{ "from": "S", "to": "K", "kind": "a" }"#,
    ))
    .expect("loads");
    assert_eq!(model.connections[0].kind, Some(ConnectionKind::A));
}

#[test]
fn s_connection_between_fixed_resources_is_valid() {
    let json = model(
        &format!(
            "{}, {}, {}, {}",
            source(),
            processor("P"),
            dispatcher("D"),
            sink("K")
        ),
        r#"{ "from": "S", "to": "P" },
           { "from": "P", "to": "D", "kind": "s" },
           { "from": "P", "to": "K" }"#,
    );
    let parsed = Model::from_json(&json).expect("S connection validates");
    assert_eq!(parsed.connections[1].kind, Some(ConnectionKind::S));
}

#[test]
fn s_connection_cannot_use_port_routing() {
    for extra in [r#""port": 0"#, r#""strategy": { "type": "round_robin" }"#] {
        let json = model(
            &format!(
                "{}, {}, {}, {}",
                source(),
                processor("P"),
                dispatcher("D"),
                sink("K")
            ),
            &format!(
                r#"{{ "from": "S", "to": "P" }},
                   {{ "from": "P", "to": "D", "kind": "s", {extra} }},
                   {{ "from": "P", "to": "K" }}"#
            ),
        );
        let error = validation_error(&json);
        assert!(error.contains("S connection cannot"), "{error}");
    }
}

#[test]
fn s_connection_cannot_touch_source_or_sink() {
    let json = model(
        &format!("{}, {}", source(), sink("K")),
        r#"{ "from": "S", "to": "K", "kind": "s" }"#,
    );
    let error = validation_error(&json);
    assert!(
        error.contains("sources and sinks cannot be endpoints"),
        "{error}"
    );
}

#[test]
fn s_connection_does_not_consume_output_ports() {
    // The A connection uses implicit port 0; the S connection between the same
    // objects must not trigger a duplicate-port error.
    let json = model(
        &format!(
            "{}, {}, {}, {}",
            source(),
            processor("P"),
            dispatcher("D"),
            sink("K")
        ),
        r#"{ "from": "S", "to": "P" },
           { "from": "P", "to": "D", "kind": "s" },
           { "from": "P", "to": "K", "port": 0 }"#,
    );
    Model::from_json(&json).expect("S connection must not occupy port 0");
}

#[test]
fn unknown_connection_kind_is_rejected() {
    let json = model(
        &format!("{}, {}", source(), sink("K")),
        r#"{ "from": "S", "to": "K", "kind": "x" }"#,
    );
    let error = load_error(&json);
    assert!(
        error.contains("unknown variant") || error.contains("expected"),
        "{error}"
    );
}
