//! Process Flow token runtime tests (B7).

use openflex_core::{SimConfig, run_model, simulate_traced};
use openflex_model::Model;

fn token_model() -> Model {
    Model::from_json(include_str!(
        "../../../examples/objects/process-flow-token.ofm"
    ))
    .expect("model")
}

#[test]
fn token_flow_reports_each_block() {
    let report = run_model(&token_model()).expect("run");
    let flow = report.process_flow.as_ref().expect("flow report");
    assert_eq!(flow.tokens_created, 10.0);
    assert_eq!(flow.tokens_completed, 10.0);
    let block = |id: &str| {
        flow.blocks
            .iter()
            .find(|block| block.id == id)
            .unwrap_or_else(|| panic!("missing block {id}"))
    };
    assert_eq!(block("Create1").output_count, 10.0);
    assert_eq!(block("Delay1").input_count, 10.0);
    assert_eq!(block("Delay1").output_count, 10.0);
    assert!((block("Delay1").avg_stay_time - 2.0).abs() < 1e-9);
    assert!((block("Delay1").content_avg - 0.2).abs() < 1e-9);
    assert_eq!(block("Assign1").input_count, 10.0);
    assert_eq!(block("Decide").input_count, 10.0);
    assert_eq!(block("SinkUrgent").input_count, 5.0);
    assert_eq!(block("SinkNormal").input_count, 5.0);
}

#[test]
fn token_flow_traces_token_events() {
    let model = token_model();
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("traced run");
    assert_eq!(result.objects.len(), 0);
    assert!(result.process_flow.is_some());
    for label in [
        "token_create",
        "token_delay",
        "token_assign",
        "token_decision",
        "token_sink",
    ] {
        let count = trace
            .events
            .iter()
            .filter(|event| event.event == label)
            .count();
        assert_eq!(count, 10, "{label}");
    }
    // Token events carry the block name and the token id.
    let first = trace
        .events
        .iter()
        .find(|event| event.event == "token_create")
        .expect("token event");
    assert_eq!(first.object, "Create1");
    assert!(first.data.contains_key("token"));
}

#[test]
fn token_flow_is_optional_in_models_without_a_flow() {
    let model = Model::from_json(
        r#"{
          "format": "openflex-model", "version": 1, "name": "plain",
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 10.0 }, "max_arrivals": 1 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
    )
    .expect("model");
    let report = run_model(&model).expect("run");
    assert!(report.process_flow.is_none());
}

#[test]
fn checkpoint_resume_matches_a_direct_flow_run() {
    use openflex_core::simulate_with_checkpoint;
    let model = token_model();
    let first = SimConfig {
        stop_time: 1.0,
        warmup: 0.0,
        seed: 1,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let second = SimConfig {
        stop_time: 100.0,
        ..first
    };
    let (resumed, _) =
        simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint)).expect("resume");
    let (direct, _) = simulate_with_checkpoint(&model, &second, 0, None).expect("direct");
    assert_eq!(resumed, direct, "flow resume must reproduce the direct run");
}

fn flow_model(blocks: &str, connections: &str) -> String {
    format!(
        r#"{{
          "format": "openflex-model", "version": 1, "name": "flow-validation",
          "objects": [],
          "process_flow": {{ "blocks": [{blocks}], "connections": [{connections}] }}
        }}"#
    )
}

#[test]
fn invalid_flows_are_rejected() {
    // Duplicate block ids.
    let duplicate = flow_model(
        r#"{"id":"A","kind":"create","max_creations":1},{"id":"A","kind":"sink"}"#,
        r#"{"from":"A","to":"A"}"#,
    );
    let error = Model::from_json(&duplicate).expect_err("duplicate ids");
    assert!(error.to_string().contains("duplicate block id"), "{error}");

    // A cycle between two blocks.
    let cycle = flow_model(
        r#"{"id":"C","kind":"create","max_creations":1},
           {"id":"D1","kind":"delay","time":{"distribution":"constant","value":1.0}},
           {"id":"D2","kind":"delay","time":{"distribution":"constant","value":1.0}},
           {"id":"S","kind":"sink"}"#,
        r#"{"from":"C","to":"D1"},{"from":"D1","to":"D2"},{"from":"D2","to":"D1"},
           {"from":"D2","to":"S"}"#,
    );
    let error = Model::from_json(&cycle).expect_err("cycle");
    assert!(error.to_string().contains("acyclic"), "{error}");

    // Transfer blocks are documented as not implemented.
    let transfer = flow_model(
        r#"{"id":"C","kind":"create","max_creations":1},
           {"id":"T","kind":"transfer","object":"SomeQueue"},
           {"id":"S","kind":"sink"}"#,
        r#"{"from":"C","to":"T"},{"from":"T","to":"S"}"#,
    );
    let error = Model::from_json(&transfer).expect_err("transfer");
    assert!(error.to_string().contains("not implemented"), "{error}");

    // Decision ports must exist.
    let port = flow_model(
        r#"{"id":"C","kind":"create","max_creations":1},
           {"id":"D","kind":"decision","conditions":[{"condition":"1 == 1","port":3}]},
           {"id":"S","kind":"sink"}"#,
        r#"{"from":"C","to":"D"},{"from":"D","to":"S","port":0}"#,
    );
    let error = Model::from_json(&port).expect_err("bad port");
    assert!(error.to_string().contains("port"), "{error}");
}

#[test]
fn process_flow_and_objects_coexist() {
    let model = Model::from_json(
        r#"{
          "format": "openflex-model", "version": 1, "name": "flow-and-objects",
          "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 10.0 }, "max_arrivals": 5 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }],
          "process_flow": {
            "blocks": [
              { "id": "C", "kind": "create", "max_creations": 1, "quantity": 3 },
              { "id": "D", "kind": "delay",
                "time": { "distribution": "constant", "value": 1.0 } },
              { "id": "K2", "kind": "sink" }
            ],
            "connections": [
              { "from": "C", "to": "D" }, { "from": "D", "to": "K2" }
            ]
          }
        }"#,
    )
    .expect("model");
    let report = run_model(&model).expect("run");
    assert_eq!(report.objects["K"].input_count.mean, 5.0);
    let flow = report.process_flow.as_ref().expect("flow report");
    assert_eq!(flow.tokens_created, 3.0);
    assert_eq!(flow.tokens_completed, 3.0);
}
