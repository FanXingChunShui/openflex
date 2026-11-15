//! Script action (`{ "action": "script" }`) and script condition tests.

use openflex_model::{Action, Model, PortStrategy};

fn model(objects: &str, connections: &str) -> String {
    format!(
        r#"{{
  "format": "openflex-model", "version": 1, "name": "script-test",
  "run": {{ "stop_time": 10.0, "warmup": 0.0, "seed": 1, "replications": 1 }},
  "objects": [ {objects} ],
  "connections": [ {connections} ]
}}"#
    )
}

fn source(params: &str) -> String {
    format!(r#"{{ "name": "S", "kind": "source", "params": {{ {params} }} }}"#)
}

fn sink(name: &str) -> String {
    format!(r#"{{ "name": "{name}", "kind": "sink", "params": {{}} }}"#)
}

fn load(json: &str) -> Model {
    Model::from_json(json).expect("model should load")
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
// Action::Script
// ---------------------------------------------------------------------------

#[test]
fn script_action_parses_and_roundtrips() {
    let json = model(
        &format!(
            "{}, {}",
            source(
                r#""triggers": { "on_entry": [ { "action": "script", "source": "set_label(\"priority\", 1.0);" } ] }"#,
            ),
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let parsed = load(&json);
    let action = &parsed.object("S").unwrap().params.triggers().on_entry[0];
    assert_eq!(
        action,
        &Action::Script {
            source: "set_label(\"priority\", 1.0);".into()
        }
    );
    let serialised = serde_json::to_string(&parsed).expect("serialise");
    assert!(serialised.contains("\"action\":\"script\""));
    assert!(serialised.contains("set_label"));
    let reparsed = Model::from_json(&serialised).expect("reparse");
    assert_eq!(parsed, reparsed);
}

#[test]
fn script_action_rejects_unknown_keys() {
    let json = model(
        &format!(
            "{}, {}",
            source(
                r#""triggers": { "on_entry": [ { "action": "script", "source": "log(1);", "code": "x" } ] }"#,
            ),
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = load_error(&json);
    assert!(error.contains("unknown key"), "{error}");
    assert!(error.contains("code"), "{error}");
}

#[test]
fn script_action_requires_source() {
    let json = model(
        &format!(
            "{}, {}",
            source(r#""triggers": { "on_entry": [ { "action": "script" } ] }"#),
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = load_error(&json);
    assert!(error.contains("requires 'source'"), "{error}");
}

#[test]
fn valid_script_action_passes_validation() {
    let json = model(
        &format!(
            "{}, {}",
            source(
                r#""triggers": {
                    "on_entry": [ { "action": "script", "source": "if item_type == 1 { set_label(\"vip\", 1.0); }" } ],
                    "on_exit": [ { "action": "script", "source": "log(\"bye\");" } ] }"#,
            ),
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    load(&json);
}

#[test]
fn invalid_script_action_fails_validation_with_location() {
    let json = model(
        &format!(
            "{}, {}",
            source(
                r#""triggers": { "on_entry": [ { "action": "script", "source": "let x = ;" } ] }"#
            ),
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("script does not compile"), "{error}");
    assert!(
        error.contains("objects.S.triggers.on_entry[0].source"),
        "{error}"
    );
    assert!(error.contains("line"), "{error}");
}

#[test]
fn empty_script_action_fails_validation() {
    let json = model(
        &format!(
            "{}, {}",
            source(r#""triggers": { "on_entry": [ { "action": "script", "source": "   " } ] }"#),
            sink("K")
        ),
        r#"{ "from": "S", "to": "K" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("cannot be empty"), "{error}");
}

// ---------------------------------------------------------------------------
// ConditionalPort.script_condition
// ---------------------------------------------------------------------------

#[test]
fn script_condition_parses_and_roundtrips_without_placeholder() {
    let json = model(
        &format!("{}, {}, {}", source(""), sink("A"), sink("B")),
        r#"{ "from": "S", "to": "A",
             "strategy": { "type": "by_condition", "conditions": [
                 { "script_condition": "item_type == 1", "port": 0 },
                 { "condition": "item_type == 2", "port": 1 }
             ] } },
           { "from": "S", "to": "B", "strategy": { "type": "by_condition", "conditions": [
                 { "script_condition": "label(\"vip\") > 0.0", "port": 0 }
             ] } }"#,
    );
    let parsed = load(&json);
    let Some(PortStrategy::ByCondition { conditions }) = &parsed.connections[0].strategy else {
        panic!("expected by_condition");
    };
    assert_eq!(
        conditions[0].script_condition.as_deref(),
        Some("item_type == 1")
    );
    assert_eq!(conditions[0].condition, openflex_model::Expr::Bool(true));
    assert_eq!(conditions[1].script_condition, None);
    let serialised = serde_json::to_string(&parsed).expect("serialise");
    let condition_json = &serialised[serialised.find("conditions").expect("conditions")..];
    assert!(
        condition_json.contains("script_condition"),
        "{condition_json}"
    );
    let reparsed = Model::from_json(&serialised).expect("reparse");
    assert_eq!(parsed, reparsed);
}

#[test]
fn condition_and_script_condition_are_mutually_exclusive() {
    let json = model(
        &format!("{}, {}, {}", source(""), sink("A"), sink("B")),
        r#"{ "from": "S", "to": "A",
             "strategy": { "type": "by_condition", "conditions": [
                 { "condition": "item_type == 1", "script_condition": "item_type == 1", "port": 0 }
             ] } },
           { "from": "S", "to": "B" }"#,
    );
    let error = load_error(&json);
    assert!(error.contains("mutually exclusive"), "{error}");
}

#[test]
fn script_condition_requires_a_port() {
    let json = model(
        &format!("{}, {}, {}", source(""), sink("A"), sink("B")),
        r#"{ "from": "S", "to": "A",
             "strategy": { "type": "by_condition", "conditions": [
                 { "script_condition": "true" }
             ] } },
           { "from": "S", "to": "B" }"#,
    );
    let error = load_error(&json);
    assert!(error.contains("'port'"), "{error}");
}

#[test]
fn invalid_script_condition_fails_validation_with_location() {
    let json = model(
        &format!("{}, {}, {}", source(""), sink("A"), sink("B")),
        r#"{ "from": "S", "to": "A",
             "strategy": { "type": "by_condition", "conditions": [
                 { "script_condition": "if {", "port": 0 }
             ] } },
           { "from": "S", "to": "B" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("script does not compile"), "{error}");
    assert!(error.contains("conditions[0].script_condition"), "{error}");
}

#[test]
fn empty_script_condition_fails_validation() {
    let json = model(
        &format!("{}, {}, {}", source(""), sink("A"), sink("B")),
        r#"{ "from": "S", "to": "A",
             "strategy": { "type": "by_condition", "conditions": [
                 { "script_condition": "", "port": 0 }
             ] } },
           { "from": "S", "to": "B" }"#,
    );
    let error = validation_error(&json);
    assert!(error.contains("cannot be empty"), "{error}");
}
