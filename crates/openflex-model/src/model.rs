//! The OpenFlex model format.
//!
//! A model is a JSON document describing a discrete-event simulation: a set of
//! objects (the 3D library in FlexSim), the connections between them, the run
//! settings and optional experiments. The format is deliberately explicit and
//! validated before a run starts; the engine never guesses what a model meant.

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::action::{Action, Triggers};
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
#[serde(deny_unknown_fields)]
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
    MultiProcessor,
    Sink,
    Combiner,
    Separator,
    Conveyor,
    Merge,
    Split,
    Transporter,
    Person,
    Agv,
    Elevator,
    /// task-50: mobile manipulator/transporter with 3D travel.
    Robot,
    /// task-50: gantry crane (vertical + horizontal travel).
    Crane,
    /// task-50: AS/RS vehicle travelling on a network.
    AsrsVehicle,
    /// task-50: continuous people conveyor.
    Escalator,
    Dispatcher,
    Resource,
    Rack,
    BasicFr,
    BasicTe,
    FluidGenerator,
    FluidTank,
    FluidValve,
    FluidSink,
    FluidPipe,
    Ticker,
    FluidMixer,
    FluidBlender,
    FluidSplitter,
    FluidProcessor,
    ItemToFluid,
    FluidToItem,
    VisualTool,
    NetworkNode,
    TrafficControl,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::Source => "source",
            ObjectKind::Queue => "queue",
            ObjectKind::Processor => "processor",
            ObjectKind::MultiProcessor => "multiprocessor",
            ObjectKind::Sink => "sink",
            ObjectKind::Combiner => "combiner",
            ObjectKind::Separator => "separator",
            ObjectKind::Conveyor => "conveyor",
            ObjectKind::Merge => "merge",
            ObjectKind::Split => "split",
            ObjectKind::Transporter => "transporter",
            ObjectKind::Person => "person",
            ObjectKind::Agv => "agv",
            ObjectKind::Elevator => "elevator",
            ObjectKind::Robot => "robot",
            ObjectKind::Crane => "crane",
            ObjectKind::AsrsVehicle => "asrs_vehicle",
            ObjectKind::Escalator => "escalator",
            ObjectKind::Dispatcher => "dispatcher",
            ObjectKind::Resource => "resource",
            ObjectKind::Rack => "rack",
            ObjectKind::BasicFr => "basic_fr",
            ObjectKind::BasicTe => "basic_te",
            ObjectKind::FluidGenerator => "fluid_generator",
            ObjectKind::FluidTank => "fluid_tank",
            ObjectKind::FluidValve => "fluid_valve",
            ObjectKind::FluidSink => "fluid_sink",
            ObjectKind::FluidPipe => "fluid_pipe",
            ObjectKind::Ticker => "ticker",
            ObjectKind::FluidMixer => "fluid_mixer",
            ObjectKind::FluidBlender => "fluid_blender",
            ObjectKind::FluidSplitter => "fluid_splitter",
            ObjectKind::FluidProcessor => "fluid_processor",
            ObjectKind::ItemToFluid => "item_to_fluid",
            ObjectKind::FluidToItem => "fluid_to_item",
            ObjectKind::VisualTool => "visual_tool",
            ObjectKind::NetworkNode => "network_node",
            ObjectKind::TrafficControl => "traffic_control",
        }
    }

    pub fn parse(value: &str) -> Option<ObjectKind> {
        Some(match value {
            "source" => ObjectKind::Source,
            "queue" => ObjectKind::Queue,
            "processor" => ObjectKind::Processor,
            "multiprocessor" => ObjectKind::MultiProcessor,
            "sink" => ObjectKind::Sink,
            "combiner" => ObjectKind::Combiner,
            "separator" => ObjectKind::Separator,
            "conveyor" => ObjectKind::Conveyor,
            "merge" => ObjectKind::Merge,
            "split" => ObjectKind::Split,
            "transporter" => ObjectKind::Transporter,
            "person" => ObjectKind::Person,
            "agv" => ObjectKind::Agv,
            "elevator" => ObjectKind::Elevator,
            "robot" => ObjectKind::Robot,
            "crane" => ObjectKind::Crane,
            "asrs_vehicle" => ObjectKind::AsrsVehicle,
            "escalator" => ObjectKind::Escalator,
            "dispatcher" => ObjectKind::Dispatcher,
            "resource" => ObjectKind::Resource,
            "rack" => ObjectKind::Rack,
            "basic_fr" => ObjectKind::BasicFr,
            "basic_te" => ObjectKind::BasicTe,
            "fluid_generator" => ObjectKind::FluidGenerator,
            "fluid_tank" => ObjectKind::FluidTank,
            "fluid_valve" => ObjectKind::FluidValve,
            "fluid_sink" => ObjectKind::FluidSink,
            "fluid_pipe" => ObjectKind::FluidPipe,
            "ticker" => ObjectKind::Ticker,
            "fluid_mixer" => ObjectKind::FluidMixer,
            "fluid_blender" => ObjectKind::FluidBlender,
            "fluid_splitter" => ObjectKind::FluidSplitter,
            "fluid_processor" => ObjectKind::FluidProcessor,
            "item_to_fluid" => ObjectKind::ItemToFluid,
            "fluid_to_item" => ObjectKind::FluidToItem,
            "visual_tool" => ObjectKind::VisualTool,
            "network_node" => ObjectKind::NetworkNode,
            "traffic_control" => ObjectKind::TrafficControl,
            _ => return None,
        })
    }

    /// Every object kind, in library order. Used by `openflex objects list`.
    pub fn all() -> &'static [ObjectKind] {
        &[
            ObjectKind::Source,
            ObjectKind::Queue,
            ObjectKind::Processor,
            ObjectKind::MultiProcessor,
            ObjectKind::Sink,
            ObjectKind::Combiner,
            ObjectKind::Separator,
            ObjectKind::Conveyor,
            ObjectKind::Merge,
            ObjectKind::Split,
            ObjectKind::Transporter,
            ObjectKind::Person,
            ObjectKind::Agv,
            ObjectKind::Elevator,
            ObjectKind::Robot,
            ObjectKind::Crane,
            ObjectKind::AsrsVehicle,
            ObjectKind::Escalator,
            ObjectKind::Dispatcher,
            ObjectKind::Resource,
            ObjectKind::Rack,
            ObjectKind::BasicFr,
            ObjectKind::BasicTe,
            ObjectKind::FluidGenerator,
            ObjectKind::FluidTank,
            ObjectKind::FluidValve,
            ObjectKind::FluidSink,
            ObjectKind::FluidPipe,
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
        ]
    }
}

/// A single object in the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub name: String,
    pub kind: ObjectKind,
    /// Planar position (x, y) in layout units.
    pub pos: [f64; 2],
    /// Height / floor elevation used by the elevator family (B6).
    pub z: f64,
    /// Planar orientation in degrees (counter-clockwise, `None` = 0°).
    ///
    /// Rendering only: the simulation is unaffected by the orientation.
    pub rot: Option<f64>,
    pub params: ObjectParams,
}

impl Object {
    pub fn new(name: impl Into<String>, params: ObjectParams) -> Self {
        let kind = params.kind();
        Self {
            name: name.into(),
            kind,
            pos: [0.0, 0.0],
            z: 0.0,
            rot: None,
            params,
        }
    }
}

impl Serialize for Object {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(6))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("kind", self.kind.as_str())?;
        map.serialize_entry("pos", &self.pos)?;
        if self.z != 0.0 {
            map.serialize_entry("z", &self.z)?;
        }
        if let Some(rot) = self.rot {
            map.serialize_entry("rot", &rot)?;
        }
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
            #[serde(default)]
            z: f64,
            #[serde(default)]
            rot: Option<f64>,
            #[serde(default = "empty_object")]
            params: serde_json::Value,
        }
        fn empty_object() -> serde_json::Value {
            serde_json::json!({})
        }
        // Parse into a value first so unknown keys can be reported together
        // with the object they belong to (A1): serde's own message has the key
        // but not the object name.
        let value = serde_json::Value::deserialize(deserializer)?;
        let object_name = value
            .get("name")
            .and_then(|name| name.as_str())
            .unwrap_or("<unnamed>")
            .to_string();
        if let Some(map) = value.as_object() {
            for key in map.keys() {
                if !matches!(
                    key.as_str(),
                    "name" | "kind" | "pos" | "z" | "rot" | "params"
                ) {
                    return Err(de::Error::custom(format!(
                        "object '{object_name}' has unknown key '{key}' (expected name, kind, pos, z, rot, params)"
                    )));
                }
            }
        }
        let raw: Raw = serde_json::from_value(value)
            .map_err(|err| de::Error::custom(format!("object '{object_name}': {err}")))?;
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
            z: raw.z,
            rot: raw.rot,
            params,
        })
    }
}

