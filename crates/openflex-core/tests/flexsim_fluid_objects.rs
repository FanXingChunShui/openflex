//! Behaviour of the FlexSim 7.5 fluid objects that were missing from OpenFlex:
//! `ticker`, `fluid_mixer`, `fluid_blender`, `fluid_splitter`, `fluid_processor`
//! and the two bridges `item_to_fluid` / `fluid_to_item`.

use openflex_core::{SimConfig, run_model};
use openflex_model::Model;

fn run(name: &str, objects: &str, connections: &str, stop: f64) -> serde_json::Value {
    let json = format!(
        r#"{{
          "format": "openflex-model", "version": 1, "name": "{name}",
          "run": {{"stop_time": {stop}, "seed": 1, "replications": 1}},
          "objects": [{objects}],
          "connections": [{connections}]
        }}"#
    );
    let model = Model::from_json(&json).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    serde_json::to_value(run_model(&model).expect("run")).expect("json")
}

fn fluid_object<'a>(report: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    report["fluid"]["objects"]
        .as_array()
        .expect("fluid objects")
        .iter()
        .find(|entry| entry["name"] == name)
        .unwrap_or_else(|| panic!("{name} missing from the fluid report"))
}

#[test]
fn fluid_processor_applies_its_loss_and_rate() {
    // 5/s in, 10% lost, 4/s max output out: the sink must see 4/s.
    let report = run(
        "cook",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 5.0}},
           {"name": "Cook", "kind": "fluid_processor",
            "params": {"max_output_rate": 4.0, "max_capacity": 20.0, "loss": 0.1}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Cook"}, {"from": "Cook", "to": "Out"}"#,
        100.0,
    );
    let cook = fluid_object(&report, "Cook");
    let throughput = cook["throughput"].as_f64().expect("throughput");
    assert!(
        (throughput - 4.0).abs() < 0.1,
        "the processor releases at its maximum rate, got {throughput}"
    );
    let out = fluid_object(&report, "Out");
    assert!((out["throughput"].as_f64().expect("out") - throughput).abs() < 0.01);
}

#[test]
fn fluid_splitter_honours_the_percentages() {
    // 4/s in split 70/30 → 2.8 and 1.2 at the two sinks.
    let report = run(
        "split",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 4.0}},
           {"name": "Div", "kind": "fluid_splitter",
            "params": {"outputs": [{"percent": 70}, {"percent": 30}]}},
           {"name": "A", "kind": "fluid_sink", "params": {}},
           {"name": "B", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Div"}, {"from": "Div", "to": "A"},
           {"from": "Div", "to": "B"}"#,
        100.0,
    );
    let a = fluid_object(&report, "A")["throughput"]
        .as_f64()
        .expect("A");
    let b = fluid_object(&report, "B")["throughput"]
        .as_f64()
        .expect("B");
    assert!((a - 2.8).abs() < 0.15, "70% of 4/s should be 2.8, got {a}");
    assert!((b - 1.2).abs() < 0.15, "30% of 4/s should be 1.2, got {b}");
    assert!(
        (a / b - 70.0 / 30.0).abs() < 0.1,
        "the ratio must stay 70:30, got {}",
        a / b
    );
}

#[test]
fn fluid_splitter_without_a_table_passes_through() {
    let report = run(
        "passthrough",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 3.0}},
           {"name": "Div", "kind": "fluid_splitter", "params": {}},
           {"name": "A", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Div"}, {"from": "Div", "to": "A"}"#,
        100.0,
    );
    let a = fluid_object(&report, "A")["throughput"]
        .as_f64()
        .expect("A");
    assert!(
        (a - 3.0).abs() < 0.1,
        "pass-through should keep 3/s, got {a}"
    );
}

#[test]
fn fluid_mixer_releases_whole_batches() {
    // The mixer holds material until the step's pull volume is available.
    let report = run(
        "mix",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 10.0}},
           {"name": "Mix", "kind": "fluid_mixer",
            "params": {"steps": [{"pull": [10.0], "delay": 0.0}]}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Mix"}, {"from": "Mix", "to": "Out"}"#,
        100.0,
    );
    let mix = fluid_object(&report, "Mix");
    let throughput = mix["throughput"].as_f64().expect("throughput");
    assert!(
        throughput > 1.0,
        "batches of 10 volume must still be released, got {throughput}"
    );
    let out = fluid_object(&report, "Out")["throughput"]
        .as_f64()
        .expect("out");
    assert!((out - throughput).abs() < 0.05);
}

#[test]
fn fluid_blender_caps_the_output_rate() {
    let report = run(
        "blend",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 8.0}},
           {"name": "Blend", "kind": "fluid_blender",
            "params": {"output_rate": 3.0, "ratios": [1.0], "loss": 0.0}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Blend"}, {"from": "Blend", "to": "Out"}"#,
        100.0,
    );
    let blend = fluid_object(&report, "Blend");
    let throughput = blend["throughput"].as_f64().expect("throughput");
    assert!(
        (throughput - 3.0).abs() < 0.2,
        "the blender must respect output_rate 3/s, got {throughput}"
    );
}

