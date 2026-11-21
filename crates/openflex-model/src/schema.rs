//! JSON Schema for the `.ofm` model format (C6).
//!
//! The schema is generated from the same field names as the Rust structures and
//! is meant for editor completion, CI validation and for the `openflex schema`
//! command. It intentionally describes the *JSON* contract (which is what users
//! write) rather than the internal representation: the object `params` are
//! linked to `kind` through `if`/`then`, and the internally tagged enums
//! (`distribution`, port `strategy`, `action`) are described as `oneOf` with a
//! `const` tag.

use serde_json::{Map, Value, json};

/// Return the JSON Schema (draft 2020-12) of an OpenFlex model.
///
/// The result is a `serde_json::Value` so callers can pretty-print it, write it
/// to a file, or embed it in a larger tool schema.
pub fn json_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://openflex.dev/schema/openflex-model-v1.json",
        "title": "OpenFlex model",
        "description": "A discrete-event simulation model (.ofm). Unknown keys are rejected.",
        "$ref": "#/$defs/model",
        "$defs": definitions(),
    })
}

fn definitions() -> Value {
    let mut defs = Map::new();
    defs.insert("model".into(), model_schema());
    defs.insert("object".into(), object_schema());
    defs.insert("connection".into(), connection_schema());
    defs.insert(
        "conditional_port".into(),
        json!({
            "type": "object",
            "description": "A routing/decision rule: an expression condition or a sandboxed rhai script condition (mutually exclusive).",
            "properties": {
                "condition": { "$ref": "#/$defs/expression" },
                "script_condition": { "type": "string", "minLength": 1 },
                "port": { "type": "integer", "minimum": 0 }
            },
            "required": ["port"],
            "additionalProperties": false,
            "oneOf": [
                { "required": ["condition"] },
                { "required": ["script_condition"] }
            ]
        }),
    );
    defs.insert("port_strategy".into(), port_strategy_schema());
    defs.insert(
        "run_settings".into(),
        json!({
            "type": "object",
            "properties": {
                "stop_time": { "type": "number", "exclusiveMinimum": 0, "default": 1000.0 },
                "warmup": { "type": "number", "minimum": 0, "default": 0.0 },
                "seed": { "type": "integer", "minimum": 0, "default": 1 },
                "replications": { "type": "integer", "minimum": 1, "default": 1 }
            },
            "additionalProperties": false
        }),
    );
    defs.insert(
        "experiment".into(),
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "replications": { "type": "integer", "minimum": 1, "default": 1 },
                "scenarios": { "type": "array", "items": { "$ref": "#/$defs/scenario" } }
            },
            "required": ["name"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "scenario".into(),
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "overrides": { "type": "array", "items": { "$ref": "#/$defs/override" } },
                "stop_time": { "type": "number", "exclusiveMinimum": 0 },
                "warmup": { "type": "number", "minimum": 0 }
            },
            "required": ["name"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "override".into(),
        json!({
            "type": "object",
            "properties": {
                "object": { "type": "string" },
                "param": { "type": "string" },
                "value": {}
            },
            "required": ["object", "param", "value"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "shift".into(),
        json!({
            "type": "object",
            "properties": {
                "start": { "type": "number", "default": 0.0 },
                "duration": { "type": "number", "exclusiveMinimum": 0, "default": 480.0 },
                "repeat_every": { "type": "number", "minimum": 0, "default": 0.0 }
            },
            "additionalProperties": false
        }),
    );
    defs.insert(
        "downtime".into(),
        json!({
            "type": "object",
            "properties": {
                "start": { "type": "number", "minimum": 0 },
                "end": { "type": "number", "exclusiveMinimum": 0 }
            },
            "required": ["start", "end"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "failure".into(),
        json!({
            "type": "object",
            "properties": {
                "mtbf": { "$ref": "#/$defs/distribution" },
                "mttr": { "$ref": "#/$defs/distribution" },
                "repair_operator": { "type": ["string", "null"] },
                "first_failure_time": { "type": ["number", "null"] },
                "count_time_in_states": {
                    "type": "array",
                    "items": {
                        "enum": ["idle", "setup", "processing", "blocked", "waiting_for_operator",
                                 "waiting_for_transport", "down", "off_shift"]
                    }
                }
            },
            "required": ["mtbf", "mttr"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "pull".into(),
        json!({
            "type": "object",
            "properties": {
                "enabled": { "type": "boolean", "default": false },
                "strategy": { "$ref": "#/$defs/port_strategy" },
                "requirement": { "$ref": "#/$defs/expression" }
            },
            "additionalProperties": false
        }),
    );
    defs.insert(
        "triggers".into(),
        json!({
            "type": "object",
            "properties": {
                "on_create": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_entry": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_exit": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_receive": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_message": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_breakdown": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_repair": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_photo_eye": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_content_change": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_state_change": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_reset": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_stop": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_resume": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_draw": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_setup_finish": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_process_finish": { "type": "array", "items": { "$ref": "#/$defs/action" } }
            },
            "additionalProperties": false
        }),
    );
    defs.insert("action".into(), action_schema());
    defs.insert(
        "table_cell".into(),
        json!({
            "description": "A global table cell: number, boolean or string.",
            "oneOf": [
                { "type": "number" },
                { "type": "boolean" },
                { "type": "string" }
            ]
        }),
    );
    defs.insert(
        "global_variable".into(),
        json!({
            "type": "object",
            "description": "A global variable (FlexSim ModelingTools/GlobalVariables): an explicit kind plus the raw value.",
            "properties": {
                "kind": { "enum": ["number", "string"] },
                "value": {}
            },
            "required": ["kind", "value"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "pick_list_entry".into(),
        json!({
            "type": "object",
            "description": "One pick list entry: a display name and the value it resolves to.",
            "properties": {
                "name": { "type": "string" },
                "value": { "type": "string" }
            },
            "required": ["name", "value"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "picklist".into(),
        json!({
            "type": "object",
            "description": "A named pick list (FlexSim PickLists/); needs at least one entry.",
            "properties": {
                "entries": {
                    "type": "array",
                    "minItems": 1,
                    "items": { "$ref": "#/$defs/pick_list_entry" }
                }
            },
            "required": ["entries"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "global_table".into(),
        json!({
            "type": "object",
            "description": "A named read-only table (FlexSim Global Table).",
            "properties": {
                "columns": {
                    "type": "array",
                    "items": { "type": "string", "minLength": 1 },
                    "uniqueItems": true
                },
                "rows": {
                    "type": "array",
                    "items": {
                        "type": "array",
                        "items": { "$ref": "#/$defs/table_cell" }
                    }
                }
            },
            "additionalProperties": false
        }),
    );
    defs.insert("expression".into(), json!({
        "description": "An OpenFlex expression: a number, boolean, string, a dotted variable path (time, item.type, Queue1.content) or one of if/min/max/abs/round/floor/ceil/sqrt/len.",
        "examples": [1.0, "item.type", "time"]
    }));
    defs.insert(
        "arrival_schedule".into(),
        json!({
            "type": "object",
            "properties": {
                "rows": { "type": "array", "items": { "$ref": "#/$defs/arrival_row" } },
                "repeat": { "type": "boolean", "default": false }
            },
            "additionalProperties": false
        }),
    );
    defs.insert(
        "arrival_row".into(),
        json!({
            "type": "object",
            "properties": {
                "time": { "type": "number", "minimum": 0 },
                "quantity": { "type": "integer", "minimum": 0, "default": 1 },
                "item_type": { "type": ["integer", "null"], "minimum": 0 },
                "labels": { "type": "object", "additionalProperties": { "type": "number" } }
            },
            "required": ["time"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "sequence_row".into(),
        json!({
            "type": "object",
            "properties": {
                "quantity": { "type": "integer", "minimum": 0, "default": 1 },
                "item_type": { "type": ["integer", "null"], "minimum": 0 },
                "labels": { "type": "object", "additionalProperties": { "type": "number" } }
            },
            "additionalProperties": false
        }),
    );
    defs.insert("distribution".into(), distribution_schema());

    defs.insert("source_params".into(), params_schema(
        json!({
            "interarrival": { "$ref": "#/$defs/distribution" },
            "items_per_arrival": { "type": "integer", "minimum": 1, "default": 1 },
            "max_arrivals": { "type": ["integer", "null"], "minimum": 1 },
            "first_arrival": { "type": "number", "minimum": 0, "default": 0.0 },
            "stop_time": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "arrival_style": { "enum": ["inter_arrival_time", "arrival_schedule", "arrival_sequence"] },
            "arrival_schedule": { "$ref": "#/$defs/arrival_schedule" },
            "arrival_sequence": { "type": "array", "items": { "$ref": "#/$defs/sequence_row" } },
            "arrival_schedule_from_table": {
                "type": "string",
                "description": "Global table whose rows define the arrival schedule (time / quantity / item_type / label columns)."
            },
            "item_type": { "type": "integer", "minimum": 0, "default": 0 },
            "labels": { "type": "object", "additionalProperties": { "type": "number" } }
        }),
    ));
    defs.insert(
        "queue_params".into(),
        params_schema(json!({
            "max_content": { "type": "integer", "minimum": 1, "default": 1000 },
            "order": { "$ref": "#/$defs/queue_order" },
            "batch_size": { "type": ["integer", "null"], "minimum": 1 },
            "batch_max_wait": { "type": "number", "minimum": 0, "default": 0.0 }
        })),
    );
    defs.insert(
        "processor_params".into(),
        params_schema(json!({
            "setup_time": { "$ref": "#/$defs/distribution" },
            "process_time": { "$ref": "#/$defs/distribution" },
            "max_content": { "type": "integer", "minimum": 1, "default": 1 },
            "setup_operator": { "type": ["string", "null"] },
            "process_operator": { "type": ["string", "null"] },
            "num_setup_operators": { "type": ["integer", "null"], "minimum": 1 },
            "num_process_operators": { "type": ["integer", "null"], "minimum": 1 },
            "use_same_num_operators": { "type": "boolean", "default": false },
            "operator_priority": { "type": "integer", "minimum": 0, "default": 0 },
            "operator_preempt": { "type": "boolean", "default": false },
            "failure": { "$ref": "#/$defs/failure" },
            "changeover_time": { "$ref": "#/$defs/distribution" }
        })),
    );
    defs.insert(
        "multiprocessor_params".into(),
        params_schema(json!({
            "slots": { "type": "integer", "minimum": 1, "default": 1 },
            "setup_time": { "$ref": "#/$defs/distribution" },
            "process_time": { "$ref": "#/$defs/distribution" },
            "setup_operator": { "type": ["string", "null"] },
            "process_operator": { "type": ["string", "null"] }
        })),
    );
    defs.insert("sink_params".into(), params_schema(json!({})));
    defs.insert(
        "combiner_params".into(),
        params_schema(json!({
            "components_per_container": { "type": "integer", "minimum": 1, "default": 2 },
            "mode": { "enum": ["pack", "join", "batch"] },
            "setup_time": { "$ref": "#/$defs/distribution" },
            "process_time": { "$ref": "#/$defs/distribution" },
            "setup_operator": { "type": ["string", "null"] },
            "process_operator": { "type": ["string", "null"] }
        })),
    );
    defs.insert(
        "separator_params".into(),
        params_schema(json!({
            "split_count": { "type": "integer", "minimum": 0, "default": 0 },
            "separate_mode": { "enum": ["unpack", "split"], "default": "unpack" }
        })),
    );
    defs.insert(
        "conveyor_params".into(),
        params_schema(json!({
            "speed": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "length": { "type": "number", "exclusiveMinimum": 0, "default": 10.0 },
            "accumulating": { "type": "boolean", "default": false },
            "item_spacing": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "max_content": { "type": "integer", "minimum": 1, "default": 1000 },
            "slug": { "type": "boolean", "default": false },
            "slug_min_gap": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "photo_eyes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "position": { "type": "number", "exclusiveMinimum": 0 },
                        "name": { "type": "string", "default": "" }
                    },
                    "required": ["position"],
                    "additionalProperties": false
                }
            },
            "stations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "position": { "type": "number", "exclusiveMinimum": 0 },
                        "process_time": { "$ref": "#/$defs/distribution" }
                    },
                    "required": ["position"],
                    "additionalProperties": false
                }
            },
            "join_inputs": { "type": ["integer", "null"], "minimum": 2 }
        })),
    );
    defs.insert(
        "merge_params".into(),
        params_schema(json!({ "wait_for_all": { "type": "boolean", "default": false } })),
    );
    defs.insert(
        "split_params".into(),
        params_schema(json!({ "by_item_type": { "type": "boolean", "default": false } })),
    );
    defs.insert(
        "transporter_params".into(),
        params_schema(json!({
            "capacity": { "type": "integer", "minimum": 1, "default": 1 },
            "speed": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "load_time": { "$ref": "#/$defs/distribution" },
            "unload_time": { "$ref": "#/$defs/distribution" },
            "home": { "type": ["string", "null"] },
            "network": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string" },
                        "to": { "type": "string" },
                        "distance": { "type": "number", "exclusiveMinimum": 0 },
                        "directed": { "type": "boolean", "default": false }
                    },
                    "required": ["from", "to", "distance"],
                    "additionalProperties": false
                }
            }
        })),
    );
    defs.insert(
        "agv_params".into(),
        params_schema(json!({
            "capacity": { "type": "integer", "minimum": 1, "default": 1 },
            "speed": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "load_time": { "$ref": "#/$defs/distribution" },
            "unload_time": { "$ref": "#/$defs/distribution" },
            "home": { "type": ["string", "null"] },
            "network": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "string" },
                        "to": { "type": "string" },
                        "distance": { "type": "number", "exclusiveMinimum": 0 },
                        "directed": { "type": "boolean", "default": false }
                    },
                    "required": ["from", "to", "distance"],
                    "additionalProperties": false
                }
            },
            "battery_capacity": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "consumption_per_distance": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "charge_time": { "$ref": "#/$defs/distribution" },
            "charger": { "type": ["string", "null"] }
        })),
    );
    defs.insert(
        "elevator_params".into(),
        params_schema(json!({
            "capacity": { "type": "integer", "minimum": 1, "default": 1 },
            "speed": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "stop_time": { "$ref": "#/$defs/distribution" },
            "home": { "type": ["string", "null"] }
        })),
    );
    defs.insert(
        "dispatcher_params".into(),
        params_schema(json!({
            "max_tasks": { "type": "integer", "minimum": 0, "default": 0 },
            "task_sequence_mode": {
                "type": "string",
                "enum": ["append", "explicit"],
                "default": "append"
            },
            "task_sequence": {
                "type": "array",
                "items": {
                    "oneOf": [
                        { "enum": ["travel", "load", "unload"] },
                        {
                            "type": "object",
                            "properties": {
                                "kind": { "enum": ["travel", "load", "unload", "utilize", "break"] },
                                "duration": { "$ref": "#/$defs/distribution" },
                                "condition": { "type": "string" },
                                "target": {
                                    "oneOf": [
                                        { "enum": ["home", "pickup", "requester"] },
                                        {
                                            "type": "object",
                                            "properties": { "object": { "type": "string" } },
                                            "required": ["object"],
                                            "additionalProperties": false
                                        },
                                        {
                                            "type": "object",
                                            "properties": {
                                                "position": {
                                                    "type": "array",
                                                    "items": { "type": "number" },
                                                    "minItems": 2,
                                                    "maxItems": 2
                                                }
                                            },
                                            "required": ["position"],
                                            "additionalProperties": false
                                        }
                                    ]
                                }
                            },
                            "required": ["kind"],
                            "additionalProperties": false
                        }
                    ]
                }
            }
        })),
    );
    defs.insert(
        "resource_params".into(),
        params_schema(json!({
            "capacity": { "type": "integer", "minimum": 1, "default": 1 },
            "downtimes": { "type": "array", "items": { "$ref": "#/$defs/downtime" } }
        })),
    );
    defs.insert(
        "rack_params".into(),
        params_schema(json!({
            "capacity": { "type": "integer", "minimum": 1, "default": 100 },
            "levels": { "type": "integer", "minimum": 1, "default": 1 },
            "rack_type": {
                "enum": ["floor", "gravity", "push_back", "drive_in"],
                "default": "floor"
            },
            "channels": { "type": ["integer", "null"], "minimum": 1 },
            "depth": { "type": ["integer", "null"], "minimum": 1 }
        })),
    );
    defs.insert("basic_fr_params".into(), params_schema(json!({})));
    defs.insert(
        "basic_te_params".into(),
        params_schema(json!({
            "speed": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "capacity": { "type": "integer", "minimum": 1, "default": 1 }
        })),
    );
    defs.insert(
        "fluid_generator_params".into(),
        params_schema(json!({
            "flow_rate": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "stop_time": { "type": ["number", "null"], "exclusiveMinimum": 0 }
        })),
    );
    defs.insert(
        "fluid_tank_params".into(),
        params_schema(json!({
            "capacity": { "type": "number", "exclusiveMinimum": 0, "default": 100.0 },
            "initial_level": { "type": "number", "minimum": 0, "default": 0.0 },
            "max_outflow": { "type": ["number", "null"], "minimum": 0 },
            "density": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "gravity": { "type": ["number", "null"], "exclusiveMinimum": 0 }
        })),
    );
    defs.insert(
        "fluid_valve_params".into(),
        params_schema(json!({
            "open": { "type": "boolean", "default": true },
            "ratio": { "type": "number", "minimum": 0, "maximum": 1, "default": 1.0 },
            "pressure_threshold": { "type": ["number", "null"], "minimum": 0 },
            "pressure_open": { "type": ["number", "null"], "minimum": 0 },
            "pressure_close": { "type": ["number", "null"], "minimum": 0 }
        })),
    );
    defs.insert("fluid_sink_params".into(), params_schema(json!({})));
    defs.insert(
        "fluid_pipe_params".into(),
        params_schema(json!({
            "length": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "speed": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
            "delay": { "type": ["number", "null"], "minimum": 0 },
            "flow_rate": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "temperature": { "type": ["number", "null"] }
        })),
    );
    defs.insert(
        "queue_order".into(),
        json!({
            "oneOf": [
                { "const": "fifo" },
                { "const": "lifo" },
                { "const": "priority" },
                {
                    "type": "object",
                    "properties": { "by_label": { "type": "string" } },
                    "required": ["by_label"],
                    "additionalProperties": false
                }
            ]
        }),
    );
    defs.insert(
        "performance_measure".into(),
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "expression": { "type": "string", "minLength": 1 },
                "unit": { "type": ["string", "null"] }
            },
            "required": ["name", "expression"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "tracked_variable".into(),
        json!({
            "type": "object",
            "description": "A model-declared time-series variable (FlexSim Tracked Variables).",
            "properties": {
                "name": { "type": "string" },
                "expression": { "type": "string" },
                "kind": {
                    "type": "string",
                    "enum": ["level", "state", "staytime", "kinetic"],
                    "default": "level"
                }
            },
            "required": ["name", "expression"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "model_triggers".into(),
        json!({
            "type": "object",
            "properties": {
                "on_start": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_reset": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "on_stop": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "start_of_experiment": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "end_of_experiment": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "start_of_scenario": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "end_of_scenario": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "start_of_replication": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "end_of_replication": { "type": "array", "items": { "$ref": "#/$defs/action" } }
            },
            "additionalProperties": false
        }),
    );
    defs.insert(
        "grid".into(),
        json!({
            "type": "object",
            "properties": {
                "width": { "type": "integer", "minimum": 1 },
                "height": { "type": "integer", "minimum": 1 },
                "cell_size": { "type": "number", "exclusiveMinimum": 0, "default": 1.0 },
                "obstacles": {
                    "type": "array",
                    "items": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "minItems": 2,
                        "maxItems": 2
                    }
                }
            },
            "required": ["width", "height"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "process_flow".into(),
        json!({
            "type": "object",
            "properties": {
                "blocks": {
                    "type": "array",
                    "items": { "$ref": "#/$defs/process_flow_block" }
                },
                "connections": {
                    "type": "array",
                    "items": { "$ref": "#/$defs/flow_connection" }
                }
            },
            "additionalProperties": false
        }),
    );
    defs.insert(
        "flow_connection".into(),
        json!({
            "type": "object",
            "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "port": { "type": ["integer", "null"], "minimum": 0 }
            },
            "required": ["from", "to"],
            "additionalProperties": false
        }),
    );
    defs.insert(
        "process_flow_block".into(),
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "minLength": 1 },
                "kind": { "enum": ["create", "delay", "assign", "decision", "sink", "transfer"] },
                "first_arrival": { "type": "number", "default": 0.0 },
                "interarrival": { "$ref": "#/$defs/distribution" },
                "quantity": { "type": "integer", "minimum": 1, "default": 1 },
                "max_creations": { "type": ["integer", "null"], "minimum": 1 },
                "item_type": { "type": "integer", "minimum": 0, "default": 0 },
                "labels": { "type": "object" },
                "time": { "$ref": "#/$defs/distribution" },
                "actions": { "type": "array", "items": { "$ref": "#/$defs/action" } },
                "conditions": {
                    "type": "array",
                    "items": { "$ref": "#/$defs/conditional_port" }
                },
                "object": { "type": "string" },
                "on_entry": { "type": "boolean", "default": false },
                "mode": { "enum": ["to_object", "from_object"], "default": "to_object" }
            },
            "required": ["id", "kind"],
            "additionalProperties": false
        }),
    );

    Value::Object(defs)
}

/// Every `*Params` object shares triggers, shifts and the A3 pull block.
fn params_schema(properties: Value) -> Value {
    let mut map = properties.as_object().cloned().unwrap_or_default();
    map.insert("triggers".into(), json!({ "$ref": "#/$defs/triggers" }));
    map.insert(
        "shifts".into(),
        json!({
            "type": "array",
            "items": { "$ref": "#/$defs/shift" }
        }),
    );
    map.insert("pull".into(), json!({ "$ref": "#/$defs/pull" }));
    map.insert(
        "process_flow".into(),
        json!({ "type": ["object", "null"], "$ref": "#/$defs/process_flow" }),
    );
    map.insert(
        "timetable".into(),
        json!({
            "type": ["object", "null"],
            "properties": {
                "windows": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "start": { "type": "number" },
                            "end": { "type": "number" }
                        },
                        "required": ["start", "end"],
                        "additionalProperties": false
                    }
                }
            },
            "additionalProperties": false
        }),
    );
    json!({
        "type": "object",
        "properties": map,
        "additionalProperties": false
    })
}

fn model_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "format": { "const": "openflex-model" },
            "version": { "const": 1 },
            "name": { "type": "string", "minLength": 1 },
            "description": { "type": "string" },
            "time_unit": { "enum": ["seconds", "minutes", "hours", "days"], "default": "seconds" },
            "run": { "$ref": "#/$defs/run_settings" },
            "objects": {
                "type": "array",
                "items": { "$ref": "#/$defs/object" }
            },
            "connections": {
                "type": "array",
                "items": { "$ref": "#/$defs/connection" }
            },
            "experiments": {
                "type": "array",
                "items": { "$ref": "#/$defs/experiment" }
            },
            "process_flow": { "$ref": "#/$defs/process_flow" },
            "fluid_step": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "grid": { "$ref": "#/$defs/grid" },
            "global_tables": {
                "type": "object",
                "description": "Named read-only tables; each row must have one cell per column.",
                "additionalProperties": { "$ref": "#/$defs/global_table" }
            },
            "global_variables": {
                "type": "object",
                "description": "Named global variables (FlexSim ModelingTools/GlobalVariables).",
                "additionalProperties": { "$ref": "#/$defs/global_variable" }
            },
            "picklists": {
                "type": "object",
                "description": "Named pick lists (FlexSim PickLists/).",
                "additionalProperties": { "$ref": "#/$defs/picklist" }
            },
            "model_triggers": { "$ref": "#/$defs/model_triggers" },
            "performance_measures": {
                "type": "array",
                "items": { "$ref": "#/$defs/performance_measure" }
            },
            "tracked_variables": {
                "type": "array",
                "items": { "$ref": "#/$defs/tracked_variable" }
            },
            "tracked_interval": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "metadata": { "type": "object" }
        },
        "required": ["name"],
        "additionalProperties": false
    })
}

fn object_schema() -> Value {
    let kinds = [
        "source",
        "queue",
        "processor",
        "multiprocessor",
        "sink",
        "combiner",
        "separator",
        "conveyor",
        "merge",
        "split",
        "transporter",
        "person",
        "agv",
        "elevator",
        "robot",
        "crane",
        "asrs_vehicle",
        "escalator",
        "dispatcher",
        "resource",
        "rack",
        "basic_fr",
        "basic_te",
        "fluid_generator",
        "fluid_tank",
        "fluid_valve",
        "fluid_sink",
        "fluid_pipe",
    ];
    let param_defs = [
        "source_params",
        "queue_params",
        "processor_params",
        "multiprocessor_params",
        "sink_params",
        "combiner_params",
        "separator_params",
        "conveyor_params",
        "merge_params",
        "split_params",
        "transporter_params",
        "transporter_params",
        "agv_params",
        "elevator_params",
        "transporter_params",
        "transporter_params",
        "transporter_params",
        "conveyor_params",
        "dispatcher_params",
        "resource_params",
        "rack_params",
        "basic_fr_params",
        "basic_te_params",
        "fluid_generator_params",
        "fluid_tank_params",
        "fluid_valve_params",
        "fluid_sink_params",
        "fluid_pipe_params",
    ];
    let links: Vec<Value> = kinds
        .iter()
        .zip(param_defs)
        .map(|(kind, def)| {
            json!({
                "if": { "properties": { "kind": { "const": kind } }, "required": ["kind"] },
                "then": { "properties": { "params": { "$ref": format!("#/$defs/{def}") } } }
            })
        })
        .collect();
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "pattern": "^[A-Za-z_][A-Za-z0-9_]*$" },
            "kind": { "enum": kinds },
            "pos": {
                "type": "array",
                "items": { "type": "number" },
                "minItems": 2,
                "maxItems": 2,
                "default": [0.0, 0.0]
            },
            "z": { "type": "number", "default": 0.0 },
            "rot": {
                "type": ["number", "null"],
                "description": "Planar orientation in degrees (counter-clockwise); omitted means 0."
            },
            "params": { "type": "object" }
        },
        "required": ["name", "kind"],
        "additionalProperties": false,
        "allOf": links
    })
}