/// Type-specific parameters, one variant per object kind.
///
/// Serialised *without* a tag: the enclosing [`Object`] already names the kind.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ObjectParams {
    Source(SourceParams),
    Queue(QueueParams),
    Processor(ProcessorParams),
    MultiProcessor(MultiProcessorParams),
    Sink(SinkParams),
    Combiner(CombinerParams),
    Separator(SeparatorParams),
    Conveyor(ConveyorParams),
    Merge(MergeParams),
    Split(SplitParams),
    Transporter(TransporterParams),
    Person(TransporterParams),
    Agv(AgvParams),
    Elevator(ElevatorParams),
    /// task-50: robot/transporter with 3D travel.
    Robot(TransporterParams),
    /// task-50: crane, vertical + horizontal travel.
    Crane(TransporterParams),
    /// task-50: AS/RS vehicle, network travel.
    AsrsVehicle(TransporterParams),
    /// task-50: escalator, a people conveyor.
    Escalator(ConveyorParams),
    Dispatcher(DispatcherParams),
    Resource(ResourceParams),
    Rack(RackParams),
    BasicFr(BasicFrParams),
    BasicTe(BasicTeParams),
    FluidGenerator(FluidGeneratorParams),
    FluidTank(FluidTankParams),
    FluidValve(FluidValveParams),
    FluidSink(FluidSinkParams),
    FluidPipe(FluidPipeParams),
    Ticker(TickerParams),
    FluidMixer(FluidMixerParams),
    FluidBlender(FluidBlenderParams),
    FluidSplitter(FluidSplitterParams),
    FluidProcessor(FluidProcessorParams),
    ItemToFluid(ItemToFluidParams),
    FluidToItem(FluidToItemParams),
    VisualTool(VisualToolParams),
    NetworkNode(NetworkNodeParams),
    TrafficControl(TrafficControlParams),
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
            ObjectParams::MultiProcessor(_) => ObjectKind::MultiProcessor,
            ObjectParams::Sink(_) => ObjectKind::Sink,
            ObjectParams::Combiner(_) => ObjectKind::Combiner,
            ObjectParams::Separator(_) => ObjectKind::Separator,
            ObjectParams::Conveyor(_) => ObjectKind::Conveyor,
            ObjectParams::Escalator(_) => ObjectKind::Conveyor,
            ObjectParams::Merge(_) => ObjectKind::Merge,
            ObjectParams::Split(_) => ObjectKind::Split,
            ObjectParams::Transporter(_) => ObjectKind::Transporter,
            ObjectParams::Robot(_) => ObjectKind::Transporter,
            ObjectParams::Crane(_) => ObjectKind::Transporter,
            ObjectParams::AsrsVehicle(_) => ObjectKind::Transporter,
            ObjectParams::Person(_) => ObjectKind::Person,
            ObjectParams::Agv(_) => ObjectKind::Agv,
            ObjectParams::Elevator(_) => ObjectKind::Elevator,
            ObjectParams::Dispatcher(_) => ObjectKind::Dispatcher,
            ObjectParams::Resource(_) => ObjectKind::Resource,
            ObjectParams::Rack(_) => ObjectKind::Rack,
            ObjectParams::BasicFr(_) => ObjectKind::BasicFr,
            ObjectParams::BasicTe(_) => ObjectKind::BasicTe,
            ObjectParams::FluidGenerator(_) => ObjectKind::FluidGenerator,
            ObjectParams::FluidTank(_) => ObjectKind::FluidTank,
            ObjectParams::FluidValve(_) => ObjectKind::FluidValve,
            ObjectParams::FluidSink(_) => ObjectKind::FluidSink,
            ObjectParams::FluidPipe(_) => ObjectKind::FluidPipe,
            ObjectParams::Ticker(_) => ObjectKind::Ticker,
            ObjectParams::FluidMixer(_) => ObjectKind::FluidMixer,
            ObjectParams::FluidBlender(_) => ObjectKind::FluidBlender,
            ObjectParams::FluidSplitter(_) => ObjectKind::FluidSplitter,
            ObjectParams::FluidProcessor(_) => ObjectKind::FluidProcessor,
            ObjectParams::ItemToFluid(_) => ObjectKind::ItemToFluid,
            ObjectParams::FluidToItem(_) => ObjectKind::FluidToItem,
            ObjectParams::VisualTool(_) => ObjectKind::VisualTool,
            ObjectParams::NetworkNode(_) => ObjectKind::NetworkNode,
            ObjectParams::TrafficControl(_) => ObjectKind::TrafficControl,
        }
    }

    /// The trigger lists shared by every object kind.
    pub fn triggers(&self) -> &Triggers {
        match self {
            ObjectParams::Source(p) => &p.triggers,
            ObjectParams::Queue(p) => &p.triggers,
            ObjectParams::Processor(p) => &p.triggers,
            ObjectParams::MultiProcessor(p) => &p.triggers,
            ObjectParams::Sink(p) => &p.triggers,
            ObjectParams::Combiner(p) => &p.triggers,
            ObjectParams::Separator(p) => &p.triggers,
            ObjectParams::Conveyor(p) => &p.triggers,
            ObjectParams::Escalator(p) => &p.triggers,
            ObjectParams::Merge(p) => &p.triggers,
            ObjectParams::Split(p) => &p.triggers,
            ObjectParams::Transporter(p) => &p.triggers,
            ObjectParams::Robot(p) => &p.triggers,
            ObjectParams::Crane(p) => &p.triggers,
            ObjectParams::AsrsVehicle(p) => &p.triggers,
            ObjectParams::Person(p) => &p.triggers,
            ObjectParams::Agv(p) => &p.triggers,
            ObjectParams::Elevator(p) => &p.triggers,
            ObjectParams::Dispatcher(p) => &p.triggers,
            ObjectParams::Resource(p) => &p.triggers,
            ObjectParams::Rack(p) => &p.triggers,
            ObjectParams::BasicFr(p) => &p.triggers,
            ObjectParams::BasicTe(p) => &p.triggers,
            ObjectParams::FluidGenerator(p) => &p.triggers,
            ObjectParams::FluidTank(p) => &p.triggers,
            ObjectParams::FluidValve(p) => &p.triggers,
            ObjectParams::FluidSink(p) => &p.triggers,
            ObjectParams::FluidPipe(p) => &p.triggers,
            ObjectParams::Ticker(p) => &p.triggers,
            ObjectParams::FluidMixer(p) => &p.triggers,
            ObjectParams::FluidBlender(p) => &p.triggers,
            ObjectParams::FluidSplitter(p) => &p.triggers,
            ObjectParams::FluidProcessor(p) => &p.triggers,
            ObjectParams::ItemToFluid(p) => &p.triggers,
            ObjectParams::FluidToItem(p) => &p.triggers,
            ObjectParams::VisualTool(p) => &p.triggers,
            ObjectParams::NetworkNode(p) => &p.triggers,
            ObjectParams::TrafficControl(p) => &p.triggers,
        }
    }

    /// Mutable access to the shared trigger lists.
    pub fn triggers_mut(&mut self) -> &mut Triggers {
        match self {
            ObjectParams::Source(p) => &mut p.triggers,
            ObjectParams::Queue(p) => &mut p.triggers,
            ObjectParams::Processor(p) => &mut p.triggers,
            ObjectParams::MultiProcessor(p) => &mut p.triggers,
            ObjectParams::Sink(p) => &mut p.triggers,
            ObjectParams::Combiner(p) => &mut p.triggers,
            ObjectParams::Separator(p) => &mut p.triggers,
            ObjectParams::Conveyor(p) => &mut p.triggers,
            ObjectParams::Escalator(p) => &mut p.triggers,
            ObjectParams::Merge(p) => &mut p.triggers,
            ObjectParams::Split(p) => &mut p.triggers,
            ObjectParams::Transporter(p) => &mut p.triggers,
            ObjectParams::Robot(p) => &mut p.triggers,
            ObjectParams::Crane(p) => &mut p.triggers,
            ObjectParams::AsrsVehicle(p) => &mut p.triggers,
            ObjectParams::Person(p) => &mut p.triggers,
            ObjectParams::Agv(p) => &mut p.triggers,
            ObjectParams::Elevator(p) => &mut p.triggers,
            ObjectParams::Dispatcher(p) => &mut p.triggers,
            ObjectParams::Resource(p) => &mut p.triggers,
            ObjectParams::Rack(p) => &mut p.triggers,
            ObjectParams::BasicFr(p) => &mut p.triggers,
            ObjectParams::BasicTe(p) => &mut p.triggers,
            ObjectParams::FluidGenerator(p) => &mut p.triggers,
            ObjectParams::FluidTank(p) => &mut p.triggers,
            ObjectParams::FluidValve(p) => &mut p.triggers,
            ObjectParams::FluidSink(p) => &mut p.triggers,
            ObjectParams::FluidPipe(p) => &mut p.triggers,
            ObjectParams::Ticker(p) => &mut p.triggers,
            ObjectParams::FluidMixer(p) => &mut p.triggers,
            ObjectParams::FluidBlender(p) => &mut p.triggers,
            ObjectParams::FluidSplitter(p) => &mut p.triggers,
            ObjectParams::FluidProcessor(p) => &mut p.triggers,
            ObjectParams::ItemToFluid(p) => &mut p.triggers,
            ObjectParams::FluidToItem(p) => &mut p.triggers,
            ObjectParams::VisualTool(p) => &mut p.triggers,
            ObjectParams::NetworkNode(p) => &mut p.triggers,
            ObjectParams::TrafficControl(p) => &mut p.triggers,
        }
    }

    /// Availability calendar shared by every object kind (B11).
    pub fn timetable(&self) -> Option<&TimeTable> {
        match self {
            ObjectParams::Source(p) => p.timetable.as_ref(),
            ObjectParams::Queue(p) => p.timetable.as_ref(),
            ObjectParams::Processor(p) => p.timetable.as_ref(),
            ObjectParams::MultiProcessor(p) => p.timetable.as_ref(),
            ObjectParams::Sink(p) => p.timetable.as_ref(),
            ObjectParams::Combiner(p) => p.timetable.as_ref(),
            ObjectParams::Separator(p) => p.timetable.as_ref(),
            ObjectParams::Conveyor(p) => p.timetable.as_ref(),
            ObjectParams::Escalator(p) => p.timetable.as_ref(),
            ObjectParams::Merge(p) => p.timetable.as_ref(),
            ObjectParams::Split(p) => p.timetable.as_ref(),
            ObjectParams::Transporter(p) => p.timetable.as_ref(),
            ObjectParams::Robot(p) => p.timetable.as_ref(),
            ObjectParams::Crane(p) => p.timetable.as_ref(),
            ObjectParams::AsrsVehicle(p) => p.timetable.as_ref(),
            ObjectParams::Person(p) => p.timetable.as_ref(),
            ObjectParams::Agv(p) => p.timetable.as_ref(),
            ObjectParams::Elevator(p) => p.timetable.as_ref(),
            ObjectParams::Dispatcher(p) => p.timetable.as_ref(),
            ObjectParams::Resource(p) => p.timetable.as_ref(),
            ObjectParams::Rack(p) => p.timetable.as_ref(),
            ObjectParams::BasicFr(p) => p.timetable.as_ref(),
            ObjectParams::BasicTe(p) => p.timetable.as_ref(),
            ObjectParams::FluidGenerator(_)
            | ObjectParams::FluidTank(_)
            | ObjectParams::FluidValve(_)
            | ObjectParams::FluidSink(_)
            | ObjectParams::FluidPipe(_) => None,
            ObjectParams::Ticker(_) => None,
            ObjectParams::FluidMixer(_) => None,
            ObjectParams::FluidBlender(_) => None,
            ObjectParams::FluidSplitter(_) => None,
            ObjectParams::FluidProcessor(_) => None,
            ObjectParams::ItemToFluid(_) => None,
            ObjectParams::FluidToItem(_) => None,
            ObjectParams::VisualTool(_) => None,
            ObjectParams::NetworkNode(_) => None,
            ObjectParams::TrafficControl(_) => None,
        }
    }

    /// Mutable access to the availability calendar (B11).
    ///
    /// Fluid kinds keep their schedule in `stop_time`/triggers only, so they
    /// have no calendar and return `None`.
    pub fn timetable_mut(&mut self) -> Option<&mut TimeTable> {
        match self {
            ObjectParams::Source(p) => p.timetable.as_mut(),
            ObjectParams::Queue(p) => p.timetable.as_mut(),
            ObjectParams::Processor(p) => p.timetable.as_mut(),
            ObjectParams::MultiProcessor(p) => p.timetable.as_mut(),
            ObjectParams::Sink(p) => p.timetable.as_mut(),
            ObjectParams::Combiner(p) => p.timetable.as_mut(),
            ObjectParams::Separator(p) => p.timetable.as_mut(),
            ObjectParams::Conveyor(p) => p.timetable.as_mut(),
            ObjectParams::Escalator(p) => p.timetable.as_mut(),
            ObjectParams::Merge(p) => p.timetable.as_mut(),
            ObjectParams::Split(p) => p.timetable.as_mut(),
            ObjectParams::Transporter(p) => p.timetable.as_mut(),
            ObjectParams::Robot(p) => p.timetable.as_mut(),
            ObjectParams::Crane(p) => p.timetable.as_mut(),
            ObjectParams::AsrsVehicle(p) => p.timetable.as_mut(),
            ObjectParams::Person(p) => p.timetable.as_mut(),
            ObjectParams::Agv(p) => p.timetable.as_mut(),
            ObjectParams::Elevator(p) => p.timetable.as_mut(),
            ObjectParams::Dispatcher(p) => p.timetable.as_mut(),
            ObjectParams::Resource(p) => p.timetable.as_mut(),
            ObjectParams::Rack(p) => p.timetable.as_mut(),
            ObjectParams::BasicFr(p) => p.timetable.as_mut(),
            ObjectParams::BasicTe(p) => p.timetable.as_mut(),
            ObjectParams::FluidGenerator(_)
            | ObjectParams::FluidTank(_)
            | ObjectParams::FluidValve(_)
            | ObjectParams::FluidSink(_)
            | ObjectParams::FluidPipe(_) => None,
            ObjectParams::Ticker(_) => None,
            ObjectParams::FluidMixer(_) => None,
            ObjectParams::FluidBlender(_) => None,
            ObjectParams::FluidSplitter(_) => None,
            ObjectParams::FluidProcessor(_) => None,
            ObjectParams::ItemToFluid(_) => None,
            ObjectParams::FluidToItem(_) => None,
            ObjectParams::VisualTool(_) => None,
            ObjectParams::NetworkNode(_) => None,
            ObjectParams::TrafficControl(_) => None,
        }
    }

    /// Replaces the availability calendar (B11); `None` clears it.
    ///
    /// Returns `false` for the fluid kinds, which have no calendar.
    pub fn set_timetable(&mut self, timetable: Option<TimeTable>) -> bool {
        match timetable {
            Some(timetable) => match self.timetable_mut() {
                Some(slot) => {
                    *slot = timetable;
                    true
                }
                None => false,
            },
            None => match self.clear_timetable() {
                Some(()) => true,
                None => false,
            },
        }
    }

    /// Clears the availability calendar when the kind supports one.
    fn clear_timetable(&mut self) -> Option<()> {
        match self {
            ObjectParams::Source(p) => p.timetable = None,
            ObjectParams::Queue(p) => p.timetable = None,
            ObjectParams::Processor(p) => p.timetable = None,
            ObjectParams::MultiProcessor(p) => p.timetable = None,
            ObjectParams::Sink(p) => p.timetable = None,
            ObjectParams::Combiner(p) => p.timetable = None,
            ObjectParams::Separator(p) => p.timetable = None,
            ObjectParams::Conveyor(p) => p.timetable = None,
            ObjectParams::Escalator(p) => p.timetable = None,
            ObjectParams::Merge(p) => p.timetable = None,
            ObjectParams::Split(p) => p.timetable = None,
            ObjectParams::Transporter(p) => p.timetable = None,
            ObjectParams::Robot(p) => p.timetable = None,
            ObjectParams::Crane(p) => p.timetable = None,
            ObjectParams::AsrsVehicle(p) => p.timetable = None,
            ObjectParams::Person(p) => p.timetable = None,
            ObjectParams::Agv(p) => p.timetable = None,
            ObjectParams::Elevator(p) => p.timetable = None,
            ObjectParams::Dispatcher(p) => p.timetable = None,
            ObjectParams::Resource(p) => p.timetable = None,
            ObjectParams::Rack(p) => p.timetable = None,
            ObjectParams::BasicFr(p) => p.timetable = None,
            ObjectParams::BasicTe(p) => p.timetable = None,
            ObjectParams::FluidGenerator(_)
            | ObjectParams::FluidTank(_)
            | ObjectParams::FluidValve(_)
            | ObjectParams::FluidSink(_)
            | ObjectParams::FluidPipe(_) => return None,
            ObjectParams::Ticker(_) => return None,
            ObjectParams::FluidMixer(_) => return None,
            ObjectParams::FluidBlender(_) => return None,
            ObjectParams::FluidSplitter(_) => return None,
            ObjectParams::FluidProcessor(_) => return None,
            ObjectParams::ItemToFluid(_) => return None,
            ObjectParams::FluidToItem(_) => return None,
            ObjectParams::VisualTool(_) => return None,
            ObjectParams::NetworkNode(_) => return None,
            ObjectParams::TrafficControl(_) => return None,
        }
        Some(())
    }

    /// Pull configuration shared by every object kind (A3).
    pub fn pull(&self) -> Option<&PullParams> {
        match self {
            ObjectParams::Source(p) => p.pull.as_ref(),
            ObjectParams::Queue(p) => p.pull.as_ref(),
            ObjectParams::Processor(p) => p.pull.as_ref(),
            ObjectParams::MultiProcessor(p) => p.pull.as_ref(),
            ObjectParams::Sink(p) => p.pull.as_ref(),
            ObjectParams::Combiner(p) => p.pull.as_ref(),
            ObjectParams::Separator(p) => p.pull.as_ref(),
            ObjectParams::Conveyor(p) => p.pull.as_ref(),
            ObjectParams::Escalator(p) => p.pull.as_ref(),
            ObjectParams::Merge(p) => p.pull.as_ref(),
            ObjectParams::Split(p) => p.pull.as_ref(),
            ObjectParams::Transporter(p) => p.pull.as_ref(),
            ObjectParams::Robot(p) => p.pull.as_ref(),
            ObjectParams::Crane(p) => p.pull.as_ref(),
            ObjectParams::AsrsVehicle(p) => p.pull.as_ref(),
            ObjectParams::Person(p) => p.pull.as_ref(),
            ObjectParams::Agv(p) => p.pull.as_ref(),
            ObjectParams::Elevator(p) => p.pull.as_ref(),
            ObjectParams::Dispatcher(p) => p.pull.as_ref(),
            ObjectParams::Resource(p) => p.pull.as_ref(),
            ObjectParams::Rack(p) => p.pull.as_ref(),
            ObjectParams::BasicFr(p) => p.pull.as_ref(),
            ObjectParams::BasicTe(p) => p.pull.as_ref(),
            ObjectParams::FluidGenerator(_)
            | ObjectParams::FluidTank(_)
            | ObjectParams::FluidValve(_)
            | ObjectParams::FluidSink(_)
            | ObjectParams::FluidPipe(_) => None,
            ObjectParams::Ticker(_) => None,
            ObjectParams::FluidMixer(_) => None,
            ObjectParams::FluidBlender(_) => None,
            ObjectParams::FluidSplitter(_) => None,
            ObjectParams::FluidProcessor(_) => None,
            ObjectParams::ItemToFluid(_) => None,
            ObjectParams::FluidToItem(_) => None,
            ObjectParams::VisualTool(_) => None,
            ObjectParams::NetworkNode(_) => None,
            ObjectParams::TrafficControl(_) => None,
        }
    }

    /// Object-level process flow (B7); fluid objects cannot own one.
    pub fn process_flow(&self) -> Option<&ProcessFlow> {
        match self {
            ObjectParams::Source(p) => p.process_flow.as_ref(),
            ObjectParams::Queue(p) => p.process_flow.as_ref(),
            ObjectParams::Processor(p) => p.process_flow.as_ref(),
            ObjectParams::MultiProcessor(p) => p.process_flow.as_ref(),
            ObjectParams::Sink(p) => p.process_flow.as_ref(),
            ObjectParams::Combiner(p) => p.process_flow.as_ref(),
            ObjectParams::Separator(p) => p.process_flow.as_ref(),
            ObjectParams::Conveyor(p) => p.process_flow.as_ref(),
            ObjectParams::Escalator(p) => p.process_flow.as_ref(),
            ObjectParams::Merge(p) => p.process_flow.as_ref(),
            ObjectParams::Split(p) => p.process_flow.as_ref(),
            ObjectParams::Transporter(p) => p.process_flow.as_ref(),
            ObjectParams::Robot(p) => p.process_flow.as_ref(),
            ObjectParams::Crane(p) => p.process_flow.as_ref(),
            ObjectParams::AsrsVehicle(p) => p.process_flow.as_ref(),
            ObjectParams::Person(p) => p.process_flow.as_ref(),
            ObjectParams::Agv(p) => p.process_flow.as_ref(),
            ObjectParams::Elevator(p) => p.process_flow.as_ref(),
            ObjectParams::Dispatcher(p) => p.process_flow.as_ref(),
            ObjectParams::Resource(p) => p.process_flow.as_ref(),
            ObjectParams::Rack(p) => p.process_flow.as_ref(),
            ObjectParams::BasicFr(p) => p.process_flow.as_ref(),
            ObjectParams::BasicTe(p) => p.process_flow.as_ref(),
            ObjectParams::FluidGenerator(_)
            | ObjectParams::FluidTank(_)
            | ObjectParams::FluidValve(_)
            | ObjectParams::FluidSink(_)
            | ObjectParams::FluidPipe(_) => None,
            ObjectParams::Ticker(_) => None,
            ObjectParams::FluidMixer(_) => None,
            ObjectParams::FluidBlender(_) => None,
            ObjectParams::FluidSplitter(_) => None,
            ObjectParams::FluidProcessor(_) => None,
            ObjectParams::ItemToFluid(_) => None,
            ObjectParams::FluidToItem(_) => None,
            ObjectParams::VisualTool(_) => None,
            ObjectParams::NetworkNode(_) => None,
            ObjectParams::TrafficControl(_) => None,
        }
    }

    /// The shift plan shared by every object kind (empty = always on shift).
    pub fn shifts(&self) -> &[Shift] {
        match self {
            ObjectParams::Source(p) => &p.shifts,
            ObjectParams::Queue(p) => &p.shifts,
            ObjectParams::Processor(p) => &p.shifts,
            ObjectParams::MultiProcessor(p) => &p.shifts,
            ObjectParams::Sink(p) => &p.shifts,
            ObjectParams::Combiner(p) => &p.shifts,
            ObjectParams::Separator(p) => &p.shifts,
            ObjectParams::Conveyor(p) => &p.shifts,
            ObjectParams::Escalator(p) => &p.shifts,
            ObjectParams::Merge(p) => &p.shifts,
            ObjectParams::Split(p) => &p.shifts,
            ObjectParams::Transporter(p) => &p.shifts,
            ObjectParams::Robot(p) => &p.shifts,
            ObjectParams::Crane(p) => &p.shifts,
            ObjectParams::AsrsVehicle(p) => &p.shifts,
            ObjectParams::Person(p) => &p.shifts,
            ObjectParams::Agv(p) => &p.shifts,
            ObjectParams::Elevator(p) => &p.shifts,
            ObjectParams::Dispatcher(p) => &p.shifts,
            ObjectParams::Resource(p) => &p.shifts,
            ObjectParams::Rack(p) => &p.shifts,
            ObjectParams::BasicFr(p) => &p.shifts,
            ObjectParams::BasicTe(p) => &p.shifts,
            ObjectParams::FluidGenerator(_)
            | ObjectParams::FluidTank(_)
            | ObjectParams::FluidValve(_)
            | ObjectParams::FluidSink(_)
            | ObjectParams::FluidPipe(_) => &[],
            ObjectParams::Ticker(_) => &[],
            ObjectParams::FluidMixer(_) => &[],
            ObjectParams::FluidBlender(_) => &[],
            ObjectParams::FluidSplitter(_) => &[],
            ObjectParams::FluidProcessor(_) => &[],
            ObjectParams::ItemToFluid(_) => &[],
            ObjectParams::FluidToItem(_) => &[],
            ObjectParams::VisualTool(_) => &[],
            ObjectParams::NetworkNode(_) => &[],
            ObjectParams::TrafficControl(_) => &[],
        }
    }

    /// Parse a `params` JSON value for the given kind.
    pub fn from_value(kind: ObjectKind, value: serde_json::Value) -> Result<ObjectParams, String> {
        Ok(match kind {
            ObjectKind::Source => ObjectParams::Source(decode(value)?),
            ObjectKind::Queue => ObjectParams::Queue(decode(value)?),
            ObjectKind::Processor => ObjectParams::Processor(decode(value)?),
            ObjectKind::MultiProcessor => ObjectParams::MultiProcessor(decode(value)?),
            ObjectKind::Sink => ObjectParams::Sink(decode(value)?),
            ObjectKind::Combiner => ObjectParams::Combiner(decode(value)?),
            ObjectKind::Separator => ObjectParams::Separator(decode(value)?),
            ObjectKind::Conveyor => ObjectParams::Conveyor(decode(value)?),
            ObjectKind::Merge => ObjectParams::Merge(decode(value)?),
            ObjectKind::Split => ObjectParams::Split(decode(value)?),
            ObjectKind::Transporter => ObjectParams::Transporter(decode(value)?),
            ObjectKind::Person => ObjectParams::Person(decode(value)?),
            ObjectKind::Agv => ObjectParams::Agv(decode(value)?),
            ObjectKind::Elevator => ObjectParams::Elevator(decode(value)?),
            ObjectKind::Robot => ObjectParams::Robot(decode(value)?),
            ObjectKind::Crane => ObjectParams::Crane(decode(value)?),
            ObjectKind::AsrsVehicle => ObjectParams::AsrsVehicle(decode(value)?),
            ObjectKind::Escalator => ObjectParams::Escalator(decode(value)?),
            ObjectKind::Dispatcher => ObjectParams::Dispatcher(decode(value)?),
            ObjectKind::Resource => ObjectParams::Resource(decode(value)?),
            ObjectKind::Rack => ObjectParams::Rack(decode(value)?),
            ObjectKind::BasicFr => ObjectParams::BasicFr(decode(value)?),
            ObjectKind::BasicTe => ObjectParams::BasicTe(decode(value)?),
            ObjectKind::FluidGenerator => ObjectParams::FluidGenerator(decode(value)?),
            ObjectKind::FluidTank => ObjectParams::FluidTank(decode(value)?),
            ObjectKind::FluidValve => ObjectParams::FluidValve(decode(value)?),
            ObjectKind::FluidSink => ObjectParams::FluidSink(decode(value)?),
            ObjectKind::FluidPipe => ObjectParams::FluidPipe(decode(value)?),
            ObjectKind::Ticker => ObjectParams::Ticker(decode(value)?),
            ObjectKind::FluidMixer => ObjectParams::FluidMixer(decode(value)?),
            ObjectKind::FluidBlender => ObjectParams::FluidBlender(decode(value)?),
            ObjectKind::FluidSplitter => ObjectParams::FluidSplitter(decode(value)?),
            ObjectKind::FluidProcessor => ObjectParams::FluidProcessor(decode(value)?),
            ObjectKind::ItemToFluid => ObjectParams::ItemToFluid(decode(value)?),
            ObjectKind::FluidToItem => ObjectParams::FluidToItem(decode(value)?),
            ObjectKind::VisualTool => ObjectParams::VisualTool(decode(value)?),
            ObjectKind::NetworkNode => ObjectParams::NetworkNode(decode(value)?),
            ObjectKind::TrafficControl => ObjectParams::TrafficControl(decode(value)?),
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
            ObjectParams::MultiProcessor(p) => p.serialize(serializer),
            ObjectParams::Sink(p) => p.serialize(serializer),
            ObjectParams::Combiner(p) => p.serialize(serializer),
            ObjectParams::Separator(p) => p.serialize(serializer),
            ObjectParams::Conveyor(p) => p.serialize(serializer),
            ObjectParams::Escalator(p) => p.serialize(serializer),
            ObjectParams::Merge(p) => p.serialize(serializer),
            ObjectParams::Split(p) => p.serialize(serializer),
            ObjectParams::Transporter(p) => p.serialize(serializer),
            ObjectParams::Robot(p) => p.serialize(serializer),
            ObjectParams::Crane(p) => p.serialize(serializer),
            ObjectParams::AsrsVehicle(p) => p.serialize(serializer),
            ObjectParams::Person(p) => p.serialize(serializer),
            ObjectParams::Agv(p) => p.serialize(serializer),
            ObjectParams::Elevator(p) => p.serialize(serializer),
            ObjectParams::Dispatcher(p) => p.serialize(serializer),
            ObjectParams::Resource(p) => p.serialize(serializer),
            ObjectParams::Rack(p) => p.serialize(serializer),
            ObjectParams::BasicFr(p) => p.serialize(serializer),
            ObjectParams::BasicTe(p) => p.serialize(serializer),
            ObjectParams::FluidGenerator(p) => p.serialize(serializer),
            ObjectParams::FluidTank(p) => p.serialize(serializer),
            ObjectParams::FluidValve(p) => p.serialize(serializer),
            ObjectParams::FluidSink(p) => p.serialize(serializer),
            ObjectParams::FluidPipe(p) => p.serialize(serializer),
            ObjectParams::Ticker(p) => p.serialize(serializer),
            ObjectParams::FluidMixer(p) => p.serialize(serializer),
            ObjectParams::FluidBlender(p) => p.serialize(serializer),
            ObjectParams::FluidSplitter(p) => p.serialize(serializer),
            ObjectParams::FluidProcessor(p) => p.serialize(serializer),
            ObjectParams::ItemToFluid(p) => p.serialize(serializer),
            ObjectParams::FluidToItem(p) => p.serialize(serializer),
            ObjectParams::VisualTool(p) => p.serialize(serializer),
            ObjectParams::NetworkNode(p) => p.serialize(serializer),
            ObjectParams::TrafficControl(p) => p.serialize(serializer),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-kind parameter structures
// ---------------------------------------------------------------------------

/// Pull configuration shared by every object kind (A3).
///
/// FlexSim lets a fixed resource actively pull from its upstream ports instead
/// of waiting for a push. OpenFlex keeps the same four pieces of state but
/// structures them so they can be validated and diffed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PullParams {
    /// Whether the object pulls instead of waiting for a push.
    #[serde(default)]
    pub enabled: bool,
    /// Routing rule that picks the *input* port to pull from; `None` means
    /// "open every input and take the first ready item".
    #[serde(default)]
    pub strategy: Option<PortStrategy>,
    /// Boolean expression evaluated per ready item: `1` accepts it, `0` skips
    /// it and waits for the next released item. `None` means "accept".
    #[serde(default)]
    pub requirement: Option<Expr>,
}

/// How a Source decides when to create flow items (`ArrivalStyle` in FlexSim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArrivalStyle {
    /// Wait one inter-arrival time after the previous batch left the source.
    #[default]
    InterArrivalTime,
    /// Table driven: each row carries an absolute arrival time.
    ArrivalSchedule,
    /// Table driven without times: the next row starts once the previous batch
    /// has completely left the source.
    ArrivalSequence,
}

/// Table of absolute arrival times (`Arrival Schedule` in FlexSim).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ArrivalSchedule {
    /// Arrival rows; times must be non-decreasing.
    #[serde(default)]
    pub rows: Vec<ArrivalRow>,
    /// Restart the table when the last row has been scheduled. The first row's
    /// time only applies to the first pass; later passes continue at the time
    /// of the last row (FlexSim `Repeat Table`).
    #[serde(default)]
    pub repeat: bool,
}

