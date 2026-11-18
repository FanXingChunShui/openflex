//! Object orientation (`rot`) tests.

use openflex_model::Model;

fn model(objects: &str) -> String {
    format!(
        r#"{{
  "format": "openflex-model", "version": 1, "name": "rotation",
  "run": {{ "stop_time": 10.0 }},
  "objects": [ {objects} ],
  "connections": []
}}"#
    )
}

fn source(extra: &str) -> String {
    if extra.is_empty() {
        r#"{ "name": "S", "kind": "source", "params": {} }"#.to_string()
    } else {
        format!(r#"{{ "name": "S", "kind": "source", "params": {{}}, {extra} }}"#)
    }
}

fn two_objects(source_extra: &str) -> String {
    model(&format!(
        "{}, {{ \"name\": \"K\", \"kind\": \"sink\", \"params\": {{}} }}",
        source(source_extra)
    ))
}

#[test]
fn rotation_is_optional_and_omitted_by_default() {
    let parsed = Model::from_json(&two_objects("")).expect("model loads");
    let source = parsed.object("S").expect("source");
    assert_eq!(source.rot, None);

    let value = serde_json::to_value(&parsed).expect("serialise");
    assert!(
        value["objects"][0].get("rot").is_none(),
        "rot must be omitted when unset: {value}"
    );

    // A programmatically built object starts without a rotation.
    let built = openflex_model::Object::new(
        "P",
        openflex_model::ObjectParams::Processor(Default::default()),
    );
    assert_eq!(built.rot, None);
}

#[test]
fn rotation_roundtrips_through_json() {
    let parsed = Model::from_json(&two_objects(r#""rot": -45.5"#)).expect("model loads");
    assert_eq!(
        parsed.object("S").and_then(|object| object.rot),
        Some(-45.5)
    );

    let text = serde_json::to_string(&parsed).expect("serialise");
    assert!(text.contains("\"rot\":-45.5"), "{text}");
    let reparsed = Model::from_json(&text).expect("reparse");
    assert_eq!(parsed, reparsed);
    assert_eq!(
        reparsed.object("S").and_then(|object| object.rot),
        Some(-45.5)
    );
}

#[test]
fn rotation_zero_is_preserved_when_explicit() {
    let parsed = Model::from_json(&two_objects(r#""rot": 0"#)).expect("model loads");
    assert_eq!(parsed.object("S").and_then(|object| object.rot), Some(0.0));
    let text = serde_json::to_string(&parsed).expect("serialise");
    assert!(text.contains("\"rot\":0.0"), "{text}");
}

#[test]
fn non_finite_rotation_is_rejected_by_validation() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut parsed = Model::from_json(&two_objects("")).expect("model loads");
        parsed.objects[0].rot = Some(bad);
        let error = parsed
            .validate()
            .expect_err("non-finite rotation must be rejected")
            .to_string();
        assert!(error.contains("objects.S.rot"), "{error}");
        assert!(error.contains("finite"), "{error}");
    }
}

#[test]
fn finite_rotation_validates() {
    let mut parsed = Model::from_json(&two_objects(r#""rot": 359.9"#)).expect("model loads");
    parsed.validate().expect("finite rotation is valid");
    // Any finite value is legal; the engine/renderer normalises it.
    parsed.objects[0].rot = Some(720.0);
    parsed.validate().expect("unnormalised rotation is valid");
}

#[test]
fn unknown_object_keys_still_mention_rot() {
    let error = Model::from_json(&two_objects(r#""rotation": 90"#))
        .expect_err("unknown key")
        .to_string();
    assert!(error.contains("rotation"), "{error}");
    assert!(error.contains("rot"), "{error}");
    assert!(error.contains("object 'S'"), "{error}");
}