fn connection_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "from": { "type": "string" },
            "to": { "type": "string" },
            "port": { "type": ["integer", "null"], "minimum": 0 },
            "strategy": { "$ref": "#/$defs/port_strategy" },
            "flow_rate": { "type": ["number", "null"], "exclusiveMinimum": 0 },
            "temperature": { "type": ["number", "null"] },
            "kind": { "enum": ["a", "s"], "default": "a" }
        },
        "required": ["from", "to"],
        "additionalProperties": false
    })
}

fn port_strategy_schema() -> Value {
    json!({
        "oneOf": [
            { "type": "object", "properties": { "type": { "const": "first_available" } },
              "required": ["type"], "additionalProperties": false },
            { "type": "object", "properties": { "type": { "const": "round_robin" } },
              "required": ["type"], "additionalProperties": false },
            { "type": "object", "properties": { "type": { "const": "random" } },
              "required": ["type"], "additionalProperties": false },
            {
                "type": "object",
                "properties": {
                    "type": { "const": "by_percentage" },
                    "percentages": { "type": "array", "items": { "type": "number", "minimum": 0 } }
                },
                "required": ["type", "percentages"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "type": { "const": "by_condition" },
                    "conditions": { "type": "array", "items": { "$ref": "#/$defs/conditional_port" } }
                },
                "required": ["type", "conditions"],
                "additionalProperties": false
            },
            { "type": "object", "properties": { "type": { "const": "by_item_type" } },
              "required": ["type"], "additionalProperties": false },
            {
                "type": "object",
                "properties": {
                    "type": { "const": "by_label" },
                    "label": { "type": "string" }
                },
                "required": ["type", "label"],
                "additionalProperties": false
            },
            { "type": "object", "properties": { "type": { "const": "hold" } },
              "required": ["type"], "additionalProperties": false }
        ]
    })
}

