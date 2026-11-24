//! Trigger actions: the small, safe subset of FlexSim's object logic that
//! OpenFlex reproduces.
//!
//! FlexSim lets a modeller attach FlexScript to OnEntry/OnExit/OnReceive and
//! other triggers. OpenFlex keeps the same trigger points but expresses each
//! action as structured JSON, so a model can be validated, diffed and rendered
//! without executing arbitrary code.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use crate::expr::Expr;

/// One action executed by a trigger.
///
/// The enum is internally tagged and serde cannot combine that with
/// `deny_unknown_fields`; the hand-written [`Deserialize`] implementation
/// rejects unknown keys explicitly (A1).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Set (or create) a numeric label on the item.
    SetLabel { name: String, value: Expr },
    /// Change the flow item's type.
    SetItemType { value: Expr },
    /// Add to a numeric label, creating it at 0 first.
    IncrementLabel { name: String, by: Expr },
    /// Send a message to another object after `delay` time units.
    SendMessage {
        target: String,
        #[serde(default = "zero_expr")]
        delay: Expr,
    },
    /// Destroy the item; only valid while the item is being received.
    DestroyItem,
    /// Release an item the object is currently holding (A4, the counterpart of
    /// `send_to_port: -1` / [`crate::model::PortStrategy::Hold`]).
    ReleaseItem,
    /// task-184: start or stop a named Motor (and its connected conveyors).
    SetMotor { target: String, running: bool },
    /// task-187: route the current item to one explicit output port.
    ///
    /// The override is consumed by the **next** routing decision, so set it
    /// *before* the object routes the item — `on_entry` works, `on_exit` is too
    /// late and instead becomes the override for the next object the item
    /// visits. Models without this action are unaffected.
    SetDestination { port: u32 },
    /// Run a sandboxed FlexScript (rhai) snippet (D2 script layer).
    ///
    /// The script is compiled once when the model is loaded (the model layer
    /// only performs a syntax check) and executed by the engine with the
    /// documented sandbox API: `time`, `item_type`, `item_id`, `label(name)`,
    /// `set_label(name, value)`, `set_item_type(value)`,
    /// `send_message(target, delay)`, `release_item()` and `log(message)`.
    /// File, network, clock and random APIs are not registered.
    Script { source: String },
}

impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let mut map = value
            .as_object()
            .cloned()
            .ok_or_else(|| de::Error::custom("an action must be a JSON object"))?;
        let tag = map
            .remove("action")
            .ok_or_else(|| de::Error::custom("an action needs an \"action\" field"))?;
        let name = tag
            .as_str()
            .ok_or_else(|| de::Error::custom("\"action\" must be a string"))?
            .to_string();
        let action = match name.as_str() {
            "set_label" => {
                check_keys(&map, &name, &["name", "value"]).map_err(de::Error::custom)?;
                Action::SetLabel {
                    name: field(&mut map, "name", &name).map_err(de::Error::custom)?,
                    value: field(&mut map, "value", &name).map_err(de::Error::custom)?,
                }
            }
            "set_item_type" => {
                check_keys(&map, &name, &["value"]).map_err(de::Error::custom)?;
                Action::SetItemType {
                    value: field(&mut map, "value", &name).map_err(de::Error::custom)?,
                }
            }
            "increment_label" => {
                check_keys(&map, &name, &["name", "by"]).map_err(de::Error::custom)?;
                Action::IncrementLabel {
                    name: field(&mut map, "name", &name).map_err(de::Error::custom)?,
                    by: field(&mut map, "by", &name).map_err(de::Error::custom)?,
                }
            }
            "send_message" => {
                check_keys(&map, &name, &["target", "delay"]).map_err(de::Error::custom)?;
                Action::SendMessage {
                    target: field(&mut map, "target", &name).map_err(de::Error::custom)?,
                    delay: optional_field(&mut map, "delay", &name)
                        .map_err(de::Error::custom)?
                        .unwrap_or_else(zero_expr),
                }
            }
            "destroy_item" => {
                check_keys(&map, &name, &[]).map_err(de::Error::custom)?;
                Action::DestroyItem
            }
            "release_item" => {
                check_keys(&map, &name, &[]).map_err(de::Error::custom)?;
                Action::ReleaseItem
            }
            "set_motor" => {
                check_keys(&map, &name, &["target", "running"]).map_err(de::Error::custom)?;
                Action::SetMotor {
                    target: field(&mut map, "target", &name).map_err(de::Error::custom)?,
                    running: field(&mut map, "running", &name).map_err(de::Error::custom)?,
                }
            }
            "set_destination" => {
                check_keys(&map, &name, &["port"]).map_err(de::Error::custom)?;
                Action::SetDestination {
                    port: field(&mut map, "port", &name).map_err(de::Error::custom)?,
                }
            }
            "script" => {
                check_keys(&map, &name, &["source"]).map_err(de::Error::custom)?;
                Action::Script {
                    source: field(&mut map, "source", &name).map_err(de::Error::custom)?,
                }
            }
            other => {
                return Err(de::Error::custom(format!(
                    "unknown action '{other}' (expected one of: set_label, set_item_type, increment_label, send_message, destroy_item, release_item, set_motor, set_destination, script)"
                )));
            }
        };
        Ok(action)
    }
}