/// One row of an arrival schedule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArrivalRow {
    /// Absolute model time of this arrival.
    pub time: f64,
    /// Number of flow items created; `0` is legal and adds a gap.
    #[serde(default = "one_u32")]
    pub quantity: u32,
    /// Item type applied to the created items (overrides
    /// [`SourceParams::item_type`]).
    #[serde(default)]
    pub item_type: Option<u32>,
    /// Labels applied to the created items.
    #[serde(default)]
    pub labels: BTreeMap<String, f64>,
}

impl Default for ArrivalRow {
    fn default() -> Self {
        Self {
            time: 0.0,
            quantity: 1,
            item_type: None,
            labels: BTreeMap::new(),
        }
    }
}

/// One row of an arrival sequence (no times: rows advance when the previous
/// batch has left the source).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceRow {
    /// Number of flow items created for this row.
    #[serde(default = "one_u32")]
    pub quantity: u32,
    /// Item type applied to the created items.
    #[serde(default)]
    pub item_type: Option<u32>,
    /// Labels applied to the created items.
    #[serde(default)]
    pub labels: BTreeMap<String, f64>,
}

impl Default for SequenceRow {
    fn default() -> Self {
        Self {
            quantity: 1,
            item_type: None,
            labels: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Explicit arrival style. When omitted it is inferred from the table
    /// fields (`arrival_schedule` / `arrival_sequence`) and defaults to
    /// [`ArrivalStyle::InterArrivalTime`].
    #[serde(default)]
    pub arrival_style: Option<ArrivalStyle>,
    /// Table of absolute arrival times; mutually exclusive with
    /// `interarrival` and `arrival_sequence`.
    #[serde(default)]
    pub arrival_schedule: Option<ArrivalSchedule>,
    /// Table of arrival batches without times; mutually exclusive with
    /// `interarrival` and `arrival_schedule`.
    #[serde(default)]
    pub arrival_sequence: Option<Vec<SequenceRow>>,
    /// Name of a global table whose rows define the arrival schedule (B7).
    ///
    /// The table must have a `time` (or `arrival_time`) column and may have
    /// `quantity`, `item_type` and extra numeric label columns. Mutually
    /// exclusive with `interarrival`, `arrival_schedule` and
    /// `arrival_sequence`. The engine resolves the table with
    /// [`Model::apply_table_reference`] (integration is a follow-up).
    #[serde(default)]
    pub arrival_schedule_from_table: Option<String>,
    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3).
    #[serde(default)]
    pub pull: Option<PullParams>,
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

impl SourceParams {
    /// Effective arrival style: explicit `arrival_style` when present,
    /// otherwise inferred from the table fields.
    pub fn effective_arrival_style(&self) -> ArrivalStyle {
        if let Some(style) = self.arrival_style {
            return style;
        }
        if self.arrival_schedule_from_table.is_some() || self.arrival_schedule.is_some() {
            ArrivalStyle::ArrivalSchedule
        } else if self.arrival_sequence.is_some() {
            ArrivalStyle::ArrivalSequence
        } else {
            ArrivalStyle::InterArrivalTime
        }
    }

    /// Whether the source is really using the inter-arrival distribution.
    pub fn uses_interarrival(&self) -> bool {
        self.effective_arrival_style() == ArrivalStyle::InterArrivalTime
    }

    /// The arrival schedule when the source is in that mode and the schedule
    /// is inline. A table-driven source returns `None` here; resolve it with
    /// [`Model::apply_table_reference`].
    pub fn arrival_schedule(&self) -> Option<&ArrivalSchedule> {
        match self.effective_arrival_style() {
            ArrivalStyle::ArrivalSchedule => self.arrival_schedule.as_ref(),
            _ => None,
        }
    }

    /// The global table backing this source's arrival schedule, if any.
    pub fn arrival_schedule_table(&self) -> Option<&str> {
        self.arrival_schedule_from_table.as_deref()
    }

    /// The arrival sequence rows when the source is in that mode.
    pub fn arrival_sequence(&self) -> Option<&[SequenceRow]> {
        match self.effective_arrival_style() {
            ArrivalStyle::ArrivalSequence => self.arrival_sequence.as_deref(),
            _ => None,
        }
    }
}

impl Default for SourceParams {
    fn default() -> Self {
        Self {
            interarrival: Distribution::default(),
            items_per_arrival: 1,
            max_arrivals: None,
            first_arrival: 0.0,
            stop_time: None,
            arrival_style: None,
            arrival_schedule: None,
            arrival_sequence: None,
            arrival_schedule_from_table: None,
            item_type: 0,
            labels: BTreeMap::new(),
            process_flow: None,
            timetable: None,
            pull: None,
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
            process_flow: None,
            timetable: None,
            pull: None,
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
            num_setup_operators: None,
            num_process_operators: None,
            use_same_num_operators: false,
            operator_priority: 0,
            operator_preempt: false,
            failure: None,
            changeover_time: Distribution::default(),
            shifts: Vec::new(),
            process_flow: None,
            timetable: None,
            pull: None,
            triggers: Triggers::default(),
        }
    }
}

impl Default for CombinerParams {
    fn default() -> Self {
        Self {
            components_per_container: 2,
            mode: CombinerMode::Pack,
            setup_time: Distribution::default(),
            process_time: Distribution::default(),
            setup_operator: None,
            process_operator: None,
            shifts: Vec::new(),
            process_flow: None,
            timetable: None,
            pull: None,
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
            grid_reserve: 0,
            grid_step: None,
            network: Vec::new(),
            shifts: Vec::new(),
            process_flow: None,
            timetable: None,
            pull: None,
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
            process_flow: None,
            timetable: None,
            pull: None,
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
#[serde(deny_unknown_fields)]
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

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Number of operators requested during setup (A10); `None` means "one
    /// operator when `setup_operator` is set".
    #[serde(default)]
    pub num_setup_operators: Option<u32>,
    /// Number of operators requested during processing (A10).
    #[serde(default)]
    pub num_process_operators: Option<u32>,
    /// Reuse the setup operators for processing instead of releasing them and
    /// requesting new ones (FlexSim "Same as Setup", A10).
    #[serde(default)]
    pub use_same_num_operators: bool,
    /// Priority of the operator task sequence, higher first (A10).
    #[serde(default)]
    pub operator_priority: u32,
    /// Preempt the operator task sequence (A10).
    #[serde(default)]
    pub operator_preempt: bool,
    #[serde(default)]
    pub failure: Option<FailureParams>,
    /// Time spent waiting before the processor can accept a new item (changeover).
    #[serde(default)]
    pub changeover_time: Distribution,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureParams {
    /// Mean time between failures (or a full distribution).
    pub mtbf: Distribution,
    /// Mean time to repair (or a full distribution).
    pub mttr: Distribution,
    /// Optional dispatcher used by the repair crew.
    #[serde(default)]
    pub repair_operator: Option<String>,
    /// Absolute time of the first failure (A8). A negative value means
    /// "ignore" and the first failure is sampled from `mtbf`.
    #[serde(default)]
    pub first_failure_time: Option<f64>,
    /// Only time spent in these states counts towards `mtbf`; empty means wall
    /// clock time (A8). Valid names are `idle`, `setup`, `processing`,
    /// `blocked`, `waiting_for_operator`, `waiting_for_transport`, `down`,
    /// `off_shift`.
    #[serde(default)]
    pub count_time_in_states: Vec<String>,
}

/// Multi-slot parallel processor (`MultiProcessor` in FlexSim, B1).
///
/// Every slot runs its own setup/process cycle, so up to `slots` items are
/// processed at the same time. The object holds at most `slots` items in total
/// (in process plus waiting); operators are requested per busy period as a crew
/// of `slots` (see `docs/BACKLOG.md` B1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiProcessorParams {
    /// Number of parallel processing slots (default 1).
    #[serde(default = "one_u32")]
    pub slots: u32,
    /// Setup delay before each item is processed (default constant 0).
    #[serde(default)]
    pub setup_time: Distribution,
    /// Processing delay per item (default constant 1).
    #[serde(default = "one_dist")]
    pub process_time: Distribution,
    /// Dispatcher asked for the crew during setup (used when
    /// `process_operator` is absent).
    #[serde(default)]
    pub setup_operator: Option<String>,
    /// Dispatcher asked for the processing crew.
    #[serde(default)]
    pub process_operator: Option<String>,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

impl Default for MultiProcessorParams {
    fn default() -> Self {
        Self {
            slots: 1,
            setup_time: Distribution::default(),
            process_time: one_dist(),
            setup_operator: None,
            process_operator: None,
            process_flow: None,
            timetable: None,
            pull: None,
            shifts: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SinkParams {
    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CombinerMode {
    /// Put the components inside the container item.
    #[default]
    Pack,
    /// Permanently join the items into one item.
    Join,
    /// Collect a batch, run setup/process, then release every item of the
    /// batch together (FlexSim `batch`, A7).
    Batch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinerParams {
    #[serde(default = "two_u32")]
    pub components_per_container: u32,
    #[serde(default)]
    pub mode: CombinerMode,
    /// Setup delay before the first component of a batch is consumed
    /// (inherited from Processor in FlexSim, A7). Default constant 0.
    #[serde(default)]
    pub setup_time: Distribution,
    /// Process delay per batch (inherited from Processor in FlexSim, A7).
    /// Default constant 0.
    #[serde(default)]
    pub process_time: Distribution,
    /// Dispatcher asked for a resource during setup.
    #[serde(default)]
    pub setup_operator: Option<String>,
    /// Dispatcher asked for a resource during processing.
    #[serde(default)]
    pub process_operator: Option<String>,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

/// Separation mode (`SeparateMode` in FlexSim, A6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SeparateMode {
    /// Unpack the container: `split_count` parts **plus** the container, i.e.
    /// `split_count + 1` released items (0 = every component).
    #[default]
    Unpack,
    /// Split/copy: `split_count` items in total, the container is not kept as
    /// a separate item.
    Split,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SeparatorParams {
    /// Unpack: split off this many parts plus the container (0 = every
    /// component inside the container). The FlexSim "unpack" semantics produce
    /// `split_count + 1` items.
    ///
    /// Split (see [`SeparateMode::Split`]): total number of released items, so
    /// it must be at least 1.
    #[serde(default)]
    pub split_count: u32,
    /// `unpack` (default) or `split`.
    #[serde(default)]
    pub separate_mode: SeparateMode,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

/// A photo eye on a conveyor (B3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoEye {
    /// Distance from the belt entry point (must be inside the belt).
    pub position: f64,
    /// Optional name, useful in diagnostics.
    #[serde(default)]
    pub name: String,
}

/// A processing station on a conveyor (B3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConveyorStation {
    /// Distance from the belt entry point (must be inside the belt).
    pub position: f64,
    /// How long an item stays at the station before it moves on.
    #[serde(default)]
    pub process_time: Distribution,
}

/// A curved conveyor section (B3 extension): travel length is the arc length.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConveyorCurve {
    /// Curve radius in layout units.
    pub radius: f64,
    /// Turn angle in degrees.
    pub angle: f64,
}

/// One segment of a multi-speed conveyor (task-53).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConveyorSegment {
    /// Straight length of the segment.
    pub length: f64,
    /// Belt speed on this segment.
    #[serde(default = "one_f64")]
    pub speed: f64,
    /// Turn angle in degrees at the end of the segment. It only adds arc length
    /// when the conveyor also declares `curve.radius`.
    #[serde(default)]
    pub angle: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Photo eyes mounted on the belt (B3): when an item reaches `position` the
    /// conveyor runs its `on_photo_eye` trigger for that item.
    #[serde(default)]
    pub photo_eyes: Vec<PhotoEye>,
    /// Stations mounted on the belt (B3): an item stops at `position` for
    /// `process_time` before it continues towards the exit.
    #[serde(default)]
    pub stations: Vec<ConveyorStation>,
    /// Join conveyor (B3): number of upstream belts merged onto this belt.
    /// `None`/`1` keeps the plain single-belt behaviour.
    #[serde(default)]
    pub join_inputs: Option<u32>,
    /// Optional curved section appended after the straight length; its arc
    /// length adds travel distance at the same speed.
    #[serde(default)]
    pub curve: Option<ConveyorCurve>,

    /// Multi-speed belt (task-53): when non-empty it replaces `length`/`speed`
    /// and the travel time is the sum of `length / speed` over the segments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<ConveyorSegment>,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

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
            photo_eyes: Vec::new(),
            stations: Vec::new(),
            join_inputs: None,
            curve: None,
            segments: Vec::new(),
            shifts: Vec::new(),
            process_flow: None,
            timetable: None,
            pull: None,
            triggers: Triggers::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MergeParams {
    /// Merge waits until all input ports have an item available.
    #[serde(default)]
    pub wait_for_all: bool,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SplitParams {
    /// Send items to output ports in round-robin order (default) or by item type.
    #[serde(default)]
    pub by_item_type: bool,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

/// One leg of a transporter network (B4).
///
/// The edge connects two model objects by name and is bidirectional; the
/// engine uses Dijkstra over the edges to get the shortest travel distance
/// between the transporter's home and the requesting object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkEdge {
    pub from: String,
    pub to: String,
    pub distance: f64,
    /// One-way edge when true; omitted/false keeps the road usable in both
    /// directions (B6 AGV routing).
    #[serde(default)]
    pub directed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// task-152: how many cells ahead a step-by-step person reserves on its
    /// path (0 = no reservation, only the cell being entered is checked).
    #[serde(default)]
    pub grid_reserve: u32,

    /// task-121: move people cell by cell on the model grid (`true`) instead of
    /// one straight travel leg. `None`/`false` keeps the historical behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_step: Option<bool>,
    /// Optional road network (B4). Legs between the home object and the
    /// requesting object use the shortest path over these edges; without a
    /// network the straight-line distance is used.
    #[serde(default)]
    pub network: Vec<NetworkEdge>,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

/// An automated guided vehicle (B6): transporter semantics plus a battery.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgvParams {
    #[serde(default = "one_u32")]
    pub capacity: u32,
    #[serde(default = "one_f64")]
    pub speed: f64,
    #[serde(default)]
    pub load_time: Distribution,
    #[serde(default)]
    pub unload_time: Distribution,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub network: Vec<NetworkEdge>,
    /// Battery capacity in distance units (None = unlimited).
    #[serde(default)]
    pub battery_capacity: Option<f64>,
    /// Battery distance consumed per travel distance unit (default 1).
    #[serde(default)]
    pub consumption_per_distance: Option<f64>,
    /// Recharge duration when the battery is empty (default constant 0).
    #[serde(default)]
    pub charge_time: Distribution,
    /// Object where charging happens (default: `home`, else in place).
    #[serde(default)]
    pub charger: Option<String>,
    /// task-148: alternative chargers considered when `charger` is set. The
    /// engine picks the reachable one with a free slot (nearest first), so
    /// several AGVs do not all queue at the same station.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chargers: Vec<String>,
    /// task-149: order in which AGVs waiting at a charger are served. The
    /// policy of the AGV that releases a slot decides who goes next, so a
    /// fleet configured the same way behaves consistently.
    #[serde(default)]
    pub charge_queue_order: ChargeQueueOrder,

    #[serde(default)]
    pub process_flow: Option<ProcessFlow>,
    #[serde(default)]
    pub timetable: Option<TimeTable>,
    #[serde(default)]
    pub pull: Option<PullParams>,
    #[serde(default)]
    pub triggers: Triggers,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

impl Default for AgvParams {
    fn default() -> Self {
        Self {
            capacity: 1,
            speed: 1.0,
            load_time: Distribution::default(),
            unload_time: Distribution::default(),
            home: None,
            network: Vec::new(),
            battery_capacity: None,
            consumption_per_distance: None,
            charge_time: Distribution::default(),
            charger: None,
            chargers: Vec::new(),
            charge_queue_order: ChargeQueueOrder::default(),
            process_flow: None,
            timetable: None,
            pull: None,
            triggers: Triggers::default(),
            shifts: Vec::new(),
        }
    }
}

/// An elevator (B6): vertical task executer between object heights (`z`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElevatorParams {
    /// How many operator tasks the cabin can serve at once.
    #[serde(default = "one_u32")]
    pub capacity: u32,
    /// Vertical travel speed (height units per time unit).
    #[serde(default = "one_f64")]
    pub speed: f64,
    /// Dwell time at every stop (doors), default constant 0.
    #[serde(default)]
    pub stop_time: Distribution,
    /// Object the cabin returns to when idle (a floor reference).
    #[serde(default)]
    pub home: Option<String>,

    /// Time the doors need to open when the cabin reaches a floor.
    #[serde(default)]
    pub door_open_time: f64,

    /// Time the doors need to close before the cabin may travel again.
    #[serde(default)]
    pub door_close_time: f64,

    /// Maximum load of the cabin (0 = unlimited). Tasks are weighed with
    /// `load_per_task`, so `max_load = 160` with the default 80 admits two.
    #[serde(default)]
    pub max_load: f64,

    /// Load one task adds to the cabin (default 80 = one person).
    #[serde(default = "default_load_per_task")]
    pub load_per_task: f64,

    /// Shaft group: elevators that share a shaft cannot travel at the same
    /// time, which models a single-car shaft and head-on conflicts.
    #[serde(default)]
    pub shaft: Option<String>,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

fn default_load_per_task() -> f64 {
    80.0
}

impl Default for ElevatorParams {
    fn default() -> Self {
        Self {
            capacity: 1,
            speed: 1.0,
            stop_time: Distribution::default(),
            home: None,
            door_open_time: 0.0,
            door_close_time: 0.0,
            max_load: 0.0,
            load_per_task: default_load_per_task(),
            shaft: None,
            process_flow: None,
            timetable: None,
            pull: None,
            triggers: Triggers::default(),
            shifts: Vec::new(),
        }
    }
}

/// One leg of a dispatcher task sequence (task-44).
///
/// The engine already performs the standard sequence `travel` (to the pickup
/// point), `travel` (to the requester), `load`, `unload`, `travel` (home). A
/// model can add extra legs: every additional `travel` costs one more trip
/// between the pickup point and the requester, every `load`/`unload` costs the
/// task executer's load/unload time again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TaskStep {
    /// Short form: `"travel"`, `"load"` or `"unload"`.
    Simple(TaskStepKind),
    /// Full form (task-62): `{"kind": "travel", "duration": {...},
    /// "condition": "time < 100"}`. A step whose condition evaluates to false
    /// is skipped; `duration` replaces the default time of that step.
    Detailed {
        kind: TaskStepKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration: Option<Distribution>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        condition: Option<Expr>,
    },
}

impl TaskStep {
    /// The kind of the step.
    pub fn kind(&self) -> TaskStepKind {
        match self {
            TaskStep::Simple(kind) => *kind,
            TaskStep::Detailed { kind, .. } => *kind,
        }
    }

    /// Optional duration override.
    pub fn duration(&self) -> Option<&Distribution> {
        match self {
            TaskStep::Simple(_) => None,
            TaskStep::Detailed { duration, .. } => duration.as_ref(),
        }
    }

    /// Optional condition; the step runs only when it evaluates to true.
    pub fn condition(&self) -> Option<&Expr> {
        match self {
            TaskStep::Simple(_) => None,
            TaskStep::Detailed { condition, .. } => condition.as_ref(),
        }
    }
}

/// Kind of one dispatcher task step (task-62).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStepKind {
    /// One trip between the pickup point and the requester.
    #[default]
    Travel,
    /// The task executer's load time.
    Load,
    /// The task executer's unload time.
    Unload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DispatcherParams {
    /// Dispatchers hand out tasks to transporters; the maximum number of tasks
    /// queued at once (0 = unlimited).
    #[serde(default)]
    pub max_tasks: u32,

    /// Optional task sequence (task-44). Empty keeps the standard
    /// travel → load → unload → travel behaviour.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_sequence: Vec<TaskStep>,

    /// task-65: delay before a preempted request is re-queued (0 = at once).
    #[serde(default)]
    pub preempt_retry_delay: f64,

    /// task-65: maximum delayed retries after a preemption (0 = unlimited).
    #[serde(default)]
    pub preempt_max_retries: u32,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceParams {
    #[serde(default = "one_u32")]
    pub capacity: u32,
    /// Optional schedule of unavailable periods.
    #[serde(default)]
    pub downtimes: Vec<DowntimeWindow>,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

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
            process_flow: None,
            timetable: None,
            pull: None,
            triggers: Triggers::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DowntimeWindow {
    pub start: f64,
    pub end: f64,
}

/// A linear availability calendar (B11, "Time Table").
///
/// The object is available inside any of the `windows` and unavailable
/// outside them. Windows are model-time intervals (`[start, end)`) and must not
/// overlap; an empty window list means "always available". The engine reports
/// unavailable time as `off_shift`, the same bucket used by shift plans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TimeTable {
    #[serde(default)]
    pub windows: Vec<DowntimeWindow>,
}

/// One work period of an object's shift plan.
///
/// The window is `[start, start + duration)` and repeats every
/// `repeat_every` time units (`0` means a single window). Objects outside
/// every window are off shift: they stop processing, do not accept flow items
/// and accumulate `off_shift` time in the statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Storage behaviour of a rack (B2).
///
/// `floor` is the existing FIFO queue behaviour. `gravity` (and the
/// `push_back` approximation) release the most recently stored item first
/// (LIFO), because a gravity flow rack pushes new items to the front.
/// `drive_in` keeps the FIFO approximation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RackType {
    #[default]
    Floor,
    Gravity,
    PushBack,
    DriveIn,
}

impl RackType {
    /// Whether items leave the rack in LIFO (last in, first out) order.
    pub fn is_lifo(self) -> bool {
        matches!(self, RackType::Gravity | RackType::PushBack)
    }
}

/// Take-away order of a rack when several levels hold items (task-44).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RackPickPolicy {
    /// Release in arrival order (the historical behaviour).
    #[default]
    Fifo,
    /// Release the item on the lowest occupied level first.
    LevelOrder,
    /// Release the item whose level is closest to the previous pick.
    Nearest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RackParams {
    #[serde(default = "default_max_content")]
    pub capacity: u32,
    #[serde(default = "one_u32")]
    pub levels: u32,
    /// Storage type (B2); controls the release order of the rack.
    #[serde(default)]
    pub rack_type: RackType,
    /// Push-back racks: parallel lanes per level. When set, the usable
    /// capacity is `levels * channels` (bounded by `capacity`).
    #[serde(default)]
    pub channels: Option<u32>,
    /// Drive-in racks: storage depth of a lane. When set, the usable capacity
    /// is `channels * depth` (bounded by `capacity`).
    #[serde(default)]
    pub depth: Option<u32>,

    /// Number of aisles. When set, the usable capacity is also bounded by
    /// `levels * aisles` (task-44).
    #[serde(default)]
    pub aisles: Option<u32>,

    /// Which item the rack releases next (task-44).
    #[serde(default)]
    pub pick_policy: RackPickPolicy,

    /// Number of bays per aisle (task-47). When set, every stored item gets an
    /// explicit (level, aisle, bay) address and the pick takes walking time.
    #[serde(default)]
    pub bays: Option<u32>,

    /// Walking time per bay on the pick trip (0 = instant, the default).
    #[serde(default)]
    pub travel_time_per_bay: f64,

    /// Walking time per aisle on the pick trip (0 = instant, the default).
    #[serde(default)]
    pub aisle_time: f64,

    /// Minimum time an item stays in the rack before it may be released
    /// (0 = release as soon as the downstream object accepts).
    #[serde(default)]
    pub min_dwell: f64,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

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
            rack_type: RackType::Floor,
            channels: None,
            depth: None,
            min_dwell: 0.0,
            aisles: None,
            pick_policy: RackPickPolicy::Fifo,
            bays: None,
            travel_time_per_bay: 0.0,
            aisle_time: 0.0,
            shifts: Vec::new(),
            process_flow: None,
            timetable: None,
            pull: None,
            triggers: Triggers::default(),
        }
    }
}

/// Emptier behaviour of a `basic_fr` object (task-36).
///
/// Semantically this is FlexSim's Emptier: the object periodically clears the
/// objects connected to its input ports. The model format keeps `kind:
/// "basic_fr"` (no extra `ObjectKind`) so older tooling keeps working.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptierParams {
    /// Time between two sweeps.
    #[serde(default = "one_dist")]
    pub interval: Distribution,
    /// Maximum number of items one sweep moves (0 = unlimited).
    #[serde(default)]
    pub max_items: u32,
    /// Remove the items instead of sending them to the object's outputs.
    #[serde(default)]
    pub destroy: bool,
}

impl Default for EmptierParams {
    fn default() -> Self {
        Self {
            interval: one_dist(),
            max_items: 0,
            destroy: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BasicFrParams {
    /// Emptier behaviour (task-36): periodically clear the objects connected to
    /// this object's inputs.
    #[serde(default)]
    pub emptier: Option<EmptierParams>,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

    /// Object logic fired by the engine at the documented trigger points.
    #[serde(default)]
    pub triggers: Triggers,

    /// Optional shift plan; outside every window the object is off shift.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shifts: Vec<Shift>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BasicTeParams {
    #[serde(default = "one_f64")]
    pub speed: f64,
    #[serde(default = "one_u32")]
    pub capacity: u32,

    /// Optional object-level process flow (B7). Its `create` blocks can run at
    /// t = 0 or whenever this object receives an item (`on_entry`), and its
    /// `transfer` blocks can inject tokens as flow items into other objects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,

    /// Linear availability windows (B11): outside them the object is off
    /// shift. `None` means "always available".
    #[serde(default)]
    pub timetable: Option<TimeTable>,

    /// Pull configuration (A3); `None` keeps the documented push default.
    #[serde(default)]
    pub pull: Option<PullParams>,

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

/// A fluid source (`fluid_generator`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidGeneratorParams {
    /// Produced volume per time unit.
    #[serde(default = "one_f64")]
    pub flow_rate: f64,
    /// Stop producing after this model time.
    #[serde(default)]
    pub stop_time: Option<f64>,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidGeneratorParams {
    fn default() -> Self {
        Self {
            flow_rate: 1.0,
            stop_time: None,
            triggers: Triggers::default(),
        }
    }
}

/// A fluid tank (`fluid_tank`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidTankParams {
    /// Maximum stored volume.
    #[serde(default = "hundred_f64")]
    pub capacity: f64,
    /// Volume present when the run starts.
    #[serde(default)]
    pub initial_level: f64,
    /// Maximum released volume per time unit (default: unlimited).
    #[serde(default)]
    pub max_outflow: Option<f64>,
    /// Fluid density (kg/m3), pressure = density * gravity * level.
    #[serde(default)]
    pub density: Option<f64>,
    /// Gravity used for the pressure-height relation (default 9.81).
    #[serde(default)]
    pub gravity: Option<f64>,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidTankParams {
    fn default() -> Self {
        Self {
            capacity: 100.0,
            initial_level: 0.0,
            max_outflow: None,
            density: None,
            gravity: None,
            triggers: Triggers::default(),
        }
    }
}

/// Valve opening curve (task-86).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ValveOpeningCurve {
    /// Effective area proportional to the opening.
    #[default]
    Linear,
    /// Equal-percentage characteristic (area grows geometrically).
    EqualPercentage,
}

/// Order in which AGVs waiting at a charger are served (task-149).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeQueueOrder {
    /// First come, first served (historical behaviour).
    #[default]
    Fifo,
    /// The AGV with the lowest battery charges first.
    LowestBattery,
    /// The AGV that has waited the longest charges first.
    LongestWaiting,
}

/// A fluid valve (`fluid_valve`): passes `ratio` of the incoming flow when open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidValveParams {
    #[serde(default = "default_true")]
    pub open: bool,
    /// Fraction of the incoming flow that passes through (0..1).
    #[serde(default = "one_f64")]
    pub ratio: f64,
    /// Minimum incoming flow rate that opens the valve (legacy flow-rate
    /// threshold: below it the valve latches closed).
    #[serde(default)]
    pub pressure_threshold: Option<f64>,
    /// Inlet pressure that (re)opens the valve (Pa, hysteresis).
    #[serde(default)]
    pub pressure_open: Option<f64>,
    /// task-86: valve opening in 0..1 (default 1 = fully open).
    #[serde(default)]
    pub opening: Option<f64>,
    /// task-86: opening curve (`linear` or `equal_percentage`).
    #[serde(default)]
    pub open_curve: Option<ValveOpeningCurve>,
    /// Inlet pressure below which the valve closes (Pa, hysteresis).
    #[serde(default)]
    pub pressure_close: Option<f64>,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidValveParams {
    fn default() -> Self {
        Self {
            open: true,
            ratio: 1.0,
            pressure_threshold: None,
            pressure_open: None,
            pressure_close: None,
            opening: None,
            open_curve: None,
            triggers: Triggers::default(),
        }
    }
}

/// A fluid pipe (`fluid_pipe`): transports volume with a pure delay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidPipeParams {
    /// Pipe length in layout units.
    #[serde(default = "one_f64")]
    pub length: f64,
    /// Transport speed (length units per time unit).
    #[serde(default = "one_f64")]
    pub speed: f64,
    /// Optional transport delay override (time units).
    #[serde(default)]
    pub delay: Option<f64>,
    /// Optional maximum volume per time unit through the pipe (also the
    /// pressure-flow calibration: delta_p of one reference head = flow_rate).
    #[serde(default)]
    pub flow_rate: Option<f64>,
    /// Fluid temperature in Celsius (simplified viscosity correction).
    #[serde(default)]
    pub temperature: Option<f64>,
    /// task-86: inner diameter (layout units). Together with `friction_factor`
    /// and `density` it enables the Darcy-Weisbach flow law; without them the
    /// `flow_rate` calibration is used unchanged.
    #[serde(default)]
    pub diameter: Option<f64>,
    /// Darcy friction factor `f` (dimensionless).
    #[serde(default)]
    pub friction_factor: Option<f64>,
    /// Fluid density at the reference temperature (kg per volume unit).
    #[serde(default)]
    pub density: Option<f64>,
    /// Linear density/temperature coefficient (1/K, default 0.0002).
    #[serde(default)]
    pub temperature_coefficient: Option<f64>,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidPipeParams {
    fn default() -> Self {
        Self {
            length: 1.0,
            speed: 1.0,
            delay: None,
            flow_rate: None,
            temperature: None,
            diameter: None,
            friction_factor: None,
            density: None,
            temperature_coefficient: None,
            triggers: Triggers::default(),
        }
    }
}

/// A fluid sink (`fluid_sink`): absorbs everything it receives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FluidSinkParams {
    #[serde(default)]
    pub triggers: Triggers,
}

/// One step of a `fluid_mixer`'s step table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MixerStep {
    /// Volume pulled from each input port (index = port).
    #[serde(default)]
    pub pull: Vec<f64>,
    /// Delay after all the material for this step has been collected.
    #[serde(default)]
    pub delay: f64,
}

/// The fluid clock (`ticker`). FlexSim requires exactly one per fluid model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickerParams {
    /// Length of one simulation tick.
    #[serde(default = "one_f64")]
    pub tick_length: f64,
    /// Global list of fluid sub-components, FlexSim's "sub-components" table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for TickerParams {
    fn default() -> Self {
        Self {
            tick_length: 1.0,
            components: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

/// Batch mixer (`fluid_mixer`): waits for a full batch before releasing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidMixerParams {
    /// Product id of the material released by the mixer.
    #[serde(default = "one_f64")]
    pub product_id: f64,
    /// Ordered steps; each pulls from its input ports, then waits `delay`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<MixerStep>,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidMixerParams {
    fn default() -> Self {
        Self {
            product_id: 1.0,
            steps: Vec::new(),
            triggers: Triggers::default(),
        }
    }
}

/// Continuous blender (`fluid_blender`): mixes inputs at a fixed ratio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidBlenderParams {
    /// Desired output rate of the blend.
    #[serde(default = "one_f64")]
    pub output_rate: f64,
    /// Share of the output taken from each input port (normalised on use).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ratios: Vec<f64>,
    /// Share of the incoming material lost in the blend (0..1).
    #[serde(default)]
    pub loss: f64,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidBlenderParams {
    fn default() -> Self {
        Self {
            output_rate: 1.0,
            ratios: Vec::new(),
            loss: 0.0,
            triggers: Triggers::default(),
        }
    }
}

/// One row of a `fluid_splitter`'s percent table (= one output port).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SplitPercent {
    /// Free-text description of the port (modeller use only).
    #[serde(default)]
    pub description: String,
    /// Share of the outgoing material for this port (0..100).
    #[serde(default)]
    pub percent: f64,
}

/// `fluid_splitter` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidSplitterParams {
    /// One entry per output port.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<SplitPercent>,
    /// Total volume the splitter sends per time unit (0 = unlimited).
    #[serde(default)]
    pub output_rate: f64,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidSplitterParams {
    fn default() -> Self {
        Self {
            outputs: Vec::new(),
            output_rate: 0.0,
            triggers: Triggers::default(),
        }
    }
}

/// Continuous processor (`fluid_processor`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidProcessorParams {
    /// Maximum output rate (volume per time unit).
    #[serde(default = "one_f64")]
    pub max_output_rate: f64,
    /// Maximum volume the processor can hold.
    #[serde(default = "hundred_f64")]
    pub max_capacity: f64,
    /// Share of incoming material lost (0..1), e.g. evaporation.
    #[serde(default)]
    pub loss: f64,
    /// First input port used (FlexSim "Receive Port Number").
    #[serde(default)]
    pub receive_port: u32,
    /// First output port used (FlexSim "Destination Port Number").
    #[serde(default)]
    pub destination_port: u32,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidProcessorParams {
    fn default() -> Self {
        Self {
            max_output_rate: 1.0,
            max_capacity: 100.0,
            loss: 0.0,
            receive_port: 0,
            destination_port: 0,
            triggers: Triggers::default(),
        }
    }
}

/// `item_to_fluid`: one discrete flow item becomes fluid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemToFluidParams {
    /// Volume created per discrete unit.
    #[serde(default = "one_f64")]
    pub fluid_per_unit: f64,
    /// Discrete units represented by one flow item.
    #[serde(default = "one_f64")]
    pub units_per_item: f64,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for ItemToFluidParams {
    fn default() -> Self {
        Self {
            fluid_per_unit: 1.0,
            units_per_item: 1.0,
            triggers: Triggers::default(),
        }
    }
}

/// `fluid_to_item`: fluid is packed back into discrete flow items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FluidToItemParams {
    /// Volume of fluid consumed by one flow item.
    #[serde(default = "one_f64")]
    pub fluid_per_item: f64,
    /// Discrete units represented by one flow item.
    #[serde(default = "one_f64")]
    pub units_per_item: f64,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for FluidToItemParams {
    fn default() -> Self {
        Self {
            fluid_per_item: 1.0,
            units_per_item: 1.0,
            triggers: Triggers::default(),
        }
    }
}

/// Shape drawn by a `visual_tool`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VisualShape {
    #[default]
    Box,
    Plane,
    Column,
    Sphere,
}

impl VisualShape {
    pub fn as_str(self) -> &'static str {
        match self {
            VisualShape::Box => "box",
            VisualShape::Plane => "plane",
            VisualShape::Column => "column",
            VisualShape::Sphere => "sphere",
        }
    }

    pub fn parse(value: &str) -> Option<VisualShape> {
        Some(match value {
            "box" => VisualShape::Box,
            "plane" => VisualShape::Plane,
            "column" => VisualShape::Column,
            "sphere" => VisualShape::Sphere,
            _ => return None,
        })
    }
}

fn default_visual_size() -> [f64; 3] {
    [2.0, 2.0, 2.0]
}

/// `visual_tool`: decoration / container with no simulation behaviour.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualToolParams {
    /// Which primitive to draw.
    #[serde(default)]
    pub shape: VisualShape,
    /// Size in model units (x, y, z).
    #[serde(default = "default_visual_size")]
    pub size: [f64; 3],
    /// Fill colour as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Caption drawn on the object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for VisualToolParams {
    fn default() -> Self {
        Self {
            shape: VisualShape::Box,
            size: default_visual_size(),
            color: None,
            text: None,
            triggers: Triggers::default(),
        }
    }
}
/// Traffic control mode (FlexSim `TrafficControl` "Mutual Exclusion" tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrafficMode {
    /// Only `capacity` travellers may occupy the area at the same time.
    #[default]
    MutualExclusion,
    /// Travellers enter freely; only the speed limit applies.
    UnTimed,
}

impl TrafficMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TrafficMode::MutualExclusion => "mutual_exclusion",
            TrafficMode::UnTimed => "un_timed",
        }
    }

