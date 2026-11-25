//! A pick list has to change what the *engine* does, not just what expressions
//! evaluate to. Routing happens through a processor: a `split` object distributes
//! to its outputs itself and ignores connection strategies, so it cannot show
//! whether the condition was honoured. The matrix cited an evaluation-level test, which would have
//! stayed green even if routing ignored the list entirely.

use openflex_core::run_model;
use openflex_model::{Model, PortStrategy};

fn load(json: &str) -> Model {
    Model::from_json(json).expect("model must validate")
}

/// Split into two sinks; port 0 (KA) is taken when the pick-list condition holds.
fn two_sink_model(condition: &str) -> String {
    let escaped = condition.replace('"', "\\\"");
    format!(
        r#"{{
          "format": "openflex-model", "version": 1, "name": "picklist-routing",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "picklists": {{
            "Recipe": {{ "entries": [ {{ "name": "Std", "value": "0" }},
                                     {{ "name": "Alt", "value": "1" }} ] }}
          }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 5.0 }},
                "max_arrivals": 4 }} }},
            {{ "name": "Router", "kind": "processor", "params": {{}} }},
            {{ "name": "KA", "kind": "sink", "params": {{}} }},
            {{ "name": "KB", "kind": "sink", "params": {{}} }}
          ],
          "connections": [
            {{ "from": "S", "to": "Router" }},
            {{ "from": "Router", "to": "KA", "strategy": {{ "type": "by_condition",
              "conditions": [ {{ "condition": "{escaped}", "port": 0 }},
                              {{ "condition": "1 == 1", "port": 1 }} ] }} }},
            {{ "from": "Router", "to": "KB" }}
          ]
        }}"#
    )
}

fn counts(condition: &str) -> (f64, f64) {
    let model = load(&two_sink_model(condition));
    let report = run_model(&model).expect("run");
    (
        report.objects["KA"].input_count.mean,
        report.objects["KB"].input_count.mean,
    )
}

#[test]
fn the_condition_is_a_expression_that_can_read_a_picklist() {
    // Guard the premise: the model really is routing through this condition.
    let model = load(&two_sink_model(
        r#"if(picklist("Recipe", "Std") == "0", 1, 0)"#,
    ));
    let Some(connection) = model
        .connections
        .iter()
        .find(|connection| connection.to == "KA")
    else {
        panic!("the KA connection must exist");
    };
    assert!(
        matches!(&connection.strategy, Some(PortStrategy::ByCondition { .. })),
        "the model must route by condition"
    );
}

#[test]
fn the_picklist_value_decides_which_port_gets_the_item() {
    // A condition that returns a truthy value picks its port (0 = KA); anything
    // falsy falls through to the catch-all, which picks KB.
    assert_eq!(
        counts(r#"if(picklist("Recipe", "Std") == "0", 1, 0)"#),
        (4.0, 0.0),
        "the Std entry is the text 0, so every item should take KA"
    );
    assert_eq!(
        counts(r#"if(picklist("Recipe", "Std") == "1", 1, 0)"#),
        (0.0, 4.0),
        "comparing the same entry to 1 must not match"
    );
    // Reading a different entry flips it back: Alt's value is "1".
    assert_eq!(
        counts(r#"if(picklist("Recipe", "Alt") == "1", 1, 0)"#),
        (4.0, 0.0)
    );
}

#[test]
fn editing_a_picklist_entry_changes_the_outcome() {
    // The editor's data path: same routing condition, different entry value.
    let base = two_sink_model(r#"if(picklist("Recipe", "Std") == "1", 1, 0)"#);
    let model = load(&base);
    let before = run_model(&model).expect("run");
    assert_eq!(
        before.objects["KA"].input_count.mean, 0.0,
        "the list says 0, so a condition looking for 1 sends everything to KB"
    );

    let edited = base.replace(
        r#""name": "Std", "value": "0""#,
        r#""name": "Std", "value": "1""#,
    );
    let model = load(&edited);
    let after = run_model(&model).expect("run");
    assert_eq!(
        after.objects["KA"].input_count.mean, 4.0,
        "changing the entry value must change where items go"
    );
}