fn check_keys(
    map: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    allowed: &[&str],
) -> Result<(), String> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(if allowed.is_empty() {
                format!("action '{name}' has unknown key '{key}'")
            } else {
                format!(
                    "action '{name}' has unknown key '{key}' (expected one of: {})",
                    allowed.join(", ")
                )
            });
        }
    }
    Ok(())
}

fn field<T: serde::de::DeserializeOwned>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    name: &str,
) -> Result<T, String> {
    let value = map
        .remove(key)
        .ok_or_else(|| format!("action '{name}' requires '{key}'"))?;
    serde_json::from_value(value).map_err(|err| format!("action '{name}.{key}': {err}"))
}

fn optional_field<T: serde::de::DeserializeOwned>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    name: &str,
) -> Result<Option<T>, String> {
    match map.remove(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|err| format!("action '{name}.{key}': {err}")),
    }
}

fn zero_expr() -> Expr {
    Expr::Number(0.0)
}

/// The trigger points the engine fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimTrigger {
    OnCreate,
    OnEntry,
    OnExit,
    OnReceive,
    OnMessage,
    OnBreakdown,
    OnRepair,
    /// A conveyor photo eye was crossed (B3).
    OnPhotoEye,
    /// The processor/multiprocessor setup phase just finished (processing
    /// has not started yet).
    SetupFinish,
    /// The processing phase just finished (the item is not yet delivered).
    ProcessFinish,
    /// The content (number of items) of the object changed.
    OnContentChange,
    /// The object changed its state (idle/busy/blocked/…).
    OnStateChange,
    /// The model was reset.
    OnReset,
    /// The object was stopped (breakdown, shift, manual stop).
    OnStop,
    /// The object resumed after a stop.
    OnResume,
    /// The 3D view is redrawn (flexsim `OnDraw`). Schema/validation only for
    /// now: OpenFlex has no renderer, and the FlexSim rule "never use random
    /// functions in OnDraw" therefore applies to future integrations.
    OnDraw,
    /// The executer finished loading (task-188). Fires at `now + dwell + load`
    /// of the transport task.
    ///
    /// **Item context**: OpenFlex transport executors are operators/dispatchers
    /// and do not carry a [`FlowItem`]; the hook sees the *requester's*
    /// processing or waiting item, not an item loaded onto the executer.
    OnLoad,
    /// The executer finished unloading (task-188). Fires on the `OperatorLoaded`
    /// event of the transport task.
    ///
    /// **Item context**: same as [`SimTrigger::OnLoad`] — the requester's item,
    /// not an item unloaded from the executer.
    OnUnload,
    /// A fluid object became empty (task-189): its level crossed from positive
    /// to zero. The transition is latched, so a level that stays at zero does
    /// not fire again until it leaves empty and becomes empty once more.
    OnEmpty,
    /// A fluid object became full (task-189).
    ///
    /// **"Full" is detected through overflow, not `level == capacity`**: a full
    /// tank keeps its level at `capacity − one inflow step` and sends the rest
    /// to its overflow counter, so the level alone never reaches the capacity.
    /// The reliable signal is therefore "this step spilled something" (or the
    /// level genuinely reached the capacity). Latched the same way as
    /// [`SimTrigger::OnEmpty`].
    OnFull,
    /// An executer asks for permission to enter a traffic-controlled area
    /// (task-189). Fires before `traffic_try_acquire` decides.
    ///
    /// **Protocol**: the hook receives a probe item (no real flow item exists
    /// for a traffic request). If it leaves `label("admit")` at `0.0` the
    /// request is refused for now and retried later; a missing label (or any
    /// other value) keeps the normal acquire path. This is an OpenFlex
    /// protocol, not a FlexSim public API.
    OnEntryRequest,
}