    pub fn parse(value: &str) -> Option<TrafficMode> {
        Some(match value {
            "mutual_exclusion" => TrafficMode::MutualExclusion,
            "un_timed" => TrafficMode::UnTimed,
            _ => return None,
        })
    }
}

/// `network_node`: one node of the travel network.
///
/// Nodes connect to each other with ordinary A connections; the edge length is
/// the straight-line distance between their positions unless `edges` overrides
/// it (FlexSim's "Configuring Paths" / distance table).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct NetworkNodeParams {
    /// Explicit edge lengths to other nodes (overrides the euclidean default).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<NetworkEdge>,
    /// FlexSim "Maximum Number of Travelers" (0 = unlimited).
    #[serde(default)]
    pub max_travelers: u32,
    #[serde(default)]
    pub triggers: Triggers,
}

/// `traffic_control`: limits how many travellers occupy a set of nodes.
///
/// The controlled nodes are the ones connected to this object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficControlParams {
    /// How many travellers may be inside the area at once.
    #[serde(default = "one_u32")]
    pub capacity: u32,
    /// What happens when the area is full.
    #[serde(default)]
    pub mode: TrafficMode,
    /// Multiplier applied to traveller speed inside the area (1.0 = unchanged).
    #[serde(default = "one_f64")]
    pub speed_factor: f64,
    #[serde(default)]
    pub triggers: Triggers,
}

