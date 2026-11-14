//! The OpenFlex model format.
//!
//! A model is a JSON document describing a discrete-event simulation: a set of
//! objects (the 3D library in FlexSim), the connections between them, the run
//! settings and optional experiments. The format is deliberately explicit and
//! validated before a run starts; the engine never guesses what a model meant.

use serde::de::{self, Deserializer};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::action::Triggers;
use crate::distribution::Distribution;
use crate::expr::{Expr, ExprError};

pub const FORMAT_NAME: &str = "openflex-model";
pub const FORMAT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Time and run settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimeUnit {
    #[default]
    Seconds,
    Minutes,
    Hours,
    Days,
}

impl TimeUnit {
    pub fn seconds_per_unit(self) -> f64 {
        match self {
            TimeUnit::Seconds => 1.0,
            TimeUnit::Minutes => 60.0,
            TimeUnit::Hours => 3600.0,
            TimeUnit::Days => 86_400.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TimeUnit::Seconds => "s",
            TimeUnit::Minutes => "min",
            TimeUnit::Hours => "h",
            TimeUnit::Days => "d",
        }
    }
}

/// Run configuration shared by every replication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSettings {
    /// Model time at which the run stops.
    #[serde(default = "default_stop_time")]
    pub stop_time: f64,
    /// Transient phase ignored by the statistics.
    #[serde(default)]
    pub warmup: f64,
    /// Base random seed; replication `i` uses a deterministic derivation of it.
    #[serde(default = "default_seed")]
    pub seed: u64,
    /// Number of independent replications the CLI runs by default.
    #[serde(default = "default_replications")]
    pub replications: u32,
}

impl Default for RunSettings {
    fn default() -> Self {
        Self {
            stop_time: default_stop_time(),
            warmup: 0.0,
            seed: default_seed(),
            replications: default_replications(),
        }
    }
}

fn default_stop_time() -> f64 {
    1_000.0
}

fn default_seed() -> u64 {
    1
}

fn default_replications() -> u32 {
    1
}

// ---------------------------------------------------------------------------
// Objects
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Source,
    Queue,
    Processor,
    Sink,
    Combiner,
    Separator,
    Conveyor,
    Merge,
    Split,
    Transporter,
    Dispatcher,
    Resource,
    Rack,
    BasicFr,
    BasicTe,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::Source => "source",
            ObjectKind::Queue => "queue",
            ObjectKind::Processor => "processor",
            ObjectKind::Sink => "sink",
            ObjectKind::Combiner => "combiner",
            ObjectKind::Separator => "separator",
            ObjectKind::Conveyor => "conveyor",
            ObjectKind::Merge => "merge",
            ObjectKind::Split => "split",
            ObjectKind::Transporter => "transporter",
            ObjectKind::Dispatcher => "dispatcher",
            ObjectKind::Resource => "resource",
            ObjectKind::Rack => "rack",
            ObjectKind::BasicFr => "basic_fr",
            ObjectKind::BasicTe => "basic_te",
        }
    }

    pub fn parse(value: &str) -> Option<ObjectKind> {
        Some(match value {
            "source" => ObjectKind::Source,
            "queue" => ObjectKind::Queue,
            "processor" => ObjectKind::Processor,
            "sink" => ObjectKind::Sink,
            "combiner" => ObjectKind::Combiner,
            "separator" => ObjectKind::Separator,
            "conveyor" => ObjectKind::Conveyor,
            "merge" => ObjectKind::Merge,
            "split" => ObjectKind::Split,
            "transporter" => ObjectKind::Transporter,
            "dispatcher" => ObjectKind::Dispatcher,
            "resource" => ObjectKind::Resource,
            "rack" => ObjectKind::Rack,
            "basic_fr" => ObjectKind::BasicFr,
            "basic_te" => ObjectKind::BasicTe,
            _ => return None,
        })
    }

    /// Every object kind, in library order. Used by `openflex objects list`.
    pub fn all() -> &'static [ObjectKind] {
        &[
            ObjectKind::Source,
            ObjectKind::Queue,
            ObjectKind::Processor,
            ObjectKind::Sink,
            ObjectKind::Combiner,
            ObjectKind::Separator,
            ObjectKind::Conveyor,
            ObjectKind::Merge,
            ObjectKind::Split,
            ObjectKind::Transporter,
            ObjectKind::Dispatcher,
            ObjectKind::Resource,
            ObjectKind::Rack,
            ObjectKind::BasicFr,
            ObjectKind::BasicTe,
        ]
    }
}

