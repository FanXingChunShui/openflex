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

    // Transfer blocks need an existing target object.
    let transfer = flow_model(
        r#"{"id":"C","kind":"create","max_creations":1},
           {"id":"T","kind":"transfer","object":"SomeQueue"},
           {"id":"S","kind":"sink"}"#,
        r#"{"from":"C","to":"T"},{"from":"T","to":"S"}"#,
    );
    let error = Model::from_json(&transfer).expect_err("transfer target");
    assert!(error.to_string().contains("unknown"), "{error}");

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

#[test]
fn object_level_flow_transfers_tokens_as_items() {
    let model = Model::from_json(include_str!(
        "../../../examples/objects/process-flow-transfer.ofm"
    ))
    .expect("model");
    let report = run_model(&model).expect("run");
    // The five source items arrive normally, plus five transferred token items.
    assert_eq!(report.objects["Sink1"].input_count.mean, 10.0);
    let flow = report.process_flow.as_ref().expect("flow report");
    assert_eq!(flow.tokens_created, 5.0);
    assert_eq!(flow.tokens_completed, 5.0);
    let block = |id: &str| {
        flow.blocks
            .iter()
            .find(|block| block.id == id)
            .unwrap_or_else(|| panic!("missing block {id}; have {:?}", flow.blocks))
    };
    assert_eq!(block("Queue1.Create1").output_count, 5.0);
    assert_eq!(block("Queue1.Assign1").output_count, 5.0);
    assert_eq!(block("Queue1.Transfer1").input_count, 5.0);
    assert_eq!(block("Queue1.Transfer1").output_count, 5.0);
    assert_eq!(report.replications[0].items_created, 10);
}

#[test]
fn object_level_flow_trace_and_checkpoint() {
    use openflex_core::{simulate_traced, simulate_with_checkpoint};
    let model = Model::from_json(include_str!(
        "../../../examples/objects/process-flow-transfer.ofm"
    ))
    .expect("model");
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (_, trace) = simulate_traced(&model, &config, 0).expect("traced run");
    for label in ["token_create", "token_assign", "token_transfer"] {
        let count = trace
            .events
            .iter()
            .filter(|event| event.event == label)
            .count();
        assert_eq!(count, 5, "{label}");
    }

    let first = SimConfig {
        stop_time: 25.0,
        ..config
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let (resumed, _) =
        simulate_with_checkpoint(&model, &config, 0, Some(&checkpoint)).expect("resume");
    let (direct, _) = simulate_with_checkpoint(&model, &config, 0, None).expect("direct");
    assert_eq!(
        resumed, direct,
        "object-flow resume must match a direct run"
    );
}

#[test]
fn invalid_transfer_and_entry_flows_are_rejected() {
    // A model-level on_entry create block has no owner.
    let model_level = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-on-entry",
      "objects": [],
      "process_flow": {
        "blocks": [
          { "id": "C", "kind": "create", "on_entry": true, "max_creations": 1 },
          { "id": "S", "kind": "sink" }
        ],
        "connections": [{ "from": "C", "to": "S" }]
      }
    }"#;
    let error = Model::from_json(model_level).expect_err("on_entry without owner");
    assert!(error.to_string().contains("object-level"), "{error}");

    // from_object transfers are documented as not implemented.
    let from_object = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-from-object",
      "objects": [
        { "name": "Q", "kind": "queue", "params": {} },
        { "name": "K", "kind": "sink", "params": {} },
        { "name": "S", "kind": "source", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "Q" }, { "from": "Q", "to": "K" }],
      "process_flow": {
        "blocks": [
          { "id": "C", "kind": "create", "max_creations": 1 },
          { "id": "T", "kind": "transfer", "object": "Q", "mode": "from_object" }
        ],
        "connections": [{ "from": "C", "to": "T" }]
      }
    }"#;
    let error = Model::from_json(from_object).expect_err("from_object");
    assert!(error.to_string().contains("from_object"), "{error}");

    // An unknown transfer target is rejected.
    let unknown = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-target",
      "objects": [
        { "name": "S", "kind": "source", "params": {} },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "K" }],
      "process_flow": {
        "blocks": [
          { "id": "C", "kind": "create", "max_creations": 1 },
          { "id": "T", "kind": "transfer", "object": "Ghost" }
        ],
        "connections": [{ "from": "C", "to": "T" }]
      }
    }"#;
    let error = Model::from_json(unknown).expect_err("unknown target");
    assert!(error.to_string().contains("unknown"), "{error}");
}