impl Default for TrafficControlParams {
    fn default() -> Self {
        Self {
            capacity: 1,
            mode: TrafficMode::MutualExclusion,
            speed_factor: 1.0,
            triggers: Triggers::default(),
        }
    }
}

fn hundred_f64() -> f64 {
    100.0
}

fn default_true() -> bool {
    true
}

/// Kind of a connection (FlexSim A/S connectors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionKind {
    /// A connection: flow items travel along it and output ports route to it
    /// (the default, and the only kind older models know).
    #[default]
    A,
    /// S connection: a center-port/resource link between objects (for example
    /// a processor linked to a dispatcher). S connections do not take part in
    /// output-port routing.
    S,
}

impl ConnectionKind {
    /// The short JSON name (`"a"`/`"s"`).
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionKind::A => "a",
            ConnectionKind::S => "s",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub from: String,
    pub to: String,
    /// Output port number (0-based) when several connections leave an object.
    #[serde(default)]
    pub port: Option<u32>,
    /// Routing rule used to pick among the object's output connections.
    #[serde(default)]
    pub strategy: Option<PortStrategy>,
    /// Fluid link capacity in volume per time unit (fluid connections only);
    /// `None` means "no additional limit".
    #[serde(default)]
    pub flow_rate: Option<f64>,
    /// Connection kind: A (flow/port routing) when omitted, or S (center
    /// port / resource link).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ConnectionKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Hold the item: do not release it until a `release_item` action runs
    /// (FlexSim `SendToPort` returning -1, A4).
    Hold,
}

