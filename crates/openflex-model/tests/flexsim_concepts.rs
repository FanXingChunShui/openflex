//! FlexSim 7.5 concepts: `ModelingTools/GlobalVariables` and `PickLists/`.
//!
//! These are model-level, read-mostly data blocks: a GUI edits them, the
//! simulation reads them. The tests pin the JSON shape, the round-trip
//! stability (empty collections must not appear in the document) and every
//! validation error.

use openflex_model::{
    GlobalVariable, GlobalVariableKind, Model, PickList, PickListEntry, TableCell,
};

/// Minimal valid model with an optional body of extra top-level fields.
fn model(extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!("{extra}, ")
    };
    format!(
        r#"{{
  "format": "openflex-model", "version": 1, "name": "flexsim-concepts",
  "run": {{ "stop_time": 10.0 }},
  {extra}"objects": [
    {{ "name": "S", "kind": "source", "params": {{}} }},
    {{ "name": "K", "kind": "sink", "params": {{}} }}
  ],
  "connections": [ {{ "from": "S", "to": "K" }} ]
}}"#
    )
}

fn load(extra: &str) -> Model {
    Model::from_json(&model(extra)).expect("model should load and validate")
}

fn parse_error(extra: &str) -> String {
    Model::from_json(&model(extra))
        .expect_err("model should be rejected while parsing")
        .to_string()
}

fn validation_error(extra: &str) -> String {
    let model = Model::parse_json(&model(extra)).expect("model should parse");
    model
        .validate()
        .expect_err("model should fail validation")
        .to_string()
}

const VARIABLES: &str = r#""global_variables": {
    "BatchSize": { "kind": "number", "value": 12 },
    "Recipe": { "kind": "string", "value": "standard" }
}"#;

const PICKLIST: &str = r#""picklists": {
    "Shift": { "entries": [
        { "name": "Day", "value": "1" },
        { "name": "Night", "value": "2" }
    ] }
}"#;

// ---------------------------------------------------------------------------
// Shape, accessors and round-trip
// ---------------------------------------------------------------------------

#[test]
fn empty_collections_are_omitted_and_old_models_still_load() {
    let parsed = load("");
    assert!(parsed.global_variables.is_empty());
    assert!(parsed.picklists.is_empty());
    assert!(parsed.global_variable("Recipe").is_none());
    assert!(parsed.picklist("Shift").is_none());

    let value = serde_json::to_value(&parsed).expect("serialise");
    assert!(
        value.get("global_variables").is_none(),
        "empty global variables must not be written: {value}"
    );
    assert!(
        value.get("picklists").is_none(),
        "empty pick lists must not be written: {value}"
    );

    // Adding an unrelated field (a table) must not sneak the empty maps in.
    let mut with_table = parsed.clone();
    with_table.global_tables.insert(
        "T".into(),
        openflex_model::GlobalTable {
            columns: vec!["a".into()],
            rows: vec![vec![TableCell::Number(1.0)]],
        },
    );
    let value = serde_json::to_value(&with_table).expect("serialise");
    assert!(value.get("global_variables").is_none());
    assert!(value.get("picklists").is_none());
}

#[test]
fn global_variables_and_picklists_roundtrip_through_json() {
    let parsed = load(&format!("{VARIABLES}, {PICKLIST}"));
    let batch = parsed.global_variable("BatchSize").expect("BatchSize");
    assert_eq!(batch.kind, GlobalVariableKind::Number);
    assert_eq!(batch.as_number(), Some(12.0));
    let recipe = parsed.global_variable("Recipe").expect("Recipe");
    assert_eq!(recipe.kind, GlobalVariableKind::String);
    assert_eq!(recipe.as_str(), Some("standard"));

    let shift = parsed.picklist("Shift").expect("Shift");
    assert_eq!(shift.len(), 2);
    assert_eq!(shift.value_of("Night"), Some("2"));
    assert_eq!(shift.value_of("Missing"), None);

    let text = serde_json::to_string(&parsed).expect("serialise");
    assert!(text.contains("\"global_variables\""), "{text}");
    assert!(text.contains("\"picklists\""), "{text}");
    let reparsed = Model::from_json(&text).expect("reparse");
    assert_eq!(parsed, reparsed, "round-trip must be stable");
}