#[test]
fn from_object_transfer_pulls_items_into_tokens() {
    let model = Model::from_json(include_str!(
        "../../../examples/objects/process-flow-from-object.ofm"
    ))
    .expect("model");
    let report = run_model(&model).expect("run");
    // Three source items are pulled out of Queue1 by three tokens.
    assert_eq!(report.objects["Queue1"].output_count.mean, 3.0);
    assert_eq!(report.objects["Queue1"].output_count.mean, 3.0);
    let flow = report.process_flow.as_ref().expect("flow report");
    assert_eq!(flow.tokens_created, 3.0);
    assert_eq!(flow.tokens_completed, 3.0);
    let pull = flow
        .blocks
        .iter()
        .find(|block| block.id == "Pull1")
        .expect("pull block");
    assert_eq!(pull.input_count, 3.0);
    assert_eq!(pull.output_count, 3.0);
}

#[test]
fn from_object_transfer_waits_for_an_item() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "from-object-wait",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 },
            "max_arrivals": 1, "first_arrival": 10.0 } },
        { "name": "Q", "kind": "queue", "params": {} },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "Q" }],
      "process_flow": {
        "blocks": [
          { "id": "C", "kind": "create", "max_creations": 1, "first_arrival": 0.0 },
          { "id": "P", "kind": "transfer", "object": "Q", "mode": "from_object" },
          { "id": "TS", "kind": "sink" }
        ],
        "connections": [
          { "from": "C", "to": "P" }, { "from": "P", "to": "TS" }
        ]
      }
    }"#;
    let model = Model::from_json(json).expect("model");
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (_, trace) = openflex_core::simulate_traced(&model, &config, 0).expect("run");
    let pulls: Vec<f64> = trace
        .events
        .iter()
        .filter(|event| event.event == "token_from_object")
        .map(|event| event.time)
        .collect();
    assert_eq!(pulls, vec![10.0]);
}

#[test]
fn from_object_requires_a_buffer_source() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "bad-from-object",
      "objects": [
        { "name": "S", "kind": "source", "params": {} },
        { "name": "P", "kind": "processor", "params": {} },
        { "name": "K", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }],
      "process_flow": {
        "blocks": [
          { "id": "C", "kind": "create", "max_creations": 1 },
          { "id": "P1", "kind": "transfer", "object": "P", "mode": "from_object" },
          { "id": "TS", "kind": "sink" }
        ],
        "connections": [
          { "from": "C", "to": "P1" }, { "from": "P1", "to": "TS" }
        ]
      }
    }"#;
    let error = Model::from_json(json).expect_err("processor source");
    assert!(error.to_string().contains("queue or rack"), "{error}");
}

// ---------------------------------------------------------------------------
// task-220: delay block capacity
// ---------------------------------------------------------------------------

fn capacity_model(capacity: &str) -> Model {
    let block = format!(
        r#"{{"id":"C","kind":"create","first_arrival":0.0,"max_creations":1,"quantity":3}},
           {{"id":"D","kind":"delay","time":{{"distribution":"constant","value":5.0}}{capacity}}},
           {{"id":"S","kind":"sink"}}"#
    );
    Model::from_json(&flow_model(
        &block,
        r#"{"from":"C","to":"D"},{"from":"D","to":"S"}"#,
    ))
    .expect("capacity model")
}

fn delay_exits(trace: &openflex_core::TraceLog) -> Vec<(f64, u64)> {
    trace
        .events
        .iter()
        .filter(|event| event.event == "token_delay")
        .map(|event| {
            (
                event.time,
                event.data.get("token").copied().unwrap_or(0.0) as u64,
            )
        })
        .collect()
}

