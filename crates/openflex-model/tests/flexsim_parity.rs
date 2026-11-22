//! FlexSim 7.5 object-library parity: the objects that exist in the Chinese
//! FlexSim 7.5 object library but were missing from OpenFlex.
//!
//! Every new kind must round-trip through JSON, parse from its snake_case name
//! and validate. The behaviour tests live in `openflex-core`.

use openflex_model::{Model, ObjectKind, ObjectParams};

/// Build a minimal valid model around one extra object's params.
fn model_with(name: &str, kind: ObjectKind, params_json: &str) -> Model {
    let json = format!(
        r#"{{
          "format": "openflex-model", "version": 1, "name": "parity",
          "run": {{"stop_time": 10.0}},
          "objects": [
            {{"name": "S", "kind": "source", "params": {{}}}},
            {{"name": "K", "kind": "sink", "params": {{}}}},
            {{"name": "{name}", "kind": "{kind}", "params": {params_json}}}
          ],
          "connections": [{{"from": "S", "to": "K"}}]
        }}"#,
        kind = kind.as_str()
    );
    // Parse without validating: several fixtures below are deliberately invalid.
    Model::parse_json(&json).unwrap_or_else(|error| panic!("{name}: {error:?}"))
}

#[test]
fn every_new_kind_round_trips() {
    for kind in [
        ObjectKind::Ticker,
        ObjectKind::FluidMixer,
        ObjectKind::FluidBlender,
        ObjectKind::FluidSplitter,
        ObjectKind::FluidProcessor,
        ObjectKind::ItemToFluid,
        ObjectKind::FluidToItem,
        ObjectKind::VisualTool,
        ObjectKind::NetworkNode,
        ObjectKind::TrafficControl,
    ] {
        let text = kind.as_str();
        assert_eq!(
            ObjectKind::parse(text),
            Some(kind),
            "{text} must parse back"
        );
        // An empty params object must decode: every field has a default.
        let model = model_with("Probe", kind, "{}");
        assert!(
            model.validate().is_ok(),
            "{text} must accept an empty params object: {:?}",
            openflex_model::validate_collect(&model)
        );
    }
}

#[test]
fn new_kinds_appear_in_the_catalogue() {
    let all = ObjectKind::all();
    for kind in [
        ObjectKind::Ticker,
        ObjectKind::FluidMixer,
        ObjectKind::FluidBlender,
        ObjectKind::FluidSplitter,
        ObjectKind::FluidProcessor,
        ObjectKind::ItemToFluid,
        ObjectKind::FluidToItem,
        ObjectKind::VisualTool,
        ObjectKind::Motor,
    ] {
        assert!(all.contains(&kind), "{kind:?} missing from ObjectKind::all");
    }
    assert_eq!(
        all.len(),
        28 + 11,
        "28 previous kinds plus the 11 FlexSim 7.5 additions"
    );
}