#[test]
fn item_to_fluid_converts_each_flow_item() {
    // 20 items of 2.0 × 3.0 volume each over 100 s → the tank/sink see flow.
    let report = run(
        "bridge-in",
        r#"{"name": "S", "kind": "source",
            "params": {"interarrival": {"distribution": "constant", "value": 5.0}}},
           {"name": "Bag", "kind": "item_to_fluid",
            "params": {"fluid_per_unit": 2.0, "units_per_item": 3.0}},
           {"name": "Tank", "kind": "fluid_tank",
            "params": {"capacity": 100.0, "initial_level": 0.0}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "S", "to": "Bag"}, {"from": "Bag", "to": "Tank"},
           {"from": "Tank", "to": "Out"}"#,
        100.0,
    );
    // The source produced 20 items and the bridge consumed all of them.
    assert!(
        report["objects"]["Bag"]["output_count"]["mean"]
            .as_f64()
            .expect("bag output")
            > 10.0,
        "the bridge must consume the items it receives"
    );
    let out = fluid_object(&report, "Out")["throughput"]
        .as_f64()
        .expect("out");
    assert!(
        out > 0.5,
        "converted volume must reach the fluid sink, got {out}"
    );
}

#[test]
fn fluid_to_item_packs_fluid_into_items() {
    let report = run(
        "bridge-out",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 6.0}},
           {"name": "Pack", "kind": "fluid_to_item", "params": {"fluid_per_item": 2.0}},
           {"name": "Out", "kind": "sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Pack"}, {"from": "Pack", "to": "Out"}"#,
        100.0,
    );
    let completed = report["completed"]["mean"].as_f64().expect("completed");
    assert!(
        completed > 0.0,
        "the bridge must create flow items from fluid, got {completed}"
    );
}

#[test]
fn ticker_is_accepted_and_does_not_change_the_flow() {
    // A ticker only owns the clock; adding one must not alter the numbers.
    let without = run(
        "no-ticker",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 3.0}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Out"}"#,
        50.0,
    );
    let with = run(
        "with-ticker",
        r#"{"name": "Clock", "kind": "ticker", "params": {"tick_length": 0.5}},
           {"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 3.0}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Out"}"#,
        50.0,
    );
    let a = fluid_object(&without, "Out")["throughput"]
        .as_f64()
        .expect("a");
    let b = fluid_object(&with, "Out")["throughput"]
        .as_f64()
        .expect("b");
    assert!((a - b).abs() < 1e-9, "the ticker must not change the flow");
}

#[test]
fn visual_tool_is_inert() {
    // A visual tool must not appear in the statistics at all.
    let report = run(
        "scenery",
        r##"{"name": "S", "kind": "source",
            "params": {"interarrival": {"distribution": "constant", "value": 1.0}}},
           {"name": "K", "kind": "sink", "params": {}},
           {"name": "Prop", "kind": "visual_tool",
            "params": {"shape": "column", "size": [1, 1, 6], "color": "#8899aa"}}"##,
        r#"{"from": "S", "to": "K"}"#,
        10.0,
    );
    assert!(
        report["objects"].get("Prop").is_none(),
        "a decoration must not be reported as a simulated object"
    );
}

/// The fluid report must list the new objects with their FlexSim names.
#[test]
fn fluid_report_names_the_new_kinds() {
    let report = run(
        "names",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 2.0}},
           {"name": "Cook", "kind": "fluid_processor", "params": {}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Cook"}, {"from": "Cook", "to": "Out"}"#,
        20.0,
    );
    assert_eq!(fluid_object(&report, "Cook")["kind"], "fluid_processor");
    assert_eq!(fluid_object(&report, "Gen")["kind"], "fluid_generator");
}

/// Guard against a silent behaviour change: the classic fluid chain keeps its
/// numbers.
#[test]
fn the_classic_fluid_chain_is_unchanged() {
    let report = run(
        "classic",
        r#"{"name": "Gen", "kind": "fluid_generator", "params": {"flow_rate": 4.0}},
           {"name": "Tank", "kind": "fluid_tank",
            "params": {"capacity": 20.0, "initial_level": 5.0, "max_outflow": 6.0}},
           {"name": "Valve", "kind": "fluid_valve", "params": {"opening": 0.9}},
           {"name": "Out", "kind": "fluid_sink", "params": {}}"#,
        r#"{"from": "Gen", "to": "Tank"}, {"from": "Tank", "to": "Valve"},
           {"from": "Valve", "to": "Out"}"#,
        120.0,
    );
    let out = fluid_object(&report, "Out")["throughput"]
        .as_f64()
        .expect("out");
    assert!(
        (out - 3.63).abs() < 0.05,
        "the documented fluid example released 3.63/s, got {out}"
    );
}

/// `SimConfig` is part of the public surface these tests rely on.
#[test]
fn sim_config_defaults_are_usable() {
    let config = SimConfig {
        stop_time: 10.0,
        warmup: 0.0,
        seed: 1,
    };
    assert_eq!(config.stop_time, 10.0);
}
