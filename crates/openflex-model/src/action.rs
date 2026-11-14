//! Trigger actions: the small, safe subset of FlexSim's object logic that
//! OpenFlex reproduces.
//!
//! FlexSim lets a modeller attach FlexScript to OnEntry/OnExit/OnReceive and
//! other triggers. OpenFlex keeps the same trigger points but expresses each
//! action as structured JSON, so a model can be validated, diffed and rendered
//! without executing arbitrary code.

use serde::{Deserialize, Serialize};

use crate::expr::Expr;

/// One action executed by a trigger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        }
    }
}

/// Trigger lists attached to every object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    }
}