#[test]
fn programmatic_constructors_produce_the_documented_json() {
    let mut parsed = load("");
    parsed
        .global_variables
        .insert("Rate".into(), GlobalVariable::number(2.5));
    parsed
        .global_variables
        .insert("Tag".into(), GlobalVariable::string("blue"));
    parsed.picklists.insert(
        "Colors".into(),
        PickList::new([("Red", "1"), ("Blue", "2")]),
    );

    let value = serde_json::to_value(&parsed).expect("serialise");
    assert_eq!(
        value["global_variables"]["Rate"],
        serde_json::json!({ "kind": "number", "value": 2.5 })
    );
    assert_eq!(
        value["global_variables"]["Tag"],
        serde_json::json!({ "kind": "string", "value": "blue" })
    );
    assert_eq!(
        value["picklists"]["Colors"]["entries"][1],
        serde_json::json!({ "name": "Blue", "value": "2" })
    );

    // Mutable accessors behave like the read-only ones.
    parsed.global_variable_mut("Rate").expect("Rate").value = serde_json::json!(3.0);
    assert_eq!(
        parsed
            .global_variable("Rate")
            .and_then(GlobalVariable::as_number),
        Some(3.0)
    );
    parsed
        .picklist_mut("Colors")
        .expect("Colors")
        .entries
        .push(PickListEntry {
            name: "Green".into(),
            value: "3".into(),
        });
    assert_eq!(parsed.picklist("Colors").map(PickList::len), Some(3));
}

#[test]
fn duplicate_definition_names_are_rejected_while_parsing() {
    let error = parse_error(
        r#""picklists": {
            "Shift": { "entries": [ { "name": "Day", "value": "1" } ] },
            "Shift": { "entries": [ { "name": "Night", "value": "2" } ] }
        }"#,
    );
    assert!(error.contains("duplicate pick list"), "{error}");
    assert!(error.contains("'Shift'"), "{error}");

    let error = parse_error(
        r#""global_variables": { "Rate": { "kind": "number", "value": 1 },
                                  "Rate": { "kind": "number", "value": 2 } }"#,
    );
    assert!(error.contains("duplicate global variable"), "{error}");
}

#[test]
fn unknown_keys_inside_the_new_blocks_are_rejected() {
    let error = parse_error(
        r#""global_variables": { "Rate": { "kind": "number", "value": 1, "unit": "s" } }"#,
    );
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("unit"), "{error}");

    let error = parse_error(r#""picklists": { "Shift": { "entries": [], "title": "Shift" } }"#);
    assert!(error.contains("unknown field"), "{error}");
    assert!(error.contains("title"), "{error}");
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[test]
fn names_must_be_identifiers() {
    let error = validation_error(
        r#""global_variables": { "batch size": { "kind": "number", "value": 1 } }"#,
    );
    assert!(error.contains("global_variables.batch size"), "{error}");
    assert!(
        error.contains("must start with a letter or underscore"),
        "{error}"
    );

    let error = validation_error(
        r#""picklists": { "2-shift": { "entries": [ { "name": "Day", "value": "1" } ] } }"#,
    );
    assert!(error.contains("picklists.2-shift"), "{error}");
    assert!(
        error.contains("must start with a letter or underscore"),
        "{error}"
    );
}

#[test]
fn global_variable_kind_must_match_the_value() {
    let error = validation_error(
        r#""global_variables": { "Rate": { "kind": "number", "value": "fast" } }"#,
    );
    assert!(error.contains("global_variables.Rate.value"), "{error}");
    assert!(error.contains("kind is 'number'"), "{error}");
    assert!(error.contains("a string"), "{error}");

    let error =
        validation_error(r#""global_variables": { "Recipe": { "kind": "string", "value": 5 } }"#);
    assert!(error.contains("global_variables.Recipe.value"), "{error}");
    assert!(error.contains("kind is 'string'"), "{error}");
    assert!(error.contains("a number"), "{error}");
}

#[test]
fn picklist_entry_names_must_be_unique_and_non_empty() {
    let error = validation_error(
        r#""picklists": { "Shift": { "entries": [
            { "name": "Day", "value": "1" },
            { "name": "Day", "value": "2" }
        ] } }"#,
    );
    assert!(error.contains("picklists.Shift.entries[1].name"), "{error}");
    assert!(error.contains("duplicate entry name 'Day'"), "{error}");

    let error = validation_error(
        r#""picklists": { "Shift": { "entries": [ { "name": "  ", "value": "1" } ] } }"#,
    );
    assert!(error.contains("picklists.Shift.entries[0].name"), "{error}");
    assert!(error.contains("cannot be empty"), "{error}");
}