impl SimTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            SimTrigger::OnCreate => "on_create",
            SimTrigger::OnEntry => "on_entry",
            SimTrigger::OnExit => "on_exit",
            SimTrigger::OnReceive => "on_receive",
            SimTrigger::OnMessage => "on_message",
            SimTrigger::OnBreakdown => "on_breakdown",
            SimTrigger::OnRepair => "on_repair",
            SimTrigger::OnPhotoEye => "on_photo_eye",
            SimTrigger::SetupFinish => "setup_finish",
            SimTrigger::ProcessFinish => "process_finish",
            SimTrigger::OnContentChange => "on_content_change",
            SimTrigger::OnStateChange => "on_state_change",
            SimTrigger::OnReset => "on_reset",
            SimTrigger::OnStop => "on_stop",
            SimTrigger::OnResume => "on_resume",
            SimTrigger::OnDraw => "on_draw",
            SimTrigger::OnLoad => "on_load",
            SimTrigger::OnUnload => "on_unload",
            SimTrigger::OnEmpty => "on_empty",
            SimTrigger::OnFull => "on_full",
            SimTrigger::OnEntryRequest => "on_entry_request",
        }
    }

    /// Parse the JSON/schema spelling of a trigger point.
    pub fn parse(value: &str) -> Option<SimTrigger> {
        Some(match value {
            "on_create" => SimTrigger::OnCreate,
            "on_entry" => SimTrigger::OnEntry,
            "on_exit" => SimTrigger::OnExit,
            "on_receive" => SimTrigger::OnReceive,
            "on_message" => SimTrigger::OnMessage,
            "on_breakdown" => SimTrigger::OnBreakdown,
            "on_repair" => SimTrigger::OnRepair,
            "on_photo_eye" => SimTrigger::OnPhotoEye,
            "on_content_change" => SimTrigger::OnContentChange,
            "on_state_change" => SimTrigger::OnStateChange,
            "on_reset" => SimTrigger::OnReset,
            "on_stop" => SimTrigger::OnStop,
            "on_resume" => SimTrigger::OnResume,
            "on_draw" => SimTrigger::OnDraw,
            "on_load" => SimTrigger::OnLoad,
            "on_unload" => SimTrigger::OnUnload,
            "on_empty" => SimTrigger::OnEmpty,
            "on_full" => SimTrigger::OnFull,
            "on_entry_request" => SimTrigger::OnEntryRequest,
            "setup_finish" => SimTrigger::SetupFinish,
            "process_finish" => SimTrigger::ProcessFinish,
            _ => return None,
        })
    }
}

