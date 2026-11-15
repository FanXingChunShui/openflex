//! FlexScript execution tests (D2 wiring).

use openflex_core::{SimConfig, run_model, simulate_traced};
use openflex_model::Model;

fn model(text: &str) -> Model {
    Model::from_json(text).expect("model")
}

#[test]
fn script_trigger_example_routes_by_script_condition() {
    let model = model(include_str!("../../../examples/objects/script-trigger.ofm"));
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("run");
    // on_create labels the first four items (time < 35) as VIPs; the routing
    // script_condition sends them to SinkVip and the expression default takes
    // the rest.
    assert_eq!(result.objects["SinkVip"].input_count, 4);
    assert_eq!(result.objects["SinkNormal"].input_count, 5);
    let logs: Vec<&String> = trace
        .events
        .iter()
        .filter(|event| event.event == "script_log")
        .map(|event| &event.message)
        .collect();
    assert_eq!(logs.len(), 9);
    assert!(
        logs.iter()
            .all(|message| message.as_str() == "processing item")
    );
}

#[test]
fn script_action_applies_labels_type_messages_and_logs() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "script-action",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "Source1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "max_arrivals": 3, "first_arrival": 0.0 } },
        { "name": "Processor1", "kind": "processor", "params": {
            "process_time": { "distribution": "constant", "value": 1.0 },
            "triggers": { "on_exit": [
              { "action": "script",
                "source": "set_label(\"tag\", 1.0); set_item_type(1); send_message(\"SinkB\", 5.0); log(\"done\");" } ] } } },
        { "name": "Queue1", "kind": "queue", "params": {} },
        { "name": "Queue2", "kind": "queue", "params": {} },
        { "name": "SinkA", "kind": "sink", "params": {} },
        { "name": "SinkB", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "Source1", "to": "Processor1" },
        { "from": "Processor1", "to": "Queue1" },
        { "from": "Queue1", "to": "SinkA" },
        { "from": "Queue1", "to": "Queue2", "strategy": { "type": "by_label", "label": "tag" } },
        { "from": "Queue2", "to": "SinkA" },
        { "from": "Queue2", "to": "SinkB", "strategy": { "type": "by_item_type" } }
      ]
    }"#;
    let model = model(json);
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("run");
    // The label `tag = 1` routes Queue1 -> Queue2, and `item_type = 1` routes
    // Queue2 -> SinkB.
    assert_eq!(result.objects["SinkA"].input_count, 0);
    assert_eq!(result.objects["SinkB"].input_count, 3);
    let logs = trace
        .events
        .iter()
        .filter(|event| event.event == "script_log")
        .count();
    assert_eq!(logs, 3);
    let messages = trace
        .events
        .iter()
        .filter(|event| event.event == "message" && event.object == "SinkB")
        .count();
    assert_eq!(messages, 3);
}

#[test]
fn script_runtime_errors_carry_the_trigger_location() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "script-error",
      "objects": [
        { "name": "Source1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "max_arrivals": 1,
            "triggers": { "on_create": [{ "action": "script", "source": "nope();" }] } } },
        { "name": "Sink1", "kind": "sink", "params": {} }
      ],
      "connections": [{ "from": "Source1", "to": "Sink1" }]
    }"#;
    let model = Model::from_json(json).expect("model");
    let error = run_model(&model).expect_err("runtime error");
    let text = error.to_string();
    assert!(text.contains("script error at"), "{text}");
    assert!(text.contains("Source1"), "{text}");
    assert!(text.contains("on_create"), "{text}");
}

#[test]
fn script_actions_create_move_and_set_state() {
    let model = model(include_str!("../../../examples/objects/script-actions.ofm"));
    let config = SimConfig {
        stop_time: 100.0,
        warmup: 0.0,
        seed: 1,
    };
    let (result, trace) = simulate_traced(&model, &config, 0).expect("run");
    // Source1 creates three items in Queue1, a later script moves one to Sink2.
    assert_eq!(result.objects["Queue1"].input_count, 5);
    assert_eq!(result.objects["Queue1"].output_count, 1);
    assert_eq!(result.objects["Sink2"].input_count, 1);
    let logs: Vec<&String> = trace
        .events
        .iter()
        .filter(|event| event.event == "script_log")
        .map(|event| &event.message)
        .collect();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0], "idle");
    // set_state("busy") applied to the source at t = 0.
    assert!(result.objects["Source1"].utilization > 0.5);
}

#[test]
fn script_condition_can_return_a_boolean_and_query_objects() {
    let json = r#"{
      "format": "openflex-model", "version": 1, "name": "script-bool",
      "run": { "stop_time": 100.0, "warmup": 0.0, "seed": 1, "replications": 1 },
      "objects": [
        { "name": "S1", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 10.0 },
            "max_arrivals": 2, "first_arrival": 0.0 } },
        { "name": "S2", "kind": "source", "params": {
            "interarrival": { "distribution": "constant", "value": 1000.0 },
            "max_arrivals": 1, "first_arrival": 25.0 } },
        { "name": "Q", "kind": "queue", "params": { "max_content": 10 } },
        { "name": "KA", "kind": "sink", "params": {} },
        { "name": "KB", "kind": "sink", "params": {} }
      ],
      "connections": [
        { "from": "S1", "to": "Q" },
        { "from": "S2", "to": "KA", "strategy": { "type": "by_condition", "conditions": [
            { "script_condition": "object_content(\"Q\") >= 1.0 && time > 20.0", "port": 0 },
            { "condition": "1 == 1", "port": 1 } ] } },
        { "from": "S2", "to": "KB" }
      ]
    }"#;
    let model = model(json);
    let report = run_model(&model).expect("run");
    // The direct boolean expression is true: two items are buffered in Q when
    // the third source arrives, so it routes to KA.
    assert_eq!(report.objects["KA"].input_count.mean, 1.0);
    assert_eq!(report.objects["KB"].input_count.mean, 0.0);
}