/// A single object in the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub name: String,
    pub kind: ObjectKind,
    pub pos: [f64; 2],
    pub params: ObjectParams,
}

impl Object {
    pub fn new(name: impl Into<String>, params: ObjectParams) -> Self {
        let kind = params.kind();
        Self {
            name: name.into(),
            kind,
            pos: [0.0, 0.0],
            params,
        }
    }
}

impl Serialize for Object {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("kind", self.kind.as_str())?;
        map.serialize_entry("pos", &self.pos)?;
        map.serialize_entry("params", &self.params)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            name: String,
            kind: String,
            #[serde(default)]
            pos: [f64; 2],
            #[serde(default = "empty_object")]
            params: serde_json::Value,
        }
        fn empty_object() -> serde_json::Value {
            serde_json::json!({})
        }
        let raw = Raw::deserialize(deserializer)?;
        let kind = ObjectKind::parse(&raw.kind).ok_or_else(|| {
            de::Error::custom(format!(
                "object '{}' has unknown kind '{}' (expected one of: {})",
                raw.name,
                raw.kind,
                ObjectKind::all()
                    .iter()
                    .map(|k| k.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
        let params = ObjectParams::from_value(kind, raw.params)
            .map_err(|err| de::Error::custom(format!("object '{}': {err}", raw.name)))?;
        Ok(Object {
            name: raw.name,
            kind,
            pos: raw.pos,
            params,
        })
    }
}

/// Type-specific parameters, one variant per object kind.
///
/// Serialised *without* a tag: the enclosing [`Object`] already names the kind.
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectParams {
    Source(SourceParams),
    Queue(QueueParams),
    Processor(ProcessorParams),
    Sink(SinkParams),
    Combiner(CombinerParams),
    Separator(SeparatorParams),
    Conveyor(ConveyorParams),
    Merge(MergeParams),
    Split(SplitParams),
    Transporter(TransporterParams),
    Dispatcher(DispatcherParams),
    Resource(ResourceParams),
    Rack(RackParams),
    BasicFr(BasicFrParams),
    BasicTe(BasicTeParams),
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|err| format!("invalid params: {err}"))
}

impl ObjectParams {
    pub fn kind(&self) -> ObjectKind {
        match self {
            ObjectParams::Source(_) => ObjectKind::Source,
            ObjectParams::Queue(_) => ObjectKind::Queue,
            ObjectParams::Processor(_) => ObjectKind::Processor,
            ObjectParams::Sink(_) => ObjectKind::Sink,
            ObjectParams::Combiner(_) => ObjectKind::Combiner,
            ObjectParams::Separator(_) => ObjectKind::Separator,
            ObjectParams::Conveyor(_) => ObjectKind::Conveyor,
            ObjectParams::Merge(_) => ObjectKind::Merge,
            ObjectParams::Split(_) => ObjectKind::Split,
            ObjectParams::Transporter(_) => ObjectKind::Transporter,
            ObjectParams::Dispatcher(_) => ObjectKind::Dispatcher,
            ObjectParams::Resource(_) => ObjectKind::Resource,
            ObjectParams::Rack(_) => ObjectKind::Rack,
            ObjectParams::BasicFr(_) => ObjectKind::BasicFr,
            ObjectParams::BasicTe(_) => ObjectKind::BasicTe,
        }
    }

    /// The trigger lists shared by every object kind.
    pub fn triggers(&self) -> &Triggers {
        match self {
            ObjectParams::Source(p) => &p.triggers,
            ObjectParams::Queue(p) => &p.triggers,
            ObjectParams::Processor(p) => &p.triggers,
            ObjectParams::Sink(p) => &p.triggers,
            ObjectParams::Combiner(p) => &p.triggers,
            ObjectParams::Separator(p) => &p.triggers,
            ObjectParams::Conveyor(p) => &p.triggers,
            ObjectParams::Merge(p) => &p.triggers,
            ObjectParams::Split(p) => &p.triggers,
            ObjectParams::Transporter(p) => &p.triggers,
            ObjectParams::Dispatcher(p) => &p.triggers,
            ObjectParams::Resource(p) => &p.triggers,
            ObjectParams::Rack(p) => &p.triggers,
            ObjectParams::BasicFr(p) => &p.triggers,
            ObjectParams::BasicTe(p) => &p.triggers,
        }
    }

    /// The shift plan shared by every object kind (empty = always on shift).
    pub fn shifts(&self) -> &[Shift] {
        match self {
            ObjectParams::Source(p) => &p.shifts,
            ObjectParams::Queue(p) => &p.shifts,
            ObjectParams::Processor(p) => &p.shifts,
            ObjectParams::Sink(p) => &p.shifts,
            ObjectParams::Combiner(p) => &p.shifts,
            ObjectParams::Separator(p) => &p.shifts,
            ObjectParams::Conveyor(p) => &p.shifts,
            ObjectParams::Merge(p) => &p.shifts,
            ObjectParams::Split(p) => &p.shifts,
            ObjectParams::Transporter(p) => &p.shifts,
            ObjectParams::Dispatcher(p) => &p.shifts,
            ObjectParams::Resource(p) => &p.shifts,
            ObjectParams::Rack(p) => &p.shifts,
            ObjectParams::BasicFr(p) => &p.shifts,
            ObjectParams::BasicTe(p) => &p.shifts,
        }
    }

    /// Parse a `params` JSON value for the given kind.
    pub fn from_value(kind: ObjectKind, value: serde_json::Value) -> Result<ObjectParams, String> {
        Ok(match kind {
            ObjectKind::Source => ObjectParams::Source(decode(value)?),
            ObjectKind::Queue => ObjectParams::Queue(decode(value)?),
            ObjectKind::Processor => ObjectParams::Processor(decode(value)?),
            ObjectKind::Sink => ObjectParams::Sink(decode(value)?),
            ObjectKind::Combiner => ObjectParams::Combiner(decode(value)?),
            ObjectKind::Separator => ObjectParams::Separator(decode(value)?),
            ObjectKind::Conveyor => ObjectParams::Conveyor(decode(value)?),
            ObjectKind::Merge => ObjectParams::Merge(decode(value)?),
            ObjectKind::Split => ObjectParams::Split(decode(value)?),
            ObjectKind::Transporter => ObjectParams::Transporter(decode(value)?),
            ObjectKind::Dispatcher => ObjectParams::Dispatcher(decode(value)?),
            ObjectKind::Resource => ObjectParams::Resource(decode(value)?),
            ObjectKind::Rack => ObjectParams::Rack(decode(value)?),
            ObjectKind::BasicFr => ObjectParams::BasicFr(decode(value)?),
            ObjectKind::BasicTe => ObjectParams::BasicTe(decode(value)?),
        })
    }
}

// Manual Serialize so the params appear without an enum tag.
impl Serialize for ObjectParams {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ObjectParams::Source(p) => p.serialize(serializer),
            ObjectParams::Queue(p) => p.serialize(serializer),
            ObjectParams::Processor(p) => p.serialize(serializer),
            ObjectParams::Sink(p) => p.serialize(serializer),
            ObjectParams::Combiner(p) => p.serialize(serializer),
            ObjectParams::Separator(p) => p.serialize(serializer),
            ObjectParams::Conveyor(p) => p.serialize(serializer),
            ObjectParams::Merge(p) => p.serialize(serializer),
            ObjectParams::Split(p) => p.serialize(serializer),
            ObjectParams::Transporter(p) => p.serialize(serializer),
            ObjectParams::Dispatcher(p) => p.serialize(serializer),
            ObjectParams::Resource(p) => p.serialize(serializer),
            ObjectParams::Rack(p) => p.serialize(serializer),
            ObjectParams::BasicFr(p) => p.serialize(serializer),
            ObjectParams::BasicTe(p) => p.serialize(serializer),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-kind parameter structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceParams {
    /// Time between consecutive arrivals (or between arrival batches).
    #[serde(default)]
    pub interarrival: Distribution,
    /// How many flow items each arrival creates.
    #[serde(default = "one_u32")]
    pub items_per_arrival: u32,
    /// Stop creating items after this many arrivals.
    #[serde(default)]
    pub max_arrivals: Option<u32>,
    /// Model time of the first arrival (defaults to one inter-arrival time).
    #[serde(default)]
    pub first_arrival: f64,
    /// The source itself stops at this model time.
    #[serde(default)]
    pub stop_time: Option<f64>,
    /// Item type given to every created flow item.
    #[serde(default)]
    pub item_type: u32,
    /// Labels attached to every created flow item.
    #[serde(default)]
    pub labels: BTreeMap<String, f64>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

// Defaults match the documented defaults used by serde when a field is
// omitted in the JSON, so programmatic construction and JSON agree.

impl Default for SourceParams {
    fn default() -> Self {
        Self {
            interarrival: Distribution::default(),
            items_per_arrival: 1,
            max_arrivals: None,
            first_arrival: 0.0,
            stop_time: None,
            item_type: 0,
            labels: BTreeMap::new(),
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

impl Default for QueueParams {
    fn default() -> Self {
        Self {
            max_content: 1_000,
            order: QueueOrder::Fifo,
            batch_size: None,
            batch_max_wait: 0.0,
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

impl Default for ProcessorParams {
    fn default() -> Self {
        Self {
            setup_time: Distribution::default(),
            process_time: one_dist(),
            max_content: 1,
            setup_operator: None,
            process_operator: None,
            failure: None,
            changeover_time: Distribution::default(),
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

impl Default for CombinerParams {
    fn default() -> Self {
        Self {
            components_per_container: 2,
            mode: CombinerMode::Pack,
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

impl Default for TransporterParams {
    fn default() -> Self {
        Self {
            capacity: 1,
            speed: 1.0,
            load_time: Distribution::default(),
            unload_time: Distribution::default(),
            home: None,
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

impl Default for BasicTeParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
            capacity: 1,
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

impl Default for ObjectParams {
    fn default() -> Self {
        ObjectParams::Sink(SinkParams::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueueOrder {
    #[default]
    Fifo,
    Lifo,
    /// Highest priority value leaves first.
    Priority,
    /// Sort by a numeric label, lowest value first.
    ByLabel(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueParams {
    #[serde(default = "default_max_content")]
    pub max_content: u32,
    #[serde(default)]
    pub order: QueueOrder,
    /// Optional batching: the queue only releases items once `size` accumulate.
    #[serde(default)]
    pub batch_size: Option<u32>,
    /// Maximum time the oldest item waits for a batch (0 = wait forever).
    #[serde(default)]
    pub batch_max_wait: f64,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessorParams {
    #[serde(default)]
    pub setup_time: Distribution,
    #[serde(default = "one_dist")]
    pub process_time: Distribution,
    #[serde(default = "one_u32")]
    pub max_content: u32,
    /// Ask this dispatcher for a resource during setup.
    #[serde(default)]
    pub setup_operator: Option<String>,
    /// Ask this dispatcher for a resource during processing.
    #[serde(default)]
    pub process_operator: Option<String>,
    #[serde(default)]
    pub failure: Option<FailureParams>,
    /// Time spent waiting before the processor can accept a new item (changeover).
    #[serde(default)]
    pub changeover_time: Distribution,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailureParams {
    /// Mean time between failures (or a full distribution).
    pub mtbf: Distribution,
    /// Mean time to repair (or a full distribution).
    pub mttr: Distribution,
    /// Optional dispatcher used by the repair crew.
    #[serde(default)]
    pub repair_operator: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SinkParams {
    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CombinerMode {
    /// Put the components inside the container item.
    #[default]
    Pack,
    /// Permanently join the items into one item.
    Join,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombinerParams {
    #[serde(default = "two_u32")]
    pub components_per_container: u32,
    #[serde(default)]
    pub mode: CombinerMode,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SeparatorParams {
    /// Unpack: split off this many parts plus the container (0 = every
    /// component inside the container). The FlexSim "unpack" semantics produce
    /// `split_count + 1` items.
    #[serde(default)]
    pub split_count: u32,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConveyorParams {
    #[serde(default = "one_f64")]
    pub speed: f64,
    #[serde(default = "ten_f64")]
    pub length: f64,
    #[serde(default)]
    pub accumulating: bool,
    #[serde(default = "default_spacing")]
    pub item_spacing: f64,
    #[serde(default = "default_max_content")]
    pub max_content: u32,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

impl Default for ConveyorParams {
    fn default() -> Self {
        Self {
            speed: 1.0,
            length: 10.0,
            accumulating: false,
            item_spacing: 1.0,
            max_content: 1_000,
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MergeParams {
    /// Merge waits until all input ports have an item available.
    #[serde(default)]
    pub wait_for_all: bool,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SplitParams {
    /// Send items to output ports in round-robin order (default) or by item type.
    #[serde(default)]
    pub by_item_type: bool,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransporterParams {
    #[serde(default = "one_u32")]
    pub capacity: u32,
    #[serde(default = "one_f64")]
    pub speed: f64,
    #[serde(default)]
    pub load_time: Distribution,
    #[serde(default)]
    pub unload_time: Distribution,
    /// Object the transporter returns to when idle.
    #[serde(default)]
    pub home: Option<String>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DispatcherParams {
    /// Dispatchers hand out tasks to transporters; the maximum number of tasks
    /// queued at once (0 = unlimited).
    #[serde(default)]
    pub max_tasks: u32,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceParams {
    #[serde(default = "one_u32")]
    pub capacity: u32,
    /// Optional schedule of unavailable periods.
    #[serde(default)]
    pub downtimes: Vec<DowntimeWindow>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

impl Default for ResourceParams {
    fn default() -> Self {
        Self {
            capacity: 1,
            downtimes: Vec::new(),
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DowntimeWindow {
    pub start: f64,
    pub end: f64,
}

/// One work period of an object's shift plan.
///
/// The window is `[start, start + duration)` and repeats every
/// `repeat_every` time units (`0` means a single window). Objects outside
/// every window are off shift: they stop processing, do not accept flow items
/// and accumulate `off_shift` time in the statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shift {
    /// Model time at which the first window begins.
    #[serde(default)]
    pub start: f64,
    /// Length of one window; must be positive to have any effect.
    #[serde(default = "eight_hours")]
    pub duration: f64,
    /// Period of the repetition; `0` means the window happens once.
    #[serde(default)]
    pub repeat_every: f64,
}

impl Default for Shift {
    fn default() -> Self {
        Self {
            start: 0.0,
            duration: eight_hours(),
            repeat_every: 0.0,
        }
    }
}

fn eight_hours() -> f64 {
    480.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RackParams {
    #[serde(default = "default_max_content")]
    pub capacity: u32,
    #[serde(default = "one_u32")]
    pub levels: u32,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

impl Default for RackParams {
    fn default() -> Self {
        Self {
            capacity: 100,
            levels: 1,
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BasicFrParams {
    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasicTeParams {
    #[serde(default = "one_f64")]
    pub speed: f64,
    #[serde(default = "one_u32")]
    pub capacity: u32,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

// ---------------------------------------------------------------------------
// Connections and routing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub from: String,
    pub to: String,
    /// Output port number (0-based) when several connections leave an object.
    #[serde(default)]
    pub port: Option<u32>,
    /// Routing rule used to pick among the object's output connections.
    #[serde(default)]
    pub strategy: Option<PortStrategy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PortStrategy {
    /// Pick the first downstream object that can accept an item.
    FirstAvailable,
    /// Rotate through the available output connections.
    RoundRobin,
    /// Pick uniformly at random among available outputs.
    Random,
    /// Weighted choice; values are percentages and should sum to 100.
    ByPercentage { percentages: Vec<f64> },
    /// First matching condition wins; `port` indexes the object's outputs.
    ByCondition { conditions: Vec<ConditionalPort> },
    /// Use `item.type` as the output port index.
    ByItemType,
    /// Use a numeric label as the output port index.
    ByLabel { label: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionalPort {
    pub condition: Expr,
    pub port: u32,
}

// ---------------------------------------------------------------------------
// Experiments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Experiment {
    pub name: String,
    #[serde(default = "default_replications")]
    pub replications: u32,
    #[serde(default)]
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scenario {
    pub name: String,
    /// Parameters replaced before the run, e.g. `{"object": "Proc1",
    /// "param": "process_time", "value": {...}}`.
    #[serde(default)]
    pub overrides: Vec<Override>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Override {
    pub object: String,
    pub param: String,
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default = "default_version")]
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub time_unit: TimeUnit,
    #[serde(default)]
    pub run: RunSettings,
    pub objects: Vec<Object>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub experiments: Vec<Experiment>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

fn default_format() -> String {
    FORMAT_NAME.to_string()
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

impl Model {
    pub fn from_json(text: &str) -> Result<Model, ModelError> {
        let model: Model =
            serde_json::from_str(text).map_err(|err| ModelError::Json(err.to_string()))?;
        model.validate()?;
        Ok(model)
    }

    pub fn to_json(&self) -> Result<String, ModelError> {
        serde_json::to_string_pretty(self).map_err(|err| ModelError::Json(err.to_string()))
    }

    pub fn object(&self, name: &str) -> Option<&Object> {
        self.objects.iter().find(|object| object.name == name)
    }

    /// Connections leaving `name`, in port order.
    pub fn outputs(&self, name: &str) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|connection| connection.from == name)
            .collect()
    }

    /// Connections entering `name`.
    pub fn inputs(&self, name: &str) -> Vec<&Connection> {
        self.connections
            .iter()
            .filter(|connection| connection.to == name)
            .collect()
    }

    /// Validate the whole model; see [`crate::validate`].
    pub fn validate(&self) -> Result<(), ModelError> {
        crate::validate::validate(self)
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model JSON is invalid: {0}")]
    Json(String),
    #[error("model is invalid:\n{}", format_issues(.0))]
    Invalid(Vec<ValidationIssue>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub location: String,
    pub message: String,
}

impl ValidationIssue {
    pub fn new(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.location.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.location, self.message)
        }
    }
}

fn format_issues(issues: &[ValidationIssue]) -> String {
    issues
        .iter()
        .map(|issue| format!("  - {issue}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn one_u32() -> u32 {
    1
}

fn two_u32() -> u32 {
    2
}

fn one_f64() -> f64 {
    1.0
}

fn ten_f64() -> f64 {
    10.0
}

fn default_spacing() -> f64 {
    1.0
}

fn default_max_content() -> u32 {
    1_000
}

fn one_dist() -> Distribution {
    Distribution::Constant { value: 1.0 }
}

// ---------------------------------------------------------------------------
// Object helpers
// ---------------------------------------------------------------------------

impl Object {
    /// Convenience accessors used by the engine and by validation.
    pub fn source(&self) -> Option<&SourceParams> {
        match &self.params {
            ObjectParams::Source(p) => Some(p),
            _ => None,
        }
    }

    pub fn queue(&self) -> Option<&QueueParams> {
        match &self.params {
            ObjectParams::Queue(p) => Some(p),
            _ => None,
        }
    }

    pub fn processor(&self) -> Option<&ProcessorParams> {
        match &self.params {
            ObjectParams::Processor(p) => Some(p),
            _ => None,
        }
    }

    pub fn conveyor(&self) -> Option<&ConveyorParams> {
        match &self.params {
            ObjectParams::Conveyor(p) => Some(p),
            _ => None,
        }
    }

    pub fn transporter(&self) -> Option<&TransporterParams> {
        match &self.params {
            ObjectParams::Transporter(p) => Some(p),
            _ => None,
        }
    }
}

// The engine needs to evaluate expressions against model-defined conditions;
// keep the error type reachable so callers can report precise problems.
pub use crate::expr::ExprError as ModelExprError;

impl From<ExprError> for ModelError {
    fn from(err: ExprError) -> Self {
        ModelError::Invalid(vec![ValidationIssue::new("", err.to_string())])
    }
}

// ---------------------------------------------------------------------------
// Scenario overrides
// ---------------------------------------------------------------------------

impl Model {
    /// Return a copy of this model with one parameter replaced, as used by
    /// experiments and by `openflex run --set`.
    ///
    /// `param` is a dotted path inside the object's `params` object, for
    /// example `process_time` or `process_time.mean`. The replacement is
    /// re-parsed through the normal schema, so an invalid value is reported
    /// before the scenario runs.
    pub fn with_override(
        &self,
        object: &str,
        param: &str,
        value: serde_json::Value,
    ) -> Result<Model, ModelError> {
        let mut root =
            serde_json::to_value(self).map_err(|err| ModelError::Json(err.to_string()))?;
        let objects = root
            .get_mut("objects")
            .and_then(|value| value.as_array_mut())
            .ok_or_else(|| ModelError::Json("model has no objects array".into()))?;
        let target = objects
            .iter_mut()
            .find(|entry| entry.get("name").and_then(|name| name.as_str()) == Some(object))
            .ok_or_else(|| {
                ModelError::Invalid(vec![ValidationIssue::new(
                    format!("overrides.{object}"),
                    "override refers to an unknown object",
                )])
            })?;
        let params = target
            .get_mut("params")
            .and_then(|value| value.as_object_mut())
            .ok_or_else(|| {
                ModelError::Invalid(vec![ValidationIssue::new(
                    format!("overrides.{object}"),
                    "object has no params object",
                )])
            })?;
        let mut cursor = params;
        let mut parts = param.split('.').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                cursor.insert(part.to_string(), value.clone());
            } else {
                let entry = cursor
                    .entry(part.to_string())
                    .or_insert_with(|| serde_json::json!({}));
                cursor = match entry.as_object_mut() {
                    Some(next) => next,
                    None => {
                        return Err(ModelError::Invalid(vec![ValidationIssue::new(
                            format!("overrides.{object}.{param}"),
                            "cannot descend into a non-object parameter",
                        )]));
                    }
                };
            }
        }
        let model: Model =
            serde_json::from_value(root).map_err(|err| ModelError::Json(err.to_string()))?;
        model.validate()?;
        Ok(model)
    }
}