/// Trigger lists attached to every object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Triggers {
    #[serde(default)]
    pub on_create: Vec<Action>,
    #[serde(default)]
    pub on_entry: Vec<Action>,
    #[serde(default)]
    pub on_exit: Vec<Action>,
    #[serde(default)]
    pub on_receive: Vec<Action>,
    #[serde(default)]
    pub on_message: Vec<Action>,
    #[serde(default)]
    pub on_breakdown: Vec<Action>,
    #[serde(default)]
    pub on_repair: Vec<Action>,
    #[serde(default)]
    pub on_photo_eye: Vec<Action>,
    /// Object content changed (see [`SimTrigger::OnContentChange`]).
    #[serde(default)]
    pub on_content_change: Vec<Action>,
    /// Object state changed (see [`SimTrigger::OnStateChange`]).
    #[serde(default)]
    pub on_state_change: Vec<Action>,
    /// Model reset (see [`SimTrigger::OnReset`]).
    #[serde(default)]
    pub on_reset: Vec<Action>,
    /// Object stopped (see [`SimTrigger::OnStop`]).
    #[serde(default)]
    pub on_stop: Vec<Action>,
    /// Object resumed (see [`SimTrigger::OnResume`]).
    #[serde(default)]
    pub on_resume: Vec<Action>,
    /// The 3D view is redrawn (see [`SimTrigger::OnDraw`]); schema/validation
    /// only, the renderer integration is a follow-up.
    #[serde(default)]
    pub on_draw: Vec<Action>,
    /// The setup phase just finished (see [`SimTrigger::SetupFinish`]).
    #[serde(default)]
    pub on_setup_finish: Vec<Action>,
    /// The processing phase just finished (see [`SimTrigger::ProcessFinish`]).
    #[serde(default)]
    pub on_process_finish: Vec<Action>,
    /// The executer finished loading a transport task (task-188). Fires at
    /// `now + dwell + load`; the transport event chain gains a
    /// `TransportLoaded` event, so the event *sequence* changes while the
    /// statistics and the total task duration do not.
    ///
    /// The hook's item context is the **requester's** processing or waiting
    /// item: OpenFlex transport executors are operators and never hold a
    /// `FlowItem` of their own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_load: Vec<Action>,
    /// The executer finished unloading a transport task (see
    /// [`SimTrigger::OnUnload`]). Fires on the `OperatorLoaded` event; the item
    /// context is the requester's item, not an item unloaded from the executer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_unload: Vec<Action>,
    /// A fluid object became empty (see [`SimTrigger::OnEmpty`]). Latched: one
    /// call per crossing from positive to zero.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_empty: Vec<Action>,
    /// A fluid object became full (see [`SimTrigger::OnFull`]). Latched: one
    /// call per crossing. "Full" is detected through overflow (a full tank's
    /// level stops at `capacity − inflow`), never through `level == capacity`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_full: Vec<Action>,
    /// An executer requests entry into a traffic-controlled area (see
    /// [`SimTrigger::OnEntryRequest`]); `label("admit") == 0` refuses this
    /// attempt, which is retried later.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_entry_request: Vec<Action>,
}

impl Triggers {
    pub fn actions(&self, trigger: SimTrigger) -> Vec<Action> {
        match trigger {
            SimTrigger::OnCreate => self.on_create.clone(),
            SimTrigger::OnEntry => self.on_entry.clone(),
            SimTrigger::OnExit => self.on_exit.clone(),
            SimTrigger::OnReceive => self.on_receive.clone(),
            SimTrigger::OnMessage => self.on_message.clone(),
            SimTrigger::OnBreakdown => self.on_breakdown.clone(),
            SimTrigger::OnRepair => self.on_repair.clone(),
            SimTrigger::OnPhotoEye => self.on_photo_eye.clone(),
            SimTrigger::OnContentChange => self.on_content_change.clone(),
            SimTrigger::OnStateChange => self.on_state_change.clone(),
            SimTrigger::OnReset => self.on_reset.clone(),
            SimTrigger::OnStop => self.on_stop.clone(),
            SimTrigger::OnResume => self.on_resume.clone(),
            SimTrigger::OnDraw => self.on_draw.clone(),
            SimTrigger::SetupFinish => self.on_setup_finish.clone(),
            SimTrigger::ProcessFinish => self.on_process_finish.clone(),
            SimTrigger::OnLoad => self.on_load.clone(),
            SimTrigger::OnUnload => self.on_unload.clone(),
            SimTrigger::OnEmpty => self.on_empty.clone(),
            SimTrigger::OnFull => self.on_full.clone(),
            SimTrigger::OnEntryRequest => self.on_entry_request.clone(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.on_create.is_empty()
            && self.on_entry.is_empty()
            && self.on_exit.is_empty()
            && self.on_receive.is_empty()
            && self.on_message.is_empty()
            && self.on_breakdown.is_empty()
            && self.on_repair.is_empty()
            && self.on_photo_eye.is_empty()
            && self.on_content_change.is_empty()
            && self.on_state_change.is_empty()
            && self.on_reset.is_empty()
            && self.on_stop.is_empty()
            && self.on_resume.is_empty()
            && self.on_draw.is_empty()
            && self.on_setup_finish.is_empty()
            && self.on_process_finish.is_empty()
            && self.on_load.is_empty()
            && self.on_unload.is_empty()
            && self.on_empty.is_empty()
            && self.on_full.is_empty()
            && self.on_entry_request.is_empty()
    }
}