#[test]
fn picklist_needs_at_least_one_entry() {
    let error = validation_error(r#""picklists": { "Empty": { "entries": [] } }"#);
    assert!(error.contains("picklists.Empty"), "{error}");
    assert!(error.contains("at least one entry"), "{error}");

    // Omitting `entries` entirely is the same as an empty list.
    let error = validation_error(r#""picklists": { "Bare": {} }"#);
    assert!(error.contains("picklists.Bare"), "{error}");
    assert!(error.contains("at least one entry"), "{error}");
}

#[test]
fn valid_definitions_pass_and_collect_no_issues() {
    let parsed = load(&format!("{VARIABLES}, {PICKLIST}"));
    parsed.validate().expect("valid concepts");

    // Every problem is reported together, with the field location.
    let model = Model::parse_json(&model(
        r#""global_variables": {
            "bad name": { "kind": "number", "value": true },
            "Typed": { "kind": "string", "value": 3 }
        },
        "picklists": { "Empty": { "entries": [] } }"#,
    ))
    .expect("parses");
    let issues = openflex_model::validate_collect(&model);
    assert_eq!(issues.len(), 4, "{issues:?}");
    let locations: Vec<&str> = issues.iter().map(|issue| issue.location.as_str()).collect();
    assert!(locations.contains(&"global_variables.bad name"));
    assert!(locations.contains(&"global_variables.bad name.value"));
    assert!(locations.contains(&"global_variables.Typed.value"));
    assert!(locations.contains(&"picklists.Empty"));
}

// ---------------------------------------------------------------------------
// JSON Schema coverage (task-151)
// ---------------------------------------------------------------------------

/// A deliberately small JSON Schema evaluator for the keywords the model
/// schema uses on these blocks. It exists so the tests can check *documents*
/// against `json_schema()` without adding a validation crate; keywords this
/// model does not need (`allOf`/`if`/`then`) are ignored, which is exactly the
/// "top-level structure" check wanted here.
fn check(
    schema: &serde_json::Value,
    instance: &serde_json::Value,
    root: &serde_json::Value,
    path: &str,
) -> Vec<String> {
    // `$ref` to a local definition.
    let mut resolved = schema;
    while let Some(reference) = resolved.get("$ref").and_then(|value| value.as_str()) {
        let name = reference
            .strip_prefix("#/$defs/")
            .unwrap_or_else(|| panic!("unsupported $ref {reference}"));
        resolved = &root["$defs"][name];
    }

    let mut errors = Vec::new();

    if let Some(options) = resolved.get("oneOf").and_then(|value| value.as_array()) {
        let matched = options
            .iter()
            .filter(|option| check(option, instance, root, path).is_empty())
            .count();
        if matched != 1 {
            errors.push(format!("{path}: matched {matched} oneOf branches"));
        }
    }
    if let Some(allowed) = resolved.get("enum").and_then(|value| value.as_array())
        && !allowed.contains(instance)
    {
        errors.push(format!("{path}: {:?} is not in the enum", instance));
    }
    if let Some(expected) = resolved.get("const")
        && expected != instance
    {
        errors.push(format!("{path}: expected const {expected}"));
    }
    if let Some(types) = resolved.get("type") {
        let types: Vec<&str> = match types {
            serde_json::Value::String(one) => vec![one.as_str()],
            serde_json::Value::Array(many) => many.iter().filter_map(|t| t.as_str()).collect(),
            _ => panic!("bad type keyword"),
        };
        let ok = types.iter().any(|kind| match *kind {
            "object" => instance.is_object(),
            "array" => instance.is_array(),
            "string" => instance.is_string(),
            "number" => instance.is_number(),
            "integer" => instance.as_f64().is_some_and(|n| n.fract() == 0.0),
            "boolean" => instance.is_boolean(),
            "null" => instance.is_null(),
            other => panic!("unsupported type {other}"),
        });
        if !ok {
            errors.push(format!("{path}: expected type {types:?}"));
            return errors;
        }
    }

    if let Some(object) = instance.as_object() {
        let properties = resolved
            .get("properties")
            .and_then(|value| value.as_object());
        for required in resolved
            .get("required")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter_map(|name| name.as_str())
        {
            if !object.contains_key(required) {
                errors.push(format!("{path}: missing required '{required}'"));
            }
        }
        for (key, value) in object {
            match properties.and_then(|properties| properties.get(key)) {
                Some(property) => {
                    errors.extend(check(property, value, root, &format!("{path}.{key}")))
                }
                None => match resolved.get("additionalProperties") {
                    Some(serde_json::Value::Bool(false)) => {
                        errors.push(format!("{path}: unknown property '{key}'"))
                    }
                    Some(extra) if extra.is_object() => {
                        errors.extend(check(extra, value, root, &format!("{path}.{key}")))
                    }
                    _ => {}
                },
            }
        }
    }

    if let Some(items) = instance.as_array() {
        if let Some(minimum) = resolved.get("minItems").and_then(|value| value.as_u64())
            && (items.len() as u64) < minimum
        {
            errors.push(format!("{path}: needs at least {minimum} items"));
        }
        if let Some(item_schema) = resolved.get("items") {
            for (index, item) in items.iter().enumerate() {
                errors.extend(check(item_schema, item, root, &format!("{path}[{index}]")));
            }
        }
    }

    errors
}

fn schema() -> serde_json::Value {
    openflex_model::json_schema()
}

/// The example document from the task-149 tests, with both concept blocks.
fn concept_document() -> serde_json::Value {
    serde_json::from_str(&model(&format!("{VARIABLES}, {PICKLIST}"))).expect("document")
}

#[test]
fn schema_defines_global_variables_and_picklists() {
    let schema = schema();
    let defs = &schema["$defs"];

    assert_eq!(
        defs["global_variable"]["required"],
        serde_json::json!(["kind", "value"])
    );
    assert_eq!(
        defs["global_variable"]["properties"]["kind"]["enum"],
        serde_json::json!(["number", "string"])
    );
    assert_eq!(defs["global_variable"]["additionalProperties"], false);

    assert_eq!(defs["picklist"]["required"], serde_json::json!(["entries"]));
    assert_eq!(defs["picklist"]["properties"]["entries"]["minItems"], 1);
    assert_eq!(
        defs["picklist"]["properties"]["entries"]["items"]["$ref"],
        "#/$defs/pick_list_entry"
    );
    assert_eq!(
        defs["pick_list_entry"]["required"],
        serde_json::json!(["name", "value"])
    );
    assert_eq!(
        defs["pick_list_entry"]["properties"]["name"]["type"],
        "string"
    );
    assert_eq!(defs["pick_list_entry"]["additionalProperties"], false);

    let model_properties = &defs["model"]["properties"];
    assert_eq!(
        model_properties["global_variables"]["additionalProperties"]["$ref"],
        "#/$defs/global_variable"
    );
    assert_eq!(
        model_properties["picklists"]["additionalProperties"]["$ref"],
        "#/$defs/picklist"
    );
    assert_eq!(defs["model"]["additionalProperties"], false);
}

#[test]
fn schema_accepts_a_document_with_global_variables_and_picklists() {
    let schema = schema();
    let document = concept_document();
    let errors = check(&schema, &document, &schema, "model");
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn schema_rejects_malformed_global_variables_and_picklists() {
    let schema = schema();
    let base = concept_document();
    let errors_for = |mutate: &dyn Fn(&mut serde_json::Value)| {
        let mut document = base.clone();
        mutate(&mut document);
        check(&schema, &document, &schema, "model")
    };

    // A variable without `kind` (or without `value`) is incomplete.
    let errors = errors_for(&|document| {
        document["global_variables"]["BatchSize"]
            .as_object_mut()
            .expect("object")
            .remove("kind");
    });
    assert!(
        errors
            .iter()
            .any(|error| error.contains("missing required 'kind'")),
        "{errors:#?}"
    );

    // Unknown keys inside a variable are rejected.
    let errors = errors_for(&|document| {
        document["global_variables"]["Recipe"]["unit"] = serde_json::json!("s");
    });
    assert!(
        errors
            .iter()
            .any(|error| error.contains("unknown property 'unit'")),
        "{errors:#?}"
    );

    // An empty pick list violates `minItems: 1`.
    let errors = errors_for(&|document| {
        document["picklists"]["Shift"]["entries"] = serde_json::json!([]);
    });
    assert!(
        errors
            .iter()
            .any(|error| error.contains("needs at least 1 items")),
        "{errors:#?}"
    );

    // Entry names and values must be strings.
    let errors = errors_for(&|document| {
        document["picklists"]["Shift"]["entries"][0]["name"] = serde_json::json!(1);
    });
    assert!(
        errors
            .iter()
            .any(|error| error.contains("expected type [\"string\"]")),
        "{errors:#?}"
    );

    // The top-level key itself is part of the schema (`additionalProperties`).
    let errors = errors_for(&|document| {
        document["picklists"] = serde_json::json!({});
        document["picklists_typo"] = serde_json::json!({});
    });
    assert!(
        errors
            .iter()
            .any(|error| error.contains("unknown property 'picklists_typo'")),
        "{errors:#?}"
    );

    // Note: `kind`/`value` consistency is deliberately *not* in the schema
    // (`"value": {}` accepts any scalar); it is enforced by validate() and
    // covered by `global_variable_kind_must_match_the_value`.
    let loose = serde_json::json!({ "kind": "number", "value": "oops" });
    let errors = check(
        &schema["$defs"]["global_variable"],
        &loose,
        &schema,
        "global_variables.X",
    );
    assert!(
        errors.is_empty(),
        "schema leaves value typing to validate: {errors:#?}"
    );
}

// ---------------------------------------------------------------------------
// Expressions over global variables and pick lists (task-153)
// ---------------------------------------------------------------------------

/// A document with one number variable, one string variable, a mixed pick list
/// and a bare number variable used through the implicit spelling.
fn expression_model() -> Model {
    Model::from_json(&model(&format!(
        r#"{VARIABLES},
        "picklists": {{
            "Shift": {{ "entries": [
                {{ "name": "Day", "value": "1" }},
                {{ "name": "Night", "value": "2" }}
            ] }},
            "Recipe": {{ "entries": [ {{ "name": "Standard", "value": "std" }} ] }}
        }}"#
    )))
    .expect("expression model")
}

fn eval_in(
    model: &Model,
    source: &str,
) -> Result<openflex_model::Value, openflex_model::ExprError> {
    openflex_model::Expr::parse(source)
        .expect("expression parses")
        .eval_in_model(model)
}

#[test]
fn numeric_global_variable_takes_part_in_arithmetic() {
    let model = expression_model();
    assert_eq!(
        eval_in(&model, "$BatchSize * 2 + 1").expect("evaluates"),
        openflex_model::Value::Number(25.0)
    );
    // The bare identifier spelling resolves through the same data (the model
    // has no engine value named `BatchSize`).
    assert_eq!(
        eval_in(&model, "BatchSize / 4").expect("evaluates"),
        openflex_model::Value::Number(3.0)
    );
}

#[test]
fn string_global_variable_takes_part_in_comparison() {
    let model = expression_model();
    assert_eq!(
        eval_in(&model, r#"$Recipe == "standard""#).expect("evaluates"),
        openflex_model::Value::Bool(true)
    );
    assert_eq!(
        eval_in(&model, r#"upper(Recipe) == "STANDARD""#).expect("evaluates"),
        openflex_model::Value::Bool(true)
    );
}

#[test]
fn undefined_global_variable_is_a_strict_error() {
    let model = expression_model();
    let error = eval_in(&model, "$Missing + 1").expect_err("must not be zero");
    assert!(
        error
            .to_string()
            .contains("unknown global variable 'Missing'"),
        "{error}"
    );

    let error = eval_in(&model, "Missing + 1").expect_err("must not be zero");
    assert!(
        error.to_string().contains("unknown variable 'Missing'"),
        "{error}"
    );
}

#[test]
fn string_global_variable_in_arithmetic_is_a_type_error() {
    let model = expression_model();
    let error = eval_in(&model, "$Recipe * 2").expect_err("string is not a number");
    assert!(error.to_string().contains("standard"), "{error}");
    assert!(error.to_string().contains("expected a number"), "{error}");
}

#[test]
fn picklist_lookup_returns_the_entry_value() {
    let model = expression_model();
    assert_eq!(
        eval_in(&model, r#"picklist("Shift", "Night")"#).expect("evaluates"),
        openflex_model::Value::Str("2".into())
    );
    // The value is text, so numeric-looking entries need an explicit cast…
    assert_eq!(
        eval_in(&model, r#"len(picklist("Shift", "Day"))"#).expect("evaluates"),
        openflex_model::Value::Number(1.0)
    );
    // …and text entries compare directly, including inside a condition.
    assert_eq!(
        eval_in(
            &model,
            r#"if(picklist("Recipe", "Standard") == "std", 1, 0)"#
        )
        .expect("evaluates"),
        openflex_model::Value::Number(1.0)
    );
}

#[test]
fn unknown_picklist_and_unknown_entry_are_distinct_errors() {
    let model = expression_model();
    let error = eval_in(&model, r#"picklist("Missing", "Day")"#).expect_err("unknown list");
    assert!(
        error.to_string().contains("unknown pick list 'Missing'"),
        "{error}"
    );

    let error = eval_in(&model, r#"picklist("Shift", "Evening")"#).expect_err("unknown entry");
    assert!(
        error
            .to_string()
            .contains("pick list 'Shift' has no entry 'Evening'"),
        "{error}"
    );
}

#[test]
fn expressions_without_model_data_still_work() {
    // The default trait methods keep contexts without model data strict, and
    // `picklist` is simply unavailable there.
    struct Empty;
    impl openflex_model::EvalContext for Empty {
        fn lookup(&self, _path: &str) -> Option<openflex_model::Value> {
            None
        }
    }
    let error = openflex_model::Expr::parse("1 + 1")
        .expect("parse")
        .eval(&Empty)
        .expect("plain arithmetic still works");
    assert_eq!(error, openflex_model::Value::Number(2.0));
    let error = openflex_model::Expr::parse("$X")
        .expect("parse")
        .eval(&Empty)
        .expect_err("no model data");
    assert!(matches!(
        error,
        openflex_model::ExprError::UnknownGlobalVariable(_)
    ));
    let error = openflex_model::Expr::parse(r#"picklist("Shift", "Day")"#)
        .expect("parse")
        .eval(&Empty)
        .expect_err("no model data");
    assert!(matches!(
        error,
        openflex_model::ExprError::UnknownPickList(_)
    ));
}

#[test]
fn builtin_lists_document_the_new_capabilities() {
    assert!(
        openflex_model::BUILTIN_VARIABLES
            .iter()
            .any(|(name, _)| *name == "$<Name>")
    );
    assert!(
        openflex_model::expr::BUILTIN_FUNCTIONS
            .iter()
            .any(|(name, _)| name.starts_with("picklist("))
    );
    // `$` cannot stand alone.
    assert!(openflex_model::Expr::parse("$ + 1").is_err());
}