/// One conditional routing rule: either an expression condition or a
/// sandboxed script condition (mutually exclusive in the JSON form).
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionalPort {
    /// Expression condition. When [`script_condition`](Self::script_condition)
    /// is set this holds the placeholder `true` and the engine ignores it.
    pub condition: Expr,
    /// Optional FlexScript (rhai) condition; mutually exclusive with an
    /// explicit `condition` in the JSON form.
    pub script_condition: Option<String>,
    /// Output port used when the condition holds.
    pub port: u32,
}

impl ConditionalPort {
    /// An expression-routed port.
    pub fn with_condition(condition: Expr, port: u32) -> Self {
        Self {
            condition,
            script_condition: None,
            port,
        }
    }

    /// A script-routed port.
    pub fn with_script(source: impl Into<String>, port: u32) -> Self {
        Self {
            condition: Expr::Bool(true),
            script_condition: Some(source.into()),
            port,
        }
    }
}

impl Serialize for ConditionalPort {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // A script-only port must not serialise the internal `true`
        // placeholder, otherwise the document would no longer parse.
        let script_only = self.script_condition.is_some() && self.condition == Expr::Bool(true);
        let fields = 1 + usize::from(!script_only) + usize::from(self.script_condition.is_some());
        let mut map = serializer.serialize_map(Some(fields))?;
        if !script_only {
            map.serialize_entry("condition", &self.condition)?;
        }
        if let Some(script) = &self.script_condition {
            map.serialize_entry("script_condition", script)?;
        }
        map.serialize_entry("port", &self.port)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for ConditionalPort {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let map = value
            .as_object()
            .ok_or_else(|| de::Error::custom("a conditional port must be a JSON object"))?;
        for key in map.keys() {
            if !matches!(key.as_str(), "condition" | "script_condition" | "port") {
                return Err(de::Error::custom(format!(
                    "conditional port has unknown key '{key}' (expected condition, script_condition, port)"
                )));
            }
        }
        let port = map
            .get("port")
            .ok_or_else(|| de::Error::custom("a conditional port needs a 'port' field"))?;
        let port: u32 = serde_json::from_value(port.clone())
            .map_err(|err| de::Error::custom(format!("conditional port 'port': {err}")))?;
        let condition =
            match map.get("condition") {
                Some(value) => Some(serde_json::from_value::<Expr>(value.clone()).map_err(
                    |err| de::Error::custom(format!("conditional port 'condition': {err}")),
                )?),
                None => None,
            };
        let script_condition = match map.get("script_condition") {
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| de::Error::custom("'script_condition' must be a string"))?
                    .to_string(),
            ),
            None => None,
        };
        match (condition, script_condition) {
            (None, None) => Err(de::Error::custom(
                "a conditional port needs either 'condition' or 'script_condition'",
            )),
            (Some(_), Some(_)) => Err(de::Error::custom(
                "conditional port 'condition' and 'script_condition' are mutually exclusive",
            )),
            (Some(condition), None) => Ok(ConditionalPort {
                condition,
                script_condition: None,
                port,
            }),
            (None, Some(script)) => Ok(ConditionalPort {
                condition: Expr::Bool(true),
                script_condition: Some(script),
                port,
            }),
        }
    }
}