fn action_schema() -> Value {
    json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "action": { "const": "set_label" },
                    "name": { "type": "string" },
                    "value": { "$ref": "#/$defs/expression" }
                },
                "required": ["action", "name", "value"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "action": { "const": "set_item_type" },
                    "value": { "$ref": "#/$defs/expression" }
                },
                "required": ["action", "value"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "action": { "const": "increment_label" },
                    "name": { "type": "string" },
                    "by": { "$ref": "#/$defs/expression" }
                },
                "required": ["action", "name", "by"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "action": { "const": "send_message" },
                    "target": { "type": "string" },
                    "delay": { "$ref": "#/$defs/expression" }
                },
                "required": ["action", "target"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": { "action": { "const": "destroy_item" } },
                "required": ["action"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": { "action": { "const": "release_item" } },
                "required": ["action"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "description": "Run a sandboxed FlexScript (rhai) snippet; the engine API is documented in openflex-core::script.",
                "properties": {
                    "action": { "const": "script" },
                    "source": { "type": "string", "minLength": 1 }
                },
                "required": ["action", "source"],
                "additionalProperties": false
            }
        ]
    })
}

/// One `oneOf` branch per distribution, with the parameters it requires.
fn distribution_variant(name: &str, fields: &[(&str, Value)], doc: &str) -> Value {
    let mut properties = Map::new();
    properties.insert("distribution".into(), json!({ "const": name }));
    let mut required = vec![json!("distribution")];
    for (field, schema) in fields {
        properties.insert((*field).to_string(), schema.clone());
        required.push(json!(field));
    }
    json!({
        "type": "object",
        "description": doc,
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn number() -> Value {
    json!({ "type": "number" })
}

fn distribution_schema() -> Value {
    let mut variants = Vec::new();
    variants.push(distribution_variant(
        "constant",
        &[("value", number())],
        "Always returns the same value.",
    ));
    variants.push(distribution_variant(
        "uniform",
        &[("min", number()), ("max", number())],
        "Continuous uniform on [min, max).",
    ));
    variants.push(distribution_variant(
        "normal",
        &[("mean", number()), ("stddev", number())],
        "Normal distribution.",
    ));
    variants.push(distribution_variant(
        "exponential",
        &[("mean", number())],
        "Exponential with the given mean.",
    ));
    variants.push(distribution_variant(
        "triangular",
        &[("min", number()), ("mode", number()), ("max", number())],
        "Triangular distribution.",
    ));
    variants.push(distribution_variant(
        "gamma",
        &[("shape", number()), ("scale", number())],
        "Gamma distribution.",
    ));
    variants.push(distribution_variant(
        "weibull",
        &[("shape", number()), ("scale", number())],
        "Weibull distribution.",
    ));
    variants.push(distribution_variant(
        "lognormal",
        &[("meanlog", number()), ("sdlog", number())],
        "Lognormal distribution.",
    ));
    variants.push(distribution_variant(
        "erlang",
        &[
            ("shape", json!({ "type": "integer", "minimum": 1 })),
            ("scale", number()),
        ],
        "Erlang distribution.",
    ));
    variants.push(distribution_variant(
        "beta",
        &[("alpha", number()), ("beta", number())],
        "Beta distribution on [0, 1].",
    ));
    variants.push(distribution_variant(
        "integer_uniform",
        &[
            ("min", json!({ "type": "integer" })),
            ("max", json!({ "type": "integer" })),
        ],
        "Discrete uniform over the inclusive integer range.",
    ));
    let mut empirical = json!({
        "type": "object",
        "description": "Empirical distribution over fixed values, optionally weighted.",
        "properties": {
            "distribution": { "const": "empirical" },
            "values": { "type": "array", "items": { "type": "number" }, "minItems": 1 },
            "weights": { "type": "array", "items": { "type": "number", "minimum": 0 } }
        },
        "required": ["distribution", "values"],
        "additionalProperties": false
    });
    if let Some(properties) = empirical
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        properties.insert(
            "weights".into(),
            json!({
                "type": ["array", "null"],
                "items": { "type": "number", "minimum": 0 }
            }),
        );
    }
    variants.push(empirical);
    variants.push(distribution_variant(
        "pareto",
        &[("shape", number()), ("scale", number())],
        "Pareto (type I) distribution: survival (scale/x)^shape.",
    ));
    variants.push(distribution_variant(
        "log_logistic",
        &[("alpha", number()), ("beta", number())],
        "Log-logistic distribution with scale alpha and shape beta.",
    ));
    variants.push(distribution_variant(
        "rayleigh",
        &[("sigma", number())],
        "Rayleigh distribution with scale sigma.",
    ));
    variants.push(distribution_variant(
        "binomial",
        &[
            ("n", json!({ "type": "integer", "minimum": 1 })),
            ("p", json!({ "type": "number", "minimum": 0, "maximum": 1 })),
        ],
        "Binomial distribution: n trials with success probability p.",
    ));
    variants.push(distribution_variant(
        "poisson",
        &[("mean", json!({ "type": "number", "exclusiveMinimum": 0 }))],
        "Poisson distribution with the given rate.",
    ));
    variants.push(distribution_variant(
        "geometric",
        &[(
            "p",
            json!({ "type": "number", "exclusiveMinimum": 0, "maximum": 1 }),
        )],
        "Geometric distribution counting failures before the first success.",
    ));
    variants.push(distribution_variant(
        "negative_binomial",
        &[
            ("r", json!({ "type": "integer", "minimum": 1 })),
            (
                "p",
                json!({ "type": "number", "exclusiveMinimum": 0, "maximum": 1 }),
            ),
        ],
        "Negative binomial counting failures before the r-th success.",
    ));
    variants.push(distribution_variant(
        "bernoulli",
        &[("p", json!({ "type": "number", "minimum": 0, "maximum": 1 }))],
        "Bernoulli distribution: 1 with probability p, 0 otherwise.",
    ));
    json!({ "oneOf": variants })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_definitions_and_version() {
        let schema = json_schema();
        assert_eq!(
            schema.get("$schema").and_then(Value::as_str),
            Some("https://json-schema.org/draft/2020-12/schema")
        );
        let defs = schema.get("$defs").and_then(Value::as_object).unwrap();
        for name in [
            "model",
            "object",
            "connection",
            "distribution",
            "source_params",
            "combiner_params",
            "separator_params",
            "scenario",
        ] {
            assert!(defs.contains_key(name), "missing $defs.{name}");
        }
    }

    #[test]
    fn distribution_schema_lists_every_distribution() {
        let schema = json_schema();
        let variants = schema["$defs"]["distribution"]["oneOf"]
            .as_array()
            .expect("distribution oneOf");
        let names: Vec<&str> = variants
            .iter()
            .map(|variant| {
                variant["properties"]["distribution"]["const"]
                    .as_str()
                    .expect("const tag")
            })
            .collect();
        for expected in [
            "constant",
            "uniform",
            "normal",
            "exponential",
            "triangular",
            "gamma",
            "weibull",
            "lognormal",
            "erlang",
            "beta",
            "integer_uniform",
            "empirical",
            "pareto",
            "log_logistic",
            "rayleigh",
            "binomial",
            "poisson",
            "geometric",
            "negative_binomial",
            "bernoulli",
        ] {
            assert!(names.contains(&expected), "missing distribution {expected}");
        }
        assert_eq!(names.len(), openflex_model_distribution_kinds());
    }

    fn openflex_model_distribution_kinds() -> usize {
        crate::distribution::DISTRIBUTION_KINDS.len()
    }

    #[test]
    fn params_schema_covers_new_fields() {
        let schema = json_schema();
        let source = &schema["$defs"]["source_params"]["properties"];
        for field in [
            "arrival_schedule",
            "arrival_sequence",
            "arrival_style",
            "pull",
        ] {
            assert!(source.get(field).is_some(), "source_params.{field}");
        }
        let separator = &schema["$defs"]["separator_params"]["properties"];
        assert!(separator.get("separate_mode").is_some());
        let combiner = &schema["$defs"]["combiner_params"]["properties"];
        for field in [
            "setup_time",
            "process_time",
            "setup_operator",
            "process_operator",
        ] {
            assert!(combiner.get(field).is_some(), "combiner_params.{field}");
        }
        let processor = &schema["$defs"]["processor_params"]["properties"];
        for field in [
            "num_setup_operators",
            "num_process_operators",
            "use_same_num_operators",
            "operator_priority",
            "operator_preempt",
        ] {
            assert!(processor.get(field).is_some(), "processor_params.{field}");
        }
        let scenario = &schema["$defs"]["scenario"]["properties"];
        for field in ["stop_time", "warmup"] {
            assert!(scenario.get(field).is_some(), "scenario.{field}");
        }
        let failure = &schema["$defs"]["failure"]["properties"];
        for field in ["first_failure_time", "count_time_in_states"] {
            assert!(failure.get(field).is_some(), "failure.{field}");
        }
        let triggers = &schema["$defs"]["triggers"]["properties"];
        for field in [
            "on_content_change",
            "on_state_change",
            "on_reset",
            "on_stop",
            "on_resume",
            "on_draw",
        ] {
            assert!(triggers.get(field).is_some(), "triggers.{field}");
        }
        assert!(
            schema["$defs"]["source_params"]["properties"]
                .get("arrival_schedule_from_table")
                .is_some()
        );
        assert!(schema["$defs"]["global_table"].get("properties").is_some());
        assert!(schema["$defs"]["table_cell"].get("oneOf").is_some());
        assert_eq!(
            schema["$defs"]["model"]["properties"]["global_variables"]["additionalProperties"]["$ref"],
            json!("#/$defs/global_variable")
        );
        assert_eq!(
            schema["$defs"]["global_variable"]["required"],
            json!(["kind", "value"])
        );
        assert_eq!(
            schema["$defs"]["model"]["properties"]["picklists"]["additionalProperties"]["$ref"],
            json!("#/$defs/picklist")
        );
        assert_eq!(
            schema["$defs"]["picklist"]["properties"]["entries"]["items"]["$ref"],
            json!("#/$defs/pick_list_entry")
        );
        assert_eq!(
            schema["$defs"]["pick_list_entry"]["required"],
            json!(["name", "value"])
        );
        assert_eq!(
            schema["$defs"]["connection"]["properties"]["kind"]["enum"],
            json!(["a", "s"])
        );
        let conditional = &schema["$defs"]["conditional_port"]["properties"];
        assert!(conditional.get("script_condition").is_some());
        assert!(conditional.get("condition").is_some());
        let actions = schema["$defs"]["action"]["oneOf"]
            .as_array()
            .expect("action oneOf");
        assert!(
            actions.iter().any(|action| {
                action["properties"]["action"]["const"].as_str() == Some("script")
            }),
            "action script variant"
        );
    }

    /// task-155: the JSON Schema exposes the performance-measure definition.
    #[test]
    fn schema_covers_performance_measures() {
        let schema = json_schema();
        let def = &schema["$defs"]["performance_measure"];
        assert_eq!(def["required"], json!(["name", "expression"]));
        assert_eq!(
            schema["$defs"]["model"]["properties"]["performance_measures"]["items"]["$ref"],
            json!("#/$defs/performance_measure")
        );
        assert_eq!(def["properties"]["unit"]["type"], json!(["string", "null"]));
    }

    /// task-164: the model-trigger definition exposes the six experiment
    /// lifecycle hooks.
    #[test]
    fn schema_covers_lifecycle_hooks() {
        let schema = json_schema();
        let triggers = &schema["$defs"]["model_triggers"]["properties"];
        for field in [
            "start_of_experiment",
            "end_of_experiment",
            "start_of_scenario",
            "end_of_scenario",
            "start_of_replication",
            "end_of_replication",
        ] {
            assert_eq!(
                triggers[field]["items"]["$ref"],
                json!("#/$defs/action"),
                "missing lifecycle hook {field}"
            );
        }
    }

    /// task-173: dispatcher schema exposes the explicit mode and travel targets.
    #[test]
    fn schema_covers_explicit_task_sequence_routes() {
        let schema = json_schema();
        let params = &schema["$defs"]["dispatcher_params"]["properties"];
        assert_eq!(params["task_sequence_mode"]["enum"][1], "explicit");
        let item = &params["task_sequence"]["items"]["oneOf"][1]["properties"];
        assert!(item["target"].is_object());
    }

    /// task-176: conveyor slug fields are in the schema.
    #[test]
    fn schema_covers_conveyor_slug() {
        let schema = json_schema();
        let conveyor = &schema["$defs"]["conveyor_params"]["properties"];
        assert_eq!(conveyor["slug"]["default"], false);
        assert!(conveyor.get("slug_min_gap").is_some());
    }
}
