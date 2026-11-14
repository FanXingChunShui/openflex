//! Conceptual export of an OpenFlex model towards FlexSim's `.fsm` concept (D2).
//!
//! # Clean-room boundary
//!
//! `.fsm` is FlexSim's private, binary project format. It is **not** documented
//! publicly and OpenFlex does not reverse engineer or write it: this module
//! only produces a *conceptual* mapping of the data an `.fsm` project carries
//! on the surface — objects (name, kind, position, parameter summary),
//! connections (from, to, port, routing strategy), run settings, and
//! experiments. It is meant for human comparison, documentation and for a
//! future, opt-in converter; it is **not** a file FlexSim can open, and the
//! field names are OpenFlex's, not FlexSim's internal field names.
//!
//! The export is pure data: it never mutates the model, never reads files and
//! has no binary parsing. Consumers can write the [`serde_json::Value`] to
//! disk as `*.fsm-concept.json`.
//!
//! # Example
//!
//! ```
//! # use openflex_model::{fsm_concept_json, Model};
//! let model = Model::from_json(r#"{
//!   "format": "openflex-model", "version": 1, "name": "demo",
//!   "objects": [
//!     { "name": "S", "kind": "source", "params": {} },
//!     { "name": "K", "kind": "sink", "params": {} }
//!   ],
//!   "connections": [ { "from": "S", "to": "K" } ]
//! }"#).expect("model");
//! let concept = fsm_concept_json(&model);
//! assert_eq!(concept["format"], "openflex-fsm-concept");
//! assert_eq!(concept["objects"][0]["name"], "S");
//! ```

use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::action::Triggers;
use crate::distribution::Distribution;
use crate::model::{Connection, Model, Object, ObjectParams};

/// Identifier written to the `format` field of the conceptual export.
pub const CONCEPT_FORMAT: &str = "openflex-fsm-concept";

/// Version of the conceptual export (independent of the `.ofm` version).
pub const CONCEPT_VERSION: u32 = 1;