// Internally tagged enums cannot use `deny_unknown_fields` in serde, so the
// strategy object is decoded by hand: this is also where a typo such as
// `"percent"` instead of `"percentages"` is reported (A1).
impl<'de> Deserialize<'de> for PortStrategy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let mut map = value
            .as_object()
            .cloned()
            .ok_or_else(|| de::Error::custom("a port strategy must be a JSON object"))?;
        let tag = map
            .remove("type")
            .ok_or_else(|| de::Error::custom("a port strategy needs a \"type\" field"))?;
        let name = tag
            .as_str()
            .ok_or_else(|| de::Error::custom("\"type\" must be a string"))?
            .to_string();
        let strategy = match name.as_str() {
            "first_available" => {
                check_strategy_keys(&map, &name, &[]).map_err(de::Error::custom)?;
                PortStrategy::FirstAvailable
            }
            "round_robin" => {
                check_strategy_keys(&map, &name, &[]).map_err(de::Error::custom)?;
                PortStrategy::RoundRobin
            }
            "random" => {
                check_strategy_keys(&map, &name, &[]).map_err(de::Error::custom)?;
                PortStrategy::Random
            }
            "by_percentage" => {
                check_strategy_keys(&map, &name, &["percentages"]).map_err(de::Error::custom)?;
                PortStrategy::ByPercentage {
                    percentages: strategy_field(&mut map, "percentages", &name)
                        .map_err(de::Error::custom)?,
                }
            }
            "by_condition" => {
                check_strategy_keys(&map, &name, &["conditions"]).map_err(de::Error::custom)?;
                PortStrategy::ByCondition {
                    conditions: strategy_field(&mut map, "conditions", &name)
                        .map_err(de::Error::custom)?,
                }
            }
            "by_item_type" => {
                check_strategy_keys(&map, &name, &[]).map_err(de::Error::custom)?;
                PortStrategy::ByItemType
            }
            "by_label" => {
                check_strategy_keys(&map, &name, &["label"]).map_err(de::Error::custom)?;
                PortStrategy::ByLabel {
                    label: strategy_field(&mut map, "label", &name).map_err(de::Error::custom)?,
                }
            }
            "hold" => {
                check_strategy_keys(&map, &name, &[]).map_err(de::Error::custom)?;
                PortStrategy::Hold
            }
            other => {
                return Err(de::Error::custom(format!(
                    "unknown port strategy '{other}' (expected one of: first_available, round_robin, random, by_percentage, by_condition, by_item_type, by_label, hold)"
                )));
            }
        };
        Ok(strategy)
    }
}

fn check_strategy_keys(
    map: &serde_json::Map<String, serde_json::Value>,
    name: &str,
    allowed: &[&str],
) -> Result<(), String> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(if allowed.is_empty() {
                format!("port strategy '{name}' has unknown key '{key}'")
            } else {
                format!(
                    "port strategy '{name}' has unknown key '{key}' (expected one of: {})",
                    allowed.join(", ")
                )
            });
        }
    }
    Ok(())
}

fn strategy_field<T: serde::de::DeserializeOwned>(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    name: &str,
) -> Result<T, String> {
    let value = map
        .remove(key)
        .ok_or_else(|| format!("port strategy '{name}' requires '{key}'"))?;
    serde_json::from_value(value).map_err(|err| format!("port strategy '{name}.{key}': {err}"))
}

// ---------------------------------------------------------------------------
// Experiments
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Experiment {
    pub name: String,
    #[serde(default = "default_replications")]
    pub replications: u32,
    #[serde(default)]
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    /// Parameters replaced before the run, e.g. `{"object": "Proc1",
    /// "param": "process_time", "value": {...}}`.
    #[serde(default)]
    pub overrides: Vec<Override>,
    /// Replaces `run.stop_time` for this scenario (A13). Must be positive.
    #[serde(default)]
    pub stop_time: Option<f64>,
    /// Replaces `run.warmup` for this scenario (A13). Must be non-negative and
    /// shorter than the effective stop time.
    #[serde(default)]
    pub warmup: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Override {
    pub object: String,
    pub param: String,
    pub value: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Process Flow (B7)
// ---------------------------------------------------------------------------

/// A Process Flow graph attached to the model.
///
/// Tokens arrive at `create` blocks, travel along the connections, and are
/// destroyed by `sink` blocks. The graph must be acyclic and every block id
/// unique; the engine runs it next to the ordinary object model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProcessFlow {
    #[serde(default)]
    pub blocks: Vec<FlowBlock>,
    #[serde(default)]
    pub connections: Vec<FlowConnection>,
}

/// One block of a process flow.
///
/// The `kind` tag selects the block parameters, which are inlined next to the
/// id: `{"id": "c1", "kind": "create", "quantity": 10, ...}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowBlock {
    pub id: String,
    #[serde(flatten)]
    pub kind: FlowBlockKind,
}

/// Parameters of every block kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FlowBlockKind {
    /// Creates tokens, either once (`max_creations: 1`) or periodically.
    Create(FlowCreateParams),
    /// Holds every token for a sampled delay before releasing it.
    Delay(FlowDelayParams),
    /// Applies label/type actions to the token.
    Assign(FlowAssignParams),
    /// Branches on token labels/type via ordered conditions.
    Decision(FlowDecisionParams),
    /// Destroys the token.
    Sink,
    /// Converts the token into a flow item and injects it into an object, or
    /// pulls a buffered item out of an object and continues as a token.
    Transfer(FlowTransferParams),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowCreateParams {
    /// Model time of the first creation (default 0).
    #[serde(default)]
    pub first_arrival: f64,
    /// Time between creation batches (default constant 0 = create once).
    #[serde(default)]
    pub interarrival: Distribution,
    /// Tokens created per batch (default 1).
    #[serde(default = "one_u32")]
    pub quantity: u32,
    /// Stop after this many batches (default: run until the stop time).
    #[serde(default)]
    pub max_creations: Option<u32>,
    /// Item type carried by the tokens.
    #[serde(default)]
    pub item_type: u32,
    /// Labels carried by the tokens.
    #[serde(default)]
    pub labels: BTreeMap<String, f64>,
    /// Object-level flows only: create this batch whenever the owning object
    /// receives a flow item instead of using the arrival schedule.
    #[serde(default)]
    pub on_entry: bool,
}

impl Default for FlowCreateParams {
    fn default() -> Self {
        Self {
            first_arrival: 0.0,
            interarrival: Distribution::default(),
            quantity: 1,
            max_creations: None,
            item_type: 0,
            labels: BTreeMap::new(),
            on_entry: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowDelayParams {
    /// How long a token stays in the block.
    #[serde(default = "one_dist")]
    pub time: Distribution,
}

impl Default for FlowDelayParams {
    fn default() -> Self {
        Self { time: one_dist() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FlowAssignParams {
    /// Label/type actions applied to each token (other actions are rejected).
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FlowDecisionParams {
    /// Ordered conditions; the first match selects a port. When no condition
    /// matches the first outgoing connection is used as the default branch.
    #[serde(default)]
    pub conditions: Vec<ConditionalPort>,
}

/// Direction of a `transfer` block (B7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
    /// Convert the token into a flow item and inject it into `object`.
    #[default]
    ToObject,
    /// Take a flow item from `object` (queue/rack) and continue as a token.
    FromObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowTransferParams {
    /// Object that should receive the token as a flow item.
    pub object: String,
    /// Transfer direction (default: into the object).
    #[serde(default)]
    pub mode: TransferMode,
}

/// One directed connection between two flow blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowConnection {
    pub from: String,
    pub to: String,
    /// Output port of the source block (used by decision blocks). When omitted
    /// the declaration order is the port number.
    #[serde(default)]
    pub port: Option<u32>,
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Optional walking grid used by person A* routing (B6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    /// Number of cells along x.
    pub width: u32,
    /// Number of cells along y.
    pub height: u32,
    /// Size of one cell in layout units (default 1).
    #[serde(default = "one_f64")]
    pub cell_size: f64,
    /// Blocked cells `[x, y]` (4-neighbour movement, no diagonals).
    #[serde(default)]
    pub obstacles: Vec<[i32; 2]>,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            width: 1,
            height: 1,
            cell_size: 1.0,
            obstacles: Vec::new(),
        }
    }
}

/// One cell of a global table.
///
/// Cells are JSON scalars: a number, a boolean or a string. Serialisation is
/// untagged, so the JSON stays `1`, `true` or `"text"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TableCell {
    /// Numeric cell (integers are stored as `f64`).
    Number(f64),
    /// Boolean cell.
    Bool(bool),
    /// String cell.
    Str(String),
}

impl TableCell {
    /// The cell as a number, when it is numeric (booleans convert to 0/1).
    pub fn as_number(&self) -> Option<f64> {
        match self {
            TableCell::Number(value) => Some(*value),
            TableCell::Bool(value) => Some(if *value { 1.0 } else { 0.0 }),
            TableCell::Str(_) => None,
        }
    }

    /// The cell as text, using the same formatting as expressions.
    pub fn as_text(&self) -> String {
        match self {
            TableCell::Number(value) => {
                if value.fract() == 0.0 && value.is_finite() && value.abs() < 1e15 {
                    format!("{value:.0}")
                } else {
                    format!("{value}")
                }
            }
            TableCell::Bool(value) => value.to_string(),
            TableCell::Str(value) => value.clone(),
        }
    }
}

/// A named table of values (FlexSim Global Table, B7).
///
/// Global tables are read-only model data; expressions can later read a cell
/// with `table.<name>[<row>][<column>]` (engine integration is a follow-up).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GlobalTable {
    /// Column names, in display order. Every row must have one cell per
    /// column.
    #[serde(default)]
    pub columns: Vec<String>,
    /// Table rows.
    #[serde(default)]
    pub rows: Vec<Vec<TableCell>>,
}

impl GlobalTable {
    /// Number of columns (0 for a table without a header).
    pub fn width(&self) -> usize {
        self.columns.len()
    }

    /// Number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when the table has no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Cell lookup by row index and column name.
    pub fn cell(&self, row: usize, column: &str) -> Option<&TableCell> {
        let column = self.columns.iter().position(|name| name == column)?;
        self.rows.get(row)?.get(column)
    }

    /// Set one cell of an existing row.
    ///
    /// The table shape is defined by `columns`; an out-of-range row or column
    /// is an error rather than a silent resize.
    pub fn set_cell(&mut self, row: usize, column: usize, value: TableCell) -> Result<(), String> {
        let width = self.columns.len();
        if column >= width {
            return Err(format!(
                "column {column} is out of range (table has {width} columns)"
            ));
        }
        let row_count = self.rows.len();
        let cells = self
            .rows
            .get_mut(row)
            .ok_or_else(|| format!("row {row} is out of range (table has {row_count} rows)"))?;
        cells[column] = value;
        Ok(())
    }

    /// Append a row; its width must match the column count.
    pub fn insert_row(&mut self, row: Vec<TableCell>) -> Result<(), String> {
        if row.len() != self.columns.len() {
            return Err(format!(
                "row has {} cells but the table has {} columns",
                row.len(),
                self.columns.len()
            ));
        }
        self.rows.push(row);
        Ok(())
    }

    /// Remove the row at `index`, returning it (`None` when out of range).
    pub fn remove_row(&mut self, index: usize) -> Option<Vec<TableCell>> {
        if index < self.rows.len() {
            Some(self.rows.remove(index))
        } else {
            None
        }
    }
}

/// Deserialise the global table map while rejecting duplicate table names.
///
/// `serde_json` normally keeps the last duplicate key silently; a duplicated
/// global table is a modelling error, so the map is built by hand.
fn deserialize_global_tables<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, GlobalTable>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TableMapVisitor;

    impl<'de> Visitor<'de> for TableMapVisitor {
        type Value = BTreeMap<String, GlobalTable>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map of global table names to tables")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut tables = BTreeMap::new();
            while let Some((name, table)) = access.next_entry::<String, GlobalTable>()? {
                if tables.insert(name.clone(), table).is_some() {
                    return Err(de::Error::custom(format!(
                        "duplicate global table '{name}'"
                    )));
                }
            }
            Ok(tables)
        }
    }

    deserializer.deserialize_map(TableMapVisitor)
}

/// The JSON kind of a global variable value (FlexSim `ModelingTools/GlobalVariables`).
///
/// FlexSim numbers global variables and lets a modeller assign a numeric or a
/// string value. OpenFlex stores the value as untyped JSON plus this explicit
/// kind so validation can reject a value that does not match the declared kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalVariableKind {
    /// The value must be a JSON number (finite at validation time).
    Number,
    /// The value must be a JSON string.
    String,
}

impl GlobalVariableKind {
    /// The JSON name (`"number"` / `"string"`).
    pub fn as_str(self) -> &'static str {
        match self {
            GlobalVariableKind::Number => "number",
            GlobalVariableKind::String => "string",
        }
    }
}

/// One global variable: an explicit kind plus the raw JSON value.
///
/// JSON form: `{ "kind": "number", "value": 12.5 }` or
/// `{ "kind": "string", "value": "vip" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalVariable {
    /// Declared type of [`value`](Self::value).
    pub kind: GlobalVariableKind,
    /// The value itself; must match `kind` (checked by validation).
    pub value: serde_json::Value,
}

impl GlobalVariable {
    /// A numeric global variable.
    pub fn number(value: f64) -> Self {
        Self {
            kind: GlobalVariableKind::Number,
            value: serde_json::json!(value),
        }
    }

    /// A string global variable.
    pub fn string(value: impl Into<String>) -> Self {
        Self {
            kind: GlobalVariableKind::String,
            value: serde_json::Value::String(value.into()),
        }
    }

    /// The value as a number, when the kind is `number` and the JSON is numeric.
    pub fn as_number(&self) -> Option<f64> {
        match self.kind {
            GlobalVariableKind::Number => self.value.as_f64(),
            GlobalVariableKind::String => None,
        }
    }

    /// The value as text, when the kind is `string`.
    pub fn as_str(&self) -> Option<&str> {
        match self.kind {
            GlobalVariableKind::String => self.value.as_str(),
            GlobalVariableKind::Number => None,
        }
    }
}

/// One entry of a [`PickList`]: a display name and the value it stands for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PickListEntry {
    /// Display name shown in the GUI picker; must be non-empty and unique
    /// inside its list.
    pub name: String,
    /// The value the entry resolves to (FlexSim stores it as text, numbers are
    /// parsed when the list is used as a numeric source).
    pub value: String,
}

/// A named pick list (FlexSim `PickLists/`).
///
/// A pick list maps display names to values so a model can present a friendly
/// choice while the simulation works with the underlying value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PickList {
    /// The entries, in display order; a pick list needs at least one.
    #[serde(default)]
    pub entries: Vec<PickListEntry>,
}

impl PickList {
    /// Build a list from `(name, value)` pairs.
    pub fn new<I, N, V>(entries: I) -> Self
    where
        I: IntoIterator<Item = (N, V)>,
        N: Into<String>,
        V: Into<String>,
    {
        Self {
            entries: entries
                .into_iter()
                .map(|(name, value)| PickListEntry {
                    name: name.into(),
                    value: value.into(),
                })
                .collect(),
        }
    }