fn run_capacity(capacity: &str) -> Vec<(f64, u64)> {
    let model = capacity_model(capacity);
    let config = SimConfig {
        stop_time: 20.0,
        warmup: 0.0,
        seed: 1,
    };
    let (_, trace) = simulate_traced(&model, &config, 0).expect("traced");
    delay_exits(&trace)
}

#[test]
fn delay_capacity_one_queues_fifo_and_starts_on_the_release_instant() {
    let exits = run_capacity(r#","capacity":1"#);
    assert_eq!(
        exits,
        vec![(5.0, 1), (10.0, 2), (15.0, 3)],
        "one slot: 5/10/15 in arrival order"
    );
    // Entry of token 2 is the exit instant of token 1 (no extra latency):
    // its own exit is exactly one delay later.
    assert!((exits[1].0 - exits[0].0 - 5.0).abs() < 1e-9);
    assert!((exits[2].0 - exits[1].0 - 5.0).abs() < 1e-9);
}

#[test]
fn delay_capacity_two_starts_the_first_two_together() {
    let exits = run_capacity(r#","capacity":2"#);
    assert_eq!(exits, vec![(5.0, 1), (5.0, 2), (10.0, 3)]);
}

#[test]
fn delay_capacity_zero_and_none_are_unlimited() {
    assert_eq!(
        run_capacity(""),
        vec![(5.0, 1), (5.0, 2), (5.0, 3)],
        "None is unlimited"
    );
    assert_eq!(
        run_capacity(r#","capacity":0"#),
        vec![(5.0, 1), (5.0, 2), (5.0, 3)],
        "0 is unlimited too"
    );
}

#[test]
fn delay_capacity_queues_tokens_without_losing_them_at_stop_time() {
    let model = capacity_model(r#","capacity":1"#);
    let config = SimConfig {
        stop_time: 7.0,
        warmup: 0.0,
        seed: 1,
    };
    let result = openflex_core::simulate(&model, &config, 0).expect("run");
    let flow = result.process_flow.as_ref().expect("flow");
    // Token 1 finished at 5; token 2 is in the delay and token 3 is queued.
    assert_eq!(flow.tokens_created, 3);
    assert_eq!(flow.tokens_completed, 1);
    assert_eq!(flow.tokens_created - flow.tokens_completed, 2);
}

#[test]
fn delay_capacity_is_deterministic_for_one_seed() {
    let model = capacity_model(r#","capacity":1"#);
    let config = SimConfig {
        stop_time: 20.0,
        warmup: 0.0,
        seed: 7,
    };
    let (first, first_trace) = simulate_traced(&model, &config, 0).expect("first");
    let (second, second_trace) = simulate_traced(&model, &config, 0).expect("second");
    assert_eq!(
        serde_json::to_string(&first).expect("json"),
        serde_json::to_string(&second).expect("json")
    );
    assert_eq!(
        serde_json::to_string(&first_trace).expect("json"),
        serde_json::to_string(&second_trace).expect("json")
    );
}

/// task-220: `capacity: None` keeps the process-flow baseline report
/// byte-identical (the executable token model has no capacity set).
#[test]
fn delay_capacity_none_keeps_the_flow_baseline() {
    let report = run_model(&token_model()).expect("run");
    let json = serde_json::to_string(&report).expect("json");
    assert_eq!(
        openflex_core::sha256::hex(json.as_bytes()),
        "9931c670b1fdfd97597789552ec917206dc778b17f3b256ed4bac487f7cd8d9c"
    );
}

#[test]
fn delay_capacity_queue_survives_a_checkpoint() {
    use openflex_core::simulate_with_checkpoint;
    let model = capacity_model(r#","capacity":1"#);
    let first = SimConfig {
        stop_time: 6.0,
        warmup: 0.0,
        seed: 1,
    };
    let (_, checkpoint) = simulate_with_checkpoint(&model, &first, 0, None).expect("save");
    let second = SimConfig {
        stop_time: 20.0,
        ..first
    };
    let (resumed, _) =
        simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint)).expect("resume");
    let (direct, _) = simulate_with_checkpoint(&model, &second, 0, None).expect("direct");
    assert_eq!(resumed, direct, "queued tokens must resume in FIFO order");
}