#[test]
fn ticker_defaults_and_validation() {
    let model = model_with("Clock", ObjectKind::Ticker, "{}");
    assert!(model.validate().is_ok());
    match &model.object("Clock").expect("object").params {
        ObjectParams::Ticker(params) => {
            assert_eq!(params.tick_length, 1.0);
            assert!(params.components.is_empty());
        }
        other => panic!("expected ticker params, got {other:?}"),
    }

    // A non-positive tick is rejected with a located message.
    let bad = model_with("Clock", ObjectKind::Ticker, r#"{"tick_length": 0}"#);
    let issues = openflex_model::validate_collect(&bad);
    assert!(
        issues
            .iter()
            .any(|issue| issue.location.contains("tick_length")),
        "expected a tick_length issue, got {issues:?}"
    );
}

#[test]
fn fluid_mixer_steps_are_validated() {
    let model = model_with(
        "Mix",
        ObjectKind::FluidMixer,
        r#"{"product_id": 3, "steps": [{"pull": [1.0, 2.0], "delay": 0.5}]}"#,
    );
    assert!(model.validate().is_ok());
    match &model.object("Mix").expect("object").params {
        ObjectParams::FluidMixer(params) => {
            assert_eq!(params.product_id, 3.0);
            assert_eq!(params.steps.len(), 1);
            assert_eq!(params.steps[0].pull, vec![1.0, 2.0]);
            assert_eq!(params.steps[0].delay, 0.5);
        }
        other => panic!("expected mixer params, got {other:?}"),
    }

    let bad = model_with(
        "Mix",
        ObjectKind::FluidMixer,
        r#"{"steps": [{"pull": [-1.0], "delay": -2.0}]}"#,
    );
    let issues = openflex_model::validate_collect(&bad);
    assert!(issues.iter().any(|issue| issue.location.contains("pull")));
    assert!(issues.iter().any(|issue| issue.location.contains("delay")));
}

#[test]
fn fluid_splitter_percentages_must_fit_in_a_hundred() {
    let ok = model_with(
        "Split",
        ObjectKind::FluidSplitter,
        r#"{"outputs": [{"description": "left", "percent": 60}, {"percent": 40}]}"#,
    );
    assert!(ok.validate().is_ok());

    let too_much = model_with(
        "Split",
        ObjectKind::FluidSplitter,
        r#"{"outputs": [{"percent": 70}, {"percent": 60}]}"#,
    );
    let issues = openflex_model::validate_collect(&too_much);
    assert!(
        issues
            .iter()
            .any(|issue| issue.message.contains("more than 100")),
        "expected a total-percentage issue, got {issues:?}"
    );

    let single = model_with(
        "Split",
        ObjectKind::FluidSplitter,
        r#"{"outputs": [{"percent": 150}]}"#,
    );
    let issues = openflex_model::validate_collect(&single);
    assert!(
        issues
            .iter()
            .any(|issue| issue.location.contains("percent"))
    );
}

#[test]
fn fluid_processor_rejects_impossible_settings() {
    let bad = model_with(
        "Cook",
        ObjectKind::FluidProcessor,
        r#"{"max_output_rate": 0, "max_capacity": -5, "loss": 1.5}"#,
    );
    let issues = openflex_model::validate_collect(&bad);
    for field in ["max_output_rate", "max_capacity", "loss"] {
        assert!(
            issues.iter().any(|issue| issue.location.contains(field)),
            "{field} should be flagged: {issues:?}"
        );
    }
}

#[test]
fn blender_ratios_and_loss_are_checked() {
    let ok = model_with(
        "Blend",
        ObjectKind::FluidBlender,
        r#"{"output_rate": 2.0, "ratios": [1.0, 3.0], "loss": 0.1}"#,
    );
    assert!(ok.validate().is_ok());

    let zero_ratios = model_with(
        "Blend",
        ObjectKind::FluidBlender,
        r#"{"ratios": [0.0, 0.0]}"#,
    );
    let issues = openflex_model::validate_collect(&zero_ratios);
    assert!(issues.iter().any(|issue| issue.location.contains("ratios")));

    let bad_loss = model_with("Blend", ObjectKind::FluidBlender, r#"{"loss": -0.5}"#);
    let issues = openflex_model::validate_collect(&bad_loss);
    assert!(issues.iter().any(|issue| issue.location.contains("loss")));
}

#[test]
fn the_two_bridges_validate_their_conversion() {
    let ok_to_fluid = model_with(
        "Bag",
        ObjectKind::ItemToFluid,
        r#"{"fluid_per_unit": 2.5, "units_per_item": 10.0}"#,
    );
    assert!(ok_to_fluid.validate().is_ok());
    match &ok_to_fluid.object("Bag").expect("object").params {
        ObjectParams::ItemToFluid(params) => {
            assert_eq!(params.fluid_per_unit, 2.5);
            assert_eq!(params.units_per_item, 10.0);
        }
        other => panic!("expected item_to_fluid params, got {other:?}"),
    }

    let ok_to_item = model_with(
        "Pack",
        ObjectKind::FluidToItem,
        r#"{"fluid_per_item": 4.0}"#,
    );
    assert!(ok_to_item.validate().is_ok());

    let bad = model_with(
        "Bag",
        ObjectKind::ItemToFluid,
        r#"{"fluid_per_unit": 0, "units_per_item": -1}"#,
    );
    let issues = openflex_model::validate_collect(&bad);
    for field in ["fluid_per_unit", "units_per_item"] {
        assert!(issues.iter().any(|issue| issue.location.contains(field)));
    }
}

#[test]
fn visual_tool_checks_size_and_colour() {
    let ok = model_with(
        "Scenery",
        ObjectKind::VisualTool,
        r##"{"shape": "sphere", "size": [3, 4, 5], "color": "#336699", "text": "dock"}"##,
    );
    assert!(ok.validate().is_ok());
    match &ok.object("Scenery").expect("object").params {
        ObjectParams::VisualTool(params) => {
            assert_eq!(params.shape.as_str(), "sphere");
            assert_eq!(params.size, [3.0, 4.0, 5.0]);
            assert_eq!(params.color.as_deref(), Some("#336699"));
            assert_eq!(params.text.as_deref(), Some("dock"));
        }
        other => panic!("expected visual_tool params, got {other:?}"),
    }

    let bad = model_with(
        "Scenery",
        ObjectKind::VisualTool,
        r##"{"size": [0, 1, 1], "color": "blue"}"##,
    );
    let issues = openflex_model::validate_collect(&bad);
    assert!(issues.iter().any(|issue| issue.location.contains("size")));
    assert!(issues.iter().any(|issue| issue.location.contains("color")));
}

#[test]
fn new_kinds_survive_a_save_and_load_cycle() {
    let model = model_with(
        "Mix",
        ObjectKind::FluidMixer,
        r#"{"product_id": 7, "steps": [{"pull": [1.5], "delay": 2}]}"#,
    );
    let text = model.to_json().expect("serialise");
    let back = Model::from_json(&text).expect("parse");
    assert_eq!(
        back, model,
        "the new kinds must round-trip byte-identically"
    );
}