    /// Look up the value of an entry by its display name.
    pub fn value_of(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.value.as_str())
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the list has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Deserialise a map of named model items while rejecting duplicate names.
///
/// `serde_json` keeps the last duplicate key silently; for global variables and
/// pick lists a duplicated name is a modelling error, so the map is built by
/// hand (this mirrors [`deserialize_global_tables`]).
fn deserialize_named_map<'de, D, V>(
    deserializer: D,
    what: &'static str,
) -> Result<BTreeMap<String, V>, D::Error>
where
    D: Deserializer<'de>,
    V: Deserialize<'de>,
{
    struct NamedMapVisitor<V> {
        what: &'static str,
        marker: std::marker::PhantomData<V>,
    }

    impl<'de, V: Deserialize<'de>> Visitor<'de> for NamedMapVisitor<V> {
        type Value = BTreeMap<String, V>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a map of {} names to definitions", self.what)
        }

        fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Self::Value, A::Error> {
            let mut items = BTreeMap::new();
            while let Some((name, value)) = access.next_entry::<String, V>()? {
                if items.insert(name.clone(), value).is_some() {
                    return Err(de::Error::custom(format!(
                        "duplicate {} '{name}'",
                        self.what
                    )));
                }
            }
            Ok(items)
        }
    }

    deserializer.deserialize_map(NamedMapVisitor {
        what,
        marker: std::marker::PhantomData,
    })
}

fn deserialize_global_variables<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, GlobalVariable>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_named_map(deserializer, "global variable")
}

fn deserialize_picklists<'de, D>(deserializer: D) -> Result<BTreeMap<String, PickList>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_named_map(deserializer, "pick list")
}

/// A user-defined performance measure (FlexSim Performance Measure).
///
/// The expression is evaluated once at the end of every replication against
/// the built-in variables (`time`, `created`, `completed`), global variables
/// and object fields; the reported statistic is its mean over replications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceMeasure {
    /// Identifier shown in the report and accepted by dashboards.
    pub name: String,
    /// Expression text (same language as trigger conditions).
    pub expression: String,
    /// Optional display unit, e.g. `"items"` or `"min"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

/// A model-declared time-series variable (FlexSim Tracked Variables).
///
/// The expression is sampled on a fixed model-time interval during the run;
/// samples appear in `SimReport::tracked_series`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackedVariable {
    /// Identifier used as the series name.
    pub name: String,
    /// Expression sampled at every interval.
    pub expression: String,
}

/// Model-level triggers (FlexSim model start / reset / stop).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelTriggers {
    /// Executed once before the first event of a fresh run.
    #[serde(default)]
    pub on_start: Vec<Action>,
    /// Executed when the warm-up boundary resets the statistics.
    #[serde(default)]
    pub on_reset: Vec<Action>,
    /// Executed when the run reaches its stop time.
    #[serde(default)]
    pub on_stop: Vec<Action>,
}

impl ModelTriggers {
    pub fn is_empty(&self) -> bool {
        self.on_start.is_empty() && self.on_reset.is_empty() && self.on_stop.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_flow: Option<ProcessFlow>,
    /// Fixed integration step (model time units) for the fluid objects
    /// (default 0.1). Ignored by models without fluid objects.
    #[serde(default)]
    pub fluid_step: Option<f64>,
    /// Optional walking grid for `person` A* routing (B6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<Grid>,
    /// Named global tables (B7); empty by default and omitted from the JSON
    /// when empty, so existing models are unchanged.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_global_tables"
    )]
    pub global_tables: BTreeMap<String, GlobalTable>,
    /// Named global variables (FlexSim `ModelingTools/GlobalVariables`); empty
    /// by default and omitted from the JSON when empty.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_global_variables"
    )]
    pub global_variables: BTreeMap<String, GlobalVariable>,
    /// Named pick lists (FlexSim `PickLists/`); empty by default and omitted
    /// from the JSON when empty.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "deserialize_picklists"
    )]
    pub picklists: BTreeMap<String, PickList>,
    /// Model-level `on_start` / `on_reset` / `on_stop` triggers.
    #[serde(default, skip_serializing_if = "ModelTriggers::is_empty")]
    pub model_triggers: ModelTriggers,
    /// User-defined performance measures, evaluated per replication.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub performance_measures: Vec<PerformanceMeasure>,
    /// Model-declared tracked variables, sampled over time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracked_variables: Vec<TrackedVariable>,
    /// Sampling interval for tracked variables (model time units, default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracked_interval: Option<f64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

/// Index of the first column matching one of `names`.
fn find_table_column(table: &GlobalTable, names: &[&str]) -> Option<usize> {
    table
        .columns
        .iter()
        .position(|column| names.iter().any(|name| column == name))
}

fn default_format() -> String {
    FORMAT_NAME.to_string()
}

fn default_version() -> u32 {
    FORMAT_VERSION
}

impl Model {
    pub fn from_json(text: &str) -> Result<Model, ModelError> {
        let model = Model::parse_json(text)?;
        model.validate()?;
        Ok(model)
    }

    /// Parse without running [`Model::validate`].
    ///
    /// Only useful for tools that want to report every validation issue of a
    /// structurally valid document; `from_json` is the normal entry point.
    ///
    /// Unknown keys are rejected. Structs use `deny_unknown_fields`; the
    /// internally tagged enums (`distribution`, `action`, port `strategy`) are
    /// deserialized by serde with tag buffering, where serde cannot enforce
    /// `deny_unknown_fields`, so the same guarantee is provided here by
    /// observing the ignored keys (A1).
    pub fn parse_json(text: &str) -> Result<Model, ModelError> {
        let mut ignored: Vec<String> = Vec::new();
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let model: Model =
            serde_ignored::deserialize(&mut deserializer, |path| ignored.push(path.to_string()))
                .map_err(|err| ModelError::Json(err.to_string()))?;
        if ignored.is_empty() {
            return Ok(model);
        }
        let names: Vec<&str> = model
            .objects
            .iter()
            .map(|object| object.name.as_str())
            .collect();
        let unknown = ignored
            .iter()
            .map(|path| describe_ignored_path(path, &names))
            .collect::<Vec<_>>()
            .join(", ");
        Err(ModelError::Json(format!(
            "unknown parameter(s) (the schema is strict): {unknown}"
        )))
    }

    pub fn to_json(&self) -> Result<String, ModelError> {
        serde_json::to_string_pretty(self).map_err(|err| ModelError::Json(err.to_string()))
    }

    pub fn object(&self, name: &str) -> Option<&Object> {
        self.objects.iter().find(|object| object.name == name)
    }

    /// A named global table, if the model defines it.
    pub fn table(&self, name: &str) -> Option<&GlobalTable> {
        self.global_tables.get(name)
    }

    /// A named global variable, if the model defines it.
    pub fn global_variable(&self, name: &str) -> Option<&GlobalVariable> {
        self.global_variables.get(name)
    }

    /// Mutable access to a named global variable (in-memory edits).
    pub fn global_variable_mut(&mut self, name: &str) -> Option<&mut GlobalVariable> {
        self.global_variables.get_mut(name)
    }

    /// A named pick list, if the model defines it.
    pub fn picklist(&self, name: &str) -> Option<&PickList> {
        self.picklists.get(name)
    }

    /// Mutable access to a named pick list (in-memory edits).
    pub fn picklist_mut(&mut self, name: &str) -> Option<&mut PickList> {
        self.picklists.get_mut(name)
    }

    /// Mutable access to a named global table (for in-memory edits after
    /// validation).
    pub fn table_mut(&mut self, name: &str) -> Option<&mut GlobalTable> {
        self.global_tables.get_mut(name)
    }

    /// Set one cell of a global table in memory.
    ///
    /// This is the model-level editing API used by tools such as the Studio
    /// table editor; it does not validate or persist the model (call
    /// [`Model::validate`] afterwards).
    pub fn set_table_cell(
        &mut self,
        table: &str,
        row: usize,
        column: usize,
        value: TableCell,
    ) -> Result<(), ModelError> {
        let target = self.global_tables.get_mut(table).ok_or_else(|| {
            ModelError::Invalid(vec![ValidationIssue::new(
                format!("global_tables.{table}"),
                "unknown global table",
            )])
        })?;
        target.set_cell(row, column, value).map_err(|message| {
            ModelError::Invalid(vec![ValidationIssue::new(
                format!("global_tables.{table}.rows[{row}]"),
                message,
            )])
        })
    }

    /// Resolve a global table into an [`ArrivalSchedule`].
    ///
    /// Column convention: `time` (or `arrival_time`) is required and must be
    /// numeric; `quantity` (optional, default 1) and `item_type` (optional)
    /// are numeric; every other numeric column becomes an item label named
    /// after the column. This is the model-layer half of the table-driven
    /// arrival parameter, so the engine can resolve
    /// [`SourceParams::arrival_schedule_from_table`] once at load time.
    pub fn apply_table_reference(&self, table_name: &str) -> Result<ArrivalSchedule, String> {
        let table = self
            .table(table_name)
            .ok_or_else(|| format!("unknown global table '{table_name}'"))?;
        let time_column = find_table_column(table, &["time", "arrival_time"]).ok_or_else(|| {
            format!("global table '{table_name}' needs a 'time' or 'arrival_time' column")
        })?;
        let quantity_column = find_table_column(table, &["quantity"]);
        let item_type_column = find_table_column(table, &["item_type", "type"]);
        let mut rows = Vec::with_capacity(table.rows.len());
        for (position, cells) in table.rows.iter().enumerate() {
            let time = cells
                .get(time_column)
                .and_then(TableCell::as_number)
                .ok_or_else(|| {
                    format!("global table '{table_name}' row {position}: time must be numeric")
                })?;
            if !time.is_finite() || time < 0.0 {
                return Err(format!(
                    "global table '{table_name}' row {position}: time must be a non-negative finite number"
                ));
            }
            let quantity = match quantity_column {
                Some(index) => cells
                    .get(index)
                    .and_then(TableCell::as_number)
                    .ok_or_else(|| {
                        format!(
                            "global table '{table_name}' row {position}: quantity must be numeric"
                        )
                    })?
                    .round()
                    .max(0.0) as u32,
                None => 1,
            };
            let item_type = item_type_column
                .and_then(|index| cells.get(index))
                .and_then(TableCell::as_number)
                .map(|value| value.round().max(0.0) as u32);
            let mut labels = BTreeMap::new();
            for (column_index, column_name) in table.columns.iter().enumerate() {
                if column_index == time_column
                    || Some(column_index) == quantity_column
                    || Some(column_index) == item_type_column
                {
                    continue;
                }
                if let Some(value) = cells.get(column_index).and_then(TableCell::as_number) {
                    labels.insert(column_name.clone(), value);
                }
            }
            rows.push(ArrivalRow {
                time,
                quantity,
                item_type,
                labels,
            });
        }
        Ok(ArrivalSchedule {
            rows,
            repeat: false,
        })
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
    ///
    /// task-158: tracked-variable checks live here (the shared `validate.rs`
    /// belongs to another task): identifiers, uniqueness, expression syntax and
    /// a positive sampling interval.
    pub fn validate(&self) -> Result<(), ModelError> {
        let mut issues = crate::validate::validate_collect(self);
        issues.extend(self.tracked_variable_issues());
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ModelError::Invalid(issues))
        }
    }

    fn tracked_variable_issues(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        for (position, variable) in self.tracked_variables.iter().enumerate() {
            let at = format!("tracked_variables[{position}]");
            let valid = {
                let mut chars = variable.name.chars();
                match chars.next() {
                    Some(first) if first.is_alphabetic() || first == '_' => {
                        chars.all(|c| c.is_alphanumeric() || c == '_')
                    }
                    _ => false,
                }
            };
            if !valid {
                issues.push(ValidationIssue::new(
                    format!("{at}.name"),
                    "must be a valid identifier (letters, digits and _; not starting with a digit)",
                ));
            } else if !names.insert(variable.name.as_str()) {
                issues.push(ValidationIssue::new(
                    format!("{at}.name"),
                    "duplicate tracked variable name",
                ));
            }
            if variable.expression.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{at}.expression"),
                    "expression cannot be empty",
                ));
            } else if let Err(error) = Expr::parse(&variable.expression) {
                issues.push(ValidationIssue::new(
                    format!("{at}.expression"),
                    format!("expression does not compile: {error}"),
                ));
            }
        }
        if let Some(interval) = self.tracked_interval
            && (!interval.is_finite() || interval <= 0.0)
        {
            issues.push(ValidationIssue::new(
                "tracked_interval",
                "must be a positive number of time units",
            ));
        }
        issues
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

/// Turn a `serde_ignored` path such as `objects[0].params.interarrival.rate`
/// into a message that names the object and the offending key.
fn describe_ignored_path(path: &str, names: &[&str]) -> String {
    let mut result = path.to_string();
    for (index, name) in names.iter().enumerate() {
        let bracket = format!("objects[{index}]");
        if result.contains(&bracket) {
            result = result.replace(&bracket, &format!("objects.{name}"));
        }
        let dotted = format!("objects.{index}.");
        if result.contains(&dotted) {
            result = result.replace(&dotted, &format!("objects.{name}."));
        }
    }
    let key = path.rsplit('.').next().unwrap_or(path);
    format!("{result} (unknown key '{key}')")
}

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
            ObjectParams::Escalator(p) => Some(p),
            _ => None,
        }
    }

    pub fn transporter(&self) -> Option<&TransporterParams> {
        match &self.params {
            ObjectParams::Transporter(p) => Some(p),
            ObjectParams::Robot(p) => Some(p),
            ObjectParams::Crane(p) => Some(p),
            ObjectParams::AsrsVehicle(p) => Some(p),
            _ => None,
        }
    }

    /// Pull configuration of this object, whatever its kind (A3).
    pub fn pull(&self) -> Option<&PullParams> {
        self.params.pull()
    }

    /// Source-specific params, if this object is a source.
    pub fn source_params(&self) -> Option<&SourceParams> {
        self.source()
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