/// Build the conceptual `.fsm` mapping of a model.
///
/// The returned value is plain JSON and stable enough to diff: it contains the
/// model header, one entry per object (with a kind-specific parameter summary),
/// connections, experiments and a small summary block. See the module
/// documentation for the clean-room boundary.
pub fn fsm_concept_json(model: &Model) -> Value {
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for object in &model.objects {
        *kinds.entry(object.kind.as_str()).or_insert(0) += 1;
    }
    json!({
        "format": CONCEPT_FORMAT,
        "concept_version": CONCEPT_VERSION,
        "generator": concat!("openflex-model ", env!("CARGO_PKG_VERSION")),
        "disclaimer": "Conceptual export only. FlexSim .fsm is a private binary format; \
                       this document is not a .fsm file and FlexSim cannot open it. \
                       Field names are OpenFlex's, not FlexSim's internal names.",
        "source_format": {
            "format": model.format,
            "version": model.version,
        },
        "model": {
            "name": model.name,
            "description": model.description,
            "time_unit": model.time_unit,
            "run": run_settings_json(model),
        },
        "objects": model.objects.iter().map(object_entry).collect::<Vec<_>>(),
        "connections": model.connections.iter().map(connection_entry).collect::<Vec<_>>(),
        "experiments": model
            .experiments
            .iter()
            .map(|experiment| {
                json!({
                    "name": experiment.name,
                    "replications": experiment.replications,
                    "scenarios": experiment
                        .scenarios
                        .iter()
                        .map(|scenario| {
                            json!({
                                "name": scenario.name,
                                "stop_time": scenario.stop_time,
                                "warmup": scenario.warmup,
                                "overrides": scenario.overrides,
                            })
                        })
                        .collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>(),
        "summary": {
            "objects": model.objects.len(),
            "connections": model.connections.len(),
            "kinds": kinds,
        },
    })
}

impl Model {
    /// Conceptual `.fsm` mapping of this model; see [`fsm_concept_json`].
    pub fn to_fsm_concept_json(&self) -> Value {
        fsm_concept_json(self)
    }
}

fn run_settings_json(model: &Model) -> Value {
    json!({
        "stop_time": model.run.stop_time,
        "warmup": model.run.warmup,
        "seed": model.run.seed,
        "replications": model.run.replications,
    })
}

fn object_entry(object: &Object) -> Value {
    json!({
        "name": object.name,
        "kind": object.kind.as_str(),
        "pos": object.pos,
        "params": serde_json::to_value(&object.params).unwrap_or(Value::Null),
        "parameter_summary": parameter_summary(&object.params),
        "shifts": object.params.shifts().len(),
        "trigger_actions": trigger_actions(object.params.triggers()),
        "pull": object
            .params
            .pull()
            .map(|pull| pull.enabled)
            .unwrap_or(false),
    })
}

fn connection_entry(connection: &Connection) -> Value {
    json!({
        "from": connection.from,
        "to": connection.to,
        "port": connection.port,
        "strategy": connection.strategy,
    })
}

fn trigger_actions(triggers: &Triggers) -> usize {
    triggers.on_create.len()
        + triggers.on_entry.len()
        + triggers.on_exit.len()
        + triggers.on_receive.len()
        + triggers.on_message.len()
        + triggers.on_breakdown.len()
        + triggers.on_repair.len()
}

fn distribution_summary(distribution: &Distribution) -> Value {
    json!({
        "kind": distribution.kind(),
        "mean": distribution.mean(),
    })
}

/// Kind-specific highlights of the parameters.
///
/// The complete parameters are exported separately in `params`; this summary
/// is the small map that is useful when comparing two models by hand (timing
/// distributions, capacities, modes). The `_` arm keeps the export working
/// when a new object kind is added to the model layer.
fn parameter_summary(params: &ObjectParams) -> Value {
    match params {
        ObjectParams::Source(p) => json!({
            "arrival_style": p.effective_arrival_style(),
            "interarrival": distribution_summary(&p.interarrival),
            "items_per_arrival": p.items_per_arrival,
            "max_arrivals": p.max_arrivals,
            "first_arrival": p.first_arrival,
            "stop_time": p.stop_time,
            "item_type": p.item_type,
        }),
        ObjectParams::Queue(p) => json!({
            "max_content": p.max_content,
            "order": p.order,
            "batch_size": p.batch_size,
            "batch_max_wait": p.batch_max_wait,
        }),
        ObjectParams::Processor(p) => json!({
            "setup_time": distribution_summary(&p.setup_time),
            "process_time": distribution_summary(&p.process_time),
            "changeover_time": distribution_summary(&p.changeover_time),
            "max_content": p.max_content,
            "setup_operator": p.setup_operator,
            "process_operator": p.process_operator,
            "num_setup_operators": p.num_setup_operators,
            "num_process_operators": p.num_process_operators,
            "operator_priority": p.operator_priority,
            "operator_preempt": p.operator_preempt,
            "failure": p.failure.is_some(),
        }),
        ObjectParams::Sink(_) => json!({}),
        ObjectParams::Combiner(p) => json!({
            "mode": p.mode,
            "components_per_container": p.components_per_container,
            "setup_time": distribution_summary(&p.setup_time),
            "process_time": distribution_summary(&p.process_time),
        }),
        ObjectParams::Separator(p) => json!({
            "separate_mode": p.separate_mode,
            "split_count": p.split_count,
        }),
        ObjectParams::Conveyor(p) => json!({
            "speed": p.speed,
            "length": p.length,
            "accumulating": p.accumulating,
            "item_spacing": p.item_spacing,
            "max_content": p.max_content,
        }),
        ObjectParams::Merge(p) => json!({
            "wait_for_all": p.wait_for_all,
        }),
        ObjectParams::Split(p) => json!({
            "by_item_type": p.by_item_type,
        }),
        ObjectParams::Transporter(p) => json!({
            "capacity": p.capacity,
            "speed": p.speed,
            "load_time": distribution_summary(&p.load_time),
            "unload_time": distribution_summary(&p.unload_time),
            "home": p.home,
        }),
        ObjectParams::Dispatcher(p) => json!({
            "max_tasks": p.max_tasks,
        }),
        ObjectParams::Resource(p) => json!({
            "capacity": p.capacity,
            "downtimes": p.downtimes.len(),
        }),
        ObjectParams::Rack(p) => json!({
            "capacity": p.capacity,
            "levels": p.levels,
        }),
        ObjectParams::BasicFr(_) => json!({}),
        ObjectParams::BasicTe(p) => json!({
            "speed": p.speed,
            "capacity": p.capacity,
        }),
        _ => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> Model {
        Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "export-demo",
              "description": "two objects",
              "time_unit": "minutes",
              "run": { "stop_time": 120.0, "warmup": 10.0, "seed": 7, "replications": 2 },
              "objects": [
                { "name": "S", "kind": "source", "pos": [1.0, 2.0], "params": {
                    "arrival_style": "arrival_schedule",
                    "arrival_schedule": { "rows": [ { "time": 0.0, "quantity": 2 } ] } } },
                { "name": "P", "kind": "processor", "params": {
                    "process_time": { "distribution": "exponential", "mean": 5.0 } } },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [
                { "from": "S", "to": "P" },
                { "from": "P", "to": "K", "strategy": { "type": "round_robin" } }
              ],
              "experiments": [ { "name": "E", "scenarios": [
                  { "name": "base", "stop_time": 60.0, "warmup": 5.0 } ] } ]
            }"#,
        )
        .expect("demo model")
    }

    #[test]
    fn exports_header_and_counts() {
        let concept = fsm_concept_json(&demo());
        assert_eq!(concept["format"], CONCEPT_FORMAT);
        assert_eq!(concept["concept_version"], CONCEPT_VERSION);
        assert_eq!(concept["model"]["name"], "export-demo");
        assert_eq!(concept["model"]["time_unit"], "minutes");
        assert_eq!(concept["model"]["run"]["stop_time"], 120.0);
        assert_eq!(concept["model"]["run"]["replications"], 2);
        assert_eq!(concept["objects"].as_array().unwrap().len(), 3);
        assert_eq!(concept["connections"].as_array().unwrap().len(), 2);
        assert_eq!(concept["summary"]["objects"], 3);
        assert_eq!(concept["summary"]["connections"], 2);
        assert_eq!(concept["summary"]["kinds"]["source"], 1);
        assert_eq!(concept["summary"]["kinds"]["sink"], 1);
    }

    #[test]
    fn exports_objects_with_parameter_summary() {
        let concept = fsm_concept_json(&demo());
        let source = &concept["objects"][0];
        assert_eq!(source["name"], "S");
        assert_eq!(source["kind"], "source");
        assert_eq!(source["pos"][0], 1.0);
        assert_eq!(
            source["parameter_summary"]["arrival_style"],
            "arrival_schedule"
        );
        let processor = &concept["objects"][1];
        assert_eq!(
            processor["parameter_summary"]["process_time"]["kind"],
            "exponential"
        );
        assert_eq!(processor["parameter_summary"]["process_time"]["mean"], 5.0);
        // The full params are exported too (round-trips through serde).
        assert_eq!(
            processor["params"]["process_time"]["distribution"],
            "exponential"
        );
        assert!(!concept["disclaimer"].as_str().unwrap().is_empty());
    }

    #[test]
    fn exports_connections_and_experiments() {
        let concept = fsm_concept_json(&demo());
        let second = &concept["connections"][1];
        assert_eq!(second["from"], "P");
        assert_eq!(second["to"], "K");
        assert_eq!(second["strategy"]["type"], "round_robin");
        let scenario = &concept["experiments"][0]["scenarios"][0];
        assert_eq!(scenario["name"], "base");
        assert_eq!(scenario["stop_time"], 60.0);
        assert_eq!(scenario["warmup"], 5.0);
    }

    #[test]
    fn method_and_free_function_agree_and_json_is_serialisable() {
        let model = demo();
        let from_method = model.to_fsm_concept_json();
        let from_function = fsm_concept_json(&model);
        assert_eq!(from_method, from_function);
        let text = serde_json::to_string_pretty(&from_function).expect("serialise");
        let parsed: Value = serde_json::from_str(&text).expect("parse back");
        assert_eq!(parsed, from_function);
    }

    #[test]
    fn empty_experiments_export_as_an_empty_array() {
        let mut model = demo();
        model.experiments.clear();
        let concept = fsm_concept_json(&model);
        assert_eq!(concept["experiments"], json!([]));
    }
}
