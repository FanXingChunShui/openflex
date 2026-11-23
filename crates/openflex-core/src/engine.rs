//! Discrete-event simulation engine.
//!
//! The engine follows the classic three-phase structure used by FlexSim:
//!
//! 1. **Objects** hold flow items and expose a small state machine
//!    (idle / busy / blocked / down / off-shift).
//! 2. **Events** are scheduled on a calendar sorted by (time, sequence). Ties
//!    are broken in insertion order, which keeps runs deterministic for a given
//!    seed.
//! 3. **Routing** pushes a flow item from one object to the next according to
//!    the connection's port strategy. A rejected item stays with its current
//!    object and is retried when the downstream object becomes available.
//!
//! All times are in model time units; the engine never converts them.
//!
//! # Object semantics implemented here
//!
//! | Kind | Behaviour |
//! |------|-----------|
//! | `source` | inter-arrival mode: first item at `first_arrival` (0 = time zero), then one interval after the previous batch *left* the source; `arrival_schedule` / `arrival_sequence` modes (A2) create items from a table |
//! | `queue`/`rack` | immediate release, optional batching with `batch_size` and `batch_max_wait` |
//! | `processor` | setup + process, breakdown/repair with preemption, optional operator (dispatcher or resource pool) and `operator_preempt` task interruption (A10) |
//! | `multiprocessor` | `slots` parallel setup/process pipelines in one object (B1); one operator crew of `slots` per busy period |
//! | `conveyor` | item positions, accumulating or rigid transport, spacing and exit blocking; photo eyes fire `on_photo_eye`, stations dwell for their `process_time`, `join_inputs` merges upstream belts fairly (B3) |
//! | `combiner` | port 1 container + `components_per_container` components; `pack` releases the container, `join` merges one item per port, `batch` runs setup/process and releases every item (A7) |
//! | `separator` | `unpack` releases `split_count + 1` items, `split` copies the item `split_count` times (A6) |
//! | `merge` | one item per input port, optional `wait_for_all` gating |
//! | `split` | round-robin or by-item-type distribution over the output ports |
//! | `transporter`/`person`/`agv`/`dispatcher` | two-leg operator tasks (travel to the home pickup point → load → travel to the requester → unload), euclidean or road-network travel, capacity (B4); `person` is a walking executor and `agv` a transporter alias (B6) |
//! | `elevator` | vertical task executer between object heights (`z`) with a door dwell `stop_time` and capacity-bounded concurrent tasks (B6) |
//! | `rack` | queue storage (B2); `gravity`/`push_back` release LIFO, `drive_in` is FIFO; `channels`/`depth` narrow the usable capacity |
//! | `fluid_generator` / `fluid_tank` / `fluid_valve` / `fluid_sink` | fixed-step incompressible fluid network (B5): rates, tank level/overflow, valve ratio, link `flow_rate` caps |
//! | any | optional `shifts` plan: off-shift pauses work and accumulates `off_shift` time |
//!
//! # Simplifications (documented, not FlexSim behaviour)
//!
//! * Conveyors transfer an item to the downstream object at the exact instant
//!   it reaches the end of the belt; there is no separate transfer delay.
//! * Task executers travel in a straight line at constant speed unless a
//!   `network` distance table is supplied (then object-to-object legs use the
//!   shortest path); there is no acceleration or collision model.
//! * A conveyor station dwells only the item at the station; the belt itself
//!   keeps moving and items behind queue up by the spacing rule.
//! * The photo-eye trigger fires on the item at the exact crossing time, so a
//!   trigger that changes `item.type` affects the routing decision at the exit.
//! * A join conveyor merges its upstream belts with a fair round-robin entry
//!   gate plus the normal spacing rule; there is no slug building or gap
//!   control, and an upstream with nothing waiting is skipped.
//! * Events at or after `stop_time` are never processed, but they stay in the
//!   calendar; a checkpoint therefore keeps the future events a resumed run
//!   needs (see [`crate::checkpoint`]).
//! * A transporter's `capacity` is interpreted as the number of operator tasks
//!   it can serve concurrently (openflex v1 does not carry flow items). The
//!   elevator uses the same convention. Elevator dispatchers order pending
//!   calls by priority, then same floor, then the current travel direction,
//!   then the nearest floor; door dwell is the elevator's `stop_time`.
//! * The elevator family only moves vertically (`z`); there is no door state
//!   machine, overload detection, call/scheduling strategy or shaft collision.
//!   Persons/AGVs move on the plane with straight lines or the optional
//!   `network`, have no walking grid/A* or crowding, and AGV batteries are not
//!   modelled (documented alias of the transporter). Persons use the optional
//!   model `grid` with 4-neighbour A* (falling back to `network`/euclidean) and
//!   a simple congestion factor: each other busy person reduces speed by 10%
//!   (minimum half speed). AGVs with a
//!   `battery_capacity` spend `consumption_per_distance` per task distance and
//!   recharge for `charge_time` when empty; the `charger` object is only a
//!   validated reference and charging happens in place.
//! * Operators stay with a processor through a breakdown instead of being
//!   released; the processor simply pauses.
//! * A MultiProcessor requests its whole crew (`slots` operators) for a busy
//!   period and holds it until every slot is idle again; it cannot run with a
//!   partial crew, so a dispatcher/pool smaller than `slots` keeps it blocked.
//! * Operator preemption (A10) interrupts a task whose operator is already
//!   granted: the interrupted setup/process step is paused and its request is
//!   re-queued, then resumed when an operator returns. Tasks still travelling
//!   are not preempted, only single-capacity transporters are candidates, and
//!   a partial multi-operator crew is not re-balanced.
//! * Process Flow (B7) runs as a token graph next to the object model:
//!   `create` blocks produce tokens, `delay` holds them for a sampled time,
//!   `assign` applies label/type actions, `decision` branches on expressions
//!   and `sink` destroys them. The graph must be acyclic (validated) and
//!   blocks have no capacity limit. Per-block statistics are reported in
//!   `SimReport.process_flow` and token events are written to the trace log
//!   (`token_create`/`token_delay`/`token_assign`/`token_decision`/`token_sink`).
//!   Every discrete object can own a `params.process_flow`: its `create` blocks
//!   run on the model calendar or, with `on_entry`, once per received item, and
//!   `transfer` blocks inject tokens as flow items into another object
//!   (`to_object`) or pull the first buffered item out of a queue/rack and
//!   continue as a token (`from_object`).
//!   The object-free equivalent of the same graph is shown in
//!   `examples/objects/process-flow-equivalent.ofm`.
//! * An item leaving an object runs that object's `on_exit` trigger before the
//!   receiver's `on_entry`/`on_receive` logic (Processor included); the sink
//!   still runs its own `on_exit` when it destroys the item.
//! * `Action::Script` and `script_condition` run through the sandboxed rhai
//!   layer (D2): model scripts are precompiled once per source, action outcomes
//!   merge labels / item type / messages / release and `log` lines become
//!   `script_log` trace events. Script conditions are truthy when they set the
//!   numeric label `result` to a positive value, but a direct boolean return is
//!   preferred; errors map to `SimError::Script` with the object/trigger
//!   location. Scripts also query objects (`object_content`, `object_state`,
//!   `object_utilization`) and can `set_state`, `create_item` and `move_item`
//!   through their outcome.
//! * Fluid nodes enforce backpressure through per-step acceptance: a closed
//!   valve (latching below its `pressure_threshold`) accepts nothing, so tanks
//!   keep receiving and count overflow instead of dropping the volume; pipes
//!   buffer what their downstream cannot take. The report exposes a
//!   `pressure` proxy (average incoming flow rate per object).
//! * The fluid family (B5) is incompressible: there is no pressure, temperature,
//!   density or pipe delay. Integration uses a fixed step (`Model.fluid_step`,
//!   default 0.1) that only exists while fluid objects are present; only steps
//!   strictly before `stop_time` move volume, and the last partial step only
//!   updates the time-weighted level. A node splits its outflow equally over
//!   its outgoing links (each capped by the link's `flow_rate`), a tank releases
//!   at most `max_outflow` per step right after receiving, a closed valve
//!   passes nothing, and volumes that cannot be forwarded are dropped (only
//!   tank overflow is counted). A `fluid_pipe` adds a pure transport delay
//!   (`length/speed` or `delay`) with an optional flow cap and no backpressure,
//!   and a valve's `pressure_threshold` is approximated by the incoming flow
//!   rate (below it the valve stays closed and the volume is dropped).
//! * A fluid object never accepts flow items and cannot be connected to a
//!   discrete object; the two sub-systems simply coexist in one model.
//! * Repeating arrival schedules treat later passes' row times as offsets from
//!   the previous pass's last row (the first row's time only applies once).
//! * A released queue batch is committed at once: the items count as leaving
//!   the queue at the batch release instant, even though a downstream object
//!   may accept them one at a time afterwards.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, VecDeque};
use std::path::{Path, PathBuf};

use openflex_model::{
    Action, ArrivalStyle, CombinerMode, ConditionalPort, ConnectionKind, DispatcherParams,
    Distribution, DowntimeWindow, EvalContext, Expr, ExtraTaskLegKind, FlowBlockKind,
    GlobalVariableKind, Grid, MergeMode, Model, NetworkEdge, Object, ObjectKind, ObjectParams,
    PickListLookup, PortStrategy, ProcessFlow, PullParams, QueueOrder, RackPickPolicy, RackType,
    RecycleMode, SeparateMode, SimTrigger, TableCell, TaskSequenceMode, TaskStepKind, TrackedKind,
    TrafficMode, TransferMode, TravelTarget, TravelTargetName, Value,
};
use rand::prelude::*;
use rand_chacha::ChaCha12Rng;
use rand_distr::{Distribution as _, Exp, Gamma, LogNormal, Normal};
use serde::{Deserialize, Serialize};

use crate::checkpoint::{CHECKPOINT_FORMAT_VERSION, Checkpoint};
use crate::conveyor::Belt;
use crate::error::SimError;
use crate::script::{CompiledScript, ScriptContext, ScriptEngine, ScriptError, ScriptObjectInfo};
use crate::shifts;
use crate::stats::{
    ObjectStats, ReplicationFlow, ReplicationFlowBlock, ReplicationFluid, ReplicationFluidObject,
    ReplicationResult, SimConfig, SimReport, StateTimes, StaytimeSummary, TrackedSummary,
};
use crate::trace::{TraceEvent, TraceLog};

/// Reference fluid density/gravity for the simplified pressure model (B5).
const REFERENCE_DENSITY: f64 = 1000.0;
const REFERENCE_GRAVITY: f64 = 9.81;

/// Safety valve: a model that never drains its events is reported instead of
/// hanging forever.
const MAX_EVENTS: u64 = 50_000_000;

/// Numerical tolerance for time comparisons.
const EPS: f64 = 1e-9;

/// task-148/154: first fallback retry of a transport leg blocked by a full
/// `traffic_control` area. The primary path is a directed wake-up when a slot
/// is released; this timer only guards against a missed wake-up.
const TRAFFIC_RETRY_DELAY: f64 = 0.1;

/// Label used to carry the number of components inside a container item.
pub const COMPONENTS_LABEL: &str = "components";

/// Serde helper: `f64::INFINITY` is not representable in JSON, so infinite
/// timers are stored as `null` and restored as infinity.
mod infinite_time {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        let encoded = if value.is_finite() {
            Some(*value)
        } else {
            None
        };
        serde::Serialize::serialize(&encoded, serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        let value = Option::<f64>::deserialize(deserializer)?;
        Ok(value.unwrap_or(f64::INFINITY))
    }

    /// Default for timers that a checkpoint written before the field existed
    /// does not contain: "no timer armed".
    pub fn default() -> f64 {
        f64::INFINITY
    }
}

/// One flow item travelling through the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowItem {
    pub id: u64,
    pub item_type: u32,
    pub labels: BTreeMap<String, f64>,
    /// Model time the item was created.
    pub created: f64,
    /// Model time the item entered its current object.
    pub entry_time: f64,
    /// task-187: one-shot output-port override for the item's next routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<u32>,
}

impl FlowItem {
    /// An item that exists only so a trigger can evaluate `item.*` paths.
    fn probe(now: f64) -> Self {
        Self {
            id: 0,
            item_type: 0,
            labels: BTreeMap::new(),
            created: now,
            entry_time: now,
            destination: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Idle,
    Busy,
    Blocked,
    Down,
    OffShift,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Busy => "busy",
            State::Blocked => "blocked",
            State::Down => "down",
            State::OffShift => "off_shift",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum EventKind {
    /// A source creates its next batch.
    Arrive(usize),
    /// An object tries to push its pending items downstream.
    Flow(usize),
    SetupDone(usize, u64),
    ProcessDone(usize, u64),
    /// A failure clock reached its threshold; the generation guards against
    /// events armed for a state the object already left (A8).
    Breakdown(usize, u64),
    RepairDone(usize),
    /// A user message reaches an object.
    Message(usize),
    /// The next conveyor movement boundary is reached.
    BeltTick(usize),
    /// One slot of a multiprocessor finished its setup/process step (B1).
    MpDone(usize, usize, u64),
    /// A queue batch waited `batch_max_wait` and is released early.
    BatchTimeout(usize),
    /// A rack item finished its minimum dwell time and may be released.
    DwellDone(usize),
    /// An Emptier (`basic_fr` with `emptier`) sweeps its upstream objects.
    EmptierSweep(usize),
    /// task-44: the extra legs of a dispatcher task sequence finished.
    TransportSettle(usize),
    /// task-47: the picker reached the rack bay; the item may leave.
    RackPick(usize),
    /// task-65: a preempted request becomes eligible again after a delay.
    PreemptRetry(usize, usize),
    /// task-68: one extra leg of a dispatcher task sequence finished.
    TaskStepDone(usize),
    /// task-124: a person moved one grid cell.
    PersonStep(usize),
    /// task-133: a low-battery AGV reached its charger.
    ChargeTravel(usize),
    /// A shift window starts or ends.
    ShiftChange(usize),
    /// A linear availability calendar (B11) window starts or ends.
    TimetableChange(usize),
    /// A transporter reaches its pickup point (home) and loads there (B4).
    TransportPickup(usize, usize),
    /// A transporter reaches the processor that requested it.
    TransportArrive(usize, usize),
    /// A transporter finished loading an operator at the processor.
    OperatorLoaded(usize, usize),
    /// A transporter finished unloading an operator after processing.
    OperatorReleased(usize, usize),
    /// An idle transporter travels back to its home object.
    TransportHome(usize),
    /// A resource pool period ended; pending requests are retried.
    ResourceRetry(usize),
    /// A3: a downstream object asks its upstream objects for an item.
    Pull(usize),
    /// A process-flow create block produces its next token batch (B7).
    FlowCreate(usize),
    /// A process-flow block processes the tokens waiting in it (B7).
    FlowAdvance(usize),
    /// A token finished its delay block (B7).
    FlowDelayDone(usize, u64),
    /// Fixed-step fluid integration tick (B5).
    FluidStep,
    /// An AGV finished charging (B6 battery).
    ChargeDone(usize),
    /// An AGV resumes a task stage (0 pickup, 1 arrive, 2 home) after charging.
    TransportResume(usize, usize, u8),
    /// task-148: retry a transport leg that was blocked by a full
    /// `traffic_control` area (stage 0 = pickup leg, 1 = delivery leg,
    /// 2 = return leg).
    TrafficWait(usize, usize, u8),
    /// task-158: sample the model's tracked-variable expressions.
    TrackedSample,
    /// task-166: a task executer starts the loaded delivery leg.
    TransportDepart(usize, usize),
    /// task-173: one explicit-route travel leg reached its target.
    ExplicitLegDone(usize),
    /// task-173: retry an explicit-route leg blocked by traffic control.
    ExplicitTrafficRetry(usize),
    /// Warm-up boundary: reset the statistics accumulators.
    ResetStats,
}

/// Static description of one process-flow block (model or object level).
#[derive(Debug, Clone)]
struct FlowBlockMeta {
    id: String,
    kind: FlowBlockKind,
    /// Object that owns an object-level flow.
    owner: Option<usize>,
}

/// One token waiting inside a process-flow block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlowTokenRt {
    pub token: FlowItem,
    /// Set while the token is in a delay block; the token leaves when the
    /// matching `FlowDelayDone` event fires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_at: Option<f64>,
}

/// Runtime state and statistics of one process-flow block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlowBlockRt {
    pub tokens: Vec<FlowTokenRt>,
    pub advance_scheduled: bool,
    pub input: u64,
    pub output: u64,
    pub content_integral: f64,
    pub stay_sum: f64,
    pub stay_count: u64,
    pub last_content_change: f64,
    /// Batches created so far (create blocks only).
    pub creations: u32,
}

impl Default for FlowBlockRt {
    fn default() -> Self {
        Self {
            tokens: Vec::new(),
            advance_scheduled: false,
            input: 0,
            output: 0,
            content_integral: 0.0,
            stay_sum: 0.0,
            stay_count: 0,
            last_content_change: 0.0,
            creations: 0,
        }
    }
}

/// One resolved process-flow connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlowConnRt {
    pub to: usize,
    pub port: u32,
}

/// The process-flow runtime of a model (B7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlowRuntime {
    pub blocks: Vec<FlowBlockRt>,
    pub connections: Vec<FlowConnRt>,
    /// Connection indices leaving each block, in declaration order.
    pub outgoing: Vec<Vec<usize>>,
    pub next_token_id: u64,
    pub tokens_created: u64,
    pub tokens_completed: u64,
}

/// Runtime state and statistics of one fluid object (B5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FluidObjectRt {
    pub level: f64,
    pub level_integral: f64,
    pub level_max: f64,
    /// Cumulative volume moved out (or received, for sinks).
    pub throughput: f64,
    /// Cumulative overflow volume (tanks only).
    pub overflow: f64,
    /// Cumulative volume entering the object.
    pub inflow: f64,
    /// Pipe transport delay buffer: (release time, volume) entries.
    #[serde(default)]
    pub buffer: Vec<(f64, f64)>,
    /// Valve state (B5 backpressure): closed valves do not accept inflow.
    #[serde(default = "default_true_valve")]
    pub valve_open: bool,
    /// Batch step reached by a `fluid_mixer` (0-based, wraps around the table).
    #[serde(default)]
    pub mixer_step: usize,
    /// task-189: latched `on_empty`/`on_full` so a level resting on a boundary
    /// fires once per crossing, not once per tick.
    #[serde(default)]
    pub empty_latched: bool,
    #[serde(default)]
    pub full_latched: bool,
    /// task-189: cumulative overflow seen at the end of the previous step, used
    /// to detect "this tank just became full" (a full tank stops at
    /// `capacity - inflow` and spills the rest, so the level alone never
    /// reaches the capacity).
    #[serde(default)]
    pub last_overflow: f64,
    /// Current pressure (Pa) at this node.
    #[serde(default)]
    pub pressure: f64,
    /// Time integral of `pressure` for the reported average.
    #[serde(default)]
    pub pressure_integral: f64,
}

fn default_true_valve() -> bool {
    true
}

impl FluidObjectRt {
    fn new(level: f64) -> Self {
        Self {
            level,
            level_integral: 0.0,
            level_max: level,
            throughput: 0.0,
            overflow: 0.0,
            inflow: 0.0,
            buffer: Vec::new(),
            empty_latched: level <= EPS,
            full_latched: false,
            last_overflow: 0.0,
            valve_open: true,
            mixer_step: 0,
            pressure: 0.0,
            pressure_integral: 0.0,
        }
    }
}

/// One resolved fluid link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FluidLinkRt {
    pub to: usize,
    pub flow_rate: Option<f64>,
}

/// The fluid sub-system of a model (B5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FluidRuntime {
    /// Model object indices of the fluid objects.
    pub objects: Vec<usize>,
    pub state: Vec<FluidObjectRt>,
    pub links: Vec<FluidLinkRt>,
    /// Link indices leaving each fluid object (position order).
    pub outgoing: Vec<Vec<usize>>,
    /// Topological processing order (fluid positions).
    pub order: Vec<usize>,
    /// Pressure-driven pipes are enabled when any density/gravity, valve
    /// hysteresis or pipe temperature is configured (B5); otherwise the legacy
    /// flow-rate behaviour is kept.
    #[serde(default)]
    pub pressure_mode: bool,
    pub step: f64,
    pub last_time: f64,
    pub total_inflow: f64,
    pub total_throughput: f64,
    pub total_overflow: f64,
}

/// task-148: one `traffic_control` mutual-exclusion area.
#[derive(Debug, Clone)]
struct TrafficControlRt {
    capacity: u32,
    /// `mutual_exclusion` enforces the capacity; `un_timed` only applies the
    /// speed factor.
    mutual_exclusion: bool,
    speed_factor: f64,
    /// Network-node names connected to the control object (A connections).
    members: std::collections::HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Event {
    time: f64,
    seq: u64,
    kind: EventKind,
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.seq == other.seq
    }
}
impl Eq for Event {}
impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Event {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time
            .partial_cmp(&other.time)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| self.seq.cmp(&other.seq))
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Accum {
    input: u64,
    output: u64,
    content_integral: f64,
    stay_sum: f64,
    stay_count: u64,
    max_content: u32,
    states: StateTimes,
    /// Details of the state times: `setup` / `processing` subdivide `busy`,
    /// `waiting_operator` / `waiting_transport` subdivide `blocked`.
    detail: StateDetail,
}

/// Sub-state accumulators reported inside [`StateTimes`].
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
struct StateDetail {
    setup: f64,
    processing: f64,
    waiting_operator: f64,
    waiting_transport: f64,
    door_opening: f64,
    door_closing: f64,
    travel_empty: f64,
    travel_loaded: f64,
    loading: f64,
    unloading: f64,
    utilize: f64,
}

/// task-169: one dispatched task leg (old append kinds plus utilize/break).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExplicitLeg {
    pub(crate) kind: PendingLeg,
    pub(crate) target: Option<([f64; 2], f64)>,
    pub(crate) duration: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExplicitState {
    pub(crate) legs: VecDeque<ExplicitLeg>,
    pub(crate) requester: usize,
    pub(crate) loaded: bool,
    pub(crate) operator_loaded: bool,
    pub(crate) waiting_release: bool,
    pub(crate) release_seen: bool,
    pub(crate) leg_in_progress: bool,
    pub(crate) current: Option<PendingLeg>,
    /// Destination of the current travel leg (rebuilt into `explicit_target`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) current_target: Option<([f64; 2], f64)>,
    /// Absolute deadline of the current leg; not serialized, rebuilt from
    /// `remaining` on restore.
    #[serde(skip)]
    pub(crate) leg_deadline: Option<f64>,
    /// Remaining time of the current leg, the value checkpointed (task-183).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) remaining: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PendingLeg {
    Travel,
    Load,
    Unload,
    Utilize,
    Break,
}

/// task-166: sub-state of a task executer, subdividing the busy state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExecutorActivity {
    #[default]
    None,
    TravelEmpty,
    TravelLoaded,
    Loading,
    Unloading,
    Utilize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Idle,
    Setup,
    Processing,
    WaitingForOutput,
}

/// One parallel slot of a [`ObjectKind::MultiProcessor`] (B1).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Lane {
    item: Option<FlowItem>,
    phase: Phase,
    token: u64,
    /// Scheduled end of the current setup/process step.
    end: f64,
    /// Remaining time of a step paused by a breakdown or a shift.
    paused_remaining: f64,
    paused: bool,
    /// Model time at which this slot started its current busy interval
    /// (0 = slot not accumulating, e.g. while paused).
    busy_since: f64,
}

/// Why a blocked object is blocked. Only the two waits with a dedicated
/// statistic are named; blocking on a full downstream object is plain
/// `blocked`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum BlockReason {
    /// Waiting for a downstream object to free capacity (no detail statistic).
    #[default]
    Output,
    /// Waiting for an operator from a resource pool.
    Operator,
    /// Waiting for an operator that has to travel to the object.
    Transport,
}

/// A reason for an object to be down.
///
/// FlexSim lets several downtime sources overlap (a breakdown during planned
/// maintenance during a shift break). The engine keeps a stack in
/// [`ObjectRt::downtime`]: the object is `Down` while the stack is non-empty
/// and recovers only when the **last** request is cleared, which is what the
/// spec requires (`logic.md` §8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DowntimeSource {
    /// A random failure (MTBF/MTTR).
    Breakdown,
    /// Planned maintenance.
    ScheduledMaintenance,
    /// The shift plan put the object off shift while it was working.
    Shift,
    /// A model-driven (`send_message`/trigger or host API) downtime request.
    Manual,
}

/// Failure clock of one processor (A8).
///
/// With an empty `count_states` the MTBF clock runs on the wall clock, which is
/// the FlexSim default and the v0.1.0 behaviour. When `count_time_in_states` is
/// given only those states advance the clock, so a machine that fails after
/// 100 hours of *processing* keeps its clock paused while it waits for work.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FailureRt {
    /// State names that advance the clock; empty = wall clock.
    count_states: Vec<String>,
    /// Counted time accumulated since the last repair.
    up_time: f64,
    /// Threshold sampled for the next failure.
    threshold: f64,
    /// Bumped on every state change; stale `Breakdown` events are ignored.
    generation: u64,
}

/// A3: where a pulled item currently sits in the upstream object.
#[derive(Debug, Clone, Copy)]
enum UpstreamSlot {
    /// The upstream object's `out_pending` head (items it finished).
    OutPending,
    /// An index into the upstream object's `content`.
    Content(usize),
}

/// Which phase of a processor an operator is requested for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperatorPhase {
    Setup,
    Process,
}

/// Where a processor's operator came from; needed to release it correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum OperatorSource {
    Transporter(usize),
    Resource(usize),
}

/// One directed connection between two objects.
#[derive(Debug, Clone)]
struct SimConn {
    from: usize,
    to: usize,
    /// The ordinal of this connection among the target's incoming *flow*
    /// connections (0 for S connections, which have no ports).
    to_port: u32,
    /// The explicit `port` from the model, if any.
    declared_port: Option<u32>,
    strategy: Option<PortStrategy>,
    /// A (flow/port routing) or S (center-port resource link).
    kind: ConnectionKind,
}

/// One fixed point on a conveyor belt (B3): a photo eye or a station.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct BeltStop {
    position: f64,
    kind: BeltStopKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum BeltStopKind {
    PhotoEye(usize),
    Station(usize),
}

/// task-179: four-state Photo Eye machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum PhotoEyeState {
    #[default]
    Cleared,
    Blocking,
    Blocked,
    Clearing,
}

fn true_bool() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ObjectRt {
    name: String,
    kind: ObjectKind,
    content: Vec<FlowItem>,
    /// Items that finished processing etc. but could not be delivered yet.
    out_pending: Vec<FlowItem>,
    /// `out_pending` items already counted in the object's output statistics
    /// (used by queue batching, where a batch is released at once).
    out_pending_accounted: bool,
    state: State,
    phase: Phase,
    /// Model time the current phase began; feeds the setup/processing detail.
    phase_start: f64,
    /// Why the object is `Blocked`; only meaningful while `state == Blocked`.
    block_reason: BlockReason,
    last_state_change: f64,
    last_content_change: f64,
    acc: Accum,
    /// Items promised to (or in transport towards) this object but not yet
    /// inside it. Counted against capacity by [`Simulation::can_accept`].
    in_transit: u32,
    /// Failure clock (A8); `None` for objects without a `failure` parameter.
    failure: Option<FailureRt>,
    /// A4: the object holds finished items until an explicit `release_item`.
    hold_release: bool,
    /// A4: the next delivery ignores the `hold` strategy (set by
    /// `release_item`) and routes to the first available output instead.
    release_bypass: bool,
    /// A3: pull bookkeeping — round-robin cursor and a de-duplicating flag for
    /// the pending `Pull` event.
    pull_cursor: usize,
    pull_scheduled: bool,
    /// Model time the failure clock was last settled (A8).
    up_time_checkpoint: f64,
    /// Downtime requests currently active, oldest first (A9). The object is
    /// down while this is non-empty and recovers when it drains.
    downtime: Vec<DowntimeSource>,
    /// Round-robin cursor for routing and for splitters.
    rr: usize,
    split_cursor: usize,
    /// Source bookkeeping.
    arrivals: u32,
    next_arrival_pending: bool,
    /// A2: next table row for `arrival_schedule` / `arrival_sequence` sources.
    arrival_row: usize,
    /// A2: base model time of the current repeating schedule pass.
    arrival_base: f64,
    /// A2: schedule pass index; pass 0 uses the table's absolute row times.
    arrival_pass: u32,
    /// Processor bookkeeping.
    processing_item: Option<FlowItem>,
    phase_token: u64,
    #[serde(with = "infinite_time")]
    phase_end: f64,
    paused_remaining: f64,
    ready_for_process: bool,
    /// Operator bookkeeping (A10): how many operators are held, how many were
    /// requested but not granted yet, and where each held one came from.
    operator_held: u32,
    operators_pending: u32,
    requesting_operator: bool,
    /// A10: the current phase was paused because its operator was preempted.
    operator_preempted: bool,
    operator_sources: Vec<OperatorSource>,
    /// Conveyor runtime.
    belt: Option<Belt>,
    /// task-176: build slugs (tight groups move as one).
    #[serde(default)]
    belt_slug: bool,
    #[serde(default)]
    belt_slug_min_gap: f64,
    belt_items: Vec<FlowItem>,
    belt_last_update: f64,
    #[serde(with = "infinite_time")]
    belt_tick_at: f64,
    /// Earliest instant a rack item finishes its minimum dwell time (B2).
    #[serde(default = "infinite_time::default", with = "infinite_time")]
    dwell_at: f64,
    /// Level of the last item a rack released (task-44 `nearest` policy).
    #[serde(default)]
    last_picked_level: Option<u32>,
    /// task-47: an in-progress rack pick (content position, ready time).
    #[serde(default)]
    rack_pick: Option<(usize, f64)>,
    /// task-65: how often this object's request was preempted.
    #[serde(default)]
    preempt_retries: u32,
    /// task-136: distance driven to the charger (deducted on arrival).
    #[serde(default)]
    charge_trip_distance: f64,
    /// task-130: accumulated charging time and the start of the current charge.
    #[serde(default)]
    charge_time_total: f64,
    #[serde(default)]
    charge_started_at: f64,
    /// task-188: items counted towards the next sink recycle batch.
    #[serde(default)]
    recycled_count: u32,
    /// task-190: tasks handed out by this dispatcher that are still pending
    /// (`max_tasks` cap).
    #[serde(default)]
    dispatcher_active: u32,
    /// task-190: type of the last item this processor started (changeover).
    #[serde(default)]
    last_item_type: Option<u32>,
    /// task-124: remaining cells of a step-by-step person move.
    #[serde(default)]
    person_path: VecDeque<[f64; 2]>,
    /// task-124: requester of the step-by-step move in progress.
    #[serde(default)]
    person_target: Option<usize>,
    /// task-68: durations of the extra task-sequence legs still to run.
    #[serde(default)]
    pending_steps: Vec<(PendingLeg, f64)>,
    /// task-65: model time of the last preemption (a request is not preempted
    /// twice at the same instant, which would otherwise loop forever).
    #[serde(default = "negative_one")]
    preempt_epoch: f64,
    /// task-47: how many items this rack has stored so far, used to rotate the
    /// (level, aisle, bay) addresses.
    #[serde(default)]
    rack_slots: u32,
    /// B3: photo eyes and stations, sorted by position.
    belt_stops: Vec<BeltStop>,
    /// B3: per-item flags of the stops already handled, aligned with
    /// `belt_items` (front first).
    belt_handled: Vec<Vec<bool>>,
    /// task-179: current state of each Photo Eye on this belt.
    #[serde(default)]
    photo_eye_states: Vec<PhotoEyeState>,
    /// task-179: sensing half-width for each Photo Eye.
    #[serde(default)]
    photo_eye_ranges: Vec<f64>,
    /// task-184: motor power flag (true for non-motors).
    #[serde(default = "true_bool")]
    motor_running: bool,
    /// task-181: sensing half-width and per-item once flags for decision points.
    #[serde(default)]
    decision_point_ranges: Vec<f64>,
    #[serde(default)]
    decision_handled: Vec<Vec<bool>>,
    /// B3: number of upstream belts merged onto this belt (0/1 = disabled).
    join_inputs: u32,
    /// task-178: merge-controller mode and its state.
    #[serde(default)]
    merge_mode: MergeMode,
    #[serde(default)]
    merge_weight: Vec<f64>,
    #[serde(default)]
    merge_counts: Vec<f64>,
    /// B3: fair round-robin cursor over the input ports of a join conveyor.
    join_cursor: usize,
    /// Queue batching.
    batch_anchor: Option<f64>,
    force_release: bool,
    /// Combiner runtime.
    combine_mode: CombinerMode,
    container: Option<FlowItem>,
    components: Vec<FlowItem>,
    join_slots: Vec<Option<FlowItem>>,
    /// MultiProcessor slots (B1); empty for every other kind.
    lanes: Vec<Lane>,
    /// Total slot-seconds worked by a MultiProcessor (feeds its utilisation).
    slot_busy: f64,
    /// Separator runtime.
    pending_parts: Vec<(FlowItem, u32)>,
    /// Merge runtime.
    merge_slots: Vec<Option<FlowItem>>,
    /// Task executer runtime.
    position: [f64; 2],
    /// Height (floor) used by the elevator family (B6).
    z: f64,
    /// Last elevator travel direction (-1 / 0 / +1), used for call ordering.
    #[serde(default)]
    elevator_direction: i8,
    /// AGV battery remaining distance (B6 battery), `None` = unlimited.
    #[serde(default)]
    battery: Option<f64>,
    #[serde(default)]
    charging: bool,
    #[serde(default)]
    charge_until: f64,
    /// task-149: model time at which this AGV started waiting at a charger.
    #[serde(default)]
    charge_queue_since: Option<f64>,
    /// task-150: a step-by-step person is walking back home; settle the task
    /// when the path runs out.
    #[serde(default)]
    person_returning: bool,
    /// task-151: consecutive node-wait retries of this AGV (deadlock breaker).
    #[serde(default)]
    agv_wait_rounds: u32,
    /// task-152: cells this person has reserved ahead on its path
    /// (`grid_reserve` cells). Other people treat them as occupied.
    #[serde(default)]
    person_reserved: Vec<[f64; 2]>,
    /// Distance accumulated by the current task (battery drain).
    #[serde(default)]
    task_distance: f64,
    /// task-148: traffic-control areas this executer is admitted to during the
    /// current leg. Serialized so a checkpoint restores admissions exactly.
    #[serde(default)]
    traffic_hold: Vec<usize>,
    /// task-154: stage of the transport leg waiting for a traffic slot.
    #[serde(default)]
    traffic_wait_stage: Option<u8>,
    /// task-154: the control areas that leg needs (one queue entry each).
    #[serde(default)]
    traffic_wait_controls: Vec<usize>,
    /// task-166: current executer sub-state and its start time.
    #[serde(default)]
    executor_activity: ExecutorActivity,
    #[serde(default)]
    executor_since: f64,
    capacity: u32,
    tasks: usize,
    /// Requester currently served by this transporter (A10 preemption).
    serving_requester: Option<usize>,
    home: Option<usize>,
    assigned_dispatcher: Option<usize>,
    /// Dispatcher / resource request queue.
    requests: VecDeque<usize>,
    resource_in_use: u32,
    retry_pending: bool,
    /// Availability.
    on_shift: bool,
}

impl ObjectRt {
    fn new(object: &Object, now: f64) -> Self {
        let mut runtime = Self {
            name: object.name.clone(),
            kind: object.kind,
            content: Vec::new(),
            out_pending: Vec::new(),
            out_pending_accounted: false,
            state: State::Idle,
            phase: Phase::Idle,
            phase_start: now,
            block_reason: BlockReason::Output,
            last_state_change: now,
            last_content_change: now,
            acc: Accum::default(),
            in_transit: 0,
            failure: None,
            hold_release: false,
            release_bypass: false,
            pull_cursor: 0,
            pull_scheduled: false,
            up_time_checkpoint: now,
            downtime: Vec::new(),
            rr: 0,
            split_cursor: 0,
            arrivals: 0,
            next_arrival_pending: false,
            arrival_row: 0,
            arrival_base: 0.0,
            arrival_pass: 0,
            processing_item: None,
            phase_token: 0,
            phase_end: f64::INFINITY,
            paused_remaining: 0.0,
            ready_for_process: false,
            operator_held: 0,
            operators_pending: 0,
            requesting_operator: false,
            operator_preempted: false,
            operator_sources: Vec::new(),
            belt: None,
            belt_slug: false,
            belt_slug_min_gap: 0.0,
            belt_items: Vec::new(),
            belt_last_update: now,
            belt_tick_at: f64::INFINITY,
            dwell_at: f64::INFINITY,
            last_picked_level: None,
            rack_pick: None,
            preempt_retries: 0,
            charge_trip_distance: 0.0,
            charge_time_total: 0.0,
            charge_started_at: 0.0,
            recycled_count: 0,
            dispatcher_active: 0,
            last_item_type: None,
            person_path: VecDeque::new(),
            person_target: None,
            pending_steps: Vec::new(),
            preempt_epoch: -1.0,
            rack_slots: 0,
            belt_stops: Vec::new(),
            belt_handled: Vec::new(),
            photo_eye_states: Vec::new(),
            photo_eye_ranges: Vec::new(),
            motor_running: true,
            decision_point_ranges: Vec::new(),
            decision_handled: Vec::new(),
            join_inputs: 0,
            merge_mode: MergeMode::RoundRobin,
            merge_weight: Vec::new(),
            merge_counts: Vec::new(),
            join_cursor: 0,
            batch_anchor: None,
            force_release: false,
            combine_mode: CombinerMode::Pack,
            container: None,
            components: Vec::new(),
            join_slots: Vec::new(),
            lanes: Vec::new(),
            slot_busy: 0.0,
            pending_parts: Vec::new(),
            merge_slots: Vec::new(),
            position: object.pos,
            z: object.z,
            elevator_direction: 0,
            battery: None,
            charging: false,
            charge_until: 0.0,
            charge_queue_since: None,
            person_returning: false,
            agv_wait_rounds: 0,
            person_reserved: Vec::new(),
            task_distance: 0.0,
            traffic_hold: Vec::new(),
            traffic_wait_stage: None,
            traffic_wait_controls: Vec::new(),
            executor_activity: ExecutorActivity::None,
            executor_since: now,
            capacity: 1,
            tasks: 0,
            serving_requester: None,
            home: None,
            assigned_dispatcher: None,
            requests: VecDeque::new(),
            resource_in_use: 0,
            retry_pending: false,
            on_shift: true,
        };
        match &object.params {
            ObjectParams::Conveyor(params) | ObjectParams::Escalator(params) => {
                // B3 curve: the straight length plus the arc length of an
                // optional curved section (same speed).
                let curve = params
                    .curve
                    .map_or(0.0, |curve| curve.radius * curve.angle.to_radians());
                // task-53: a segmented belt replaces length/speed. The travel
                // time is the sum of `length / speed` over the segments, which
                // the single-speed belt model represents with the equivalent
                // speed `total_length / total_time`.
                // task-56: a segmented belt keeps its speed profile; the item
                // position integrates each band's speed instead of using a
                // single equivalent speed.
                let (belt_length, belt_speed, segment_bounds, segment_speeds) =
                    if params.segments.is_empty() {
                        (
                            params.length + curve.max(0.0),
                            params.speed,
                            Vec::new(),
                            Vec::new(),
                        )
                    } else {
                        let mut bounds = Vec::new();
                        let mut speeds = Vec::new();
                        let mut position = 0.0;
                        for segment in &params.segments {
                            let arc = match params.curve.as_ref() {
                                Some(curve) if segment.angle.abs() > 0.0 => {
                                    curve.radius * segment.angle.to_radians().abs()
                                }
                                _ => 0.0,
                            };
                            position += segment.length.max(0.0) + arc;
                            bounds.push(position);
                            speeds.push(segment.speed.max(EPS));
                        }
                        let total = bounds.pop().unwrap_or(0.0);
                        let first = speeds.first().copied().unwrap_or(params.speed);
                        (total, first, bounds, speeds)
                    };
                let mut belt = Belt::new(
                    belt_length,
                    belt_speed,
                    params.item_spacing,
                    params.accumulating,
                );
                if !segment_speeds.is_empty() {
                    belt.set_segments(segment_bounds, segment_speeds);
                }
                runtime.belt_slug = params.slug;
                runtime.belt_slug_min_gap =
                    params.slug_min_gap.unwrap_or(params.item_spacing).max(0.0);
                runtime.belt = Some(belt);
                let mut stops: Vec<BeltStop> = Vec::new();
                for (eye_index, photo_eye) in params.photo_eyes.iter().enumerate() {
                    stops.push(BeltStop {
                        position: photo_eye.position,
                        kind: BeltStopKind::PhotoEye(eye_index),
                    });
                }
                runtime.photo_eye_states = vec![PhotoEyeState::Cleared; params.photo_eyes.len()];
                runtime.photo_eye_ranges = params
                    .photo_eyes
                    .iter()
                    .map(|eye| eye.range.unwrap_or(0.5).max(EPS))
                    .collect();
                runtime.decision_point_ranges = params
                    .decision_points
                    .iter()
                    .map(|point| point.range.max(EPS))
                    .collect();
                for (station, conveyor_station) in params.stations.iter().enumerate() {
                    stops.push(BeltStop {
                        position: conveyor_station.position,
                        kind: BeltStopKind::Station(station),
                    });
                }
                stops.sort_by(|a, b| {
                    a.position
                        .partial_cmp(&b.position)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                runtime.belt_stops = stops;
                runtime.join_inputs = params.join_inputs.unwrap_or(0);
                runtime.merge_mode = params.merge_mode;
                runtime.merge_weight = params.merge_weight.clone().unwrap_or_default();
                runtime.merge_counts = vec![0.0; runtime.join_inputs as usize];
            }
            ObjectParams::Motor(params) => runtime.motor_running = params.running,
            ObjectParams::Transporter(params) => runtime.capacity = params.capacity.max(1),
            ObjectParams::Person(_) => runtime.capacity = 1,
            ObjectParams::Agv(params) => {
                runtime.capacity = params.capacity.max(1);
                runtime.battery = params.battery_capacity;
            }
            ObjectParams::Elevator(params) => runtime.capacity = params.capacity.max(1),
            ObjectParams::Combiner(params) => runtime.combine_mode = params.mode,
            ObjectParams::MultiProcessor(params) => {
                runtime.lanes = (0..params.slots.max(1)).map(|_| Lane::default()).collect();
            }
            _ => {}
        }
        runtime
    }

    /// Size the per-port staging buffers once the connections are known.
    fn configure_ports(&mut self, input_count: usize) {
        match self.kind {
            ObjectKind::Merge => self.merge_slots = vec![None; input_count.max(1)],
            ObjectKind::Combiner if self.combine_mode == CombinerMode::Join => {
                self.join_slots = vec![None; input_count.max(1)];
            }
            _ => {}
        }
    }

    /// Items the object currently holds. Container items count once, their
    /// components are inside the container and are not counted separately.
    fn held(&self) -> usize {
        match self.kind {
            ObjectKind::Combiner => match self.combine_mode {
                CombinerMode::Pack => usize::from(self.container.is_some()),
                CombinerMode::Batch => {
                    usize::from(self.container.is_some()) + self.components.len()
                }
                CombinerMode::Join => self.join_slots.iter().flatten().count(),
            },
            ObjectKind::Separator => {
                self.content.len() + usize::from(!self.pending_parts.is_empty())
            }
            ObjectKind::Conveyor | ObjectKind::Escalator => self.belt_items.len(),
            ObjectKind::Merge => self.merge_slots.iter().flatten().count(),
            ObjectKind::MultiProcessor => {
                self.content.len() + self.lanes.iter().filter(|lane| lane.item.is_some()).count()
            }
            _ => self.content.len() + usize::from(self.processing_item.is_some()),
        }
    }

    /// `held()` plus the items that are still on their way here (A5). Capacity
    /// checks use this so a model cannot over-commit an object by having many
    /// transports in flight.
    fn effective_content(&self) -> usize {
        self.held() + self.in_transit as usize
    }

    /// Integrate the content integral up to `now`. Must be called *before*
    /// changing what the object holds, so the previous interval is charged with
    /// the previous content.
    fn note_content(&mut self, now: f64) {
        let dt = (now - self.last_content_change).max(0.0);
        self.acc.content_integral += self.held() as f64 * dt;
        self.last_content_change = now;
        self.acc.max_content = self.acc.max_content.max(self.held() as u32);
    }

    /// Record the peak content after a change.
    fn touch(&mut self) {
        self.acc.max_content = self.acc.max_content.max(self.held() as u32);
    }

    fn push_content(&mut self, now: f64, item: FlowItem) {
        self.note_content(now);
        self.content.push(item);
        self.touch();
    }

    fn take_content(&mut self, now: f64, position: usize) -> Option<FlowItem> {
        if position >= self.content.len() {
            return None;
        }
        self.note_content(now);
        let item = self.content.remove(position);
        self.touch();
        Some(item)
    }

    fn set_state(&mut self, now: f64, state: State) {
        if self.state == state {
            return;
        }
        self.accumulate_state(now);
        self.accrue_up_time(now);
        // Settle the phase detail with the *old* state: leaving `busy` must stop
        // the setup/processing clock, entering it restarts it from now.
        self.charge_phase(now);
        self.state = state;
        self.last_state_change = now;
        if state != State::Blocked {
            self.block_reason = BlockReason::Output;
        }
    }

    /// task-166: charge the open executer sub-state and switch it.
    fn set_executor_activity(&mut self, now: f64, activity: ExecutorActivity) {
        self.charge_executor_activity(now);
        self.executor_activity = activity;
        self.executor_since = now;
    }

    fn charge_executor_activity(&mut self, now: f64) {
        let dt = (now - self.executor_since).max(0.0);
        if dt <= 0.0 {
            return;
        }
        match self.executor_activity {
            ExecutorActivity::None => {}
            ExecutorActivity::TravelEmpty => self.acc.detail.travel_empty += dt,
            ExecutorActivity::TravelLoaded => self.acc.detail.travel_loaded += dt,
            ExecutorActivity::Loading => self.acc.detail.loading += dt,
            ExecutorActivity::Unloading => self.acc.detail.unloading += dt,
            ExecutorActivity::Utilize => self.acc.detail.utilize += dt,
        }
        self.executor_since = now;
    }

    /// Change the processor phase, settling the detail timer at the boundary.
    fn set_phase(&mut self, now: f64, phase: Phase) {
        if self.phase == phase {
            return;
        }
        self.accrue_up_time(now);
        self.charge_phase(now);
        self.phase = phase;
    }

    /// Charge the time since the last phase boundary to the current phase.
    ///
    /// Only busy time counts: a processor that is down or off shift keeps its
    /// phase (so it can resume) but must not accrue setup/processing time.
    fn charge_phase(&mut self, now: f64) {
        let dt = (now - self.phase_start).max(0.0);
        self.phase_start = now;
        if dt <= 0.0 || self.state != State::Busy {
            return;
        }
        match self.phase {
            Phase::Setup => self.acc.detail.setup += dt,
            Phase::Processing => self.acc.detail.processing += dt,
            _ => {}
        }
    }

    /// Enter the blocked state with a reason that feeds the detail statistics.
    /// The reason is set before the state change so the interval is charged to
    /// the right bucket.
    fn block(&mut self, now: f64, reason: BlockReason) {
        if self.state != State::Blocked {
            self.block_reason = reason;
        }
        self.set_state(now, State::Blocked);
    }

    /// Charge the interval that just ended to the failure clock (A8) and
    /// invalidate the events armed for the old state.
    fn accrue_up_time(&mut self, now: f64) {
        let dt = (now - self.up_time_checkpoint).max(0.0);
        self.up_time_checkpoint = now;
        let counted = self.failure.as_ref().is_some_and(|failure| {
            Self::counts_up_time(failure, self.state, self.phase, self.block_reason)
        });
        if let Some(failure) = self.failure.as_mut() {
            if counted {
                failure.up_time += dt;
            }
            if !failure.count_states.is_empty() {
                failure.generation = failure.generation.wrapping_add(1);
            }
        }
    }

    /// Whether `state` (with `phase`) advances the failure clock.
    fn counts_up_time(
        failure: &FailureRt,
        state: State,
        phase: Phase,
        block_reason: BlockReason,
    ) -> bool {
        if failure.count_states.is_empty() {
            return false;
        }
        failure.count_states.iter().any(|name| match name.as_str() {
            "idle" => state == State::Idle,
            "busy" => state == State::Busy,
            "setup" => state == State::Busy && phase == Phase::Setup,
            "processing" => state == State::Busy && phase == Phase::Processing,
            "blocked" => state == State::Blocked,
            "waiting_for_operator" => {
                state == State::Blocked && block_reason == BlockReason::Operator
            }
            "waiting_for_transport" => {
                state == State::Blocked && block_reason == BlockReason::Transport
            }
            "down" => state == State::Down,
            "off_shift" => state == State::OffShift,
            _ => false,
        })
    }

    fn accumulate_state(&mut self, now: f64) {
        let dt = (now - self.last_state_change).max(0.0);
        match self.state {
            State::Idle => self.acc.states.idle += dt,
            State::Busy => self.acc.states.busy += dt,
            State::Blocked => {
                self.acc.states.blocked += dt;
                match self.block_reason {
                    BlockReason::Operator => self.acc.detail.waiting_operator += dt,
                    BlockReason::Transport => self.acc.detail.waiting_transport += dt,
                    BlockReason::Output => {}
                }
            }
            State::Down => self.acc.states.down += dt,
            State::OffShift => self.acc.states.off_shift += dt,
        }
        self.last_state_change = now;
    }

    fn finalize(&mut self, now: f64) {
        self.note_content(now);
        self.accumulate_state(now);
        self.charge_phase(now);
        self.charge_executor_activity(now);
        for lane in &mut self.lanes {
            if lane.busy_since > 0.0 || lane.item.is_some() {
                if lane.item.is_some() && lane.busy_since > 0.0 {
                    self.slot_busy += (now - lane.busy_since).max(0.0);
                }
                lane.busy_since = 0.0;
            }
        }
    }

    /// Statistics at `now` **without** touching the accumulators (task-50).
    ///
    /// `stats()` assumes `finalize()` already ran; this variant adds the still
    /// open interval on a copy, so callers can read statistics as often as they
    /// like (the studio does it on every frame) without changing the numbers.
    fn stats_at(&self, now: f64, elapsed: f64) -> ObjectStats {
        let denom = elapsed.max(1e-12);
        let dt = (now - self.last_state_change).max(0.0);
        let mut states = self.acc.states;
        let mut detail = self.acc.detail;
        match self.state {
            State::Idle => states.idle += dt,
            State::Busy => {
                states.busy += dt;
                // The phase detail runs on its own clock (`phase_start`).
                let phase_dt = (now - self.phase_start).max(0.0);
                match self.phase {
                    Phase::Setup => detail.setup += phase_dt,
                    Phase::Processing => detail.processing += phase_dt,
                    _ => {}
                }
            }
            State::Blocked => {
                states.blocked += dt;
                match self.block_reason {
                    BlockReason::Operator => detail.waiting_operator += dt,
                    BlockReason::Transport => detail.waiting_transport += dt,
                    BlockReason::Output => {}
                }
            }
            State::Down => states.down += dt,
            State::OffShift => states.off_shift += dt,
        }
        if self.state == State::Busy {
            let activity_dt = (now - self.executor_since).max(0.0);
            match self.executor_activity {
                ExecutorActivity::None => {}
                ExecutorActivity::TravelEmpty => detail.travel_empty += activity_dt,
                ExecutorActivity::TravelLoaded => detail.travel_loaded += activity_dt,
                ExecutorActivity::Loading => detail.loading += activity_dt,
                ExecutorActivity::Unloading => detail.unloading += activity_dt,
                ExecutorActivity::Utilize => detail.utilize += activity_dt,
            }
        }
        states.setup = detail.setup;
        states.processing = detail.processing;
        states.waiting_for_operator = detail.waiting_operator;
        states.waiting_for_transport = detail.waiting_transport;
        states.door_opening = detail.door_opening;
        states.door_closing = detail.door_closing;
        states.travel_empty = detail.travel_empty;
        states.travel_loaded = detail.travel_loaded;
        states.loading = detail.loading;
        states.unloading = detail.unloading;
        states.utilize = detail.utilize;
        let content_dt = (now - self.last_content_change).max(0.0);
        let content_integral = self.acc.content_integral + self.held() as f64 * content_dt;
        ObjectStats {
            kind: self.kind,
            input_count: self.acc.input,
            output_count: self.acc.output,
            avg_content: content_integral / denom,
            max_content: self.acc.max_content.max(self.held() as u32),
            avg_stay_time: if self.acc.stay_count > 0 {
                self.acc.stay_sum / self.acc.stay_count as f64
            } else {
                0.0
            },
            state_times: states,
            utilization: states.busy / denom,
            throughput: self.acc.output as f64 / denom,
            battery_remaining: self.battery,
            charging_fraction: self.charge_time_total / denom,
            content: self.content.len() as u32,
            state: Some(self.state),
            phase: Some(self.phase),
            position: Some(self.position),
            z: self.z,
        }
    }

    fn stats(&self, elapsed: f64) -> ObjectStats {
        let denom = elapsed.max(1e-12);
        let mut state_times = self.acc.states;
        state_times.setup = self.acc.detail.setup;
        state_times.processing = self.acc.detail.processing;
        state_times.waiting_for_operator = self.acc.detail.waiting_operator;
        state_times.waiting_for_transport = self.acc.detail.waiting_transport;
        state_times.door_opening = self.acc.detail.door_opening;
        state_times.door_closing = self.acc.detail.door_closing;
        state_times.travel_empty = self.acc.detail.travel_empty;
        state_times.travel_loaded = self.acc.detail.travel_loaded;
        state_times.loading = self.acc.detail.loading;
        state_times.unloading = self.acc.detail.unloading;
        state_times.utilize = self.acc.detail.utilize;
        ObjectStats {
            kind: self.kind,
            input_count: self.acc.input,
            output_count: self.acc.output,
            avg_content: self.acc.content_integral / denom,
            max_content: self.acc.max_content,
            avg_stay_time: if self.acc.stay_count > 0 {
                self.acc.stay_sum / self.acc.stay_count as f64
            } else {
                0.0
            },
            state_times,
            // MultiProcessor slots work in parallel: report the mean slot
            // utilisation instead of the aggregate busy fraction (B1).
            utilization: if self.kind == ObjectKind::MultiProcessor {
                self.slot_busy / (denom * self.lanes.len().max(1) as f64)
            } else {
                self.acc.states.busy / denom
            },
            throughput: self.acc.output as f64 / denom,
            battery_remaining: self.battery,
            charging_fraction: self.charge_time_total / denom,
            content: self.content.len() as u32,
            state: Some(self.state),
            phase: Some(self.phase),
            position: Some(self.position),
            z: self.z,
        }
    }
}

/// Run every replication described by the model and aggregate the results.
pub fn run_model(model: &Model) -> Result<SimReport, SimError> {
    let replications = model.run.replications.max(1);
    let mut results = Vec::with_capacity(replications as usize);
    for index in 0..replications {
        let seed = derive_seed(model.run.seed, index);
        let config = SimConfig::from_run_settings(&model.run, seed);
        results.push(simulate(model, &config, index)?);
    }
    Ok(SimReport::from_replications(model, results))
}

/// task-59: run every replication on `workers` threads.
///
/// The replications are independent (each has its own seed and engine), so the
/// results are identical to [`run_model`]; they are re-assembled in replication
/// order, which keeps the report deterministic.
pub fn run_model_parallel(model: &Model, workers: usize) -> Result<SimReport, SimError> {
    let replications = model.run.replications.max(1);
    // task-62: `workers == 0` means "use the available parallelism".
    let requested = if workers == 0 {
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
    } else {
        workers
    };
    let worker_count = requested.max(1).min(replications as usize);
    if worker_count <= 1 {
        return run_model(model);
    }
    let mut collected: Vec<Option<ReplicationResult>> = vec![None; replications as usize];
    let mut failure: Option<SimError> = None;
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for worker in 0..worker_count {
            handles.push(scope.spawn(move || {
                let mut local: Vec<(u32, ReplicationResult)> = Vec::new();
                let mut errors: Vec<SimError> = Vec::new();
                let mut index = worker as u32;
                while index < replications {
                    let seed = derive_seed(model.run.seed, index);
                    let config = SimConfig::from_run_settings(&model.run, seed);
                    match simulate(model, &config, index) {
                        Ok(result) => local.push((index, result)),
                        Err(error) => errors.push(error),
                    }
                    index += worker_count as u32;
                }
                (local, errors)
            }));
        }
        for handle in handles {
            let (local, errors) = handle.join().expect("a replication worker panicked");
            for (index, result) in local {
                collected[index as usize] = Some(result);
            }
            if failure.is_none()
                && let Some(error) = errors.into_iter().next()
            {
                failure = Some(error);
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    let results: Vec<ReplicationResult> = collected.into_iter().flatten().collect();
    Ok(SimReport::from_replications(model, results))
}

/// Run one replication.
pub fn simulate(
    model: &Model,
    config: &SimConfig,
    index: u32,
) -> Result<ReplicationResult, SimError> {
    simulate_run(model, config, index, false, None, false).map(|(result, _, _)| result)
}

/// Run one replication and record every processed event (B12).
///
/// A tracing run is otherwise identical to [`simulate`]; the returned
/// [`TraceLog`] can be streamed as JSONL or inspected in tests. The log is
/// capped at [`crate::trace::MAX_TRACE_EVENTS`] events.
pub fn simulate_traced(
    model: &Model,
    config: &SimConfig,
    index: u32,
) -> Result<(ReplicationResult, TraceLog), SimError> {
    let (result, _, trace) = simulate_run(model, config, index, true, None, false)?;
    Ok((result, trace))
}

/// Run one replication, optionally resuming from a checkpoint (B12).
///
/// * `resume = None` starts at t = 0.
/// * `resume = Some(checkpoint)` continues from the checkpoint's model time;
///   the checkpoint must come from the same model, seed and warm-up, and
///   `config.stop_time` must be at least the checkpoint time.
///
/// The returned [`crate::checkpoint::Checkpoint`] captures the state at
/// `config.stop_time`, so `stop 1000` + resume to `stop 2000` produces exactly
/// the same [`ReplicationResult`] as a single run to `stop 2000`.
pub fn simulate_with_checkpoint(
    model: &Model,
    config: &SimConfig,
    index: u32,
    resume: Option<&Checkpoint>,
) -> Result<(ReplicationResult, Checkpoint), SimError> {
    let (result, checkpoint, _) = simulate_run(model, config, index, false, resume, true)?;
    let checkpoint =
        checkpoint.ok_or_else(|| SimError::Config("the checkpoint was not captured".into()))?;
    Ok((result, checkpoint))
}

/// task-44: run until `stop_at`, fire `on_stop` on every object, and return a
/// checkpoint that [`resume_from_checkpoint`] can continue from.
///
/// Unlike [`simulate_with_checkpoint`] this entry point **does** run the
/// `on_stop` triggers; the regular checkpoint API keeps the old behaviour of
/// saving a snapshot without firing run-level triggers.
pub fn simulate_until(
    model: &Model,
    config: &SimConfig,
    index: u32,
    stop_at: f64,
) -> Result<(ReplicationResult, Checkpoint), SimError> {
    model.validate()?;
    if stop_at <= 0.0 {
        return Err(SimError::Config("stop_at must be positive".into()));
    }
    if stop_at > config.stop_time + EPS {
        return Err(SimError::Config(
            "stop_at must not exceed the configured stop_time".into(),
        ));
    }
    let mut sim = Simulation::new(model, config.clone(), false)?;
    sim.start()?;
    sim.pause_at = Some(stop_at);
    sim.run_events()?;
    sim.now = stop_at;
    sim.fire_run_triggers(SimTrigger::OnStop);
    if let Some(err) = sim.trigger_error.take() {
        return Err(err);
    }
    // Actions of the trigger (`release_item`, `send_message`) belong to the
    // pause instant, so they run before the snapshot is taken.
    sim.drain_instant_events()?;
    sim.finalize_stats()?;
    let checkpoint = sim.snapshot();
    let result = sim.replication_result(index, stop_at)?;
    Ok((result, checkpoint))
}

/// task-44: continue a run paused by [`simulate_until`], fire `on_resume` on
/// every object and run to `resume_at`.
pub fn resume_from_checkpoint(
    model: &Model,
    config: &SimConfig,
    index: u32,
    checkpoint: &Checkpoint,
    resume_at: f64,
) -> Result<ReplicationResult, SimError> {
    model.validate()?;
    if resume_at <= checkpoint.now + EPS {
        return Err(SimError::Config(
            "resume_at must be later than the checkpoint time".into(),
        ));
    }
    let mut resumed_config = config.clone();
    resumed_config.stop_time = resume_at;
    let mut sim = Simulation::new(model, resumed_config, false)?;
    sim.restore(checkpoint)?;
    sim.now = checkpoint.now;
    sim.pause_at = Some(resume_at);
    sim.fire_run_triggers(SimTrigger::OnResume);
    if let Some(err) = sim.trigger_error.take() {
        return Err(err);
    }
    sim.run_events()?;
    sim.now = resume_at;
    let stop_actions = sim.model.model_triggers.on_stop.clone();
    sim.fire_model_actions("model_triggers.on_stop", stop_actions);
    if let Some(error) = sim.trigger_error.take() {
        return Err(error);
    }
    sim.finalize_stats()?;
    let end_actions = sim.model.model_triggers.end_of_replication.clone();
    sim.fire_model_actions("model_triggers.end_of_replication", end_actions);
    if let Some(error) = sim.trigger_error.take() {
        return Err(error);
    }
    sim.replication_result(index, resume_at)
}

/// Summary of one scheduled event (task-50).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventPreview {
    /// Model time the event fires.
    pub time: f64,
    /// Insertion sequence number (breaks ties between events at the same time).
    pub seq: u64,
    /// Stable event name (`arrive`, `flow`, `process_done`, …).
    pub kind: String,
    /// Name of the object the event belongs to, if any.
    pub object: Option<String>,
    /// Optional extra detail such as `generation=3` or `token=2` (task-53).
    #[serde(default)]
    pub detail: Option<String>,
}

/// One replication of a multi-checkpoint run (task-56).
#[derive(Debug, Clone, PartialEq)]
pub struct ReplicationCheckpoint {
    /// Replication index (0-based).
    pub index: u32,
    /// Result of this replication at `stop_time`.
    pub result: ReplicationResult,
    /// File the checkpoint was written to (`checkpoint-<index>.json`).
    pub path: PathBuf,
}

/// One entry of a checkpoint directory manifest (task-59).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckpointManifestEntry {
    /// Replication index (0-based).
    pub index: u32,
    /// Seed used by this replication.
    pub seed: u64,
    /// Checkpoint file name inside the directory.
    pub file: String,
    pub items_created: u64,
    pub items_completed: u64,
    pub items_in_system: u64,
    /// False when `keep` pruned the file after the run.
    pub retained: bool,
    /// task-74: SHA-256 of the checkpoint file (hex), when it was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// `manifest.json` written next to the checkpoints (task-59).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckpointManifest {
    pub format_version: u32,
    pub model: String,
    pub stop_time: f64,
    pub replications: u32,
    pub base_seed: u64,
    /// Unix timestamp (seconds) of the latest run.
    pub created_at: u64,
    pub entries: Vec<CheckpointManifestEntry>,
    /// task-62: one record per run that wrote this manifest (oldest first).
    #[serde(default)]
    pub history: Vec<CheckpointManifestRun>,
}

/// One historical run recorded in a checkpoint manifest (task-62).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckpointManifestRun {
    pub created_at: u64,
    pub stop_time: f64,
    pub replications: u32,
}

/// Result of a checkpointed multi-replication run (task-59).
#[derive(Debug, Clone, PartialEq)]
pub struct CheckpointRun {
    /// One entry per replication (files that `keep` pruned are still listed,
    /// with `retained: false`).
    pub entries: Vec<ReplicationCheckpoint>,
    pub manifest: CheckpointManifest,
}

/// task-56: run every replication of the model to `stop_time` and write one
/// checkpoint per replication into `dir` (no pruning).
pub fn run_model_with_checkpoints(
    model: &Model,
    dir: &Path,
    stop_time: f64,
) -> Result<Vec<ReplicationCheckpoint>, SimError> {
    Ok(run_model_with_checkpoints_retained(model, dir, stop_time, None)?.entries)
}

/// task-59: like [`run_model_with_checkpoints`], but also writes
/// `manifest.json` and prunes the directory when `keep` is set.
///
/// `keep = Some(n)` keeps the newest `n` checkpoint files (by modification
/// time) and deletes the older ones; entries whose file was pruned stay in the
/// manifest with `retained: false`.
pub fn run_model_with_checkpoints_retained(
    model: &Model,
    dir: &Path,
    stop_time: f64,
    keep: Option<usize>,
) -> Result<CheckpointRun, SimError> {
    if stop_time <= 0.0 {
        return Err(SimError::Config("stop_time must be positive".into()));
    }
    std::fs::create_dir_all(dir).map_err(|error| {
        SimError::Config(format!(
            "could not create the checkpoint directory {}: {error}",
            dir.display()
        ))
    })?;
    let replications = model.run.replications.max(1);
    let mut written = Vec::with_capacity(replications as usize);
    let mut entries = Vec::with_capacity(replications as usize);
    for index in 0..replications {
        let seed = derive_seed(model.run.seed, index);
        let mut config = SimConfig::from_run_settings(&model.run, seed);
        config.stop_time = stop_time;
        let (result, checkpoint) = simulate_with_checkpoint(model, &config, index, None)?;
        let file = format!("checkpoint-{index}.json");
        let path = dir.join(&file);
        let json = checkpoint.to_json()?;
        std::fs::write(&path, &json).map_err(|error| {
            SimError::Config(format!("could not write {}: {error}", path.display()))
        })?;
        entries.push(CheckpointManifestEntry {
            index,
            seed,
            file,
            items_created: result.items_created,
            items_completed: result.items_completed,
            items_in_system: result.items_in_system,
            retained: true,
            sha256: Some(crate::sha256::hex(json.as_bytes())),
        });
        written.push(ReplicationCheckpoint {
            index,
            result,
            path,
        });
    }
    // task-59: keep only the newest `keep` checkpoint files.
    if let Some(limit) = keep {
        let removed = prune_checkpoints(dir, limit)?;
        for entry in &mut entries {
            if removed.contains(&entry.file) {
                entry.retained = false;
            }
        }
    }
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    // task-62: merge with a previous manifest in the same directory: entries
    // are keyed by replication index, and every run keeps a history record.
    let previous = load_manifest(dir);
    let mut merged: Vec<CheckpointManifestEntry> = previous
        .as_ref()
        .map(|manifest| manifest.entries.clone())
        .unwrap_or_default();
    for entry in entries {
        match merged
            .iter_mut()
            .find(|existing| existing.index == entry.index)
        {
            Some(existing) => *existing = entry,
            None => merged.push(entry),
        }
    }
    merged.sort_by_key(|entry| entry.index);
    // Entries whose file is no longer on disk are not retained.
    for entry in &mut merged {
        entry.retained = dir.join(&entry.file).is_file();
    }
    let mut history = previous
        .map(|manifest| manifest.history)
        .unwrap_or_default();
    history.push(CheckpointManifestRun {
        created_at,
        stop_time,
        replications,
    });
    // task-68: keep the history bounded (newest entries survive).
    const HISTORY_LIMIT: usize = 50;
    if history.len() > HISTORY_LIMIT {
        let excess = history.len() - HISTORY_LIMIT;
        history.drain(0..excess);
    }
    let manifest = CheckpointManifest {
        format_version: 1,
        model: model.name.clone(),
        stop_time,
        replications,
        base_seed: model.run.seed,
        created_at,
        entries: merged,
        history,
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| SimError::Config(format!("could not serialise manifest: {error}")))?;
    std::fs::write(dir.join("manifest.json"), json).map_err(|error| {
        SimError::Config(format!(
            "could not write {}: {error}",
            dir.join("manifest.json").display()
        ))
    })?;
    Ok(CheckpointRun {
        entries: written,
        manifest,
    })
}

/// Read an existing manifest; a missing or incompatible file is ignored.
fn load_manifest(dir: &Path) -> Option<CheckpointManifest> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let manifest: CheckpointManifest = serde_json::from_str(&text).ok()?;
    (manifest.format_version == 1).then_some(manifest)
}

fn negative_one() -> f64 {
    -1.0
}

fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// task-62/65: merge `entries` into the directory manifest and write it back.
fn write_merged_manifest(
    dir: &Path,
    model_name: &str,
    base_seed: u64,
    stop_time: f64,
    replications: u32,
    entries: Vec<CheckpointManifestEntry>,
    created_at: u64,
) -> Result<CheckpointManifest, SimError> {
    let previous = load_manifest(dir);
    let mut merged: Vec<CheckpointManifestEntry> = previous
        .as_ref()
        .map(|manifest| manifest.entries.clone())
        .unwrap_or_default();
    for entry in entries {
        match merged
            .iter_mut()
            .find(|existing| existing.index == entry.index)
        {
            Some(existing) => *existing = entry,
            None => merged.push(entry),
        }
    }
    merged.sort_by_key(|entry| entry.index);
    for entry in &mut merged {
        entry.retained = dir.join(&entry.file).is_file();
    }
    let mut history = previous
        .map(|manifest| manifest.history)
        .unwrap_or_default();
    history.push(CheckpointManifestRun {
        created_at,
        stop_time,
        replications,
    });
    let manifest = CheckpointManifest {
        format_version: 1,
        model: model_name.to_string(),
        stop_time,
        replications,
        base_seed,
        created_at,
        entries: merged,
        history,
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|error| SimError::Config(format!("could not serialise manifest: {error}")))?;
    let path = dir.join("manifest.json");
    std::fs::write(&path, json).map_err(|error| {
        SimError::Config(format!("could not write {}: {error}", path.display()))
    })?;
    Ok(manifest)
}

/// task-65: run the replications in parallel and write one checkpoint each.
///
/// The files are the same `checkpoint-<index>.json` files the serial variant
/// writes, the manifest is merged the same way, and the replication order in
/// the result is preserved.
pub fn run_model_with_checkpoints_parallel(
    model: &Model,
    dir: &Path,
    stop_time: f64,
    workers: usize,
) -> Result<CheckpointRun, SimError> {
    if stop_time <= 0.0 {
        return Err(SimError::Config("stop_time must be positive".into()));
    }
    std::fs::create_dir_all(dir).map_err(|error| {
        SimError::Config(format!(
            "could not create the checkpoint directory {}: {error}",
            dir.display()
        ))
    })?;
    let replications = model.run.replications.max(1);
    let requested = if workers == 0 {
        std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
    } else {
        workers
    };
    let worker_count = requested.max(1).min(replications as usize);
    // (result, file name) per replication index.
    let mut collected: Vec<Option<(ReplicationResult, String)>> = vec![None; replications as usize];
    let mut failure: Option<SimError> = None;
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for worker in 0..worker_count {
            handles.push(scope.spawn(move || {
                let mut local: Vec<(u32, ReplicationResult, String)> = Vec::new();
                let mut errors: Vec<SimError> = Vec::new();
                let mut index = worker as u32;
                while index < replications {
                    let seed = derive_seed(model.run.seed, index);
                    let mut config = SimConfig::from_run_settings(&model.run, seed);
                    config.stop_time = stop_time;
                    match simulate_with_checkpoint(model, &config, index, None) {
                        Ok((result, checkpoint)) => {
                            let file = format!("checkpoint-{index}.json");
                            let path = dir.join(&file);
                            let written = checkpoint.to_json().and_then(|json| {
                                std::fs::write(&path, json).map_err(|error| {
                                    SimError::Config(format!(
                                        "could not write {}: {error}",
                                        path.display()
                                    ))
                                })
                            });
                            match written {
                                Ok(()) => local.push((index, result, file)),
                                Err(error) => errors.push(error),
                            }
                        }
                        Err(error) => errors.push(error),
                    }
                    index += worker_count as u32;
                }
                (local, errors)
            }));
        }
        for handle in handles {
            let (local, errors) = handle.join().expect("a checkpoint worker panicked");
            for (index, result, file) in local {
                collected[index as usize] = Some((result, file));
            }
            if failure.is_none()
                && let Some(error) = errors.into_iter().next()
            {
                failure = Some(error);
            }
        }
    });
    if let Some(error) = failure {
        return Err(error);
    }
    let mut written = Vec::with_capacity(replications as usize);
    let mut entries = Vec::with_capacity(replications as usize);
    for (index, slot) in collected.into_iter().enumerate() {
        let Some((result, file)) = slot else {
            return Err(SimError::Config(format!(
                "replication {index} produced no checkpoint"
            )));
        };
        let seed = derive_seed(model.run.seed, index as u32);
        let content = std::fs::read(dir.join(&file)).ok();
        entries.push(CheckpointManifestEntry {
            index: index as u32,
            seed,
            file: file.clone(),
            items_created: result.items_created,
            items_completed: result.items_completed,
            items_in_system: result.items_in_system,
            retained: true,
            sha256: content.map(|bytes| crate::sha256::hex(&bytes)),
        });
        written.push(ReplicationCheckpoint {
            index: index as u32,
            result,
            path: dir.join(file),
        });
    }
    let manifest = write_merged_manifest(
        dir,
        &model.name,
        model.run.seed,
        stop_time,
        replications,
        entries,
        now_seconds(),
    )?;
    Ok(CheckpointRun {
        entries: written,
        manifest,
    })
}

/// Delete the oldest `checkpoint-*.json` files until at most `keep` remain.
/// Returns the names of the removed files.
fn prune_checkpoints(dir: &Path, keep: usize) -> Result<Vec<String>, SimError> {
    let mut files: Vec<(std::time::SystemTime, String)> = Vec::new();
    let read = std::fs::read_dir(dir)
        .map_err(|error| SimError::Config(format!("could not read {}: {error}", dir.display())))?;
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("checkpoint-") || !name.ends_with(".json") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        files.push((modified, name));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut removed = Vec::new();
    while files.len() > keep {
        let (_, name) = files.remove(0);
        let path = dir.join(&name);
        std::fs::remove_file(&path).map_err(|error| {
            SimError::Config(format!("could not remove {}: {error}", path.display()))
        })?;
        removed.push(name);
    }
    Ok(removed)
}

/// A live simulation that can be advanced incrementally (task-47)./// A live simulation that can be advanced incrementally (task-47).
///
/// The one-shot helpers ([`simulate_until`], [`resume_from_checkpoint`]) build a
/// fresh engine for every segment. A handle keeps the event calendar and the
/// object state in memory, so a host (CLI, studio, tests) can step, pause,
/// resume, snapshot and inspect the same run.
pub struct SimulationHandle<'m> {
    sim: Simulation<'m>,
    index: u32,
    paused: bool,
}

/// Read-only view of a running simulation (task-47).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationState {
    /// Current model time.
    pub now: f64,
    /// Whether [`SimulationHandle::pause`] was called and not yet resumed.
    pub paused: bool,
    pub items_created: u64,
    pub items_completed: u64,
    /// Per-object statistics up to `now`.
    pub objects: BTreeMap<String, ObjectStats>,
}

impl<'m> SimulationHandle<'m> {
    /// Build a handle for replication `index` at t = 0.
    pub fn new(model: &'m Model, config: &SimConfig, index: u32) -> Result<Self, SimError> {
        model.validate()?;
        if config.stop_time <= 0.0 {
            return Err(SimError::Config("stop_time must be positive".into()));
        }
        let mut sim = Simulation::new(model, config.clone(), false)?;
        sim.start()?;
        Ok(Self {
            sim,
            index,
            paused: false,
        })
    }

    /// Advance to `time` inclusive. Calling it repeatedly moves the same
    /// simulation forward; it never rebuilds the engine.
    pub fn step_until(&mut self, time: f64) -> Result<(), SimError> {
        if time + EPS < self.sim.now {
            return Err(SimError::Config(format!(
                "cannot step backwards: the simulation is at {}",
                self.sim.now
            )));
        }
        if time > self.sim.config.stop_time + EPS {
            return Err(SimError::Config(
                "step target exceeds the configured stop_time".into(),
            ));
        }
        if self.paused {
            self.resume()?;
        }
        self.sim.pause_at = Some(time);
        self.sim.run_events()?;
        self.sim.now = time;
        self.sim.sample_tracked_variables(time)?;
        Ok(())
    }

    /// Fire `on_stop` on every object and mark the handle paused.
    pub fn pause(&mut self) -> Result<(), SimError> {
        if self.paused {
            return Ok(());
        }
        self.sim.fire_run_triggers(SimTrigger::OnStop);
        if let Some(err) = self.sim.trigger_error.take() {
            return Err(err);
        }
        self.sim.drain_instant_events()?;
        self.paused = true;
        Ok(())
    }

    /// Fire `on_resume` (if paused) and continue.
    pub fn resume(&mut self) -> Result<(), SimError> {
        if !self.paused {
            return Ok(());
        }
        self.sim.fire_run_triggers(SimTrigger::OnResume);
        if let Some(err) = self.sim.trigger_error.take() {
            return Err(err);
        }
        self.paused = false;
        Ok(())
    }

    /// task-50: handle exactly one event. `None` means the event calendar is
    /// exhausted for the current horizon (stop time or pause point).
    pub fn step_event(&mut self) -> Result<Option<EventPreview>, SimError> {
        if self.paused {
            self.resume()?;
        }
        self.sim.step_one_event()
    }

    /// task-50: the next event that would be handled, without handling it.
    pub fn next_event(&self) -> Option<EventPreview> {
        self.sim.peek_event()
    }

    /// Checkpoint of the current state (same format as [`simulate_until`]).
    pub fn snapshot(&self) -> Checkpoint {
        self.sim.snapshot()
    }

    /// Restore a checkpoint into this handle.
    pub fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), SimError> {
        self.sim.restore(checkpoint)?;
        self.paused = false;
        Ok(())
    }

    /// Current time, pause flag and per-object statistics.
    pub fn state(&self) -> SimulationState {
        let elapsed = (self.sim.now - self.sim.config.warmup).max(0.0);
        let mut objects = BTreeMap::new();
        for object in &self.sim.objects {
            objects.insert(object.name.clone(), object.stats_at(self.sim.now, elapsed));
        }
        SimulationState {
            now: self.sim.now,
            paused: self.paused,
            items_created: self.sim.stats_created,
            items_completed: self.sim.stats_completed,
            objects,
        }
    }

    /// The replication result at the current time.
    ///
    /// Stepping cannot return an error; a failing performance-measure
    /// expression is surfaced by [`SimulationHandle::try_result`], and this
    /// convenience method falls back to a result without measures.
    pub fn result(&self) -> ReplicationResult {
        self.try_result().unwrap_or_else(|_| {
            self.sim
                .raw_replication_result(self.index, self.sim.now, BTreeMap::new())
        })
    }

    /// Fallible variant of [`SimulationHandle::result`] (task-155): a failing
    /// performance measure is reported instead of being silently omitted.
    pub fn try_result(&self) -> Result<ReplicationResult, SimError> {
        self.sim.replication_result(self.index, self.sim.now)
    }

    pub fn now(&self) -> f64 {
        self.sim.now
    }

    /// task-158: the model-declared tracked-variable series collected so far.
    pub fn tracked_series(&self) -> &BTreeMap<String, Vec<(f64, f64)>> {
        &self.sim.tracked_series
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

/// Like [`simulate_with_checkpoint`], but also records the event trace.
///
/// Tracing a *resumed* run is not supported (the event log would only cover
/// the resumed segment), so combining `resume` with tracing is an error.
pub fn simulate_with_checkpoint_traced(
    model: &Model,
    config: &SimConfig,
    index: u32,
    resume: Option<&Checkpoint>,
) -> Result<(ReplicationResult, Checkpoint, TraceLog), SimError> {
    if resume.is_some() {
        return Err(SimError::Config(
            "tracing is not supported while resuming from a checkpoint".into(),
        ));
    }
    let (result, checkpoint, trace) = simulate_run(model, config, index, true, resume, true)?;
    let checkpoint =
        checkpoint.ok_or_else(|| SimError::Config("the checkpoint was not captured".into()))?;
    Ok((result, checkpoint, trace))
}

/// task-164: execute one lifecycle hook's actions with a throwaway simulation
/// context (used by the CLI experiment runner for the experiment/scenario
/// boundaries). The returned trace contains the hook's `script_log` messages.
pub fn fire_model_hook_actions(
    model: &Model,
    config: &SimConfig,
    actions: Vec<Action>,
    location: &str,
) -> Result<TraceLog, SimError> {
    if actions.is_empty() {
        return Ok(TraceLog::default());
    }
    let mut sim = Simulation::new(model, config.clone(), true)?;
    sim.fire_model_actions(location, actions);
    if let Some(error) = sim.trigger_error.take() {
        return Err(error);
    }
    Ok(sim.trace.unwrap_or_default())
}

/// task-164: script-log messages of a hook's trace, in order.
pub fn hook_logs(trace: &TraceLog) -> Vec<String> {
    trace
        .events
        .iter()
        .filter(|event| event.event == "script_log")
        .map(|event| event.message.clone())
        .collect()
}

fn simulate_run(
    model: &Model,
    config: &SimConfig,
    index: u32,
    trace: bool,
    resume: Option<&Checkpoint>,
    save_checkpoint: bool,
) -> Result<(ReplicationResult, Option<Checkpoint>, TraceLog), SimError> {
    model.validate()?;
    if config.stop_time <= 0.0 {
        return Err(SimError::Config("stop_time must be positive".into()));
    }
    let mut sim = Simulation::new(model, config.clone(), trace)?;
    if let Some(checkpoint) = resume {
        sim.restore(checkpoint)?;
    } else {
        sim.start()?;
        let start_actions = sim.model.model_triggers.start_of_replication.clone();
        sim.fire_model_actions("model_triggers.start_of_replication", start_actions);
        if let Some(error) = sim.trigger_error.take() {
            return Err(error);
        }
    }
    sim.run_events()?;
    sim.advance_to_stop();
    let stop_actions = sim.model.model_triggers.on_stop.clone();
    sim.fire_model_actions("model_triggers.on_stop", stop_actions);
    if let Some(error) = sim.trigger_error.take() {
        return Err(error);
    }
    let checkpoint = if save_checkpoint {
        Some(sim.snapshot())
    } else {
        None
    };
    sim.finalize_stats()?;
    let end_actions = sim.model.model_triggers.end_of_replication.clone();
    sim.fire_model_actions("model_triggers.end_of_replication", end_actions);
    if let Some(error) = sim.trigger_error.take() {
        return Err(error);
    }
    let mut objects = BTreeMap::new();
    let elapsed = config.stop_time - config.warmup;
    for object in &sim.objects {
        // Decoration (`visual_tool`) takes no part in the simulation, so it must
        // not appear as a row of zeros next to the real objects.
        if object.kind == ObjectKind::VisualTool {
            continue;
        }
        objects.insert(object.name.clone(), object.stats(elapsed));
    }
    let items_completed = sim.stats_completed;
    let items_in_system: u64 = sim
        .objects
        .iter()
        .map(|object| (object.held() + object.out_pending.len()) as u64)
        .sum();
    let result = ReplicationResult {
        index,
        seed: config.seed,
        final_time: config.stop_time,
        warmup: config.warmup,
        items_in_system,
        items_created: sim.stats_created,
        items_completed,
        objects,
        process_flow: sim.flow_replication(elapsed),
        fluid: sim.fluid_replication(elapsed),
        performance_measures: sim.evaluate_performance_measures()?,
        tracked_series: sim.tracked_series.clone(),
        tracked_summaries: sim.tracked_summaries(elapsed),
    };
    Ok((result, checkpoint, sim.trace.unwrap_or_default()))
}

/// Derive the seed for replication `index` deterministically.
pub fn derive_seed(base: u64, index: u32) -> u64 {
    // SplitMix64: cheap, well distributed, and independent of the RNG crate.
    let mut z = base.wrapping_add(0x9E37_79B9_7F4A_7C15u64.wrapping_mul(u64::from(index) + 1));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// task-168: percentile with linear interpolation between order statistics.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = q.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let fraction = rank - lower as f64;
        sorted[lower] * (1.0 - fraction) + sorted[upper] * fraction
    }
}

fn staytime_summary(samples: &[f64]) -> StaytimeSummary {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = if sorted.is_empty() {
        0.0
    } else {
        sorted.iter().sum::<f64>() / sorted.len() as f64
    };
    StaytimeSummary {
        n: sorted.len() as u64,
        min: sorted.first().copied().unwrap_or(0.0),
        p50: percentile(&sorted, 0.5),
        p90: percentile(&sorted, 0.9),
        max: sorted.last().copied().unwrap_or(0.0),
        mean,
    }
}

struct Simulation<'m> {
    model: &'m Model,
    config: SimConfig,
    now: f64,
    events: BinaryHeap<Reverse<Event>>,
    next_seq: u64,
    objects: Vec<ObjectRt>,
    by_name: HashMap<String, usize>,
    /// Connection indices leaving / entering each object, in declaration order.
    outputs: Vec<Vec<usize>>,
    inputs: Vec<Vec<usize>>,
    conns: Vec<SimConn>,
    /// Transporters assigned to each dispatcher.
    transporters: Vec<Vec<usize>>,
    rng: ChaCha12Rng,
    /// Separate stream for source inter-arrival times: scenarios that only
    /// change processing times then see the same arrival sequence (common
    /// random numbers), which makes `openflex compare` meaningful.
    arrival_rng: ChaCha12Rng,
    items_created: u64,
    /// Items created since the last warm-up reset (reported).
    stats_created: u64,
    /// Items created after the warm-up that reached a sink (reported). Items
    /// created before the warm-up belong to the transient phase and are not
    /// counted, so `created - completed` equals the in-system items of the
    /// statistical window.
    stats_completed: u64,
    event_count: u64,
    /// B7: the optional process-flow token runtime (model and object flows).
    process_flow: Option<FlowRuntime>,
    /// Precompiled FlexScript sources, keyed by their source text (D2).
    scripts: HashMap<String, CompiledScript>,
    /// Trigger location of the actions currently being applied (script errors).
    action_location: String,
    /// task-106: AGVs currently occupying each charger.
    charger_in_use: HashMap<usize, u32>,
    /// task-106: AGVs waiting for a charger slot (FIFO per charger).
    charger_queue: HashMap<usize, VecDeque<usize>>,
    /// task-148: static traffic-control areas and their current occupancy.
    traffic_controls: Vec<TrafficControlRt>,
    traffic_occupancy: Vec<u32>,
    /// task-154: FIFO waiters per control area (transporter indices).
    traffic_waiters: Vec<VecDeque<usize>>,
    /// task-158/168: parsed tracked-variable expressions and their kinds.
    tracked_exprs: Vec<(String, Expr, TrackedKind)>,
    tracked_interval: f64,
    tracked_series: BTreeMap<String, Vec<(f64, f64)>>,
    /// task-168: previous raw value for `kinetic` (time, value).
    tracked_prev: BTreeMap<String, (f64, f64)>,
    /// task-168: completed-item stay times for `staytime` summaries.
    stay_samples: Vec<f64>,
    track_staytime: bool,
    /// task-171: item hints for the next `flow` trace event per object.
    flow_item_hints: HashMap<usize, u64>,
    /// task-171: item served by each executer task, for transport_* events.
    transport_item_hints: HashMap<usize, u64>,
    /// task-184: motor -> controlled conveyors and reverse map.
    motor_conveyors: Vec<Vec<usize>>,
    conveyor_motors: Vec<Vec<usize>>,
    /// task-173/175: active explicit task chain per executer.
    explicit_routes: HashMap<usize, ExplicitState>,
    /// task-173: target being approached by an active explicit travel leg.
    explicit_target: HashMap<usize, ([f64; 2], f64)>,
    /// task-56: cached shortest distances for transport legs, keyed by
    /// `(task executer, from object, to object)`. Object positions are fixed
    /// for the whole run, so a hit is always correct; restoring a checkpoint
    /// clears the cache.
    distance_cache: RefCell<HashMap<(usize, usize, usize), f64>>,
    /// task-44: stop the event loop when the clock reaches this time (the
    /// `on_stop` / `on_resume` controlled run); `None` = run to `stop_time`.
    pause_at: Option<f64>,
    /// Shaft group currently travelled by an elevator: a shared shaft admits
    /// one cabin at a time (task-36).
    shaft_owner: HashMap<String, usize>,
    /// Depth of the lifecycle trigger currently running. A trigger fired from
    /// inside another one is skipped so the engine stays re-entrant.
    trigger_depth: u32,
    /// A lifecycle trigger error, reported at the next event boundary.
    trigger_error: Option<SimError>,
    /// Static definitions of every flow block, by global block index.
    flow_defs: Vec<FlowBlockMeta>,
    /// Transfer blocks that inject items into an object, by target object.
    flow_transfer_targets: Vec<Vec<usize>>,
    /// `on_entry` create blocks, by owning object.
    flow_entry_blocks: Vec<Vec<usize>>,
    /// `from_object` transfer blocks waiting for items, by source object.
    flow_pull_sources: Vec<Vec<usize>>,
    /// B5: the optional fluid sub-system.
    fluid: Option<FluidRuntime>,
    /// Fluid volume waiting to be converted by a bridge object, keyed by index.
    pending_bridge_volume: HashMap<usize, f64>,
    /// B12: recorded events when tracing is enabled.
    trace: Option<TraceLog>,
}

impl<'m> Simulation<'m> {
    fn new(model: &'m Model, config: SimConfig, trace: bool) -> Result<Self, SimError> {
        let mut by_name = HashMap::new();
        for (index, object) in model.objects.iter().enumerate() {
            if by_name.insert(object.name.clone(), index).is_some() {
                return Err(SimError::Model(openflex_model::ModelError::Invalid(vec![
                    openflex_model::ValidationIssue::new(
                        format!("objects.{}", object.name),
                        "duplicate object name",
                    ),
                ])));
            }
        }

        // Precompute the connection tables: ports are the declaration order of
        // the connections, which is what the model format documents.
        let mut conns = Vec::with_capacity(model.connections.len());
        let mut outputs = vec![Vec::new(); model.objects.len()];
        let mut inputs = vec![Vec::new(); model.objects.len()];
        let mut input_ports = vec![0u32; model.objects.len()];
        // Every connection, including S links, in declaration order. Used for
        // team/resource resolution, never for routing.
        let mut all_outputs = vec![Vec::new(); model.objects.len()];
        for connection in &model.connections {
            let Some(&from) = by_name.get(&connection.from) else {
                continue;
            };
            let Some(&to) = by_name.get(&connection.to) else {
                continue;
            };
            let kind = connection.kind.unwrap_or_default();
            let index = conns.len();
            all_outputs[from].push(index);
            if kind == ConnectionKind::S {
                // S connections are center-port resource links (for example a
                // processor linked to its dispatcher). They occupy no output
                // port, add no input port and are never routed along.
                conns.push(SimConn {
                    from,
                    to,
                    to_port: 0,
                    declared_port: connection.port,
                    strategy: connection.strategy.clone(),
                    kind,
                });
                continue;
            }
            let to_port = input_ports[to];
            input_ports[to] += 1;
            conns.push(SimConn {
                from,
                to,
                to_port,
                declared_port: connection.port,
                strategy: connection.strategy.clone(),
                kind,
            });
            outputs[from].push(index);
            inputs[to].push(index);
        }

        let mut objects = Vec::with_capacity(model.objects.len());
        for (index, object) in model.objects.iter().enumerate() {
            let mut runtime = ObjectRt::new(object, 0.0);
            runtime.configure_ports(inputs[index].len());
            objects.push(runtime);
        }

        // Transporter home positions and dispatcher teams (either direction).
        let mut transporters = vec![Vec::new(); model.objects.len()];
        for (index, object) in model.objects.iter().enumerate() {
            let home = match &object.params {
                ObjectParams::Transporter(params) => params.home.clone(),
                ObjectParams::Person(params) => params.home.clone(),
                ObjectParams::Agv(params) => params.home.clone(),
                ObjectParams::Elevator(params) => params.home.clone(),
                ObjectParams::Robot(params) => params.home.clone(),
                ObjectParams::Crane(params) => params.home.clone(),
                ObjectParams::AsrsVehicle(params) => params.home.clone(),
                _ => None,
            };
            if let Some(home) = home
                && let Some(&target) = by_name.get(&home)
            {
                objects[index].home = Some(target);
            }
            // Resource teams may be expressed with an S link, so resolve them
            // from every connection, not only from the flow ports.
            for &conn in &all_outputs[index] {
                let to = conns[conn].to;
                let from_kind = object.kind;
                let to_kind = model.objects[to].kind;
                if from_kind == ObjectKind::Dispatcher && is_task_executer_kind(to_kind) {
                    transporters[index].push(to);
                    objects[to].assigned_dispatcher = Some(index);
                } else if is_task_executer_kind(from_kind) && to_kind == ObjectKind::Dispatcher {
                    transporters[to].push(index);
                    objects[index].assigned_dispatcher = Some(to);
                }
            }
        }

        let mut motor_conveyors = vec![Vec::new(); model.objects.len()];
        let mut conveyor_motors = vec![Vec::new(); model.objects.len()];
        for connection in &conns {
            if connection.kind == ConnectionKind::S {
                continue;
            }
            if model.objects[connection.from].kind == ObjectKind::Motor
                && matches!(
                    model.objects[connection.to].kind,
                    ObjectKind::Conveyor | ObjectKind::Escalator
                )
            {
                motor_conveyors[connection.from].push(connection.to);
                conveyor_motors[connection.to].push(connection.from);
            }
        }
        let scripts = compile_model_scripts(model)?;

        // B7: resolve model-level and object-level process flows into one
        // global block graph (events identify blocks by their global index).
        let (process_flow, flow_defs, flow_transfer_targets, flow_entry_blocks, flow_pull_sources) =
            build_process_flows(model, &by_name);

        let fluid = build_fluid_runtime(model);
        let traffic_controls = build_traffic_controls(model);
        let mut tracked_exprs = Vec::new();
        for variable in &model.tracked_variables {
            let expression =
                Expr::parse(&variable.expression).map_err(|error| SimError::Expression {
                    location: format!("tracked_variables.{}", variable.name),
                    detail: format!("expression does not compile: {error}"),
                })?;
            tracked_exprs.push((variable.name.clone(), expression, variable.kind));
        }
        let track_staytime = tracked_exprs
            .iter()
            .any(|(_, _, kind)| *kind == TrackedKind::Staytime);
        let tracked_interval = model.tracked_interval.unwrap_or(1.0);
        if !tracked_exprs.is_empty() && (!tracked_interval.is_finite() || tracked_interval <= 0.0) {
            return Err(SimError::Config(
                "tracked_interval must be a positive number of time units".into(),
            ));
        }

        let seed = config.seed;
        let mut sim = Self {
            model,
            config,
            now: 0.0,
            events: BinaryHeap::new(),
            next_seq: 0,
            objects,
            by_name,
            outputs,
            inputs,
            conns,
            transporters,
            rng: ChaCha12Rng::seed_from_u64(seed),
            arrival_rng: ChaCha12Rng::seed_from_u64(derive_seed(seed ^ 0xA5A5_5A5A_1234_5678, 7)),
            items_created: 0,
            stats_created: 0,
            stats_completed: 0,
            event_count: 0,
            process_flow,
            scripts,
            action_location: String::new(),
            distance_cache: RefCell::new(HashMap::new()),
            charger_in_use: HashMap::new(),
            charger_queue: HashMap::new(),
            traffic_waiters: vec![VecDeque::new(); traffic_controls.len()],
            traffic_occupancy: vec![0; traffic_controls.len()],
            traffic_controls,
            motor_conveyors,
            conveyor_motors,
            tracked_exprs,
            tracked_interval,
            tracked_series: BTreeMap::new(),
            tracked_prev: BTreeMap::new(),
            stay_samples: Vec::new(),
            track_staytime,
            flow_item_hints: HashMap::new(),
            transport_item_hints: HashMap::new(),
            explicit_routes: HashMap::new(),
            explicit_target: HashMap::new(),
            pause_at: None,
            shaft_owner: HashMap::new(),
            trigger_depth: 0,
            trigger_error: None,
            flow_defs,
            flow_transfer_targets,
            flow_entry_blocks,
            flow_pull_sources,
            fluid,
            pending_bridge_volume: HashMap::new(),
            trace: if trace {
                Some(TraceLog::default())
            } else {
                None
            },
        };
        sim.initialize()?;
        Ok(sim)
    }

    fn initialize(&mut self) -> Result<(), SimError> {
        if self.config.warmup > 0.0 {
            self.schedule(self.config.warmup, EventKind::ResetStats);
        }
        // Shift plans and availability calendars decide which objects are
        // available at t = 0 and when their availability changes (B11).
        for index in 0..self.model.objects.len() {
            let shifts = self.model.objects[index].params.shifts().to_vec();
            let timetable = self.model.objects[index].params.timetable().cloned();
            if shifts.is_empty() && timetable.is_none() {
                continue;
            }
            let shifts_ok = shifts.is_empty() || shifts::is_active(&shifts, 0.0);
            let timetable_ok = timetable
                .as_ref()
                .is_none_or(|table| timetable_active(&table.windows, 0.0));
            if !(shifts_ok && timetable_ok) {
                self.objects[index].on_shift = false;
                self.objects[index].downtime.push(DowntimeSource::Shift);
                self.set_state(index, State::OffShift);
            }
            if !shifts.is_empty()
                && let Some(next) = shifts::next_transition(&shifts, 0.0)
            {
                self.schedule(next, EventKind::ShiftChange(index));
            }
            if let Some(table) = timetable
                && let Some(next) = next_timetable_transition(&table.windows, 0.0)
            {
                self.schedule(next, EventKind::TimetableChange(index));
            }
        }
        for index in 0..self.model.objects.len() {
            let params = self.model.objects[index].params.clone();
            match params {
                ObjectParams::Source(params) => {
                    self.schedule_first_arrival(index, &params)?;
                }
                ObjectParams::Processor(params) => {
                    if let Some(failure) = &params.failure {
                        // A8: `first_failure_time` overrides the first MTBF
                        // draw; negative values mean "ignore".
                        self.schedule_next_failure(index, failure.first_failure_time);
                    }
                }
                _ => {}
            }
        }
        // A3: a puller starts empty, so it must ask for work at t = 0.
        for index in 0..self.model.objects.len() {
            self.schedule_pull(index);
        }
        // task-36: an Emptier starts its sweep cycle.
        for index in 0..self.model.objects.len() {
            self.schedule_emptier_sweep(index);
        }
        // B5: fluid models integrate on a fixed step.
        if let Some(fluid) = self.fluid.as_ref() {
            let first = fluid.step;
            self.schedule(first, EventKind::FluidStep);
        }
        // task-158: model-declared tracked variables sample on a fixed interval
        // from the end of warm-up up to (but not including) the stop time; the
        // final sample at the stop time is added by `finalize_stats`.
        if self
            .tracked_exprs
            .iter()
            .any(|(_, _, kind)| matches!(kind, TrackedKind::Level | TrackedKind::Kinetic))
        {
            let first = self.config.warmup + self.tracked_interval;
            if first < self.config.stop_time - EPS {
                self.schedule(first, EventKind::TrackedSample);
            }
        }
        // B7: create blocks start producing tokens.
        if self.process_flow.is_some() {
            for block in 0..self.process_flow_block_count() {
                if matches!(self.flow_block_kind(block), Some(FlowBlockKind::Create(_))) {
                    self.schedule_flow_create(block)?;
                }
            }
        }
        // task-184: motors that start stopped keep their conveyors frozen from
        // the first tick (positions remain whatever the initial state is).
        for motor in 0..self.objects.len() {
            if self.objects[motor].kind == ObjectKind::Motor && !self.objects[motor].motor_running {
                for conveyor in self.motor_conveyors.get(motor).cloned().unwrap_or_default() {
                    self.objects[conveyor].belt_last_update = self.now;
                    self.objects[conveyor].belt_tick_at = f64::INFINITY;
                    self.set_state(conveyor, State::OffShift);
                }
            }
        }
        Ok(())
    }

    fn schedule(&mut self, time: f64, kind: EventKind) {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.events.push(Reverse(Event { time, seq, kind }));
    }

    fn sample(&mut self, distribution: &Distribution) -> f64 {
        sample_distribution(&mut self.rng, distribution)
    }

    /// Sample an inter-arrival time from the arrivals-only stream.
    fn sample_arrival(&mut self, distribution: &Distribution) -> f64 {
        sample_distribution(&mut self.arrival_rng, distribution)
    }
}

/// A10: how many operators a setup or process phase needs.
///
/// `use_same_num_operators` makes the process phase reuse the setup count, so a
/// model only declares the crew size once.
/// B1: a MultiProcessor requests a crew of `crew` operators for its busy
/// period through the shared dispatcher/resource machinery.
fn mp_operator_params(
    params: &openflex_model::MultiProcessorParams,
    crew: u32,
) -> openflex_model::ProcessorParams {
    let operator = params
        .process_operator
        .clone()
        .or_else(|| params.setup_operator.clone());
    openflex_model::ProcessorParams {
        setup_operator: operator.clone(),
        process_operator: operator,
        num_setup_operators: Some(crew),
        num_process_operators: Some(crew),
        use_same_num_operators: true,
        ..Default::default()
    }
}

/// A7: the Combiner inherits the Processor operator fields; wrap them in a
/// `ProcessorParams` so the shared operator machinery can be reused.
fn combiner_operator_params(
    params: &openflex_model::CombinerParams,
) -> openflex_model::ProcessorParams {
    openflex_model::ProcessorParams {
        setup_operator: params.setup_operator.clone(),
        process_operator: params.process_operator.clone(),
        ..Default::default()
    }
}

fn operators_needed(params: &openflex_model::ProcessorParams, phase: OperatorPhase) -> u32 {
    let count = match (phase, params.use_same_num_operators) {
        (OperatorPhase::Process, true) => {
            params.num_setup_operators.or(params.num_process_operators)
        }
        (OperatorPhase::Setup, _) => params.num_setup_operators,
        (OperatorPhase::Process, false) => params.num_process_operators,
    };
    count.unwrap_or(1).max(1)
}

fn sample_distribution(rng: &mut ChaCha12Rng, distribution: &Distribution) -> f64 {
    {
        match distribution {
            Distribution::Constant { value } => *value,
            Distribution::Uniform { min, max } => rng.r#gen_range(*min..*max),
            Distribution::Normal { mean, stddev } => {
                if *stddev <= 0.0 {
                    *mean
                } else {
                    Normal::new(*mean, *stddev)
                        .map(|d| d.sample(&mut *rng))
                        .unwrap_or(*mean)
                }
            }
            Distribution::Exponential { mean } => Exp::new(1.0 / *mean)
                .map(|d| d.sample(&mut *rng))
                .unwrap_or(*mean),
            Distribution::Triangular { min, mode, max } => {
                let u: f64 = rng.r#gen();
                let width = max - min;
                if width <= 0.0 {
                    *min
                } else {
                    let split = (mode - min) / width;
                    if u < split {
                        min + (u * width * (mode - min)).sqrt()
                    } else {
                        max - ((1.0 - u) * width * (max - mode)).sqrt()
                    }
                }
            }
            Distribution::Gamma { shape, scale } => Gamma::new(*shape, *scale)
                .map(|d| d.sample(&mut *rng))
                .unwrap_or(*scale),
            Distribution::Weibull { shape, scale } => {
                // Inverse CDF: scale * (-ln(U))^(1/shape)
                let u: f64 = rng.r#gen::<f64>().max(1e-12);
                scale * (-u.ln()).powf(1.0 / shape)
            }
            Distribution::Lognormal { meanlog, sdlog } => {
                if *sdlog <= 0.0 {
                    meanlog.exp()
                } else {
                    LogNormal::new(*meanlog, *sdlog)
                        .map(|d| d.sample(&mut *rng))
                        .unwrap_or_else(|_| meanlog.exp())
                }
            }
            Distribution::Erlang { shape, scale } => {
                let mut total = 0.0;
                for _ in 0..*shape {
                    let u: f64 = rng.r#gen::<f64>().max(1e-12);
                    total += -scale * u.ln();
                }
                total
            }
            Distribution::Beta { alpha, beta } => {
                // Two gamma samples: X ~ Gamma(alpha), Y ~ Gamma(beta), X/(X+Y).
                let x = Gamma::new(*alpha, 1.0)
                    .map(|d| d.sample(&mut *rng))
                    .unwrap_or(0.0);
                let y = Gamma::new(*beta, 1.0)
                    .map(|d| d.sample(&mut *rng))
                    .unwrap_or(1.0);
                if x + y > 0.0 { x / (x + y) } else { 0.5 }
            }
            Distribution::IntegerUniform { min, max } => rng.r#gen_range(*min..=*max) as f64,
            Distribution::Pareto { shape, scale } => {
                // Inverse CDF: scale / U^(1/shape).
                let u: f64 = rng.r#gen::<f64>().clamp(1e-12, 1.0);
                scale / u.powf(1.0 / shape)
            }
            Distribution::LogLogistic { alpha, beta } => {
                // Inverse CDF: alpha * (U / (1 - U))^(1/beta).
                let u: f64 = rng.r#gen::<f64>().clamp(1e-12, 1.0 - 1e-12);
                alpha * (u / (1.0 - u)).powf(1.0 / beta)
            }
            Distribution::Rayleigh { sigma } => {
                // Inverse CDF: sigma * sqrt(-2 ln U).
                let u: f64 = rng.r#gen::<f64>().clamp(1e-12, 1.0);
                sigma * (-2.0 * u.ln()).sqrt()
            }
            Distribution::Binomial { n, p } => {
                let n_f = f64::from(*n);
                let mean = n_f * p;
                let variance = n_f * p * (1.0 - p);
                if *n > 100 {
                    // Normal approximation keeps the cost independent of `n`.
                    let sample = Normal::new(mean, variance.sqrt().max(EPS))
                        .map(|d| d.sample(&mut *rng))
                        .unwrap_or(mean);
                    sample.round().clamp(0.0, n_f)
                } else {
                    let mut successes = 0.0;
                    for _ in 0..*n {
                        if rng.r#gen::<f64>() < *p {
                            successes += 1.0;
                        }
                    }
                    successes
                }
            }
            Distribution::Poisson { mean } => {
                if *mean <= 0.0 {
                    0.0
                } else if *mean < 30.0 {
                    // Knuth's product method.
                    let limit = (-mean).exp();
                    let mut count = 0.0;
                    let mut product = 1.0;
                    loop {
                        count += 1.0;
                        product *= rng.r#gen::<f64>().max(1e-300);
                        if product <= limit || count > 1_000_000.0 {
                            break;
                        }
                    }
                    count - 1.0
                } else {
                    // Normal approximation for the large-rate case.
                    let sample = Normal::new(*mean, mean.sqrt())
                        .map(|d| d.sample(&mut *rng))
                        .unwrap_or(*mean);
                    sample.round().max(0.0)
                }
            }
            Distribution::Geometric { p } => {
                // Failures before the first success: floor(ln U / ln(1 - p)).
                if *p >= 1.0 {
                    0.0
                } else {
                    let u: f64 = rng.r#gen::<f64>().clamp(1e-12, 1.0);
                    (u.ln() / (1.0 - p).ln()).floor().max(0.0)
                }
            }
            Distribution::NegativeBinomial { r, p } => {
                // Sum of `r` geometric (failure count) samples.
                if *p >= 1.0 {
                    0.0
                } else {
                    let mut failures = 0.0;
                    for _ in 0..*r {
                        let u: f64 = rng.r#gen::<f64>().clamp(1e-12, 1.0);
                        failures += (u.ln() / (1.0 - p).ln()).floor().max(0.0);
                    }
                    failures
                }
            }
            Distribution::Bernoulli { p } => {
                if rng.r#gen::<f64>() < *p {
                    1.0
                } else {
                    0.0
                }
            }
            Distribution::Empirical { values, weights } => {
                if values.is_empty() {
                    return 0.0;
                }
                match weights {
                    None => values[rng.r#gen_range(0..values.len())],
                    Some(weights) if weights.len() == values.len() => {
                        let total: f64 = weights.iter().sum();
                        let mut pick = rng.r#gen::<f64>() * total;
                        for (value, weight) in values.iter().zip(weights) {
                            if pick <= *weight {
                                return *value;
                            }
                            pick -= *weight;
                        }
                        values[values.len() - 1]
                    }
                    Some(_) => values[0],
                }
            }
        }
    }
}

impl<'m> Simulation<'m> {
    /// Process every event strictly before `stop_time`.
    ///
    /// Events at or after the stop time stay in the calendar: a checkpoint
    /// taken here keeps them, so resuming with a later stop time continues
    /// exactly where the run left off.
    fn run_events(&mut self) -> Result<(), SimError> {
        while let Some(Reverse(next)) = self.events.peek() {
            let next_time = next.time;
            let horizon = match self.pause_at {
                Some(pause) => pause.min(self.config.stop_time),
                None => self.config.stop_time,
            };
            if next_time >= horizon - EPS {
                break;
            }
            let Some(Reverse(event)) = self.events.pop() else {
                break;
            };
            if event.time > self.now {
                self.now = event.time;
            }
            self.event_count += 1;
            if self.event_count > MAX_EVENTS {
                return Err(SimError::EventLimit(MAX_EVENTS));
            }
            self.record_trace(event.time, event.seq, &event.kind);
            self.handle(event.kind)?;
            if let Some(err) = self.trigger_error.take() {
                return Err(err);
            }
        }
        // task-44: align the clock with the pause point so the snapshot and the
        // reported statistics stop exactly there.
        if let Some(pause) = self.pause_at
            && self.now < pause
        {
            self.now = pause;
        }
        Ok(())
    }

    /// Process the events scheduled at the current instant. A run-level trigger
    /// may schedule work (`release_item`, `send_message`) that belongs to the
    /// pause instant; without this the pause would swallow it.
    fn drain_instant_events(&mut self) -> Result<(), SimError> {
        let now = self.now;
        while let Some(Reverse(next)) = self.events.peek() {
            if next.time > now + EPS {
                break;
            }
            let Some(Reverse(event)) = self.events.pop() else {
                break;
            };
            self.event_count += 1;
            if self.event_count > MAX_EVENTS {
                return Err(SimError::EventLimit(MAX_EVENTS));
            }
            self.handle(event.kind)?;
            if let Some(err) = self.trigger_error.take() {
                return Err(err);
            }
        }
        Ok(())
    }

    /// task-50: preview the next event without consuming it.
    fn peek_event(&self) -> Option<EventPreview> {
        let Reverse(next) = self.events.peek()?;
        let horizon = match self.pause_at {
            Some(pause) => pause.min(self.config.stop_time),
            None => self.config.stop_time,
        };
        if next.time >= horizon - EPS {
            return None;
        }
        let (index, kind, detail) = describe_event(&next.kind);
        Some(EventPreview {
            time: next.time,
            seq: next.seq,
            kind: kind.to_string(),
            object: index.map(|index| self.objects[index].name.clone()),
            detail: detail.map(|(name, value)| format!("{name}={value}")),
        })
    }

    /// task-50: handle exactly one event.
    fn step_one_event(&mut self) -> Result<Option<EventPreview>, SimError> {
        let Some(Reverse(next)) = self.events.peek() else {
            return Ok(None);
        };
        let horizon = match self.pause_at {
            Some(pause) => pause.min(self.config.stop_time),
            None => self.config.stop_time,
        };
        if next.time >= horizon - EPS {
            return Ok(None);
        }
        let (object, kind, detail) = describe_event(&next.kind);
        let preview = EventPreview {
            time: next.time,
            seq: next.seq,
            kind: kind.to_string(),
            object: object.map(|index| self.objects[index].name.clone()),
            detail: detail.map(|(name, value)| format!("{name}={value}")),
        };
        let Some(Reverse(event)) = self.events.pop() else {
            return Ok(None);
        };
        if event.time > self.now {
            self.now = event.time;
        }
        self.event_count += 1;
        if self.event_count > MAX_EVENTS {
            return Err(SimError::EventLimit(MAX_EVENTS));
        }
        self.record_trace(event.time, event.seq, &event.kind);
        self.handle(event.kind)?;
        if let Some(err) = self.trigger_error.take() {
            return Err(err);
        }
        Ok(Some(preview))
    }

    /// Fire a run-level trigger (`on_stop` / `on_resume`) on every object.
    fn fire_run_triggers(&mut self, trigger: SimTrigger) {
        for index in 0..self.objects.len() {
            self.fire_lifecycle_trigger(index, trigger);
        }
    }

    /// Build the replication result at `final_time` (the caller finalised the
    /// accumulators first).
    /// task-155: evaluate the model's performance measures once per
    /// replication. Expressions may read `time`, `created`, `completed`,
    /// global variables and object fields; a bad expression is a hard error
    /// that names the measure.
    fn evaluate_performance_measures(&self) -> Result<BTreeMap<String, f64>, SimError> {
        let mut values = BTreeMap::new();
        for measure in &self.model.performance_measures {
            let location = format!("performance_measures.{}", measure.name);
            let expression =
                Expr::parse(&measure.expression).map_err(|error| SimError::Expression {
                    location: location.clone(),
                    detail: format!("expression does not compile: {error}"),
                })?;
            let value = expression
                .eval(&ExprContext::new(self, None))
                .map_err(|error| SimError::Expression {
                    location: location.clone(),
                    detail: error.to_string(),
                })?;
            let number = value.as_number().map_err(|_| SimError::Expression {
                location: location.clone(),
                detail: format!(
                    "performance measure '{}' must evaluate to a number, got {value:?}",
                    measure.name
                ),
            })?;
            values.insert(measure.name.clone(), number);
        }
        Ok(values)
    }

    fn replication_result(
        &self,
        index: u32,
        final_time: f64,
    ) -> Result<ReplicationResult, SimError> {
        Ok(self.raw_replication_result(index, final_time, self.evaluate_performance_measures()?))
    }

    fn raw_replication_result(
        &self,
        index: u32,
        final_time: f64,
        performance_measures: BTreeMap<String, f64>,
    ) -> ReplicationResult {
        let elapsed = (final_time - self.config.warmup).max(0.0);
        let mut objects = BTreeMap::new();
        for object in &self.objects {
            objects.insert(
                object.name.clone(),
                object.stats_at(self.now.max(final_time), elapsed),
            );
        }
        let items_in_system: u64 = self
            .objects
            .iter()
            .map(|object| (object.held() + object.out_pending.len()) as u64)
            .sum();
        ReplicationResult {
            index,
            seed: self.config.seed,
            final_time,
            warmup: self.config.warmup,
            items_in_system,
            items_created: self.stats_created,
            items_completed: self.stats_completed,
            objects,
            process_flow: self.flow_replication(elapsed),
            fluid: self.fluid_replication(elapsed),
            performance_measures,
            tracked_series: self.tracked_series.clone(),
            tracked_summaries: self.tracked_summaries(elapsed),
        }
    }

    /// Statistics run until the stop time even if no event lands exactly there.
    fn advance_to_stop(&mut self) {
        if self.now < self.config.stop_time {
            self.now = self.config.stop_time;
        }
    }

    /// task-158: evaluate every tracked expression and append `(time, value)`
    /// samples. A failing expression is reported with the variable name.
    /// task-168: build `state`/`staytime` summaries for one replication.
    fn tracked_summaries(&self, elapsed: f64) -> BTreeMap<String, TrackedSummary> {
        let mut out = BTreeMap::new();
        let wants_state = self
            .tracked_exprs
            .iter()
            .any(|(_, _, kind)| *kind == TrackedKind::State);
        let state_shares = wants_state.then(|| {
            let mut totals: BTreeMap<String, f64> = BTreeMap::new();
            for object in &self.objects {
                if object.kind == ObjectKind::VisualTool {
                    continue;
                }
                let states = object.stats(elapsed).state_times;
                for (name, value) in [
                    ("idle", states.idle),
                    ("busy", states.busy),
                    ("blocked", states.blocked),
                    ("down", states.down),
                    ("off_shift", states.off_shift),
                ] {
                    *totals.entry(name.to_string()).or_default() += value;
                }
            }
            let total: f64 = totals.values().sum();
            if total <= 0.0 {
                BTreeMap::new()
            } else {
                // Model-wide fallback: the expression is compile-checked but
                // not used to select one object (documented limitation).
                totals
                    .into_iter()
                    .map(|(name, value)| (name, value / total))
                    .collect()
            }
        });
        let wants_staytime = self
            .tracked_exprs
            .iter()
            .any(|(_, _, kind)| *kind == TrackedKind::Staytime);
        let staytime = wants_staytime.then(|| staytime_summary(&self.stay_samples));
        for (name, _, kind) in &self.tracked_exprs {
            let summary = match kind {
                TrackedKind::State => TrackedSummary {
                    state_shares: state_shares.clone(),
                    staytime: None,
                },
                TrackedKind::Staytime => TrackedSummary {
                    state_shares: None,
                    staytime: staytime.clone(),
                },
                TrackedKind::Level | TrackedKind::Kinetic => continue,
            };
            out.insert(name.clone(), summary);
        }
        out
    }

    fn sample_tracked_variables(&mut self, at: f64) -> Result<(), SimError> {
        if self
            .tracked_exprs
            .iter()
            .all(|(_, _, kind)| !matches!(kind, TrackedKind::Level | TrackedKind::Kinetic))
        {
            return Ok(());
        }
        if !self.tracked_series.is_empty()
            && self.tracked_series.values().all(|samples| {
                samples
                    .last()
                    .is_some_and(|(time, _)| (at - *time).abs() <= EPS)
            })
        {
            return Ok(());
        }
        let expressions = self.tracked_exprs.clone();
        for (name, expression, kind) in expressions {
            if !matches!(kind, TrackedKind::Level | TrackedKind::Kinetic) {
                continue;
            }
            let location = format!("tracked_variables.{name}");
            let value = expression
                .eval(&ExprContext::new(self, None))
                .map_err(|error| SimError::Expression {
                    location: location.clone(),
                    detail: error.to_string(),
                })?;
            let number = value.as_number().map_err(|_| SimError::Expression {
                location: location.clone(),
                detail: format!(
                    "tracked variable '{name}' must evaluate to a number, got {value:?}"
                ),
            })?;
            let samples = self.tracked_series.entry(name.clone()).or_default();
            if samples
                .last()
                .is_some_and(|(time, _)| (at - *time).abs() <= EPS)
            {
                continue;
            }
            let sampled = match kind {
                TrackedKind::Level => number,
                TrackedKind::Kinetic => match self.tracked_prev.get(&name).copied() {
                    // First kinetic sample has no previous point: record 0.
                    None => 0.0,
                    Some((previous_time, previous_value)) => {
                        let interval = (at - previous_time).max(EPS);
                        (number - previous_value) / interval
                    }
                },
                TrackedKind::State | TrackedKind::Staytime => unreachable!(),
            };
            self.tracked_prev.insert(name, (at, number));
            samples.push((at, sampled));
        }
        Ok(())
    }

    fn handle_tracked_sample(&mut self) -> Result<(), SimError> {
        let at = self.now;
        self.sample_tracked_variables(at)?;
        let next = at + self.tracked_interval;
        let horizon = match self.pause_at {
            Some(pause) => pause.min(self.config.stop_time),
            None => self.config.stop_time,
        };
        if next < horizon - EPS {
            self.schedule(next, EventKind::TrackedSample);
        }
        Ok(())
    }

    fn finalize_stats(&mut self) -> Result<(), SimError> {
        let now = self.now;
        for object in &mut self.objects {
            object.finalize(now);
        }
        self.finalize_flow();
        self.finalize_fluid();
        self.sample_tracked_variables(self.now)?;
        Ok(())
    }

    /// Capture the current state (B12); called at the stop time, before the
    /// statistical finalisation, so a resumed run continues seamlessly.
    fn snapshot(&self) -> Checkpoint {
        Checkpoint {
            format_version: CHECKPOINT_FORMAT_VERSION,
            model_name: self.model.name.clone(),
            now: self.now,
            object_names: self
                .objects
                .iter()
                .map(|object| object.name.clone())
                .collect(),
            next_seq: self.next_seq,
            events: self
                .events
                .iter()
                .map(|Reverse(event)| event.clone())
                .collect(),
            objects: self.objects.clone(),
            rng: self.rng.clone(),
            arrival_rng: self.arrival_rng.clone(),
            items_created: self.items_created,
            stats_created: self.stats_created,
            stats_completed: self.stats_completed,
            event_count: self.event_count,
            process_flow: self.process_flow.clone(),
            fluid: self.fluid.clone(),
            explicit_routes: self
                .explicit_routes
                .iter()
                .map(|(index, state)| {
                    let mut state = state.clone();
                    state.remaining = state
                        .leg_deadline
                        .map(|deadline| (deadline - self.now).max(0.0));
                    (self.objects[*index].name.clone(), state)
                })
                .collect(),
            config: self.config.clone(),
        }
    }

    /// Restore a snapshot produced by [`Simulation::snapshot`].
    ///
    /// The object model must be unchanged (same name, same object list) and
    /// the seed/warm-up must match the saved run; the stop time may be later
    /// than the checkpoint time (that is the point of resuming).
    fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), SimError> {
        self.distance_cache.borrow_mut().clear();
        if checkpoint.format_version != CHECKPOINT_FORMAT_VERSION {
            return Err(SimError::Config(format!(
                "unsupported checkpoint format version {} (this build supports {})",
                checkpoint.format_version, CHECKPOINT_FORMAT_VERSION
            )));
        }
        if checkpoint.model_name != self.model.name {
            return Err(SimError::Config(format!(
                "checkpoint belongs to model '{}', not '{}'",
                checkpoint.model_name, self.model.name
            )));
        }
        let names: Vec<String> = self
            .objects
            .iter()
            .map(|object| object.name.clone())
            .collect();
        if names != checkpoint.object_names {
            return Err(SimError::Config(
                "checkpoint object list does not match the model".into(),
            ));
        }
        if checkpoint.config.seed != self.config.seed {
            return Err(SimError::Config(format!(
                "checkpoint seed {} does not match the run seed {}",
                checkpoint.config.seed, self.config.seed
            )));
        }
        if (checkpoint.config.warmup - self.config.warmup).abs() > EPS {
            return Err(SimError::Config(format!(
                "checkpoint warm-up {} does not match the run warm-up {}",
                checkpoint.config.warmup, self.config.warmup
            )));
        }
        if self.config.stop_time < checkpoint.now - EPS {
            return Err(SimError::Config(format!(
                "cannot resume a checkpoint from t={} with stop_time {}",
                checkpoint.now, self.config.stop_time
            )));
        }
        self.now = checkpoint.now;
        self.next_seq = checkpoint.next_seq;
        self.events = checkpoint
            .events
            .iter()
            .cloned()
            .map(Reverse)
            .collect::<BinaryHeap<_>>();
        self.objects = checkpoint.objects.clone();
        // task-148: repeat the admission counters from the restored holdings so
        // a resumed run never leaks (or forgets) traffic-control slots.
        self.traffic_occupancy = vec![0; self.traffic_controls.len()];
        self.traffic_waiters = vec![VecDeque::new(); self.traffic_controls.len()];
        for (index, object) in self.objects.iter().enumerate() {
            for &control in &object.traffic_hold {
                if let Some(slot) = self.traffic_occupancy.get_mut(control) {
                    *slot += 1;
                }
            }
            if object.traffic_wait_stage.is_some() {
                for &control in &object.traffic_wait_controls {
                    if let Some(queue) = self.traffic_waiters.get_mut(control) {
                        queue.push_back(index);
                    }
                }
            }
        }
        self.rng = checkpoint.rng.clone();
        self.arrival_rng = checkpoint.arrival_rng.clone();
        self.items_created = checkpoint.items_created;
        self.stats_created = checkpoint.stats_created;
        self.stats_completed = checkpoint.stats_completed;
        self.event_count = checkpoint.event_count;
        if let Some(saved) = checkpoint.process_flow.as_ref()
            && saved.blocks.len() != self.flow_defs.len()
        {
            return Err(SimError::Config(
                "checkpoint process-flow block count does not match the model".into(),
            ));
        }
        self.process_flow = checkpoint.process_flow.clone();
        if let (Some(saved), Some(current)) = (checkpoint.fluid.as_ref(), self.fluid.as_ref())
            && saved.objects.len() != current.objects.len()
        {
            return Err(SimError::Config(
                "checkpoint fluid object count does not match the model".into(),
            ));
        }
        self.fluid = checkpoint.fluid.clone();
        // task-183: rebuild explicit-chain state by object name. Missing
        // explicit_routes means an older checkpoint: the chain starts from
        // scratch (documented compatibility policy).
        self.explicit_routes.clear();
        self.explicit_target.clear();
        for (name, saved) in &checkpoint.explicit_routes {
            let Some(&index) = self.by_name.get(name) else {
                continue;
            };
            let mut state = saved.clone();
            if let Some(remaining) = state.remaining {
                state.leg_deadline = Some(checkpoint.now + remaining);
            }
            if let Some(target) = state.current_target {
                self.explicit_target.insert(index, target);
            }
            self.explicit_routes.insert(index, state);
        }
        // A resumed run does not replay the trace of the saved segment.
        self.trace = None;
        Ok(())
    }

    fn handle(&mut self, kind: EventKind) -> Result<(), SimError> {
        match kind {
            EventKind::Arrive(index) => self.handle_arrival(index),
            EventKind::Flow(index) => self.flow(index),
            EventKind::SetupDone(index, token) => self.handle_setup_done(index, token),
            EventKind::ProcessDone(index, token) => self.handle_process_done(index, token),
            EventKind::Breakdown(index, generation) => self.handle_breakdown(index, generation),
            EventKind::RepairDone(index) => self.handle_repair(index),
            EventKind::Message(index) => self.handle_message(index),
            EventKind::MpDone(index, lane, token) => self.handle_mp_done(index, lane, token),
            EventKind::BeltTick(index) => self.handle_belt_tick(index),
            EventKind::BatchTimeout(index) => self.handle_batch_timeout(index),
            EventKind::DwellDone(index) => self.handle_dwell_done(index),
            EventKind::EmptierSweep(index) => self.handle_emptier_sweep(index),
            EventKind::TransportSettle(transporter) => self.finish_task(transporter),
            EventKind::RackPick(index) => self.flow(index),
            EventKind::PreemptRetry(dispatcher, requester) => {
                self.handle_preempt_retry(dispatcher, requester)
            }
            EventKind::TaskStepDone(transporter) => self.handle_task_step_done(transporter),
            EventKind::PersonStep(transporter) => self.handle_person_step(transporter),
            EventKind::ChargeTravel(transporter) => self.handle_charge_travel(transporter),
            EventKind::ShiftChange(index) => self.handle_shift_change(index),
            EventKind::TimetableChange(index) => self.handle_timetable_change(index),
            EventKind::FlowCreate(block) => self.handle_flow_create(block),
            EventKind::FlowAdvance(block) => self.handle_flow_advance(block),
            EventKind::FlowDelayDone(block, token) => self.handle_flow_delay_done(block, token),
            EventKind::FluidStep => self.handle_fluid_step(),
            EventKind::TrackedSample => self.handle_tracked_sample(),
            EventKind::TransportDepart(transporter, requester) => {
                self.handle_transport_depart(transporter, requester)
            }
            EventKind::ExplicitLegDone(transporter) => self.handle_explicit_leg_done(transporter),
            EventKind::ExplicitTrafficRetry(transporter) => {
                self.handle_explicit_traffic_retry(transporter)
            }
            EventKind::ChargeDone(executor) => self.handle_charge_done(executor),
            EventKind::TransportResume(executor, requester, stage) => {
                self.handle_transport_resume(executor, requester, stage)
            }
            EventKind::TrafficWait(transporter, requester, stage) => {
                if self.objects[transporter].traffic_wait_stage != Some(stage) {
                    // A directed wake-up already handled this waiter.
                    Ok(())
                } else {
                    match stage {
                        1 => self.handle_transport_pickup(transporter, requester),
                        2 => self.handle_operator_released(transporter, requester),
                        _ => self.schedule_transport_pickup(transporter, requester),
                    }
                }
            }
            EventKind::TransportPickup(transporter, requester) => {
                self.handle_transport_pickup(transporter, requester)
            }
            EventKind::TransportArrive(transporter, requester) => {
                self.handle_transport_arrive(transporter, requester)
            }
            EventKind::OperatorLoaded(transporter, requester) => {
                self.handle_operator_loaded(transporter, requester)
            }
            EventKind::OperatorReleased(transporter, requester) => {
                self.handle_operator_released(transporter, requester)
            }
            EventKind::TransportHome(transporter) => self.handle_transport_home(transporter),
            EventKind::ResourceRetry(resource) => self.handle_resource_retry(resource),
            EventKind::Pull(index) => self.handle_pull(index),
            EventKind::ResetStats => {
                self.reset_stats();
                Ok(())
            }
        }
    }

    /// B12: append one processed event to the trace, when enabled.
    fn record_trace(&mut self, time: f64, seq: u64, kind: &EventKind) {
        if self.trace.is_none()
            || matches!(
                kind,
                EventKind::FlowCreate(_)
                    | EventKind::FlowAdvance(_)
                    | EventKind::FlowDelayDone(_, _)
                    | EventKind::ExplicitLegDone(_)
            )
        {
            return;
        }
        let (index, name, detail) = describe_event(kind);
        let object = match index {
            Some(index) => self.objects[index].name.clone(),
            None => "sim".to_string(),
        };
        let item_id = self.event_item_id(kind);
        if let EventKind::Flow(index) = kind {
            self.flow_item_hints.remove(index);
        }
        if let EventKind::TransportHome(transporter) = kind {
            self.transport_item_hints.remove(transporter);
        }
        if let Some(log) = self.trace.as_mut() {
            let mut event = TraceEvent::new(time, seq, name, object);
            if let Some((key, value)) = detail {
                event = event.with(key, value);
            }
            if let Some(item) = item_id {
                event = event.with("item", item as f64);
            }
            log.push(event);
        }
    }

    /// task-171: the single item currently associated with an object, if the
    /// association is unambiguous. Never guesses between several items.
    fn sole_item_id(&self, index: usize) -> Option<u64> {
        let object = &self.objects[index];
        let mut ids = Vec::new();
        if let Some(item) = object.processing_item.as_ref() {
            ids.push(item.id);
        }
        ids.extend(object.content.iter().map(|item| item.id));
        ids.extend(object.out_pending.iter().map(|item| item.id));
        ids.sort_unstable();
        ids.dedup();
        if ids.len() == 1 { Some(ids[0]) } else { None }
    }

    /// task-171: item id for an event kind when it is unambiguous at record time.
    fn event_item_id(&self, kind: &EventKind) -> Option<u64> {
        match kind {
            // `create_item` assigns the next id before this event is handled.
            EventKind::Arrive(_) => Some(self.items_created + 1),
            EventKind::SetupDone(index, _) | EventKind::ProcessDone(index, _) => self
                .objects
                .get(*index)
                .and_then(|object| object.processing_item.as_ref())
                .map(|item| item.id),
            EventKind::Flow(index) => self
                .flow_item_hints
                .get(index)
                .copied()
                .or_else(|| self.sole_item_id(*index)),
            EventKind::TransportPickup(transporter, requester)
            | EventKind::TransportArrive(transporter, requester)
            | EventKind::TransportDepart(transporter, requester) => self
                .transport_item_hints
                .get(transporter)
                .copied()
                .or_else(|| self.sole_item_id(*requester)),
            EventKind::TransportHome(transporter) => self
                .transport_item_hints
                .get(transporter)
                .copied()
                .or_else(|| {
                    self.objects[*transporter]
                        .serving_requester
                        .and_then(|requester| self.sole_item_id(requester))
                }),
            _ => None,
        }
    }

    /// task-171: append a real object-boundary event carrying the item id.
    fn record_item_trace(&mut self, label: &str, object: usize, item: u64) {
        if self.trace.is_none() {
            return;
        }
        let time = self.now;
        let seq = self.next_seq;
        self.next_seq += 1;
        let name = self.objects[object].name.clone();
        if let Some(log) = self.trace.as_mut() {
            log.push(TraceEvent::new(time, seq, label, name).with("item", item as f64));
        }
    }

    fn kind(&self, index: usize) -> ObjectKind {
        self.objects[index].kind
    }

    /// Read one cell of a model-level global table (read-only).
    fn table_cell(&self, name: &str, row: usize, column: &str) -> Option<Value> {
        let cell = self.model.table(name)?.cell(row, column)?;
        Some(match cell {
            TableCell::Number(value) => Value::Number(*value),
            TableCell::Bool(value) => Value::Bool(*value),
            TableCell::Str(value) => Value::Str(value.clone()),
        })
    }

    fn set_state(&mut self, index: usize, state: State) {
        let changed = self.objects[index].state != state;
        self.objects[index].set_state(self.now, state);
        self.reschedule_failure(index);
        if changed {
            self.fire_lifecycle_trigger(index, SimTrigger::OnStateChange);
        }
    }

    fn set_phase(&mut self, index: usize, phase: Phase) {
        let now = self.now;
        self.objects[index].set_phase(now, phase);
        self.reschedule_failure(index);
    }

    /// task-166: switch the executer sub-state (no-op for other object kinds).
    fn set_executor_activity(&mut self, index: usize, activity: ExecutorActivity) {
        let now = self.now;
        self.objects[index].set_executor_activity(now, activity);
    }

    /// task-166: stop the sub-state clock when a blocked/idle interval starts.
    fn clear_executor_activity(&mut self, index: usize) {
        self.set_executor_activity(index, ExecutorActivity::None);
    }

    /// Remove one item from an object's content and fire `OnContentChange`.
    fn take_content(&mut self, index: usize, position: usize) -> Option<FlowItem> {
        let now = self.now;
        let item = self.objects[index].take_content(now, position)?;
        self.fire_lifecycle_trigger(index, SimTrigger::OnContentChange);
        Some(item)
    }

    /// Fire an `OnStateChange` / `OnContentChange` / `OnReset` trigger.
    ///
    /// These triggers fire very frequently, so they run with a synthetic item
    /// (the actions can still use `object.<Name>.*` and `time`) and are
    /// suppressed while another lifecycle trigger runs. Errors are parked in
    /// `trigger_error` and reported at the next event boundary, which keeps
    /// `set_state` infallible.
    fn fire_lifecycle_trigger(&mut self, index: usize, trigger: SimTrigger) {
        if self.trigger_depth > 0 || self.trigger_error.is_some() {
            return;
        }
        let actions = self.trigger_actions(index, trigger);
        if actions.is_empty() {
            return;
        }
        self.trigger_depth += 1;
        self.action_location = format!("{}.{}", self.objects[index].name, trigger.as_str());
        let mut probe = FlowItem::probe(self.now);
        let result = self.apply_actions(index, &mut probe, actions);
        self.trigger_depth -= 1;
        if let Err(err) = result {
            self.trigger_error = Some(err);
        }
    }

    /// task-152: execute model-level triggers (on_start / on_reset / on_stop).
    /// Actions run with a synthetic item; scripts may still send messages,
    /// create/move items and log. The first failure is parked in
    /// `trigger_error`, exactly like object lifecycle triggers.
    fn fire_model_actions(&mut self, location: &str, actions: Vec<Action>) {
        for action in actions {
            if let Err(error) = self.apply_model_action(location, action) {
                self.trigger_error = Some(error);
                break;
            }
        }
    }

    fn apply_model_action(&mut self, location: &str, action: Action) -> Result<(), SimError> {
        let mut probe = FlowItem::probe(self.now);
        match action {
            Action::SetLabel { value, .. } | Action::SetItemType { value } => {
                // Model triggers have no flow item; the expression is still
                // evaluated so bad references surface as errors.
                let _ = self.eval_expr(location, &probe, &value)?;
            }
            Action::IncrementLabel { by, .. } => {
                let _ = self.eval_expr(location, &probe, &by)?;
            }
            Action::SendMessage { target, delay } => {
                let delay = self.eval_expr(location, &probe, &delay)?;
                let delay = delay.as_number().unwrap_or(0.0).max(0.0);
                if let Some(&target_index) = self.by_name.get(&target) {
                    self.schedule(self.now + delay, EventKind::Message(target_index));
                }
            }
            Action::DestroyItem | Action::ReleaseItem => {
                // No current item/object at model level: nothing to release.
            }
            Action::SetMotor { target, running } => {
                let Some(&motor) = self.by_name.get(&target) else {
                    return Err(SimError::Config(format!(
                        "set_motor target '{target}' is not an object"
                    )));
                };
                self.set_motor_running(motor, running)?;
            }
            Action::SetDestination { .. } => {
                return Err(SimError::Config(
                    "set_destination is only valid for an item-level trigger".into(),
                ));
            }
            Action::Script { source } => {
                self.action_location = location.to_string();
                self.run_script_action(
                    &source,
                    &format!("{location} script action"),
                    "model",
                    &mut probe,
                    None,
                )?;
            }
        }
        Ok(())
    }

    /// task-152: run the model's `on_start` triggers before the first event.
    fn start(&mut self) -> Result<(), SimError> {
        let actions = self.model.model_triggers.on_start.clone();
        self.fire_model_actions("model_triggers.on_start", actions);
        if let Some(error) = self.trigger_error.take() {
            return Err(error);
        }
        Ok(())
    }

    /// Enter the blocked state with a statistics reason (A12).
    fn block(&mut self, index: usize, reason: BlockReason) {
        let now = self.now;
        self.objects[index].block(now, reason);
        self.reschedule_failure(index);
    }

    /// Push a downtime request (A9). The object goes down immediately and only
    /// recovers when the stack is empty again.
    fn add_downtime(&mut self, index: usize, source: DowntimeSource) {
        if !self.objects[index].downtime.contains(&source) {
            self.objects[index].downtime.push(source);
        }
        self.pause_phase(index);
        self.set_state(index, State::Down);
    }

    /// Remove one downtime request from the stack.
    fn clear_downtime(&mut self, index: usize, source: DowntimeSource) {
        self.objects[index]
            .downtime
            .retain(|active| *active != source);
    }

    /// Recompute the state after one downtime request cleared: the object only
    /// recovers when no request is left, otherwise it stays down (A9).
    fn settle_downtime(&mut self, index: usize) -> Result<(), SimError> {
        let still_down = self.objects[index]
            .downtime
            .iter()
            .any(|source| *source != DowntimeSource::Shift);
        if still_down {
            self.set_state(index, State::Down);
            return Ok(());
        }
        if !self.objects[index].on_shift {
            self.set_state(index, State::OffShift);
            return Ok(());
        }
        if self.resume_phase(index)? {
            return Ok(());
        }
        self.set_state(index, State::Idle);
        self.flow(index)?;
        Ok(())
    }

    /// Sample the next failure and arm the failure clock (A8).
    ///
    /// `first_failure_time` replaces the first MTBF draw; a negative value is
    /// ignored, which is what FlexSim documents.
    fn schedule_next_failure(&mut self, index: usize, first_failure_time: Option<f64>) {
        let Some(failure) = self
            .processor_params(index)
            .and_then(|params| params.failure)
        else {
            return;
        };
        let threshold = match first_failure_time {
            Some(time) if time >= 0.0 => time,
            _ => self.sample(&failure.mtbf).max(EPS),
        };
        let generation = self.objects[index]
            .failure
            .as_ref()
            .map(|clock| clock.generation)
            .unwrap_or(0);
        let count_states = failure.count_time_in_states.clone();
        self.objects[index].failure = Some(FailureRt {
            count_states: count_states.clone(),
            up_time: 0.0,
            threshold,
            generation,
        });
        if count_states.is_empty() {
            self.schedule(
                self.now + threshold.max(EPS),
                EventKind::Breakdown(index, generation),
            );
            return;
        }
        self.reschedule_failure(index);
    }

    /// Arm a failure event when the object currently accumulates up time (A8).
    fn reschedule_failure(&mut self, index: usize) {
        let Some(clock) = self.objects[index].failure.clone() else {
            return;
        };
        if clock.count_states.is_empty() {
            return;
        }
        if !ObjectRt::counts_up_time(
            &clock,
            self.objects[index].state,
            self.objects[index].phase,
            self.objects[index].block_reason,
        ) {
            return;
        }
        let remaining = (clock.threshold - clock.up_time).max(0.0);
        self.schedule(
            self.now + remaining,
            EventKind::Breakdown(index, clock.generation),
        );
    }

    // -----------------------------------------------------------------------
    // Sources
    // -----------------------------------------------------------------------

    fn handle_arrival(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Source(params) => params.clone(),
            _ => return Ok(()),
        };
        if let Some(max) = params.max_arrivals
            && self.objects[index].arrivals >= max
        {
            return Ok(());
        }
        if let Some(stop) = params.stop_time
            && self.now >= stop - EPS
        {
            return Ok(());
        }
        if self.now >= self.config.stop_time - EPS {
            return Ok(());
        }
        let style = params.effective_arrival_style();
        if !self.objects[index].on_shift {
            return self.handle_off_shift_arrival(index, &params, style);
        }
        match style {
            ArrivalStyle::InterArrivalTime => self.handle_interarrival(index, &params),
            ArrivalStyle::ArrivalSchedule => self.handle_schedule_arrival(index, &params),
            ArrivalStyle::ArrivalSequence => self.handle_sequence_arrival(index, &params),
        }
    }

    /// The model time at which a source stops creating items: its own
    /// `stop_time` and the run's stop time, whichever comes first.
    fn source_horizon(&self, params: &openflex_model::SourceParams) -> f64 {
        // Only the source's own stop time bounds the arrival table; the run's
        // stop time does not, so a checkpoint keeps the future arrivals that a
        // resumed run needs.
        params.stop_time.unwrap_or(f64::INFINITY)
    }

    /// Schedule the very first arrival according to the source's arrival style
    /// (inter-arrival, absolute schedule or table sequence).
    fn schedule_first_arrival(
        &mut self,
        index: usize,
        params: &openflex_model::SourceParams,
    ) -> Result<(), SimError> {
        match params.effective_arrival_style() {
            ArrivalStyle::InterArrivalTime => {
                // `first_arrival = 0` means "an item exists at t = 0".
                let first = params.first_arrival.max(0.0);
                if first < self.source_horizon(params) - EPS {
                    self.schedule(first, EventKind::Arrive(index));
                }
            }
            ArrivalStyle::ArrivalSchedule => {
                self.objects[index].arrival_row = 0;
                self.objects[index].arrival_base = 0.0;
                self.objects[index].arrival_pass = 0;
                self.schedule_next_schedule_row(index, params)?;
            }
            ArrivalStyle::ArrivalSequence => {
                // A sequence starts as soon as the run starts.
                if params
                    .arrival_sequence()
                    .is_some_and(|rows| !rows.is_empty())
                {
                    self.schedule(self.now, EventKind::Arrive(index));
                }
            }
        }
        Ok(())
    }

    /// Inter-arrival-time sources: create `items_per_arrival` items and let
    /// `drain_out_pending` schedule the next arrival once the batch has left.
    fn handle_interarrival(
        &mut self,
        index: usize,
        params: &openflex_model::SourceParams,
    ) -> Result<(), SimError> {
        self.objects[index].arrivals += 1;
        for _ in 0..params.items_per_arrival {
            let item = self.create_item(params.item_type, &params.labels, index)?;
            self.objects[index].out_pending.push(item);
        }
        // The next inter-arrival time is measured from the moment the batch
        // leaves the source (FlexSim inter-arrivaltime semantics), so it is
        // scheduled by `drain_out_pending` once the items were accepted.
        self.objects[index].next_arrival_pending = true;
        self.flow(index)
    }

    /// Arrival schedule (A2): one row is created per absolute arrival time.
    fn handle_schedule_arrival(
        &mut self,
        index: usize,
        params: &openflex_model::SourceParams,
    ) -> Result<(), SimError> {
        let Some(schedule) = params.arrival_schedule() else {
            return Ok(());
        };
        let Some(row) = schedule.rows.get(self.objects[index].arrival_row) else {
            return Ok(());
        };
        self.objects[index].arrivals += 1;
        self.create_row_items(index, params, row.quantity, row.item_type, &row.labels)?;
        self.objects[index].arrival_row += 1;
        self.schedule_next_schedule_row(index, params)?;
        self.flow(index)
    }

    /// Schedule the next row of an arrival schedule, handling `repeat`.
    ///
    /// The first row's time only applies to the first pass: when the table
    /// repeats, the next pass starts at the last row's time and later passes
    /// use the row times as offsets from that pass start.
    fn schedule_next_schedule_row(
        &mut self,
        index: usize,
        params: &openflex_model::SourceParams,
    ) -> Result<(), SimError> {
        let Some(schedule) = params.arrival_schedule() else {
            return Ok(());
        };
        if schedule.rows.is_empty() {
            return Ok(());
        }
        if self.objects[index].arrival_row >= schedule.rows.len() {
            if !schedule.repeat {
                return Ok(());
            }
            let last = schedule.rows[schedule.rows.len() - 1].time;
            if last <= EPS {
                return Err(SimError::Config(format!(
                    "object {}: a repeating arrival_schedule needs a positive last row time",
                    self.objects[index].name
                )));
            }
            self.objects[index].arrival_base += last;
            self.objects[index].arrival_pass += 1;
            self.objects[index].arrival_row = 0;
        }
        if let Some(max) = params.max_arrivals
            && self.objects[index].arrivals >= max
        {
            return Ok(());
        }
        let row = self.objects[index].arrival_row;
        let first_pass = self.objects[index].arrival_pass == 0;
        let offset = if first_pass || row > 0 {
            schedule.rows[row].time
        } else {
            0.0
        };
        let at = self.objects[index].arrival_base + offset;
        if at < self.source_horizon(params) - EPS {
            self.schedule(at, EventKind::Arrive(index));
        }
        Ok(())
    }

    /// Arrival sequence (A2): rows advance once the previous batch has left
    /// the source, so no time is scheduled here.
    fn handle_sequence_arrival(
        &mut self,
        index: usize,
        params: &openflex_model::SourceParams,
    ) -> Result<(), SimError> {
        let Some(rows) = params.arrival_sequence() else {
            return Ok(());
        };
        let Some(row) = rows.get(self.objects[index].arrival_row) else {
            return Ok(());
        };
        self.objects[index].arrivals += 1;
        self.create_row_items(index, params, row.quantity, row.item_type, &row.labels)?;
        self.objects[index].arrival_row += 1;
        self.objects[index].next_arrival_pending = true;
        self.flow(index)
    }

    /// Create the items of one table row with the row's item type and labels
    /// layered on top of the source defaults.
    fn create_row_items(
        &mut self,
        index: usize,
        params: &openflex_model::SourceParams,
        quantity: u32,
        item_type: Option<u32>,
        labels: &BTreeMap<String, f64>,
    ) -> Result<(), SimError> {
        let mut merged = params.labels.clone();
        merged.extend(labels.iter().map(|(name, value)| (name.clone(), *value)));
        let item_type = item_type.unwrap_or(params.item_type);
        for _ in 0..quantity {
            let item = self.create_item(item_type, &merged, index)?;
            self.objects[index].out_pending.push(item);
        }
        Ok(())
    }

    /// A source that is off shift creates nothing. Inter-arrival sources keep
    /// their rhythm; schedules skip the row; sequences resume at the next
    /// shift window.
    fn handle_off_shift_arrival(
        &mut self,
        index: usize,
        params: &openflex_model::SourceParams,
        style: ArrivalStyle,
    ) -> Result<(), SimError> {
        match style {
            ArrivalStyle::InterArrivalTime => self.schedule_next_arrival(index),
            ArrivalStyle::ArrivalSchedule => {
                if self.objects[index].arrival_row
                    < params.arrival_schedule().map_or(0, |s| s.rows.len())
                {
                    self.objects[index].arrival_row += 1;
                }
                self.schedule_next_schedule_row(index, params)
            }
            ArrivalStyle::ArrivalSequence => {
                if let Some(at) = self.next_shift_start(index)
                    && at < self.source_horizon(params) - EPS
                {
                    self.schedule(at, EventKind::Arrive(index));
                }
                Ok(())
            }
        }
    }

    /// First model time after now at which the object's shift plan makes it
    /// available again.
    fn next_shift_start(&self, index: usize) -> Option<f64> {
        let shifts = self.model.objects[index].params.shifts();
        if shifts.is_empty() {
            return None;
        }
        let mut cursor = self.now;
        for _ in 0..16 {
            let next = shifts::next_transition(shifts, cursor)?;
            if shifts::is_active(shifts, next + EPS) {
                return Some(next);
            }
            cursor = next;
        }
        None
    }

    /// Schedule the next arrival one inter-arrival time from now (inter-arrival
    /// style) or the next sequence row (no delay).
    fn schedule_next_arrival(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Source(params) => params.clone(),
            _ => return Ok(()),
        };
        if let Some(max) = params.max_arrivals
            && self.objects[index].arrivals >= max
        {
            return Ok(());
        }
        if let Some(stop) = params.stop_time
            && self.now >= stop - EPS
        {
            return Ok(());
        }
        // Arrival sequence: the next row starts as soon as the previous batch
        // has left the source (the event carries no delay).
        if let Some(rows) = params.arrival_sequence() {
            if self.objects[index].arrival_row < rows.len() {
                self.schedule(self.now, EventKind::Arrive(index));
            }
            return Ok(());
        }
        let interval = self.sample_arrival(&params.interarrival).max(0.0);
        // A safety net for zero-time loops: a source that samples a zero
        // inter-arrival time without an arrival cap would create items at the
        // same instant forever and never advance the clock.
        if interval <= EPS && params.max_arrivals.is_none() {
            return Err(SimError::Config(format!(
                "object {}: interarrival samples must be positive (got {}); set max_arrivals, \
                 stop_time or a positive interarrival distribution",
                self.objects[index].name, interval
            )));
        }
        let next = self.now + interval;
        if next < self.source_horizon(&params) - EPS {
            self.schedule(next, EventKind::Arrive(index));
        }
        Ok(())
    }

    fn create_item(
        &mut self,
        item_type: u32,
        labels: &BTreeMap<String, f64>,
        source: usize,
    ) -> Result<FlowItem, SimError> {
        self.items_created += 1;
        self.stats_created += 1;
        let mut item = FlowItem {
            id: self.items_created,
            item_type,
            labels: labels.clone(),
            created: self.now,
            entry_time: self.now,
            destination: None,
        };
        self.record_item_trace("enter", source, item.id);
        self.flow_item_hints.insert(source, item.id);
        let actions = self.trigger_actions(source, SimTrigger::OnCreate);
        if !actions.is_empty() {
            self.action_location = format!("{}.on_create", self.objects[source].name);
            self.apply_actions(source, &mut item, actions)?;
        }
        Ok(item)
    }

    /// Create one of the parts produced by a separator, derived from its
    /// container item.
    fn derive_item(&mut self, container: &FlowItem, part: usize) -> FlowItem {
        self.items_created += 1;
        self.stats_created += 1;
        let mut item = container.clone();
        item.id = self.items_created;
        item.destination = None;
        item.labels.insert("part".to_string(), part as f64);
        item
    }

    // -----------------------------------------------------------------------
    // Connections, routing and delivery
    // -----------------------------------------------------------------------

    /// Connection index for the given output port of an object.
    fn conn_at_port(&self, from: usize, port: u32) -> Option<usize> {
        let outputs = &self.outputs[from];
        if let Some(conn) = outputs
            .iter()
            .copied()
            .find(|&conn| self.conns[conn].declared_port == Some(port))
        {
            return Some(conn);
        }
        outputs.get(port as usize).copied()
    }

    fn conn_accepts(&self, conn: usize) -> bool {
        let target = &self.conns[conn];
        self.can_accept(target.to, target.to_port)
    }

    /// Capacity / staging check, aware of the input port the item would use.
    /// task-184: whether any controlling motor is stopped.
    fn conveyor_stopped(&self, index: usize) -> bool {
        self.conveyor_motors.get(index).is_some_and(|motors| {
            motors
                .iter()
                .any(|motor| !self.objects[*motor].motor_running)
        })
    }

    /// task-184: power/stop the conveyors controlled by a motor.
    fn set_motor_running(&mut self, motor: usize, running: bool) -> Result<(), SimError> {
        if self.objects[motor].kind != ObjectKind::Motor {
            return Err(SimError::Config(format!(
                "set_motor target '{}' is not a motor",
                self.objects[motor].name
            )));
        }
        self.objects[motor].motor_running = running;
        let conveyors = self.motor_conveyors.get(motor).cloned().unwrap_or_default();
        for conveyor in conveyors {
            if running {
                self.objects[conveyor].belt_last_update = self.now;
                let busy = self.objects[conveyor]
                    .belt
                    .as_ref()
                    .is_some_and(|belt| !belt.is_empty());
                self.set_state(conveyor, if busy { State::Busy } else { State::Idle });
                self.notify_upstreams(conveyor)?;
                self.schedule(self.now, EventKind::Flow(conveyor));
            } else {
                self.sync_belt(conveyor);
                self.objects[conveyor].belt_last_update = self.now;
                self.set_state(conveyor, State::OffShift);
                self.objects[conveyor].belt_tick_at = f64::INFINITY;
            }
        }
        Ok(())
    }

    fn can_accept(&self, index: usize, port: u32) -> bool {
        if !self.objects[index].on_shift {
            return false;
        }
        if self.conveyor_stopped(index) {
            return false;
        }
        let object = &self.objects[index];
        match &self.model.objects[index].params {
            ObjectParams::Source(_) => false,
            ObjectParams::Sink(_) => true,
            ObjectParams::Queue(params) => (object.effective_content() as u32) < params.max_content,
            ObjectParams::Rack(params) => {
                (object.effective_content() as u32) < rack_capacity(params)
            }
            ObjectParams::Processor(params) => {
                (object.effective_content() as u32) < params.max_content
            }
            ObjectParams::MultiProcessor(params) => {
                (object.effective_content() as u32) < params.slots.max(1)
            }
            ObjectParams::Conveyor(params) | ObjectParams::Escalator(params) => object
                .belt
                .as_ref()
                .is_some_and(|belt| belt.entry_free(params.max_content)),
            ObjectParams::Combiner(params) => match object.combine_mode {
                CombinerMode::Pack | CombinerMode::Batch => {
                    if port == 0 {
                        object.container.is_none() && object.components.is_empty()
                    } else {
                        object.container.is_some()
                            && (object.components.len() as u32) < params.components_per_container
                    }
                }
                CombinerMode::Join => object
                    .join_slots
                    .get(port as usize)
                    .is_some_and(Option::is_none),
            },
            ObjectParams::Separator(_) => {
                object.content.is_empty() && object.pending_parts.is_empty()
            }
            ObjectParams::Merge(_) => object
                .merge_slots
                .get(port as usize)
                .is_some_and(Option::is_none),
            ObjectParams::Split(_) => true,
            ObjectParams::Transporter(_)
            | ObjectParams::Person(_)
            | ObjectParams::Agv(_)
            | ObjectParams::Elevator(_)
            | ObjectParams::Robot(_)
            | ObjectParams::Crane(_)
            | ObjectParams::AsrsVehicle(_)
            | ObjectParams::Dispatcher(_)
            | ObjectParams::Resource(_) => false,
            ObjectParams::BasicFr(params) => {
                // task-36: an Emptier never accepts pushed items; it pulls
                // them out of its upstream objects on its own schedule.
                params.emptier.is_none() && object.content.is_empty()
            }
            ObjectParams::BasicTe(params) => (object.effective_content() as u32) < params.capacity,
            ObjectParams::FluidGenerator(_)
            | ObjectParams::FluidTank(_)
            | ObjectParams::FluidValve(_)
            | ObjectParams::FluidSink(_)
            | ObjectParams::FluidPipe(_)
            | ObjectParams::Ticker(_)
            | ObjectParams::FluidMixer(_)
            | ObjectParams::FluidBlender(_)
            | ObjectParams::FluidSplitter(_)
            | ObjectParams::FluidProcessor(_)
            | ObjectParams::NetworkNode(_)
            | ObjectParams::TrafficControl(_)
            | ObjectParams::VisualTool(_)
            | ObjectParams::Motor(_) => false,
            // The two bridges accept discrete flow items one at a time.
            ObjectParams::ItemToFluid(_) | ObjectParams::FluidToItem(_) => {
                object.effective_content() == 0
            }
        }
    }

    /// Try to push everything an object can currently send.
    fn flow(&mut self, index: usize) -> Result<(), SimError> {
        let kind = self.objects[index].kind;
        if !self.objects[index].on_shift {
            return Ok(());
        }
        // A3: a downstream object that pulls is never pushed into, so flow has
        // to tell it that items are waiting here.
        self.notify_pullers(index);
        if !self.drain_out_pending(index)? {
            return Ok(());
        }
        match kind {
            ObjectKind::Queue | ObjectKind::Rack => self.flow_queue(index)?,
            ObjectKind::Processor => self.flow_processor(index)?,
            ObjectKind::MultiProcessor => self.flow_multiprocessor(index)?,
            ObjectKind::Conveyor | ObjectKind::Escalator => self.flow_conveyor(index)?,
            ObjectKind::ItemToFluid => self.flow_item_to_fluid(index)?,
            ObjectKind::FluidToItem => self.flow_fluid_to_item(index)?,
            ObjectKind::Combiner => self.flow_combiner(index)?,
            ObjectKind::Separator => self.flow_separator(index)?,
            ObjectKind::Merge => self.flow_merge(index)?,
            ObjectKind::Split => self.flow_split(index)?,
            ObjectKind::Source
            | ObjectKind::Sink
            | ObjectKind::Transporter
            | ObjectKind::Person
            | ObjectKind::Agv
            | ObjectKind::Elevator
            | ObjectKind::Dispatcher
            | ObjectKind::Resource
            | ObjectKind::FluidGenerator
            | ObjectKind::FluidTank
            | ObjectKind::FluidValve
            | ObjectKind::FluidSink => {}
            _ => self.flow_generic(index)?,
        }
        // Some handlers (combiner, merge, separator) produce output into
        // `out_pending`; deliver it in the same step.
        self.drain_out_pending(index)?;
        // A3: an object that pulls asks its upstream for more work whenever it
        // may have room again.
        self.schedule_pull(index);
        Ok(())
    }

    /// Deliver `out_pending` items. Returns `false` when the object blocked.
    fn drain_out_pending(&mut self, index: usize) -> Result<bool, SimError> {
        // A4: `send_to_port == -1` kept the item here; only `release_item`
        // clears the hold, so this is not a capacity block.
        if self.objects[index].hold_release && !self.objects[index].release_bypass {
            self.set_state(index, State::Blocked);
            return Ok(false);
        }
        while let Some(item) = self.objects[index].out_pending.first().cloned() {
            let Some(conn) = self.route(index, &item)? else {
                self.set_state(index, State::Blocked);
                self.objects[index].phase = Phase::WaitingForOutput;
                return Ok(false);
            };
            if !self.deliver(index, conn, item.clone())? {
                self.set_state(index, State::Blocked);
                self.objects[index].phase = Phase::WaitingForOutput;
                return Ok(false);
            }
            self.objects[index].out_pending.remove(0);
            if !self.objects[index].out_pending_accounted {
                self.objects[index].acc.output += 1;
                self.objects[index].acc.stay_sum += self.now - item.entry_time;
                self.objects[index].acc.stay_count += 1;
            }
        }
        self.objects[index].out_pending_accounted = false;
        // `hold_release` / `release_bypass` are owned by `route` (which consumes
        // the bypass) and by `release_item`; nothing to reset here.
        // The source schedules its next arrival when the previous batch has
        // left the object.
        if self.objects[index].kind == ObjectKind::Source
            && self.objects[index].next_arrival_pending
        {
            self.objects[index].next_arrival_pending = false;
            self.schedule_next_arrival(index)?;
        }
        Ok(true)
    }

    fn route(&mut self, from: usize, item: &FlowItem) -> Result<Option<usize>, SimError> {
        // A3: a downstream object that pulls its input is never pushed into,
        // otherwise the item would be delivered twice. S (resource) links are
        // additionally excluded defensively: they must never carry a flow item.
        let outputs: Vec<usize> = self.outputs[from]
            .iter()
            .copied()
            .filter(|&conn| {
                self.conns[conn].kind == ConnectionKind::A
                    && !self.pull_enabled(self.conns[conn].to)
            })
            .collect();
        if let Some(port) = item.destination {
            // task-187: explicit destination wins over the routing strategy.
            let Some(conn) = self.conn_at_port(from, port) else {
                return Err(SimError::Config(format!(
                    "set_destination: object '{}' has no output port {} (it has {} output ports)",
                    self.objects[from].name,
                    port,
                    self.outputs[from].len()
                )));
            };
            if !outputs.contains(&conn) {
                return Err(SimError::Config(format!(
                    "set_destination: output port {} on object '{}' is not a sendable A connection",
                    port, self.objects[from].name
                )));
            }
            // A chosen but busy destination keeps the item here for a retry.
            return Ok(self.conn_accepts(conn).then_some(conn));
        }
        if outputs.is_empty() {
            return Ok(None);
        }
        let strategy = outputs
            .iter()
            .find_map(|&conn| self.conns[conn].strategy.clone())
            .unwrap_or(PortStrategy::FirstAvailable);
        let chosen = match strategy {
            PortStrategy::FirstAvailable => outputs
                .iter()
                .copied()
                .find(|&conn| self.conn_accepts(conn)),
            PortStrategy::RoundRobin => {
                let start = self.objects[from].rr % outputs.len();
                let found = (0..outputs.len())
                    .map(|offset| outputs[(start + offset) % outputs.len()])
                    .find(|&conn| self.conn_accepts(conn));
                self.objects[from].rr = self.objects[from].rr.wrapping_add(1);
                found
            }
            PortStrategy::Random => {
                let available: Vec<usize> = outputs
                    .iter()
                    .copied()
                    .filter(|&conn| self.conn_accepts(conn))
                    .collect();
                if available.is_empty() {
                    None
                } else {
                    Some(available[self.rng.r#gen_range(0..available.len())])
                }
            }
            PortStrategy::ByPercentage { percentages } => {
                let total: f64 = percentages.iter().sum();
                if percentages.len() != outputs.len() || total <= 0.0 {
                    None
                } else {
                    let mut pick = self.rng.r#gen::<f64>() * total;
                    let mut chosen = None;
                    for (conn, weight) in outputs.iter().zip(&percentages) {
                        if pick <= *weight {
                            chosen = Some(*conn);
                            break;
                        }
                        pick -= *weight;
                    }
                    chosen
                }
            }
            PortStrategy::ByCondition { conditions } => {
                let name = self.objects[from].name.clone();
                let mut chosen = None;
                for condition in &conditions {
                    let matched = if let Some(script) = &condition.script_condition {
                        self.eval_script_condition(
                            script,
                            &format!("{name} routing condition"),
                            Some(item),
                        )?
                    } else {
                        let context = ExprContext::new(self, Some(item));
                        condition
                            .condition
                            .eval(&context)
                            .map_err(|err| SimError::Expression {
                                location: format!("{name} routing condition"),
                                detail: err.to_string(),
                            })?
                            .as_bool()
                    };
                    if matched {
                        chosen = outputs.get(condition.port as usize).copied();
                        break;
                    }
                }
                chosen
            }
            PortStrategy::ByItemType => outputs.get(item.item_type as usize).copied(),
            PortStrategy::ByLabel { label } => {
                let port = item.labels.get(&label).copied().unwrap_or(0.0).max(0.0) as usize;
                outputs.get(port).copied()
            }
            // A4: `send_to_port == -1`. The item is not offered to any output;
            // it stays with the object until an explicit `release_item`.
            PortStrategy::Hold => {
                if self.objects[from].release_bypass {
                    // A4: `release_item` re-runs the send with the hold
                    // lifted, i.e. it falls back to the first free output.
                    // The bypass is single-shot: after this attempt the object
                    // holds again.
                    self.objects[from].release_bypass = false;
                    self.objects[from].hold_release = false;
                    outputs
                        .iter()
                        .copied()
                        .find(|&conn| self.conn_accepts(conn))
                } else {
                    self.objects[from].hold_release = true;
                    None
                }
            }
        };
        Ok(chosen)
    }

    /// Deliver an item over one connection. Returns `false` when the target
    /// rejects it; the caller keeps the item and retries later.
    fn deliver(&mut self, from: usize, conn: usize, item: FlowItem) -> Result<bool, SimError> {
        let (target, to_port) = {
            let connection = &self.conns[conn];
            (connection.to, connection.to_port)
        };
        if !self.can_accept(target, to_port) {
            return Ok(false);
        }
        // B3 join conveyor: only the port the round-robin cursor prefers may
        // enter; when that upstream has nothing waiting any port is allowed.
        if self.objects[target].join_inputs >= 2
            && let Some(preferred) = self.join_preferred_port(target)
            && preferred != to_port
        {
            return Ok(false);
        }
        let mut item = item;
        // task-187: the destination override is consumed by this routing
        // decision; it must not leak into the receiving object.
        item.destination = None;
        // The item now leaves the upstream object: its OnExit trigger runs
        // before the receiving object's OnEntry/OnReceive logic (A10-on_exit).
        let exit_actions = self.trigger_actions(from, SimTrigger::OnExit);
        if !exit_actions.is_empty() {
            self.action_location = format!("{}.on_exit", self.objects[from].name);
            self.apply_actions(from, &mut item, exit_actions)?;
        }
        self.receive_item_from(target, to_port, item, Some(from))
    }

    /// Receive an item at the target object: entry triggers, capacity already
    /// checked, content/out-pending bookkeeping and the follow-up flow event.
    /// Used by normal connections and by process-flow `transfer` blocks.
    fn receive_item(
        &mut self,
        target: usize,
        to_port: u32,
        item: FlowItem,
    ) -> Result<bool, SimError> {
        self.receive_item_from(target, to_port, item, None)
    }

    fn receive_item_from(
        &mut self,
        target: usize,
        to_port: u32,
        mut item: FlowItem,
        from: Option<usize>,
    ) -> Result<bool, SimError> {
        if !self.can_accept(target, to_port) {
            return Ok(false);
        }
        if let Some(from) = from {
            self.record_item_trace("exit", from, item.id);
        }
        self.record_item_trace("enter", target, item.id);
        self.flow_item_hints.insert(target, item.id);
        let kind = self.objects[target].kind;
        // Trigger the receiving object's OnEntry/OnReceive logic.
        let actions = self.trigger_actions(target, SimTrigger::OnEntry);
        if !actions.is_empty() {
            self.action_location = format!("{}.on_entry", self.objects[target].name);
            self.apply_actions(target, &mut item, actions)?;
        }
        let receive_actions = self.trigger_actions(target, SimTrigger::OnReceive);
        if !receive_actions.is_empty() {
            self.action_location = format!("{}.on_receive", self.objects[target].name);
            self.apply_actions(target, &mut item, receive_actions)?;
        }
        item.entry_time = self.now;
        self.objects[target].acc.input += 1;
        let now = self.now;
        match kind {
            ObjectKind::Sink => {
                // task-188: the item object is dropped as soon as it enters a
                // sink, so the recycle strategy only makes that explicit and
                // fires the batch hook; the statistics below stay untouched.
                let (recycle, batch) = match &self.model.objects[target].params {
                    ObjectParams::Sink(params) => (params.recycle, params.recycle_batch),
                    _ => (RecycleMode::None, 100),
                };
                let recycled = match recycle {
                    RecycleMode::None => false,
                    RecycleMode::OnEntry => true,
                    RecycleMode::ByBatch => {
                        self.objects[target].recycled_count =
                            self.objects[target].recycled_count.saturating_add(1);
                        if self.objects[target].recycled_count >= batch.max(1) {
                            self.objects[target].recycled_count = 0;
                            true
                        } else {
                            false
                        }
                    }
                };
                if recycled {
                    self.record_item_trace("recycle", target, item.id);
                }
                self.objects[target].acc.output += 1;
                if item.created + EPS >= self.config.warmup {
                    self.stats_completed += 1;
                    if self.track_staytime {
                        self.stay_samples.push((now - item.created).max(0.0));
                    }
                }
                self.objects[target].acc.stay_sum += now - item.entry_time;
                self.objects[target].acc.stay_count += 1;
                self.record_item_trace("exit", target, item.id);
                self.trigger_actions_run(target, SimTrigger::OnExit, &mut item)?;
            }
            ObjectKind::Queue | ObjectKind::Rack => {
                // task-44: rstack items carry the level they were stored on so
                // `level_order` / `nearest` can pick by level.
                if self.objects[target].kind == ObjectKind::Rack {
                    let (levels, aisles, bays) = match &self.model.objects[target].params {
                        ObjectParams::Rack(params) => (
                            params.levels.max(1),
                            params.aisles.unwrap_or(1).max(1),
                            params.bays.unwrap_or(1).max(1),
                        ),
                        _ => (1, 1, 1),
                    };
                    // task-47: a concrete (level, aisle, bay) address. The
                    // slot counter keeps rotating even when the rack is empty.
                    let slot = self.objects[target].rack_slots;
                    self.objects[target].rack_slots =
                        self.objects[target].rack_slots.wrapping_add(1);
                    let level = slot % levels;
                    let aisle = (slot / levels) % aisles;
                    let bay = (slot / (levels * aisles)) % bays;
                    item.labels.insert("level".to_string(), f64::from(level));
                    item.labels.insert("aisle".to_string(), f64::from(aisle));
                    item.labels.insert("bay".to_string(), f64::from(bay));
                }
                self.objects[target].push_content(now, item);
                self.set_state(target, State::Busy);
                self.schedule(now, EventKind::Flow(target));
            }
            ObjectKind::Processor => {
                self.objects[target].push_content(now, item);
                if self.objects[target].phase == Phase::Idle
                    && self.objects[target].state != State::Down
                {
                    self.flow_processor(target)?;
                }
            }
            ObjectKind::Conveyor | ObjectKind::Escalator => {
                self.sync_belt(target);
                self.process_belt_stops(target)?;
                self.objects[target].note_content(now);
                if let Some(belt) = self.objects[target].belt.as_mut() {
                    belt.admit();
                }
                self.objects[target].belt_items.push(item);
                let point_count = self.objects[target].decision_point_ranges.len();
                self.objects[target]
                    .decision_handled
                    .push(vec![false; point_count]);
                let stop_count = self.objects[target].belt_stops.len();
                self.objects[target]
                    .belt_handled
                    .push(vec![false; stop_count]);
                self.objects[target].belt_last_update = now;
                self.objects[target].touch();
                if self.objects[target].join_inputs >= 2 {
                    let inputs = self.objects[target].join_inputs as usize;
                    match self.objects[target].merge_mode {
                        MergeMode::Weighted => {
                            let port = to_port as usize;
                            if self.objects[target].merge_counts.len() < inputs {
                                self.objects[target].merge_counts.resize(inputs, 0.0);
                            }
                            if let Some(count) = self.objects[target].merge_counts.get_mut(port) {
                                *count += 1.0;
                            }
                        }
                        MergeMode::RoundRobin | MergeMode::ByItemType => {
                            self.objects[target].join_cursor = (to_port as usize + 1) % inputs;
                        }
                    }
                }
                self.set_state(target, State::Busy);
                self.schedule(now, EventKind::Flow(target));
            }
            ObjectKind::Combiner => {
                self.objects[target].note_content(now);
                match self.objects[target].combine_mode {
                    CombinerMode::Pack | CombinerMode::Batch => {
                        if to_port == 0 {
                            self.objects[target].container = Some(item);
                        } else {
                            self.objects[target].components.push(item);
                        }
                    }
                    CombinerMode::Join => {
                        if let Some(slot) =
                            self.objects[target].join_slots.get_mut(to_port as usize)
                        {
                            *slot = Some(item);
                        }
                    }
                }
                self.objects[target].touch();
                self.schedule(now, EventKind::Flow(target));
            }
            ObjectKind::MultiProcessor => {
                self.objects[target].push_content(now, item);
                self.schedule(now, EventKind::Flow(target));
            }
            ObjectKind::Separator => {
                self.objects[target].push_content(now, item);
                self.schedule(now, EventKind::Flow(target));
            }
            ObjectKind::Merge => {
                self.objects[target].note_content(now);
                if let Some(slot) = self.objects[target].merge_slots.get_mut(to_port as usize) {
                    *slot = Some(item);
                }
                self.objects[target].touch();
                self.set_state(target, State::Busy);
                self.schedule(now, EventKind::Flow(target));
            }
            ObjectKind::Split => {
                self.objects[target].push_content(now, item);
                self.schedule(now, EventKind::Flow(target));
            }
            _ => {
                self.objects[target].push_content(now, item);
                self.schedule(now, EventKind::Flow(target));
            }
        }
        // The object's content grew: fire `on_content_change`. Sinks do not
        // hold content, so they are skipped.
        if self.objects[target].kind != ObjectKind::Sink {
            self.fire_lifecycle_trigger(target, SimTrigger::OnContentChange);
        }
        self.trigger_entry_flow(target)?;
        self.notify_upstreams(target)?;
        Ok(true)
    }

    fn notify_upstreams(&mut self, index: usize) -> Result<(), SimError> {
        let inputs = self.inputs[index].clone();
        for conn in inputs {
            let (from, to_port) = {
                let connection = &self.conns[conn];
                (connection.from, connection.to_port)
            };
            if self.can_accept(index, to_port) {
                self.schedule(self.now, EventKind::Flow(from));
            }
        }
        // A3: items became available here, so pulling neighbours may proceed.
        self.notify_pullers(index);
        // B7: transfer blocks waiting to inject an item into this object.
        let blocks = self
            .flow_transfer_targets
            .get(index)
            .cloned()
            .unwrap_or_default();
        for block in blocks {
            self.flow_schedule_advance(block);
        }
        let blocks = self
            .flow_pull_sources
            .get(index)
            .cloned()
            .unwrap_or_default();
        for block in blocks {
            self.flow_schedule_advance(block);
        }
        Ok(())
    }

    /// Wake every downstream object that pulls its input from `index`.
    fn notify_pullers(&mut self, index: usize) {
        let outputs = self.outputs[index].clone();
        for conn in outputs {
            let to = self.conns[conn].to;
            if self.pull_enabled(to) {
                self.schedule_pull(to);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Pull mode (A3)
    // -----------------------------------------------------------------------

    /// A4: whether the object currently holds an item that `release_item` can
    /// send (a finished item or a buffered queue item).
    fn has_held_item(&self, index: usize) -> bool {
        if !self.objects[index].out_pending.is_empty() {
            return true;
        }
        if self.objects[index].kind == ObjectKind::Conveyor {
            return self.objects[index]
                .belt
                .as_ref()
                .is_some_and(|belt| belt.front_ready());
        }
        matches!(
            self.objects[index].kind,
            ObjectKind::Queue | ObjectKind::Rack
        ) && !self.objects[index].content.is_empty()
    }

    /// task-178: type of the first ready item an upstream object holds.
    fn held_item_type(&self, index: usize) -> Option<u32> {
        let object = &self.objects[index];
        if let Some(item) = object.out_pending.first() {
            return Some(item.item_type);
        }
        if matches!(object.kind, ObjectKind::Conveyor | ObjectKind::Escalator) {
            return object.belt_items.first().map(|item| item.item_type);
        }
        if matches!(object.kind, ObjectKind::Queue | ObjectKind::Rack) {
            return object.content.first().map(|item| item.item_type);
        }
        None
    }

    /// task-179 note: Photo Eyes do not notify the merge controller when a slug
    /// clears. The controller only needs its own successful-admission counts
    /// (`merge_counts`) and the ready/held state of its inputs; adding a Photo
    /// Eye notification would be redundant state, so it is intentionally not
    /// implemented (the FlexSim Merging help describes that coupling, but our
    /// controller owns the decision).
    ///
    /// B3/task-178: the input port a join conveyor should serve next.
    ///
    /// `round_robin` is the original fair cursor; `weighted` picks the ready
    /// port with the lowest served/weight ratio; `by_item_type` picks the ready
    /// port whose next item has the lowest `item_type` (round-robin on ties).
    /// `None` means "no preference" (no upstream is ready, so any port may be
    /// accepted). The main-belt entry check happens before this selection, so a
    /// rejected attempt never consumes a weighted quota.
    fn join_preferred_port(&self, target: usize) -> Option<u32> {
        let inputs = self.objects[target].join_inputs;
        if inputs < 2 {
            return None;
        }
        let ports = inputs as usize;
        let start = self.objects[target].join_cursor % ports;
        let connections = &self.inputs[target];
        let mut ready: Vec<(usize, u32)> = Vec::new();
        for offset in 0..ports {
            let port = (start + offset) % ports;
            let Some(&conn) = connections.get(port) else {
                continue;
            };
            let from = self.conns[conn].from;
            if self.has_held_item(from) {
                ready.push((port, self.held_item_type(from).unwrap_or(u32::MAX)));
            }
        }
        match self.objects[target].merge_mode {
            MergeMode::RoundRobin => ready.first().map(|(port, _)| *port as u32),
            MergeMode::ByItemType => ready
                .into_iter()
                .min_by_key(|(port, item_type)| (*item_type, ready_position(*port, start, ports)))
                .map(|(port, _)| port as u32),
            MergeMode::Weighted => {
                let weights = &self.objects[target].merge_weight;
                ready
                    .into_iter()
                    .min_by(|(left, _), (right, _)| {
                        let score = |port: usize| {
                            let weight = weights
                                .get(port)
                                .copied()
                                .filter(|weight| *weight > EPS)
                                .unwrap_or(1.0);
                            let served = self.objects[target]
                                .merge_counts
                                .get(port)
                                .copied()
                                .unwrap_or(0.0);
                            (served + 1.0) / weight
                        };
                        score(*left)
                            .partial_cmp(&score(*right))
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| {
                                ready_position(*left, start, ports)
                                    .cmp(&ready_position(*right, start, ports))
                            })
                    })
                    .map(|(port, _)| port as u32)
            }
        }
    }

    /// Whether an object pulls its input instead of being pushed into.
    fn pull_enabled(&self, index: usize) -> bool {
        self.model.objects[index]
            .pull()
            .is_some_and(|pull| pull.enabled)
    }

    /// Ask an object to pull once the current event finishes. The flag makes
    /// the request idempotent inside one instant.
    fn schedule_pull(&mut self, index: usize) {
        if !self.pull_enabled(index) || self.objects[index].pull_scheduled {
            return;
        }
        self.objects[index].pull_scheduled = true;
        self.schedule(self.now, EventKind::Pull(index));
    }

    fn handle_pull(&mut self, index: usize) -> Result<(), SimError> {
        self.objects[index].pull_scheduled = false;
        self.pull_into(index)
    }

    /// Take as many items as the object can accept from its upstream objects.
    ///
    /// The upstream gives up its item exactly like a successful push: the item
    /// leaves the upstream `content`/`out_pending`, counts as upstream output
    /// and is delivered through the normal `deliver` path, so entry triggers
    /// and input statistics stay identical.
    fn pull_into(&mut self, index: usize) -> Result<(), SimError> {
        let Some(pull) = self.model.objects[index].pull().cloned() else {
            return Ok(());
        };
        if !pull.enabled || !self.objects[index].on_shift {
            return Ok(());
        }
        let inputs = self.inputs[index].clone();
        if inputs.is_empty() {
            return Ok(());
        }
        let now = self.now;
        let order = self.pull_order(index, &inputs, &pull);
        for conn in order {
            let (from, to_port) = {
                let connection = &self.conns[conn];
                (connection.from, connection.to_port)
            };
            if !self.can_accept(index, to_port) {
                break;
            }
            let mut delivered = false;
            for slot in self.pull_candidates(from) {
                let item = match slot {
                    UpstreamSlot::OutPending => self.objects[from].out_pending[0].clone(),
                    UpstreamSlot::Content(at) => self.objects[from].content[at].clone(),
                };
                if !self.pull_requirement_met(index, &pull, &item)? {
                    continue;
                }
                match slot {
                    UpstreamSlot::OutPending => {
                        self.objects[from].out_pending.remove(0);
                        if !self.objects[from].out_pending_accounted {
                            self.objects[from].acc.output += 1;
                            self.objects[from].acc.stay_sum += now - item.entry_time;
                            self.objects[from].acc.stay_count += 1;
                        }
                    }
                    UpstreamSlot::Content(at) => {
                        self.take_content(from, at);
                        self.objects[from].acc.output += 1;
                        self.objects[from].acc.stay_sum += now - item.entry_time;
                        self.objects[from].acc.stay_count += 1;
                    }
                }
                if !self.deliver(from, conn, item)? {
                    break;
                }
                delivered = true;
                break;
            }
            if delivered {
                // A source measures its inter-arrival time from the moment its
                // batch left, and a pull takes the item out of the out-buffer
                // without going through `drain_out_pending`.
                if self.objects[from].kind == ObjectKind::Source
                    && self.objects[from].next_arrival_pending
                    && self.objects[from].out_pending.is_empty()
                {
                    self.objects[from].next_arrival_pending = false;
                    self.schedule_next_arrival(from)?;
                }
                // The upstream object lost an item: let it and its neighbours
                // settle (it may now accept work or become idle).
                self.schedule(now, EventKind::Flow(from));
                self.notify_upstreams(from)?;
            }
        }
        Ok(())
    }

    /// Evaluate the per-item pull requirement; `None` accepts everything.
    fn pull_requirement_met(
        &self,
        index: usize,
        pull: &PullParams,
        item: &FlowItem,
    ) -> Result<bool, SimError> {
        let Some(requirement) = &pull.requirement else {
            return Ok(true);
        };
        let name = self.objects[index].name.clone();
        let context = ExprContext::new(self, Some(item));
        requirement
            .eval(&context)
            .map(|value| value.as_bool())
            .map_err(|err| SimError::Expression {
                location: format!("{name} pull requirement"),
                detail: err.to_string(),
            })
    }

    /// Connection order in which this object tries to pull.
    fn pull_order(&mut self, index: usize, inputs: &[usize], pull: &PullParams) -> Vec<usize> {
        match &pull.strategy {
            None | Some(PortStrategy::FirstAvailable) => inputs.to_vec(),
            Some(PortStrategy::RoundRobin) => {
                let start = self.objects[index].pull_cursor % inputs.len().max(1);
                self.objects[index].pull_cursor = self.objects[index].pull_cursor.wrapping_add(1);
                (0..inputs.len())
                    .map(|offset| inputs[(start + offset) % inputs.len()])
                    .collect()
            }
            Some(PortStrategy::Random) => {
                let mut order = inputs.to_vec();
                for i in (1..order.len()).rev() {
                    let j = self.rng.r#gen_range(0..=i);
                    order.swap(i, j);
                }
                order
            }
            Some(PortStrategy::ByPercentage { percentages }) => self
                .pick_pull_percentage(inputs, percentages)
                .into_iter()
                .collect(),
            Some(PortStrategy::ByCondition { conditions }) => {
                let mut order = Vec::new();
                for condition in conditions {
                    let matched = if let Some(script) = &condition.script_condition {
                        self.eval_script_condition(script, "pull routing condition", None)
                            .unwrap_or(false)
                    } else {
                        let context = ExprContext::new(self, None);
                        condition
                            .condition
                            .eval(&context)
                            .map(|value| value.as_bool())
                            .unwrap_or(false)
                    };
                    if matched {
                        if let Some(&conn) = inputs.get(condition.port as usize) {
                            order.push(conn);
                        }
                        break;
                    }
                }
                order
            }
            Some(PortStrategy::ByItemType) | Some(PortStrategy::ByLabel { .. }) => inputs.to_vec(),
            // A held pull strategy never takes anything.
            Some(PortStrategy::Hold) => Vec::new(),
        }
    }

    fn pick_pull_percentage(&mut self, inputs: &[usize], percentages: &[f64]) -> Option<usize> {
        if percentages.len() != inputs.len() || inputs.is_empty() {
            return inputs.first().copied();
        }
        let total: f64 = percentages.iter().sum();
        if total <= 0.0 {
            return inputs.first().copied();
        }
        let mut pick = self.rng.r#gen::<f64>() * total;
        for (conn, weight) in inputs.iter().zip(percentages) {
            if pick <= *weight {
                return Some(*conn);
            }
            pick -= *weight;
        }
        inputs.last().copied()
    }

    /// Items an upstream object can hand over. Finished items come first; a
    /// queue, rack or source also offers its buffered content, while a
    /// processor never gives up half-processed work.
    fn pull_candidates(&self, from: usize) -> Vec<UpstreamSlot> {
        let mut slots = Vec::new();
        if !self.objects[from].out_pending.is_empty() {
            slots.push(UpstreamSlot::OutPending);
        }
        match self.objects[from].kind {
            ObjectKind::Queue | ObjectKind::Rack | ObjectKind::Source => {
                // A rack only offers items that finished their minimum dwell.
                let min_dwell = match &self.model.objects[from].params {
                    ObjectParams::Rack(params) => params.min_dwell.max(0.0),
                    _ => 0.0,
                };
                for at in 0..self.objects[from].content.len() {
                    if min_dwell > 0.0
                        && self.now + EPS < self.objects[from].content[at].entry_time + min_dwell
                    {
                        continue;
                    }
                    slots.push(UpstreamSlot::Content(at));
                }
            }
            _ => {}
        }
        slots
    }

    // -----------------------------------------------------------------------
    // Queue and rack
    // -----------------------------------------------------------------------

    fn flow_queue(&mut self, index: usize) -> Result<(), SimError> {
        let params = queue_params_of(&self.model.objects[index]);
        // B2: a rack can hold every item for a minimum dwell time.
        let min_dwell = match &self.model.objects[index].params {
            ObjectParams::Rack(params) => params.min_dwell.max(0.0),
            _ => 0.0,
        };
        loop {
            if self.objects[index].content.is_empty() {
                self.objects[index].force_release = false;
                self.objects[index].batch_anchor = None;
                self.set_state(index, State::Idle);
                return Ok(());
            }
            if let Some(batch_size) = params.batch_size {
                let enough = self.objects[index].content.len() >= batch_size as usize;
                if !enough && !self.objects[index].force_release {
                    self.set_state(index, State::Blocked);
                    self.arm_batch_timeout(index, &params);
                    return Ok(());
                }
                // A batch is released as a whole: the items are committed at
                // this instant (that is when their stay time ends), even
                // though the downstream object may only take them one by one.
                self.objects[index].note_content(self.now);
                let items = std::mem::take(&mut self.objects[index].content);
                for item in items {
                    self.objects[index].acc.output += 1;
                    self.objects[index].acc.stay_sum += self.now - item.entry_time;
                    self.objects[index].acc.stay_count += 1;
                    self.objects[index].out_pending.push(item);
                }
                self.objects[index].out_pending_accounted = true;
                self.objects[index].force_release = false;
                self.objects[index].batch_anchor = None;
                self.objects[index].touch();
                return Ok(());
            }
            // task-47: a pending pick waits for the picker to walk to the bay.
            let position = if let Some((pending, ready_at)) = self.objects[index].rack_pick {
                if self.now + EPS < ready_at {
                    self.set_state(index, State::Blocked);
                    return Ok(());
                }
                self.objects[index].rack_pick = None;
                if pending < self.objects[index].content.len() {
                    pending
                } else {
                    // The slot vanished while walking: pick again.
                    self.choose_rack_position(index, &params)
                }
            } else {
                let chosen = self.choose_rack_position(index, &params);
                let travel = self.rack_pick_travel(index, chosen);
                if travel > EPS && self.now + travel < self.config.stop_time - EPS {
                    self.objects[index].rack_pick = Some((chosen, self.now + travel));
                    self.set_state(index, State::Blocked);
                    self.schedule(self.now + travel, EventKind::RackPick(index));
                    return Ok(());
                }
                chosen
            };
            let picked_level = self.objects[index]
                .content
                .get(position)
                .and_then(|item| item.labels.get("level").copied());
            if min_dwell > 0.0
                && let Some(next) = self.objects[index].content.get(position)
                && self.now + EPS < next.entry_time + min_dwell
            {
                let ready_at = next.entry_time + min_dwell;
                self.set_state(index, State::Blocked);
                self.arm_rack_dwell(index, ready_at);
                return Ok(());
            }
            let Some(item) = self.take_content(index, position) else {
                return Ok(());
            };
            let Some(conn) = self.route(index, &item)? else {
                self.undo_queue_take(index, position, item);
                return Ok(());
            };
            if !self.deliver(index, conn, item.clone())? {
                self.undo_queue_take(index, position, item);
                return Ok(());
            }
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += self.now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
            if let Some(level) = picked_level {
                self.objects[index].last_picked_level = Some(level.max(0.0) as u32);
            }
        }
    }

    /// task-44: the rack position the pick policy wants next (`None` = use the
    /// ordinary queue order).
    /// task-47: content position of the next rack item (policy + queue order).
    fn choose_rack_position(&self, index: usize, params: &openflex_model::QueueParams) -> usize {
        let policy = match &self.model.objects[index].params {
            ObjectParams::Rack(rack) => rack.pick_policy,
            _ => RackPickPolicy::Fifo,
        };
        self.pick_rack_position(index, policy)
            .unwrap_or_else(|| self.pick_queue_position(index, &params.order))
    }

    /// task-47: walking time from the rack face to the item's bay.
    fn rack_pick_travel(&self, index: usize, position: usize) -> f64 {
        let (per_bay, per_aisle) = match &self.model.objects[index].params {
            ObjectParams::Rack(params) => (
                params.travel_time_per_bay.max(0.0),
                params.aisle_time.max(0.0),
            ),
            _ => return 0.0,
        };
        if per_bay <= 0.0 && per_aisle <= 0.0 {
            return 0.0;
        }
        let Some(item) = self.objects[index].content.get(position) else {
            return 0.0;
        };
        let bay = item.labels.get("bay").copied().unwrap_or(0.0).max(0.0);
        let aisle = item.labels.get("aisle").copied().unwrap_or(0.0).max(0.0);
        per_bay * bay + per_aisle * aisle
    }

    fn pick_rack_position(&self, index: usize, policy: RackPickPolicy) -> Option<usize> {
        let content = &self.objects[index].content;
        if content.is_empty() {
            return None;
        }
        let level_of =
            |item: &FlowItem| item.labels.get("level").copied().unwrap_or(0.0).max(0.0) as u32;
        match policy {
            RackPickPolicy::Fifo => None,
            RackPickPolicy::LevelOrder => content
                .iter()
                .enumerate()
                .min_by_key(|(_, item)| level_of(item))
                .map(|(position, _)| position),
            RackPickPolicy::Nearest => {
                let reference = self.objects[index].last_picked_level.unwrap_or(0);
                content
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, item)| level_of(item).abs_diff(reference))
                    .map(|(position, _)| position)
            }
        }
    }

    /// Put an item back at the position it was taken from and block.
    fn undo_queue_take(&mut self, index: usize, position: usize, item: FlowItem) {
        let at = position.min(self.objects[index].content.len());
        self.objects[index].note_content(self.now);
        self.objects[index].content.insert(at, item);
        self.set_state(index, State::Blocked);
    }

    // -----------------------------------------------------------------------
    // Emptier (task-36)
    // -----------------------------------------------------------------------

    /// Emptier parameters of a `basic_fr` object.
    fn emptier_params(&self, index: usize) -> Option<openflex_model::EmptierParams> {
        match &self.model.objects[index].params {
            ObjectParams::BasicFr(params) => params.emptier.clone(),
            _ => None,
        }
    }

    /// Schedule the next sweep of an Emptier.
    fn schedule_emptier_sweep(&mut self, index: usize) {
        let Some(params) = self.emptier_params(index) else {
            return;
        };
        let interval = self.sample(&params.interval).max(EPS);
        let at = self.now + interval;
        if at < self.config.stop_time - EPS {
            self.schedule(at, EventKind::EmptierSweep(index));
        }
    }

    /// Move the items waiting in the upstream objects to this object's outputs
    /// (or destroy them). The upstream objects are the connections entering the
    /// Emptier; they cannot push into it because its `can_accept` is false.
    fn handle_emptier_sweep(&mut self, index: usize) -> Result<(), SimError> {
        let Some(params) = self.emptier_params(index) else {
            return Ok(());
        };
        let max_items = params.max_items as usize;
        let now = self.now;
        let inputs = self.inputs[index].clone();
        let mut moved = 0usize;
        'sweep: for conn in inputs {
            let from = self.conns[conn].from;
            loop {
                if max_items > 0 && moved >= max_items {
                    break 'sweep;
                }
                let Some(slot) = self.pull_candidates(from).into_iter().next() else {
                    break;
                };
                let item = match slot {
                    UpstreamSlot::OutPending => self.objects[from].out_pending[0].clone(),
                    UpstreamSlot::Content(at) => self.objects[from].content[at].clone(),
                };
                // Take the item out of the upstream object (fires its
                // on_content_change) and charge its output statistics.
                match slot {
                    UpstreamSlot::OutPending => {
                        self.objects[from].out_pending.remove(0);
                        if !self.objects[from].out_pending_accounted {
                            self.objects[from].acc.output += 1;
                            self.objects[from].acc.stay_sum += now - item.entry_time;
                            self.objects[from].acc.stay_count += 1;
                        }
                    }
                    UpstreamSlot::Content(at) => {
                        self.take_content(from, at);
                        self.objects[from].acc.output += 1;
                        self.objects[from].acc.stay_sum += now - item.entry_time;
                        self.objects[from].acc.stay_count += 1;
                    }
                }
                self.objects[index].acc.input += 1;
                self.objects[index].acc.output += 1;
                self.objects[index].acc.stay_sum += now - item.entry_time;
                self.objects[index].acc.stay_count += 1;
                moved += 1;
                if params.destroy {
                    continue;
                }
                let Some(out_conn) = self.route(index, &item)? else {
                    // Nowhere to put it: put it back and stop this sweep.
                    self.objects[from].push_content(now, item);
                    break 'sweep;
                };
                if !self.deliver(index, out_conn, item.clone())? {
                    self.objects[from].push_content(now, item);
                    break 'sweep;
                }
                // The upstream object lost an item: let it settle.
                self.schedule(now, EventKind::Flow(from));
            }
        }
        self.notify_upstreams(index)?;
        self.schedule_emptier_sweep(index);
        Ok(())
    }

    /// A rack item finished its minimum dwell time: try to release again.
    fn handle_dwell_done(&mut self, index: usize) -> Result<(), SimError> {
        self.objects[index].dwell_at = f64::INFINITY;
        self.flow(index)
    }

    /// Arm the dwell timer of a rack; it fires when the oldest releasable item
    /// has waited `min_dwell` (B2).
    fn arm_rack_dwell(&mut self, index: usize, ready_at: f64) {
        if ready_at >= self.config.stop_time - EPS {
            return;
        }
        if self.objects[index].dwell_at <= ready_at + EPS {
            return;
        }
        self.objects[index].dwell_at = ready_at;
        self.schedule(ready_at, EventKind::DwellDone(index));
    }

    /// Arm the batching timeout for a queue that is waiting for a full batch.
    fn arm_batch_timeout(&mut self, index: usize, params: &openflex_model::QueueParams) {
        if params.batch_max_wait <= 0.0 || self.objects[index].batch_anchor.is_some() {
            return;
        }
        let anchor = self.objects[index]
            .content
            .first()
            .map(|item| item.entry_time)
            .unwrap_or(self.now);
        self.objects[index].batch_anchor = Some(anchor);
        let due = (anchor + params.batch_max_wait).max(self.now);
        self.schedule(due, EventKind::BatchTimeout(index));
    }

    fn handle_batch_timeout(&mut self, index: usize) -> Result<(), SimError> {
        let params = queue_params_of(&self.model.objects[index]);
        if let Some(anchor) = self.objects[index].batch_anchor {
            let due = anchor + params.batch_max_wait;
            if self.now + EPS >= due {
                self.objects[index].force_release = true;
                self.objects[index].batch_anchor = None;
            }
        }
        self.flow(index)
    }

    fn pick_queue_position(&self, index: usize, order: &QueueOrder) -> usize {
        match order {
            QueueOrder::Fifo => 0,
            QueueOrder::Lifo => self.objects[index].content.len() - 1,
            QueueOrder::Priority => self.objects[index]
                .content
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| {
                    a.labels
                        .get("priority")
                        .copied()
                        .unwrap_or(0.0)
                        .partial_cmp(&b.labels.get("priority").copied().unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(index, _)| index)
                .unwrap_or(0),
            QueueOrder::ByLabel(label) => self.objects[index]
                .content
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.labels
                        .get(label)
                        .copied()
                        .unwrap_or(0.0)
                        .partial_cmp(&b.labels.get(label).copied().unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(index, _)| index)
                .unwrap_or(0),
        }
    }

    // -----------------------------------------------------------------------
    // Processor
    // -----------------------------------------------------------------------

    /// task-190: changeover delay for a processor switching item types.
    fn changeover_delay(
        &mut self,
        index: usize,
        params: &openflex_model::ProcessorParams,
        next_type: Option<u32>,
    ) -> f64 {
        let Some(next_type) = next_type else {
            return 0.0;
        };
        let Some(previous) = self.objects[index].last_item_type else {
            return 0.0;
        };
        if previous == next_type {
            return 0.0;
        }
        let delay = self.sample(&params.changeover_time).max(0.0);
        if delay > EPS {
            self.record_object_trace("changeover", index);
        }
        delay
    }

    fn combiner_params(&self, index: usize) -> Option<openflex_model::CombinerParams> {
        match &self.model.objects[index].params {
            ObjectParams::Combiner(params) => Some(params.clone()),
            _ => None,
        }
    }

    fn processor_params(&self, index: usize) -> Option<openflex_model::ProcessorParams> {
        match &self.model.objects[index].params {
            ObjectParams::Processor(params) => Some(params.clone()),
            _ => None,
        }
    }

    fn flow_processor(&mut self, index: usize) -> Result<(), SimError> {
        let Some(params) = self.processor_params(index) else {
            return Ok(());
        };
        if !self.objects[index].on_shift || self.objects[index].state == State::Down {
            return Ok(());
        }
        // Continue after a setup whose process operator was not available yet.
        if self.objects[index].ready_for_process {
            if !self.ensure_operator(index, OperatorPhase::Process, &params)? {
                return Ok(());
            }
            self.objects[index].ready_for_process = false;
            let process = self.sample(&params.process_time).max(0.0);
            self.set_phase(index, Phase::Processing);
            self.objects[index].phase_token += 1;
            let token = self.objects[index].phase_token;
            self.objects[index].phase_end = self.now + process;
            self.set_state(index, State::Busy);
            self.schedule(self.now + process, EventKind::ProcessDone(index, token));
            return Ok(());
        }
        if matches!(self.objects[index].phase, Phase::Setup | Phase::Processing) {
            return Ok(());
        }
        if self.objects[index].processing_item.is_some() {
            return Ok(());
        }
        if self.objects[index].content.is_empty() {
            self.set_phase(index, Phase::Idle);
            self.set_state(index, State::Idle);
            return Ok(());
        }
        // task-190: a type change between consecutive items adds a changeover
        // delay to the setup of the next item (same type: no delay; first item:
        // no previous type, so no delay).
        let next_type = self.objects[index]
            .content
            .first()
            .map(|item| item.item_type);
        let changeover = self.changeover_delay(index, &params, next_type);
        let setup = self.sample(&params.setup_time).max(0.0) + changeover;
        let phase = if setup > 0.0 {
            OperatorPhase::Setup
        } else {
            OperatorPhase::Process
        };
        if !self.ensure_operator(index, phase, &params)? {
            return Ok(());
        }
        let Some(item) = self.take_content(index, 0) else {
            return Ok(());
        };
        self.objects[index].last_item_type = Some(item.item_type);
        self.objects[index].processing_item = Some(item);
        self.objects[index].phase_token += 1;
        let token = self.objects[index].phase_token;
        let now = self.now;
        if setup > 0.0 {
            self.set_phase(index, Phase::Setup);
            self.objects[index].phase_end = now + setup;
            self.set_state(index, State::Busy);
            self.schedule(now + setup, EventKind::SetupDone(index, token));
        } else {
            let process = self.sample(&params.process_time).max(0.0);
            self.set_phase(index, Phase::Processing);
            self.objects[index].phase_end = now + process;
            self.set_state(index, State::Busy);
            self.schedule(now + process, EventKind::ProcessDone(index, token));
        }
        Ok(())
    }

    fn handle_setup_done(&mut self, index: usize, token: u64) -> Result<(), SimError> {
        if self.objects[index].phase_token != token {
            return Ok(());
        }
        match self.kind(index) {
            ObjectKind::Processor => {
                let Some(params) = self.processor_params(index) else {
                    return Ok(());
                };
                let same_operator = params.setup_operator.is_some()
                    && params.setup_operator == params.process_operator;
                if !same_operator {
                    self.release_operator(index)?;
                }
                self.set_phase(index, Phase::Idle);
                self.objects[index].ready_for_process = true;
                // task-152: the setup clock just reached zero. This is not
                // OnReceive/OnEntry (those fire when the item arrives); the
                // processing clock starts right after this hook.
                self.fire_lifecycle_trigger(index, SimTrigger::SetupFinish);
                self.flow_processor(index)
            }
            ObjectKind::Combiner => self.handle_combiner_setup_done(index),
            _ => Ok(()),
        }
    }

    /// A7: setup of a combiner batch finished; continue with processing.
    fn handle_combiner_setup_done(&mut self, index: usize) -> Result<(), SimError> {
        let Some(params) = self.combiner_params(index) else {
            return Ok(());
        };
        let same_operator =
            params.setup_operator.is_some() && params.setup_operator == params.process_operator;
        if !same_operator {
            self.release_operator(index)?;
        }
        self.set_phase(index, Phase::Idle);
        self.objects[index].ready_for_process = true;
        self.flow_combiner(index)
    }

    /// A7: processing of a combiner batch finished; release every item.
    fn handle_combiner_process_done(&mut self, index: usize) -> Result<(), SimError> {
        let now = self.now;
        self.objects[index].note_content(now);
        let mut items: Vec<FlowItem> = Vec::new();
        if let Some(container) = self.objects[index].container.take() {
            items.push(container);
        }
        items.extend(std::mem::take(&mut self.objects[index].components));
        let count = items.len();
        for mut item in items {
            item.labels.insert("batch".to_string(), count as f64);
            self.objects[index].out_pending.push(item);
        }
        self.objects[index].touch();
        self.set_phase(index, Phase::WaitingForOutput);
        self.release_operator(index)?;
        self.flow(index)?;
        if self.objects[index].out_pending.is_empty()
            && self.objects[index].container.is_none()
            && self.objects[index].components.is_empty()
            && !matches!(self.objects[index].phase, Phase::Setup | Phase::Processing)
        {
            self.set_phase(index, Phase::Idle);
            self.set_state(index, State::Idle);
        }
        self.notify_upstreams(index)?;
        Ok(())
    }

    fn handle_process_done(&mut self, index: usize, token: u64) -> Result<(), SimError> {
        if self.objects[index].phase_token != token {
            return Ok(());
        }
        if self.kind(index) == ObjectKind::Combiner {
            return self.handle_combiner_process_done(index);
        }
        self.objects[index].note_content(self.now);
        let Some(item) = self.objects[index].processing_item.take() else {
            self.set_phase(index, Phase::Idle);
            self.set_state(index, State::Idle);
            return Ok(());
        };
        self.objects[index].out_pending.push(item);
        self.objects[index].touch();
        self.set_phase(index, Phase::WaitingForOutput);
        self.release_operator(index)?;
        // task-152: the processing clock just reached zero; the item is not yet
        // delivered (on_exit fires later, when it leaves the object).
        self.fire_lifecycle_trigger(index, SimTrigger::ProcessFinish);
        // `flow` delivers the finished item and, when there is another item
        // waiting, starts the next setup/process step.
        self.flow(index)?;
        if self.objects[index].out_pending.is_empty()
            && self.objects[index].content.is_empty()
            && !matches!(self.objects[index].phase, Phase::Setup | Phase::Processing)
        {
            self.set_phase(index, Phase::Idle);
            self.set_state(index, State::Idle);
        }
        // A finished item frees a slot: upstream objects may now flow.
        self.notify_upstreams(index)?;
        Ok(())
    }

    /// Pause the current setup/process step, remembering the remaining time.
    fn pause_phase(&mut self, index: usize) {
        if self.kind(index) == ObjectKind::MultiProcessor {
            let now = self.now;
            for lane in 0..self.objects[index].lanes.len() {
                if !matches!(
                    self.objects[index].lanes[lane].phase,
                    Phase::Setup | Phase::Processing
                ) || self.objects[index].lanes[lane].paused
                {
                    continue;
                }
                self.objects[index].lanes[lane].paused_remaining =
                    (self.objects[index].lanes[lane].end - now).max(0.0);
                self.objects[index].lanes[lane].token += 1;
                self.objects[index].lanes[lane].paused = true;
                let busy_since = self.objects[index].lanes[lane].busy_since;
                if busy_since > 0.0 {
                    self.objects[index].slot_busy += (now - busy_since).max(0.0);
                    self.objects[index].lanes[lane].busy_since = 0.0;
                }
            }
            return;
        }
        if matches!(self.objects[index].phase, Phase::Setup | Phase::Processing)
            && self.objects[index].paused_remaining <= 0.0
        {
            let remaining = (self.objects[index].phase_end - self.now).max(0.0);
            self.objects[index].paused_remaining = remaining;
            // Invalidate the pending completion event.
            self.objects[index].phase_token += 1;
        }
    }

    /// Resume a paused setup/process step. Returns `true` when a step resumed.
    fn resume_phase(&mut self, index: usize) -> Result<bool, SimError> {
        if self.kind(index) == ObjectKind::MultiProcessor {
            let mut resumed = false;
            for lane in 0..self.objects[index].lanes.len() {
                if !self.objects[index].lanes[lane].paused {
                    continue;
                }
                let remaining = self.objects[index].lanes[lane].paused_remaining;
                self.objects[index].lanes[lane].paused = false;
                self.objects[index].lanes[lane].paused_remaining = 0.0;
                self.objects[index].lanes[lane].busy_since = self.now;
                self.objects[index].lanes[lane].token += 1;
                let token = self.objects[index].lanes[lane].token;
                self.objects[index].lanes[lane].end = self.now + remaining;
                self.schedule(self.now + remaining, EventKind::MpDone(index, lane, token));
                resumed = true;
            }
            if resumed {
                self.refresh_multiprocessor(index)?;
            }
            return Ok(resumed);
        }
        if !matches!(self.objects[index].phase, Phase::Setup | Phase::Processing) {
            return Ok(false);
        }
        let remaining = self.objects[index].paused_remaining;
        self.objects[index].paused_remaining = 0.0;
        self.objects[index].phase_token += 1;
        let token = self.objects[index].phase_token;
        self.objects[index].phase_end = self.now + remaining;
        let event = match self.objects[index].phase {
            Phase::Setup => EventKind::SetupDone(index, token),
            _ => EventKind::ProcessDone(index, token),
        };
        self.set_state(index, State::Busy);
        self.schedule(self.now + remaining, event);
        Ok(true)
    }

    // -----------------------------------------------------------------------
    // Process Flow (B7)
    // -----------------------------------------------------------------------

    fn process_flow_block_count(&self) -> usize {
        self.process_flow
            .as_ref()
            .map_or(0, |flow| flow.blocks.len())
    }

    fn flow_block_kind(&self, block: usize) -> Option<FlowBlockKind> {
        self.flow_defs.get(block).map(|meta| meta.kind.clone())
    }

    fn flow_block_name(&self, block: usize) -> String {
        self.flow_defs
            .get(block)
            .map(|meta| match meta.owner {
                Some(owner) => format!("{}.{}", self.objects[owner].name, meta.id),
                None => meta.id.clone(),
            })
            .unwrap_or_default()
    }

    /// Settle the time-weighted token count of one block up to `now`.
    fn flow_note(&mut self, block: usize, now: f64) {
        let Some(flow) = self.process_flow.as_mut() else {
            return;
        };
        let Some(state) = flow.blocks.get_mut(block) else {
            return;
        };
        let dt = (now - state.last_content_change).max(0.0);
        state.content_integral += state.tokens.len() as f64 * dt;
        state.last_content_change = now;
    }

    fn flow_add_token(&mut self, block: usize, token: FlowItem, ready_at: Option<f64>) {
        if let Some(flow) = self.process_flow.as_mut()
            && let Some(state) = flow.blocks.get_mut(block)
        {
            state.tokens.push(FlowTokenRt { token, ready_at });
        }
    }

    /// Take a token out of a block (settling the block's content first).
    fn flow_take_token(&mut self, block: usize, index: usize) -> Option<FlowTokenRt> {
        self.flow_note(block, self.now);
        let flow = self.process_flow.as_mut()?;
        let state = flow.blocks.get_mut(block)?;
        if index >= state.tokens.len() {
            return None;
        }
        Some(state.tokens.remove(index))
    }

    /// Schedule one `FlowAdvance` per block and instant.
    fn flow_schedule_advance(&mut self, block: usize) {
        let scheduled = self
            .process_flow
            .as_ref()
            .and_then(|flow| flow.blocks.get(block))
            .is_some_and(|state| state.advance_scheduled);
        if scheduled {
            return;
        }
        if let Some(state) = self
            .process_flow
            .as_mut()
            .and_then(|flow| flow.blocks.get_mut(block))
        {
            state.advance_scheduled = true;
        }
        self.schedule(self.now, EventKind::FlowAdvance(block));
    }

    /// Schedule the next creation of a create block.
    fn schedule_flow_create(&mut self, block: usize) -> Result<(), SimError> {
        let Some(FlowBlockKind::Create(params)) = self.flow_block_kind(block) else {
            return Ok(());
        };
        let creations = self
            .process_flow
            .as_ref()
            .and_then(|flow| flow.blocks.get(block))
            .map_or(0, |state| state.creations);
        if params.on_entry {
            return Ok(());
        }
        if let Some(max) = params.max_creations
            && creations >= max
        {
            return Ok(());
        }
        let at = if creations == 0 {
            params.first_arrival.max(0.0)
        } else {
            let interval = self.sample(&params.interarrival).max(0.0);
            if interval <= EPS && params.max_creations.is_none() {
                return Err(SimError::Config(format!(
                    "process flow block '{}': a zero interarrival without max_creations \
                     would create tokens forever",
                    self.flow_block_name(block)
                )));
            }
            self.now + interval
        };
        self.schedule(at, EventKind::FlowCreate(block));
        Ok(())
    }

    fn handle_flow_create(&mut self, block: usize) -> Result<(), SimError> {
        let Some(FlowBlockKind::Create(params)) = self.flow_block_kind(block) else {
            return Ok(());
        };
        let creations = self
            .process_flow
            .as_ref()
            .and_then(|flow| flow.blocks.get(block))
            .map_or(0, |state| state.creations);
        if let Some(max) = params.max_creations
            && creations >= max
        {
            return Ok(());
        }
        let quantity = params.quantity.max(1);
        self.spawn_flow_tokens(block, quantity, params.item_type, &params.labels);
        if let Some(state) = self
            .process_flow
            .as_mut()
            .and_then(|flow| flow.blocks.get_mut(block))
        {
            state.creations += 1;
        }
        self.flow_schedule_advance(block);
        self.schedule_flow_create(block)
    }

    /// Create `quantity` tokens at `block` (shared by scheduled and
    /// entry-triggered create blocks).
    fn spawn_flow_tokens(
        &mut self,
        block: usize,
        quantity: u32,
        item_type: u32,
        labels: &BTreeMap<String, f64>,
    ) {
        for _ in 0..quantity {
            let token_id = {
                let Some(flow) = self.process_flow.as_mut() else {
                    return;
                };
                flow.next_token_id += 1;
                flow.tokens_created += 1;
                flow.next_token_id
            };
            let token = FlowItem {
                id: token_id,
                item_type,
                labels: labels.clone(),
                created: self.now,
                entry_time: self.now,
                destination: None,
            };
            self.flow_note(block, self.now);
            self.flow_add_token(block, token, None);
            self.record_token_trace("token_create", block, token_id);
        }
    }

    /// B7: run the `on_entry` create blocks of an object that just received an
    /// item.
    fn trigger_entry_flow(&mut self, target: usize) -> Result<(), SimError> {
        let blocks = self
            .flow_entry_blocks
            .get(target)
            .cloned()
            .unwrap_or_default();
        for block in blocks {
            let Some(FlowBlockKind::Create(params)) = self.flow_block_kind(block) else {
                continue;
            };
            let quantity = params.quantity.max(1);
            self.spawn_flow_tokens(block, quantity, params.item_type, &params.labels);
            self.flow_schedule_advance(block);
        }
        Ok(())
    }

    fn handle_flow_advance(&mut self, block: usize) -> Result<(), SimError> {
        if let Some(state) = self
            .process_flow
            .as_mut()
            .and_then(|flow| flow.blocks.get_mut(block))
        {
            state.advance_scheduled = false;
        }
        while let Some(kind) = self.flow_block_kind(block) {
            let index = {
                let Some(flow) = self.process_flow.as_ref() else {
                    break;
                };
                let Some(state) = flow.blocks.get(block) else {
                    break;
                };
                state
                    .tokens
                    .iter()
                    .position(|entry| entry.ready_at.is_none())
            };
            let Some(index) = index else {
                break;
            };
            let now = self.now;
            match kind {
                FlowBlockKind::Create(_) => {
                    let Some(entry) = self.flow_take_token(block, index) else {
                        break;
                    };
                    self.flow_route(block, entry.token)?;
                }
                FlowBlockKind::Delay(params) => {
                    let delay = self.sample(&params.time).max(0.0);
                    let token_id = {
                        let Some(flow) = self.process_flow.as_mut() else {
                            break;
                        };
                        let Some(state) = flow.blocks.get_mut(block) else {
                            break;
                        };
                        let Some(entry) = state.tokens.get_mut(index) else {
                            break;
                        };
                        entry.ready_at = Some(now + delay);
                        entry.token.id
                    };
                    self.schedule(now + delay, EventKind::FlowDelayDone(block, token_id));
                }
                FlowBlockKind::Assign(params) => {
                    let Some(entry) = self.flow_take_token(block, index) else {
                        break;
                    };
                    let mut token = entry.token;
                    self.apply_token_actions(block, &mut token, params.actions.clone())?;
                    self.record_token_trace("token_assign", block, token.id);
                    self.flow_route(block, token)?;
                }
                FlowBlockKind::Decision(params) => {
                    let Some(entry) = self.flow_take_token(block, index) else {
                        break;
                    };
                    let token = entry.token;
                    let port = self.flow_decision_port(&token, &params.conditions)?;
                    self.record_token_trace("token_decision", block, token.id);
                    self.flow_route_at(block, token, port)?;
                }
                FlowBlockKind::Sink => {
                    let Some(entry) = self.flow_take_token(block, index) else {
                        break;
                    };
                    let token = entry.token;
                    let stay = now - token.entry_time;
                    if let Some(state) = self
                        .process_flow
                        .as_mut()
                        .and_then(|flow| flow.blocks.get_mut(block))
                    {
                        state.output += 1;
                        state.stay_sum += stay;
                        state.stay_count += 1;
                    }
                    if let Some(flow) = self.process_flow.as_mut() {
                        flow.tokens_completed += 1;
                    }
                    self.record_token_trace("token_sink", block, token.id);
                }
                FlowBlockKind::Transfer(params) => {
                    if params.mode == TransferMode::FromObject {
                        let Some(entry) = self.flow_take_token(block, index) else {
                            break;
                        };
                        let mut token = entry.token;
                        let Some(&source) = self.by_name.get(&params.object) else {
                            break;
                        };
                        let Some(item) = self.take_content(source, 0) else {
                            // No item to pull yet: keep the token waiting and
                            // retry when the source receives something.
                            self.flow_add_token(block, token, None);
                            break;
                        };
                        let stay = now - item.entry_time;
                        {
                            let state = &mut self.objects[source];
                            state.acc.output += 1;
                            state.acc.stay_sum += stay;
                            state.acc.stay_count += 1;
                        }
                        token.item_type = item.item_type;
                        token.labels = item.labels.clone();
                        token.created = item.created;
                        token.entry_time = now;
                        self.record_token_trace("token_from_object", block, token.id);
                        self.flow_route(block, token)?;
                        continue;
                    }
                    let Some(&target) = self.by_name.get(&params.object) else {
                        break;
                    };
                    if !self.can_accept(target, 0) {
                        // Keep the token in the block; `notify_upstreams`
                        // wakes it when the target frees capacity.
                        break;
                    }
                    let Some(entry) = self.flow_take_token(block, index) else {
                        break;
                    };
                    let token = entry.token;
                    let stay = now - token.entry_time;
                    if let Some(state) = self
                        .process_flow
                        .as_mut()
                        .and_then(|flow| flow.blocks.get_mut(block))
                    {
                        state.output += 1;
                        state.stay_sum += stay;
                        state.stay_count += 1;
                    }
                    self.items_created += 1;
                    self.stats_created += 1;
                    let item = FlowItem {
                        id: self.items_created,
                        item_type: token.item_type,
                        labels: token.labels.clone(),
                        created: now,
                        entry_time: now,
                        destination: None,
                    };
                    self.record_token_trace("token_transfer", block, token.id);
                    if !self.receive_item(target, 0, item)? {
                        self.flow_add_token(block, token, None);
                        break;
                    }
                    if let Some(flow) = self.process_flow.as_mut() {
                        flow.tokens_completed += 1;
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_flow_delay_done(&mut self, block: usize, token: u64) -> Result<(), SimError> {
        let index = {
            let Some(flow) = self.process_flow.as_ref() else {
                return Ok(());
            };
            let Some(state) = flow.blocks.get(block) else {
                return Ok(());
            };
            state
                .tokens
                .iter()
                .position(|entry| entry.token.id == token && entry.ready_at.is_some())
        };
        let Some(index) = index else {
            return Ok(());
        };
        let ready_at = self
            .process_flow
            .as_ref()
            .and_then(|flow| flow.blocks.get(block))
            .and_then(|state| state.tokens.get(index))
            .and_then(|entry| entry.ready_at);
        if ready_at.is_some_and(|at| at > self.now + EPS) {
            return Ok(());
        }
        let Some(entry) = self.flow_take_token(block, index) else {
            return Ok(());
        };
        self.record_token_trace("token_delay", block, token);
        self.flow_route(block, entry.token)
    }

    /// Send a token to the first outgoing connection of a block.
    fn flow_route(&mut self, block: usize, token: FlowItem) -> Result<(), SimError> {
        self.flow_route_at(block, token, None)
    }

    /// Send a token to the outgoing connection with `port`, or the first one.
    fn flow_route_at(
        &mut self,
        block: usize,
        mut token: FlowItem,
        port: Option<u32>,
    ) -> Result<(), SimError> {
        let now = self.now;
        let target = {
            let Some(flow) = self.process_flow.as_ref() else {
                return Ok(());
            };
            let Some(outgoing) = flow.outgoing.get(block) else {
                return Ok(());
            };
            let connection = match port {
                Some(port) => outgoing
                    .iter()
                    .copied()
                    .find(|&index| flow.connections[index].port == port),
                None => outgoing.first().copied(),
            };
            connection.map(|index| flow.connections[index].to)
        };
        let Some(target) = target else {
            return Ok(());
        };
        let stay = now - token.entry_time;
        self.flow_note(block, now);
        if let Some(state) = self
            .process_flow
            .as_mut()
            .and_then(|flow| flow.blocks.get_mut(block))
        {
            state.output += 1;
            state.stay_sum += stay;
            state.stay_count += 1;
        }
        token.entry_time = now;
        self.flow_note(target, now);
        if let Some(state) = self
            .process_flow
            .as_mut()
            .and_then(|flow| flow.blocks.get_mut(target))
        {
            state.input += 1;
        }
        self.flow_add_token(target, token, None);
        self.flow_schedule_advance(target);
        Ok(())
    }

    /// Pick the decision port for a token; `None` means "default branch".
    fn flow_decision_port(
        &self,
        token: &FlowItem,
        conditions: &[ConditionalPort],
    ) -> Result<Option<u32>, SimError> {
        for condition in conditions {
            let matched = if let Some(script) = &condition.script_condition {
                self.eval_script_condition(script, "process flow decision", Some(token))?
            } else {
                let context = ExprContext::new(self, Some(token));
                condition
                    .condition
                    .eval(&context)
                    .map_err(|err| SimError::Expression {
                        location: "process flow decision".into(),
                        detail: err.to_string(),
                    })?
                    .as_bool()
            };
            if matched {
                return Ok(Some(condition.port));
            }
        }
        Ok(None)
    }

    /// Apply the label/type actions of an assign block to a token.
    fn apply_token_actions(
        &mut self,
        block: usize,
        token: &mut FlowItem,
        actions: Vec<Action>,
    ) -> Result<(), SimError> {
        let block_name = self.flow_block_name(block);
        for action in actions {
            match action {
                Action::SetLabel { name, value } => {
                    let value = self.eval_expr(&block_name, token, &value)?;
                    token.labels.insert(name, value.as_number().unwrap_or(0.0));
                }
                Action::SetItemType { value } => {
                    let value = self.eval_expr(&block_name, token, &value)?;
                    token.item_type = value.as_number().unwrap_or(0.0).max(0.0) as u32;
                }
                Action::IncrementLabel { name, by } => {
                    let by = self.eval_expr(&block_name, token, &by)?;
                    let entry = token.labels.entry(name).or_insert(0.0);
                    *entry += by.as_number().unwrap_or(0.0);
                }
                _ => {
                    return Err(SimError::Config(format!(
                        "process flow block '{block_name}' only supports label/type actions"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Record one token event in the trace log (B7).
    fn record_token_trace(&mut self, label: &str, block: usize, token: u64) {
        if self.trace.is_none() {
            return;
        }
        let block_name = self.flow_block_name(block);
        let time = self.now;
        let seq = self.next_seq;
        self.next_seq += 1;
        if let Some(log) = self.trace.as_mut() {
            log.push(TraceEvent::new(time, seq, label, block_name).with("token", token as f64));
        }
    }

    fn finalize_flow(&mut self) {
        let now = self.now;
        for block in 0..self.process_flow_block_count() {
            self.flow_note(block, now);
        }
    }

    /// Build the reported Process Flow statistics of this replication.
    fn flow_replication(&self, elapsed: f64) -> Option<ReplicationFlow> {
        let flow = self.process_flow.as_ref()?;
        let denom = elapsed.max(1e-12);
        let blocks = flow
            .blocks
            .iter()
            .enumerate()
            .map(|(position, state)| {
                let definition = self.flow_defs.get(position);
                ReplicationFlowBlock {
                    id: definition
                        .map(|block| match block.owner {
                            Some(owner) => {
                                format!("{}.{}", self.objects[owner].name, block.id)
                            }
                            None => block.id.clone(),
                        })
                        .unwrap_or_default(),
                    kind: definition
                        .map(|block| flow_block_kind_name(&block.kind).to_string())
                        .unwrap_or_default(),
                    input_count: state.input,
                    output_count: state.output,
                    content_avg: state.content_integral / denom,
                    avg_stay_time: if state.stay_count > 0 {
                        state.stay_sum / state.stay_count as f64
                    } else {
                        0.0
                    },
                }
            })
            .collect();
        Some(ReplicationFlow {
            tokens_created: flow.tokens_created,
            tokens_completed: flow.tokens_completed,
            blocks,
        })
    }

    // -----------------------------------------------------------------------
    // Fluid family (B5)
    // -----------------------------------------------------------------------

    fn handle_fluid_step(&mut self) -> Result<(), SimError> {
        let Some(step) = self.fluid.as_ref().map(|fluid| fluid.step) else {
            return Ok(());
        };
        let now = self.now;
        self.fluid_advance(now);
        self.fire_fluid_boundary_triggers()?;
        // Keep the fixed step on the calendar; events past the stop time stay
        // for checkpoint/resume, the run loop stops at the stop time.
        self.schedule(now + step, EventKind::FluidStep);
        Ok(())
    }

    /// task-189: fire `on_empty`/`on_full` on the real level transitions.
    ///
    /// The latch (`empty_latched`/`full_latched`) is what keeps a level that
    /// rests on a boundary from firing every tick: the hook fires when the
    /// latch flips, and it re-arms as soon as the level leaves that boundary.
    /// No numeric dead band is introduced — a genuine crossing always fires,
    /// and a level that stays at 0/capacity cannot fire twice.
    fn fire_fluid_boundary_triggers(&mut self) -> Result<(), SimError> {
        let Some(fluid) = self.fluid.as_ref() else {
            return Ok(());
        };
        let mut fire: Vec<(usize, SimTrigger)> = Vec::new();
        for (position, state) in fluid.state.iter().enumerate() {
            let object = fluid.objects[position];
            let capacity = match &self.model.objects[object].params {
                ObjectParams::FluidTank(params) => params.capacity,
                _ => continue,
            };
            if !state.empty_latched && state.level <= EPS {
                fire.push((object, SimTrigger::OnEmpty));
            }
            // A tank that is full stops at `capacity - inflow` and spills the
            // rest, so the level alone rarely reaches the capacity exactly; a
            // fresh overflow is the reliable "became full" signal.
            let spilled = state.overflow > state.last_overflow + EPS;
            if !state.full_latched && capacity > EPS && (state.level >= capacity - EPS || spilled) {
                fire.push((object, SimTrigger::OnFull));
            }
        }
        if fire.is_empty() {
            return Ok(());
        }
        for (object, trigger) in fire {
            self.fire_fluid_trigger(object, trigger)?;
        }
        if let Some(fluid) = self.fluid.as_mut() {
            for (position, state) in fluid.state.iter_mut().enumerate() {
                let object = fluid.objects[position];
                let capacity = match &self.model.objects[object].params {
                    ObjectParams::FluidTank(params) => params.capacity,
                    _ => 0.0,
                };
                let spilled = state.overflow > state.last_overflow + EPS;
                state.empty_latched = state.level <= EPS;
                state.full_latched = capacity > EPS && (state.level >= capacity - EPS || spilled);
                state.last_overflow = state.overflow;
            }
        }
        Ok(())
    }

    /// task-189: run one fluid boundary hook; the probe item carries no volume,
    /// it only gives `item.*` paths something to evaluate.
    fn fire_fluid_trigger(&mut self, object: usize, trigger: SimTrigger) -> Result<(), SimError> {
        let actions = self.trigger_actions(object, trigger);
        if actions.is_empty() {
            return Ok(());
        }
        self.record_object_trace(trigger.as_str(), object);
        let mut item = FlowItem::probe(self.now);
        self.action_location = format!("{}.{}", self.objects[object].name, trigger.as_str());
        self.apply_actions(object, &mut item, actions)
    }

    /// Integrate one fixed fluid step ending at `now`.
    fn fluid_advance(&mut self, now: f64) {
        // Everything the step needs is cloned inside this block so the shared
        // borrow ends before the (mutating) bridge injection below.
        let (dt, count, order, links, outgoing, model_indices) = {
            let Some(fluid) = self.fluid.as_ref() else {
                return;
            };
            let dt = (now - fluid.last_time).max(0.0);
            (
                dt,
                fluid.objects.len(),
                fluid.order.clone(),
                fluid.links.clone(),
                fluid.outgoing.clone(),
                fluid.objects.clone(),
            )
        };
        if dt <= 0.0 {
            return;
        }
        let kinds: Vec<ObjectKind> = model_indices
            .iter()
            .map(|&index| self.model.objects[index].kind)
            .collect();
        let mut influx = vec![0.0f64; count];
        // Bridge objects (`item_to_fluid`) hand their converted volume to the
        // fluid sub-graph at the start of the step, so downstream objects see it
        // in the same tick.
        let bridged: Vec<(usize, f64)> = self
            .pending_bridge_volume
            .iter()
            .map(|(index, volume)| (*index, *volume))
            .collect();
        for (model_index, volume) in bridged {
            if volume <= EPS {
                continue;
            }
            // The bridge itself is not part of the fluid sub-graph, so walk the
            // model's own connections to find the fluid objects it feeds.
            let from_name = self.model.objects[model_index].name.clone();
            let targets: Vec<usize> = self
                .model
                .connections
                .iter()
                .filter(|connection| connection.from == from_name)
                .filter_map(|connection| {
                    let target = self
                        .model
                        .objects
                        .iter()
                        .position(|object| object.name == connection.to)?;
                    model_indices.iter().position(|index| *index == target)
                })
                .collect();
            if targets.is_empty() {
                continue;
            }
            let share = volume / targets.len() as f64;
            let mut sent = 0.0;
            for position in targets {
                influx[position] += share;
                sent += share;
            }
            // Downstream objects add the volume to their own inflow as they
            // process it in this same step.
            let leftover = (volume - sent).max(0.0);
            if leftover > EPS {
                self.pending_bridge_volume.insert(model_index, leftover);
            } else {
                self.pending_bridge_volume.remove(&model_index);
            }
        }
        // B5 pressure field: tank pressure from the liquid column; pipes carry
        // their inlet pressure, valves/sinks are treated as 0 Pa downstream.
        let mut pressures = vec![0.0f64; count];
        let mut inlet_pressures = vec![0.0f64; count];
        let mut incoming_links: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut link_source = vec![0usize; links.len()];
        for (from, list) in outgoing.iter().enumerate() {
            for &link in list {
                if let Some(slot) = link_source.get_mut(link) {
                    *slot = from;
                }
            }
        }
        for (index, link) in links.iter().enumerate() {
            if let Some(list) = incoming_links.get_mut(link.to) {
                list.push(index);
            }
        }
        for &position in &order {
            let upstream = incoming_links[position]
                .iter()
                .map(|&link| pressures[link_source[link]])
                .fold(0.0f64, f64::max);
            let own = match kinds[position] {
                ObjectKind::FluidTank => {
                    let (density, gravity, level) =
                        match &self.model.objects[model_indices[position]].params {
                            ObjectParams::FluidTank(params) => (
                                params.density.unwrap_or(REFERENCE_DENSITY),
                                params.gravity.unwrap_or(REFERENCE_GRAVITY),
                                self.fluid
                                    .as_ref()
                                    .map_or(0.0, |fluid| fluid.state[position].level),
                            ),
                            _ => (REFERENCE_DENSITY, REFERENCE_GRAVITY, 0.0),
                        };
                    density * gravity * level
                }
                ObjectKind::FluidPipe => upstream,
                _ => 0.0,
            };
            pressures[position] = own;
            inlet_pressures[position] = upstream;
        }
        // Acceptance capacities (backpressure) for this step.
        let model_pressure = pressures.clone();
        let mut acceptance: Vec<f64> = (0..count)
            .map(|position| match kinds[position] {
                ObjectKind::FluidSink => f64::INFINITY,
                ObjectKind::FluidTank => f64::INFINITY,
                ObjectKind::FluidValve => {
                    if self
                        .fluid
                        .as_ref()
                        .is_some_and(|fluid| fluid.state[position].valve_open)
                    {
                        f64::INFINITY
                    } else {
                        0.0
                    }
                }
                ObjectKind::FluidPipe => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidPipe(params) => params.clone(),
                        _ => {
                            return f64::INFINITY;
                        }
                    };
                    // task-153: the pipe-full limit applies in both flow
                    // modes: a pipe of known diameter can only pass its
                    // internal volume per residence time.
                    let residence = params
                        .delay
                        .unwrap_or_else(|| params.length / params.speed.max(EPS))
                        .max(EPS);
                    let volume_limit = pipe_volume(&params)
                        .map_or(f64::INFINITY, |volume| volume / residence * dt);
                    if !self.fluid.as_ref().is_some_and(|fluid| fluid.pressure_mode) {
                        let base = params.flow_rate.map_or(f64::INFINITY, |rate| rate * dt);
                        return base.min(volume_limit);
                    }
                    let (inlet, target) = match outgoing[position].first() {
                        Some(&link) => (model_pressure[position], pressures[links[link].to]),
                        None => (model_pressure[position], 0.0),
                    };
                    let delta = (inlet - target).max(0.0);
                    let temperature = params.temperature.unwrap_or(20.0);
                    let temperature_factor = 1.0 / (1.0 + 0.01 * (temperature - 20.0)).max(1e-6);
                    let rate = params.flow_rate.unwrap_or(10.0);
                    let k = rate / (REFERENCE_DENSITY * REFERENCE_GRAVITY);
                    // task-89: a pipe with diameter/friction_factor/density
                    // follows the Darcy-Weisbach law; otherwise the linear
                    // pressure-flow calibration is kept.
                    let linear = (k * delta * temperature_factor).min(rate);
                    let capacity = pipe_capacity(&params, Some(delta), linear);
                    (capacity * dt).min(volume_limit)
                }
                // The continuous objects accept up to their free capacity;
                // without this they silently refused every drop.
                ObjectKind::FluidProcessor => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidProcessor(params) => params.clone(),
                        _ => return 0.0,
                    };
                    let level = self
                        .fluid
                        .as_ref()
                        .map_or(0.0, |fluid| fluid.state[position].level);
                    (params.max_capacity - level).max(0.0)
                }
                // `fluid_to_item` takes fluid in and turns it into items, so it
                // accepts everything the upstream can give.
                ObjectKind::FluidToItem => f64::INFINITY,
                // `item_to_fluid` creates its own volume; it accepts no fluid.
                ObjectKind::ItemToFluid => 0.0,
                ObjectKind::FluidMixer => f64::INFINITY,
                ObjectKind::FluidBlender => f64::INFINITY,
                ObjectKind::FluidSplitter => f64::INFINITY,
                _ => 0.0,
            })
            .collect();
        let _ = inlet_pressures;
        // A pipe cannot accept more than its downstream can take (no infinite
        // buffering), so a closed valve also backpressures the pipe.
        let base_acceptance = acceptance.clone();
        for position in 0..count {
            if kinds[position] != ObjectKind::FluidPipe {
                continue;
            }
            let downstream: f64 = outgoing[position]
                .iter()
                .map(|&link| base_acceptance[links[link].to])
                .sum();
            acceptance[position] = acceptance[position].min(downstream);
        }

        // Time-weighted level uses the level at the start of the step.
        if let Some(fluid) = self.fluid.as_mut() {
            for state in &mut fluid.state {
                state.level_integral += state.level * dt;
                state.level_max = state.level_max.max(state.level);
                state.pressure_integral += state.pressure * dt;
            }
        }

        for &position in &order {
            match kinds[position] {
                ObjectKind::FluidGenerator => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidGenerator(params) => params.clone(),
                        _ => continue,
                    };
                    let produced = generator_volume(&params, now, dt);
                    if let Some(fluid) = self.fluid.as_mut() {
                        fluid.total_inflow += produced;
                    }
                    let sent = distribute_fluid(
                        &links,
                        &outgoing[position],
                        produced,
                        dt,
                        &mut influx,
                        &mut acceptance,
                    );
                    if let Some(fluid) = self.fluid.as_mut() {
                        fluid.state[position].throughput += sent;
                    }
                }
                ObjectKind::FluidTank => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidTank(params) => params.clone(),
                        _ => continue,
                    };
                    let level = self
                        .fluid
                        .as_ref()
                        .map_or(0.0, |fluid| fluid.state[position].level);
                    let entering = influx[position];
                    let accepted = entering;
                    let overflow = (level + entering - params.capacity).max(0.0);
                    let level_after = (level + entering).min(params.capacity);
                    // task-89: a tank releases through its outlet; the head
                    // (density * gravity * level) sets the driving pressure and a
                    // denser fluid therefore needs the same volume but carries
                    // more pressure. The released volume stays level-based; the
                    // density only changes the reported pressure and the
                    // pressure-mode downstream flow.
                    let offered = match params.max_outflow {
                        Some(rate) => level_after.min(rate * dt),
                        None => level_after,
                    };
                    let sent = distribute_fluid(
                        &links,
                        &outgoing[position],
                        offered,
                        dt,
                        &mut influx,
                        &mut acceptance,
                    );
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        state.level = (level_after - sent).max(0.0);
                        // The peak of the step is the level before releasing.
                        state.level_max = state.level_max.max(level_after);
                        state.inflow += accepted;
                        state.overflow += overflow;
                        state.throughput += sent;
                        let density = params.density.unwrap_or(REFERENCE_DENSITY);
                        let gravity = params.gravity.unwrap_or(REFERENCE_GRAVITY);
                        state.pressure = density * gravity * state.level;
                        fluid.total_overflow += overflow;
                    }
                }
                ObjectKind::FluidValve => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidValve(params) => params.clone(),
                        _ => continue,
                    };
                    let entering = influx[position];
                    let incoming_rate = if dt > EPS { entering / dt } else { 0.0 };
                    let was_open = self
                        .fluid
                        .as_ref()
                        .is_some_and(|fluid| fluid.state[position].valve_open);
                    // task-86: the effective opening combines `ratio` with the
                    // `opening`/`open_curve` characteristic when given.
                    let fraction = valve_effective_fraction(
                        params.ratio,
                        params.opening,
                        params.open_curve.as_ref(),
                    );
                    let offered = if was_open { entering * fraction } else { 0.0 };
                    let sent = distribute_fluid(
                        &links,
                        &outgoing[position],
                        offered,
                        dt,
                        &mut influx,
                        &mut acceptance,
                    );
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        state.inflow += entering;
                        state.throughput += sent;
                        state.pressure = inlet_pressures[position];
                        // Hysteresis pressure control (B5): close below
                        // pressure_close, reopen above pressure_open. Legacy
                        // pressure_threshold keeps the flow-rate latch.
                        if let (Some(open), Some(close)) =
                            (params.pressure_open, params.pressure_close)
                        {
                            let inlet = inlet_pressures[position];
                            let was_open = state.valve_open;
                            state.valve_open = if was_open {
                                params.open && inlet >= close - EPS
                            } else {
                                params.open && inlet >= open - EPS
                            };
                            if was_open != state.valve_open {
                                let label = if state.valve_open {
                                    "fluid_valve_open"
                                } else {
                                    "fluid_valve_close"
                                };
                                let index = model_indices[position];
                                self.record_object_trace(label, index);
                            }
                        } else if was_open && entering > EPS {
                            state.valve_open = params.open
                                && params
                                    .pressure_threshold
                                    .is_none_or(|threshold| incoming_rate >= threshold - EPS);
                        }
                    }
                }
                ObjectKind::FluidPipe => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidPipe(params) => params.clone(),
                        _ => continue,
                    };
                    let delay = params
                        .delay
                        .unwrap_or_else(|| params.length / params.speed.max(EPS))
                        .max(0.0);
                    // Release the volume whose travel time has elapsed; the
                    // downstream acceptance limits how much can leave, the rest
                    // stays buffered (backpressure through the pipe).
                    let mut released = 0.0;
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        let mut remaining: Vec<(f64, f64)> = Vec::new();
                        for (arrival, volume) in state.buffer.drain(..) {
                            if arrival <= now + EPS {
                                released += volume;
                            } else {
                                remaining.push((arrival, volume));
                            }
                        }
                        state.buffer = remaining;
                    }
                    let sent = distribute_fluid(
                        &links,
                        &outgoing[position],
                        released,
                        dt,
                        &mut influx,
                        &mut acceptance,
                    );
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        if released - sent > EPS {
                            state.buffer.push((now, released - sent));
                        }
                        let accepted = influx[position];
                        if accepted > 0.0 {
                            state.buffer.push((now + delay, accepted));
                        }
                        state.inflow += accepted;
                        state.throughput += sent;
                        state.level = state.buffer.iter().map(|(_, volume)| volume).sum();
                        state.level_max = state.level_max.max(state.level);
                        state.pressure = pressures[position];
                    }
                }
                ObjectKind::FluidProcessor => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidProcessor(params) => params.clone(),
                        _ => continue,
                    };
                    // Loss is applied as the material enters (FlexSim: "whenever
                    // any material enters the Processor, it is immediately
                    // reduced by this percentage").
                    let entering = influx[position];
                    let kept = entering * (1.0 - params.loss.clamp(0.0, 1.0));
                    let lost = entering - kept;
                    let level = self
                        .fluid
                        .as_ref()
                        .map_or(0.0, |fluid| fluid.state[position].level);
                    let overflow = (level + kept - params.max_capacity).max(0.0);
                    let held = (level + kept).min(params.max_capacity);
                    // The processor drains at its maximum output rate; whatever
                    // the downstream refuses stays inside and backs up.
                    let offered = held.min(params.max_output_rate * dt);
                    let sent = distribute_fluid(
                        &links,
                        &outgoing[position],
                        offered,
                        dt,
                        &mut influx,
                        &mut acceptance,
                    );
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        state.level = (held - sent).max(0.0);
                        state.level_max = state.level_max.max(held);
                        state.inflow += entering;
                        state.overflow += overflow;
                        state.throughput += sent;
                        state.pressure = inlet_pressures[position];
                        fluid.total_overflow += overflow;
                        // `lost` is tracked as overflow-like waste so the
                        // reported inflow still balances.
                        let _ = lost;
                    }
                }
                ObjectKind::FluidSplitter => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidSplitter(params) => params.clone(),
                        _ => continue,
                    };
                    let entering = influx[position];
                    let offered = if params.output_rate > 0.0 {
                        entering.min(params.output_rate * dt)
                    } else {
                        entering
                    };
                    // Percentages are kept in proportion even when the
                    // downstream cannot take everything: scale every share by
                    // the same factor (FlexSim keeps the ratios equal).
                    let shares: Vec<f64> = if params.outputs.is_empty() {
                        Vec::new()
                    } else {
                        let total: f64 = params
                            .outputs
                            .iter()
                            .map(|output| output.percent.max(0.0))
                            .sum();
                        if total <= EPS {
                            Vec::new()
                        } else {
                            params
                                .outputs
                                .iter()
                                .map(|output| output.percent.max(0.0) / total)
                                .collect()
                        }
                    };
                    let mut sent_total = 0.0;
                    if shares.is_empty() {
                        // No table: behave like a pass-through.
                        sent_total = distribute_fluid(
                            &links,
                            &outgoing[position],
                            offered,
                            dt,
                            &mut influx,
                            &mut acceptance,
                        );
                    } else {
                        for (link, share) in outgoing[position].iter().zip(shares.iter()) {
                            let target = links[*link].to;
                            let want = offered * share;
                            let room = acceptance[target];
                            let give = want.min(room);
                            if give <= EPS {
                                continue;
                            }
                            influx[target] += give;
                            acceptance[target] -= give;
                            sent_total += give;
                        }
                    }
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        let refused = (offered - sent_total).max(0.0);
                        state.inflow += entering;
                        state.throughput += sent_total;
                        state.overflow += refused;
                        state.pressure = inlet_pressures[position];
                        fluid.total_overflow += refused;
                    }
                }
                ObjectKind::FluidBlender => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidBlender(params) => params.clone(),
                        _ => continue,
                    };
                    let entering = influx[position];
                    let kept = entering * (1.0 - params.loss.clamp(0.0, 1.0));
                    let lost = entering - kept;
                    // Blending is rate based: the blend leaves at the configured
                    // rate (limited by what arrived), whatever stays inside is
                    // buffered and mixes into the next step.
                    let level = self
                        .fluid
                        .as_ref()
                        .map_or(0.0, |fluid| fluid.state[position].level);
                    let held = level + kept;
                    let offered = held.min(params.output_rate * dt);
                    let sent = distribute_fluid(
                        &links,
                        &outgoing[position],
                        offered,
                        dt,
                        &mut influx,
                        &mut acceptance,
                    );
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        state.level = (held - sent).max(0.0);
                        state.level_max = state.level_max.max(held);
                        state.inflow += entering;
                        state.throughput += sent;
                        state.overflow += lost;
                        state.pressure = inlet_pressures[position];
                        fluid.total_overflow += lost;
                    }
                }
                ObjectKind::FluidMixer => {
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidMixer(params) => params.clone(),
                        _ => continue,
                    };
                    let entering = influx[position];
                    let level = self
                        .fluid
                        .as_ref()
                        .map_or(0.0, |fluid| fluid.state[position].level);
                    let held = level + entering;
                    // The mixer works in batches: it releases nothing until the
                    // current step's pull volumes have been collected, then it
                    // waits out the step delay before the next batch.
                    // Read the batch cursor through a fresh borrow: `fluid` is
                    // still the immutable view taken at the top of the function.
                    let cursor = self
                        .fluid
                        .as_ref()
                        .map_or(0, |fluid| fluid.state[position].mixer_step);
                    let step = params
                        .steps
                        .get(cursor % params.steps.len().max(1))
                        .cloned()
                        .unwrap_or_default();
                    let needed: f64 = step.pull.iter().sum();
                    let ready = needed <= EPS || held >= needed - EPS;
                    let offered = if ready {
                        if needed <= EPS {
                            held
                        } else {
                            needed.min(held)
                        }
                    } else {
                        0.0
                    };
                    let sent = distribute_fluid(
                        &links,
                        &outgoing[position],
                        offered,
                        dt,
                        &mut influx,
                        &mut acceptance,
                    );
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        state.level = (held - sent).max(0.0);
                        state.level_max = state.level_max.max(held);
                        state.inflow += entering;
                        state.throughput += sent;
                        state.pressure = inlet_pressures[position];
                        if sent > EPS {
                            state.mixer_step = state.mixer_step.saturating_add(1);
                        }
                    }
                }
                // The bridge to the discrete side: whatever fluid arrives is
                // buffered for `flow_fluid_to_item` to pack into flow items.
                ObjectKind::FluidToItem => {
                    let received = influx[position];
                    if received > 0.0 {
                        let model_index = model_indices[position];
                        self.pending_bridge_volume
                            .entry(model_index)
                            .and_modify(|volume| *volume += received)
                            .or_insert(received);
                    }
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        state.inflow += received;
                        state.throughput += received;
                        fluid.total_throughput += received;
                    }
                    // Pack the received volume into flow items right away: the
                    // discrete side will forward them on its next step.
                    let params = match &self.model.objects[model_indices[position]].params {
                        ObjectParams::FluidToItem(params) => params.clone(),
                        _ => continue,
                    };
                    let per_item = params.fluid_per_item.max(EPS);
                    let stock = self
                        .pending_bridge_volume
                        .get(&model_indices[position])
                        .copied()
                        .unwrap_or(0.0);
                    let ready = (stock / per_item).floor().min(10_000.0);
                    if ready >= 1.0 {
                        let leftover = stock - ready * per_item;
                        if leftover > EPS {
                            self.pending_bridge_volume
                                .insert(model_indices[position], leftover);
                        } else {
                            self.pending_bridge_volume.remove(&model_indices[position]);
                        }
                        let mut made = false;
                        for _ in 0..ready as u64 {
                            let labels = std::collections::BTreeMap::new();
                            if let Ok(item) = self.create_item(0, &labels, model_indices[position])
                            {
                                let now = self.now;
                                self.objects[model_indices[position]].push_content(now, item);
                                made = true;
                            }
                        }
                        // The items must be forwarded on the discrete side, which
                        // only runs when it has an event waiting for this object.
                        if made {
                            let now = self.now;
                            self.schedule(now, EventKind::Flow(model_indices[position]));
                        }
                    }
                }
                ObjectKind::FluidSink => {
                    let received = influx[position];
                    if let Some(fluid) = self.fluid.as_mut() {
                        let state = &mut fluid.state[position];
                        state.inflow += received;
                        state.throughput += received;
                        fluid.total_throughput += received;
                    }
                }
                _ => {}
            }
        }
        if let Some(fluid) = self.fluid.as_mut() {
            fluid.last_time = now;
        }
    }

    /// Integrate the level from the last step to the stop time.
    fn finalize_fluid(&mut self) {
        let now = self.now;
        let Some(fluid) = self.fluid.as_mut() else {
            return;
        };
        let dt = (now - fluid.last_time).max(0.0);
        for state in &mut fluid.state {
            state.level_integral += state.level * dt;
            state.level_max = state.level_max.max(state.level);
        }
        fluid.last_time = now;
    }

    /// Build the reported fluid statistics of this replication.
    fn fluid_replication(&self, elapsed: f64) -> Option<ReplicationFluid> {
        let fluid = self.fluid.as_ref()?;
        let denom = elapsed.max(1e-12);
        let objects = fluid
            .objects
            .iter()
            .enumerate()
            .map(|(position, &model_index)| {
                let definition = &self.model.objects[model_index];
                let state = &fluid.state[position];
                ReplicationFluidObject {
                    name: definition.name.clone(),
                    kind: fluid_kind_name(definition.kind).to_string(),
                    level_avg: state.level_integral / denom,
                    level_max: state.level_max,
                    throughput: state.throughput / denom,
                    overflow: state.overflow,
                    pressure: state.pressure_integral / denom,
                }
            })
            .collect();
        Some(ReplicationFluid {
            objects,
            total_throughput: fluid.total_throughput / denom,
            total_overflow: fluid.total_overflow,
            total_inflow: fluid.total_inflow / denom,
        })
    }

    // -----------------------------------------------------------------------
    // Operators, dispatchers, transporters and resource pools
    // -----------------------------------------------------------------------

    /// Make sure the operators required for `phase` are held (A10). Returns
    /// `false` when the processor has to wait (its state is then `Blocked`).
    fn ensure_operator(
        &mut self,
        index: usize,
        phase: OperatorPhase,
        params: &openflex_model::ProcessorParams,
    ) -> Result<bool, SimError> {
        let needed = operators_needed(params, phase);
        if self.objects[index].operator_held >= needed {
            return Ok(true);
        }
        let name = match phase {
            OperatorPhase::Setup => params.setup_operator.clone(),
            OperatorPhase::Process => params.process_operator.clone(),
        };
        let Some(name) = name else { return Ok(true) };
        let Some(&target) = self.by_name.get(&name) else {
            // Unknown reference: no operator constraint.
            return Ok(true);
        };
        let kind = self.objects[target].kind;
        if !matches!(kind, ObjectKind::Dispatcher | ObjectKind::Resource) {
            // Not a task executer or pool: treat as always available.
            return Ok(true);
        }
        // How many operators are still missing (held + already requested)?
        let missing = needed.saturating_sub(
            self.objects[index].operator_held + self.objects[index].operators_pending,
        );
        if missing == 0 {
            return Ok(false);
        }
        self.objects[index].requesting_operator = true;
        self.objects[index].operators_pending += missing;
        // A request served by a dispatcher means the operator has to travel;
        // a pool hands a unit over immediately. The distinction feeds the
        // `waiting_for_operator` / `waiting_for_transport` statistics.
        let reason = if kind == ObjectKind::Dispatcher && !self.transporters[target].is_empty() {
            BlockReason::Transport
        } else {
            BlockReason::Operator
        };
        self.block(index, reason);
        for _ in 0..missing {
            match kind {
                ObjectKind::Dispatcher => self.objects[target].requests.push_back(index),
                ObjectKind::Resource => self.resource_request(target, index)?,
                _ => {}
            }
        }
        if kind == ObjectKind::Dispatcher {
            self.dispatch(target)?;
        }
        Ok(self.objects[index].operator_held >= needed)
    }

    /// Hand one operator to a waiting processor and let it continue.
    fn grant_operator(&mut self, index: usize, source: OperatorSource) {
        self.objects[index].operator_held += 1;
        self.objects[index].operator_sources.push(source);
        self.objects[index].operators_pending =
            self.objects[index].operators_pending.saturating_sub(1);
        self.objects[index].requesting_operator = self.objects[index].operators_pending > 0;
        // A10: an operator returned after a preemption; continue the paused
        // setup/process step where it stopped.
        if self.objects[index].operator_preempted && self.objects[index].operator_held > 0 {
            self.objects[index].operator_preempted = false;
            let _ = self.resume_phase(index);
        }
        self.schedule(self.now, EventKind::Flow(index));
    }

    /// Release every operator held by a processor.
    fn release_operator(&mut self, index: usize) -> Result<(), SimError> {
        let sources = std::mem::take(&mut self.objects[index].operator_sources);
        self.objects[index].operator_held = 0;
        self.objects[index].operators_pending = 0;
        self.objects[index].requesting_operator = false;
        for source in sources {
            match source {
                OperatorSource::Resource(resource) => {
                    if self.objects[resource].resource_in_use > 0 {
                        self.objects[resource].resource_in_use -= 1;
                    }
                    if self.objects[resource].resource_in_use == 0 {
                        self.set_state(resource, State::Idle);
                    }
                    self.grant_waiting_resource(resource)?;
                }
                OperatorSource::Transporter(transporter) => {
                    let unload_distribution = match &self.model.objects[transporter].params {
                        ObjectParams::Transporter(params) => params.unload_time.clone(),
                        _ => Distribution::Constant { value: 0.0 },
                    };
                    let unload = self.sample(&unload_distribution).max(0.0);
                    self.schedule(
                        self.now + unload,
                        EventKind::OperatorReleased(transporter, index),
                    );
                }
            }
        }
        Ok(())
    }

    /// Assign queued requests to idle transporters of a dispatcher.
    ///
    /// A10: requests are served highest `operator_priority` first; ties keep
    /// the arrival order, so a model without priorities behaves exactly like
    /// v0.1.0. A request with `operator_preempt` may interrupt a running task
    /// of a lower priority (see [`Simulation::try_preempt`]).
    fn dispatch(&mut self, dispatcher: usize) -> Result<(), SimError> {
        if !self.objects[dispatcher].on_shift {
            return Ok(());
        }
        // task-190: at most `max_tasks` pending tasks; the extra requests stay
        // queued (not dropped) and `finish_task`/preemption wake them up because
        // both call `dispatch` again.
        let max_tasks = match &self.model.objects[dispatcher].params {
            ObjectParams::Dispatcher(params) => params.max_tasks,
            _ => 0,
        };
        if max_tasks > 0 && self.objects[dispatcher].dispatcher_active >= max_tasks {
            return Ok(());
        }
        // B6: a dispatcher that only owns elevators uses floor-aware call
        // ordering (same floor, then same direction, then nearest floor).
        let elevator_only = !self.transporters[dispatcher].is_empty()
            && self.transporters[dispatcher]
                .iter()
                .all(|&executor| self.objects[executor].kind == ObjectKind::Elevator);
        if elevator_only {
            return self.dispatch_elevator(dispatcher);
        }
        while let Some(position) = self.pick_request(dispatcher) {
            let requester = self.objects[dispatcher].requests[position];
            let Some(transporter) = self.pick_transporter(dispatcher, requester) else {
                // A10: a preemptive request may interrupt a lower-priority task.
                if self.try_preempt(dispatcher, requester)? {
                    continue;
                }
                break;
            };
            self.objects[dispatcher].requests.remove(position);
            self.objects[dispatcher].dispatcher_active =
                self.objects[dispatcher].dispatcher_active.saturating_add(1);
            self.objects[transporter].tasks += 1;
            self.objects[transporter].serving_requester = Some(requester);
            if let Some(item) = self.sole_item_id(requester) {
                self.transport_item_hints.insert(transporter, item);
            }
            self.objects[transporter].task_distance =
                self.transport_estimate_distance(transporter, requester);
            self.objects[transporter].acc.input += 1;
            self.claim_shaft(transporter);
            let explicit = self
                .dispatcher_params(dispatcher)
                .is_some_and(|params| params.task_sequence_mode == TaskSequenceMode::Explicit);
            if explicit {
                // task-173: no operator transfer happens in this slice; the
                // route is the whole task. Do not count an in-transit operator.
                let route = self.explicit_route_legs(dispatcher, transporter, requester)?;
                if route.is_empty() {
                    self.finish_task(transporter)?;
                    continue;
                }
                if route.iter().any(|leg| leg.kind == PendingLeg::Unload) {
                    // The operator occupies a requester slot until the explicit
                    // unload fires OperatorLoaded.
                    self.objects[requester].in_transit += 1;
                }
                self.explicit_routes.insert(
                    transporter,
                    ExplicitState {
                        legs: route,
                        requester,
                        loaded: false,
                        operator_loaded: false,
                        waiting_release: false,
                        release_seen: false,
                        leg_in_progress: false,
                        current: None,
                        current_target: None,
                        leg_deadline: None,
                        remaining: None,
                    },
                );
                self.set_state(transporter, State::Busy);
                self.advance_explicit_route(transporter)?;
            } else {
                // A5: the operator is on its way; the destination counts it as
                // an in-transit item until it arrives.
                self.objects[requester].in_transit += 1;
                self.set_state(transporter, State::Busy);
                self.set_executor_activity(transporter, ExecutorActivity::TravelEmpty);
                // B4: the task has two legs — travel to the pickup point (the
                // transporter's home) and then to the requester.
                self.schedule_transport_pickup(transporter, requester)?;
            }
        }
        // Forget requests whose requester already got what it needed.
        let stale: Vec<usize> = self.objects[dispatcher]
            .requests
            .iter()
            .copied()
            .filter(|&requester| !self.objects[requester].requesting_operator)
            .collect();
        if !stale.is_empty() {
            self.objects[dispatcher]
                .requests
                .retain(|requester| !stale.contains(requester));
        }
        let busy = !self.objects[dispatcher].requests.is_empty();
        self.set_state(dispatcher, if busy { State::Blocked } else { State::Idle });
        Ok(())
    }

    /// B6: dispatch queued operator tasks to the elevators, preferring calls on
    /// the current floor, then the current direction, then the nearest floor.
    fn dispatch_elevator(&mut self, dispatcher: usize) -> Result<(), SimError> {
        while let Some(executor) = self.transporters[dispatcher]
            .iter()
            .copied()
            .find(|&executor| {
                self.objects[executor].tasks < self.elevator_effective_capacity(executor) as usize
                    && self.shaft_available(executor)
            })
        {
            let Some(position) = self.pick_elevator_request(dispatcher, executor) else {
                break;
            };
            let requester = self.objects[dispatcher].requests[position];
            self.objects[dispatcher].requests.remove(position);
            self.objects[executor].tasks += 1;
            self.objects[executor].serving_requester = Some(requester);
            self.objects[executor].acc.input += 1;
            self.claim_shaft(executor);
            let delta = self.objects[requester].z - self.objects[executor].z;
            self.objects[executor].elevator_direction = if delta > EPS {
                1
            } else if delta < -EPS {
                -1
            } else {
                0
            };
            self.objects[requester].in_transit += 1;
            self.set_state(executor, State::Busy);
            self.set_executor_activity(executor, ExecutorActivity::TravelEmpty);
            self.schedule_transport_pickup(executor, requester)?;
        }
        let stale: Vec<usize> = self.objects[dispatcher]
            .requests
            .iter()
            .copied()
            .filter(|&requester| !self.objects[requester].requesting_operator)
            .collect();
        if !stale.is_empty() {
            self.objects[dispatcher]
                .requests
                .retain(|requester| !stale.contains(requester));
        }
        let busy = !self.objects[dispatcher].requests.is_empty();
        self.set_state(dispatcher, if busy { State::Blocked } else { State::Idle });
        Ok(())
    }

    fn pick_elevator_request(&self, dispatcher: usize, executor: usize) -> Option<usize> {
        let current_floor = self.objects[executor].z;
        let direction = self.objects[executor].elevator_direction;
        let mut best: Option<(usize, u32, bool, bool, f64)> = None;
        for (position, &requester) in self.objects[dispatcher].requests.iter().enumerate() {
            if !self.objects[requester].requesting_operator {
                continue;
            }
            let priority = self.request_priority(requester);
            let floor = self.objects[requester].z;
            let delta = floor - current_floor;
            let same_floor = delta.abs() <= EPS;
            let same_direction = direction != 0 && !same_floor && (delta > 0.0) == (direction > 0);
            let score = (priority, same_floor, same_direction, -delta.abs());
            let better = match best {
                None => true,
                Some((_, best_priority, best_floor, best_direction, best_distance)) => {
                    score.0 > best_priority
                        || (score.0 == best_priority
                            && (score.1, score.2, score.3)
                                > (best_floor, best_direction, best_distance))
                }
            };
            if better {
                best = Some((position, priority, same_floor, same_direction, -delta.abs()));
            }
        }
        best.map(|(position, _, _, _, _)| position)
    }

    /// Position of the pending request with the highest operator priority
    /// (A10). Requests out of the queue keep their relative order.
    fn pick_request(&self, dispatcher: usize) -> Option<usize> {
        let mut best: Option<(usize, u32)> = None;
        for (position, &requester) in self.objects[dispatcher].requests.iter().enumerate() {
            if !self.objects[requester].requesting_operator {
                continue;
            }
            let priority = self
                .processor_params(requester)
                .map(|params| params.operator_priority)
                .unwrap_or(0);
            match best {
                None => best = Some((position, priority)),
                Some((_, current)) if priority > current => best = Some((position, priority)),
                _ => {}
            }
        }
        best.map(|(position, _)| position)
    }

    /// Priority of a queued operator request (A10); non-processors use 0.
    fn request_priority(&self, index: usize) -> u32 {
        self.processor_params(index)
            .map(|params| params.operator_priority)
            .unwrap_or(0)
    }

    /// Whether a request is allowed to interrupt a running task (A10).
    fn request_preempts(&self, index: usize) -> bool {
        self.processor_params(index)
            .map(|params| params.operator_preempt)
            .unwrap_or(false)
    }

    /// A10: interrupt a lower-priority transporter task for a preemptive
    /// request. Returns `true` when a transporter was freed (the dispatch loop
    /// then retries the assignment).
    ///
    /// Preemption is only modelled while the operator is actually in use (the
    /// interrupted object pauses its setup/process step and re-queues its
    /// request); tasks in transit are left alone. Only single-capacity
    /// transporters are preempted, which keeps the partial-crew case for
    /// `num_process_operators > 1` out of scope.
    fn try_preempt(&mut self, dispatcher: usize, requester: usize) -> Result<bool, SimError> {
        if !self.request_preempts(requester) {
            return Ok(false);
        }
        let new_priority = self.request_priority(requester);
        let mut victim: Option<(usize, usize)> = None;
        for &transporter in &self.transporters[dispatcher] {
            if self.objects[transporter].tasks != 1 {
                continue;
            }
            // task-175: preemption is disabled while an explicit chain is active.
            if self.explicit_routes.contains_key(&transporter) {
                continue;
            }
            let Some(low) = self.objects[transporter].serving_requester else {
                continue;
            };
            if low == requester {
                continue;
            }
            // task-65: never preempt the same request twice at one instant.
            if self.objects[low].preempt_epoch >= self.now - EPS {
                continue;
            }
            if self.request_priority(low) < new_priority {
                victim = Some((transporter, low));
                break;
            }
        }
        let Some((transporter, low)) = victim else {
            return Ok(false);
        };
        // task-65: mark the instant even if the victim no longer holds the
        // executer; otherwise the same request is picked again and again at
        // this instant.
        self.objects[low].preempt_epoch = self.now;
        self.preempt_operator(low, transporter)?;
        Ok(true)
    }

    /// Take the operator away from `index` and re-queue its request.
    fn preempt_operator(&mut self, index: usize, transporter: usize) -> Result<(), SimError> {
        let removed = self.objects[index].operator_sources.iter().position(
            |source| matches!(source, OperatorSource::Transporter(t) if *t == transporter),
        );
        let Some(position) = removed else {
            // The victim no longer holds this executer: clear the stale task so
            // the request can be served again instead of blocking the queue.
            self.objects[transporter].serving_requester = None;
            self.objects[transporter].tasks = self.objects[transporter].tasks.saturating_sub(1);
            if let Some(dispatcher) = self.objects[transporter].assigned_dispatcher {
                self.objects[dispatcher].dispatcher_active =
                    self.objects[dispatcher].dispatcher_active.saturating_sub(1);
            }
            self.set_state(transporter, State::Idle);
            return Ok(());
        };
        self.objects[index].operator_sources.remove(position);
        self.objects[index].operator_held = self.objects[index].operator_held.saturating_sub(1);
        // Abort the transporter task (it is not counted as completed).
        self.objects[transporter].serving_requester = None;
        self.objects[transporter].tasks = self.objects[transporter].tasks.saturating_sub(1);
        if let Some(dispatcher) = self.objects[transporter].assigned_dispatcher {
            self.objects[dispatcher].dispatcher_active =
                self.objects[dispatcher].dispatcher_active.saturating_sub(1);
        }
        self.set_state(transporter, State::Idle);
        // Pause the interrupted setup/process step and re-queue the request.
        self.pause_phase(index);
        self.objects[index].operator_preempted = true;
        self.objects[index].preempt_epoch = self.now;
        self.objects[index].requesting_operator = true;
        self.objects[index].operators_pending += 1;
        self.block(index, BlockReason::Transport);
        if let Some(dispatcher) = self.objects[transporter].assigned_dispatcher {
            // task-65: an optional delay before the preempted request is
            // queued again, bounded by `preempt_max_retries`.
            let (delay, max_retries) = match &self.model.objects[dispatcher].params {
                ObjectParams::Dispatcher(params) => (
                    params.preempt_retry_delay.max(0.0),
                    params.preempt_max_retries,
                ),
                _ => (0.0, 0),
            };
            self.objects[index].preempt_retries =
                self.objects[index].preempt_retries.wrapping_add(1);
            let retries = self.objects[index].preempt_retries;
            let delay_applies = delay > EPS && (max_retries == 0 || retries <= max_retries);
            if delay_applies && self.now + delay < self.config.stop_time - EPS {
                self.schedule(self.now + delay, EventKind::PreemptRetry(dispatcher, index));
            } else {
                // `pick_request` always serves the highest priority first, so
                // the queue order itself does not matter.
                self.objects[dispatcher].requests.push_back(index);
            }
        }
        Ok(())
    }

    /// task-65: queue a preempted request again once its delay elapsed.
    fn handle_preempt_retry(
        &mut self,
        dispatcher: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        if self.objects[requester].requesting_operator {
            self.objects[dispatcher].requests.push_back(requester);
            self.dispatch(dispatcher)?;
        }
        Ok(())
    }

    /// Pick an idle transporter for a request, with the travel time to the
    /// requesting object (euclidean distance / speed).
    /// B6 battery: consume the distance of the finished task and start a
    /// recharge when the pack is empty.
    fn drain_agv_battery(&mut self, transporter: usize) -> Result<(), SimError> {
        if self.objects[transporter].kind != ObjectKind::Agv {
            return Ok(());
        }
        let Some(capacity) = self.agv_battery_capacity(transporter) else {
            return Ok(());
        };
        let consumption = self.agv_consumption(transporter);
        let leg_distance = std::mem::take(&mut self.objects[transporter].task_distance);
        let battery = self.objects[transporter].battery.unwrap_or(capacity);
        let remaining = (battery - consumption * leg_distance).max(0.0);
        self.objects[transporter].battery = Some(remaining);
        if remaining <= EPS {
            // task-133: drive to the charger first (when one is configured),
            // then charge; without a charger the AGV charges in place.
            if let Some(charger) = self.charger_slot(transporter) {
                let speed = self.transport_speed(transporter).max(EPS);
                // task-136: network shortest path when the AGV has one (the
                // route starts at its home object), otherwise euclidean.
                let trip_distance = match self.objects[transporter].home {
                    // Network/eucldiean distance from the home object.
                    Some(home) => self.transport_object_distance(transporter, home, charger),
                    // No home: the straight line from the current position.
                    None => crate::engine::euclidean(
                        self.objects[transporter].position,
                        self.objects[charger].position,
                    ),
                };
                let trip = trip_distance / speed;
                if trip > EPS {
                    self.objects[transporter].charge_trip_distance = trip_distance;
                    self.objects[transporter].charging = true;
                    self.objects[transporter].charge_until = f64::INFINITY;
                    self.record_object_trace("agv_charge_travel", transporter);
                    self.schedule(self.now + trip, EventKind::ChargeTravel(transporter));
                    return Ok(());
                }
            }
            self.start_charging(transporter)?;
        }
        Ok(())
    }

    fn agv_battery_capacity(&self, transporter: usize) -> Option<f64> {
        match &self.model.objects[transporter].params {
            ObjectParams::Agv(params) => params.battery_capacity,
            _ => None,
        }
    }

    fn agv_consumption(&self, transporter: usize) -> f64 {
        match &self.model.objects[transporter].params {
            ObjectParams::Agv(params) => params.consumption_per_distance.unwrap_or(1.0).max(EPS),
            _ => 1.0,
        }
    }

    /// task-133: the AGV reached its charger; begin (or queue for) charging.
    fn handle_charge_travel(&mut self, transporter: usize) -> Result<(), SimError> {
        // task-136: the trip costs battery; the value may go negative until the
        // charger refills it.
        let distance = std::mem::take(&mut self.objects[transporter].charge_trip_distance);
        if distance > EPS {
            let consumption = self.agv_consumption(transporter);
            let battery = self.objects[transporter].battery.unwrap_or(0.0);
            self.objects[transporter].battery = Some(battery - consumption * distance);
        }
        if let Some(charger) = self.charger_slot(transporter) {
            self.objects[transporter].position = self.objects[charger].position;
            self.objects[transporter].z = self.objects[charger].z;
        }
        self.objects[transporter].charging = false;
        self.objects[transporter].charge_until = 0.0;
        self.start_charging(transporter)
    }

    /// task-106/task-148: the charger an AGV drives to.
    ///
    /// The primary `charger` is the preferred station; `chargers` adds
    /// alternatives. The engine keeps the nearest reachable station that has a
    /// free slot (falling back to the primary when all are busy, so the AGV
    /// still queues instead of charging in place).
    fn charger_slot(&self, transporter: usize) -> Option<usize> {
        let (primary, alternatives) = match &self.model.objects[transporter].params {
            ObjectParams::Agv(params) => (
                params
                    .charger
                    .as_ref()
                    .and_then(|name| self.by_name.get(name).copied()),
                params.chargers.clone(),
            ),
            _ => (None, Vec::new()),
        };
        let mut candidates: Vec<usize> = Vec::new();
        if let Some(primary) = primary {
            candidates.push(primary);
        }
        for name in alternatives {
            if let Some(index) = self.by_name.get(&name).copied()
                && !candidates.contains(&index)
            {
                candidates.push(index);
            }
        }
        if candidates.len() <= 1 {
            return candidates.first().copied();
        }
        let position = self.objects[transporter].position;
        let mut best: Option<(usize, f64)> = None;
        for charger in &candidates {
            let capacity = self.charger_capacity(*charger);
            let in_use = self.charger_in_use.get(charger).copied().unwrap_or(0);
            let free = in_use < capacity;
            let target = self.objects[*charger].position;
            let distance = (target[0] - position[0]).hypot(target[1] - position[1]);
            // Free stations always win over busy ones; among equals the nearest.
            let score = distance + if free { 0.0 } else { 1e6 };
            if best.is_none_or(|(_, best_score)| score < best_score) {
                best = Some((*charger, score));
            }
        }
        best.map(|(charger, _)| charger)
    }

    fn charger_capacity(&self, charger: usize) -> u32 {
        match &self.model.objects[charger].params {
            ObjectParams::Resource(params) => params.capacity.max(1),
            _ => self.objects[charger].capacity.max(1),
        }
    }

    fn start_charging(&mut self, transporter: usize) -> Result<(), SimError> {
        if self.objects[transporter].charging {
            return Ok(());
        }
        // task-106: with a charger object the AGV takes a slot; without one it
        // charges in place (the historical behaviour).
        if let Some(charger) = self.charger_slot(transporter) {
            let capacity = self.charger_capacity(charger);
            let in_use = self.charger_in_use.get(&charger).copied().unwrap_or(0);
            if in_use >= capacity {
                // task-149: remember when the AGV started waiting so the
                // `longest_waiting` policy can order the queue.
                self.objects[transporter].charge_queue_since = Some(self.now);
                self.charger_queue
                    .entry(charger)
                    .or_default()
                    .push_back(transporter);
                self.objects[transporter].charging = true;
                self.objects[transporter].charge_until = f64::INFINITY;
                self.record_object_trace("agv_charge_wait", transporter);
                return Ok(());
            }
            *self.charger_in_use.entry(charger).or_insert(0) += 1;
            self.record_object_trace("agv_charge_start", transporter);
        }
        let Some(capacity) = self.agv_battery_capacity(transporter) else {
            return Ok(());
        };
        let distribution = match &self.model.objects[transporter].params {
            ObjectParams::Agv(params) => params.charge_time.clone(),
            _ => Distribution::Constant { value: 0.0 },
        };
        let charge = self.sample(&distribution).max(0.0);
        self.objects[transporter].charging = true;
        self.objects[transporter].charge_started_at = self.now;
        self.objects[transporter].charge_until = self.now + charge;
        self.record_object_trace("agv_charge", transporter);
        if charge <= EPS {
            self.objects[transporter].battery = Some(capacity);
            self.objects[transporter].charging = false;
        } else {
            self.schedule(self.now + charge, EventKind::ChargeDone(transporter));
        }
        Ok(())
    }

    fn handle_charge_done(&mut self, transporter: usize) -> Result<(), SimError> {
        if let Some(capacity) = self.agv_battery_capacity(transporter) {
            self.objects[transporter].battery = Some(capacity);
        }
        self.objects[transporter].charging = false;
        self.objects[transporter].charge_until = 0.0;
        if self.objects[transporter].charge_started_at > 0.0 {
            let charged = (self.now - self.objects[transporter].charge_started_at).max(0.0);
            self.objects[transporter].charge_time_total += charged;
            self.objects[transporter].charge_started_at = 0.0;
        }
        // task-106: free the charger slot and wake the next waiting AGV.
        if let Some(charger) = self.charger_slot(transporter) {
            if let Some(slot) = self.charger_in_use.get_mut(&charger) {
                *slot = slot.saturating_sub(1);
            }
            // task-149: pick the next AGV by the charger queue policy of the
            // AGV that just finished (FIFO by default).
            let order = self.agv_queue_order(transporter);
            let next = self.pop_charger_queue(charger, order);
            if let Some(next) = next {
                self.objects[next].charging = false;
                self.objects[next].charge_until = 0.0;
                self.start_charging(next)?;
            }
        }
        if let Some(dispatcher) = self.objects[transporter].assigned_dispatcher {
            self.dispatch(dispatcher)?;
        }
        Ok(())
    }

    /// task-149: the queue policy configured on an AGV.
    fn agv_queue_order(&self, transporter: usize) -> openflex_model::ChargeQueueOrder {
        match &self.model.objects[transporter].params {
            ObjectParams::Agv(params) => params.charge_queue_order,
            _ => openflex_model::ChargeQueueOrder::Fifo,
        }
    }

    /// Removes and returns the next AGV of a charger queue under `order`.
    fn pop_charger_queue(
        &mut self,
        charger: usize,
        order: openflex_model::ChargeQueueOrder,
    ) -> Option<usize> {
        let queue = self.charger_queue.get_mut(&charger)?;
        match order {
            openflex_model::ChargeQueueOrder::Fifo => queue.pop_front(),
            openflex_model::ChargeQueueOrder::LowestBattery => {
                let best = queue
                    .iter()
                    .enumerate()
                    .min_by(|(_, left), (_, right)| {
                        let left_battery = self.objects[**left].battery.unwrap_or(f64::INFINITY);
                        let right_battery = self.objects[**right].battery.unwrap_or(f64::INFINITY);
                        left_battery.total_cmp(&right_battery)
                    })
                    .map(|(index, _)| index)?;
                queue.remove(best)
            }
            openflex_model::ChargeQueueOrder::LongestWaiting => {
                let best = queue
                    .iter()
                    .enumerate()
                    .min_by(|(_, left), (_, right)| {
                        let left_since = self.objects[**left]
                            .charge_queue_since
                            .unwrap_or(f64::INFINITY);
                        let right_since = self.objects[**right]
                            .charge_queue_since
                            .unwrap_or(f64::INFINITY);
                        left_since.total_cmp(&right_since)
                    })
                    .map(|(index, _)| index)?;
                queue.remove(best)
            }
        }
    }

    fn record_object_trace(&mut self, label: &str, index: usize) {
        if self.trace.is_none() {
            return;
        }
        let name = self.objects[index].name.clone();
        let time = self.now;
        let seq = self.next_seq;
        self.next_seq += 1;
        if let Some(log) = self.trace.as_mut() {
            log.push(TraceEvent::new(time, seq, label, name));
        }
    }

    fn pick_transporter(&self, dispatcher: usize, requester: usize) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for &transporter in &self.transporters[dispatcher] {
            if !self.objects[transporter].on_shift {
                continue;
            }
            let capacity = if self.objects[transporter].kind == ObjectKind::Elevator {
                self.elevator_effective_capacity(transporter)
            } else {
                self.objects[transporter].capacity
            };
            if self.objects[transporter].tasks >= capacity as usize {
                continue;
            }
            if !self.shaft_available(transporter) {
                continue;
            }
            if self.objects[transporter].charging {
                continue;
            }
            let travel = self.transport_estimate_travel(transporter, requester);
            if best.is_none_or(|(_, current)| travel < current) {
                best = Some((transporter, travel));
            }
        }
        best.map(|(transporter, _)| transporter)
    }

    /// Travel time from the transporter's current position to its pickup point
    /// (its `home`, or the current position when no home is configured).
    fn transport_is_elevator(&self, transporter: usize) -> bool {
        self.objects[transporter].kind == ObjectKind::Elevator
    }

    fn elevator_params(&self, index: usize) -> Option<openflex_model::ElevatorParams> {
        match &self.model.objects[index].params {
            ObjectParams::Elevator(params) => Some(params.clone()),
            _ => None,
        }
    }

    /// Task-36: the cabin capacity, limited by `max_load` / `load_per_task`.
    fn elevator_effective_capacity(&self, index: usize) -> u32 {
        let capacity = self.objects[index].capacity.max(1);
        let Some(params) = self.elevator_params(index) else {
            return capacity;
        };
        if params.max_load <= 0.0 || params.load_per_task <= 0.0 {
            return capacity;
        }
        let by_load = (params.max_load / params.load_per_task).floor().max(0.0) as u32;
        capacity.min(by_load)
    }

    fn elevator_shaft(&self, index: usize) -> Option<String> {
        self.elevator_params(index)?.shaft
    }

    /// Task-36: a shaft admits one cabin at a time.
    fn shaft_available(&self, index: usize) -> bool {
        let Some(shaft) = self.elevator_shaft(index) else {
            return true;
        };
        self.shaft_owner
            .get(&shaft)
            .is_none_or(|&owner| owner == index)
    }

    fn claim_shaft(&mut self, index: usize) {
        if let Some(shaft) = self.elevator_shaft(index) {
            self.shaft_owner.insert(shaft, index);
        }
    }

    fn release_shaft(&mut self, index: usize) {
        if let Some(shaft) = self.elevator_shaft(index)
            && self.shaft_owner.get(&shaft) == Some(&index)
        {
            self.shaft_owner.remove(&shaft);
        }
    }

    fn elevator_door_open(&self, index: usize) -> f64 {
        self.elevator_params(index)
            .map(|params| params.door_open_time.max(0.0))
            .unwrap_or(0.0)
    }

    fn elevator_door_close(&self, index: usize) -> f64 {
        self.elevator_params(index)
            .map(|params| params.door_close_time.max(0.0))
            .unwrap_or(0.0)
    }

    /// task-148: schedule the first leg (current position -> pickup home),
    /// waiting when the controlled area is full. Stage 0 of the retry event.
    fn schedule_transport_pickup(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        if self.objects[transporter].state == State::Blocked {
            self.set_state(transporter, State::Busy);
        }
        self.traffic_release(transporter);
        self.set_executor_activity(transporter, ExecutorActivity::TravelEmpty);
        if let Some(home) = self.objects[transporter].home {
            let from = self.objects[transporter].position;
            let to = self.objects[home].position;
            let controls = self.traffic_controls_for_leg(from, to);
            if !self.traffic_try_acquire(transporter, &controls)? {
                // Blocked by the control area: that interval is `blocked`, not
                // travel (task-166).
                self.clear_executor_activity(transporter);
                self.set_state(transporter, State::Blocked);
                self.traffic_wait_for_slot(transporter, requester, 0, &controls);
                return Ok(());
            }
            let speed_scale = self.traffic_speed_factor(from, to).max(EPS);
            let travel = self.transport_pickup_travel(transporter) / speed_scale;
            self.schedule(
                self.now + travel,
                EventKind::TransportPickup(transporter, requester),
            );
        } else {
            self.schedule(self.now, EventKind::TransportPickup(transporter, requester));
        }
        Ok(())
    }

    /// task-148: the nearest `network_node` name to a layout position.
    fn nearest_network_node(&self, position: [f64; 2]) -> Option<&str> {
        self.model
            .objects
            .iter()
            .filter(|object| object.kind == ObjectKind::NetworkNode)
            .min_by(|left, right| {
                distance(left.pos, position)
                    .partial_cmp(&distance(right.pos, position))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|object| object.name.as_str())
    }

    /// task-148: mutual-exclusion areas crossed by a leg. A leg belongs to an
    /// area when both endpoint objects are nearest to member nodes; the model
    /// semantics are "between two member nodes" of the same area.
    fn traffic_controls_for_leg(&self, from: [f64; 2], to: [f64; 2]) -> Vec<usize> {
        if self.traffic_controls.is_empty() {
            return Vec::new();
        }
        let Some(from_node) = self.nearest_network_node(from) else {
            return Vec::new();
        };
        let Some(to_node) = self.nearest_network_node(to) else {
            return Vec::new();
        };
        self.traffic_controls
            .iter()
            .enumerate()
            .filter(|(_, control)| {
                control.mutual_exclusion
                    && control.members.contains(from_node)
                    && control.members.contains(to_node)
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// task-148: speed multiplier for a leg crossing controlled areas (both
    /// modes apply it; `un_timed` only applies this factor).
    fn traffic_speed_factor(&self, from: [f64; 2], to: [f64; 2]) -> f64 {
        let (Some(from_node), Some(to_node)) = (
            self.nearest_network_node(from),
            self.nearest_network_node(to),
        ) else {
            return 1.0;
        };
        self.traffic_controls
            .iter()
            .filter(|control| {
                control.members.contains(from_node) && control.members.contains(to_node)
            })
            .map(|control| control.speed_factor)
            .product::<f64>()
            .max(EPS)
    }

    /// task-148: acquire one slot in every listed area, all-or-nothing.
    fn traffic_try_acquire(
        &mut self,
        transporter: usize,
        controls: &[usize],
    ) -> Result<bool, SimError> {
        if controls.is_empty() {
            return Ok(true);
        }
        // task-189: `on_entry_request` may refuse this attempt before the
        // capacity check; the caller then waits and retries.
        if !self.traffic_entry_request(transporter)? {
            return Ok(false);
        }
        if controls.iter().any(|&control| {
            self.traffic_occupancy[control] >= self.traffic_controls[control].capacity
        }) {
            return Ok(false);
        }
        for &control in controls {
            self.traffic_occupancy[control] += 1;
        }
        self.objects[transporter].traffic_hold = controls.to_vec();
        self.objects[transporter].traffic_wait_stage = None;
        self.objects[transporter].traffic_wait_controls.clear();
        Ok(true)
    }

    /// task-189: run the executer's `on_entry_request` hook.
    ///
    /// The hook receives a probe item (`FlowItem::probe`) because a traffic
    /// request has no flow item of its own. **Protocol**: if the hook leaves
    /// `label("admit")` at `0.0`, this attempt is refused and the caller retries
    /// later; a missing label (or any other value) keeps the normal acquire
    /// path. This is an OpenFlex protocol, not a FlexSim public API.
    fn traffic_entry_request(&mut self, executer: usize) -> Result<bool, SimError> {
        let actions = self.trigger_actions(executer, SimTrigger::OnEntryRequest);
        if actions.is_empty() {
            return Ok(true);
        }
        let mut item = FlowItem::probe(self.now);
        self.action_location = format!("{}.on_entry_request", self.objects[executer].name);
        self.apply_actions(executer, &mut item, actions)?;
        Ok(item.labels.get("admit").copied().unwrap_or(1.0) != 0.0)
    }

    /// task-148/154: release every area held by an executer (idempotent) and
    /// wake the first waiter of each released area immediately.
    fn traffic_release(&mut self, transporter: usize) {
        let holds = std::mem::take(&mut self.objects[transporter].traffic_hold);
        for control in holds {
            if let Some(slot) = self.traffic_occupancy.get_mut(control) {
                *slot = slot.saturating_sub(1);
            }
            let waiters = {
                let Some(queue) = self.traffic_waiters.get_mut(control) else {
                    continue;
                };
                let mut found = Vec::new();
                while let Some(candidate) = queue.pop_front() {
                    if self.objects[candidate].traffic_wait_stage.is_some()
                        && !found.contains(&candidate)
                    {
                        found.push(candidate);
                    }
                }
                found
            };
            // Wake every waiter of the released area: a waiter that also needs
            // another area re-queues itself if that one is still full, so this
            // cannot dead-lock and needs no polling.
            for candidate in waiters {
                let stage = self.objects[candidate].traffic_wait_stage.unwrap_or(0);
                let requester = self.objects[candidate]
                    .serving_requester
                    .unwrap_or(candidate);
                self.schedule(
                    self.now,
                    EventKind::TrafficWait(candidate, requester, stage),
                );
            }
        }
    }

    /// task-154: register a blocked leg in the control queues. A released slot
    /// wakes the queue head immediately; one 0.1 s fallback tick guards against
    /// a release that happened before this waiter was registered. Re-registering
    /// after a failed wake-up (multi-area legs) re-enqueues without polling.
    fn traffic_wait_for_slot(
        &mut self,
        transporter: usize,
        requester: usize,
        stage: u8,
        controls: &[usize],
    ) {
        let first_wait = self.objects[transporter].traffic_wait_stage != Some(stage);
        if first_wait {
            self.objects[transporter].traffic_wait_stage = Some(stage);
            self.objects[transporter].traffic_wait_controls = controls.to_vec();
        }
        for &control in controls {
            if let Some(queue) = self.traffic_waiters.get_mut(control) {
                // The queue may already contain this executer from an earlier
                // wake-up attempt; stale entries are skipped by the stage check.
                queue.push_back(transporter);
            }
        }
        if first_wait {
            // One fallback tick only: it covers a release that happened just
            // before this waiter was registered. After that the directed
            // wake-up owns the retry, so no 0.1 s polling storm can form.
            self.schedule(
                self.now + TRAFFIC_RETRY_DELAY,
                EventKind::TrafficWait(transporter, requester, stage),
            );
        }
    }

    fn transport_pickup_travel(&self, transporter: usize) -> f64 {
        let speed = self.transport_speed(transporter);
        let Some(home) = self.objects[transporter].home else {
            return 0.0;
        };
        if self.transport_is_elevator(transporter) {
            (self.objects[transporter].z - self.objects[home].z).abs() / speed
        } else if self.objects[transporter].kind == ObjectKind::Person
            && let Some(grid) = self.model.grid.as_ref()
            && let Some(distance) = grid_distance(
                grid,
                self.objects[transporter].position,
                self.objects[home].position,
            )
        {
            distance / speed
        } else {
            distance(
                self.objects[transporter].position,
                self.objects[home].position,
            ) / speed
        }
    }

    /// B6: dwell time at a stop (elevator doors); zero for other executors.
    fn transport_dwell(&mut self, transporter: usize) -> f64 {
        let distribution = match &self.model.objects[transporter].params {
            ObjectParams::Elevator(params) => params.stop_time.clone(),
            _ => return 0.0,
        };
        self.sample(&distribution).max(0.0)
    }

    fn transport_load_time(&mut self, transporter: usize) -> f64 {
        let distribution = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params)
            | ObjectParams::Person(params)
            | ObjectParams::Robot(params)
            | ObjectParams::Crane(params)
            | ObjectParams::AsrsVehicle(params) => params.load_time.clone(),
            ObjectParams::Agv(params) => params.load_time.clone(),
            _ => Distribution::Constant { value: 0.0 },
        };
        self.sample(&distribution).max(0.0)
    }

    fn transport_unload_time(&mut self, transporter: usize) -> f64 {
        let distribution = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params)
            | ObjectParams::Person(params)
            | ObjectParams::Robot(params)
            | ObjectParams::Crane(params)
            | ObjectParams::AsrsVehicle(params) => params.unload_time.clone(),
            ObjectParams::Agv(params) => params.unload_time.clone(),
            _ => Distribution::Constant { value: 0.0 },
        };
        self.sample(&distribution).max(0.0)
    }

    /// B4/B6: first task leg — travel to the pickup point (the executor's
    /// `home`), load the operator there and travel to the requester.
    /// task-124: whether this executer moves cell by cell on the grid.
    fn person_grid_step(&self, index: usize) -> bool {
        let flag = match &self.model.objects[index].params {
            ObjectParams::Person(params)
            | ObjectParams::Transporter(params)
            | ObjectParams::Robot(params) => params.grid_step,
            _ => None,
        };
        flag == Some(true)
    }

    /// task-124: duration of one grid cell for a walking person.
    fn person_step_duration(&self, transporter: usize) -> f64 {
        let size = self
            .model
            .grid
            .as_ref()
            .map(|grid| grid.cell_size)
            .unwrap_or(1.0)
            .max(EPS);
        let factor = self.person_density_factor(transporter).max(EPS);
        size / (self.transport_speed(transporter) * factor).max(EPS)
    }

    /// task-124: arm the next cell (or finish the move).
    fn schedule_person_step(&mut self, transporter: usize) -> Result<(), SimError> {
        if self.objects[transporter].person_path.is_empty() {
            self.objects[transporter].person_reserved.clear();
            if let Some(requester) = self.objects[transporter].person_target.take() {
                // Snap onto the exact (sub-cell) position of the requester.
                self.objects[transporter].position = self.objects[requester].position;
                self.objects[transporter].z = self.objects[requester].z;
                return self.handle_transport_arrive(transporter, requester);
            }
            if self.objects[transporter].person_returning {
                // task-150: the walk home finished; snap onto the home object
                // and settle the task like the single-leg path does.
                self.objects[transporter].person_returning = false;
                if let Some(home) = self.objects[transporter].home {
                    self.objects[transporter].position = self.objects[home].position;
                    self.objects[transporter].z = self.objects[home].z;
                }
                return self.settle_task(transporter);
            }
            return Ok(());
        }
        let step = self.person_step_duration(transporter);
        self.schedule(self.now + step, EventKind::PersonStep(transporter));
        Ok(())
    }

    /// task-124/128: one cell of a step-by-step person move.
    /// task-152: refresh this person's forward reservation window.
    fn refresh_person_reservations(&mut self, transporter: usize) {
        let window = self.person_reservation_window(transporter);
        let path: Vec<[f64; 2]> = self.objects[transporter]
            .person_path
            .iter()
            .take(window)
            .copied()
            .collect();
        self.objects[transporter].person_reserved = path;
    }

    /// task-152: how many cells ahead a person reserves (per person params).
    fn person_reservation_window(&self, transporter: usize) -> usize {
        match &self.model.objects[transporter].params {
            ObjectParams::Person(params) => params.grid_reserve as usize,
            ObjectParams::Robot(params) => params.grid_reserve as usize,
            _ => 0,
        }
    }

    fn handle_person_step(&mut self, transporter: usize) -> Result<(), SimError> {
        // task-128: the next cell may be taken by a lower-index person; wait a
        // tick instead of overlapping (the lower index has priority, which also
        // breaks head-on deadlocks).
        if let Some(&next) = self.objects[transporter].person_path.front() {
            // task-152: a cell is taken when a lower-index person stands on it
            // **or has reserved it ahead**. Reservations therefore make people
            // yield early instead of colliding at the next cell.
            let blocked = self.objects.iter().enumerate().any(|(index, object)| {
                index != transporter
                    && index < transporter
                    && object.kind == ObjectKind::Person
                    && object.tasks > 0
                    && (distance(object.position, next) < 0.5
                        || object
                            .person_reserved
                            .iter()
                            .any(|cell| distance(*cell, next) < 0.5))
            });
            if blocked {
                self.record_object_trace("person_wait", transporter);
                self.schedule(self.now + 0.5, EventKind::PersonStep(transporter));
                return Ok(());
            }
        }
        if let Some(next) = self.objects[transporter].person_path.pop_front() {
            self.objects[transporter].position = next;
        }
        self.refresh_person_reservations(transporter);
        self.schedule_person_step(transporter)
    }

    fn handle_transport_pickup(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        // task-124: people with `grid_step` walk cell by cell on the grid.
        if self.person_grid_step(transporter)
            && self.objects[transporter].kind == ObjectKind::Person
            && let Some(grid) = self.model.grid.clone()
        {
            let from = self.objects[transporter]
                .home
                .map(|home| self.objects[home].position)
                .unwrap_or(self.objects[transporter].position);
            let to = self.objects[requester].position;
            if let Some(path) = grid_path(&grid, from, to) {
                let size = grid.cell_size.max(EPS);
                self.objects[transporter].person_path = path
                    .into_iter()
                    .skip(1)
                    .map(|[x, y]| [(x as f64 + 0.5) * size, (y as f64 + 0.5) * size])
                    .collect();
                self.objects[transporter].person_target = Some(requester);
                self.objects[transporter].position = from;
                self.refresh_person_reservations(transporter);
                return self.schedule_person_step(transporter);
            }
        }
        // task-112: a person cannot step onto a cell already taken by a
        // lower-index person; it waits one tick and retries the pickup leg.
        if self.objects[transporter].kind == ObjectKind::Person
            && let Some(home) = self.objects[transporter].home
        {
            let target = self.objects[home].position;
            let blocked = self.objects.iter().enumerate().any(|(index, object)| {
                index != transporter
                    && index < transporter
                    && object.kind == ObjectKind::Person
                    && object.tasks > 0
                    && distance(object.position, target) < 0.5
            });
            if blocked {
                self.record_object_trace("person_wait", transporter);
                self.schedule(
                    self.now + 0.5,
                    EventKind::TransportPickup(transporter, requester),
                );
                return Ok(());
            }
        }
        if let Some(home) = self.objects[transporter].home {
            self.objects[transporter].position = self.objects[home].position;
            self.objects[transporter].z = self.objects[home].z;
        }
        if self.objects[transporter].state == State::Blocked {
            self.set_state(transporter, State::Busy);
        }
        self.traffic_release(transporter);
        self.set_executor_activity(transporter, ExecutorActivity::Loading);
        let from_position = self.objects[transporter].position;
        let to_position = self.objects[requester].position;
        let controls = self.traffic_controls_for_leg(from_position, to_position);
        if !self.traffic_try_acquire(transporter, &controls)? {
            self.clear_executor_activity(transporter);
            self.set_state(transporter, State::Blocked);
            self.traffic_wait_for_slot(transporter, requester, 1, &controls);
            return Ok(());
        }
        let dwell = self.transport_dwell(transporter);
        let load = self.transport_load_time(transporter);
        let pickup_distance =
            self.transport_pickup_travel(transporter) * self.transport_speed(transporter);
        if self.agv_mid_task_charge(transporter, requester, pickup_distance, 0)? {
            return Ok(());
        }
        // Loading ends (and the loaded delivery leg begins) after the dwell and
        // the load time; `TransportDepart` switches the sub-state there.
        self.schedule(
            self.now + dwell + load,
            EventKind::TransportDepart(transporter, requester),
        );
        Ok(())
    }

    /// task-166: the loaded delivery leg begins; this is where
    /// `travel_loaded` starts (loading ended at the previous event).
    /// The transport task finished loading and starts the loaded delivery leg.
    ///
    /// task-188: this is the exact `now + dwell + load` instant, so it also
    /// fires the executer's `on_load` trigger. No extra event is inserted: the
    /// `TransportDepart` event already *is* the loading-finished instant, so the
    /// event sequence (and every statistic) is unchanged.
    fn handle_transport_depart(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        self.fire_executer_trigger(transporter, requester, SimTrigger::OnLoad)?;
        self.set_state(transporter, State::Busy);
        self.set_executor_activity(transporter, ExecutorActivity::TravelLoaded);
        let from_position = self.objects[transporter].position;
        let to_position = self.objects[requester].position;
        let speed_scale = self
            .traffic_speed_factor(from_position, to_position)
            .max(EPS);
        let travel = self.transport_leg_travel(transporter, requester) / speed_scale;
        self.schedule(
            self.now + travel,
            EventKind::TransportArrive(transporter, requester),
        );
        Ok(())
    }

    /// B6/B29: consume battery for a leg; when empty, stop in place and charge,
    /// then resume the pickup leg (no teleporting to the charger).
    fn agv_mid_task_charge(
        &mut self,
        transporter: usize,
        requester: usize,
        distance: f64,
        stage: u8,
    ) -> Result<bool, SimError> {
        if self.objects[transporter].kind != ObjectKind::Agv {
            return Ok(false);
        }
        let Some(capacity) = self.agv_battery_capacity(transporter) else {
            return Ok(false);
        };
        let consumption = self.agv_consumption(transporter);
        let needed = consumption * distance;
        let battery = self.objects[transporter].battery.unwrap_or(capacity);
        if battery >= needed - EPS {
            self.objects[transporter].battery = Some((battery - needed).max(0.0));
            self.objects[transporter].task_distance =
                (self.objects[transporter].task_distance - distance).max(0.0);
            return Ok(false);
        }
        // task-143: a full battery that still cannot cover the leg must not
        // loop (charge -> resume -> charge). Drain it and let the task go on.
        if battery >= capacity - EPS {
            self.objects[transporter].battery = Some(0.0);
            self.objects[transporter].task_distance =
                (self.objects[transporter].task_distance - distance).max(0.0);
            return Ok(false);
        }
        self.objects[transporter].battery = Some(0.0);
        // task-166: charging mid-task is classified as on-site task time
        // (`utilize`); the existing charging fraction remains the authoritative
        // battery statistic.
        self.set_executor_activity(transporter, ExecutorActivity::Utilize);
        self.record_object_trace("agv_charge", transporter);
        let distribution = match &self.model.objects[transporter].params {
            ObjectParams::Agv(params) => params.charge_time.clone(),
            _ => Distribution::Constant { value: 0.0 },
        };
        let charge = self.sample(&distribution).max(0.0);
        self.schedule(
            self.now + charge,
            EventKind::TransportResume(transporter, requester, stage),
        );
        Ok(true)
    }

    fn handle_transport_resume(
        &mut self,
        transporter: usize,
        requester: usize,
        stage: u8,
    ) -> Result<(), SimError> {
        if let Some(capacity) = self.agv_battery_capacity(transporter) {
            self.objects[transporter].battery = Some(capacity);
        }
        match stage {
            1 => self.handle_transport_arrive(transporter, requester),
            2 => self.handle_transport_home(transporter),
            _ => self.handle_transport_pickup(transporter, requester),
        }
    }

    fn handle_transport_arrive(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        // task-148: the controlled delivery leg ends when the executer arrives.
        self.traffic_release(transporter);
        if self.objects[transporter].kind == ObjectKind::Agv {
            // task-151: a node occupied by another AGV is a wait, but a ring of
            // AGVs can all wait on each other forever. After a few retries the
            // AGV with the largest index stops claiming priority, so one of them
            // always makes progress.
            // Only AGVs with a *lower* index claim the node, so two AGVs that
            // block each other cannot both wait: the higher index always moves.
            // After three retries the wait is escalated and the node is entered
            // anyway — a ring of AGVs must never dead-lock the run.
            let rounds = self.objects[transporter].agv_wait_rounds;
            let escalated = rounds >= 3;
            let busy_here = !escalated
                && self.objects.iter().enumerate().any(|(index, object)| {
                    index != transporter
                        && object.kind == ObjectKind::Agv
                        && object.tasks > 0
                        && index < transporter
                        && distance(object.position, self.objects[requester].position) <= EPS
                });
            if busy_here {
                // Node occupied: queue outside the node and retry (avoids
                // entering the same station simultaneously / head-on deadlocks).
                self.objects[transporter].agv_wait_rounds = rounds.saturating_add(1);
                self.record_object_trace("agv_wait", transporter);
                self.schedule(
                    self.now + 0.5,
                    EventKind::TransportArrive(transporter, requester),
                );
                return Ok(());
            }
            self.objects[transporter].agv_wait_rounds = 0;
        }
        // task-109: two people cannot stand on the same cell. The lower object
        // index has priority; the other one waits one tick and retries, which
        // also breaks head-on deadlocks.
        if self.objects[transporter].kind == ObjectKind::Person {
            let target = self.objects[requester].position;
            let occupied_by_smaller = self.objects.iter().enumerate().any(|(index, object)| {
                index != transporter
                    && index < transporter
                    && object.kind == ObjectKind::Person
                    && object.tasks > 0
                    && distance(object.position, target) < 0.5
            });
            if occupied_by_smaller {
                self.record_object_trace("person_wait", transporter);
                self.schedule(
                    self.now + 0.5,
                    EventKind::TransportArrive(transporter, requester),
                );
                return Ok(());
            }
        }
        self.objects[transporter].position = self.objects[requester].position;
        self.objects[transporter].z = self.objects[requester].z;
        self.set_executor_activity(transporter, ExecutorActivity::Unloading);
        let leg_distance =
            self.transport_leg_travel(transporter, requester) * self.transport_speed(transporter);
        if self.agv_mid_task_charge(transporter, requester, leg_distance, 1)? {
            return Ok(());
        }
        let dwell = self.transport_dwell(transporter);
        let unload = self.transport_unload_time(transporter);
        // Task-36: the doors have to open before loading/unloading starts.
        let door_open = self.elevator_door_open(transporter);
        // task-103: door state statistics.
        self.objects[transporter].acc.detail.door_opening += door_open;
        self.schedule(
            self.now + door_open + dwell + unload,
            EventKind::OperatorLoaded(transporter, requester),
        );
        Ok(())
    }

    /// B4/B6: shortest distance for one operational leg of a transport task.
    ///
    /// With a `network` the leg uses the shortest path between the two objects;
    /// without one it falls back to the straight-line distance. The elevator
    /// family travels vertically between object heights (`z`) only.
    fn transport_object_distance(&self, transporter: usize, from: usize, to: usize) -> f64 {
        // task-56: shortest-path planning is the expensive part of a task; the
        // same (executer, from, to) triple always answers the same distance.
        let key = (transporter, from, to);
        if let Some(cached) = self.distance_cache.borrow().get(&key) {
            return *cached;
        }
        let distance = self.compute_object_distance(transporter, from, to);
        self.distance_cache.borrow_mut().insert(key, distance);
        distance
    }

    fn compute_object_distance(&self, transporter: usize, from: usize, to: usize) -> f64 {
        if self.transport_is_elevator(transporter) {
            return (self.objects[from].z - self.objects[to].z).abs();
        }
        match self.objects[transporter].kind {
            // task-50/53: a robot plans on the walking grid (8 neighbours,
            // obstacles) and adds the height difference; without a grid it
            // falls back to the straight 3D line.
            ObjectKind::Robot => {
                let dz = (self.objects[to].z - self.objects[from].z).abs();
                if let Some(grid) = self.model.grid.as_ref()
                    && let Some(flat) =
                        grid_distance(grid, self.objects[from].position, self.objects[to].position)
                {
                    return flat + dz;
                }
                let dx = self.objects[to].position[0] - self.objects[from].position[0];
                let dy = self.objects[to].position[1] - self.objects[from].position[1];
                return (dx * dx + dy * dy + dz * dz).sqrt();
            }
            // task-50: a gantry crane travels vertically and horizontally at
            // the same time, so one leg is the longer of the two.
            ObjectKind::Crane => {
                let dz = (self.objects[to].z - self.objects[from].z).abs();
                let horizontal = distance(self.objects[from].position, self.objects[to].position);
                return dz.max(horizontal);
            }
            _ => {}
        }
        if self.objects[transporter].kind == ObjectKind::Person
            && let Some(grid) = self.model.grid.as_ref()
            && let Some(distance) =
                grid_distance(grid, self.objects[from].position, self.objects[to].position)
        {
            return distance;
        }
        // FlexSim `NetworkNode`: a node graph laid out in the model wins over the
        // per-transporter edge table, which stays the fallback for models that
        // declare their roads inline.
        if let Some(routed) =
            node_network_distance(self.model, &self.objects[from], &self.objects[to])
        {
            return routed;
        }
        let edges: Vec<NetworkEdge> = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params)
            | ObjectParams::Person(params)
            | ObjectParams::Robot(params)
            | ObjectParams::Crane(params)
            | ObjectParams::AsrsVehicle(params) => params.network.clone(),
            ObjectParams::Agv(params) => params.network.clone(),
            _ => Vec::new(),
        };
        if !edges.is_empty()
            && let Some(distance) =
                shortest_network_distance(&edges, &self.objects[from].name, &self.objects[to].name)
        {
            return distance;
        }
        distance(self.objects[from].position, self.objects[to].position)
    }

    /// B6 congestion: every other person currently on a task slows this one by
    /// 10% (down to half speed).
    fn person_congestion(&self, person: usize) -> f64 {
        let peers = self
            .objects
            .iter()
            .enumerate()
            .filter(|(index, object)| {
                *index != person && object.kind == ObjectKind::Person && object.tasks > 0
            })
            .count();
        (1.0 + 0.1 * peers as f64).recip().max(0.5)
    }

    /// B6 density/avoidance: neighbours in the 8 surrounding cells slow a
    /// walker (15% each), another person in the same cell waits (50%).
    fn person_density_factor(&self, person: usize) -> f64 {
        let Some(grid) = self.model.grid.as_ref() else {
            return 1.0;
        };
        let size = grid.cell_size.max(EPS);
        let cell = |point: [f64; 2]| {
            (
                (point[0] / size).floor() as i32,
                (point[1] / size).floor() as i32,
            )
        };
        let here = cell(self.objects[person].position);
        let mut neighbors: f64 = 0.0;
        let mut same: f64 = 0.0;
        for (index, object) in self.objects.iter().enumerate() {
            if index == person || object.kind != ObjectKind::Person {
                continue;
            }
            let other = cell(object.position);
            let distance = (other.0 - here.0).abs().max((other.1 - here.1).abs());
            if distance == 0 {
                same += 1.0;
            } else if distance == 1 {
                neighbors += 1.0;
            }
        }
        (1.0 / (1.0 + 0.15 * neighbors + 0.5 * same)).max(0.4)
    }

    fn transport_speed(&self, transporter: usize) -> f64 {
        match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params) => params.speed.max(EPS),
            ObjectParams::Person(params) => {
                params.speed.max(EPS)
                    * self.person_congestion(transporter)
                    * self.person_density_factor(transporter)
            }
            ObjectParams::Agv(params) => params.speed.max(EPS),
            ObjectParams::Elevator(params) => params.speed.max(EPS),
            ObjectParams::Robot(params) | ObjectParams::Crane(params) => params.speed.max(EPS),
            ObjectParams::AsrsVehicle(params) => params.speed.max(EPS),
            _ => 1.0,
        }
    }

    /// Travel time from the pickup point to the requester.
    fn transport_leg_travel(&self, transporter: usize, requester: usize) -> f64 {
        let speed = self.transport_speed(transporter);
        match self.objects[transporter].home {
            Some(home) => self.transport_object_distance(transporter, home, requester) / speed,
            None if self.transport_is_elevator(transporter) => {
                (self.objects[transporter].z - self.objects[requester].z).abs() / speed
            }
            None if self.objects[transporter].kind == ObjectKind::Person => self
                .model
                .grid
                .as_ref()
                .and_then(|grid| {
                    grid_distance(
                        grid,
                        self.objects[transporter].position,
                        self.objects[requester].position,
                    )
                })
                .map_or_else(
                    || {
                        distance(
                            self.objects[transporter].position,
                            self.objects[requester].position,
                        ) / speed
                    },
                    |grid_distance| grid_distance / speed,
                ),
            None => {
                distance(
                    self.objects[transporter].position,
                    self.objects[requester].position,
                ) / speed
            }
        }
    }

    /// Full task estimate used to pick the closest idle executor (B4/B6).
    fn transport_estimate_travel(&self, transporter: usize, requester: usize) -> f64 {
        self.transport_pickup_travel(transporter)
            + self.transport_leg_travel(transporter, requester)
    }

    /// Full task travel distance (pickup + delivery + return) for battery drain.
    fn transport_estimate_distance(&self, transporter: usize, requester: usize) -> f64 {
        let speed = self.transport_speed(transporter);
        let pickup = self.transport_pickup_travel(transporter) * speed;
        let leg = self.transport_leg_travel(transporter, requester) * speed;
        let home = match self.objects[transporter].home {
            Some(home) => self.transport_object_distance(transporter, requester, home),
            None => 0.0,
        };
        pickup + leg + home
    }

    fn handle_operator_loaded(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        // task-188: this event is the unloading-finished instant, so it fires
        // the executer's `on_unload` trigger before the operator is granted.
        self.fire_executer_trigger(transporter, requester, SimTrigger::OnUnload)?;
        self.set_executor_activity(transporter, ExecutorActivity::Utilize);
        // A5: the operator reached the requester, so it stops occupying a slot.
        if self.objects[requester].in_transit > 0 {
            self.objects[requester].in_transit -= 1;
        }
        if self.objects[requester].requesting_operator {
            self.grant_operator(requester, OperatorSource::Transporter(transporter));
        }
        Ok(())
    }

    /// The transporter finished unloading. It stays busy while it travels back
    /// to its home object, so a dispatcher cannot assign it to another task
    /// that it physically could not reach in time.
    fn handle_operator_released(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        // task-175: an explicit chain owns this release; it resumes the chain
        // (or parks it) instead of running the standard return-home sequence.
        if self.explicit_routes.contains_key(&transporter) {
            return self.handle_explicit_release(transporter, requester);
        }
        // task-150: people with `grid_step` also walk home cell by cell.
        if self.person_grid_step(transporter)
            && self.objects[transporter].kind == ObjectKind::Person
            && self.objects[transporter].home.is_some()
            && let Some(grid) = self.model.grid.clone()
        {
            let home = self.objects[transporter].home.expect("checked above");
            let from = self.objects[requester].position;
            let to = self.objects[home].position;
            if let Some(path) = grid_path(&grid, from, to)
                && path.len() > 1
            {
                let size = grid.cell_size.max(EPS);
                self.objects[transporter].person_path = path
                    .into_iter()
                    .skip(1)
                    .map(|[x, y]| [(x as f64 + 0.5) * size, (y as f64 + 0.5) * size])
                    .collect();
                self.objects[transporter].person_target = None;
                self.objects[transporter].person_returning = true;
                self.objects[transporter].position = from;
                self.refresh_person_reservations(transporter);
                return self.schedule_person_step(transporter);
            }
        }
        if self.objects[transporter].state == State::Blocked {
            self.set_state(transporter, State::Busy);
        }
        self.set_executor_activity(transporter, ExecutorActivity::TravelEmpty);
        self.traffic_release(transporter);
        let speed = self.transport_speed(transporter);
        let mut home_travel = self.objects[transporter]
            .home
            .map(|home| self.transport_object_distance(transporter, requester, home) / speed);
        if let Some(home) = self.objects[transporter].home {
            let from_position = self.objects[requester].position;
            let to_position = self.objects[home].position;
            if home_travel.is_some_and(|travel| travel > EPS) {
                let controls = self.traffic_controls_for_leg(from_position, to_position);
                if !self.traffic_try_acquire(transporter, &controls)? {
                    self.clear_executor_activity(transporter);
                    self.set_state(transporter, State::Blocked);
                    self.traffic_wait_for_slot(transporter, requester, 2, &controls);
                    return Ok(());
                }
                let speed_scale = self
                    .traffic_speed_factor(from_position, to_position)
                    .max(EPS);
                home_travel = home_travel.map(|travel| travel / speed_scale);
            }
            let distance = self.transport_object_distance(transporter, requester, home);
            if self.agv_mid_task_charge(transporter, requester, distance, 2)? {
                return Ok(());
            }
        }
        // Task-36: the doors must close before the cabin may travel again.
        let door_close = self.elevator_door_close(transporter);
        // task-103: door state statistics.
        self.objects[transporter].acc.detail.door_closing += door_close;
        if let Some(travel) = home_travel
            && travel > EPS
        {
            self.set_state(transporter, State::Busy);
            self.schedule(
                self.now + door_close + travel,
                EventKind::TransportHome(transporter),
            );
            return Ok(());
        }
        if door_close > EPS {
            self.set_state(transporter, State::Busy);
            self.schedule(self.now + door_close, EventKind::TransportHome(transporter));
            return Ok(());
        }
        self.settle_task(transporter)
    }

    fn handle_transport_home(&mut self, transporter: usize) -> Result<(), SimError> {
        self.clear_executor_activity(transporter);
        self.traffic_release(transporter);
        if let Some(home) = self.objects[transporter].home {
            self.objects[transporter].position = self.objects[home].position;
            self.objects[transporter].z = self.objects[home].z;
        }
        self.settle_task(transporter)
    }

    /// task-44: settle the task sequence before the task counts as finished.
    fn settle_task(&mut self, transporter: usize) -> Result<(), SimError> {
        // task-68: every extra leg of the task sequence is its own event.
        let steps = self.task_sequence_steps(transporter);
        self.objects[transporter].pending_steps = steps;
        self.schedule_pending_step(transporter)
    }

    fn dispatcher_params(&self, dispatcher: usize) -> Option<DispatcherParams> {
        match &self.model.objects[dispatcher].params {
            ObjectParams::Dispatcher(params) => Some(params.clone()),
            _ => None,
        }
    }

    /// task-173/175: resolve the explicit task chain at dispatch.
    ///
    /// Travel targets are resolved once here; `pickup` is the executer's home
    /// (its pickup point), `requester` is the current task's requester.
    fn explicit_route_legs(
        &mut self,
        dispatcher: usize,
        transporter: usize,
        requester: usize,
    ) -> Result<VecDeque<ExplicitLeg>, SimError> {
        let Some(params) = self.dispatcher_params(dispatcher) else {
            return Ok(VecDeque::new());
        };
        let mut legs = VecDeque::new();
        for (position, step) in params.task_sequence.iter().enumerate() {
            if let Some(condition) = step.condition() {
                let context = ExprContext::new(self, None);
                let matched = condition
                    .eval(&context)
                    .map(|value| value.as_bool())
                    .unwrap_or(false);
                if !matched {
                    continue;
                }
            }
            if let Some(extra) = step.extra_kind() {
                let kind = match extra {
                    ExtraTaskLegKind::Utilize => PendingLeg::Utilize,
                    ExtraTaskLegKind::Break => PendingLeg::Break,
                };
                let duration = step
                    .duration()
                    .map_or(0.0, |distribution| self.sample(distribution).max(0.0));
                legs.push_back(ExplicitLeg {
                    kind,
                    target: None,
                    duration: Some(duration),
                });
                continue;
            }
            match step.kind() {
                TaskStepKind::Travel => {
                    let target = step.target().ok_or_else(|| {
                        SimError::Config(format!(
                            "task_sequence[{position}] explicit travel leg has no target"
                        ))
                    })?;
                    let (destination, z) = match target {
                        TravelTarget::Named(TravelTargetName::Home) => {
                            match self.objects[transporter].home {
                                Some(home) => (self.objects[home].position, self.objects[home].z),
                                None => (
                                    self.objects[transporter].position,
                                    self.objects[transporter].z,
                                ),
                            }
                        }
                        TravelTarget::Named(TravelTargetName::Pickup) => {
                            match self.objects[transporter].home {
                                Some(home) => (self.objects[home].position, self.objects[home].z),
                                None => (
                                    self.objects[transporter].position,
                                    self.objects[transporter].z,
                                ),
                            }
                        }
                        TravelTarget::Named(TravelTargetName::Requester) => {
                            (self.objects[requester].position, self.objects[requester].z)
                        }
                        TravelTarget::Object(object) => {
                            let Some(index) = self.by_name.get(&object.object).copied() else {
                                return Err(SimError::Model(openflex_model::ModelError::Invalid(
                                    vec![openflex_model::ValidationIssue::new(
                                        format!(
                                            "objects.{}.task_sequence[{position}].target.object",
                                            self.objects[dispatcher].name
                                        ),
                                        format!("unknown object \"{}\"", object.object),
                                    )],
                                )));
                            };
                            (self.objects[index].position, self.objects[index].z)
                        }
                        TravelTarget::Position(position) => {
                            (position.position, self.objects[transporter].z)
                        }
                    };
                    legs.push_back(ExplicitLeg {
                        kind: PendingLeg::Travel,
                        target: Some((destination, z)),
                        duration: None,
                    });
                }
                TaskStepKind::Load => legs.push_back(ExplicitLeg {
                    kind: PendingLeg::Load,
                    target: None,
                    duration: None,
                }),
                TaskStepKind::Unload => legs.push_back(ExplicitLeg {
                    kind: PendingLeg::Unload,
                    target: None,
                    duration: None,
                }),
            }
        }
        // A pure zero-distance travel route would finish immediately and be
        // re-dispatched at the same instant forever. Chains with load/unload/
        // utilize/break have their own duration and are allowed.
        if legs.iter().all(|leg| leg.kind == PendingLeg::Travel) {
            let mut from = self.objects[transporter].position;
            let mut total = 0.0;
            for leg in &legs {
                if let Some((destination, _)) = leg.target {
                    total += route_distance(self.model, from, destination);
                    from = destination;
                }
            }
            if !legs.is_empty() && total <= EPS {
                return Err(SimError::Config(
                    "explicit travel route has zero total distance; add a non-zero target".into(),
                ));
            }
        }
        Ok(legs)
    }

    /// task-173: one trace event per route travel leg (no requester/item).
    fn record_route_leg(&mut self, label: &str, transporter: usize, position: [f64; 2]) {
        if self.trace.is_none() {
            return;
        }
        let time = self.now;
        let seq = self.next_seq;
        self.next_seq += 1;
        let name = self.objects[transporter].name.clone();
        if let Some(log) = self.trace.as_mut() {
            log.push(
                TraceEvent::new(time, seq, label, name)
                    .with("x", position[0])
                    .with("y", position[1]),
            );
        }
    }

    fn park_explicit_route(&mut self, transporter: usize) {
        self.set_state(transporter, State::Busy);
        self.set_executor_activity(transporter, ExecutorActivity::Utilize);
    }

    /// task-173/175: run the next explicit chain leg (or wait for release).
    fn advance_explicit_route(&mut self, transporter: usize) -> Result<(), SimError> {
        loop {
            let (leg, requester, loaded) = {
                let Some(state) = self.explicit_routes.get_mut(&transporter) else {
                    return self.finish_task(transporter);
                };
                if state.waiting_release && !state.release_seen {
                    let may_run = state.legs.front().is_some_and(|leg| {
                        matches!(leg.kind, PendingLeg::Utilize | PendingLeg::Break)
                    });
                    if !may_run {
                        // Stay at the requester until the processor releases the
                        // operator; this is the only synchronisation event.
                        self.park_explicit_route(transporter);
                        return Ok(());
                    }
                }
                let Some(leg) = state.legs.pop_front() else {
                    self.explicit_routes.remove(&transporter);
                    self.explicit_target.remove(&transporter);
                    return self.finish_task(transporter);
                };
                state.leg_in_progress = true;
                state.current = Some(leg.kind);
                state.current_target = None;
                state.leg_deadline = None;
                (leg, state.requester, state.loaded)
            };
            match leg.kind {
                PendingLeg::Travel => {
                    let Some((destination, z)) = leg.target else {
                        return Err(SimError::Config(
                            "explicit travel leg has no resolved target".into(),
                        ));
                    };
                    self.explicit_target.insert(transporter, (destination, z));
                    if let Some(state) = self.explicit_routes.get_mut(&transporter) {
                        state.current_target = Some((destination, z));
                    }
                    let from = self.objects[transporter].position;
                    let controls = self.traffic_controls_for_leg(from, destination);
                    if !self.traffic_try_acquire(transporter, &controls)? {
                        self.clear_executor_activity(transporter);
                        self.set_state(transporter, State::Blocked);
                        self.schedule(
                            self.now + TRAFFIC_RETRY_DELAY,
                            EventKind::ExplicitTrafficRetry(transporter),
                        );
                        return Ok(());
                    }
                    let speed_scale = self.traffic_speed_factor(from, destination).max(EPS);
                    let speed = self.transport_speed(transporter).max(EPS) * speed_scale;
                    let travel = route_distance(self.model, from, destination) / speed;
                    self.set_state(transporter, State::Busy);
                    self.set_executor_activity(
                        transporter,
                        if loaded {
                            ExecutorActivity::TravelLoaded
                        } else {
                            ExecutorActivity::TravelEmpty
                        },
                    );
                    self.record_route_leg("transport_route_depart", transporter, destination);
                    if travel > EPS {
                        self.schedule(self.now + travel, EventKind::ExplicitLegDone(transporter));
                        return Ok(());
                    }
                    self.objects[transporter].position = destination;
                    self.objects[transporter].z = z;
                    self.record_route_leg("transport_route_arrive", transporter, destination);
                    self.explicit_target.remove(&transporter);
                    self.traffic_release(transporter);
                    if let Some(state) = self.explicit_routes.get_mut(&transporter) {
                        state.leg_in_progress = false;
                    }
                }
                PendingLeg::Load => {
                    let Some(home) = self.objects[transporter].home else {
                        return Err(SimError::Config(
                            "explicit load leg requires a home/pickup point".into(),
                        ));
                    };
                    if distance(
                        self.objects[transporter].position,
                        self.objects[home].position,
                    ) > 0.5
                    {
                        return Err(SimError::Config(
                            "explicit load leg must start at the pickup point".into(),
                        ));
                    }
                    let dwell = self.transport_dwell(transporter);
                    let load = self.transport_load_time(transporter);
                    self.set_state(transporter, State::Busy);
                    self.set_executor_activity(transporter, ExecutorActivity::Loading);
                    if let Some(state) = self.explicit_routes.get_mut(&transporter) {
                        state.leg_deadline = Some(self.now + dwell + load);
                    }
                    self.schedule(
                        self.now + dwell + load,
                        EventKind::ExplicitLegDone(transporter),
                    );
                    return Ok(());
                }
                PendingLeg::Unload => {
                    if !loaded {
                        return Err(SimError::Config(
                            "explicit unload leg requires a preceding load/loaded state".into(),
                        ));
                    }
                    if distance(
                        self.objects[transporter].position,
                        self.objects[requester].position,
                    ) > 0.5
                    {
                        return Err(SimError::Config(
                            "explicit unload leg must start at the requester".into(),
                        ));
                    }
                    let dwell = self.transport_dwell(transporter);
                    let unload = self.transport_unload_time(transporter);
                    let door_open = self.elevator_door_open(transporter);
                    self.objects[transporter].acc.detail.door_opening += door_open;
                    self.set_state(transporter, State::Busy);
                    self.set_executor_activity(transporter, ExecutorActivity::Unloading);
                    if let Some(state) = self.explicit_routes.get_mut(&transporter) {
                        state.leg_deadline = Some(self.now + door_open + dwell + unload);
                    }
                    self.schedule(
                        self.now + door_open + dwell + unload,
                        EventKind::ExplicitLegDone(transporter),
                    );
                    return Ok(());
                }
                PendingLeg::Utilize => {
                    let duration = leg.duration.unwrap_or(0.0).max(0.0);
                    self.set_state(transporter, State::Busy);
                    self.set_executor_activity(transporter, ExecutorActivity::Utilize);
                    self.schedule(self.now + duration, EventKind::ExplicitLegDone(transporter));
                    return Ok(());
                }
                PendingLeg::Break => {
                    let duration = leg.duration.unwrap_or(0.0).max(0.0);
                    self.clear_executor_activity(transporter);
                    self.set_state(transporter, State::Idle);
                    self.schedule(self.now + duration, EventKind::ExplicitLegDone(transporter));
                    return Ok(());
                }
            }
        }
    }

    fn handle_explicit_traffic_retry(&mut self, transporter: usize) -> Result<(), SimError> {
        let Some((destination, z)) = self.explicit_target.get(&transporter).copied() else {
            return self.advance_explicit_route(transporter);
        };
        let from = self.objects[transporter].position;
        let controls = self.traffic_controls_for_leg(from, destination);
        if !self.traffic_try_acquire(transporter, &controls)? {
            self.schedule(
                self.now + TRAFFIC_RETRY_DELAY,
                EventKind::ExplicitTrafficRetry(transporter),
            );
            return Ok(());
        }
        let loaded = self
            .explicit_routes
            .get(&transporter)
            .is_some_and(|state| state.loaded);
        let speed_scale = self.traffic_speed_factor(from, destination).max(EPS);
        let speed = self.transport_speed(transporter).max(EPS) * speed_scale;
        let travel = route_distance(self.model, from, destination) / speed;
        self.set_state(transporter, State::Busy);
        self.set_executor_activity(
            transporter,
            if loaded {
                ExecutorActivity::TravelLoaded
            } else {
                ExecutorActivity::TravelEmpty
            },
        );
        self.record_route_leg("transport_route_depart", transporter, destination);
        if travel > EPS {
            if let Some(state) = self.explicit_routes.get_mut(&transporter) {
                state.leg_deadline = Some(self.now + travel);
            }
            self.schedule(self.now + travel, EventKind::ExplicitLegDone(transporter));
        } else {
            self.objects[transporter].position = destination;
            self.objects[transporter].z = z;
            self.record_route_leg("transport_route_arrive", transporter, destination);
            self.explicit_target.remove(&transporter);
            self.traffic_release(transporter);
            if let Some(state) = self.explicit_routes.get_mut(&transporter) {
                state.leg_in_progress = false;
            }
            self.advance_explicit_route(transporter)?;
        }
        Ok(())
    }

    /// task-175: one explicit leg finished; do its completion bookkeeping and
    /// advance the chain (or wait for OperatorReleased).
    fn handle_explicit_leg_done(&mut self, transporter: usize) -> Result<(), SimError> {
        self.traffic_release(transporter);
        let Some(state) = self.explicit_routes.get_mut(&transporter) else {
            return Ok(());
        };
        let kind = state.current.take();
        state.leg_in_progress = false;
        state.current_target = None;
        state.leg_deadline = None;
        let requester = state.requester;
        match kind {
            Some(PendingLeg::Travel) => {
                if let Some((position, z)) = self.explicit_target.remove(&transporter) {
                    self.objects[transporter].position = position;
                    self.objects[transporter].z = z;
                    self.record_route_leg("transport_route_arrive", transporter, position);
                }
            }
            Some(PendingLeg::Load) => {
                state.loaded = true;
            }
            Some(PendingLeg::Unload) => {
                state.loaded = false;
                state.waiting_release = true;
                if !state.operator_loaded {
                    state.operator_loaded = true;
                    // task-175: fire OperatorLoaded exactly once as a real event
                    // (trace + grant) instead of silently calling the handler.
                    self.schedule(self.now, EventKind::OperatorLoaded(transporter, requester));
                }
            }
            Some(PendingLeg::Utilize | PendingLeg::Break) | None => {}
        }
        self.advance_explicit_route(transporter)
    }

    fn handle_explicit_release(
        &mut self,
        transporter: usize,
        _requester: usize,
    ) -> Result<(), SimError> {
        let Some(state) = self.explicit_routes.get_mut(&transporter) else {
            return Ok(());
        };
        state.release_seen = true;
        let running = state.leg_in_progress;
        if !running {
            self.advance_explicit_route(transporter)?;
        }
        Ok(())
    }

    /// task-68: durations of the task-sequence legs beyond the standard ones
    /// (two travel legs, one load, one unload). Skipped steps and steps with a
    /// false condition contribute nothing.
    fn task_sequence_steps(&mut self, transporter: usize) -> Vec<(PendingLeg, f64)> {
        let Some(dispatcher) = self.objects[transporter].assigned_dispatcher else {
            return Vec::new();
        };
        let sequence = match &self.model.objects[dispatcher].params {
            ObjectParams::Dispatcher(params) => params.task_sequence.clone(),
            _ => Vec::new(),
        };
        if sequence.is_empty() {
            return Vec::new();
        }
        let trip = match self.objects[transporter].serving_requester {
            Some(requester) => {
                self.transport_leg_travel(transporter, requester)
                    + self.transport_pickup_travel(transporter)
            }
            None => 0.0,
        };
        let load = self.transport_load_time(transporter);
        let unload = self.transport_unload_time(transporter);
        let mut base_travels = 2u32;
        let mut base_loads = 1u32;
        let mut base_unloads = 1u32;
        let mut steps = Vec::new();
        for step in &sequence {
            if let Some(condition) = step.condition() {
                let context = ExprContext::new(self, None);
                let matched = condition
                    .eval(&context)
                    .map(|value| value.as_bool())
                    .unwrap_or(false);
                if !matched {
                    continue;
                }
            }
            let (kind, baseline) = match step.extra_kind() {
                Some(ExtraTaskLegKind::Utilize) => (PendingLeg::Utilize, None),
                Some(ExtraTaskLegKind::Break) => (PendingLeg::Break, None),
                None => match step.kind() {
                    TaskStepKind::Travel => (PendingLeg::Travel, Some(&mut base_travels)),
                    TaskStepKind::Load => (PendingLeg::Load, Some(&mut base_loads)),
                    TaskStepKind::Unload => (PendingLeg::Unload, Some(&mut base_unloads)),
                },
            };
            let default = match kind {
                PendingLeg::Travel => trip,
                PendingLeg::Load => load,
                PendingLeg::Unload => unload,
                // utilize/break have no default; validation requires duration.
                PendingLeg::Utilize | PendingLeg::Break => 0.0,
            };
            let duration = match step.duration() {
                Some(distribution) => self.sample(distribution).max(0.0),
                None => default,
            };
            if let Some(baseline) = baseline
                && *baseline > 0
            {
                *baseline -= 1;
                continue;
            }
            steps.push((kind, duration));
        }
        steps
    }

    /// task-68: arm the next extra task-sequence leg (or finish the task).
    fn schedule_pending_step(&mut self, transporter: usize) -> Result<(), SimError> {
        let Some(&(kind, duration)) = self.objects[transporter].pending_steps.first() else {
            return self.finish_task(transporter);
        };
        let at = self.now + duration;
        if at < self.config.stop_time - EPS {
            match kind {
                PendingLeg::Break => {
                    // A break is off-duty time: it counts as `idle` (not
                    // `blocked`, since nothing is waiting on a resource) and the
                    // task still holds capacity, so the dispatcher cannot assign
                    // this executer a new task while the break runs.
                    self.clear_executor_activity(transporter);
                    self.set_state(transporter, State::Idle);
                }
                PendingLeg::Travel
                | PendingLeg::Load
                | PendingLeg::Unload
                | PendingLeg::Utilize => {
                    self.set_state(transporter, State::Busy);
                    // task-166/169: extra legs and utilize are on-site task time.
                    self.set_executor_activity(transporter, ExecutorActivity::Utilize);
                }
            }
            self.schedule(at, EventKind::TaskStepDone(transporter));
            Ok(())
        } else {
            self.objects[transporter].pending_steps.clear();
            self.finish_task(transporter)
        }
    }

    /// task-68: one extra task-sequence leg elapsed.
    fn handle_task_step_done(&mut self, transporter: usize) -> Result<(), SimError> {
        if !self.objects[transporter].pending_steps.is_empty() {
            self.objects[transporter].pending_steps.remove(0);
        }
        self.schedule_pending_step(transporter)
    }

    /// Mark one transporter task as complete and look for queued work.
    fn finish_task(&mut self, transporter: usize) -> Result<(), SimError> {
        self.clear_executor_activity(transporter);
        self.release_shaft(transporter);
        self.objects[transporter].tasks = self.objects[transporter].tasks.saturating_sub(1);
        if let Some(dispatcher) = self.objects[transporter].assigned_dispatcher {
            self.objects[dispatcher].dispatcher_active =
                self.objects[dispatcher].dispatcher_active.saturating_sub(1);
        }
        self.objects[transporter].serving_requester = None;
        self.objects[transporter].acc.output += 1;
        self.drain_agv_battery(transporter)?;
        self.set_state(transporter, State::Idle);
        if let Some(dispatcher) = self.objects[transporter].assigned_dispatcher {
            self.dispatch(dispatcher)?;
        }
        Ok(())
    }

    fn resource_params(&self, index: usize) -> Option<openflex_model::ResourceParams> {
        match &self.model.objects[index].params {
            ObjectParams::Resource(params) => Some(params.clone()),
            _ => None,
        }
    }

    fn resource_request(&mut self, resource: usize, requester: usize) -> Result<(), SimError> {
        let Some(params) = self.resource_params(resource) else {
            return Ok(());
        };
        if !self.objects[resource].on_shift {
            self.objects[resource].requests.push_back(requester);
            return Ok(());
        }
        if let Some(resume) = downtime_until(&params.downtimes, self.now) {
            self.schedule_resource_retry(resource, resume);
            self.objects[resource].requests.push_back(requester);
            return Ok(());
        }
        if self.objects[resource].resource_in_use < params.capacity {
            self.objects[resource].resource_in_use += 1;
            self.set_state(resource, State::Busy);
            self.grant_operator(requester, OperatorSource::Resource(resource));
        } else {
            self.objects[resource].requests.push_back(requester);
        }
        Ok(())
    }

    /// Give free resource units to queued processors.
    fn grant_waiting_resource(&mut self, resource: usize) -> Result<(), SimError> {
        let Some(params) = self.resource_params(resource) else {
            return Ok(());
        };
        if !self.objects[resource].on_shift {
            return Ok(());
        }
        if let Some(resume) = downtime_until(&params.downtimes, self.now) {
            self.schedule_resource_retry(resource, resume);
            return Ok(());
        }
        while self.objects[resource].resource_in_use < params.capacity {
            let requester = loop {
                let Some(requester) = self.objects[resource].requests.pop_front() else {
                    return Ok(());
                };
                if self.objects[requester].requesting_operator {
                    break requester;
                }
            };
            self.objects[resource].resource_in_use += 1;
            self.set_state(resource, State::Busy);
            self.grant_operator(requester, OperatorSource::Resource(resource));
        }
        if self.objects[resource].resource_in_use == 0 {
            self.set_state(resource, State::Idle);
        }
        Ok(())
    }

    fn schedule_resource_retry(&mut self, resource: usize, at: f64) {
        if self.objects[resource].retry_pending {
            return;
        }
        let at = at.max(self.now);
        self.objects[resource].retry_pending = true;
        self.schedule(at, EventKind::ResourceRetry(resource));
    }

    fn handle_resource_retry(&mut self, resource: usize) -> Result<(), SimError> {
        self.objects[resource].retry_pending = false;
        self.grant_waiting_resource(resource)
    }

    // -----------------------------------------------------------------------
    // MultiProcessor (B1)
    // -----------------------------------------------------------------------

    fn multiprocessor_params(&self, index: usize) -> Option<openflex_model::MultiProcessorParams> {
        match &self.model.objects[index].params {
            ObjectParams::MultiProcessor(params) => Some(params.clone()),
            _ => None,
        }
    }

    /// Start as many free slots as possible and keep the state in sync.
    ///
    /// Operators are requested as a crew of `slots` for the whole busy period
    /// (one crew per busy period, held until every slot is idle), which keeps
    /// the shared dispatcher/resource machinery unchanged; see the module
    /// documentation for the simplification.
    fn flow_multiprocessor(&mut self, index: usize) -> Result<(), SimError> {
        let Some(params) = self.multiprocessor_params(index) else {
            return Ok(());
        };
        if !self.objects[index].on_shift || self.objects[index].state == State::Down {
            return Ok(());
        }
        let slots = self.objects[index].lanes.len();
        let active = self.objects[index]
            .lanes
            .iter()
            .filter(|lane| lane.item.is_some())
            .count();
        let wanted = (active + self.objects[index].content.len()).min(slots);
        if wanted > 0 && (params.process_operator.is_some() || params.setup_operator.is_some()) {
            let operator_params = mp_operator_params(&params, wanted as u32);
            if !self.ensure_operator(index, OperatorPhase::Process, &operator_params)? {
                // Existing slots keep working while the crew is incomplete.
                if active > 0 {
                    self.set_phase(index, Phase::Processing);
                    self.set_state(index, State::Busy);
                }
                return Ok(());
            }
        }
        while let Some(lane) = self.objects[index]
            .lanes
            .iter()
            .position(|lane| lane.item.is_none())
        {
            let Some(item) = self.take_content(index, 0) else {
                break;
            };
            let setup = self.sample(&params.setup_time).max(0.0);
            self.objects[index].lanes[lane].item = Some(item);
            self.objects[index].lanes[lane].busy_since = self.now;
            self.objects[index].lanes[lane].token += 1;
            let token = self.objects[index].lanes[lane].token;
            if setup > 0.0 {
                self.objects[index].lanes[lane].phase = Phase::Setup;
                self.objects[index].lanes[lane].end = self.now + setup;
                self.schedule(self.now + setup, EventKind::MpDone(index, lane, token));
            } else {
                let process = self.sample(&params.process_time).max(0.0);
                self.objects[index].lanes[lane].phase = Phase::Processing;
                self.objects[index].lanes[lane].end = self.now + process;
                self.schedule(self.now + process, EventKind::MpDone(index, lane, token));
            }
        }
        self.refresh_multiprocessor(index)?;
        Ok(())
    }

    /// Publish the object-level phase/state from the slot states and release
    /// the crew once the object is idle again.
    fn refresh_multiprocessor(&mut self, index: usize) -> Result<(), SimError> {
        let active = self.objects[index]
            .lanes
            .iter()
            .filter(|lane| lane.item.is_some())
            .count();
        if active == 0 {
            self.set_phase(index, Phase::Idle);
            if self.objects[index].content.is_empty() && self.objects[index].out_pending.is_empty()
            {
                self.set_state(index, State::Idle);
                self.release_operator(index)?;
            } else {
                self.set_state(index, State::Busy);
            }
            return Ok(());
        }
        let any_setup = self.objects[index]
            .lanes
            .iter()
            .any(|lane| lane.phase == Phase::Setup);
        self.set_phase(
            index,
            if any_setup {
                Phase::Setup
            } else {
                Phase::Processing
            },
        );
        self.set_state(index, State::Busy);
        Ok(())
    }

    fn handle_mp_done(&mut self, index: usize, lane: usize, token: u64) -> Result<(), SimError> {
        let Some(params) = self.multiprocessor_params(index) else {
            return Ok(());
        };
        let Some(current) = self.objects[index].lanes.get(lane) else {
            return Ok(());
        };
        if current.token != token || current.item.is_none() {
            return Ok(());
        }
        if current.phase == Phase::Setup {
            let process = self.sample(&params.process_time).max(0.0);
            let lane_rt = &mut self.objects[index].lanes[lane];
            lane_rt.phase = Phase::Processing;
            lane_rt.token += 1;
            lane_rt.end = self.now + process;
            let next_token = lane_rt.token;
            self.schedule(
                self.now + process,
                EventKind::MpDone(index, lane, next_token),
            );
            // task-152: this slot's setup phase just finished (processing starts
            // now; OnReceive/OnEntry fire earlier when the item arrives).
            self.fire_lifecycle_trigger(index, SimTrigger::SetupFinish);
            self.refresh_multiprocessor(index)?;
            return Ok(());
        }
        let now = self.now;
        self.objects[index].note_content(now);
        if let Some(item) = self.objects[index].lanes[lane].item.take() {
            self.objects[index].out_pending.push(item);
        }
        let busy_since = self.objects[index].lanes[lane].busy_since;
        if busy_since > 0.0 {
            self.objects[index].slot_busy += (now - busy_since).max(0.0);
        }
        self.objects[index].lanes[lane].busy_since = 0.0;
        self.objects[index].lanes[lane].phase = Phase::Idle;
        self.objects[index].lanes[lane].token += 1;
        self.objects[index].touch();
        // task-152: this slot's processing clock finished; the item is still
        // pending delivery (on_exit fires later).
        self.fire_lifecycle_trigger(index, SimTrigger::ProcessFinish);
        // Deliver the finished item and start the next one in a free slot.
        self.flow(index)?;
        self.refresh_multiprocessor(index)?;
        self.notify_upstreams(index)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Conveyor
    // -----------------------------------------------------------------------

    /// Move the belt forward to the current model time.
    fn sync_belt(&mut self, index: usize) {
        if self.conveyor_stopped(index) {
            return;
        }
        let dt = self.now - self.objects[index].belt_last_update;
        if dt <= 0.0 {
            return;
        }
        if self.objects[index].belt_slug {
            self.advance_slug_belt(index, dt);
        } else if let Some(belt) = self.objects[index].belt.as_mut() {
            belt.advance(dt);
        }
        self.objects[index].belt_last_update = self.now;
    }

    fn slug_groups(&self, index: usize) -> Vec<(usize, usize)> {
        let Some(belt) = self.objects[index].belt.as_ref() else {
            return Vec::new();
        };
        let min_gap = self.objects[index].belt_slug_min_gap.max(EPS);
        slug_groups(&belt.positions, min_gap)
    }

    fn slug_target(&self, index: usize, start: usize, stops: &[f64]) -> f64 {
        let Some(belt) = self.objects[index].belt.as_ref() else {
            return 0.0;
        };
        // Between distinct slugs the ordinary item spacing applies, exactly as
        // on a non-slug belt; if the gap later drops below slug_min_gap the next
        // tick groups them.
        let separation = if start == 0 {
            0.0
        } else {
            belt.spacing.max(0.0)
        };
        let mut target = if start == 0 {
            belt.length
        } else {
            (belt.positions[start - 1] - separation).max(belt.positions[start])
        };
        if let Some(stop) = stops
            .iter()
            .copied()
            .filter(|stop| *stop > belt.positions[start] + EPS && *stop < target)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        {
            target = stop;
        }
        target
    }

    fn slug_time_to_next_event(&self, index: usize, stops: &[f64]) -> Option<f64> {
        let belt = self.objects[index].belt.as_ref()?;
        if belt.is_empty() {
            return None;
        }
        let groups = self.slug_groups(index);
        let mut next: Option<f64> = None;
        let mut consider = |time: f64| {
            if time > EPS {
                next = Some(next.map_or(time, |current: f64| current.min(time)));
            }
        };
        for (start, end) in groups {
            let hold = belt.holds[start..end]
                .iter()
                .copied()
                .filter(|hold| *hold > EPS)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            if let Some(hold) = hold {
                consider(hold);
                continue;
            }
            let target = self.slug_target(index, start, stops);
            if target > belt.positions[start] + EPS {
                consider(belt.time_to_reach(belt.positions[start], target));
            }
        }
        next
    }

    fn advance_slug_belt(&mut self, index: usize, dt: f64) {
        let mut remaining = dt;
        let mut guard = 0;
        while remaining > EPS && guard < 10_000 {
            guard += 1;
            let groups = self.slug_groups(index);
            if groups.is_empty() {
                break;
            }
            let stops = self.belt_stop_positions(index);
            let mut step = remaining;
            let mut moving = false;
            {
                let Some(belt) = self.objects[index].belt.as_ref() else {
                    break;
                };
                for (start, end) in &groups {
                    let hold = belt.holds[*start..*end]
                        .iter()
                        .copied()
                        .filter(|hold| *hold > EPS)
                        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some(hold) = hold {
                        step = step.min(hold);
                        continue;
                    }
                    let target = self.slug_target(index, *start, &stops);
                    if target > belt.positions[*start] + EPS {
                        moving = true;
                        step = step.min(belt.time_to_reach(belt.positions[*start], target));
                    }
                }
            }
            if !moving {
                break;
            }
            let step = step.min(remaining);
            if step <= EPS {
                break;
            }
            for (start, end) in &groups {
                let target = self.slug_target(index, *start, &stops);
                let Some(belt) = self.objects[index].belt.as_mut() else {
                    return;
                };
                if belt.holds[*start..*end].iter().any(|hold| *hold > EPS) {
                    for hold in &mut belt.holds[*start..*end] {
                        *hold = (*hold - step).max(0.0);
                    }
                } else {
                    let position = belt.positions[*start];
                    let delta = (belt.advance_position(position, step) - position)
                        .min((target - position).max(0.0));
                    for offset in *start..*end {
                        belt.positions[offset] = (belt.positions[offset] + delta)
                            .min(if offset == 0 {
                                belt.length
                            } else {
                                f64::INFINITY
                            })
                            .max(belt.positions[offset]);
                    }
                }
            }
            remaining -= step;
        }
    }

    /// Positions of the photo eyes and stations on a belt, sorted ascending.
    fn belt_stop_positions(&self, index: usize) -> Vec<f64> {
        self.objects[index]
            .belt_stops
            .iter()
            .map(|stop| stop.position)
            .collect()
    }

    /// Fire photo-eye triggers and start station dwells for every item that
    /// reached one of the belt's fixed points (B3).
    fn process_belt_stops(&mut self, index: usize) -> Result<(), SimError> {
        if self.objects[index].belt_stops.is_empty() {
            return Ok(());
        }
        // task-180: evaluate the photo-eye state before legacy stop clamping, so
        // a boundary event at position+range can clear the beam at that instant.
        self.update_photo_eyes(index)?;
        let stops = self.objects[index].belt_stops.clone();
        let items = self.objects[index].belt_items.len();
        for item_index in 0..items {
            for (stop_index, stop) in stops.iter().enumerate() {
                let handled = self.objects[index]
                    .belt_handled
                    .get(item_index)
                    .and_then(|flags| flags.get(stop_index))
                    .copied()
                    .unwrap_or(false);
                if handled {
                    continue;
                }
                let Some(position) = self.objects[index]
                    .belt
                    .as_ref()
                    .and_then(|belt| belt.position(item_index))
                else {
                    continue;
                };
                if position + EPS < stop.position {
                    continue;
                }
                if let Some(flags) = self.objects[index].belt_handled.get_mut(item_index)
                    && let Some(flag) = flags.get_mut(stop_index)
                {
                    *flag = true;
                }
                if let Some(belt) = self.objects[index].belt.as_mut() {
                    belt.set_position(item_index, stop.position);
                }
                match stop.kind {
                    BeltStopKind::PhotoEye(_) => self.fire_photo_eye(index, item_index)?,
                    BeltStopKind::Station(station) => {
                        let dwell = self.sample_station_time(index, station);
                        if let Some(belt) = self.objects[index].belt.as_mut() {
                            belt.set_hold(item_index, dwell);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// task-181: evaluate decision points for each item in range. Actions are
    /// applied to that item; `once_per_item` is reset when it leaves the range.
    fn update_decision_points(&mut self, index: usize) -> Result<(), SimError> {
        let points = match &self.model.objects[index].params {
            ObjectParams::Conveyor(params) => params.decision_points.clone(),
            _ => return Ok(()),
        };
        if points.is_empty() {
            return Ok(());
        }
        let positions = self.objects[index]
            .belt
            .as_ref()
            .map(|belt| belt.positions.clone())
            .unwrap_or_default();
        let ranges = self.objects[index].decision_point_ranges.clone();
        let item_count = self.objects[index].belt_items.len();
        if self.objects[index].decision_handled.len() < item_count {
            self.objects[index]
                .decision_handled
                .resize_with(item_count, || vec![false; points.len()]);
        }
        for item_index in 0..self.objects[index].belt_items.len() {
            for (point_index, point) in points.iter().enumerate() {
                let range = ranges.get(point_index).copied().unwrap_or(0.5).max(EPS);
                let position = positions.get(item_index).copied().unwrap_or(0.0);
                let covered = position >= point.position - range - EPS
                    && position < point.position + range - EPS;
                let handled = self.objects[index]
                    .decision_handled
                    .get(item_index)
                    .and_then(|flags| flags.get(point_index))
                    .copied()
                    .unwrap_or(false);
                if !covered {
                    if let Some(flags) = self.objects[index].decision_handled.get_mut(item_index)
                        && let Some(flag) = flags.get_mut(point_index)
                    {
                        *flag = false;
                    }
                    continue;
                }
                if point.once_per_item && handled {
                    continue;
                }
                let Some(mut item) = self.objects[index].belt_items.get(item_index).cloned() else {
                    continue;
                };
                let matched = match &point.condition {
                    Some(condition) => {
                        let value = condition
                            .eval(&ExprContext::new(self, Some(&item)))
                            .map_err(|error| SimError::Expression {
                                location: format!(
                                    "{}.decision_points[{point_index}] (item {})",
                                    self.objects[index].name, item.id
                                ),
                                detail: format!("t={}: {error}", self.now),
                            })?;
                        value.as_bool()
                    }
                    None => true,
                };
                if matched {
                    self.action_location = format!(
                        "{}.decision_points[{point_index}].actions",
                        self.objects[index].name
                    );
                    self.apply_actions(index, &mut item, point.actions.clone())?;
                    if let Some(slot) = self.objects[index].belt_items.get_mut(item_index) {
                        *slot = item;
                    }
                }
                if point.once_per_item
                    && let Some(flags) = self.objects[index].decision_handled.get_mut(item_index)
                    && let Some(flag) = flags.get_mut(point_index)
                {
                    *flag = true;
                }
            }
        }
        Ok(())
    }

    /// task-179: update the four-state Photo Eye machine after a belt tick.
    ///
    /// The beam is blocked while an item sits exactly on the eye (the normal
    /// stop-clamping path or a station hold). Blocking/Clearing are the transient
    /// entry states; with no Block Time / Clear Time settings yet they resolve
    /// immediately, so only entering Blocked and Cleared fires hooks. This is
    /// deliberately additive: the legacy per-item `on_photo_eye` is unchanged.
    fn update_photo_eyes(&mut self, index: usize) -> Result<(), SimError> {
        let stops: Vec<(usize, f64, f64)> = self.objects[index]
            .belt_stops
            .iter()
            .filter_map(|stop| match stop.kind {
                BeltStopKind::PhotoEye(eye) => Some((
                    eye,
                    stop.position,
                    self.objects[index]
                        .photo_eye_ranges
                        .get(eye)
                        .copied()
                        .unwrap_or(0.5)
                        .max(EPS),
                )),
                BeltStopKind::Station(_) => None,
            })
            .collect();
        if stops.is_empty() {
            return Ok(());
        }
        let positions = self.objects[index]
            .belt
            .as_ref()
            .map(|belt| belt.positions.clone())
            .unwrap_or_default();
        let eye_count = match &self.model.objects[index].params {
            ObjectParams::Conveyor(params) => params.photo_eyes.len(),
            _ => 0,
        };
        if self.objects[index].photo_eye_states.len() < eye_count {
            self.objects[index]
                .photo_eye_states
                .resize(eye_count, PhotoEyeState::Cleared);
        }
        for (eye, position, range) in stops {
            // Our own sensing granularity: `range` is a half-width around the
            // eye (default 0.5 distance units). This is an OpenFlex choice, not
            // a published FlexSim value; it keeps a contiguous slug inside the
            // beam so the state does not jitter per item.
            let covered = positions
                .iter()
                .any(|item| *item >= position - range - EPS && *item < position + range - EPS);
            if std::env::var_os("OPENFLEX_DEBUG_EYE").is_some() {
                eprintln!(
                    "eye {eye} at {} pos={positions:?} range={range} covered={covered}",
                    self.now
                );
            }
            let state = self.objects[index]
                .photo_eye_states
                .get(eye)
                .copied()
                .unwrap_or(PhotoEyeState::Cleared);
            match (state, covered) {
                (PhotoEyeState::Cleared | PhotoEyeState::Clearing, true) => {
                    // Enter Blocking then Blocked (zero transition time).
                    self.objects[index].photo_eye_states[eye] = PhotoEyeState::Blocking;
                    self.objects[index].photo_eye_states[eye] = PhotoEyeState::Blocked;
                    self.fire_photo_eye_state(index, eye, PhotoEyeState::Blocked)?;
                }
                (PhotoEyeState::Blocked | PhotoEyeState::Blocking, false) => {
                    self.objects[index].photo_eye_states[eye] = PhotoEyeState::Clearing;
                    self.objects[index].photo_eye_states[eye] = PhotoEyeState::Cleared;
                    self.fire_photo_eye_state(index, eye, PhotoEyeState::Cleared)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn fire_photo_eye_state(
        &mut self,
        index: usize,
        eye: usize,
        state: PhotoEyeState,
    ) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Conveyor(params) => params,
            _ => return Ok(()),
        };
        let Some(photo_eye) = params.photo_eyes.get(eye) else {
            return Ok(());
        };
        let (name, actions) = match state {
            PhotoEyeState::Blocked => ("on_blocked", photo_eye.triggers.on_blocked.clone()),
            PhotoEyeState::Cleared => ("on_cleared", photo_eye.triggers.on_cleared.clone()),
            _ => return Ok(()),
        };
        if actions.is_empty() {
            return Ok(());
        }
        let mut probe = FlowItem::probe(self.now);
        self.action_location = format!(
            "{}.photo_eyes[{eye}].triggers.{name}",
            self.objects[index].name
        );
        self.apply_actions(index, &mut probe, actions)
    }

    /// Run the conveyor's `on_photo_eye` trigger against one item.
    fn fire_photo_eye(&mut self, index: usize, item_index: usize) -> Result<(), SimError> {
        let actions = self.trigger_actions(index, SimTrigger::OnPhotoEye);
        if actions.is_empty() {
            return Ok(());
        }
        let Some(mut item) = self.objects[index].belt_items.get(item_index).cloned() else {
            return Ok(());
        };
        self.action_location = format!("{}.on_photo_eye", self.objects[index].name);
        self.apply_actions(index, &mut item, actions)?;
        if let Some(slot) = self.objects[index].belt_items.get_mut(item_index) {
            *slot = item;
        }
        Ok(())
    }

    /// Sample how long an item stays at a conveyor station (B3).
    fn sample_station_time(&mut self, index: usize, station: usize) -> f64 {
        let distribution = match &self.model.objects[index].params {
            ObjectParams::Conveyor(params) => params
                .stations
                .get(station)
                .map(|stop| stop.process_time.clone()),
            _ => None,
        };
        match distribution {
            Some(distribution) => self.sample(&distribution).max(0.0),
            None => 0.0,
        }
    }

    /// Schedule the next movement boundary, if the belt is moving.
    /// task-180: earliest Photo Eye boundary crossing candidate. Boundaries are
    /// `position ± range`; only reachable, non-zero-time crossings count, so an
    /// item resting on an eye cannot create a zero-interval self-spin. An eye
    /// whose range covers the whole belt is ignored.
    fn photo_eye_next_event(&self, index: usize) -> Option<f64> {
        let belt = self.objects[index].belt.as_ref()?;
        if belt.is_empty() {
            return None;
        }
        let stops: Vec<f64> = self.objects[index]
            .belt_stops
            .iter()
            .filter_map(|stop| match stop.kind {
                BeltStopKind::PhotoEye(eye) => Some(stop.position).map(|position| (eye, position)),
                BeltStopKind::Station(_) => None,
            })
            .map(|(_, position)| position)
            .collect();
        let mut next: Option<f64> = None;
        for (eye_index, position) in stops.iter().enumerate() {
            let range = self.objects[index]
                .photo_eye_ranges
                .get(eye_index)
                .copied()
                .unwrap_or(0.5)
                .max(EPS);
            let lower = position - range;
            let upper = position + range;
            if lower <= EPS && upper >= belt.length - EPS {
                // Range covers the whole belt: boundary events would fire every tick.
                continue;
            }
            for item in 0..belt.positions.len() {
                if belt.hold_remaining(item) > EPS {
                    continue;
                }
                let current = belt.positions[item];
                let limit = if item == 0 {
                    belt.length
                } else {
                    (belt.positions[item - 1] - belt.spacing).max(current)
                };
                let boundary = if current < lower - EPS {
                    lower
                } else if current < upper - EPS {
                    upper
                } else {
                    continue;
                };
                if boundary <= current + EPS
                    || boundary > limit + EPS
                    || boundary > belt.length + EPS
                {
                    continue;
                }
                let time = belt.time_to_reach(current, boundary);
                if time > EPS {
                    next = Some(next.map_or(time, |value: f64| value.min(time)));
                }
            }
        }
        next
    }

    /// task-181: earliest decision-point boundary candidate (same boundary
    /// mechanism as Photo Eyes, so no separate time model).
    fn decision_point_next_event(&self, index: usize) -> Option<f64> {
        let belt = self.objects[index].belt.as_ref()?;
        if belt.is_empty() {
            return None;
        }
        let points = match &self.model.objects[index].params {
            ObjectParams::Conveyor(params) => &params.decision_points,
            _ => return None,
        };
        let mut next: Option<f64> = None;
        for (point_index, point) in points.iter().enumerate() {
            let range = self.objects[index]
                .decision_point_ranges
                .get(point_index)
                .copied()
                .unwrap_or(0.5)
                .max(EPS);
            let lower = point.position - range;
            let upper = point.position + range;
            if lower <= EPS && upper >= belt.length - EPS {
                continue;
            }
            for item in 0..belt.positions.len() {
                if belt.hold_remaining(item) > EPS {
                    continue;
                }
                let current = belt.positions[item];
                let limit = if item == 0 {
                    belt.length
                } else {
                    (belt.positions[item - 1] - belt.spacing).max(current)
                };
                let boundary = if current < lower - EPS {
                    lower
                } else if current < upper - EPS {
                    upper
                } else {
                    continue;
                };
                if boundary <= current + EPS
                    || boundary > limit + EPS
                    || boundary > belt.length + EPS
                {
                    continue;
                }
                let time = belt.time_to_reach(current, boundary);
                if time > EPS {
                    next = Some(next.map_or(time, |value: f64| value.min(time)));
                }
            }
        }
        next
    }

    fn schedule_belt_tick(&mut self, index: usize) {
        if self.conveyor_stopped(index) {
            return;
        }
        let stops = self.belt_stop_positions(index);
        let next = if self.objects[index].belt_slug {
            self.slug_time_to_next_event(index, &stops)
        } else {
            self.objects[index]
                .belt
                .as_ref()
                .and_then(|belt| belt.time_to_next_event_with_stops(&stops))
        };
        let photo = self.photo_eye_next_event(index);
        let decision = self.decision_point_next_event(index);
        let next_dt = [next, photo, decision]
            .into_iter()
            .flatten()
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let Some(dt) = next_dt else {
            return;
        };
        if dt <= EPS {
            return;
        }
        let at = self.now + dt;
        if at + EPS < self.objects[index].belt_tick_at {
            self.objects[index].belt_tick_at = at;
            self.schedule(at, EventKind::BeltTick(index));
        }
    }

    fn handle_belt_tick(&mut self, index: usize) -> Result<(), SimError> {
        if self.conveyor_stopped(index) {
            return Ok(());
        }
        self.objects[index].belt_tick_at = f64::INFINITY;
        self.sync_belt(index);
        self.update_decision_points(index)?;
        self.process_belt_stops(index)?;
        self.flow(index)?;
        self.notify_upstreams(index)
    }

    fn flow_conveyor(&mut self, index: usize) -> Result<(), SimError> {
        if self.conveyor_stopped(index) {
            return Ok(());
        }
        self.sync_belt(index);
        self.update_decision_points(index)?;
        self.process_belt_stops(index)?;
        loop {
            let ready = self.objects[index]
                .belt
                .as_ref()
                .is_some_and(|belt| belt.front_ready());
            if !ready {
                break;
            }
            let Some(item) = self.objects[index].belt_items.first().cloned() else {
                break;
            };
            let Some(conn) = self.route(index, &item)? else {
                self.set_state(index, State::Blocked);
                self.schedule_belt_tick(index);
                return Ok(());
            };
            if !self.deliver(index, conn, item.clone())? {
                self.set_state(index, State::Blocked);
                self.schedule_belt_tick(index);
                return Ok(());
            }
            let now = self.now;
            self.objects[index].note_content(now);
            if let Some(belt) = self.objects[index].belt.as_mut() {
                belt.remove_front();
            }
            if !self.objects[index].belt_items.is_empty() {
                self.objects[index].belt_items.remove(0);
            }
            if !self.objects[index].belt_handled.is_empty() {
                self.objects[index].belt_handled.remove(0);
            }
            if !self.objects[index].decision_handled.is_empty() {
                self.objects[index].decision_handled.remove(0);
            }
            self.objects[index].touch();
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
            self.update_photo_eyes(index)?;
            self.notify_upstreams(index)?;
        }
        if self.objects[index].belt_items.is_empty() {
            self.set_state(index, State::Idle);
            self.objects[index].belt_tick_at = f64::INFINITY;
        } else {
            self.set_state(index, State::Busy);
            self.schedule_belt_tick(index);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Combiner
    // -----------------------------------------------------------------------

    fn flow_combiner(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Combiner(params) => params.clone(),
            _ => return Ok(()),
        };
        match self.objects[index].combine_mode {
            CombinerMode::Pack => self.flow_combiner_pack(index, &params),
            CombinerMode::Join => self.flow_combiner_join(index),
            CombinerMode::Batch => self.flow_combiner_batch(index, &params),
        }
    }

    /// `pack`: wait for the container plus its components, then release the
    /// container with the components inside.
    fn flow_combiner_pack(
        &mut self,
        index: usize,
        params: &openflex_model::CombinerParams,
    ) -> Result<(), SimError> {
        if self.objects[index].container.is_none() {
            self.set_state(index, State::Idle);
            return Ok(());
        }
        if (self.objects[index].components.len() as u32) < params.components_per_container {
            // Waiting for the remaining components.
            self.set_state(index, State::Blocked);
            return Ok(());
        }
        let now = self.now;
        self.objects[index].note_content(now);
        let Some(mut container) = self.objects[index].container.take() else {
            return Ok(());
        };
        let components = std::mem::take(&mut self.objects[index].components);
        let count = components.len();
        for component in &components {
            for (label, value) in &component.labels {
                *container.labels.entry(label.clone()).or_insert(0.0) += *value;
            }
        }
        container
            .labels
            .insert(COMPONENTS_LABEL.to_string(), count as f64);
        self.objects[index].out_pending.push(container);
        self.objects[index].touch();
        self.set_state(index, State::Idle);
        Ok(())
    }

    /// `join`: permanently merge one item per input port into a single item.
    fn flow_combiner_join(&mut self, index: usize) -> Result<(), SimError> {
        let all = self.objects[index].join_slots.iter().all(Option::is_some);
        let any = self.objects[index].join_slots.iter().any(Option::is_some);
        if !all {
            self.set_state(index, if any { State::Blocked } else { State::Idle });
            return Ok(());
        }
        let now = self.now;
        self.objects[index].note_content(now);
        let mut merged: Option<FlowItem> = None;
        let mut count = 0usize;
        for slot in self.objects[index].join_slots.iter_mut() {
            if let Some(item) = slot.take() {
                count += 1;
                match merged.as_mut() {
                    None => merged = Some(item),
                    Some(base) => {
                        for (label, value) in &item.labels {
                            *base.labels.entry(label.clone()).or_insert(0.0) += *value;
                        }
                        base.entry_time = base.entry_time.min(item.entry_time);
                    }
                }
            }
        }
        if let Some(mut merged) = merged {
            merged
                .labels
                .insert(COMPONENTS_LABEL.to_string(), count as f64);
            self.objects[index].out_pending.push(merged);
        }
        self.objects[index].touch();
        self.set_state(index, State::Idle);
        Ok(())
    }

    /// `batch` (A7): collect the container plus its components, run the
    /// inherited setup/process cycle (including operators), then release every
    /// item of the batch together.
    fn flow_combiner_batch(
        &mut self,
        index: usize,
        params: &openflex_model::CombinerParams,
    ) -> Result<(), SimError> {
        if !self.objects[index].on_shift || self.objects[index].state == State::Down {
            return Ok(());
        }
        if self.objects[index].container.is_none() {
            self.set_state(index, State::Idle);
            return Ok(());
        }
        if (self.objects[index].components.len() as u32) < params.components_per_container {
            self.set_state(index, State::Blocked);
            return Ok(());
        }
        let operator_params = combiner_operator_params(params);
        // Continue after a setup whose process operator was not available yet.
        if self.objects[index].ready_for_process {
            if !self.ensure_operator(index, OperatorPhase::Process, &operator_params)? {
                return Ok(());
            }
            self.objects[index].ready_for_process = false;
            let process = self.sample(&params.process_time).max(0.0);
            self.set_phase(index, Phase::Processing);
            self.objects[index].phase_token += 1;
            let token = self.objects[index].phase_token;
            self.objects[index].phase_end = self.now + process;
            self.set_state(index, State::Busy);
            self.schedule(self.now + process, EventKind::ProcessDone(index, token));
            return Ok(());
        }
        if matches!(self.objects[index].phase, Phase::Setup | Phase::Processing) {
            return Ok(());
        }
        let setup = self.sample(&params.setup_time).max(0.0);
        let phase = if setup > 0.0 {
            OperatorPhase::Setup
        } else {
            OperatorPhase::Process
        };
        if !self.ensure_operator(index, phase, &operator_params)? {
            return Ok(());
        }
        self.objects[index].phase_token += 1;
        let token = self.objects[index].phase_token;
        let now = self.now;
        if setup > 0.0 {
            self.set_phase(index, Phase::Setup);
            self.objects[index].phase_end = now + setup;
            self.set_state(index, State::Busy);
            self.schedule(now + setup, EventKind::SetupDone(index, token));
        } else {
            let process = self.sample(&params.process_time).max(0.0);
            self.set_phase(index, Phase::Processing);
            self.objects[index].phase_end = now + process;
            self.set_state(index, State::Busy);
            self.schedule(now + process, EventKind::ProcessDone(index, token));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Separator
    // -----------------------------------------------------------------------

    fn flow_separator(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Separator(params) => params.clone(),
            _ => return Ok(()),
        };
        if self.objects[index].pending_parts.is_empty() && !self.objects[index].content.is_empty() {
            let now = self.now;
            let Some(container) = self.take_content(index, 0) else {
                return Ok(());
            };
            let outputs = self.outputs[index].len();
            if outputs == 0 {
                // Nothing to split into: put the container back and fire the
                // content-change trigger for the restored content.
                self.objects[index].push_content(now, container);
                self.fire_lifecycle_trigger(index, SimTrigger::OnContentChange);
                self.set_state(index, State::Blocked);
                return Ok(());
            }
            let components = container
                .labels
                .get(COMPONENTS_LABEL)
                .copied()
                .unwrap_or(0.0)
                .max(0.0) as usize;
            // `unpack` releases the container as well (qty + 1 items), while
            // `split` copies the item `split_count` times and does not keep
            // the container as a separate output (A6).
            let parts = match params.separate_mode {
                SeparateMode::Unpack => {
                    if params.split_count > 0 {
                        params.split_count as usize + 1
                    } else {
                        components + 1
                    }
                }
                SeparateMode::Split => (params.split_count as usize).max(1),
            };
            for part in 0..parts {
                let item = if part == 0 {
                    container.clone()
                } else {
                    self.derive_item(&container, part)
                };
                let port = (part % outputs) as u32;
                self.objects[index].pending_parts.push((item, port));
            }
            self.objects[index].touch();
        }
        let now = self.now;
        while let Some((item, port)) = self.objects[index].pending_parts.first().cloned() {
            let Some(conn) = self.conn_at_port(index, port) else {
                self.set_state(index, State::Blocked);
                return Ok(());
            };
            if !self.deliver(index, conn, item.clone())? {
                self.set_state(index, State::Blocked);
                return Ok(());
            }
            self.objects[index].note_content(now);
            self.objects[index].pending_parts.remove(0);
            self.objects[index].touch();
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
        }
        if self.objects[index].content.is_empty() && self.objects[index].pending_parts.is_empty() {
            self.set_state(index, State::Idle);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Merge and split
    // -----------------------------------------------------------------------

    fn flow_merge(&mut self, index: usize) -> Result<(), SimError> {
        let wait_for_all = match &self.model.objects[index].params {
            ObjectParams::Merge(params) => params.wait_for_all,
            _ => false,
        };
        let slots = self.objects[index].merge_slots.len();
        let filled = self.objects[index]
            .merge_slots
            .iter()
            .filter(|slot| slot.is_some())
            .count();
        let ready = if wait_for_all {
            slots > 0 && filled == slots
        } else {
            filled > 0
        };
        if !ready {
            self.set_state(
                index,
                if filled > 0 {
                    State::Blocked
                } else {
                    State::Idle
                },
            );
            return Ok(());
        }
        let now = self.now;
        self.objects[index].note_content(now);
        let mut merged: Option<FlowItem> = None;
        let mut count = 0usize;
        for slot in self.objects[index].merge_slots.iter_mut() {
            if let Some(item) = slot.take() {
                count += 1;
                match merged.as_mut() {
                    None => merged = Some(item),
                    Some(base) => {
                        for (label, value) in &item.labels {
                            *base.labels.entry(label.clone()).or_insert(0.0) += *value;
                        }
                        base.entry_time = base.entry_time.min(item.entry_time);
                    }
                }
            }
        }
        if let Some(mut merged) = merged {
            merged
                .labels
                .insert(COMPONENTS_LABEL.to_string(), count as f64);
            self.objects[index].out_pending.push(merged);
        }
        self.objects[index].touch();
        self.set_state(index, State::Idle);
        Ok(())
    }

    fn flow_split(&mut self, index: usize) -> Result<(), SimError> {
        let by_item_type = match &self.model.objects[index].params {
            ObjectParams::Split(params) => params.by_item_type,
            _ => false,
        };
        let outputs = self.outputs[index].len();
        if outputs == 0 {
            return Ok(());
        }
        while let Some(item) = self.objects[index].content.first().cloned() {
            let port = if by_item_type {
                (item.item_type as usize) % outputs
            } else {
                self.objects[index].split_cursor % outputs
            };
            let Some(conn) = self.outputs[index].get(port).copied() else {
                break;
            };
            if !self.deliver(index, conn, item.clone())? {
                self.set_state(index, State::Blocked);
                return Ok(());
            }
            if !by_item_type {
                self.objects[index].split_cursor += 1;
            }
            let now = self.now;
            self.take_content(index, 0);
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
        }
        self.set_state(index, State::Idle);
        Ok(())
    }

    /// Objects without a dedicated behaviour pass items through, which keeps
    /// `basic_fr` / `basic_te` usable while their logic comes from triggers.
    /// `item_to_fluid`: consume the flow items it receives and turn each into
    /// `fluid_per_unit * units_per_item` volume for the fluid sub-graph.
    fn flow_item_to_fluid(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::ItemToFluid(params) => params.clone(),
            _ => return Ok(()),
        };
        let per_item = params.fluid_per_unit.max(0.0) * params.units_per_item.max(0.0);
        let now = self.now;
        let mut converted = 0.0;
        // A burst cap keeps one step from looping forever on a huge backlog.
        for _ in 0..1_000 {
            let Some(item) = self.objects[index].take_content(now, 0) else {
                break;
            };
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
            converted += per_item;
        }
        if converted > 0.0 {
            self.pending_bridge_volume
                .entry(index)
                .and_modify(|volume| *volume += converted)
                .or_insert(converted);
        }
        self.set_state(index, State::Idle);
        Ok(())
    }

    /// `fluid_to_item`: pack buffered fluid into flow items and forward them.
    fn flow_fluid_to_item(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::FluidToItem(params) => params.clone(),
            _ => return Ok(()),
        };
        let per_item = params.fluid_per_item.max(EPS);
        let stock = self
            .pending_bridge_volume
            .get(&index)
            .copied()
            .unwrap_or(0.0);
        let ready = (stock / per_item).floor().min(10_000.0);
        let leftover = stock - ready * per_item;
        if leftover > EPS {
            self.pending_bridge_volume.insert(index, leftover);
        } else {
            self.pending_bridge_volume.remove(&index);
        }
        for _ in 0..ready as u64 {
            let labels = BTreeMap::new();
            let item = self.create_item(0, &labels, index)?;
            let now = self.now;
            self.objects[index].push_content(now, item);
        }
        self.flow_generic(index)
    }

    fn flow_generic(&mut self, index: usize) -> Result<(), SimError> {
        while let Some(item) = self.objects[index].content.first().cloned() {
            let Some(conn) = self.route(index, &item)? else {
                self.set_state(index, State::Blocked);
                return Ok(());
            };
            if !self.deliver(index, conn, item.clone())? {
                self.set_state(index, State::Blocked);
                return Ok(());
            }
            let now = self.now;
            self.take_content(index, 0);
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
        }
        self.set_state(index, State::Idle);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Failures
    // -----------------------------------------------------------------------

    /// task-163: charge the interval up to a state-limited failure event. The
    /// clock otherwise only settles on state changes, so the event that reaches
    /// the remaining threshold would see stale `up_time` and re-arm itself.
    fn settle_failure_clock(&mut self, index: usize) {
        let state = self.objects[index].state;
        let phase = self.objects[index].phase;
        let now = self.now;
        let checkpoint = self.objects[index].up_time_checkpoint;
        let Some(clock) = self.objects[index].failure.as_ref() else {
            return;
        };
        if clock.count_states.is_empty() {
            return;
        }
        let block_reason = self.objects[index].block_reason;
        let counted = ObjectRt::counts_up_time(clock, state, phase, block_reason);
        if let Some(clock) = self.objects[index].failure.as_mut()
            && counted
        {
            clock.up_time += (now - checkpoint).max(0.0);
        }
        self.objects[index].up_time_checkpoint = now;
    }

    fn handle_breakdown(&mut self, index: usize, generation: u64) -> Result<(), SimError> {
        self.settle_failure_clock(index);
        let Some(params) = self.processor_params(index) else {
            return Ok(());
        };
        let Some(failure) = params.failure.clone() else {
            return Ok(());
        };
        if self.objects[index].state == State::Down {
            return Ok(());
        }
        // A8: with a state-based clock the event can be stale (the object left
        // the counted states) or early (it was paused in between).
        if let Some(clock) = self.objects[index].failure.clone()
            && !clock.count_states.is_empty()
        {
            if clock.generation != generation {
                return Ok(());
            }
            if clock.up_time + EPS < clock.threshold {
                let remaining = (clock.threshold - clock.up_time).max(EPS);
                self.schedule(
                    self.now + remaining,
                    EventKind::Breakdown(index, generation),
                );
                return Ok(());
            }
        }
        self.fire_trigger(index, SimTrigger::OnBreakdown)?;
        self.pause_phase(index);
        // A9: a breakdown is one downtime request among possibly several.
        self.add_downtime(index, DowntimeSource::Breakdown);
        let repair = self.sample(&failure.mttr).max(0.0);
        self.schedule(self.now + repair, EventKind::RepairDone(index));
        Ok(())
    }

    fn handle_repair(&mut self, index: usize) -> Result<(), SimError> {
        if self.processor_params(index).is_none() {
            return Ok(());
        }
        self.fire_trigger(index, SimTrigger::OnRepair)?;
        // Re-arm the failure clock before the object recovers: the old clock is
        // exhausted and must not fire again immediately.
        self.schedule_next_failure(index, None);
        // A9: only the breakdown request clears here; other sources (shift,
        // maintenance, manual) keep the object down.
        self.clear_downtime(index, DowntimeSource::Breakdown);
        self.settle_downtime(index)?;
        self.flow(index)?;
        Ok(())
    }

    fn handle_message(&mut self, index: usize) -> Result<(), SimError> {
        let actions = self.trigger_actions(index, SimTrigger::OnMessage);
        if actions.is_empty() {
            return Ok(());
        }
        let mut probe = FlowItem::probe(self.now);
        self.apply_actions(index, &mut probe, actions)
    }

    // -----------------------------------------------------------------------
    // Shifts
    // -----------------------------------------------------------------------

    fn handle_shift_change(&mut self, index: usize) -> Result<(), SimError> {
        let shifts = self.model.objects[index].params.shifts().to_vec();
        if shifts.is_empty() {
            return Ok(());
        }
        let active = shifts::is_active(&shifts, self.now) && self.timetable_available(index);
        self.set_availability(index, active)?;
        if let Some(next) = shifts::next_transition(&shifts, self.now) {
            self.schedule(next, EventKind::ShiftChange(index));
        }
        Ok(())
    }

    /// B11: a linear availability window starts or ends.
    fn handle_timetable_change(&mut self, index: usize) -> Result<(), SimError> {
        let Some(timetable) = self.model.objects[index].params.timetable().cloned() else {
            return Ok(());
        };
        let active = self.timetable_available(index) && self.shifts_available(index);
        self.set_availability(index, active)?;
        if let Some(next) = next_timetable_transition(&timetable.windows, self.now) {
            self.schedule(next, EventKind::TimetableChange(index));
        }
        Ok(())
    }

    /// Is the object's linear availability calendar open right now?
    fn timetable_available(&self, index: usize) -> bool {
        match self.model.objects[index].params.timetable() {
            Some(table) => timetable_active(&table.windows, self.now),
            None => true,
        }
    }

    /// Is the object's shift plan open right now?
    fn shifts_available(&self, index: usize) -> bool {
        let shifts = self.model.objects[index].params.shifts();
        shifts.is_empty() || shifts::is_active(shifts, self.now)
    }

    fn set_availability(&mut self, index: usize, active: bool) -> Result<(), SimError> {
        if self.objects[index].on_shift == active {
            return Ok(());
        }
        self.objects[index].on_shift = active;
        if !active {
            // A9: going off shift is itself a downtime request.
            if !self.objects[index]
                .downtime
                .contains(&DowntimeSource::Shift)
            {
                self.objects[index].downtime.push(DowntimeSource::Shift);
            }
            self.pause_phase(index);
            if self.objects[index].state != State::Down {
                self.set_state(index, State::OffShift);
            }
            return Ok(());
        }
        // Back on shift: only recover when no other request keeps the object
        // down.
        self.objects[index]
            .downtime
            .retain(|source| *source != DowntimeSource::Shift);
        if self.objects[index]
            .downtime
            .iter()
            .any(|source| *source != DowntimeSource::Shift)
        {
            return Ok(());
        }
        if self.resume_phase(index)? {
            return Ok(());
        }
        self.set_state(index, State::Idle);
        self.flow(index)?;
        match self.kind(index) {
            ObjectKind::Resource => self.grant_waiting_resource(index)?,
            ObjectKind::Dispatcher => self.dispatch(index)?,
            _ => {}
        }
        self.notify_upstreams(index)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Triggers
    // -----------------------------------------------------------------------

    /// task-188: run an executer load/unload trigger against the requester's
    /// current item.
    ///
    /// OpenFlex transport executers are operators/dispatchers, not carriers of
    /// FlowItems, so there is no "item loaded onto the executer" to hand to the
    /// hook. The context item is the requester's `processing_item`, or else the
    /// first waiting item in its content. It is taken out and put back so the
    /// actions can mutate it in place (labels survive to the other hook).
    fn fire_executer_trigger(
        &mut self,
        executer: usize,
        requester: usize,
        trigger: SimTrigger,
    ) -> Result<(), SimError> {
        let actions = self.trigger_actions(executer, trigger);
        if actions.is_empty() {
            return Ok(());
        }
        let Some((mut item, from_processing)) = self.take_context_item(requester) else {
            return Ok(());
        };
        let location = format!("{}.{}", self.objects[executer].name, trigger.as_str());
        self.action_location = location;
        let result = self.apply_actions(executer, &mut item, actions);
        self.put_context_item(requester, item, from_processing);
        result
    }

    fn take_context_item(&mut self, requester: usize) -> Option<(FlowItem, bool)> {
        if let Some(item) = self.objects[requester].processing_item.take() {
            return Some((item, true));
        }
        if self.objects[requester].content.is_empty() {
            None
        } else {
            Some((self.objects[requester].content.remove(0), false))
        }
    }

    fn put_context_item(&mut self, requester: usize, item: FlowItem, from_processing: bool) {
        if from_processing {
            self.objects[requester].processing_item = Some(item);
        } else {
            self.objects[requester].content.insert(0, item);
        }
    }

    fn trigger_actions(&self, index: usize, trigger: SimTrigger) -> Vec<Action> {
        let params = &self.model.objects[index].params;
        params.triggers().actions(trigger)
    }

    fn trigger_actions_run(
        &mut self,
        index: usize,
        trigger: SimTrigger,
        item: &mut FlowItem,
    ) -> Result<(), SimError> {
        let actions = self.trigger_actions(index, trigger);
        if actions.is_empty() {
            return Ok(());
        }
        self.action_location = format!("{}.{}", self.objects[index].name, trigger.as_str());
        self.apply_actions(index, item, actions)
    }

    /// Fire a trigger that has no flow item of its own (breakdown, repair).
    fn fire_trigger(&mut self, index: usize, trigger: SimTrigger) -> Result<(), SimError> {
        let actions = self.trigger_actions(index, trigger);
        if actions.is_empty() {
            return Ok(());
        }
        let mut probe = FlowItem::probe(self.now);
        self.apply_actions(index, &mut probe, actions)
    }

    fn apply_actions(
        &mut self,
        index: usize,
        item: &mut FlowItem,
        actions: Vec<Action>,
    ) -> Result<(), SimError> {
        let object_name = self.objects[index].name.clone();
        for action in actions {
            match action {
                Action::SetLabel { name, value } => {
                    let value = self.eval_expr(&object_name, item, &value)?;
                    item.labels.insert(name, value.as_number().unwrap_or(0.0));
                }
                Action::SetItemType { value } => {
                    let value = self.eval_expr(&object_name, item, &value)?;
                    item.item_type = value.as_number().unwrap_or(0.0).max(0.0) as u32;
                }
                Action::IncrementLabel { name, by } => {
                    let by = self.eval_expr(&object_name, item, &by)?;
                    let entry = item.labels.entry(name).or_insert(0.0);
                    *entry += by.as_number().unwrap_or(0.0);
                }
                Action::SendMessage { target, delay } => {
                    let delay = self.eval_expr(&object_name, item, &delay)?;
                    let delay = delay.as_number().unwrap_or(0.0).max(0.0);
                    if let Some(&target_index) = self.by_name.get(&target) {
                        self.schedule(self.now + delay, EventKind::Message(target_index));
                    }
                }
                Action::DestroyItem => {
                    // Removing an item outside the normal flow is only safe at
                    // receive time; the item is simply dropped here.
                }
                Action::SetMotor { target, running } => {
                    let Some(&motor) = self.by_name.get(&target) else {
                        return Err(SimError::Config(format!(
                            "set_motor target '{target}' is not an object"
                        )));
                    };
                    self.set_motor_running(motor, running)?;
                }
                Action::SetDestination { port } => {
                    let Some(conn) = self.conn_at_port(index, port) else {
                        return Err(SimError::Config(format!(
                            "set_destination on object '{}' has no output port {port} (it has {} output ports)",
                            object_name,
                            self.outputs[index].len()
                        )));
                    };
                    if !self.outputs[index].contains(&conn) {
                        return Err(SimError::Config(format!(
                            "set_destination on object '{object_name}' port {port} is not a sendable A output"
                        )));
                    }
                    item.destination = Some(port);
                }
                Action::Script { source } => {
                    let location = if self.action_location.is_empty() {
                        format!("{object_name} script action")
                    } else {
                        format!("{} script action", self.action_location)
                    };
                    self.run_script_action(&source, &location, &object_name, item, Some(index))?;
                }
                Action::ReleaseItem => {
                    // A4: clear the hold and retry the delivery in the same
                    // instant, exactly like `sendToPort` after `releaseitem`.
                    // The bypass is only armed when there is something to
                    // release, so it cannot leak into a later item.
                    self.objects[index].hold_release = false;
                    if self.has_held_item(index) {
                        self.objects[index].release_bypass = true;
                    }
                    self.schedule(self.now, EventKind::Flow(index));
                }
            }
        }
        Ok(())
    }

    /// task-152: execute one sandboxed script action. `object_index` is the
    /// object whose hold/state the script may change; model-level triggers pass
    /// `None` (message/create/move/log still apply).
    fn run_script_action(
        &mut self,
        source: &str,
        location: &str,
        object_name: &str,
        item: &mut FlowItem,
        object_index: Option<usize>,
    ) -> Result<(), SimError> {
        let script = self.scripts.get(source).cloned().ok_or_else(|| {
            enrich_script_error(
                ScriptError::Compile("script source was not precompiled with the model".into()),
                source,
                location,
                self.now,
            )
        })?;
        let mut context = ScriptContext::new(self.now, item.item_type, item.id);
        context.labels = item.labels.clone();
        context.objects = self.script_objects();
        let outcome = script
            .run(&mut context)
            .map_err(|error| enrich_script_error(error, source, location, self.now))?;
        item.labels = outcome.labels;
        if let Some(item_type) = outcome.item_type {
            item.item_type = item_type;
        }
        for message in &outcome.messages {
            if let Some(&target) = self.by_name.get(&message.target) {
                self.schedule(
                    self.now + message.delay.max(0.0),
                    EventKind::Message(target),
                );
            }
        }
        if let Some(index) = object_index {
            if outcome.release_item {
                // Same semantics as `release_item` (A4): lift the hold and
                // retry the delivery in this instant.
                self.objects[index].hold_release = false;
                if self.has_held_item(index) {
                    self.objects[index].release_bypass = true;
                }
                self.schedule(self.now, EventKind::Flow(index));
            }
            if let Some(state_name) = &outcome.state {
                self.apply_script_state(index, state_name);
            }
        }
        for (target, count) in &outcome.create_items {
            self.apply_script_create(target, *count)?;
        }
        for (from, to) in &outcome.move_items {
            self.apply_script_move(from, to)?;
        }
        for log in &outcome.logs {
            if self.trace.is_some() {
                let time = self.now;
                let seq = self.next_seq;
                self.next_seq += 1;
                if let Some(trace) = self.trace.as_mut() {
                    trace.push(
                        TraceEvent::new(time, seq, "script_log", object_name.to_string())
                            .with_message(log.clone()),
                    );
                }
            }
        }
        Ok(())
    }

    fn eval_expr(
        &self,
        object_name: &str,
        item: &FlowItem,
        expr: &openflex_model::Expr,
    ) -> Result<Value, SimError> {
        let context = ExprContext::new(self, Some(item));
        expr.eval(&context).map_err(|err| SimError::Expression {
            location: format!("{object_name} trigger"),
            detail: err.to_string(),
        })
    }

    /// Evaluate a sandboxed script condition (truthy when it sets the numeric
    /// label `result` to a positive value).
    fn eval_script_condition(
        &self,
        source: &str,
        location: &str,
        item: Option<&FlowItem>,
    ) -> Result<bool, SimError> {
        let script = self.scripts.get(source).ok_or_else(|| {
            enrich_script_error(
                ScriptError::Compile("script source was not precompiled with the model".into()),
                source,
                location,
                self.now,
            )
        })?;
        let mut context = ScriptContext::new(
            self.now,
            item.map_or(0, |item| item.item_type),
            item.map_or(0, |item| item.id),
        );
        if let Some(item) = item {
            context.labels = item.labels.clone();
        }
        context.objects = self.script_objects();
        let outcome = script
            .run(&mut context)
            .map_err(|error| enrich_script_error(error, source, location, self.now))?;
        Ok(outcome
            .condition
            .unwrap_or_else(|| outcome.labels.get("result").copied().unwrap_or(0.0) > 0.0))
    }

    /// Read-only object snapshot for script queries.
    fn script_objects(&self) -> BTreeMap<String, ScriptObjectInfo> {
        let elapsed = self.now.max(EPS);
        self.objects
            .iter()
            .map(|object| {
                let stats = object.stats(elapsed);
                (
                    object.name.clone(),
                    ScriptObjectInfo {
                        content: (object.content.len() + object.out_pending.len()) as f64,
                        state: object.state.name().to_string(),
                        utilization: stats.utilization,
                    },
                )
            })
            .collect()
    }

    fn apply_script_state(&mut self, index: usize, name: &str) {
        let state = match name {
            "idle" => State::Idle,
            "busy" => State::Busy,
            "blocked" => State::Blocked,
            "down" => State::Down,
            "off_shift" => State::OffShift,
            _ => return,
        };
        self.set_state(index, state);
    }

    fn apply_script_create(&mut self, name: &str, count: u32) -> Result<(), SimError> {
        let Some(&target) = self.by_name.get(name) else {
            return Ok(());
        };
        for _ in 0..count {
            if !self.can_accept(target, 0) {
                break;
            }
            self.items_created += 1;
            self.stats_created += 1;
            let item = FlowItem {
                id: self.items_created,
                item_type: 0,
                labels: BTreeMap::new(),
                created: self.now,
                entry_time: self.now,
                destination: None,
            };
            self.receive_item(target, 0, item)?;
        }
        Ok(())
    }

    fn apply_script_move(&mut self, from: &str, to: &str) -> Result<(), SimError> {
        let (Some(&from), Some(&to)) = (self.by_name.get(from), self.by_name.get(to)) else {
            return Ok(());
        };
        if !self.can_accept(to, 0) {
            return Ok(());
        }
        let Some(item) = self.take_content(from, 0) else {
            return Ok(());
        };
        let stay = self.now - item.entry_time;
        {
            let state = &mut self.objects[from];
            state.acc.output += 1;
            state.acc.stay_sum += stay;
            state.acc.stay_count += 1;
        }
        self.receive_item(to, 0, item)?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Statistics
    // -----------------------------------------------------------------------

    fn reset_stats(&mut self) {
        self.stats_created = 0;
        self.stats_completed = 0;
        if let Some(flow) = self.process_flow.as_mut() {
            flow.tokens_created = 0;
            flow.tokens_completed = 0;
            for block in &mut flow.blocks {
                block.input = 0;
                block.output = 0;
                block.content_integral = 0.0;
                block.stay_sum = 0.0;
                block.stay_count = 0;
                block.last_content_change = self.now;
            }
        }
        if let Some(fluid) = self.fluid.as_mut() {
            fluid.total_inflow = 0.0;
            fluid.total_throughput = 0.0;
            fluid.total_overflow = 0.0;
            fluid.last_time = self.now;
            for state in &mut fluid.state {
                state.level_integral = 0.0;
                state.level = state.buffer.iter().map(|(_, volume)| volume).sum();
                state.level_max = state.level;
                state.throughput = 0.0;
                state.overflow = 0.0;
                state.inflow = 0.0;
            }
        }
        for object in &mut self.objects {
            object.slot_busy = 0.0;
            object.acc = Accum::default();
            object.last_state_change = self.now;
            object.last_content_change = self.now;
            object.phase_start = self.now;
        }
        // `on_reset` runs once per object after the warm-up boundary cleared
        // the statistics.
        for index in 0..self.objects.len() {
            self.fire_lifecycle_trigger(index, SimTrigger::OnReset);
        }
        let actions = self.model.model_triggers.on_reset.clone();
        self.fire_model_actions("model_triggers.on_reset", actions);
    }
}

/// B12: map an event to the object it belongs to, a stable label and one
/// optional numeric detail.
#[allow(clippy::type_complexity)]
fn describe_event(kind: &EventKind) -> (Option<usize>, &'static str, Option<(&'static str, f64)>) {
    match kind {
        EventKind::Arrive(index) => (Some(*index), "arrive", None),
        EventKind::Flow(index) => (Some(*index), "flow", None),
        EventKind::SetupDone(index, token) => {
            (Some(*index), "setup_done", Some(("token", *token as f64)))
        }
        EventKind::ProcessDone(index, token) => {
            (Some(*index), "process_done", Some(("token", *token as f64)))
        }
        EventKind::Breakdown(index, generation) => (
            Some(*index),
            "breakdown",
            Some(("generation", *generation as f64)),
        ),
        EventKind::RepairDone(index) => (Some(*index), "repair_done", None),
        EventKind::Message(index) => (Some(*index), "message", None),
        EventKind::MpDone(index, lane, _) => {
            (Some(*index), "mp_done", Some(("lane", *lane as f64)))
        }
        EventKind::BeltTick(index) => (Some(*index), "belt_tick", None),
        EventKind::BatchTimeout(index) => (Some(*index), "batch_timeout", None),
        EventKind::DwellDone(index) => (Some(*index), "dwell_done", None),
        EventKind::EmptierSweep(index) => (Some(*index), "emptier_sweep", None),
        EventKind::TransportSettle(transporter) => (Some(*transporter), "transport_settle", None),
        EventKind::RackPick(index) => (Some(*index), "rack_pick", None),
        EventKind::TaskStepDone(transporter) => (Some(*transporter), "task_step_done", None),
        EventKind::PersonStep(transporter) => (Some(*transporter), "person_step", None),
        EventKind::ChargeTravel(transporter) => (Some(*transporter), "charge_travel", None),
        EventKind::PreemptRetry(dispatcher, requester) => (
            Some(*dispatcher),
            "preempt_retry",
            Some(("requester", *requester as f64)),
        ),
        EventKind::ShiftChange(index) => (Some(*index), "shift_change", None),
        EventKind::TimetableChange(index) => (Some(*index), "timetable_change", None),
        EventKind::TransportPickup(transporter, requester) => (
            Some(*transporter),
            "transport_pickup",
            Some(("requester", *requester as f64)),
        ),
        EventKind::TransportArrive(transporter, requester) => (
            Some(*transporter),
            "transport_arrive",
            Some(("requester", *requester as f64)),
        ),
        EventKind::OperatorLoaded(transporter, requester) => (
            Some(*transporter),
            "operator_loaded",
            Some(("requester", *requester as f64)),
        ),
        EventKind::OperatorReleased(transporter, requester) => (
            Some(*transporter),
            "operator_released",
            Some(("requester", *requester as f64)),
        ),
        EventKind::TransportHome(transporter) => (Some(*transporter), "transport_home", None),
        EventKind::ResourceRetry(resource) => (Some(*resource), "resource_retry", None),
        EventKind::Pull(index) => (Some(*index), "pull", None),
        EventKind::FlowCreate(block) => (None, "flow_create", Some(("block", *block as f64))),
        EventKind::FlowAdvance(block) => (None, "flow_advance", Some(("block", *block as f64))),
        EventKind::FlowDelayDone(block, _) => {
            (None, "flow_delay_done", Some(("block", *block as f64)))
        }
        EventKind::FluidStep => (None, "fluid_step", None),
        EventKind::ChargeDone(executor) => (
            Some(*executor),
            "agv_charged",
            Some(("executor", *executor as f64)),
        ),
        EventKind::TransportResume(executor, requester, stage) => (
            Some(*executor),
            "agv_resume",
            Some(("requester", *requester as f64 + *stage as f64 * 0.0)),
        ),
        EventKind::TrafficWait(transporter, requester, stage) => (
            Some(*transporter),
            "traffic_wait",
            Some(("requester", *requester as f64 + *stage as f64 * 0.0)),
        ),
        EventKind::TrackedSample => (None, "tracked_sample", None),
        EventKind::TransportDepart(transporter, requester) => (
            Some(*transporter),
            "transport_depart",
            Some(("requester", *requester as f64)),
        ),
        EventKind::ExplicitLegDone(transporter) => (Some(*transporter), "explicit_leg_done", None),
        EventKind::ExplicitTrafficRetry(transporter) => {
            (Some(*transporter), "explicit_traffic_retry", None)
        }
        EventKind::ResetStats => (None, "reset_stats", None),
    }
}

/// Shortest distance between two named network nodes (B4).
///
/// Edges are bidirectional; the graph is small, so Dijkstra uses a linear scan
/// for the next node instead of a priority queue.
/// FlexSim `NetworkNode` routing: the shortest path over the model's nodes.
///
/// Nodes connect to each other with ordinary A connections; an edge is as long
/// as the straight line between the two positions unless the node overrides it
/// with an explicit `edges` entry. A leg additionally walks straight from the
/// endpoint object to its nearest node and back out again.
fn network_distance_positions(
    model: &Model,
    from_position: [f64; 2],
    to_position: [f64; 2],
) -> Option<f64> {
    let nodes: Vec<(&str, [f64; 2])> = model
        .objects
        .iter()
        .filter(|object| object.kind == ObjectKind::NetworkNode)
        .map(|object| (object.name.as_str(), object.pos))
        .collect();
    if nodes.len() < 2 {
        return None;
    }
    // Edges: explicit ones from the node parameters, then every A connection
    // between two nodes (euclidean unless already covered by an explicit edge).
    let mut edges: Vec<NetworkEdge> = Vec::new();
    for object in model
        .objects
        .iter()
        .filter(|object| object.kind == ObjectKind::NetworkNode)
    {
        if let ObjectParams::NetworkNode(params) = &object.params {
            edges.extend(params.edges.iter().cloned());
        }
    }
    // A plain function rather than a closure so `edges` stays free to grow.
    fn connects(edges: &[NetworkEdge], from_name: &str, to_name: &str) -> bool {
        edges.iter().any(|edge| {
            (edge.from == from_name && edge.to == to_name)
                || (!edge.directed && edge.from == to_name && edge.to == from_name)
        })
    }
    for connection in &model.connections {
        let Some(from_source) = model.object(&connection.from) else {
            continue;
        };
        let Some(to_target) = model.object(&connection.to) else {
            continue;
        };
        if from_source.kind != ObjectKind::NetworkNode || to_target.kind != ObjectKind::NetworkNode
        {
            continue;
        }
        if connects(&edges, &connection.from, &connection.to) {
            continue;
        }
        edges.push(NetworkEdge {
            from: connection.from.clone(),
            to: connection.to.clone(),
            distance: distance(from_source.pos, to_target.pos),
            directed: false,
        });
    }
    let nearest = |position: [f64; 2]| -> &str {
        nodes
            .iter()
            .min_by(|left, right| {
                distance(left.1, position)
                    .partial_cmp(&distance(right.1, position))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|node| node.0)
            .unwrap_or("")
    };
    let from_node = nearest(from_position);
    let to_node = nearest(to_position);
    if from_node.is_empty() || to_node.is_empty() {
        return None;
    }
    if from_node == to_node {
        return Some(distance(from_position, to_position));
    }
    let path = shortest_network_distance(&edges, from_node, to_node)?;
    // Use the actual start/end node positions (not model order): the path may
    // run in either direction.
    let node_position = |name: &str| nodes.iter().find(|node| node.0 == name).map(|node| node.1);
    let lead = node_position(from_node).map_or(0.0, |start| distance(from_position, start));
    let trail = node_position(to_node).map_or(0.0, |end| distance(end, to_position));
    Some(path + lead + trail)
}

fn node_network_distance(model: &Model, from: &ObjectRt, to: &ObjectRt) -> Option<f64> {
    network_distance_positions(model, from.position, to.position)
}

fn ready_position(port: usize, start: usize, ports: usize) -> usize {
    (port + ports - start) % ports
}

fn slug_groups(positions: &[f64], min_gap: f64) -> Vec<(usize, usize)> {
    if positions.is_empty() {
        return Vec::new();
    }
    let mut groups = Vec::new();
    let mut start = 0;
    for index in 1..positions.len() {
        if positions[index - 1] - positions[index] >= min_gap - EPS {
            groups.push((start, index));
            start = index;
        }
    }
    groups.push((start, positions.len()));
    groups
}

fn route_distance(model: &Model, from: [f64; 2], to: [f64; 2]) -> f64 {
    network_distance_positions(model, from, to).unwrap_or_else(|| distance(from, to))
}

fn shortest_network_distance(edges: &[NetworkEdge], from: &str, to: &str) -> Option<f64> {
    if from == to {
        return Some(0.0);
    }
    let mut adjacency: std::collections::HashMap<&str, Vec<(&str, f64)>> =
        std::collections::HashMap::new();
    for edge in edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push((edge.to.as_str(), edge.distance));
        if !edge.directed {
            adjacency
                .entry(edge.to.as_str())
                .or_default()
                .push((edge.from.as_str(), edge.distance));
        }
    }
    let mut distances: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
    let mut visited: std::collections::HashMap<&str, bool> = std::collections::HashMap::new();
    distances.insert(from, 0.0);
    loop {
        let mut current: Option<(&str, f64)> = None;
        for (node, distance) in &distances {
            if visited.get(*node).copied().unwrap_or(false) {
                continue;
            }
            if current.is_none_or(|(_, best)| *distance < best) {
                current = Some((*node, *distance));
            }
        }
        let (node, node_distance) = current?;
        if node == to {
            return Some(node_distance);
        }
        visited.insert(node, true);
        if let Some(neighbours) = adjacency.get(node) {
            for (next, weight) in neighbours {
                let candidate = node_distance + weight;
                let entry = distances.entry(*next).or_insert(f64::INFINITY);
                if candidate < *entry {
                    *entry = candidate;
                }
            }
        }
    }
}

/// Euclidean distance between two points (task-133 helper).
fn euclidean(from: [f64; 2], to: [f64; 2]) -> f64 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    (dx * dx + dy * dy).sqrt()
}

/// Euclidean distance between two object positions.
fn distance(from: [f64; 2], to: [f64; 2]) -> f64 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    (dx * dx + dy * dy).sqrt()
}

/// Is `now` inside one of the linear availability windows (B11)?
fn timetable_active(windows: &[DowntimeWindow], now: f64) -> bool {
    windows.is_empty()
        || windows
            .iter()
            .any(|window| now + EPS >= window.start && now < window.end)
}

/// The first availability-calendar boundary strictly after `after` (B11).
fn next_timetable_transition(windows: &[DowntimeWindow], after: f64) -> Option<f64> {
    windows
        .iter()
        .flat_map(|window| [window.start, window.end])
        .filter(|time| *time > after + EPS)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

/// The end of the downtime window that contains `now`, if any.
fn downtime_until(windows: &[DowntimeWindow], now: f64) -> Option<f64> {
    windows
        .iter()
        .find(|window| now + EPS >= window.start && now < window.end)
        .map(|window| window.end)
}

/// Expression context used by routing conditions and triggers.
struct ExprContext<'a, 'm> {
    sim: &'a Simulation<'m>,
    item: Option<&'a FlowItem>,
}

impl<'a, 'm> ExprContext<'a, 'm> {
    fn new(sim: &'a Simulation<'m>, item: Option<&'a FlowItem>) -> Self {
        Self { sim, item }
    }
}

impl EvalContext for ExprContext<'_, '_> {
    fn global_variable(&self, name: &str) -> Option<Value> {
        let variable = self.sim.model.global_variable(name)?;
        Some(match variable.kind {
            GlobalVariableKind::Number => Value::Number(variable.value.as_f64()?),
            GlobalVariableKind::String => Value::Str(variable.value.as_str()?.to_string()),
        })
    }

    fn picklist_entry(&self, list: &str, entry: &str) -> PickListLookup {
        let Some(picklist) = self.sim.model.picklist(list) else {
            return PickListLookup::UnknownList;
        };
        match picklist.value_of(entry) {
            Some(value) => PickListLookup::Value(value.to_string()),
            None => PickListLookup::UnknownEntry,
        }
    }

    fn global_table(&self, name: &str) -> Option<&openflex_model::GlobalTable> {
        self.sim.model.table(name)
    }

    /// Advertised so `query()` can tell "no table here" from "this context has
    /// no tables at all" instead of reporting a bogus `unknown table`.
    fn supports_global_tables(&self) -> bool {
        true
    }

    fn lookup(&self, path: &str) -> Option<Value> {
        match path {
            "time" | "sim.time" | "now" => return Some(Value::Number(self.sim.now)),
            "created" | "sim.created" | "sim.items_created" => {
                return Some(Value::Number(self.sim.stats_created as f64));
            }
            "completed" | "sim.completed" => {
                return Some(Value::Number(self.sim.stats_completed as f64));
            }
            _ => {}
        }
        if let Some(rest) = path.strip_prefix("item.") {
            let item = self.item?;
            return match rest {
                "type" => Some(Value::Number(f64::from(item.item_type))),
                "id" => Some(Value::Number(item.id as f64)),
                "created" => Some(Value::Number(item.created)),
                "age" => Some(Value::Number(self.sim.now - item.created)),
                _ => rest
                    .strip_prefix("label.")
                    .and_then(|label| item.labels.get(label).copied())
                    .map(Value::Number),
            };
        }
        if let Some(rest) = path.strip_prefix("var.") {
            return self
                .item
                .and_then(|item| item.labels.get(rest).copied())
                .map(Value::Number);
        }
        if let Some(rest) = path.strip_prefix("table.") {
            // Read-only global table access:
            //   `table.<Name>.<column>`      first row
            //   `table.<Name>.<row>.<column>`
            // The `table(name, row, column)` function resolves to the second
            // form, so both spellings share one implementation.
            let parts: Vec<&str> = rest.split('.').collect();
            return match parts.as_slice() {
                [name, column] => self.sim.table_cell(name, 0, column),
                [name, row, column] => row
                    .parse::<usize>()
                    .ok()
                    .and_then(|row| self.sim.table_cell(name, row, column)),
                _ => None,
            };
        }
        if let Some(rest) = path.strip_prefix("object.") {
            let (name, field) = rest.split_once('.')?;
            let index = *self.sim.by_name.get(name)?;
            return object_field(self.sim, index, field);
        }
        // `Object.field` shorthand.
        if let Some((name, field)) = path.split_once('.')
            && let Some(&index) = self.sim.by_name.get(name)
        {
            return object_field(self.sim, index, field);
        }
        None
    }
}

fn object_field(sim: &Simulation<'_>, index: usize, field: &str) -> Option<Value> {
    let object = &sim.objects[index];
    match field {
        "content" | "wip" => Some(Value::Number(object.held() as f64)),
        "state" => Some(Value::Str(object.state.name().to_string())),
        "input" | "inputs" => Some(Value::Number(object.acc.input as f64)),
        "output" | "outputs" => Some(Value::Number(object.acc.output as f64)),
        "idle" => Some(Value::Number(object.acc.states.idle)),
        "busy" => Some(Value::Number(object.acc.states.busy)),
        "blocked" => Some(Value::Number(object.acc.states.blocked)),
        "down" => Some(Value::Number(object.acc.states.down)),
        "off_shift" => Some(Value::Number(object.acc.states.off_shift)),
        "on_shift" => Some(Value::Bool(object.on_shift)),
        _ => None,
    }
}

/// A* / BFS distance on a walking grid; `None` when a point is outside the
/// grid, blocked or unreachable (callers fall back to network/euclidean).
/// task-118: the cell sequence of the shortest 8-neighbour path.
///
/// Uses exactly the same cost model as [`grid_distance`] (straight 1000,
/// diagonal 1414, obstacles blocked), so the path length and the distance
/// always agree. Returns the cells including the start and the goal.
fn grid_path(grid: &Grid, from: [f64; 2], to: [f64; 2]) -> Option<Vec<[usize; 2]>> {
    let size = grid.cell_size.max(EPS);
    let cell = |point: [f64; 2]| -> Option<(i32, i32)> {
        let x = (point[0] / size).floor() as i32;
        let y = (point[1] / size).floor() as i32;
        if x < 0 || y < 0 || x >= grid.width as i32 || y >= grid.height as i32 {
            None
        } else {
            Some((x, y))
        }
    };
    let start = cell(from)?;
    let goal = cell(to)?;
    let blocked: std::collections::HashSet<(i32, i32)> = grid
        .obstacles
        .iter()
        .map(|cell| (cell[0], cell[1]))
        .collect();
    if blocked.contains(&start) || blocked.contains(&goal) {
        return None;
    }
    let mut best: std::collections::HashMap<(i32, i32), u64> = std::collections::HashMap::new();
    let mut previous: std::collections::HashMap<(i32, i32), (i32, i32)> =
        std::collections::HashMap::new();
    let mut heap = std::collections::BinaryHeap::new();
    best.insert(start, 0);
    heap.push(std::cmp::Reverse((0u64, start)));
    while let Some(std::cmp::Reverse((cost, current))) = heap.pop() {
        if current == goal {
            let mut path = vec![current];
            let mut node = current;
            while let Some(&prev) = previous.get(&node) {
                path.push(prev);
                node = prev;
            }
            path.reverse();
            return Some(
                path.into_iter()
                    .map(|(x, y)| [x as usize, y as usize])
                    .collect(),
            );
        }
        if best.get(&current).is_some_and(|&known| cost > known) {
            continue;
        }
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let next = (current.0 + dx, current.1 + dy);
            if next.0 < 0
                || next.1 < 0
                || next.0 >= grid.width as i32
                || next.1 >= grid.height as i32
                || blocked.contains(&next)
            {
                continue;
            }
            let step_cost = if dx != 0 && dy != 0 { 1414 } else { 1000 };
            let next_cost = cost + step_cost;
            if best.get(&next).is_none_or(|&known| next_cost < known) {
                best.insert(next, next_cost);
                previous.insert(next, current);
                heap.push(std::cmp::Reverse((next_cost, next)));
            }
        }
    }
    None
}

fn grid_distance(grid: &Grid, from: [f64; 2], to: [f64; 2]) -> Option<f64> {
    let size = grid.cell_size.max(EPS);
    let cell = |point: [f64; 2]| -> Option<(i32, i32)> {
        let x = (point[0] / size).floor() as i32;
        let y = (point[1] / size).floor() as i32;
        if x < 0 || y < 0 || x >= grid.width as i32 || y >= grid.height as i32 {
            None
        } else {
            Some((x, y))
        }
    };
    let start = cell(from)?;
    let goal = cell(to)?;
    let blocked: std::collections::HashSet<(i32, i32)> = grid
        .obstacles
        .iter()
        .map(|cell| (cell[0], cell[1]))
        .collect();
    if blocked.contains(&start) || blocked.contains(&goal) {
        return None;
    }
    // Dijkstra with 8-neighbour moves: straight 1000, diagonal 1414 units.
    let mut best: std::collections::HashMap<(i32, i32), u64> = std::collections::HashMap::new();
    let mut heap = std::collections::BinaryHeap::new();
    best.insert(start, 0);
    heap.push(std::cmp::Reverse((0u64, start)));
    while let Some(std::cmp::Reverse((cost, (x, y)))) = heap.pop() {
        if (x, y) == goal {
            return Some(cost as f64 / 1000.0 * size);
        }
        if best.get(&(x, y)).is_some_and(|&known| cost > known) {
            continue;
        }
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let next = (x + dx, y + dy);
            if next.0 < 0
                || next.1 < 0
                || next.0 >= grid.width as i32
                || next.1 >= grid.height as i32
                || blocked.contains(&next)
            {
                continue;
            }
            if dx != 0
                && dy != 0
                && (blocked.contains(&(x + dx, y)) || blocked.contains(&(x, y + dy)))
            {
                continue;
            }
            let step = if dx != 0 && dy != 0 { 1414 } else { 1000 };
            let next_cost = cost + step;
            if best.get(&next).is_none_or(|&known| next_cost < known) {
                best.insert(next, next_cost);
                heap.push(std::cmp::Reverse((next_cost, next)));
            }
        }
    }
    None
}

/// Whether an object kind can execute dispatcher tasks (B4/B6).
fn is_task_executer_kind(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::Transporter
            | ObjectKind::Person
            | ObjectKind::Agv
            | ObjectKind::Elevator
            | ObjectKind::Robot
            | ObjectKind::Crane
            | ObjectKind::AsrsVehicle
    )
}

/// Whether an object kind belongs to the fluid family (B5).
fn is_fluid_kind(kind: ObjectKind) -> bool {
    matches!(
        kind,
        ObjectKind::FluidGenerator
            | ObjectKind::FluidTank
            | ObjectKind::FluidValve
            | ObjectKind::FluidSink
            | ObjectKind::FluidPipe
            | ObjectKind::Ticker
            | ObjectKind::FluidMixer
            | ObjectKind::FluidBlender
            | ObjectKind::FluidSplitter
            | ObjectKind::FluidProcessor
    )
}

/// task-89: Darcy-Weisbach flow law for a pipe.
///
/// With `diameter`, `friction_factor` and `density` set, the pipe resistance is
/// `R = f·L / (D·2·ρ·A²)` and the flow follows `Q = sqrt(Δp / R)`. The density
/// is corrected for temperature with the linear approximation
/// `ρ(T) = ρ0·(1 − β·T)` (`β` = `temperature_coefficient`, default 0.0002/K).
/// Without the three parameters the caller falls back to the `flow_rate`
/// calibration, so every existing model keeps its behaviour.
/// task-92: capacity of a fluid pipe.
///
/// The Darcy law applies when the pipe declares `diameter`, `friction_factor`
/// and `density` **and** a pressure difference is available; otherwise the
/// caller's linear `flow_rate` fallback is returned unchanged.
/// Effective opening fraction of a valve from its `opening`/`open_curve`.
///
/// `Linear` scales the area with the opening; `EqualPercentage` uses the
/// standard `R^(x-1)` characteristic (rangeability `R = 50`), normalised so a
/// fully open valve passes everything and a closed one nothing.
fn valve_effective_fraction(
    ratio: f64,
    opening: Option<f64>,
    curve: Option<&openflex_model::ValveOpeningCurve>,
) -> f64 {
    let ratio = ratio.clamp(0.0, 1.0);
    let Some(opening) = opening else {
        return ratio;
    };
    let opening = opening.clamp(0.0, 1.0);
    if opening <= 0.0 {
        return 0.0;
    }
    let characteristic = match curve {
        Some(openflex_model::ValveOpeningCurve::EqualPercentage) => {
            const RANGEABILITY: f64 = 50.0;
            RANGEABILITY.powf(opening - 1.0)
        }
        _ => opening,
    };
    (ratio * characteristic).clamp(0.0, 1.0)
}

/// task-153: internal volume of a pipe (`π/4 · d² · length`) when its
/// diameter is known. `None` means the pipe is treated as loss-free
/// (unbounded), which keeps every existing model unchanged.
fn pipe_volume(pipe: &openflex_model::FluidPipeParams) -> Option<f64> {
    let diameter = pipe.diameter?;
    if diameter <= 0.0 || pipe.length <= 0.0 {
        return None;
    }
    let area = std::f64::consts::PI * diameter * diameter / 4.0;
    Some(area * pipe.length)
}

fn pipe_capacity(
    pipe: &openflex_model::FluidPipeParams,
    delta_p: Option<f64>,
    fallback: f64,
) -> f64 {
    match delta_p {
        Some(delta) => darcy_flow(pipe, delta).unwrap_or(fallback),
        None => fallback,
    }
}

fn darcy_flow(pipe: &openflex_model::FluidPipeParams, delta_p: f64) -> Option<f64> {
    let diameter = pipe.diameter?;
    let friction = pipe.friction_factor?;
    let density = pipe.density?;
    if diameter <= 0.0 || friction <= 0.0 || density <= 0.0 {
        return None;
    }
    let beta = pipe.temperature_coefficient.unwrap_or(0.0002);
    let temperature = pipe.temperature.unwrap_or(0.0);
    let rho = density * (1.0 - beta * temperature).max(0.0);
    if rho <= 0.0 {
        return None;
    }
    let area = std::f64::consts::PI * diameter * diameter / 4.0;
    let resistance = friction * pipe.length.max(EPS) / (diameter * 2.0 * rho * area * area);
    if resistance <= 0.0 {
        return None;
    }
    Some((delta_p.max(0.0) / resistance).sqrt())
}

fn fluid_kind_name(kind: ObjectKind) -> &'static str {
    match kind {
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
        _ => "fluid",
    }
}

/// Volume a generator produces in one step, honouring its stop time.
fn generator_volume(params: &openflex_model::FluidGeneratorParams, now: f64, dt: f64) -> f64 {
    let rate = params.flow_rate.max(0.0);
    match params.stop_time {
        None => rate * dt,
        Some(stop) => {
            let start = now - dt;
            if start >= stop {
                0.0
            } else {
                rate * (stop.min(now) - start).max(0.0)
            }
        }
    }
}

/// Split `volume` equally over the outgoing fluid links, capped by each link's
/// `flow_rate`; returns the volume actually sent and adds it to the targets.
fn distribute_fluid(
    links: &[FluidLinkRt],
    outgoing: &[usize],
    volume: f64,
    dt: f64,
    influx: &mut [f64],
    acceptance: &mut [f64],
) -> f64 {
    if outgoing.is_empty() || volume <= 0.0 {
        return 0.0;
    }
    let share = volume / outgoing.len() as f64;
    let mut sent = 0.0;
    for &link in outgoing {
        let target = links[link].to;
        let capacity = links[link]
            .flow_rate
            .map(|rate| rate * dt)
            .unwrap_or(f64::INFINITY);
        let available = acceptance
            .get(target)
            .copied()
            .unwrap_or(f64::INFINITY)
            .max(0.0);
        let amount = share.min(capacity).min(available);
        if let Some(slot) = influx.get_mut(target) {
            *slot += amount;
        }
        if let Some(slot) = acceptance.get_mut(target)
            && slot.is_finite()
        {
            *slot = (*slot - amount).max(0.0);
        }
        sent += amount;
    }
    sent
}

/// task-148: resolve the traffic-control areas once. A control's members are
/// the `network_node` objects connected to it by A connections (direction is
/// irrelevant, S links are resource plumbing and never members).
fn build_traffic_controls(model: &Model) -> Vec<TrafficControlRt> {
    let mut controls = Vec::new();
    for (index, object) in model.objects.iter().enumerate() {
        let ObjectParams::TrafficControl(params) = &object.params else {
            continue;
        };
        let mut members = std::collections::HashSet::new();
        for connection in &model.connections {
            if connection.kind.unwrap_or_default() == ConnectionKind::S {
                continue;
            }
            let (Some(from), Some(to)) = (
                model
                    .objects
                    .iter()
                    .position(|candidate| candidate.name == connection.from),
                model
                    .objects
                    .iter()
                    .position(|candidate| candidate.name == connection.to),
            ) else {
                continue;
            };
            if from == index && model.objects[to].kind == ObjectKind::NetworkNode {
                members.insert(connection.to.clone());
            } else if to == index && model.objects[from].kind == ObjectKind::NetworkNode {
                members.insert(connection.from.clone());
            }
        }
        controls.push(TrafficControlRt {
            capacity: params.capacity.max(1),
            mutual_exclusion: params.mode == TrafficMode::MutualExclusion,
            speed_factor: params.speed_factor.max(EPS),
            members,
        });
    }
    controls
}

/// Collect every sandboxed script source in the model with a diagnostic
/// location (D2).
fn collect_model_scripts(model: &Model) -> Vec<(String, String)> {
    let mut sources = Vec::new();
    fn push(sources: &mut Vec<(String, String)>, source: &str, location: String) {
        if !source.trim().is_empty() {
            sources.push((source.to_string(), location));
        }
    }
    for object in &model.objects {
        let triggers = object.params.triggers();
        let lists = [
            ("on_create", &triggers.on_create),
            ("on_entry", &triggers.on_entry),
            ("on_exit", &triggers.on_exit),
            ("on_receive", &triggers.on_receive),
            ("on_message", &triggers.on_message),
            ("on_breakdown", &triggers.on_breakdown),
            ("on_repair", &triggers.on_repair),
            ("on_photo_eye", &triggers.on_photo_eye),
            ("on_content_change", &triggers.on_content_change),
            ("on_state_change", &triggers.on_state_change),
            ("on_reset", &triggers.on_reset),
            ("on_stop", &triggers.on_stop),
            ("on_resume", &triggers.on_resume),
            ("on_draw", &triggers.on_draw),
            ("setup_finish", &triggers.on_setup_finish),
            ("process_finish", &triggers.on_process_finish),
            // task-188: these two fire on the transporter path; leaving them out
            // made a `script` action there fail with "not precompiled" even
            // though validation accepted it.
            ("on_load", &triggers.on_load),
            ("on_unload", &triggers.on_unload),
            ("on_empty", &triggers.on_empty),
            ("on_full", &triggers.on_full),
            ("on_entry_request", &triggers.on_entry_request),
        ];
        for (trigger, actions) in lists {
            for (position, action) in actions.iter().enumerate() {
                if let Action::Script { source } = action {
                    push(
                        &mut sources,
                        source,
                        format!("objects.{}.triggers.{trigger}[{position}]", object.name),
                    );
                }
            }
        }
        if let ObjectParams::Conveyor(params) = &object.params {
            for (point_index, point) in params.decision_points.iter().enumerate() {
                for (position, action) in point.actions.iter().enumerate() {
                    if let Action::Script { source } = action {
                        push(
                            &mut sources,
                            source,
                            format!(
                                "objects.{}.decision_points[{point_index}].actions[{position}]",
                                object.name
                            ),
                        );
                    }
                }
            }
            for (eye_index, eye) in params.photo_eyes.iter().enumerate() {
                for (trigger, actions) in [
                    ("on_blocked", &eye.triggers.on_blocked),
                    ("on_cleared", &eye.triggers.on_cleared),
                ] {
                    for (position, action) in actions.iter().enumerate() {
                        if let Action::Script { source } = action {
                            push(
                                &mut sources,
                                source,
                                format!(
                                    "objects.{}.photo_eyes[{eye_index}].triggers.{trigger}[{position}]",
                                    object.name
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
    for (trigger, actions) in [
        ("on_start", &model.model_triggers.on_start),
        ("on_reset", &model.model_triggers.on_reset),
        ("on_stop", &model.model_triggers.on_stop),
        (
            "start_of_experiment",
            &model.model_triggers.start_of_experiment,
        ),
        ("end_of_experiment", &model.model_triggers.end_of_experiment),
        ("start_of_scenario", &model.model_triggers.start_of_scenario),
        ("end_of_scenario", &model.model_triggers.end_of_scenario),
        (
            "start_of_replication",
            &model.model_triggers.start_of_replication,
        ),
        (
            "end_of_replication",
            &model.model_triggers.end_of_replication,
        ),
    ] {
        for (position, action) in actions.iter().enumerate() {
            if let Action::Script { source } = action {
                push(
                    &mut sources,
                    source,
                    format!("model_triggers.{trigger}[{position}]"),
                );
            }
        }
    }
    for connection in &model.connections {
        if let Some(PortStrategy::ByCondition { conditions }) = &connection.strategy {
            for (position, condition) in conditions.iter().enumerate() {
                if let Some(source) = &condition.script_condition {
                    push(
                        &mut sources,
                        source,
                        format!(
                            "connections {}->{}.conditions[{position}]",
                            connection.from, connection.to
                        ),
                    );
                }
            }
        }
    }
    let flow_scripts = |flow: &ProcessFlow, prefix: &str, sources: &mut Vec<(String, String)>| {
        for block in &flow.blocks {
            if let FlowBlockKind::Decision(params) = &block.kind {
                for (position, condition) in params.conditions.iter().enumerate() {
                    if let Some(source) = &condition.script_condition {
                        push(
                            sources,
                            source,
                            format!("{prefix}.blocks.{}.conditions[{position}]", block.id),
                        );
                    }
                }
            }
        }
    };
    if let Some(flow) = model.process_flow.as_ref() {
        flow_scripts(flow, "process_flow", &mut sources);
    }
    for object in &model.objects {
        if let Some(flow) = object.params.process_flow() {
            let prefix = format!("objects.{}.process_flow", object.name);
            flow_scripts(flow, &prefix, &mut sources);
        }
    }
    sources
}

/// task-177: one script checked by `openflex check-scripts`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScriptCheck {
    pub location: String,
    pub source: String,
    /// `None` means the source compiled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// task-177: compile every model script without running the simulation.
pub fn check_scripts(model: &Model) -> Vec<ScriptCheck> {
    let engine = ScriptEngine::new();
    collect_model_scripts(model)
        .into_iter()
        .map(|(source, location)| ScriptCheck {
            location,
            source: source.clone(),
            error: engine.compile(&source).err().map(|error| error.to_string()),
        })
        .collect()
}

fn truncated_script(source: &str) -> String {
    let cleaned = source.replace(['\n', '\r'], " ");
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() <= 80 {
        cleaned
    } else {
        format!(
            "{}…{}",
            chars[..39].iter().collect::<String>(),
            chars[chars.len() - 40..].iter().collect::<String>()
        )
    }
}

/// task-177: enrich a script error without changing the `SimError::Script`
/// variant (the old `script error at …` prefix stays first).
fn enrich_script_error(error: ScriptError, source: &str, location: &str, time: f64) -> SimError {
    let undefined = error
        .to_string()
        .to_ascii_lowercase()
        .contains("variable not found");
    let mut context = format!(
        "\n  script: {}\n  at {location} (t={time:.6})",
        truncated_script(source)
    );
    if undefined {
        context.push_str("\n  available: ");
        // Use the script surface, not the expression paths: a rhai script cannot
        // resolve `item.type`, so advertising it would send the user the wrong way.
        context.push_str(
            &crate::script::SCRIPT_VARIABLES
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    let source = match error {
        ScriptError::Compile(message) => ScriptError::Compile(format!("{message}{context}")),
        ScriptError::Runtime(message) => ScriptError::Runtime(format!("{message}{context}")),
        ScriptError::TooManyOperations { limit } => ScriptError::Runtime(format!(
            "script exceeded the operation limit of {limit}{context}"
        )),
    };
    SimError::Script {
        location: location.to_string(),
        detail: source,
    }
}

/// Precompile every script source of the model once (D2).
fn compile_model_scripts(model: &Model) -> Result<HashMap<String, CompiledScript>, SimError> {
    let engine = ScriptEngine::new();
    let mut scripts = HashMap::new();
    for (source, location) in collect_model_scripts(model) {
        if scripts.contains_key(&source) {
            continue;
        }
        let compiled = engine
            .compile(&source)
            .map_err(|error| enrich_script_error(error, &source, &location, 0.0))?;
        scripts.insert(source, compiled);
    }
    Ok(scripts)
}

/// Resolve the model-level and object-level process flows into one graph (B7).
#[allow(clippy::type_complexity)]
fn build_process_flows(
    model: &Model,
    by_name: &HashMap<String, usize>,
) -> (
    Option<FlowRuntime>,
    Vec<FlowBlockMeta>,
    Vec<Vec<usize>>,
    Vec<Vec<usize>>,
    Vec<Vec<usize>>,
) {
    let mut defs: Vec<FlowBlockMeta> = Vec::new();
    let mut connections: Vec<FlowConnRt> = Vec::new();
    let mut outgoing: Vec<Vec<usize>> = Vec::new();
    let mut transfer_targets: Vec<Vec<usize>> = vec![Vec::new(); model.objects.len()];
    let mut entry_blocks: Vec<Vec<usize>> = vec![Vec::new(); model.objects.len()];
    let mut pull_sources: Vec<Vec<usize>> = vec![Vec::new(); model.objects.len()];

    let mut add_flow = |flow: &ProcessFlow, owner: Option<usize>| {
        let base = defs.len();
        let index: HashMap<&str, usize> = flow
            .blocks
            .iter()
            .enumerate()
            .map(|(position, block)| (block.id.as_str(), base + position))
            .collect();
        for (position, block) in flow.blocks.iter().enumerate() {
            let global = base + position;
            defs.push(FlowBlockMeta {
                id: block.id.clone(),
                kind: block.kind.clone(),
                owner,
            });
            outgoing.push(Vec::new());
            match &block.kind {
                FlowBlockKind::Transfer(params) if params.mode == TransferMode::ToObject => {
                    if let Some(&target) = by_name.get(params.object.as_str()) {
                        transfer_targets[target].push(global);
                    }
                }
                FlowBlockKind::Transfer(params) if params.mode == TransferMode::FromObject => {
                    if let Some(&source) = by_name.get(params.object.as_str()) {
                        pull_sources[source].push(global);
                    }
                }
                FlowBlockKind::Create(params) if params.on_entry => {
                    if let Some(owner) = owner {
                        entry_blocks[owner].push(global);
                    }
                }
                _ => {}
            }
        }
        for connection in &flow.connections {
            let (Some(&from), Some(&to)) = (
                index.get(connection.from.as_str()),
                index.get(connection.to.as_str()),
            ) else {
                continue;
            };
            let port = connection.port.unwrap_or(outgoing[from].len() as u32);
            outgoing[from].push(connections.len());
            connections.push(FlowConnRt { to, port });
        }
    };

    if let Some(flow) = model.process_flow.as_ref() {
        add_flow(flow, None);
    }
    for (index, object) in model.objects.iter().enumerate() {
        if let Some(flow) = object.params.process_flow() {
            add_flow(flow, Some(index));
        }
    }

    if defs.is_empty() {
        return (None, defs, transfer_targets, entry_blocks, pull_sources);
    }
    let runtime = FlowRuntime {
        blocks: vec![FlowBlockRt::default(); defs.len()],
        connections,
        outgoing,
        next_token_id: 0,
        tokens_created: 0,
        tokens_completed: 0,
    };
    (
        Some(runtime),
        defs,
        transfer_targets,
        entry_blocks,
        pull_sources,
    )
}

/// Resolve the fluid sub-graph once at start-up (B5).
fn build_fluid_runtime(model: &Model) -> Option<FluidRuntime> {
    let fluid_indices: Vec<usize> = model
        .objects
        .iter()
        .enumerate()
        // Bridges belong to the fluid side: fluid flows in (or out) through
        // them, and they must appear in the fluid report so the volume is not
        // lost between the two worlds.
        .filter(|(_, object)| {
            is_fluid_kind(object.kind)
                || matches!(
                    object.kind,
                    ObjectKind::ItemToFluid | ObjectKind::FluidToItem
                )
        })
        .map(|(index, _)| index)
        .collect();
    if fluid_indices.is_empty() {
        return None;
    }
    let mut position_of: HashMap<usize, usize> = HashMap::new();
    for (position, &index) in fluid_indices.iter().enumerate() {
        position_of.insert(index, position);
    }
    let by_name: HashMap<&str, usize> = model
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.name.as_str(), index))
        .collect();
    let count = fluid_indices.len();
    let mut state = Vec::with_capacity(count);
    for &index in &fluid_indices {
        let initial = match &model.objects[index].params {
            ObjectParams::FluidTank(params) => params.initial_level.max(0.0),
            _ => 0.0,
        };
        let mut runtime = FluidObjectRt::new(initial);
        if let ObjectParams::FluidTank(params) = &model.objects[index].params
            && params.capacity > EPS
            && initial >= params.capacity - EPS
        {
            // Starting full is not a transition.
            runtime.full_latched = true;
        }
        if let ObjectParams::FluidValve(params) = &model.objects[index].params {
            runtime.valve_open = params.open;
        }
        state.push(runtime);
    }
    let mut links = Vec::new();
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); count];
    for connection in &model.connections {
        let (Some(&from_index), Some(&to_index)) = (
            by_name.get(connection.from.as_str()),
            by_name.get(connection.to.as_str()),
        ) else {
            continue;
        };
        let (Some(&from), Some(&to)) = (position_of.get(&from_index), position_of.get(&to_index))
        else {
            continue;
        };
        outgoing[from].push(links.len());
        links.push(FluidLinkRt {
            to,
            flow_rate: connection.flow_rate,
        });
    }
    // Kahn topological order; validation guarantees a DAG.
    let mut indegree = vec![0usize; count];
    for link in &links {
        indegree[link.to] += 1;
    }
    let mut queue: VecDeque<usize> = (0..count).filter(|index| indegree[*index] == 0).collect();
    let mut order = Vec::with_capacity(count);
    while let Some(position) = queue.pop_front() {
        order.push(position);
        for &link in &outgoing[position] {
            let target = links[link].to;
            indegree[target] = indegree[target].saturating_sub(1);
            if indegree[target] == 0 {
                queue.push_back(target);
            }
        }
    }
    if order.len() != count {
        // Should not happen (validation rejects cycles); fall back to model order.
        order = (0..count).collect();
    }
    Some(FluidRuntime {
        objects: fluid_indices,
        state,
        links,
        outgoing,
        order,
        pressure_mode: model.objects.iter().any(|object| match &object.params {
            ObjectParams::FluidTank(params) => params.density.is_some() || params.gravity.is_some(),
            ObjectParams::FluidValve(params) => {
                params.pressure_open.is_some() || params.pressure_close.is_some()
            }
            ObjectParams::FluidPipe(params) => params.temperature.is_some(),
            _ => false,
        }),
        step: model.fluid_step.unwrap_or(0.1).max(EPS),
        last_time: 0.0,
        total_inflow: 0.0,
        total_throughput: 0.0,
        total_overflow: 0.0,
    })
}

/// Stable label of a process-flow block kind (B7).
fn flow_block_kind_name(kind: &FlowBlockKind) -> &'static str {
    match kind {
        FlowBlockKind::Create(_) => "create",
        FlowBlockKind::Delay(_) => "delay",
        FlowBlockKind::Assign(_) => "assign",
        FlowBlockKind::Decision(_) => "decision",
        FlowBlockKind::Sink => "sink",
        FlowBlockKind::Transfer(_) => "transfer",
    }
}

/// Usable capacity of a rack (B2).
///
/// `capacity` stays the hard upper bound; the storage geometry narrows it:
/// a push-back rack has `levels * channels` slots and a drive-in rack has
/// `channels * depth` slots. Without the new fields the plain `capacity` is
/// used, so existing models keep their behaviour.
fn rack_capacity(params: &openflex_model::RackParams) -> u32 {
    // task-44/47: the level/aisle/bay grid bounds the usable capacity
    // (`levels * aisles * bays`; missing dimensions count as 1).
    let grid = if params.aisles.is_some() || params.bays.is_some() {
        Some(
            params
                .levels
                .max(1)
                .saturating_mul(params.aisles.unwrap_or(1).max(1))
                .saturating_mul(params.bays.unwrap_or(1).max(1)),
        )
    } else {
        None
    };
    let geometry = match params.rack_type {
        RackType::PushBack => params
            .channels
            .map(|channels| params.levels.saturating_mul(channels)),
        RackType::DriveIn => params
            .depth
            .map(|depth| depth.saturating_mul(params.channels.unwrap_or(1))),
        RackType::Floor | RackType::Gravity => None,
    };
    geometry
        .or(grid)
        .unwrap_or(params.capacity)
        .min(params.capacity)
        .max(1)
}

/// Queue parameters for both queues and racks.
fn queue_params_of(object: &Object) -> openflex_model::QueueParams {
    match &object.params {
        ObjectParams::Queue(params) => params.clone(),
        ObjectParams::Rack(params) => openflex_model::QueueParams {
            max_content: rack_capacity(params),
            // B2: gravity and push-back racks release the most recently
            // stored item first (LIFO); floor and drive-in racks are FIFO.
            order: if params.rack_type == RackType::Gravity
                || params.rack_type == RackType::PushBack
            {
                QueueOrder::Lifo
            } else {
                QueueOrder::Fifo
            },
            ..Default::default()
        },
        _ => openflex_model::QueueParams::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_nodes_route_over_the_shortest_path() {
        // A -> N1 -> N2 -> N3 -> B is a detour; the node graph must be used
        // instead of the straight line between the endpoints.
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "nodes",
          "run": { "stop_time": 100.0 },
          "objects": [
            { "name": "S", "kind": "source", "pos": [-40, 0], "params": {} },
            { "name": "A", "kind": "queue", "pos": [-30, 0], "params": {} },
            { "name": "B", "kind": "sink", "pos": [40, 0], "params": {} },
            { "name": "N1", "kind": "network_node", "pos": [-30, 0], "params": {} },
            { "name": "N2", "kind": "network_node", "pos": [0, 30], "params": {} },
            { "name": "N3", "kind": "network_node", "pos": [40, 0], "params": {} }
          ],
          "connections": [
            { "from": "A", "to": "B" },
            { "from": "N1", "to": "N2" },
            { "from": "N2", "to": "N3" }
          ]
        }"#,
        )
        .expect("model");
        let sim = Simulation::new(
            &model,
            SimConfig {
                stop_time: 100.0,
                warmup: 0.0,
                seed: 1,
            },
            false,
        )
        .expect("sim");
        let from = sim
            .objects
            .iter()
            .position(|object| object.name == "A")
            .expect("A");
        let to = sim
            .objects
            .iter()
            .position(|object| object.name == "B")
            .expect("B");
        let routed = node_network_distance(&model, &sim.objects[from], &sim.objects[to])
            .expect("the node graph must answer");
        // N1 -> N2 = 42.43, N2 -> N3 = 50, plus A->N1 = 0 and N3->B = 0.
        assert!(
            (routed - 92.43).abs() < 0.1,
            "expected the detour 92.43, got {routed}"
        );
        assert!(
            routed > distance(sim.objects[from].position, sim.objects[to].position),
            "the node path must be longer than the straight line"
        );
    }

    #[test]
    fn a_single_node_falls_back_to_the_straight_line() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "one",
          "run": { "stop_time": 10.0 },
          "objects": [
            { "name": "S", "kind": "source", "pos": [-5, 0], "params": {} },
            { "name": "A", "kind": "queue", "pos": [0, 0], "params": {} },
            { "name": "B", "kind": "sink", "pos": [10, 0], "params": {} },
            { "name": "N1", "kind": "network_node", "pos": [5, 0], "params": {} }
          ],
          "connections": [ { "from": "A", "to": "B" } ]
        }"#,
        )
        .expect("model");
        let sim = Simulation::new(
            &model,
            SimConfig {
                stop_time: 10.0,
                warmup: 0.0,
                seed: 1,
            },
            false,
        )
        .expect("sim");
        let from = sim
            .objects
            .iter()
            .position(|object| object.name == "A")
            .expect("A");
        let to = sim
            .objects
            .iter()
            .position(|object| object.name == "B")
            .expect("B");
        assert!(
            node_network_distance(&model, &sim.objects[from], &sim.objects[to]).is_none(),
            "one node cannot form a network"
        );
    }

    #[test]
    fn node_edges_override_the_euclidean_length() {
        // N1 and N2 are 100 apart, but the modeller declares a 10 m road.
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "short",
          "run": { "stop_time": 10.0 },
          "objects": [
            { "name": "S", "kind": "source", "pos": [-10, 0], "params": {} },
            { "name": "A", "kind": "queue", "pos": [0, 0], "params": {} },
            { "name": "B", "kind": "sink", "pos": [100, 0], "params": {} },
            { "name": "N1", "kind": "network_node", "pos": [0, 0],
              "params": { "edges": [ { "from": "N1", "to": "N2", "distance": 10.0 } ] } },
            { "name": "N2", "kind": "network_node", "pos": [100, 0], "params": {} }
          ],
          "connections": [ { "from": "A", "to": "B" } ]
        }"#,
        )
        .expect("model");
        let sim = Simulation::new(
            &model,
            SimConfig {
                stop_time: 10.0,
                warmup: 0.0,
                seed: 1,
            },
            false,
        )
        .expect("sim");
        let from = sim
            .objects
            .iter()
            .position(|object| object.name == "A")
            .expect("A");
        let to = sim
            .objects
            .iter()
            .position(|object| object.name == "B")
            .expect("B");
        let routed =
            node_network_distance(&model, &sim.objects[from], &sim.objects[to]).expect("routed");
        assert!(
            (routed - 10.0).abs() < 1e-9,
            "the explicit edge length must win, got {routed}"
        );
    }

    fn capacity_model() -> Model {
        Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "capacity",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "P", "kind": "processor", "params": { "max_content": 10 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "P" }, { "from": "P", "to": "K" } ]
        }"#,
        )
        .expect("model")
    }

    fn item() -> FlowItem {
        FlowItem {
            id: 1,
            item_type: 0,
            labels: BTreeMap::new(),
            created: 0.0,
            entry_time: 0.0,
            destination: None,
        }
    }

    /// A5: items in transit occupy capacity slots, so a downstream object is
    /// closed before the transports physically arrive.
    #[test]
    fn in_transit_items_count_against_capacity() {
        let model = capacity_model();
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let processor = sim.by_name["P"];
        assert!(sim.can_accept(processor, 0));

        // Five items inside and four on their way: 9 of 10 slots are taken.
        for _ in 0..5 {
            sim.objects[processor].content.push(item());
        }
        sim.objects[processor].in_transit = 4;
        assert!(sim.can_accept(processor, 0), "9 of 10 slots taken");

        // The fifth transport closes the input although the content did not
        // change: this is the whole point of A5.
        sim.objects[processor].in_transit = 5;
        assert!(!sim.can_accept(processor, 0), "10 of 10 slots taken");

        // Releasing the transit slot opens the input again.
        sim.objects[processor].in_transit = 4;
        assert!(sim.can_accept(processor, 0));
    }

    /// A5: the peak content statistic ignores in-transit items (they are not
    /// inside the object yet).
    #[test]
    fn in_transit_is_not_counted_as_content() {
        let model = capacity_model();
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let processor = sim.by_name["P"];
        sim.objects[processor].content.push(item());
        sim.objects[processor].in_transit = 3;
        sim.objects[processor].touch();
        assert_eq!(sim.objects[processor].held(), 1);
        assert_eq!(sim.objects[processor].effective_content(), 4);
        assert_eq!(sim.objects[processor].acc.max_content, 1);
    }

    fn rack_model() -> Model {
        Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "rack",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "R", "kind": "rack", "params": { "capacity": 10, "levels": 3 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "R" }, { "from": "R", "to": "K" } ]
        }"#,
        )
        .expect("model")
    }

    /// task-44: items stored in a rack carry the level they were put on.
    #[test]
    fn rack_assigns_levels_on_entry() {
        let model = rack_model();
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let rack = sim.by_name["R"];
        for expected in 0..3 {
            assert!(sim.receive_item(rack, 0, item()).expect("receive"));
            let level = sim.objects[rack]
                .content
                .last()
                .and_then(|item| item.labels.get("level"))
                .copied();
            assert_eq!(
                level,
                Some(expected as f64),
                "level label of item {expected}"
            );
        }
    }

    /// task-44: `level_order` releases the lowest level first, `nearest` the
    /// level closest to the previous pick.
    #[test]
    fn rack_pick_policy_orders_by_level() {
        let model = rack_model();
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let rack = sim.by_name["R"];
        for level in [2.0, 0.0, 1.0] {
            let mut item = item();
            item.labels.insert("level".to_string(), level);
            sim.objects[rack].content.push(item);
        }
        assert_eq!(
            sim.pick_rack_position(rack, RackPickPolicy::LevelOrder),
            Some(1),
            "level 0 is the first item"
        );
        assert_eq!(
            sim.pick_rack_position(rack, RackPickPolicy::Fifo),
            None,
            "fifo defers to the queue order"
        );
        sim.objects[rack].last_picked_level = Some(2);
        assert_eq!(
            sim.pick_rack_position(rack, RackPickPolicy::Nearest),
            Some(0),
            "level 2 is closest to the previous pick"
        );
    }

    /// task-47: every stored item gets a concrete level/aisle/bay address.
    #[test]
    fn rack_assigns_level_aisle_bay_addresses() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "rack-bays",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "R", "kind": "rack", "params": {
                "capacity": 24, "levels": 2, "aisles": 2, "bays": 3 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "R" }, { "from": "R", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let rack = sim.by_name["R"];
        let expected = [
            (0.0, 0.0, 0.0),
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (1.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (1.0, 0.0, 1.0),
        ];
        for (slot, (level, aisle, bay)) in expected.iter().enumerate() {
            assert!(sim.receive_item(rack, 0, item()).expect("receive"));
            let stored = sim.objects[rack].content.last().expect("stored item");
            assert_eq!(stored.labels.get("level"), Some(level), "slot {slot} level");
            assert_eq!(stored.labels.get("aisle"), Some(aisle), "slot {slot} aisle");
            assert_eq!(stored.labels.get("bay"), Some(bay), "slot {slot} bay");
        }
    }

    /// task-56: the shortest-path result is cached per (executer, from, to).
    #[test]
    fn distance_cache_reuses_shortest_path_results() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "cache",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "pos": [5, 0], "params": {} },
            { "name": "K", "kind": "sink", "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [0, 0], "params": {} },
            { "name": "E", "kind": "robot", "pos": [0, 0], "params": {
                "capacity": 1, "speed": 1.0, "home": "D" } }
          ],
          "connections": [ { "from": "S", "to": "K" }, { "from": "D", "to": "E" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let robot = sim.by_name["E"];
        let from = sim.by_name["D"];
        let to = sim.by_name["S"];
        assert!(sim.distance_cache.borrow().is_empty());

        let first = sim.transport_object_distance(robot, from, to);
        assert_eq!(sim.distance_cache.borrow().len(), 1);
        let second = sim.transport_object_distance(robot, from, to);
        assert_eq!(first, second, "a cache hit must return the same distance");
        assert_eq!(
            sim.distance_cache.borrow().len(),
            1,
            "a cache hit must not add an entry"
        );

        // Restoring a checkpoint invalidates the cache.
        let checkpoint = sim.snapshot();
        sim.restore(&checkpoint).expect("restore");
        assert!(sim.distance_cache.borrow().is_empty());
    }

    /// task-56: caching does not change the planned distance of a detour.
    #[test]
    fn distance_cache_preserves_grid_detours() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "cache-grid",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "grid": { "width": 6, "height": 6, "cell_size": 1.0,
                    "obstacles": [[2, 0], [2, 1], [2, 2], [2, 3]] },
          "objects": [
            { "name": "S", "kind": "source", "pos": [4, 0], "params": {} },
            { "name": "K", "kind": "sink", "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [0, 0], "params": {} },
            { "name": "E", "kind": "robot", "pos": [0, 0], "params": {
                "capacity": 1, "speed": 1.0, "home": "D" } }
          ],
          "connections": [ { "from": "S", "to": "K" }, { "from": "D", "to": "E" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let sim = Simulation::new(&model, config, false).expect("simulation");
        let robot = sim.by_name["E"];
        let from = sim.by_name["D"];
        let to = sim.by_name["S"];
        let plain = sim.compute_object_distance(robot, from, to);
        let cached_first = sim.transport_object_distance(robot, from, to);
        let cached_second = sim.transport_object_distance(robot, from, to);
        assert_eq!(plain, cached_first);
        assert_eq!(cached_first, cached_second);
        // The detour around the wall is longer than the straight line (4).
        assert!(
            cached_first > 4.0,
            "the wall forces a detour: {cached_first}"
        );
    }

    /// task-89: the Darcy law gives `Q ∝ sqrt(Δp)` and a slightly lower flow
    /// for warmer (less dense) fluid.
    #[test]
    fn darcy_flow_follows_the_quadratic_law() {
        let pipe = openflex_model::FluidPipeParams {
            diameter: Some(0.1),
            friction_factor: Some(0.02),
            density: Some(1000.0),
            length: 10.0,
            ..Default::default()
        };
        let low = darcy_flow(&pipe, 1000.0).expect("darcy");
        let high = darcy_flow(&pipe, 2000.0).expect("darcy");
        assert!(
            (high / low - 2.0f64.sqrt()).abs() < 1e-9,
            "doubling the pressure drop multiplies the flow by sqrt(2): {}",
            high / low
        );
        // Without the three parameters the old calibration path is used.
        let plain = openflex_model::FluidPipeParams {
            flow_rate: Some(5.0),
            ..Default::default()
        };
        assert!(darcy_flow(&plain, 1000.0).is_none());
        // Warmer fluid is less dense and therefore flows a little slower.
        let mut warm = pipe.clone();
        warm.temperature = Some(80.0);
        let warm_flow = darcy_flow(&warm, 1000.0).expect("darcy");
        assert!(warm_flow < low, "warm flow {warm_flow} vs cold {low}");
        assert!((warm_flow / low - (1.0f64 - 0.0002 * 80.0).sqrt()).abs() < 1e-9);
    }

    /// task-92: `pipe_capacity` falls back to the linear value when the pipe
    /// has no Darcy parameters or no pressure difference is available.
    #[test]
    fn pipe_capacity_uses_darcy_only_when_possible() {
        let darcy = openflex_model::FluidPipeParams {
            diameter: Some(0.1),
            friction_factor: Some(0.02),
            density: Some(1000.0),
            length: 10.0,
            ..Default::default()
        };
        let linear = 7.5;
        assert_eq!(pipe_capacity(&darcy, None, linear), linear);
        let flow = pipe_capacity(&darcy, Some(1000.0), linear);
        assert!(flow > 0.0 && flow != linear, "Darcy must differ: {flow}");
        let plain = openflex_model::FluidPipeParams {
            flow_rate: Some(5.0),
            ..Default::default()
        };
        assert_eq!(pipe_capacity(&plain, Some(1000.0), linear), linear);
    }

    /// task-106: a charger with capacity 1 serves one AGV and queues the next;
    /// finishing a charge wakes the waiting AGV.
    /// task-151: an AGV whose target node is held by another AGV retries a few
    /// times but never waits forever (the escalation breaks a would-be ring).
    #[test]
    fn agv_node_wait_escalates_instead_of_blocking_forever() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "agv-wait",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "P", "kind": "processor", "pos": [0.0, 0.0], "params": { "process_operator": "D" } },
            { "name": "D", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {} },
            { "name": "A1", "kind": "agv", "pos": [0.0, 0.0], "params": { "speed": 1.0 } },
            { "name": "A2", "kind": "agv", "pos": [0.0, 0.0], "params": { "speed": 1.0 } },
            { "name": "S", "kind": "source",
              "params": { "max_arrivals": 1, "interarrival": { "distribution": "constant", "value": 1.0 } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let high = sim.by_name["A1"];
        let low = sim.by_name["A2"];
        let requester = sim.by_name["P"];
        // The higher-index AGV occupies the node the lower-index one wants.
        sim.objects[high].tasks = 1;
        sim.objects[low].tasks = 1;
        sim.objects[low].agv_wait_rounds = 0;
        sim.handle_transport_arrive(low, requester)
            .expect("first attempt");
        assert_eq!(sim.objects[low].agv_wait_rounds, 1, "wait counted");
        // Reaching the escalation threshold stops the wait.
        sim.objects[low].agv_wait_rounds = 3;
        sim.handle_transport_arrive(low, requester)
            .expect("escalated attempt");
        assert_eq!(
            sim.objects[low].agv_wait_rounds, 0,
            "after escalation the wait counter resets and the AGV proceeds"
        );
    }

    #[test]
    fn charger_queue_orders_by_policy() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "queue-order",
          "run": { "stop_time": 200.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "A1", "kind": "agv", "params": {
                "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1", "charge_queue_order": "lowest_battery" } },
            { "name": "A2", "kind": "agv", "params": {
                "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1" } },
            { "name": "A3", "kind": "agv", "params": {
                "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1" } },
            { "name": "C1", "kind": "resource", "params": { "capacity": 1 } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 200.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let a1 = sim.by_name["A1"];
        let a2 = sim.by_name["A2"];
        let a3 = sim.by_name["A3"];
        let charger = sim.by_name["C1"];

        // A1 takes the only slot; A2 (battery 2) and A3 (battery 1) queue up.
        sim.objects[a1].battery = Some(3.0);
        sim.start_charging(a1).expect("first charge");
        sim.objects[a2].battery = Some(2.0);
        sim.start_charging(a2).expect("queued");
        sim.objects[a3].battery = Some(1.0);
        sim.start_charging(a3).expect("queued");
        assert_eq!(sim.charger_queue[&charger].len(), 2);

        // The lowest-battery policy serves A3 before A2.
        let next = sim
            .pop_charger_queue(charger, openflex_model::ChargeQueueOrder::LowestBattery)
            .expect("queued AGV");
        assert_eq!(next, a3, "lowest battery should be served first");
        let rest = sim
            .pop_charger_queue(charger, openflex_model::ChargeQueueOrder::LowestBattery)
            .expect("remaining AGV");
        assert_eq!(rest, a2);

        // FIFO keeps the arrival order instead.
        sim.charger_queue.get_mut(&charger).unwrap().clear();
        sim.charger_queue.get_mut(&charger).unwrap().push_back(a2);
        sim.charger_queue.get_mut(&charger).unwrap().push_back(a3);
        let fifo = sim
            .pop_charger_queue(charger, openflex_model::ChargeQueueOrder::Fifo)
            .expect("queued AGV");
        assert_eq!(fifo, a2, "FIFO serves the first arrival");
    }

    #[test]
    fn agv_prefers_a_free_charger_over_a_busy_one() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "multicharger",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "A1", "kind": "agv", "pos": [0.0, 0.0], "params": {
                "speed": 1.0, "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1", "chargers": ["C2"] } },
            { "name": "C1", "kind": "resource", "pos": [2.0, 0.0], "params": { "capacity": 1 } },
            { "name": "C2", "kind": "resource", "pos": [40.0, 0.0], "params": { "capacity": 1 } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let agv = sim.by_name["A1"];
        let c1 = sim.by_name["C1"];
        let c2 = sim.by_name["C2"];
        // Both free: the nearest station (C1) wins.
        assert_eq!(sim.charger_slot(agv), Some(c1));
        // C1 busy: the AGV drives to the free C2 instead of queueing.
        sim.charger_in_use.insert(c1, 1);
        assert_eq!(sim.charger_slot(agv), Some(c2));
        // C2 busy as well: fall back to the nearest (C1) and queue there.
        sim.charger_in_use.insert(c2, 1);
        assert_eq!(sim.charger_slot(agv), Some(c1));
    }

    #[test]
    fn agv_charger_slots_queue() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "charger",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "A1", "kind": "agv", "params": {
                "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1" } },
            { "name": "A2", "kind": "agv", "params": {
                "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1" } },
            { "name": "C1", "kind": "resource", "params": { "capacity": 1 } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let first = sim.by_name["A1"];
        let second = sim.by_name["A2"];
        let charger = sim.by_name["C1"];
        sim.objects[first].battery = Some(0.0);
        sim.objects[second].battery = Some(0.0);

        sim.start_charging(first).expect("first charge");
        assert_eq!(sim.charger_in_use.get(&charger).copied(), Some(1));
        assert!(sim.objects[first].charging);
        assert!(sim.objects[first].charge_until.is_finite());

        sim.start_charging(second).expect("queued");
        assert_eq!(
            sim.charger_in_use.get(&charger).copied(),
            Some(1),
            "one slot"
        );
        assert_eq!(
            sim.charger_queue.get(&charger).map(VecDeque::len),
            Some(1),
            "the second AGV waits"
        );
        assert!(sim.objects[second].charging, "a waiting AGV takes no tasks");
        assert!(
            sim.objects[second].charge_until.is_infinite(),
            "still waiting"
        );

        // Finishing the first charge hands the slot to the queued AGV.
        sim.handle_charge_done(first).expect("done");
        assert_eq!(sim.charger_in_use.get(&charger).copied(), Some(1));
        assert_eq!(sim.charger_queue.get(&charger).map(VecDeque::len), Some(0));
        assert!(sim.objects[second].charge_until.is_finite(), "now charging");
    }

    /// task-109: a person whose target cell is taken by a lower-index person
    /// waits a tick instead of stacking on the same cell.
    #[test]
    fn person_waits_for_an_occupied_cell() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "person-wait",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "P", "kind": "processor", "pos": [5, 0], "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [0, 0], "params": {} },
            { "name": "E0", "kind": "person", "pos": [0, 0], "params": {} },
            { "name": "E1", "kind": "person", "pos": [0, 0], "params": {} },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let processor = sim.by_name["P"];
        let first = sim.by_name["E0"];
        let second = sim.by_name["E1"];
        assert!(first < second, "the model order defines the priority");

        // The first person already stands on the cell.
        sim.objects[first].position = [5.0, 0.0];
        sim.objects[first].tasks = 1;
        sim.objects[second].position = [0.0, 0.0];
        sim.objects[second].tasks = 1;

        sim.handle_transport_arrive(second, processor)
            .expect("arrive");
        assert_eq!(
            sim.objects[second].position,
            [0.0, 0.0],
            "the waiting person must not enter the occupied cell"
        );

        // Once the cell is free the same person arrives normally.
        sim.objects[first].tasks = 0;
        sim.handle_transport_arrive(second, processor)
            .expect("arrive");
        assert_eq!(sim.objects[second].position, [5.0, 0.0]);
    }

    /// task-112: a following person waits when the pickup cell is occupied by
    /// a lower-index person, and proceeds once it is free.
    #[test]
    fn person_waits_before_starting_the_pickup_leg() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "person-follow",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "D", "kind": "dispatcher", "pos": [0, 0], "params": {} },
            { "name": "P", "kind": "processor", "pos": [5, 0], "params": {} },
            { "name": "E0", "kind": "person", "pos": [0, 0], "params": { "home": "D" } },
            { "name": "E1", "kind": "person", "pos": [0, 0], "params": { "home": "D" } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let processor = sim.by_name["P"];
        let first = sim.by_name["E0"];
        let second = sim.by_name["E1"];
        assert!(first < second);

        // The first person occupies the shared pickup cell.
        sim.objects[first].position = [0.0, 0.0];
        sim.objects[first].tasks = 1;
        sim.objects[second].position = [5.0, 0.0];
        sim.objects[second].tasks = 1;
        let before = sim.events.len();

        sim.handle_transport_pickup(second, processor)
            .expect("pickup");
        assert!(
            sim.events.len() > before,
            "a retry must be scheduled while the cell is taken"
        );
        assert_eq!(
            sim.objects[second].position,
            [5.0, 0.0],
            "the waiting person must not jump onto the occupied cell"
        );

        // Once the cell is free the pickup proceeds and the person moves to
        // its home (the shared cell).
        sim.objects[first].tasks = 0;
        sim.handle_transport_pickup(second, processor)
            .expect("pickup");
        assert_eq!(sim.objects[second].position, [0.0, 0.0]);
    }

    /// task-118: the path length and the distance share one cost model, the
    /// path ends on the goal and blocked goals are unreachable.
    #[test]
    fn grid_path_matches_the_distance_model() {
        let open = openflex_model::Grid {
            width: 10,
            height: 10,
            cell_size: 1.0,
            obstacles: Vec::new(),
        };
        let path = grid_path(&open, [0.5, 0.5], [4.5, 0.5]).expect("path");
        assert_eq!(path.first(), Some(&[0, 0]));
        assert_eq!(path.last(), Some(&[4, 0]));
        assert_eq!(path.len(), 5, "four straight cells plus the start");
        let distance = grid_distance(&open, [0.5, 0.5], [4.5, 0.5]).expect("distance");
        assert!(
            ((path.len() - 1) as f64 * open.cell_size - distance).abs() < 1e-9,
            "path length {path:?} vs distance {distance}"
        );

        // A wall forces a detour: the path stays connected, the ends match.
        let walled = openflex_model::Grid {
            width: 6,
            height: 6,
            cell_size: 1.0,
            obstacles: vec![[2, 0], [2, 1], [2, 2], [2, 3]],
        };
        let detour = grid_path(&walled, [0.5, 0.5], [4.5, 0.5]).expect("detour");
        assert_eq!(detour.first(), Some(&[0, 0]));
        assert_eq!(detour.last(), Some(&[4, 0]));
        let detour_distance = grid_distance(&walled, [0.5, 0.5], [4.5, 0.5]).expect("distance");
        // Diagonal steps cost sqrt(2), so compare the weighted path length.
        let weighted: f64 = detour
            .windows(2)
            .map(|window| {
                let dx = (window[1][0] as i64 - window[0][0] as i64).abs();
                let dy = (window[1][1] as i64 - window[0][1] as i64).abs();
                if dx != 0 && dy != 0 { 1.414 } else { 1.0 }
            })
            .sum();
        // The detour must not be longer than the distance model reports; the
        // two searches may pick different (equally valid) shortest paths.
        assert!(
            weighted <= detour_distance + 1e-3,
            "detour cost {weighted} vs distance {detour_distance}"
        );
        assert!(detour.len() > 5, "the wall forces extra cells");
        for window in detour.windows(2) {
            let dx = (window[1][0] as i64 - window[0][0] as i64).abs();
            let dy = (window[1][1] as i64 - window[0][1] as i64).abs();
            assert!(dx <= 1 && dy <= 1 && dx + dy > 0, "8-neighbour steps only");
        }

        // A blocked goal has no path at all.
        let blocked_goal = openflex_model::Grid {
            width: 6,
            height: 6,
            cell_size: 1.0,
            obstacles: vec![[4, 0]],
        };
        assert!(grid_path(&blocked_goal, [0.5, 0.5], [4.5, 0.5]).is_none());
    }

    fn grid_person_model(grid_step: bool) -> Model {
        let flag = if grid_step {
            r#", "grid_step": true"#
        } else {
            ""
        };
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "person-grid",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "grid": {{ "width": 10, "height": 10, "cell_size": 1.0 }},
          "objects": [
            {{ "name": "D", "kind": "dispatcher", "pos": [0.5, 0.5], "params": {{}} }},
            {{ "name": "P", "kind": "processor", "pos": [4.5, 0.5], "params": {{}} }},
            {{ "name": "E", "kind": "person", "pos": [0.5, 0.5], "params": {{
                "speed": 1.0, "home": "D"{flag} }} }},
            {{ "name": "S", "kind": "source", "params": {{}} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [ {{ "from": "S", "to": "K" }} ]
        }}"#
        ))
        .expect("model")
    }

    /// task-152: `grid_reserve` cells ahead are treated as occupied by other
    /// people, so a follower yields before it reaches the reserved cell.
    #[test]
    fn person_reservations_block_followers_early() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "person-reserve",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "grid": { "width": 10, "height": 10, "cell_size": 1.0 },
          "objects": [
            { "name": "D", "kind": "dispatcher", "pos": [0.5, 0.5], "params": {} },
            { "name": "P", "kind": "processor", "pos": [4.5, 0.5], "params": {} },
            { "name": "E0", "kind": "person", "pos": [0.5, 0.5],
              "params": { "speed": 1.0, "home": "D", "grid_step": true, "grid_reserve": 2 } },
            { "name": "E1", "kind": "person", "pos": [0.5, 0.5],
              "params": { "speed": 1.0, "home": "D", "grid_step": true, "grid_reserve": 2 } },
            { "name": "S", "kind": "source",
              "params": { "max_arrivals": 1, "interarrival": { "distribution": "constant", "value": 1.0 } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let leader = sim.by_name["E0"];
        let follower = sim.by_name["E1"];
        let requester = sim.by_name["P"];
        sim.objects[follower].tasks = 1;
        sim.objects[follower].person_target = Some(requester);

        // The leader reserves two cells ahead of the follower.
        let reserved = [[1.5, 0.5], [2.5, 0.5]];
        sim.objects[leader].tasks = 1;
        sim.objects[leader].person_reserved = reserved.to_vec();
        // The follower's next cell is the leader's *second* reservation.
        sim.objects[follower].person_path.clear();
        sim.objects[follower].person_path.push_back([2.5, 0.5]);
        sim.objects[follower].position = [1.5, 0.5];

        sim.handle_person_step(follower).expect("step");
        assert_eq!(
            sim.objects[follower].position,
            [1.5, 0.5],
            "a reserved cell must not be entered"
        );
        // Without the reservation the follower walks on (clear the target so
        // the finished path does not snap onto the requester).
        sim.objects[leader].person_reserved.clear();
        sim.objects[follower].person_target = None;
        sim.handle_person_step(follower).expect("step");
        assert_eq!(
            sim.objects[follower].position,
            [2.5, 0.5],
            "after the reservation is released the follower proceeds"
        );
    }

    /// task-150: the walk home is also split into cells when `grid_step` is on.
    #[test]
    fn person_grid_step_walks_home_cell_by_cell() {
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        // A dedicated model: the shared helper's source has a zero
        // inter-arrival, which is fine for single-event tests but not for a
        // loop that steps through the calendar.
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "person-walk-home",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "grid": { "width": 10, "height": 10, "cell_size": 1.0 },
          "objects": [
            { "name": "D", "kind": "dispatcher", "pos": [0.5, 0.5], "params": {} },
            { "name": "P", "kind": "processor", "pos": [4.5, 0.5], "params": {} },
            { "name": "E", "kind": "person", "pos": [0.5, 0.5],
              "params": { "speed": 1.0, "home": "D", "grid_step": true } },
            { "name": "S", "kind": "source", "params": { "max_arrivals": 1, "interarrival": { "distribution": "constant", "value": 1.0 } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let person = sim.by_name["E"];
        let requester = sim.by_name["P"];
        let home = sim.by_name["D"];
        // Pretend the person just finished a task at the requester.
        sim.objects[person].position = sim.objects[requester].position;
        sim.objects[person].tasks = 1;
        sim.objects[person].serving_requester = Some(requester);

        sim.handle_operator_released(person, requester)
            .expect("release");
        assert!(
            sim.objects[person].person_returning,
            "the return leg should be a step-by-step walk"
        );
        assert!(
            !sim.objects[person].person_path.is_empty(),
            "the home path should be queued"
        );

        // Walk every cell: after the last one the task settles at the home.
        let mut guard = 0;
        while sim.objects[person].person_returning && guard < 200 {
            sim.step_one_event().expect("step");
            guard += 1;
        }
        assert!(!sim.objects[person].person_returning, "return finished");
        assert!(
            distance(sim.objects[person].position, sim.objects[home].position) < 0.5,
            "the person should be back home"
        );
        assert_eq!(sim.objects[person].tasks, 0, "the task settled");
    }

    /// task-124: with `grid_step` the pickup leg is split into cells whose
    /// total time equals `grid_distance / speed`.
    #[test]
    fn person_grid_step_splits_the_pickup_leg() {
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let model = grid_person_model(true);
        let mut sim = Simulation::new(&model, config.clone(), false).expect("simulation");
        let person = sim.by_name["E"];
        let requester = sim.by_name["P"];
        sim.handle_transport_pickup(person, requester)
            .expect("pickup");
        // 4 cells from [0.5,0.5] to [4.5,0.5] (the start cell is skipped).
        assert_eq!(sim.objects[person].person_path.len(), 4);
        assert_eq!(sim.objects[person].person_target, Some(requester));
        let grid = model.grid.clone().expect("grid");
        let distance = grid_distance(&grid, [0.5, 0.5], [4.5, 0.5]).expect("distance");
        let total = sim.person_step_duration(person) * 4.0;
        assert!(
            (total - distance).abs() < 1e-9,
            "step total {total} vs distance {distance}"
        );
    }

    /// Without `grid_step` the historical one-leg pickup is kept.
    #[test]
    fn person_without_grid_step_keeps_the_single_leg() {
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let model = grid_person_model(false);
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let person = sim.by_name["E"];
        let requester = sim.by_name["P"];
        sim.handle_transport_pickup(person, requester)
            .expect("pickup");
        assert!(sim.objects[person].person_path.is_empty(), "no cell path");
        assert!(sim.objects[person].person_target.is_none());
        // The old path jumps to the home position immediately.
        assert_eq!(sim.objects[person].position, [0.5, 0.5]);
    }

    /// task-128: a following person waits when the next cell is taken by a
    /// lower-index person and moves on once the cell is free.
    #[test]
    fn person_grid_step_waits_for_an_occupied_cell() {
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "person-follow-grid",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "grid": { "width": 10, "height": 10, "cell_size": 1.0 },
          "objects": [
            { "name": "D", "kind": "dispatcher", "pos": [0.5, 0.5], "params": {} },
            { "name": "P", "kind": "processor", "pos": [4.5, 0.5], "params": {} },
            { "name": "E0", "kind": "person", "pos": [0.5, 0.5], "params": {
                "speed": 1.0, "home": "D", "grid_step": true } },
            { "name": "E1", "kind": "person", "pos": [0.5, 0.5], "params": {
                "speed": 1.0, "home": "D", "grid_step": true } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let first = sim.by_name["E0"];
        let second = sim.by_name["E1"];
        let requester = sim.by_name["P"];
        assert!(first < second);

        // Both people plan the same path; the first one occupies the next cell.
        sim.handle_transport_pickup(second, requester)
            .expect("pickup");
        assert_eq!(sim.objects[second].person_path.len(), 4);
        let next_cell = sim.objects[second].person_path[0];
        sim.objects[first].position = next_cell;
        sim.objects[first].tasks = 1;
        let before = sim.events.len();

        sim.handle_person_step(second).expect("step");
        assert!(
            sim.events.len() > before,
            "a retry must be scheduled while the cell is taken"
        );
        assert_eq!(
            sim.objects[second].person_path.len(),
            4,
            "the blocked cell stays in the path"
        );
        assert_ne!(sim.objects[second].position, next_cell);

        // Once the cell is free the person enters it.
        sim.objects[first].tasks = 0;
        sim.handle_person_step(second).expect("step");
        assert_eq!(sim.objects[second].position, next_cell);
        assert_eq!(sim.objects[second].person_path.len(), 3);
    }

    /// task-130: the report exposes the remaining battery and the share of
    /// time spent charging.
    #[test]
    fn agv_stats_report_battery_and_charging() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "agv-stats",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "A", "kind": "agv", "params": { "battery_capacity": 10.0 } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let agv = sim.by_name["A"];
        sim.objects[agv].battery = Some(4.0);
        sim.objects[agv].charge_time_total = 25.0;
        let stats = sim.objects[agv].stats(100.0);
        assert_eq!(stats.battery_remaining, Some(4.0));
        assert!(
            (stats.charging_fraction - 0.25).abs() < 1e-9,
            "charging fraction: {}",
            stats.charging_fraction
        );
        // A non-AGV has no battery.
        let source = sim.by_name["S"];
        assert_eq!(sim.objects[source].stats(100.0).battery_remaining, None);
    }

    /// task-133: a low-battery AGV drives to its charger before charging; the
    /// trip takes distance / speed.
    #[test]
    fn agv_drives_to_the_charger_before_charging() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "agv-travel",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "A", "kind": "agv", "pos": [0, 0], "params": {
                "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "speed": 2.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1" } },
            { "name": "C1", "kind": "resource", "pos": [10, 0], "params": { "capacity": 1 } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let agv = sim.by_name["A"];
        sim.objects[agv].battery = Some(0.0);

        sim.drain_agv_battery(agv).expect("drain");
        assert!(sim.objects[agv].charging, "no new tasks while driving");
        assert!(
            sim.objects[agv].charge_until.is_infinite(),
            "not charging yet"
        );

        // 10 distance / 2 speed = 5 time units before charging starts.
        let mut travel = None;
        while let Some(Reverse(event)) = sim.events.pop() {
            if matches!(event.kind, EventKind::ChargeTravel(_)) {
                travel = Some(event);
                break;
            }
        }
        let travel = travel.expect("a ChargeTravel event must be scheduled");
        assert_eq!(travel.time, 5.0);
        sim.now = travel.time;
        sim.handle(travel.kind).expect("arrive");
        assert!(sim.objects[agv].charging);
        assert!((sim.objects[agv].charge_started_at - 5.0).abs() < 1e-9);
        assert!(sim.objects[agv].charge_until.is_finite());
    }

    /// task-136: the drive to the charger uses the network when the AGV has
    /// one, and the trip costs battery on arrival.
    #[test]
    fn agv_charge_trip_uses_the_network_and_drains_battery() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "agv-network",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "A", "kind": "agv", "pos": [0, 0], "params": {
                "battery_capacity": 10.0, "consumption_per_distance": 1.0,
                "speed": 2.0,
                "charge_time": { "distribution": "constant", "value": 10.0 },
                "charger": "C1", "home": "D",
                "network": [ { "from": "D", "to": "C1", "distance": 30.0, "directed": true } ] } },
            { "name": "D", "kind": "dispatcher", "pos": [0, 0], "params": {} },
            { "name": "C1", "kind": "resource", "pos": [10, 0], "params": { "capacity": 1 } },
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let agv = sim.by_name["A"];
        sim.objects[agv].battery = Some(0.0);

        sim.drain_agv_battery(agv).expect("drain");
        let mut travel = None;
        while let Some(Reverse(event)) = sim.events.pop() {
            if matches!(event.kind, EventKind::ChargeTravel(_)) {
                travel = Some(event);
                break;
            }
        }
        let travel = travel.expect("ChargeTravel scheduled");
        // Network distance 30 / speed 2 = 15 (the straight line would be 5).
        assert_eq!(travel.time, 15.0);
        sim.now = travel.time;
        sim.handle(travel.kind).expect("arrive");
        // The trip consumed 30 units of battery; charging refills it.
        assert_eq!(sim.objects[agv].battery, Some(-30.0));
    }

    // task-142/143: KNOWN FAILURE - the AGV still finishes no task in a real
    // model. Diagnosis so far: (1) `agv_mid_task_charge` used to loop forever
    // (charge -> resume -> charge) when a full battery could not cover a leg;
    // fixed in task-143 by draining it and letting the task continue.
    // (2) after that fix the chain still stops before `finish_task` (AGV keeps
    // `tasks == 1`, `serving_requester == Some(P)`). Reproduce with
    // `cargo test -p openflex-core -- --ignored --nocapture` and the
    // `agv_task_chain_diagnosis` helper.
    #[test]
    fn agv_battery_drains_on_a_real_task() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "agv-real",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "pos": [0, 0], "params": {
                "interarrival": { "distribution": "constant", "value": 10.0 },
                "max_arrivals": 8, "first_arrival": 0.0 } },
            { "name": "P", "kind": "processor", "pos": [0, 0], "params": {
                "process_time": { "distribution": "constant", "value": 3.0 },
                "process_operator": "D" } },
            { "name": "K", "kind": "sink", "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [10, 0], "params": {} },
            { "name": "C", "kind": "resource", "pos": [20, 0], "params": { "capacity": 1 } },
            { "name": "A", "kind": "agv", "pos": [10, 0], "params": {
                "capacity": 1, "speed": 5.0, "home": "D",
                "battery_capacity": 5.0, "consumption_per_distance": 1.0,
                "charge_time": { "distribution": "constant", "value": 20.0 },
                "charger": "C" } }
          ],
          "connections": [
            { "from": "S", "to": "P" }, { "from": "P", "to": "K" },
            { "from": "D", "to": "A" }
          ]
        }"#,
        )
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        let agv = &report.replications[0].objects["A"];
        println!(
            "battery={:?} charging_fraction={} output={}",
            agv.battery_remaining, agv.charging_fraction, agv.output_count
        );
        assert!(
            agv.output_count > 0,
            "the AGV must finish at least one task"
        );
        assert!(
            agv.charging_fraction > 0.0,
            "the drained AGV must recharge at its charger (got {})",
            agv.charging_fraction
        );
        let battery = agv.battery_remaining.expect("AGV reports a battery");
        assert!(
            battery <= 5.0 + 1e-9,
            "the battery never exceeds its capacity (got {battery})"
        );
    }

    /// task-145: `stats_at` exposes the live content and state for the GUI.
    #[test]
    fn object_stats_expose_live_content_and_state() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "live-stats",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "Q", "kind": "queue", "params": { "max_content": 10 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [ { "from": "S", "to": "Q" }, { "from": "Q", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let queue = sim.by_name["Q"];
        for _ in 0..3 {
            let item = item();
            sim.objects[queue].push_content(sim.now, item);
        }
        let stats = sim.objects[queue].stats_at(sim.now, 10.0);
        assert_eq!(stats.content, 3, "live content");
        assert_eq!(stats.state, Some(State::Idle), "state before any flow");
        assert_eq!(stats.phase, Some(Phase::Idle));

        sim.set_state(queue, State::Busy);
        let stats = sim.objects[queue].stats_at(sim.now, 10.0);
        assert_eq!(stats.state, Some(State::Busy), "state follows the engine");
        assert_eq!(stats.content, 3, "content is unaffected by the state");
    }

    /// task-146: `stats_at` reports the live position and height.
    #[test]
    fn object_stats_expose_the_live_position() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "live-position",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} },
            { "name": "T", "kind": "transporter", "pos": [0, 0], "params": { "speed": 1.0 } }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let transporter = sim.by_name["T"];
        assert_eq!(
            sim.objects[transporter].stats_at(sim.now, 10.0).position,
            Some([0.0, 0.0])
        );

        // Halfway between two stations.
        sim.objects[transporter].position = [5.0, 2.5];
        sim.objects[transporter].z = 1.5;
        let stats = sim.objects[transporter].stats_at(sim.now, 10.0);
        assert_eq!(stats.position, Some([5.0, 2.5]));
        assert_eq!(stats.z, 1.5);

        // Arrived at the destination.
        sim.objects[transporter].position = [10.0, 2.5];
        sim.objects[transporter].z = 0.0;
        let stats = sim.objects[transporter].stats_at(sim.now, 10.0);
        assert_eq!(stats.position, Some([10.0, 2.5]));
        assert_eq!(stats.z, 0.0);
    }
    /// task-148 fixture: two transporters share one `traffic_control` area with
    /// a single slot between the home node N1 and the station node N2.
    fn traffic_model(mode: &str, capacity: u32, speed_factor: f64) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "traffic",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S1", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "S2", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "P1", "kind": "processor", "pos": [10.0, 0.0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "P2", "kind": "processor", "pos": [10.0, 0.0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "K1", "kind": "sink", "params": {{}} }},
            {{ "name": "K2", "kind": "sink", "params": {{}} }},
            {{ "name": "D", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {{}} }},
            {{ "name": "T1", "kind": "transporter", "pos": [0.0, 0.0],
               "params": {{ "speed": 1.0, "home": "D" }} }},
            {{ "name": "T2", "kind": "transporter", "pos": [0.0, 0.0],
               "params": {{ "speed": 1.0, "home": "D" }} }},
            {{ "name": "N1", "kind": "network_node", "pos": [0.0, 0.0],
               "params": {{ "edges": [
                   {{ "from": "N1", "to": "N2", "distance": 10.0 }} ] }} }},
            {{ "name": "N2", "kind": "network_node", "pos": [10.0, 0.0], "params": {{}} }},
            {{ "name": "TC", "kind": "traffic_control", "pos": [5.0, 5.0], "params": {{
                "capacity": {capacity}, "mode": "{mode}", "speed_factor": {speed_factor} }} }}
          ],
          "connections": [
            {{ "from": "S1", "to": "P1" }}, {{ "from": "P1", "to": "K1" }},
            {{ "from": "S2", "to": "P2" }}, {{ "from": "P2", "to": "K2" }},
            {{ "from": "D", "to": "T1" }}, {{ "from": "D", "to": "T2" }},
            {{ "from": "TC", "to": "N1" }}, {{ "from": "TC", "to": "N2" }}
          ]
        }}"#
        ))
        .expect("model")
    }

    fn loaded_times(trace: &TraceLog) -> Vec<f64> {
        trace
            .object("T1")
            .chain(trace.object("T2"))
            .filter(|event| event.event == "operator_loaded")
            .map(|event| event.time)
            .collect()
    }

    /// task-148: with capacity 1 the second transporter waits at N1 until the
    /// first one arrives at N2, then both tasks complete.
    #[test]
    fn traffic_control_mutual_exclusion_waits_and_releases() {
        let model = traffic_model("mutual_exclusion", 1, 1.0);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let mut times = loaded_times(&trace);
        times.sort_by(|left, right| left.partial_cmp(right).expect("time"));
        assert_eq!(times.len(), 2, "both transporters must deliver");
        assert!((times[0] - 10.0).abs() < 1e-9, "first enters immediately");
        assert!(
            times[1] >= 20.0 - 1e-9,
            "second waits for the first arrival, got {times:?}"
        );
        assert_eq!(result.items_completed, 2, "both items must finish");
        let wait = trace
            .events
            .iter()
            .filter(|event| event.event == "traffic_wait")
            .count();
        println!("traffic_wait={wait} times={times:?}");
        assert!(wait > 0, "the blocked leg must wake at least once");
        assert!(
            wait < 100,
            "directed wake-up must replace polling, got {wait} traffic_wait events"
        );
        assert!(
            (times[1] - 20.0).abs() < 1e-9,
            "release still delivers at t=20: {times:?}"
        );
    }

    /// task-148: with capacity 2 both transporters are admitted at once.
    #[test]
    fn traffic_control_capacity_two_admits_both() {
        let model = traffic_model("mutual_exclusion", 2, 1.0);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let times = loaded_times(&trace);
        assert_eq!(times.len(), 2);
        assert!(
            times.iter().all(|time| (time - 10.0).abs() < 1e-9),
            "capacity 2 must admit both immediately: {times:?}"
        );
        assert_eq!(result.items_completed, 2);
        assert!(
            trace
                .events
                .iter()
                .all(|event| event.event != "traffic_wait"),
            "capacity 2 leaves no waiter"
        );
    }

    /// task-148: `un_timed` skips the slot limit and only applies the speed
    /// factor, so both vehicles depart together but arrive later.
    #[test]
    fn traffic_control_un_timed_applies_speed_only() {
        let model = traffic_model("un_timed", 1, 0.5);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let times = loaded_times(&trace);
        assert_eq!(times.len(), 2, "both transporters deliver");
        assert!(
            times.iter().all(|time| (time - 20.0).abs() < 1e-9),
            "speed factor 0.5 must double the leg: {times:?}"
        );
        assert_eq!(result.items_completed, 2);
        assert!(
            trace
                .events
                .iter()
                .all(|event| event.event != "traffic_wait"),
            "un_timed must not wait"
        );
    }

    /// task-148: admissions are part of the checkpoint; a resumed run rebuilds
    /// the occupancy counters from the restored holds and matches the direct run.
    #[test]
    fn traffic_control_admissions_survive_checkpoint_restore() {
        let model = traffic_model("mutual_exclusion", 1, 1.0);
        let first = SimConfig {
            stop_time: 5.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, checkpoint) = crate::simulate_with_checkpoint(&model, &first, 0, None)
            .expect("save the waiting state");
        let mut sim = Simulation::new(&model, first.clone(), false).expect("sim");
        sim.restore(&checkpoint).expect("restore");
        assert_eq!(
            sim.traffic_occupancy.iter().sum::<u32>(),
            sim.objects
                .iter()
                .map(|object| object.traffic_hold.len() as u32)
                .sum::<u32>(),
            "occupancy must match the restored holds"
        );
        let second = SimConfig {
            stop_time: 100.0,
            ..first
        };
        let (resumed, _) =
            crate::simulate_with_checkpoint(&model, &second, 0, Some(&checkpoint)).expect("resume");
        let (direct, _) =
            crate::simulate_with_checkpoint(&model, &second, 0, None).expect("direct");
        assert_eq!(resumed, direct, "checkpointed admissions must not leak");
    }

    fn log_messages(trace: &TraceLog) -> Vec<(f64, String)> {
        trace
            .events
            .iter()
            .filter(|event| event.event == "script_log")
            .map(|event| (event.time, event.message.clone()))
            .collect()
    }

    /// task-152: SetupFinish fires when the setup clock ends (before processing
    /// starts); ProcessFinish fires when the processing clock ends, before the
    /// item leaves the processor.
    #[test]
    fn setup_and_process_finish_hooks_fire_at_the_phase_boundaries() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "phase-hooks",
          "run": { "stop_time": 20.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0 } },
            { "name": "P", "kind": "processor", "params": {
                "setup_time": { "distribution": "constant", "value": 2.0 },
                "process_time": { "distribution": "constant", "value": 3.0 },
                "triggers": {
                  "on_setup_finish": [{ "action": "script", "source": "log(\"setup_finish\");" }],
                  "on_process_finish": [{ "action": "script", "source": "log(\"process_finish\");" }]
                } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let logs = log_messages(&trace);
        let setup: Vec<f64> = logs
            .iter()
            .filter(|(_, message)| message == "setup_finish")
            .map(|(time, _)| *time)
            .collect();
        let process: Vec<f64> = logs
            .iter()
            .filter(|(_, message)| message == "process_finish")
            .map(|(time, _)| *time)
            .collect();
        assert_eq!(setup.len(), 1, "setup finish fires once");
        assert_eq!(process.len(), 1, "process finish fires once");
        assert!((setup[0] - 2.0).abs() < 1e-9, "setup ends at t=2");
        assert!((process[0] - 5.0).abs() < 1e-9, "process ends at t=5");
    }

    /// task-152: model triggers run once at start (t=0), reset (warm-up) and
    /// stop time, in that order.
    #[test]
    fn model_triggers_fire_at_start_reset_and_stop() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "model-triggers",
          "run": { "stop_time": 10.0, "warmup": 3.0, "seed": 1, "replications": 1 },
          "model_triggers": {
            "on_start": [{ "action": "script", "source": "log(\"start\");" }],
            "on_reset": [{ "action": "script", "source": "log(\"reset\");" }],
            "on_stop": [{ "action": "script", "source": "log(\"stop\");" }]
          },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 10.0,
            warmup: 3.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let logs = log_messages(&trace);
        let counts = |name: &str| logs.iter().filter(|(_, message)| message == name).count();
        assert_eq!(counts("start"), 1, "on_start once");
        assert_eq!(counts("reset"), 1, "on_reset once");
        assert_eq!(counts("stop"), 1, "on_stop once");
        let start = logs
            .iter()
            .find(|(_, message)| message == "start")
            .expect("start");
        let reset = logs
            .iter()
            .find(|(_, message)| message == "reset")
            .expect("reset");
        let stop = logs
            .iter()
            .find(|(_, message)| message == "stop")
            .expect("stop");
        assert!(start.0 < 1e-9, "on_start before the first event");
        assert!((reset.0 - 3.0).abs() < 1e-9, "on_reset at the warm-up");
        assert!((stop.0 - 10.0).abs() < 1e-9, "on_stop at stop_time");
    }

    /// task-152: a failing model trigger is an error, not a silent no-op.
    #[test]
    fn model_trigger_errors_are_reported() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "model-trigger-error",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "model_triggers": {
            "on_start": [{ "action": "script", "source": "nope();" }]
          },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
        )
        .expect("model");
        let error = crate::run_model(&model).expect_err("on_start error");
        let text = error.to_string();
        assert!(text.contains("script error at"), "{text}");
        assert!(text.contains("model_triggers.on_start"), "{text}");
    }

    /// task-154: engine expressions resolve `$GlobalVariable` and
    /// `picklist(...)` from the model at run time (trigger action + routing).
    #[test]
    fn global_variable_and_picklist_resolve_at_runtime() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "globals",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "global_variables": {
            "BatchSize": { "kind": "number", "value": 12 }
          },
          "picklists": {
            "Shift": { "entries": [ { "name": "Night", "value": "2" } ] }
          },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0,
                "triggers": { "on_create": [
                  { "action": "set_label", "name": "vip", "value": "$BatchSize" } ] } } },
            { "name": "KA", "kind": "sink", "params": {} },
            { "name": "KB", "kind": "sink", "params": {} }
          ],
          "connections": [
            { "from": "S", "to": "KA", "strategy": { "type": "by_condition", "conditions": [
                { "condition": "item.label.vip == $BatchSize && picklist(\"Shift\", \"Night\") == \"2\"",
                  "port": 0 },
                { "condition": "1 == 1", "port": 1 } ] } },
            { "from": "S", "to": "KB" }
          ]
        }"#,
        )
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        assert_eq!(report.objects["KA"].input_count.mean, 1.0);
        assert_eq!(report.objects["KB"].input_count.mean, 0.0);
    }

    /// task-154: an undefined global variable is a clear simulation error, not
    /// a silent zero.
    #[test]
    fn undefined_global_variable_is_a_sim_error() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "globals-missing",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1,
                "triggers": { "on_create": [
                  { "action": "set_label", "name": "vip", "value": "$Missing" } ] } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
        )
        .expect("model");
        let error = crate::run_model(&model).expect_err("unknown variable");
        let text = error.to_string();
        assert!(text.contains("Missing"), "{text}");
        assert!(text.contains("global variable"), "{text}");
    }

    fn performance_measure_model(replications: u32) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "measures",
          "run": {{ "stop_time": 20.0, "seed": 1, "replications": {replications} }},
          "performance_measures": [
            {{ "name": "WIP", "expression": "completed", "unit": "items" }}
          ],
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "exponential", "mean": 1.0 }},
                "first_arrival": 0.0 }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "K" }}]
        }}"#
        ))
        .expect("model")
    }

    /// task-155: `WIP = completed` is evaluated per replication and aggregated
    /// with the mean and a 95% CI over replications.
    #[test]
    fn performance_measure_single_and_multi_replication_stats() {
        let single = crate::run_model(&performance_measure_model(1)).expect("single");
        let measure = &single.performance_measures["WIP"];
        let replication = &single.replications[0];
        assert_eq!(
            measure.mean, replication.performance_measures["WIP"],
            "single replication mean is its value"
        );
        assert_eq!(measure.ci95, 0.0, "no interval for one sample");
        assert_eq!(measure.n, 1);

        let multi = crate::run_model(&performance_measure_model(3)).expect("multi");
        let measure = &multi.performance_measures["WIP"];
        assert_eq!(measure.n, 3);
        let expected = multi
            .replications
            .iter()
            .map(|replication| replication.performance_measures["WIP"])
            .sum::<f64>()
            / 3.0;
        assert!((measure.mean - expected).abs() < 1e-12);
        assert!(measure.std_dev.is_finite());
        assert!(measure.ci95 >= 0.0);
        assert!(measure.min <= measure.mean && measure.mean <= measure.max);
    }

    /// task-155: a failing expression names the measure instead of silently
    /// reporting zero.
    #[test]
    fn unknown_performance_measure_variable_is_an_error() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "bad-measure",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "performance_measures": [
            { "name": "Broken", "expression": "missing_variable + 1" }
          ],
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1.0 },
                "max_arrivals": 1 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
        )
        .expect("model");
        let error = crate::run_model(&model).expect_err("bad measure");
        let text = error.to_string();
        assert!(text.contains("Broken"), "{text}");
        assert!(text.contains("missing_variable"), "{text}");
    }

    /// task-155: models without measures keep exactly the old JSON/run output.
    #[test]
    fn models_without_performance_measures_are_unchanged() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "old-model",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1.0 },
                "max_arrivals": 2 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
        )
        .expect("model");
        let value = serde_json::to_value(&model).expect("model json");
        assert!(value.get("performance_measures").is_none());
        let report = crate::run_model(&model).expect("run");
        assert!(report.performance_measures.is_empty());
        let report_json = serde_json::to_value(&report).expect("report json");
        assert!(report_json.get("performance_measures").is_none());
    }

    fn tracked_model(variables: &str, interval: &str) -> String {
        format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "tracked",
          "run": {{ "stop_time": 5.0, "seed": 1, "replications": 1 }},
          "tracked_variables": [{variables}],
          {interval}
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 1000.0 }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "K" }}]
        }}"#
        )
    }

    /// task-158: a tracked variable samples once per interval and the final
    /// stop time is included.
    #[test]
    fn tracked_variable_samples_at_a_fixed_interval() {
        let model = Model::from_json(&tracked_model(
            r#"{ "name": "Clock", "expression": "time" }"#,
            "",
        ))
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        let samples = &report.tracked_series["Clock"];
        assert_eq!(
            samples,
            &vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0), (5.0, 5.0)]
        );
    }

    /// task-158: two tracked variables keep separate series and independent
    /// expressions.
    #[test]
    fn tracked_variables_are_independent() {
        let model = Model::from_json(&tracked_model(
            r#"{ "name": "A", "expression": "time" },
               { "name": "B", "expression": "time * 2" }"#,
            "",
        ))
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        let a = &report.tracked_series["A"];
        let b = &report.tracked_series["B"];
        assert_eq!(a.len(), 5);
        assert_eq!(b.len(), 5);
        for ((time, value), (other_time, doubled)) in a.iter().zip(b) {
            assert_eq!(time, other_time);
            assert!((doubled - value * 2.0).abs() < 1e-12);
        }
    }

    /// task-158: a bad tracked expression is a hard error that names the
    /// variable and the sample time.
    #[test]
    fn unknown_tracked_variable_symbol_is_an_error() {
        let model = Model::from_json(&tracked_model(
            r#"{ "name": "Broken", "expression": "missing_symbol" }"#,
            "",
        ))
        .expect("model");
        let error = crate::run_model(&model).expect_err("bad tracked expression");
        let text = error.to_string();
        assert!(text.contains("tracked_variables.Broken"), "{text}");
        assert!(text.contains("missing_symbol"), "{text}");
    }

    /// task-158: models without tracked variables keep the old report JSON and
    /// do not schedule sampling events.
    #[test]
    fn models_without_tracked_variables_are_unchanged() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "no-tracked",
          "run": { "stop_time": 5.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1.0 },
                "max_arrivals": 1 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
        )
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        assert!(report.tracked_series.is_empty());
        let json = serde_json::to_value(&report).expect("report json");
        assert!(json.get("tracked_series").is_none());
        assert!(
            serde_json::to_value(&model)
                .expect("model json")
                .get("tracked_variables")
                .is_none()
        );
    }

    /// task-158: name/uniqueness/expression/interval checks run from
    /// `Model::validate` (validate.rs belongs to another task).
    #[test]
    fn tracked_variable_validation_rejects_bad_definitions() {
        let duplicate = tracked_model(
            r#"{ "name": "A", "expression": "time" },
               { "name": "A", "expression": "created" }"#,
            "",
        );
        let error = Model::from_json(&duplicate).expect_err("duplicate");
        assert!(
            error
                .to_string()
                .contains("duplicate tracked variable name"),
            "{error}"
        );

        let bad = tracked_model(
            r#"{ "name": "Bad Name", "expression": "1 +" }"#,
            r#""tracked_interval": 0.0,"#,
        );
        let error = Model::from_json(&bad).expect_err("bad definition");
        let text = error.to_string();
        assert!(text.contains("tracked_variables[0].name"), "{text}");
        assert!(text.contains("tracked_variables[0].expression"), "{text}");
        assert!(text.contains("tracked_interval"), "{text}");
    }

    fn failure_model(count_states: &str, interarrival: f64, max_arrivals: Option<u32>) -> String {
        let max_line = max_arrivals
            .map(|max| format!(r#", "max_arrivals": {max}"#))
            .unwrap_or_default();
        // `count_states` is inserted verbatim (empty = omit the field).
        format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "mtbf",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": {interarrival} }},
                "first_arrival": 0.0{max_line} }} }},
            {{ "name": "P", "kind": "processor", "pos": [0.0, 0.0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 10.0 }},
                "triggers": {{ "on_breakdown": [{{ "action": "script", "source": "log(\"fail\");" }}] }},
                "failure": {{
                  "mtbf": {{ "distribution": "constant", "value": 15.0 }},
                  "mttr": {{ "distribution": "constant", "value": 1.0 }}{count_states} }} }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }}]
        }}"#
        )
    }

    /// task-163: an empty `count_time_in_states` is exactly the old wall-clock
    /// behaviour (bit-for-bit identical report).
    #[test]
    fn empty_count_time_in_states_matches_the_omitted_field() {
        let omitted = Model::from_json(&failure_model("", 20.0, None)).expect("omitted");
        let empty = Model::from_json(&failure_model(
            r#", "count_time_in_states": []"#,
            20.0,
            None,
        ))
        .expect("empty");
        let reference = crate::run_model(&omitted).expect("run omitted");
        let actual = crate::run_model(&empty).expect("run empty");
        assert_eq!(actual, reference, "empty list must not change any number");
    }

    /// task-163: counting only `processing` ignores idle time, so a single short
    /// job never reaches the 15 time-unit MTBF (the wall clock does fail).
    #[test]
    fn processing_only_clock_ignores_idle_time() {
        // One item only (next arrival beyond stop): 10 processing units < 15.
        let model = Model::from_json(&failure_model(
            r#", "count_time_in_states": ["processing"]"#,
            100.0,
            Some(1),
        ))
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        assert_eq!(
            report.objects["P"].down_fraction.mean, 0.0,
            "the clock pauses while the processor is idle"
        );
        let wall = Model::from_json(&failure_model("", 100.0, Some(1))).expect("wall");
        assert!(
            crate::run_model(&wall).expect("wall run").objects["P"]
                .down_fraction
                .mean
                > 0.0,
            "wall clock still fails while idle"
        );
    }

    /// task-163: the counted clock pauses during an idle gap and resumes on the
    /// next processing interval, so the first breakdown lands at t = 25 rather
    /// than at the wall-clock t = 15.
    #[test]
    fn processing_clock_pauses_and_resumes_across_an_idle_gap() {
        let model = Model::from_json(&failure_model(
            r#", "count_time_in_states": ["processing"]"#,
            20.0,
            None,
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let real: Vec<f64> = trace
            .events
            .iter()
            .filter(|event| event.event == "script_log" && event.message == "fail")
            .map(|event| event.time)
            .collect();
        let first = real.first().copied().expect("processing clock fires");
        // Work at 0..10 (10 units), idle 10..20, work 20..25 -> threshold 15.
        assert!((first - 25.0).abs() < 1e-9, "first failure at {first}");
    }

    /// task-163: support/usage of a task sequence: the engine generates the
    /// standard legs; baseline occurrences are consumed, extras are appended in
    /// exactly the listed order (unload, load, travel here).
    #[test]
    fn task_sequence_appends_extra_legs_in_order() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "task-steps",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0 } },
            { "name": "P", "kind": "processor", "pos": [100.0, 0.0], "params": {
                "process_time": { "distribution": "constant", "value": 1.0 },
                "process_operator": "D" } },
            { "name": "K", "kind": "sink", "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {
                "task_sequence": ["travel", "travel", "load", "unload",
                                  "unload", "load", "travel"] } },
            { "name": "T", "kind": "transporter", "pos": [0.0, 0.0], "params": {
                "speed": 10.0, "load_time": { "distribution": "constant", "value": 2.0 },
                "unload_time": { "distribution": "constant", "value": 3.0 },
                "home": "D" } }
          ],
          "connections": [
            { "from": "S", "to": "P" }, { "from": "P", "to": "K" },
            { "from": "D", "to": "T" }
          ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let times: Vec<f64> = trace
            .object("T")
            .filter(|event| event.event == "task_step_done")
            .map(|event| event.time)
            .collect();
        assert_eq!(times.len(), 3, "exactly three extra legs: {times:?}");
        // Extra legs in order: unload (3), load (2), travel (100/10 = 10).
        let home = trace
            .object("T")
            .filter(|event| event.event == "transport_home")
            .map(|event| event.time)
            .next()
            .expect("return home");
        assert!(
            (times[0] - home - 3.0).abs() < 1e-9,
            "{times:?} home={home}"
        );
        let deltas: Vec<f64> = times.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!((deltas[0] - 2.0).abs() < 1e-9, "{deltas:?}");
        assert!((deltas[1] - 10.0).abs() < 1e-9, "{deltas:?}");
    }

    /// task-163: counted time really advances inside the selected state: a
    /// single 20-unit processing block reaches a 15-unit MTBF at t = 15.
    #[test]
    fn processing_clock_advances_inside_the_counted_state() {
        let json = failure_model(
            r#", "count_time_in_states": ["processing"]"#,
            100.0,
            Some(1),
        )
        .replace(
            r#""process_time": { "distribution": "constant", "value": 10.0 }"#,
            r#""process_time": { "distribution": "constant", "value": 20.0 }"#,
        );
        let model = Model::from_json(&json).expect("model");
        let (_, trace) = crate::simulate_traced(
            &model,
            &SimConfig {
                stop_time: 100.0,
                warmup: 0.0,
                seed: 1,
            },
            0,
        )
        .expect("run");
        let first = trace
            .events
            .iter()
            .find(|event| event.event == "script_log" && event.message == "fail")
            .map(|event| event.time)
            .expect("counted clock fires");
        assert!((first - 15.0).abs() < 1e-9, "first failure at {first}");
    }

    fn lifecycle_model(end_script: &str) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "lifecycle",
          "run": {{ "stop_time": 5.0, "seed": 1, "replications": 1 }},
          "model_triggers": {{
            "start_of_experiment": [{{ "action": "script", "source": "log(\"experiment_start\");" }}],
            "end_of_experiment": [{{ "action": "script", "source": "log(\"experiment_end\");" }}],
            "start_of_scenario": [{{ "action": "script", "source": "log(\"scenario_start\");" }}],
            "end_of_scenario": [{{ "action": "script", "source": "log(\"scenario_end\");" }}],
            "start_of_replication": [{{ "action": "script", "source": "log(\"replication_start\");" }}],
            "end_of_replication": [{{ "action": "script", "source": "{end_script}" }}]
          }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 1000.0 }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "K" }}]
        }}"#
        ))
        .expect("model")
    }

    /// task-164: a plain run fires the two replication hooks around the run and
    /// leaves the experiment/scenario hooks untouched.
    #[test]
    fn replication_lifecycle_hooks_fire_once_per_run() {
        let model = lifecycle_model(r#"log(\"replication_end\");"#);
        let config = SimConfig {
            stop_time: 5.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let logs: Vec<String> = trace
            .events
            .iter()
            .filter(|event| event.event == "script_log")
            .map(|event| event.message.clone())
            .collect();
        assert_eq!(
            logs,
            vec![
                "replication_start".to_string(),
                "replication_end".to_string()
            ]
        );
        let start = trace
            .events
            .iter()
            .find(|event| event.message == "replication_start")
            .expect("start log")
            .time;
        let end = trace
            .events
            .iter()
            .find(|event| event.message == "replication_end")
            .expect("end log")
            .time;
        assert!(start < 1e-9);
        assert!((end - 5.0).abs() < 1e-9);
    }

    /// task-164: the experiment/scenario hooks fire in the documented order
    /// (2 scenarios x 2 replications) with the expected counts.
    #[test]
    fn experiment_lifecycle_hooks_run_in_order() {
        let model = lifecycle_model(r#"log(\"replication_end\");"#);
        let config = SimConfig {
            stop_time: 5.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut order = Vec::new();
        order.extend(hook_logs(
            &fire_model_hook_actions(
                &model,
                &config,
                model.model_triggers.start_of_experiment.clone(),
                "model_triggers.start_of_experiment",
            )
            .expect("experiment start"),
        ));
        for _scenario in 0..2 {
            order.extend(hook_logs(
                &fire_model_hook_actions(
                    &model,
                    &config,
                    model.model_triggers.start_of_scenario.clone(),
                    "model_triggers.start_of_scenario",
                )
                .expect("scenario start"),
            ));
            for replication in 0..2 {
                let config = SimConfig {
                    stop_time: 5.0,
                    warmup: 0.0,
                    seed: replication as u64 + 1,
                };
                let (_, trace) = crate::simulate_traced(&model, &config, replication as u32)
                    .expect("replication");
                order.extend(
                    trace
                        .events
                        .iter()
                        .filter(|event| event.event == "script_log")
                        .map(|event| event.message.clone()),
                );
            }
            order.extend(hook_logs(
                &fire_model_hook_actions(
                    &model,
                    &config,
                    model.model_triggers.end_of_scenario.clone(),
                    "model_triggers.end_of_scenario",
                )
                .expect("scenario end"),
            ));
        }
        order.extend(hook_logs(
            &fire_model_hook_actions(
                &model,
                &config,
                model.model_triggers.end_of_experiment.clone(),
                "model_triggers.end_of_experiment",
            )
            .expect("experiment end"),
        ));
        let count = |name: &str| order.iter().filter(|message| *message == name).count();
        assert_eq!(count("replication_start"), 4);
        assert_eq!(count("replication_end"), 4);
        assert_eq!(count("scenario_start"), 2);
        assert_eq!(count("scenario_end"), 2);
        assert_eq!(count("experiment_start"), 1);
        assert_eq!(count("experiment_end"), 1);
        assert_eq!(order.first().map(String::as_str), Some("experiment_start"));
        assert_eq!(order.last().map(String::as_str), Some("experiment_end"));
        assert_eq!(
            &order[1..5],
            &[
                "scenario_start".to_string(),
                "replication_start".to_string(),
                "replication_end".to_string(),
                "replication_start".to_string(),
            ]
        );
    }

    /// task-164: a failing lifecycle script is a SimError::Script with the hook
    /// location; syntax errors are caught by validation before the run.
    #[test]
    fn lifecycle_hook_errors_are_reported() {
        let model = lifecycle_model("nope();");
        let error = crate::run_model(&model).expect_err("runtime error");
        let text = error.to_string();
        assert!(text.contains("model_triggers.end_of_replication"), "{text}");
        assert!(text.contains("script error at"), "{text}");

        // `lifecycle_model` validates too, so build the broken JSON manually.
        let bad = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "bad-hook",
              "model_triggers": { "end_of_replication": [
                { "action": "script", "source": "1 +" } ] },
              "objects": [
                { "name": "S", "kind": "source", "params": {} },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [{ "from": "S", "to": "K" }]
            }"#,
        );
        assert!(
            bad.is_err(),
            "uncompilable hook script must fail validation"
        );
    }

    /// task-164: empty new hooks must not make `model_triggers` appear in JSON;
    /// non-empty new hooks must be written.
    #[test]
    fn empty_lifecycle_hooks_keep_the_model_json_unchanged() {
        let plain = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "plain",
              "objects": [
                { "name": "S", "kind": "source", "params": {} },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [{ "from": "S", "to": "K" }]
            }"#,
        )
        .expect("plain");
        let empty = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "plain",
              "model_triggers": {},
              "objects": [
                { "name": "S", "kind": "source", "params": {} },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [{ "from": "S", "to": "K" }]
            }"#,
        )
        .expect("empty");
        assert_eq!(
            serde_json::to_value(&plain).expect("plain json"),
            serde_json::to_value(&empty).expect("empty json"),
            "empty triggers must serialize exactly like no triggers"
        );
        assert!(
            serde_json::to_value(&plain)
                .expect("json")
                .get("model_triggers")
                .is_none()
        );

        let model = lifecycle_model(r#"log(\"replication_end\");"#);
        let value = serde_json::to_value(&model).expect("json");
        let triggers = value.get("model_triggers").expect("model_triggers present");
        assert!(triggers.get("start_of_experiment").is_some());
        assert!(triggers.get("end_of_replication").is_some());
    }

    fn executer_report() -> crate::SimReport {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "executer-detail",
          "run": { "stop_time": 100.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0 } },
            { "name": "P", "kind": "processor", "pos": [30.0, 0.0], "params": {
                "process_time": { "distribution": "constant", "value": 5.0 },
                "process_operator": "D" } },
            { "name": "K", "kind": "sink", "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [10.0, 0.0], "params": {} },
            { "name": "T", "kind": "transporter", "pos": [0.0, 0.0], "params": {
                "speed": 1.0, "load_time": { "distribution": "constant", "value": 2.0 },
                "unload_time": { "distribution": "constant", "value": 3.0 },
                "home": "D" } }
          ],
          "connections": [
            { "from": "S", "to": "P" }, { "from": "P", "to": "K" },
            { "from": "D", "to": "T" }
          ]
        }"#,
        )
        .expect("model");
        crate::run_model(&model).expect("run")
    }

    /// task-166: the four task phases add up to the whole busy interval; the
    /// geometry gives 10 empty (pickup) + 20 loaded (delivery) + 20 empty
    /// (return), 2 loading and 3 unloading.
    #[test]
    fn executer_activity_phases_match_a_single_task() {
        let report = executer_report();
        let stats = &report.replications[0].objects["T"].state_times;
        assert!((stats.travel_empty - 30.0).abs() < 1e-6, "{stats:?}");
        assert!((stats.travel_loaded - 20.0).abs() < 1e-6, "{stats:?}");
        assert!((stats.loading - 2.0).abs() < 1e-6, "{stats:?}");
        assert!((stats.unloading - 3.0).abs() < 1e-6, "{stats:?}");
        // On-site waiting/service: processing 5 + the release unload 3.
        assert!((stats.utilize - 8.0).abs() < 1e-6, "{stats:?}");
    }

    /// task-166: the new details are subdivisions of the existing states and
    /// never double-count.
    #[test]
    fn executer_activity_details_stay_inside_the_parent_states() {
        let report = executer_report();
        let stats = &report.replications[0].objects["T"].state_times;
        let details = stats.travel_empty
            + stats.travel_loaded
            + stats.loading
            + stats.unloading
            + stats.utilize;
        let parents = stats.idle + stats.busy + stats.blocked + stats.down + stats.off_shift;
        assert!(details <= parents + 1e-6, "{stats:?}");
        assert!((details - stats.busy).abs() < 1e-6, "{stats:?}");
    }

    /// task-166: time spent waiting for a traffic-control slot is `blocked`,
    /// not any of the travel details.
    #[test]
    fn traffic_control_wait_is_blocked_and_not_travel() {
        let report =
            crate::run_model(&traffic_model("mutual_exclusion", 1, 1.0)).expect("traffic run");
        let stats = &report.replications[0].objects["T2"].state_times;
        assert!(stats.blocked > 5.0, "the second executer must be blocked");
        // Pickup 0 + return home 10 = 10 empty driving; the 10 s wait is not travel.
        assert!((stats.travel_empty - 10.0).abs() < 1e-6, "{stats:?}");
        assert!((stats.travel_loaded - 10.0).abs() < 1e-6, "{stats:?}");
        assert!(
            stats.travel_empty + stats.travel_loaded < 20.0 + stats.blocked - 1.0,
            "waiting must not add travel time: {stats:?}"
        );
    }

    /// task-166: a model without task executers serializes exactly like before:
    /// the new detail keys are absent from the JSON.
    #[test]
    fn models_without_executers_keep_the_report_json_unchanged() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "no-executer",
          "run": { "stop_time": 5.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1.0 },
                "max_arrivals": 2 } },
            { "name": "P", "kind": "processor", "params": {
                "process_time": { "distribution": "constant", "value": 1.0 } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [
            { "from": "S", "to": "P" }, { "from": "P", "to": "K" }
          ]
        }"#,
        )
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        let text = serde_json::to_string(&report).expect("json");
        for key in [
            "travel_empty",
            "travel_loaded",
            "loading",
            "unloading",
            "utilize",
        ] {
            assert!(
                !text.contains(key),
                "non-executer report leaks {key}: {text}"
            );
        }
        // A state-detail object of a non-executer has no new keys either.
        let state = &report.replications[0].objects["P"].state_times;
        assert_eq!(state.travel_empty, 0.0);
        let state_json = serde_json::to_value(state).expect("state json");
        assert!(state_json.get("utilize").is_none());
    }

    /// task-166: an executer report exposes the detail in the aggregated report
    /// JSON so dashboards can read it.
    #[test]
    fn executer_report_exposes_the_detail_aggregates() {
        let report = executer_report();
        let object = &report.objects["T"];
        let empty = object
            .travel_empty
            .as_ref()
            .expect("executer detail in the report");
        assert!((empty.mean - 30.0).abs() < 1e-6);
        let loaded = object.travel_loaded.as_ref().expect("travel_loaded");
        assert!((loaded.mean - 20.0).abs() < 1e-6);
    }

    /// task-168: omitting `kind` keeps the old model JSON and the old level
    /// sampling behaviour.
    #[test]
    fn tracked_kind_defaults_to_level_without_changing_json() {
        let json = tracked_model(r#"{ "name": "Clock", "expression": "time" }"#, "");
        let model = Model::from_json(&json).expect("model");
        let value = serde_json::to_value(&model).expect("json");
        assert!(
            value["tracked_variables"][0].get("kind").is_none(),
            "default level must not add a kind field"
        );
        let report = crate::run_model(&model).expect("run");
        assert_eq!(
            report.tracked_series["Clock"],
            vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0), (4.0, 4.0), (5.0, 5.0)]
        );
        assert!(report.tracked_summaries.is_empty());
        // Explicit `level` is equivalent to the omitted kind, including the
        // serialized (old-format) report JSON.
        let explicit = Model::from_json(&tracked_model(
            r#"{ "name": "Clock", "expression": "time", "kind": "level" }"#,
            "",
        ))
        .expect("explicit level");
        let explicit_report = crate::run_model(&explicit).expect("run explicit");
        assert_eq!(
            serde_json::to_string(&report).expect("json"),
            serde_json::to_string(&explicit_report).expect("json")
        );
    }

    /// task-168: kinetic samples are the difference between the current and
    /// previous expression values divided by the actual interval.
    #[test]
    fn kinetic_tracked_variable_reports_the_rate_of_change() {
        let model = Model::from_json(&tracked_model(
            r#"{ "name": "Slope", "expression": "time * 3", "kind": "kinetic" }"#,
            "",
        ))
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        let samples = &report.tracked_series["Slope"];
        assert_eq!(samples.len(), 5);
        assert_eq!(samples[0], (1.0, 0.0), "first sample has no previous value");
        for pair in samples.windows(2) {
            let interval = pair[1].0 - pair[0].0;
            let expected = 3.0;
            assert!(
                (pair[1].1 - expected).abs() < 1e-9,
                "kinetic sample {:?} over interval {interval}",
                pair[1]
            );
        }
    }

    /// task-168: `state` reports model-wide state shares that sum to one.
    #[test]
    fn state_tracked_variable_reports_state_shares() {
        let model = Model::from_json(&tracked_model(
            r#"{ "name": "States", "expression": "time", "kind": "state" }"#,
            "",
        ))
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        let shares = report.tracked_summaries["States"]
            .state_shares
            .as_ref()
            .expect("state shares");
        let total: f64 = shares.values().sum();
        assert!((total - 1.0).abs() < 1e-9, "{shares:?}");
        assert!(shares["idle"] > 0.0);
        assert!(report.tracked_series.is_empty());
    }

    /// task-168: `staytime` reports the stay-time distribution of completed
    /// items; n tracks completions and p50 sits inside [min, max].
    #[test]
    fn staytime_tracked_variable_reports_the_distribution() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "staytime",
          "run": { "stop_time": 30.0, "seed": 1, "replications": 1 },
          "tracked_variables": [
            { "name": "Stay", "expression": "time", "kind": "staytime" }
          ],
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1.0 },
                "max_arrivals": 5 } },
            { "name": "P", "kind": "processor", "params": {
                "process_time": { "distribution": "constant", "value": 2.0 } } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [
            { "from": "S", "to": "P" }, { "from": "P", "to": "K" }
          ]
        }"#,
        )
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        let summary = report.tracked_summaries["Stay"]
            .staytime
            .as_ref()
            .expect("staytime summary");
        assert_eq!(summary.n, report.replications[0].items_completed);
        assert!(summary.n > 0);
        assert!(summary.p50 >= summary.min && summary.p50 <= summary.max);
        assert!(summary.p90 >= summary.p50 && summary.p90 <= summary.max);
    }

    /// task-168: all four kinds coexist without leaking into each other's slots.
    #[test]
    fn tracked_kinds_are_independent() {
        let model = Model::from_json(&tracked_model(
            r#"{ "name": "L", "expression": "time", "kind": "level" },
               { "name": "K", "expression": "time * 2", "kind": "kinetic" },
               { "name": "S", "expression": "time", "kind": "state" },
               { "name": "T", "expression": "time", "kind": "staytime" }"#,
            "",
        ))
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        assert_eq!(report.tracked_series.len(), 2);
        assert_eq!(report.tracked_summaries.len(), 2);
        assert!(report.tracked_series.contains_key("L"));
        assert!(report.tracked_series.contains_key("K"));
        assert!(report.tracked_summaries["S"].state_shares.is_some());
        assert!(report.tracked_summaries["T"].staytime.is_some());
        assert!(!report.tracked_series.contains_key("S"));
        assert!(!report.tracked_series.contains_key("T"));
    }

    /// task-168: schema/serde rejects an unknown tracked-variable kind.
    #[test]
    fn invalid_tracked_kind_is_rejected() {
        let json = tracked_model(
            r#"{ "name": "Bad", "expression": "time", "kind": "bogus" }"#,
            "",
        );
        let error = Model::from_json(&json).expect_err("bad kind");
        let text = error.to_string();
        assert!(text.contains("bogus"), "{text}");
        assert!(text.contains("unknown variant"), "{text}");
    }

    fn append_leg_model(extra: &str) -> String {
        format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "extra-legs",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S1", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "S2", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "P1", "kind": "processor", "pos": [10.0, 0.0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "P2", "kind": "processor", "pos": [10.0, 0.0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "K1", "kind": "sink", "params": {{}} }},
            {{ "name": "K2", "kind": "sink", "params": {{}} }},
            {{ "name": "D", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {{
                "task_sequence": ["travel", "travel", "load", "unload"{extra}] }} }},
            {{ "name": "T", "kind": "transporter", "pos": [0.0, 0.0], "params": {{
                "speed": 1.0, "load_time": {{ "distribution": "constant", "value": 2.0 }},
                "unload_time": {{ "distribution": "constant", "value": 3.0 }},
                "home": "D" }} }}
          ],
          "connections": [
            {{ "from": "S1", "to": "P1" }}, {{ "from": "P1", "to": "K1" }},
            {{ "from": "S2", "to": "P2" }}, {{ "from": "P2", "to": "K2" }},
            {{ "from": "D", "to": "T" }}
          ]
        }}"#
        )
    }

    fn task_step_done_times(trace: &TraceLog) -> Vec<f64> {
        trace
            .object("T")
            .filter(|event| event.event == "task_step_done")
            .map(|event| event.time)
            .collect()
    }

    /// task-169: the old append legs (travel/load/unload) stay deterministic and
    /// byte-identical across runs.
    #[test]
    fn legacy_append_legs_are_deterministic_and_unchanged() {
        let model = Model::from_json(&append_leg_model(r#", "unload", "load""#)).expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (first, first_trace) = crate::simulate_traced(&model, &config, 0).expect("first run");
        let (second, second_trace) =
            crate::simulate_traced(&model, &config, 0).expect("second run");
        assert_eq!(
            serde_json::to_string(&first).expect("json"),
            serde_json::to_string(&second).expect("json")
        );
        assert_eq!(
            task_step_done_times(&first_trace),
            task_step_done_times(&second_trace)
        );
        // Two queued tasks, each with the two extra append legs.
        assert_eq!(task_step_done_times(&first_trace).len(), 4);
    }

    /// task-169: utilize consumes real wall time in the executer and keeps it
    /// unavailable for the next queued task; it is charged to the utilize detail.
    #[test]
    fn utilize_extra_leg_occupies_the_executer() {
        let model = Model::from_json(&append_leg_model(
            r#", {"kind": "utilize", "duration": {"distribution": "constant", "value": 5.0}}"#,
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let home = trace
            .object("T")
            .filter(|event| event.event == "transport_home")
            .map(|event| event.time)
            .next()
            .expect("home");
        let done = task_step_done_times(&trace);
        assert!(done.len() >= 2, "{done:?}");
        assert!((done[0] - home - 5.0).abs() < 1e-9, "{done:?} home={home}");
        assert!(result.objects["T"].state_times.utilize >= 5.0 - 1e-9);
        let pickups: Vec<f64> = trace
            .object("T")
            .filter(|event| event.event == "transport_pickup")
            .map(|event| event.time)
            .collect();
        assert!(pickups.len() >= 2, "{pickups:?}");
        assert!(
            pickups[1] >= done[0] - 1e-9,
            "the queued task may not start during utilize: {pickups:?} done={done:?}"
        );
    }

    /// task-169: a break is idle time (not blocked), blocks new assignment while
    /// it runs, and does not inflate the utilize detail.
    #[test]
    fn break_extra_leg_is_idle_and_occupies_the_executer() {
        let model = Model::from_json(&append_leg_model(
            r#", {"kind": "break", "duration": {"distribution": "constant", "value": 7.0}}"#,
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let home = trace
            .object("T")
            .filter(|event| event.event == "transport_home")
            .map(|event| event.time)
            .next()
            .expect("home");
        let done = task_step_done_times(&trace);
        assert!(done.len() >= 2, "{done:?}");
        assert!((done[0] - home - 7.0).abs() < 1e-9, "{done:?} home={home}");
        assert!(result.objects["T"].state_times.idle >= 7.0 - 1e-9);
        assert!(result.objects["T"].state_times.blocked < 1e-9);
        let pickups: Vec<f64> = trace
            .object("T")
            .filter(|event| event.event == "transport_pickup")
            .map(|event| event.time)
            .collect();
        assert!(pickups.len() >= 2, "{pickups:?}");
        assert!(pickups[1] >= done[0] - 1e-9, "{pickups:?} done={done:?}");
    }

    /// task-169: utilization/break validation rejects missing or non-positive
    /// durations (unknown kinds are rejected by the enum/serde layer).
    #[test]
    fn invalid_extra_legs_are_rejected() {
        let missing = Model::from_json(&append_leg_model(r#", {"kind": "utilize"}"#));
        let error = missing.expect_err("missing duration");
        assert!(
            error.to_string().contains("task_sequence[4].duration"),
            "{error}"
        );

        let zero = Model::from_json(&append_leg_model(
            r#", {"kind": "break", "duration": {"distribution": "constant", "value": 0.0}}"#,
        ));
        let error = zero.expect_err("non-positive duration");
        assert!(error.to_string().contains("must be positive"), "{error}");

        let unknown = Model::from_json(&append_leg_model(
            r#", {"kind": "teleport", "duration": {"distribution": "constant", "value": 1.0}}"#,
        ));
        assert!(unknown.is_err(), "unknown kind must be rejected");
    }

    /// task-169: models without `task_sequence` serialize and run exactly as
    /// before (no new fields, deterministic same-seed repeat).
    #[test]
    fn models_without_task_sequence_are_unchanged() {
        let plain = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "plain",
              "run": { "stop_time": 5.0, "seed": 1, "replications": 1 },
              "objects": [
                { "name": "S", "kind": "source", "params": {
                    "interarrival": { "distribution": "constant", "value": 1.0 },
                    "max_arrivals": 2 } },
                { "name": "P", "kind": "processor", "params": {
                    "process_time": { "distribution": "constant", "value": 1.0 } } },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
            }"#,
        )
        .expect("plain");
        let with_empty = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "plain",
              "run": { "stop_time": 5.0, "seed": 1, "replications": 1 },
              "objects": [
                { "name": "S", "kind": "source", "params": {
                    "interarrival": { "distribution": "constant", "value": 1.0 },
                    "max_arrivals": 2 } },
                { "name": "P", "kind": "processor", "params": {
                    "process_time": { "distribution": "constant", "value": 1.0 } } },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [{ "from": "S", "to": "P" }, { "from": "P", "to": "K" }]
            }"#,
        )
        .expect("empty");
        let a = crate::run_model(&plain).expect("run");
        let b = crate::run_model(&with_empty).expect("run");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&crate::run_model(&plain).expect("repeat")).expect("json")
        );
    }

    fn trace_item_events(trace: &TraceLog) -> Vec<(String, String, u64)> {
        trace
            .events
            .iter()
            .filter_map(|event| {
                event
                    .data
                    .get("item")
                    .map(|item| (event.event.clone(), event.object.clone(), *item as u64))
            })
            .collect()
    }

    fn simple_trace_model(max_arrivals: u32) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "item-path",
          "run": {{ "stop_time": 100.0, "seed": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1.0 }},
                "max_arrivals": {max_arrivals} }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "K" }}]
        }}"#
        ))
        .expect("model")
    }

    /// task-171: one item keeps one id across its whole source -> sink path and
    /// gets real enter/exit events on both objects.
    #[test]
    fn item_trace_follows_one_item_from_source_to_sink() {
        let model = simple_trace_model(1);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let events = trace_item_events(&trace);
        assert_eq!(
            events,
            vec![
                ("arrive".to_string(), "S".to_string(), 1),
                ("enter".to_string(), "S".to_string(), 1),
                ("exit".to_string(), "S".to_string(), 1),
                ("enter".to_string(), "K".to_string(), 1),
                ("exit".to_string(), "K".to_string(), 1),
                ("flow".to_string(), "S".to_string(), 1),
            ]
        );
        assert_eq!(trace.events[0].event, "arrive");
        assert_eq!(trace.events[0].data["item"], 1.0);
        assert_eq!(trace_object(&trace, "K", "enter"), 1);
        assert_eq!(trace_object(&trace, "K", "exit"), 1);
    }

    fn trace_object(trace: &TraceLog, object: &str, event: &str) -> usize {
        trace
            .events
            .iter()
            .filter(|entry| entry.object == object && entry.event == event)
            .count()
    }

    /// task-171: enter/exit are real object-boundary events and enter comes
    /// before exit for each object.
    #[test]
    fn item_enter_exit_are_paired_in_order() {
        let model = simple_trace_model(1);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        for object in ["S", "K"] {
            let enter = trace
                .events
                .iter()
                .position(|event| event.object == object && event.event == "enter")
                .expect("enter");
            let exit = trace
                .events
                .iter()
                .position(|event| event.object == object && event.event == "exit")
                .expect("exit");
            assert!(enter < exit, "enter must precede exit for {object}");
            assert!(trace.events[enter].data.contains_key("item"));
            assert!(trace.events[exit].data.contains_key("item"));
        }
    }

    /// task-171: two interleaved items never swap ids on their enter/exit path.
    #[test]
    fn interleaved_items_keep_distinct_ids() {
        let model = simple_trace_model(2);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        assert_eq!(result.items_created, 2);
        let ids: std::collections::BTreeSet<u64> = trace_item_events(&trace)
            .into_iter()
            .map(|(_, _, id)| id)
            .collect();
        assert_eq!(ids, [1, 2].into_iter().collect());
        for id in [1, 2] {
            let path: Vec<(String, String)> = trace_item_events(&trace)
                .into_iter()
                .filter(|(_, _, entry)| *entry == id)
                .map(|(event, object, _)| (event, object))
                .collect();
            assert!(
                path.contains(&("enter".to_string(), "S".to_string())),
                "{id}: {path:?}"
            );
            assert!(
                path.contains(&("enter".to_string(), "K".to_string())),
                "{id}: {path:?}"
            );
            assert!(
                path.contains(&("exit".to_string(), "K".to_string())),
                "{id}: {path:?}"
            );
        }
    }

    /// task-171: all transport events carry the served item id, including the
    /// return-home leg (where the requester link may already be cleared).
    #[test]
    fn transport_events_carry_the_served_item_id() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "trace-transport",
          "run": { "stop_time": 100.0, "seed": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0 } },
            { "name": "P", "kind": "processor", "pos": [10.0, 0.0], "params": {
                "process_time": { "distribution": "constant", "value": 1.0 },
                "process_operator": "D" } },
            { "name": "K", "kind": "sink", "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {} },
            { "name": "T", "kind": "transporter", "pos": [0.0, 0.0],
               "params": { "speed": 1.0, "home": "D" } }
          ],
          "connections": [
            { "from": "S", "to": "P" }, { "from": "P", "to": "K" },
            { "from": "D", "to": "T" }
          ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        for event in [
            "transport_pickup",
            "transport_depart",
            "transport_arrive",
            "transport_home",
        ] {
            let entry = trace
                .object("T")
                .find(|entry| entry.event == event)
                .unwrap_or_else(|| panic!("missing {event}"));
            assert_eq!(entry.data.get("item"), Some(&1.0), "{entry:?}");
        }
    }

    /// task-171: events unrelated to a tracked item do not get a guessed id.
    #[test]
    fn unrelated_events_do_not_carry_an_item_id() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "trace-unrelated",
          "run": { "stop_time": 20.0, "seed": 1 },
          "tracked_variables": [{ "name": "Clock", "expression": "time" }],
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 1000.0 } },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let samples: Vec<_> = trace
            .events
            .iter()
            .filter(|event| event.event == "tracked_sample")
            .collect();
        assert!(!samples.is_empty());
        assert!(samples.iter().all(|event| !event.data.contains_key("item")));
        assert!(trace_item_events(&trace).is_empty());
    }

    fn explicit_route_model(sequence: &str, mode: &str) -> String {
        format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "explicit-route",
          "run": {{ "stop_time": 20.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "P", "kind": "processor", "pos": [10.0, 0.0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }},
            {{ "name": "D", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {{
                "task_sequence_mode": "{mode}",
                "task_sequence": [{sequence}] }} }},
            {{ "name": "T", "kind": "transporter", "pos": [0.0, 0.0], "params": {{
                "speed": 1.0, "home": "D" }} }}
          ],
          "connections": [
            {{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }},
            {{ "from": "D", "to": "T" }}
          ]
        }}"#
        )
    }

    /// task-173: explicit travel legs follow home / object / position targets in
    /// order (the zero-length home leg is skipped without an event).
    #[test]
    fn explicit_route_follows_home_object_and_position_targets() {
        let model = Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": "home"},
               {"kind": "travel", "target": {"object": "P"}},
               {"kind": "travel", "target": {"position": [15.0, 2.0]}}"#,
            "explicit",
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let legs: Vec<(f64, f64, f64)> = trace
            .object("T")
            .filter(|event| event.event == "transport_route_arrive")
            .map(|event| (event.time, event.data["x"], event.data["y"]))
            .collect();
        assert_eq!(legs.len(), 3, "{legs:?}");
        assert!((legs[0].0 - 0.0).abs() < 1e-9, "{legs:?}");
        assert!((legs[0].1 - 0.0).abs() < 1e-9, "{legs:?}");
        assert!((legs[1].0 - 10.0).abs() < 1e-9, "{legs:?}");
        assert!((legs[1].1 - 10.0).abs() < 1e-9, "{legs:?}");
        assert!(
            (legs[2].0 - (10.0 + 5.385_164_807_134_505)).abs() < 1e-9,
            "{legs:?}"
        );
        assert!((legs[2].1 - 15.0).abs() < 1e-9, "{legs:?}");
        assert!((legs[2].2 - 2.0).abs() < 1e-9, "{legs:?}");
    }

    /// task-175: pickup / requester targets are real now; a load/unload chain
    /// fires OperatorLoaded once and waits for OperatorReleased before leaving.
    #[test]
    fn explicit_chain_supports_pickup_requester_and_operator_sync() {
        let model = Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": "pickup"},
               {"kind": "load"},
               {"kind": "travel", "target": "requester"},
               {"kind": "unload"},
               {"kind": "travel", "target": "home"}"#,
            "explicit",
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let loaded: Vec<f64> = trace
            .events
            .iter()
            .filter(|event| event.event == "operator_loaded")
            .map(|event| event.time)
            .collect();
        assert_eq!(loaded.len(), 1, "OperatorLoaded must fire exactly once");
        let released = trace
            .events
            .iter()
            .find(|event| event.event == "operator_released")
            .map(|event| event.time)
            .expect("release");
        let home_leg = trace
            .object("T")
            .filter(|event| event.event == "transport_route_depart")
            .find(|event| {
                event.data["x"] == 0.0 && event.data["y"] == 0.0 && event.time >= released - 1e-9
            })
            .map(|event| event.time)
            .expect("return-home leg must start after release");
        assert!(
            home_leg >= released - 1e-9,
            "the executer left before release: home={home_leg} release={released}"
        );
        let arrivals: Vec<(f64, f64)> = trace
            .object("T")
            .filter(|event| event.event == "transport_route_arrive")
            .map(|event| (event.data["x"], event.data["y"]))
            .collect();
        assert_eq!(arrivals, vec![(0.0, 0.0), (10.0, 0.0), (0.0, 0.0)]);
    }

    /// task-173: explicit validation requires travel-only, non-empty routes with
    /// targets, and rejects unknown object names.
    #[test]
    fn explicit_route_validation_rejects_incomplete_routes() {
        let empty = Model::from_json(&explicit_route_model("", "explicit"));
        let text = empty.expect_err("empty route").to_string();
        assert!(text.contains("non-empty task route"), "{text}");

        let no_target =
            Model::from_json(&explicit_route_model(r#"{"kind": "travel"}"#, "explicit"));
        let text = no_target.expect_err("missing target").to_string();
        assert!(text.contains("require a target"), "{text}");

        let unknown = Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": {"object": "Nope"}}"#,
            "explicit",
        ));
        let text = unknown.expect_err("unknown object").to_string();
        assert!(text.contains("unknown object"), "{text}");

        let target_on_load = Model::from_json(&explicit_route_model(
            r#"{"kind": "load", "target": "home"}"#,
            "explicit",
        ));
        let text = target_on_load.expect_err("target on load").to_string();
        assert!(
            text.contains("only travel legs may have a target"),
            "{text}"
        );
    }

    /// task-173: append mode keeps the old JSON (mode absent) and old behaviour.
    #[test]
    fn append_mode_keeps_legacy_task_sequence_json_and_behaviour() {
        let model = Model::from_json(&explicit_route_model(
            r#""travel","travel","load","unload""#,
            "append",
        ))
        .expect("model");
        let value = serde_json::to_value(&model).expect("json");
        assert!(
            value["objects"][3]["params"]
                .get("task_sequence_mode")
                .is_none(),
            "append mode must not serialize a mode field"
        );
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (first, _) = crate::simulate_traced(&model, &config, 0).expect("first");
        let (second, _) = crate::simulate_traced(&model, &config, 0).expect("second");
        assert_eq!(
            serde_json::to_string(&first).expect("json"),
            serde_json::to_string(&second).expect("json")
        );
    }

    /// task-173: explicit routes are deterministic for one seed.
    #[test]
    fn explicit_route_is_deterministic() {
        let model = Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": "home"},
               {"kind": "travel", "target": {"position": [12.0, 3.0]}}"#,
            "explicit",
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 7,
        };
        let (first, first_trace) = crate::simulate_traced(&model, &config, 0).expect("first");
        let (second, second_trace) = crate::simulate_traced(&model, &config, 0).expect("second");
        assert_eq!(
            serde_json::to_string(&first).expect("json"),
            serde_json::to_string(&second).expect("json")
        );
        let path = |trace: &TraceLog| -> Vec<(f64, f64, f64)> {
            trace
                .object("T")
                .filter(|event| event.event == "transport_route_arrive")
                .map(|event| (event.time, event.data["x"], event.data["y"]))
                .collect()
        };
        assert_eq!(path(&first_trace), path(&second_trace));
    }

    /// task-173: an explicit route leg waiting for a `traffic_control` slot
    /// spends that interval in blocked, not in travel_empty.
    #[test]
    fn explicit_route_traffic_wait_is_blocked() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "explicit-traffic",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S1", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0 } },
            { "name": "S2", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0 } },
            { "name": "P1", "kind": "processor", "pos": [10.0, 0.0], "params": {
                "process_time": { "distribution": "constant", "value": 1.0 },
                "process_operator": "D" } },
            { "name": "P2", "kind": "processor", "pos": [10.0, 0.0], "params": {
                "process_time": { "distribution": "constant", "value": 1.0 },
                "process_operator": "D" } },
            { "name": "K1", "kind": "sink", "params": {} },
            { "name": "K2", "kind": "sink", "params": {} },
            { "name": "D", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {
                "task_sequence_mode": "explicit",
                "task_sequence": [
                    { "kind": "travel", "target": { "position": [10.0, 0.0] } }
                ] } },
            { "name": "T1", "kind": "transporter", "pos": [0.0, 0.0],
               "params": { "speed": 1.0, "home": "D" } },
            { "name": "T2", "kind": "transporter", "pos": [0.0, 0.0],
               "params": { "speed": 1.0, "home": "D" } },
            { "name": "N1", "kind": "network_node", "pos": [0.0, 0.0],
               "params": { "edges": [
                   { "from": "N1", "to": "N2", "distance": 10.0 } ] } },
            { "name": "N2", "kind": "network_node", "pos": [10.0, 0.0], "params": {} },
            { "name": "TC", "kind": "traffic_control", "pos": [5.0, 5.0], "params": {
                "capacity": 1, "mode": "mutual_exclusion", "speed_factor": 1.0 } }
          ],
          "connections": [
            { "from": "S1", "to": "P1" }, { "from": "P1", "to": "K1" },
            { "from": "S2", "to": "P2" }, { "from": "P2", "to": "K2" },
            { "from": "D", "to": "T1" }, { "from": "D", "to": "T2" },
            { "from": "TC", "to": "N1" }, { "from": "TC", "to": "N2" }
          ]
        }"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 10.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, _) = crate::simulate_traced(&model, &config, 0).expect("run");
        let blocked = result.objects["T2"].state_times.blocked;
        let travel = result.objects["T2"].state_times.travel_empty;
        assert!(
            blocked > 5.0,
            "T2 must wait for the control area: {blocked}"
        );
        assert!(
            travel < 1e-9,
            "blocked route time must not be counted as travel: {travel}"
        );
    }

    /// task-175: utilize/break legs after unload run before the executer is
    /// allowed to leave, and keep the task-169 activity attribution.
    #[test]
    fn explicit_chain_runs_utilize_and_break_after_unload() {
        let model = Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": "pickup"},
               {"kind": "load"},
               {"kind": "travel", "target": "requester"},
               {"kind": "unload"},
               {"kind": "utilize", "duration": {"distribution": "constant", "value": 5.0}},
               {"kind": "break", "duration": {"distribution": "constant", "value": 7.0}},
               {"kind": "travel", "target": "home"}"#,
            "explicit",
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let released = trace
            .events
            .iter()
            .find(|event| event.event == "operator_released")
            .map(|event| event.time)
            .expect("release");
        let home_departs: Vec<f64> = trace
            .object("T")
            .filter(|event| {
                event.event == "transport_route_depart"
                    && event.data["x"] == 0.0
                    && event.data["y"] == 0.0
            })
            .map(|event| event.time)
            .filter(|time| *time >= released - 1e-9)
            .collect();
        assert_eq!(home_departs.len(), 1, "{home_departs:?}");
        assert!(
            home_departs[0] >= released + 7.0 - 1e-9,
            "break must finish before the return leg: release={released} home={:?}",
            home_departs
        );
        assert!(result.objects["T"].state_times.utilize >= 5.0 - 1e-9);
        assert!(result.objects["T"].state_times.idle >= 7.0 - 1e-9);
        assert_eq!(
            trace
                .events
                .iter()
                .filter(|event| event.event == "operator_loaded")
                .count(),
            1
        );
    }

    /// task-175: the explicit chain visits pickup -> requester -> home in order.
    #[test]
    fn explicit_chain_leg_order_is_table_driven() {
        let model = Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": "pickup"},
               {"kind": "load"},
               {"kind": "travel", "target": "requester"},
               {"kind": "unload"},
               {"kind": "travel", "target": "home"}"#,
            "explicit",
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let route: Vec<(String, f64, f64)> = trace
            .object("T")
            .filter(|event| {
                event.event == "transport_route_depart" || event.event == "transport_route_arrive"
            })
            .map(|event| (event.event.clone(), event.data["x"], event.data["y"]))
            .collect();
        let mut expected = Vec::new();
        for (label, x, y) in [
            ("transport_route_depart", 0.0, 0.0),
            ("transport_route_arrive", 0.0, 0.0),
            ("transport_route_depart", 10.0, 0.0),
            ("transport_route_arrive", 10.0, 0.0),
            ("transport_route_depart", 0.0, 0.0),
            ("transport_route_arrive", 0.0, 0.0),
        ] {
            expected.push((label.to_string(), x, y));
        }
        assert_eq!(route, expected);
    }

    /// task-175: a full load/unload chain is deterministic for one seed.
    #[test]
    fn explicit_chain_with_load_unload_is_deterministic() {
        let model = Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": "pickup"},
               {"kind": "load"},
               {"kind": "travel", "target": "requester"},
               {"kind": "unload"},
               {"kind": "travel", "target": "home"}"#,
            "explicit",
        ))
        .expect("model");
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 3,
        };
        let (first, first_trace) = crate::simulate_traced(&model, &config, 0).expect("first");
        let (second, second_trace) = crate::simulate_traced(&model, &config, 0).expect("second");
        assert_eq!(
            serde_json::to_string(&first).expect("json"),
            serde_json::to_string(&second).expect("json")
        );
        assert_eq!(
            first_trace
                .object("T")
                .filter(|event| event.event.starts_with("transport_route_")
                    || event.event == "operator_loaded"
                    || event.event == "operator_released")
                .map(|event| (event.event.clone(), event.time, event.data.clone()))
                .collect::<Vec<_>>(),
            second_trace
                .object("T")
                .filter(|event| event.event.starts_with("transport_route_")
                    || event.event == "operator_loaded"
                    || event.event == "operator_released")
                .map(|event| (event.event.clone(), event.time, event.data.clone()))
                .collect::<Vec<_>>()
        );
    }

    fn slug_belt_model(slug: &str, min_gap: &str) -> String {
        format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "slug",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1.0 }},
                "max_arrivals": 3, "first_arrival": 0.0 }} }},
            {{ "name": "C", "kind": "conveyor", "params": {{
                "speed": 1.0, "length": 10.0, "accumulating": true,
                "item_spacing": 1.0{slug}{min_gap} }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "C" }}, {{ "from": "C", "to": "K" }}]
        }}"#
        )
    }

    fn slug_sim(slug: &str, min_gap: &str) -> (Simulation<'static>, usize) {
        // Leak the model so the Simulation can hold the reference in tests.
        let model: &'static Model = Box::leak(Box::new(
            Model::from_json(&slug_belt_model(slug, min_gap)).expect("model"),
        ));
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let sim = Simulation::new(model, config, false).expect("sim");
        let index = sim.by_name["C"];
        (sim, index)
    }

    /// task-176: items closer than slug_min_gap move as one rigid group; the
    /// whole group stops when its leader is blocked and resumes together.
    #[test]
    fn slug_group_moves_and_blocks_as_one() {
        let (mut sim, c) = slug_sim(r#", "slug": true"#, r#", "slug_min_gap": 2.0"#);
        sim.objects[c].belt.as_mut().expect("belt").positions = vec![8.0, 6.5, 5.0];
        sim.objects[c].belt.as_mut().expect("belt").holds = vec![0.0; 3];
        sim.objects[c].belt_last_update = 0.0;
        sim.now = 2.0;
        sim.sync_belt(c);
        let positions = sim.objects[c]
            .belt
            .as_ref()
            .expect("belt")
            .positions
            .clone();
        assert!((positions[0] - 10.0).abs() < 1e-9, "{positions:?}");
        assert!((positions[1] - 8.5).abs() < 1e-9, "{positions:?}");
        assert!((positions[2] - 7.0).abs() < 1e-9, "{positions:?}");
        // The leader is at the exit: the whole group stops.
        sim.now = 4.0;
        sim.sync_belt(c);
        assert_eq!(
            sim.objects[c].belt.as_ref().expect("belt").positions,
            vec![10.0, 8.5, 7.0]
        );
        // Release the leader: the rest of the slug resumes with unchanged gaps.
        sim.objects[c].belt.as_mut().expect("belt").remove_front();
        sim.now = 5.0;
        sim.sync_belt(c);
        let positions = sim.objects[c]
            .belt
            .as_ref()
            .expect("belt")
            .positions
            .clone();
        assert!((positions[0] - 9.5).abs() < 1e-9, "{positions:?}");
        assert!((positions[1] - 8.0).abs() < 1e-9, "{positions:?}");
        assert!(
            (positions[0] - positions[1] - 1.5).abs() < 1e-9,
            "{positions:?}"
        );
    }

    /// task-176: items farther apart than slug_min_gap stay independent.
    #[test]
    fn far_apart_items_are_not_slugged() {
        let (mut sim, c) = slug_sim(r#", "slug": true"#, r#", "slug_min_gap": 2.0"#);
        sim.objects[c].belt.as_mut().expect("belt").positions = vec![8.0, 4.0];
        sim.objects[c].belt.as_mut().expect("belt").holds = vec![0.0; 2];
        sim.objects[c].belt_last_update = 0.0;
        sim.now = 2.0;
        sim.sync_belt(c);
        let positions = sim.objects[c]
            .belt
            .as_ref()
            .expect("belt")
            .positions
            .clone();
        assert!((positions[0] - 10.0).abs() < 1e-9, "{positions:?}");
        assert!((positions[1] - 6.0).abs() < 1e-9, "{positions:?}");
        assert!(
            (positions[0] - positions[1] - 4.0).abs() < 1e-9,
            "{positions:?}"
        );
    }

    /// task-176: `slug` absent and `slug:false` produce identical old reports.
    #[test]
    fn slug_false_keeps_the_old_report() {
        let absent = Model::from_json(&slug_belt_model("", "")).expect("absent");
        let explicit_false =
            Model::from_json(&slug_belt_model(r#", "slug": false"#, "")).expect("false");
        let value = serde_json::to_value(&absent).expect("model json");
        assert!(
            value["objects"][1]["params"].get("slug").is_none(),
            "default slug must not be serialized"
        );
        let a = crate::run_model(&absent).expect("run absent");
        let b = crate::run_model(&explicit_false).expect("run false");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&crate::run_model(&absent).expect("repeat")).expect("json")
        );
    }

    /// task-176: slug runs are deterministic for one seed.
    #[test]
    fn slug_mode_is_deterministic() {
        let model = Model::from_json(&slug_belt_model(
            r#", "slug": true"#,
            r#", "slug_min_gap": 2.0"#,
        ))
        .expect("model");
        let a = crate::run_model(&model).expect("first");
        let b = crate::run_model(&model).expect("second");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
        assert_eq!(a.completed.mean, b.completed.mean);
    }

    /// task-176: slug mode still produces self-consistent stay/throughput stats.
    #[test]
    fn slug_mode_keeps_stay_and_throughput_consistent() {
        let model = Model::from_json(&slug_belt_model(
            r#", "slug": true"#,
            r#", "slug_min_gap": 2.0"#,
        ))
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        assert!(report.replications[0].items_completed > 0);
        assert!(report.completed.mean > 0.0);
        assert!(report.objects["C"].throughput.mean > 0.0);
        assert!(report.objects["C"].avg_stay_time.mean >= 0.0);
    }

    fn script_check_model(source: &str, model_trigger: bool) -> Model {
        let trigger = if model_trigger {
            format!(
                r#""model_triggers": {{ "on_start": [{{ "action": "script", "source": {source:?} }}] }},"#
            )
        } else {
            format!(
                r#""objects": [ {{ "name": "S", "kind": "source", "params": {{ "triggers": {{ "on_create": [{{ "action": "script", "source": {source:?} }}] }} }} }}, {{ "name": "K", "kind": "sink", "params": {{}} }} ], "connections": [{{ "from": "S", "to": "K" }}],"#
            )
        };
        let json = if model_trigger {
            format!(
                r#"{{ "format": "openflex-model", "version": 1, "name": "script-check", {trigger} "objects": [ {{ "name": "S", "kind": "source", "params": {{}} }}, {{ "name": "K", "kind": "sink", "params": {{}} }} ], "connections": [{{ "from": "S", "to": "K" }}] }}"#
            )
        } else {
            format!(
                r#"{{ "format": "openflex-model", "version": 1, "name": "script-check", {trigger} "run": {{ "stop_time": 1.0 }} }}"#
            )
        };
        Model::parse_json(&json).expect("model")
    }

    /// task-177: check-scripts detects compile failures without running.
    #[test]
    fn check_scripts_reports_compile_failures() {
        let model = script_check_model("let x = ;", false);
        let checks = check_scripts(&model);
        assert_eq!(checks.len(), 1);
        assert!(
            checks[0].location.contains("objects.S.triggers.on_create"),
            "{:?}",
            checks[0]
        );
        assert_eq!(checks[0].source, "let x = ;");
        assert!(
            checks[0]
                .error
                .as_ref()
                .is_some_and(|error| error.contains("compile")),
            "{:?}",
            checks[0]
        );
    }

    /// task-177: a runtime-only error passes check-scripts but is enriched
    /// with source, location, model time and the available-variable list.
    #[test]
    fn script_runtime_error_has_source_time_and_available_variables() {
        let model = script_check_model("log(does_not_exist);", true);
        let checks = check_scripts(&model);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].error.is_none(), "{:?}", checks[0]);
        let error = crate::run_model(&model).expect_err("runtime script error");
        let text = error.to_string();
        assert!(text.starts_with("script error at "), "{text}");
        assert!(text.contains("log(does_not_exist);"), "{text}");
        assert!(text.contains("t=0.000000"), "{text}");
        assert!(text.contains("available:"), "{text}");
        assert!(text.contains("time"), "{text}");
        assert!(text.contains("Variable not found"), "{text}");
    }

    /// The `available:` hint must list the *script* surface. `item.type` is an
    /// expression path a rhai script cannot resolve, so advertising it sends the
    /// user somewhere that cannot work.
    #[test]
    fn script_hint_lists_script_identifiers_not_expression_paths() {
        let model = script_check_model("log(nope_variable);", true);
        let error = crate::run_model(&model).expect_err("runtime script error");
        let text = error.to_string();
        let hint = text
            .lines()
            .find(|line| line.trim_start().starts_with("available:"))
            .unwrap_or_else(|| panic!("no hint line in {text}"));
        assert!(
            hint.contains("item_type"),
            "the script variable is listed: {hint}"
        );
        assert!(hint.contains("set_label("), "functions are listed: {hint}");
        assert!(
            !hint.contains("item.type"),
            "an expression path leaked into a script hint: {hint}"
        );
        // The same wording is shared with the expression layer, so they cannot
        // drift apart silently.
        assert!(
            crate::script::SCRIPT_VARIABLES
                .iter()
                .any(|(name, _)| *name == "item_type")
        );
    }

    /// The diagnostic is printed once: `SimError::Script`'s field is not called
    /// `source`, so a cause-chain printer (anyhow's `{:?}`) does not repeat text
    /// that the `Display` already inlines.
    #[test]
    fn script_diagnostic_is_not_duplicated_by_the_cause_chain() {
        let model = script_check_model("log(does_not_exist);", true);
        let error = crate::run_model(&model).expect_err("runtime script error");
        let display = error.to_string();
        assert_eq!(
            display.matches("Variable not found").count(),
            1,
            "the message repeats itself: {display}"
        );
        assert_eq!(
            display.matches("available:").count(),
            1,
            "the hint repeats itself: {display}"
        );
        // A cause-chain printer must show the same single occurrence.
        let anyhow_style = format!("{:?}", anyhow_like(&error));
        assert_eq!(
            anyhow_style.matches("Variable not found").count(),
            1,
            "a cause chain duplicates the diagnostic: {anyhow_style}"
        );
    }

    /// Minimal stand-in for how an error-reporting front end walks a chain.
    fn anyhow_like(error: &SimError) -> String {
        let mut text = error.to_string();
        let mut cursor: Option<&dyn std::error::Error> = std::error::Error::source(error);
        while let Some(cause) = cursor {
            text.push_str(&format!("\n\nCaused by:\n    {cause}"));
            cursor = cause.source();
        }
        text
    }

    /// task-177: valid scripts report passed counts and no errors.
    #[test]
    fn check_scripts_counts_valid_scripts() {
        let model = script_check_model("log(\"ok\");", false);
        let checks = check_scripts(&model);
        assert_eq!(checks.len(), 1);
        assert!(checks[0].error.is_none(), "{:?}", checks[0]);
    }

    /// task-177: compile errors already surfaced by run_model keep the old
    /// prefix and gain source/location context.
    #[test]
    fn compile_error_keeps_existing_prefix_and_context() {
        let model = script_check_model("let broken = ;", false);
        let error = compile_model_scripts(&model).expect_err("compile error");
        let text = error.to_string();
        assert!(text.starts_with("script error at "), "{text}");
        assert!(text.contains("script:"), "{text}");
        assert!(text.contains("let broken = ;"), "{text}");
    }

    /// task-177: long source snippets are truncated in the message.
    #[test]
    fn script_error_source_is_truncated() {
        let long = format!("let value = 1;{}log(does_not_exist);", " ".repeat(200));
        let model = script_check_model(&long, true);
        let error = crate::run_model(&model).expect_err("runtime error");
        let text = error.to_string();
        assert!(text.contains("…"), "source should be ellipsised: {text}");
        assert!(text.len() < long.len() + 400, "message should be bounded");
    }

    fn merge_model_json(merge: &str, stop: f64, type_a: u32, type_b: u32) -> String {
        format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "merge",
          "run": {{ "stop_time": {stop}, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "SA", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1.0 }},
                "first_arrival": 0.0, "item_type": {type_a} }} }},
            {{ "name": "SB", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1.0 }},
                "first_arrival": 0.0, "item_type": {type_b} }} }},
            {{ "name": "QA", "kind": "queue", "params": {{ "max_content": 1000 }} }},
            {{ "name": "QB", "kind": "queue", "params": {{ "max_content": 1000 }} }},
            {{ "name": "C", "kind": "conveyor", "params": {{
                "speed": 1.0, "length": 3.0, "item_spacing": 1.0,
                "accumulating": true, "max_content": 10,
                "join_inputs": 2{merge} }} }},
            {{ "name": "P", "kind": "processor", "params": {{
                "process_time": {{ "distribution": "constant", "value": 5.0 }} }} }},
            {{ "name": "KA", "kind": "sink", "params": {{}} }},
            {{ "name": "KB", "kind": "sink", "params": {{}} }}
          ],
          "connections": [
            {{ "from": "SA", "to": "QA" }}, {{ "from": "SB", "to": "QB" }},
            {{ "from": "QA", "to": "C" }}, {{ "from": "QB", "to": "C" }},
            {{ "from": "C", "to": "P" }},
            {{ "from": "P", "to": "KA", "strategy": {{ "type": "by_item_type" }} }},
            {{ "from": "P", "to": "KB" }}
          ]
        }}"#
        )
    }

    fn merge_model(merge: &str, stop: f64, type_a: u32, type_b: u32) -> Model {
        Model::from_json(&merge_model_json(merge, stop, type_a, type_b)).expect("model")
    }

    /// task-178: round_robin omitted and explicit are byte-identical.
    #[test]
    fn merge_round_robin_is_the_old_behaviour() {
        let absent = merge_model("", 200.0, 0, 1);
        let explicit = merge_model(r#", "merge_mode": "round_robin""#, 200.0, 0, 1);
        let a = crate::run_model(&absent).expect("run absent");
        let b = crate::run_model(&explicit).expect("run explicit");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
    }

    /// task-178: weighted 3:1 controls the long-run mix within ±5%.
    #[test]
    fn merge_weighted_three_to_one() {
        let model = merge_model(
            r#", "merge_mode": "weighted", "merge_weight": [3.0, 1.0]"#,
            1000.0,
            0,
            1,
        );
        let report = crate::run_model(&model).expect("run");
        let a = report.objects["KA"].input_count.mean;
        let b = report.objects["KB"].input_count.mean;
        assert!(a > 20.0 && b > 5.0, "both inputs should flow: {a}/{b}");
        let ratio = a / b;
        assert!(
            (ratio - 3.0).abs() / 3.0 <= 0.05,
            "expected 3:1, got {a}:{b} = {ratio}"
        );
    }

    /// task-178: by_item_type gives lower item types priority when both are
    /// ready.
    #[test]
    fn merge_by_item_type_prioritises_low_types() {
        let model = merge_model(r#", "merge_mode": "by_item_type""#, 200.0, 2, 1);
        let report = crate::run_model(&model).expect("run");
        let a = report.objects["KA"].input_count.mean;
        let b = report.objects["KB"].input_count.mean;
        assert!(b > a * 2.0, "type 1 should dominate type 2: {a}/{b}");
    }

    /// task-178: validation rejects bad weights and weights outside weighted.
    #[test]
    fn merge_weight_validation_rejects_bad_configurations() {
        let missing = Model::parse_json(&merge_model_json(
            r#", "merge_mode": "weighted""#,
            10.0,
            0,
            1,
        ))
        .expect("parse");
        assert!(
            missing
                .validate()
                .expect_err("missing")
                .to_string()
                .contains("requires merge_weight")
        );
        let wrong = Model::parse_json(&merge_model_json(
            r#", "merge_mode": "weighted", "merge_weight": [1.0]"#,
            10.0,
            0,
            1,
        ))
        .expect("parse");
        assert!(
            wrong
                .validate()
                .expect_err("wrong")
                .to_string()
                .contains("expected 2 weights")
        );
        let nonpositive = Model::parse_json(&merge_model_json(
            r#", "merge_mode": "weighted", "merge_weight": [3.0, 0.0]"#,
            10.0,
            0,
            1,
        ))
        .expect("parse");
        assert!(
            nonpositive
                .validate()
                .expect_err("nonpositive")
                .to_string()
                .contains("positive")
        );
        let misplaced = Model::parse_json(&merge_model_json(
            r#", "merge_weight": [1.0, 1.0]"#,
            10.0,
            0,
            1,
        ))
        .expect("parse");
        assert!(
            misplaced
                .validate()
                .expect_err("misplaced")
                .to_string()
                .contains("only valid with")
        );
    }

    #[test]
    fn merge_same_seed_is_deterministic() {
        let model = merge_model(
            r#", "merge_mode": "weighted", "merge_weight": [3.0, 1.0]"#,
            500.0,
            0,
            1,
        );
        let a = crate::run_model(&model).expect("first");
        let b = crate::run_model(&model).expect("second");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
    }

    /// task-178: a blocked main-belt entry must not consume weighted quota.
    #[test]
    fn blocked_merge_entry_does_not_consume_weighted_quota() {
        let model = merge_model(
            r#", "merge_mode": "weighted", "merge_weight": [3.0, 1.0]"#,
            10.0,
            0,
            1,
        );
        let config = SimConfig {
            stop_time: 10.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("sim");
        let qa = sim.by_name["QA"];
        let c = sim.by_name["C"];
        let conn = sim.outputs[qa]
            .iter()
            .copied()
            .find(|conn| sim.conns[*conn].to == c)
            .expect("QA -> C");
        // The main belt entry is occupied (back item at 0 < spacing).
        sim.objects[c].belt.as_mut().expect("belt").positions = vec![0.0];
        sim.objects[c].belt.as_mut().expect("belt").holds = vec![0.0];
        let before = sim.objects[c].merge_counts.clone();
        assert!(sim.deliver(qa, conn, item()).is_ok());
        assert_eq!(
            sim.objects[c].merge_counts, before,
            "blocked attempt consumed quota"
        );
        // Clear the entry and deliver again: now the quota is consumed.
        sim.objects[c]
            .belt
            .as_mut()
            .expect("belt")
            .positions
            .clear();
        sim.objects[c].belt.as_mut().expect("belt").holds.clear();
        sim.deliver(qa, conn, item()).expect("delivery");
        assert!(sim.objects[c].merge_counts[0] > 0.0);
    }

    fn photo_eye_model(
        slug: &str,
        eye_extras: &str,
        max_arrivals: u32,
        interarrival: f64,
    ) -> Model {
        let json = format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "photo-eye",
          "run": {{ "stop_time": 30.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": {interarrival} }},
                "max_arrivals": {max_arrivals}, "first_arrival": 0.0 }} }},
            {{ "name": "C", "kind": "conveyor", "params": {{
                "speed": 1.0, "length": 10.0, "item_spacing": 1.0,
                "accumulating": true{slug},
                "photo_eyes": [{{ "position": 2.0, "name": "PE"{eye_extras} }}],
                "triggers": {{ "on_photo_eye": [
                    {{ "action": "script", "source": "log(\"photo\");" }} ] }} }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "C" }}, {{ "from": "C", "to": "K" }}]
        }}"#
        );
        Model::parse_json(&json).expect("model")
    }

    fn photo_logs(trace: &TraceLog, message: &str) -> usize {
        trace
            .events
            .iter()
            .filter(|event| event.event == "script_log" && event.message == message)
            .count()
    }

    /// task-179: one item entering/leaving the beam fires Blocked and Cleared
    /// exactly once; the legacy on_photo_eye still fires per crossing.
    #[test]
    fn photo_eye_four_states_fire_blocked_and_cleared_once() {
        let model = photo_eye_model(
            "",
            r#", "triggers": {
                "on_blocked": [{ "action": "script", "source": "log(\"blocked\");" }],
                "on_cleared": [{ "action": "script", "source": "log(\"cleared\");" }]
            }"#,
            1,
            5.0,
        );
        let config = SimConfig {
            stop_time: 30.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        assert_eq!(photo_logs(&trace, "blocked"), 1);
        assert_eq!(photo_logs(&trace, "cleared"), 1);
        assert_eq!(photo_logs(&trace, "photo"), 1);
        let blocked = trace
            .events
            .iter()
            .find(|event| event.message == "blocked")
            .map(|event| event.time)
            .expect("blocked");
        let cleared = trace
            .events
            .iter()
            .find(|event| event.message == "cleared")
            .map(|event| event.time)
            .expect("cleared");
        assert!(blocked < cleared, "Blocked must precede Cleared");
    }

    /// task-179: contiguous items on a slug do not re-fire Blocked until the
    /// beam actually clears.
    #[test]
    fn photo_eye_slug_stays_blocked_for_contiguous_items() {
        let model = photo_eye_model(
            r#", "slug": true, "slug_min_gap": 2.0"#,
            r#", "range": 2.0, "triggers": {
                "on_blocked": [{ "action": "script", "source": "log(\"blocked\");" }],
                "on_cleared": [{ "action": "script", "source": "log(\"cleared\");" }]
            }"#,
            2,
            1.0,
        );
        let config = SimConfig {
            stop_time: 30.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        assert_eq!(
            photo_logs(&trace, "blocked"),
            1,
            "contiguous slug should stay blocked"
        );
        assert_eq!(photo_logs(&trace, "cleared"), 1);
        assert_eq!(
            photo_logs(&trace, "photo"),
            2,
            "legacy trigger remains per item"
        );
    }

    /// task-179: photo-eye trigger scripts are compiled by validate/check-scripts.
    #[test]
    fn photo_eye_trigger_scripts_are_validated() {
        let model = photo_eye_model(
            "",
            r#", "triggers": {
                "on_blocked": [{ "action": "script", "source": "let broken = ;" }]
            }"#,
            1,
            5.0,
        );
        // The helper uses from_json, which would fail; parse without validation
        // to inspect the issue.
        let json = serde_json::to_string(&model).expect("json");
        let raw = Model::parse_json(&json).expect("parse");
        let error = raw.validate().expect_err("compile error");
        assert!(
            error
                .to_string()
                .contains("objects.C.photo_eyes[0].triggers.on_blocked[0]"),
            "{error}"
        );
    }

    /// task-179: empty triggers are omitted from old model JSON.
    #[test]
    fn photo_eye_old_json_is_unchanged() {
        let model = photo_eye_model("", "", 1, 5.0);
        let value = serde_json::to_value(&model).expect("json");
        assert!(
            value["objects"][1]["params"]["photo_eyes"][0]
                .get("triggers")
                .is_none(),
            "empty photo-eye triggers must not serialize"
        );
    }

    /// task-179: two items inside the same beam keep a single Blocked state;
    /// holding them there does not repeat a transition.
    #[test]
    fn photo_eye_two_items_do_not_jitter_or_retrigger() {
        let model = photo_eye_model(
            "",
            r#", "triggers": {
                "on_blocked": [{ "action": "script", "source": "log(\"blocked\");" }],
                "on_cleared": [{ "action": "script", "source": "log(\"cleared\");" }]
            }"#,
            1,
            5.0,
        );
        let config = SimConfig {
            stop_time: 30.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, true).expect("sim");
        let c = sim.by_name["C"];
        sim.objects[c].belt.as_mut().expect("belt").positions = vec![2.0, 2.0];
        sim.objects[c].belt.as_mut().expect("belt").holds = vec![0.0; 2];
        sim.update_photo_eyes(c).expect("blocked");
        assert_eq!(sim.objects[c].photo_eye_states[0], PhotoEyeState::Blocked);
        // Re-running while items are still held at the eye repeats nothing.
        sim.update_photo_eyes(c).expect("still blocked");
        assert_eq!(sim.objects[c].photo_eye_states[0], PhotoEyeState::Blocked);
        let blocked = sim
            .trace
            .as_ref()
            .expect("trace")
            .events
            .iter()
            .filter(|event| event.message == "blocked")
            .count();
        assert_eq!(blocked, 1, "Blocked transition fired more than once");
        sim.objects[c]
            .belt
            .as_mut()
            .expect("belt")
            .positions
            .clear();
        sim.update_photo_eyes(c).expect("cleared");
        assert_eq!(sim.objects[c].photo_eye_states[0], PhotoEyeState::Cleared);
    }

    /// task-179: a photo eye without triggers leaves the model/report JSON
    /// unchanged (empty triggers serialize like an absent block).
    #[test]
    fn photo_eye_without_triggers_is_unchanged() {
        let absent = photo_eye_model("", "", 1, 5.0);
        let mut with_empty: Model =
            Model::parse_json(&serde_json::to_string(&absent).expect("json")).expect("parse");
        if let ObjectParams::Conveyor(params) = &mut with_empty.objects[1].params {
            params.photo_eyes[0].triggers = Default::default();
        }
        let a = crate::run_model(&absent).expect("run absent");
        let b = crate::run_model(&with_empty).expect("run empty");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
    }

    /// task-179: photo-eye state/trigger runs are deterministic for one seed.
    #[test]
    fn photo_eye_triggers_are_deterministic() {
        let model = photo_eye_model(
            "",
            r#", "triggers": {
                "on_blocked": [{ "action": "script", "source": "log(\"blocked\");" }],
                "on_cleared": [{ "action": "script", "source": "log(\"cleared\");" }]
            }"#,
            1,
            5.0,
        );
        let config = SimConfig {
            stop_time: 30.0,
            warmup: 0.0,
            seed: 3,
        };
        let (first, first_trace) = crate::simulate_traced(&model, &config, 0).expect("first");
        let (second, second_trace) = crate::simulate_traced(&model, &config, 0).expect("second");
        assert_eq!(
            serde_json::to_string(&first).expect("json"),
            serde_json::to_string(&second).expect("json")
        );
        let logs = |trace: &TraceLog| -> Vec<(f64, String)> {
            trace
                .events
                .iter()
                .filter(|event| event.event == "script_log")
                .map(|event| (event.time, event.message.clone()))
                .collect()
        };
        assert_eq!(logs(&first_trace), logs(&second_trace));
    }

    fn photo_eye_timing_model(
        length: f64,
        eye: f64,
        range: Option<f64>,
        max_arrivals: u32,
        interval: f64,
        station: bool,
    ) -> Model {
        let range = range.map_or_else(String::new, |range| format!(r#", "range": {range}"#));
        let station = if station {
            r#", "stations": [{ "position": 10.0, "process_time": {
                "distribution": "constant", "value": 10.0 } }]"#
                .to_string()
        } else {
            String::new()
        };
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "eye-timing",
          "run": {{ "stop_time": 40.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": {interval} }},
                "max_arrivals": {max_arrivals}, "first_arrival": 0.0 }} }},
            {{ "name": "C", "kind": "conveyor", "params": {{
                "speed": 1.0, "length": {length}, "item_spacing": 1.0,
                "accumulating": true{station},
                "photo_eyes": [{{ "position": {eye}, "name": "PE"{range},
                    "triggers": {{
                        "on_blocked": [{{ "action": "script", "source": "log(\"blocked\");" }}],
                        "on_cleared": [{{ "action": "script", "source": "log(\"cleared\");" }}]
                    }} }}] }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "C" }}, {{ "from": "C", "to": "K" }}]
        }}"#
        ))
        .expect("model")
    }

    fn photo_time(trace: &TraceLog, message: &str) -> Option<f64> {
        trace
            .events
            .iter()
            .find(|event| event.event == "script_log" && event.message == message)
            .map(|event| event.time)
    }

    /// task-180: boundary crossing itself is a belt event, so a single item
    /// produces Blocked at 9.5 and Cleared at 10.5, not at the exit.
    #[test]
    fn photo_eye_boundary_events_are_timely() {
        let model = photo_eye_timing_model(20.0, 10.0, None, 1, 5.0, false);
        let config = SimConfig {
            stop_time: 40.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let blocked = photo_time(&trace, "blocked").expect("blocked");
        let cleared = photo_time(&trace, "cleared").expect("cleared");
        assert!((blocked - 9.5).abs() < 0.01, "blocked at {blocked}");
        assert!((cleared - 10.5).abs() < 0.01, "cleared at {cleared}");
    }

    /// task-180: with three items in the beam, Blocked is one transition and
    /// Cleared happens only after the last one has left the sensing span.
    #[test]
    fn photo_eye_clears_when_last_item_leaves_the_span() {
        let model = photo_eye_timing_model(20.0, 10.0, None, 1, 5.0, false);
        let config = SimConfig {
            stop_time: 40.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, true).expect("sim");
        let c = sim.by_name["C"];
        sim.objects[c].belt.as_mut().expect("belt").positions = vec![10.6, 9.6, 8.6];
        sim.objects[c].belt.as_mut().expect("belt").holds = vec![0.0; 3];
        sim.now = 9.5;
        sim.update_photo_eyes(c).expect("blocked");
        assert_eq!(sim.objects[c].photo_eye_states[0], PhotoEyeState::Blocked);
        // All three have moved out of [9.5, 10.5); the last one leaves at 10.5.
        sim.objects[c].belt.as_mut().expect("belt").positions = vec![12.0, 11.0, 10.6];
        sim.now = 10.5;
        sim.update_photo_eyes(c).expect("cleared");
        assert_eq!(sim.objects[c].photo_eye_states[0], PhotoEyeState::Cleared);
        let logs: Vec<(f64, String)> = sim
            .trace
            .as_ref()
            .expect("trace")
            .events
            .iter()
            .filter(|event| event.event == "script_log")
            .map(|event| (event.time, event.message.clone()))
            .collect();
        assert_eq!(
            logs,
            vec![(9.5, "blocked".to_string()), (10.5, "cleared".to_string())]
        );
    }

    /// task-180: a held item on the eye does not create a zero-interval spin.
    #[test]
    fn photo_eye_on_a_held_item_does_not_spin() {
        let model = photo_eye_timing_model(20.0, 10.0, None, 1, 5.0, true);
        let config = SimConfig {
            stop_time: 40.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let cleared = photo_time(&trace, "cleared").expect("cleared");
        assert!(
            cleared >= 19.0,
            "station hold should delay clearing: {cleared}"
        );
        assert!(
            trace.events.len() < 100,
            "unexpected photo-eye event spin: {}",
            trace.events.len()
        );
    }

    /// task-180: photo-eye timing is deterministic for one seed.
    #[test]
    fn photo_eye_boundary_events_are_deterministic() {
        let model = photo_eye_timing_model(20.0, 10.0, None, 3, 1.0, false);
        let config = SimConfig {
            stop_time: 40.0,
            warmup: 0.0,
            seed: 3,
        };
        let (first, first_trace) = crate::simulate_traced(&model, &config, 0).expect("first");
        let (second, second_trace) = crate::simulate_traced(&model, &config, 0).expect("second");
        assert_eq!(
            serde_json::to_string(&first).expect("json"),
            serde_json::to_string(&second).expect("json")
        );
        assert_eq!(first_trace, second_trace);
    }

    /// task-180: no photo-eye conveyor keeps the old report exactly.
    #[test]
    fn photo_eye_boundary_events_do_not_touch_plain_conveyors() {
        let json = serde_json::to_string(&photo_eye_timing_model(20.0, 10.0, None, 1, 5.0, false))
            .expect("json");
        let plain = Model::from_json(&json.replacen(
            r#", "photo_eyes": [{ "position": 10.0, "name": "PE", "triggers": {
                        "on_blocked": [{ "action": "script", "source": "log("blocked");" }],
                        "on_cleared": [{ "action": "script", "source": "log("cleared");" }]
                    } }]"#,
            "",
            1,
        ))
        .expect("plain");
        let a = crate::run_model(&plain).expect("first");
        let b = crate::run_model(&plain).expect("second");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
        assert!(a.tracked_series.is_empty());
    }

    fn decision_point_model(condition: &str, once: bool, item_type: u32) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "decision-point",
          "run": {{ "stop_time": 20.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0, "item_type": {item_type} }} }},
            {{ "name": "C", "kind": "conveyor", "params": {{
                "speed": 1.0, "length": 20.0, "item_spacing": 1.0, "accumulating": true,
                "decision_points": [{{
                    "position": 5.0, "range": 0.5, "condition": "{condition}",
                    "once_per_item": {once},
                    "actions": [{{ "action": "script", "source": "log(\"dp\");" }}]
                }}] }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [{{ "from": "S", "to": "C" }}, {{ "from": "C", "to": "K" }}]
        }}"#
        ))
        .expect("model")
    }

    /// task-181: decision point evaluates item condition and runs actions at
    /// the sensing boundary (not at the exit).
    #[test]
    fn decision_point_runs_actions_for_matching_item() {
        let model = decision_point_model("item.type == 0", true, 0);
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let logs: Vec<f64> = trace
            .events
            .iter()
            .filter(|event| event.event == "script_log" && event.message == "dp")
            .map(|event| event.time)
            .collect();
        assert_eq!(logs.len(), 1, "{logs:?}");
        assert!((logs[0] - 4.5).abs() < 0.01, "decision at {logs:?}");
    }

    /// task-181: a false item condition executes nothing.
    #[test]
    fn decision_point_skips_non_matching_item() {
        let model = decision_point_model("item.type == 0", true, 1);
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        assert!(
            !trace
                .events
                .iter()
                .any(|event| event.event == "script_log" && event.message == "dp")
        );
    }

    /// task-181: an invalid decision condition/actions and position are rejected.
    #[test]
    fn decision_point_validation_rejects_bad_points() {
        let model = decision_point_model("item.type == 0", true, 0);
        let mut raw =
            Model::parse_json(&serde_json::to_string(&model).expect("json")).expect("parse");
        if let ObjectParams::Conveyor(params) = &mut raw.objects[1].params {
            params.decision_points[0].position = 99.0;
        }
        let error = raw.validate().expect_err("position");
        assert!(
            error.to_string().contains("decision_points[0].position"),
            "{error}"
        );

        let mut raw =
            Model::parse_json(&serde_json::to_string(&model).expect("json")).expect("parse");
        if let ObjectParams::Conveyor(params) = &mut raw.objects[1].params {
            params.decision_points[0].actions = vec![openflex_model::Action::Script {
                source: "let broken = ;".into(),
            }];
        }
        let error = raw.validate().expect_err("action");
        assert!(
            error.to_string().contains("decision_points[0].actions"),
            "{error}"
        );
    }

    /// task-181: decision points are deterministic and absent decision points
    /// leave a conveyor report unchanged.
    #[test]
    fn decision_points_are_deterministic_and_optional() {
        let model = decision_point_model("item.type == 0", true, 0);
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 3,
        };
        let (first, first_trace) = crate::simulate_traced(&model, &config, 0).expect("first");
        let (second, second_trace) = crate::simulate_traced(&model, &config, 0).expect("second");
        assert_eq!(
            serde_json::to_string(&first).expect("json"),
            serde_json::to_string(&second).expect("json")
        );
        assert_eq!(first_trace, second_trace);

        let no_points = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "no-dp",
              "run": { "stop_time": 20.0, "seed": 1 },
              "objects": [
                { "name": "S", "kind": "source", "params": {
                    "interarrival": { "distribution": "constant", "value": 1.0 },
                    "max_arrivals": 1 } },
                { "name": "C", "kind": "conveyor", "params": { "speed": 1.0, "length": 10.0 } },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [
                { "from": "S", "to": "C" }, { "from": "C", "to": "K" }
              ]
            }"#,
        )
        .expect("model");
        let a = crate::run_model(&no_points).expect("a");
        let b = crate::run_model(&no_points).expect("b");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
    }

    /// task-181: once_per_item suppresses repeat actions while the item stays
    /// inside the sensing range; false re-evaluates on each update.
    #[test]
    fn decision_point_once_per_item_controls_repeats() {
        for once in [true, false] {
            let model = decision_point_model("item.type == 0", once, 0);
            let config = SimConfig {
                stop_time: 20.0,
                warmup: 0.0,
                seed: 1,
            };
            let mut sim = Simulation::new(&model, config, true).expect("sim");
            let c = sim.by_name["C"];
            sim.objects[c].belt.as_mut().expect("belt").positions = vec![5.0];
            sim.objects[c].belt_items.push(item());
            sim.update_decision_points(c).expect("first");
            sim.update_decision_points(c).expect("second");
            let count = sim
                .trace
                .as_ref()
                .expect("trace")
                .events
                .iter()
                .filter(|event| event.event == "script_log" && event.message == "dp")
                .count();
            assert_eq!(count, if once { 1 } else { 2 }, "once={once}");
        }
    }

    /// task-181: a condition evaluation failure is a SimError naming the point,
    /// item id and model time; it is never silently treated as false.
    #[test]
    fn decision_point_condition_error_is_reported() {
        let model = decision_point_model("1 / 0 > 0", true, 0);
        let error = crate::run_model(&model).expect_err("condition error");
        let text = error.to_string();
        assert!(text.contains("decision_points[0]"), "{text}");
        assert!(text.contains("item 1"), "{text}");
        assert!(text.contains("t="), "{text}");
    }

    /// task-181: a decision point and a Photo Eye at the same position are
    /// independent sources of events/actions.
    #[test]
    fn decision_point_and_photo_eye_coexist() {
        let mut model = decision_point_model("item.type == 0", true, 0);
        if let ObjectParams::Conveyor(params) = &mut model.objects[1].params {
            params.photo_eyes.push(openflex_model::PhotoEye {
                position: 5.0,
                name: "PE".into(),
                range: Some(0.5),
                triggers: openflex_model::PhotoEyeTriggers {
                    on_blocked: vec![openflex_model::Action::Script {
                        source: "log(\"eye\");".into(),
                    }],
                    on_cleared: Vec::new(),
                },
            });
        }
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let logs: Vec<&str> = trace
            .events
            .iter()
            .filter(|event| event.event == "script_log")
            .map(|event| event.message.as_str())
            .collect();
        assert_eq!(logs.iter().filter(|message| **message == "dp").count(), 1);
        assert_eq!(logs.iter().filter(|message| **message == "eye").count(), 1);
    }

    fn explicit_checkpoint_model() -> Model {
        Model::from_json(&explicit_route_model(
            r#"{"kind": "travel", "target": "pickup"},
               {"kind": "load"},
               {"kind": "travel", "target": "requester"},
               {"kind": "unload"},
               {"kind": "travel", "target": "home"}"#,
            "explicit",
        ))
        .expect("model")
    }

    /// task-183: a checkpoint mid-route resumes with the same report as an
    /// uninterrupted run, including the explicit-chain state.
    #[test]
    fn explicit_chain_checkpoint_matches_uninterrupted_run() {
        let model = explicit_checkpoint_model();
        let save = SimConfig {
            stop_time: 5.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, checkpoint) =
            crate::simulate_with_checkpoint(&model, &save, 0, None).expect("checkpoint");
        let resume = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let resumed =
            crate::resume_from_checkpoint(&model, &resume, 0, &checkpoint, 20.0).expect("resume");
        let direct = crate::simulate(&model, &resume, 0).expect("direct");
        assert_eq!(
            serde_json::to_string(&direct).expect("json"),
            serde_json::to_string(&resumed).expect("json")
        );
    }

    /// task-183: a checkpoint taken while waiting for OperatorReleased restores
    /// that wait state (and does not leave early).
    #[test]
    fn explicit_chain_checkpoint_keeps_waiting_for_release() {
        let model = explicit_checkpoint_model();
        let save = SimConfig {
            stop_time: 10.5,
            warmup: 0.0,
            seed: 1,
        };
        let (_, checkpoint) =
            crate::simulate_with_checkpoint(&model, &save, 0, None).expect("checkpoint");
        let mut sim = Simulation::new(&model, save.clone(), false).expect("sim");
        sim.restore(&checkpoint).expect("restore");
        let t = sim.by_name["T"];
        let state = sim.explicit_routes.get(&t).expect("explicit state");
        assert!(state.waiting_release);
        assert!(!state.release_seen);
        let resume = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let resumed =
            crate::resume_from_checkpoint(&model, &resume, 0, &checkpoint, 20.0).expect("resume");
        let direct = crate::simulate(&model, &resume, 0).expect("direct");
        assert_eq!(
            serde_json::to_string(&direct).expect("json"),
            serde_json::to_string(&resumed).expect("json")
        );
    }

    /// task-183: an old checkpoint without explicit_routes restores safely and
    /// explicitly follows the "chain starts from scratch" compatibility policy.
    #[test]
    fn old_checkpoint_without_explicit_chain_starts_from_scratch() {
        let model = explicit_checkpoint_model();
        let save = SimConfig {
            stop_time: 5.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, checkpoint) =
            crate::simulate_with_checkpoint(&model, &save, 0, None).expect("checkpoint");
        let mut value = serde_json::to_value(&checkpoint).expect("json");
        value
            .as_object_mut()
            .expect("object")
            .remove("explicit_routes");
        let old = crate::checkpoint::Checkpoint::from_json(&value.to_string()).expect("old");
        let mut sim = Simulation::new(&model, save, false).expect("sim");
        sim.restore(&old).expect("restore old");
        assert!(
            sim.explicit_routes.is_empty(),
            "old checkpoint must start a fresh chain"
        );
    }

    /// task-183: chain state is keyed by object name in the checkpoint payload,
    /// so a different model cannot silently receive it.
    #[test]
    fn explicit_checkpoint_state_is_keyed_by_object_name() {
        let model = explicit_checkpoint_model();
        let save = SimConfig {
            stop_time: 5.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, checkpoint) =
            crate::simulate_with_checkpoint(&model, &save, 0, None).expect("checkpoint");
        let value = serde_json::to_value(&checkpoint).expect("json");
        assert!(value["explicit_routes"].get("T").is_some());
        assert!(value["explicit_routes"].get("0").is_none());
    }

    fn motor_model(running: bool, resume: bool, stop: f64) -> Model {
        let resume_trigger = if resume {
            r#", "triggers": { "on_message": [
                { "action": "set_motor", "target": "M", "running": true } ] }"#
        } else {
            ""
        };
        let start_message = if resume {
            r#", "model_triggers": { "on_start": [
                { "action": "send_message", "target": "M", "delay": "10" } ] }"#
        } else {
            ""
        };
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "motor",
          "run": {{ "stop_time": {stop}, "seed": 1, "replications": 1 }}{start_message},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "C", "kind": "conveyor", "params": {{
                "speed": 1.0, "length": 20.0, "accumulating": true }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }},
            {{ "name": "M", "kind": "motor", "params": {{
                "running": {running}{resume_trigger} }} }}
          ],
          "connections": [
            {{ "from": "S", "to": "C" }}, {{ "from": "C", "to": "K" }},
            {{ "from": "M", "to": "C" }}
          ]
        }}"#
        ))
        .expect("model")
    }

    /// task-184: a stopped motor freezes the belt and never accepts upstream
    /// items; no output is produced while stopped.
    #[test]
    fn stopped_motor_freezes_conveyor_and_acceptance() {
        let model = motor_model(false, false, 50.0);
        let report = crate::run_model(&model).expect("run");
        assert_eq!(report.completed.mean, 0.0);
        assert_eq!(report.objects["C"].input_count.mean, 0.0);
    }

    /// task-184: restarting a motor resumes the belt; the queued item reaches
    /// the sink without losing its position or being counted twice.
    #[test]
    fn motor_resume_continues_conveyor() {
        let model = motor_model(false, true, 50.0);
        let cfg = SimConfig {
            stop_time: 50.0,
            warmup: 0.0,
            seed: 1,
        };
        let _ = crate::simulate_traced(&model, &cfg, 0).expect("run");
        let report = crate::run_model(&model).expect("run");
        assert_eq!(report.completed.mean, 1.0, "{report:?}");
    }

    /// task-184: a stopped belt does not create a zero-interval event spin.
    #[test]
    fn stopped_motor_keeps_event_count_bounded() {
        let model = motor_model(false, false, 100.0);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        assert!(
            trace.events.len() < 500,
            "event spin: {}",
            trace.events.len()
        );
    }

    /// task-184: Motor connections are validated: only conveyors are valid
    /// targets, and S connections are rejected with the motor name.
    #[test]
    fn motor_connection_validation_rejects_non_conveyors_and_s_links() {
        let base = motor_model(false, false, 10.0);
        let mut model =
            Model::parse_json(&serde_json::to_string(&base).expect("json")).expect("parse");
        // Replace the valid M->C with M->K (sink).
        let sink = model.objects[2].name.clone();
        if let Some(connection) = model
            .connections
            .iter_mut()
            .find(|connection| connection.from == "M")
        {
            connection.to = sink;
        }
        let error = model.validate().expect_err("non-conveyor");
        let text = error.to_string();
        assert!(text.contains("motor 'M'"), "{text}");
        assert!(text.contains("conveyor"), "{text}");

        let mut s_model =
            Model::parse_json(&serde_json::to_string(&base).expect("json")).expect("parse");
        if let Some(connection) = s_model
            .connections
            .iter_mut()
            .find(|connection| connection.from == "M")
        {
            connection.kind = Some(openflex_model::ConnectionKind::S);
        }
        let error = s_model.validate().expect_err("S link");
        assert!(error.to_string().contains("S connection"), "{error}");
    }

    /// task-184: motor runs are deterministic and no-motor models keep their
    /// old report shape.
    #[test]
    fn motor_is_deterministic_and_optional() {
        let model = motor_model(false, true, 50.0);
        let a = crate::run_model(&model).expect("first");
        let b = crate::run_model(&model).expect("second");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
    }

    /// task-184: stopping a motor freezes an item already on the belt; resuming
    /// does not reset its position or re-count it.
    #[test]
    fn motor_stop_freezes_existing_belt_items() {
        let model = motor_model(true, false, 100.0);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("sim");
        let c = sim.by_name["C"];
        let m = sim.by_name["M"];
        sim.objects[c].belt.as_mut().expect("belt").positions = vec![5.0];
        sim.objects[c].belt.as_mut().expect("belt").holds = vec![0.0];
        sim.objects[c].belt_items.push(item());
        sim.set_motor_running(m, false).expect("stop");
        let before = sim.objects[c]
            .belt
            .as_ref()
            .expect("belt")
            .positions
            .clone();
        sim.now = 50.0;
        sim.sync_belt(c);
        assert_eq!(
            sim.objects[c].belt.as_ref().expect("belt").positions,
            before
        );
        assert!(
            !sim.can_accept(c, 0),
            "stopped belt must reject upstream items"
        );
        sim.set_motor_running(m, true).expect("start");
        sim.now = 51.0;
        sim.sync_belt(c);
        assert!(sim.objects[c].belt.as_ref().expect("belt").positions[0] > 5.0);
    }

    /// task-184: models without a Motor keep the same model/report shape (no
    /// motor property/row is introduced by the new enum variant).
    #[test]
    fn models_without_motor_are_unchanged() {
        let model = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "no-motor",
              "run": { "stop_time": 20.0, "seed": 1 },
              "objects": [
                { "name": "S", "kind": "source", "params": {
                    "interarrival": { "distribution": "constant", "value": 1.0 },
                    "max_arrivals": 2 } },
                { "name": "C", "kind": "conveyor", "params": {
                    "speed": 1.0, "length": 10.0 } },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [
                { "from": "S", "to": "C" }, { "from": "C", "to": "K" }
              ]
            }"#,
        )
        .expect("model");
        let a = crate::run_model(&model).expect("run");
        let json = serde_json::to_value(&a).expect("json");
        assert!(json["objects"].get("M").is_none());
        assert!(!json.to_string().contains("Motor"));
        let b = crate::run_model(&model).expect("repeat");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
    }

    /// task-186: the retired max_travelers field is now rejected by the model
    /// (it was declared but never wired; keeping it silently was worse).
    #[test]
    fn removed_max_travelers_is_rejected() {
        let json = r#"{
          "format": "openflex-model", "version": 1, "name": "dead-param",
          "objects": [
            { "name": "N", "kind": "network_node", "params": { "max_travelers": 1 } }
          ]
        }"#;
        let error = Model::from_json(json).expect_err("max_travelers must be rejected");
        assert!(error.to_string().contains("max_travelers"), "{error}");
    }

    /// task-186: a NetworkNode without max_travelers serializes and reports
    /// exactly as before (no new field/row).
    #[test]
    fn network_nodes_without_max_travelers_are_unchanged() {
        let model = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "node",
              "run": { "stop_time": 5.0, "seed": 1 },
              "objects": [
                { "name": "S", "kind": "source", "params": {
                    "interarrival": { "distribution": "constant", "value": 1000.0 },
                    "max_arrivals": 1 } },
                { "name": "N", "kind": "network_node", "pos": [1.0, 2.0], "params": {} },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [{ "from": "S", "to": "K" }]
            }"#,
        )
        .expect("model");
        let value = serde_json::to_value(&model).expect("json");
        assert!(value["objects"][1]["params"].get("max_travelers").is_none());
        let a = crate::run_model(&model).expect("first");
        let b = crate::run_model(&model).expect("second");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
    }

    /// task-186: explicit load/unload durations are observable and exact:
    /// load_time=5 moves the delivery leg 5 after pickup; unload_time=3 puts
    /// OperatorLoaded 3 after arriving at the requester.
    #[test]
    fn explicit_load_and_unload_durations_are_exact() {
        let json = explicit_route_model(
            r#"{"kind": "travel", "target": "pickup"},
               {"kind": "load"},
               {"kind": "travel", "target": "requester"},
               {"kind": "unload"},
               {"kind": "travel", "target": "home"}"#,
            "explicit",
        )
        .replace(
            r#""speed": 1.0, "home": "D""#,
            r#""speed": 1.0, "home": "D",
                "load_time": { "distribution": "constant", "value": 5.0 },
                "unload_time": { "distribution": "constant", "value": 3.0 }"#,
        );
        let model = Model::from_json(&json).expect("model");
        let config = SimConfig {
            stop_time: 40.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let route = |event_name: &str| -> Vec<(f64, f64)> {
            trace
                .object("T")
                .filter(|event| event.event == event_name)
                .map(|event| (event.time, event.data["x"]))
                .collect()
        };
        let pickup_arrive = route("transport_route_arrive")
            .into_iter()
            .find(|(_, x)| *x == 0.0)
            .map(|(time, _)| time)
            .expect("pickup");
        let delivery_depart = route("transport_route_depart")
            .into_iter()
            .find(|(_, x)| *x == 10.0)
            .map(|(time, _)| time)
            .expect("delivery depart");
        let requester_arrive = route("transport_route_arrive")
            .into_iter()
            .find(|(_, x)| *x == 10.0)
            .map(|(time, _)| time)
            .expect("requester arrive");
        let loaded = trace
            .events
            .iter()
            .find(|event| event.event == "operator_loaded")
            .map(|event| event.time)
            .expect("operator loaded");
        assert!(
            (delivery_depart - pickup_arrive - 5.0).abs() < 1e-9,
            "load: {pickup_arrive}->{delivery_depart}"
        );
        assert!(
            (loaded - requester_arrive - 3.0).abs() < 1e-9,
            "unload: {requester_arrive}->{loaded}"
        );
    }

    /// task-186: NetworkNode JSON round-trips without the retired field; the
    /// serialized params stay empty for the old `{ "params": {} }` form.
    #[test]
    fn network_node_json_round_trips_without_the_retired_field() {
        let json = r#"{
          "format": "openflex-model", "version": 1, "name": "node-roundtrip",
          "run": { "stop_time": 1.0, "seed": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "N", "kind": "network_node", "params": {} },
            { "name": "K", "kind": "sink", "params": {} }
          ],
          "connections": [{ "from": "S", "to": "K" }]
        }"#;
        let model = Model::from_json(json).expect("model");
        let value = serde_json::to_value(&model).expect("json");
        assert!(
            !value["objects"][1]["params"]
                .as_object()
                .expect("params")
                .contains_key("max_travelers")
        );
        let text =
            serde_json::to_string(&serde_json::to_value(&model).expect("json")).expect("json");
        assert!(!text.contains("max_travelers"));
    }

    fn set_destination_model(port: u32) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "set-destination",
          "run": {{ "stop_time": 20.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0,
                "triggers": {{ "on_create": [
                    {{ "action": "set_destination", "port": {port} }} ] }} }} }},
            {{ "name": "K1", "kind": "sink", "params": {{}} }},
            {{ "name": "K2", "kind": "sink", "params": {{}} }}
          ],
          "connections": [
            {{ "from": "S", "to": "K1" }}, {{ "from": "S", "to": "K2" }}
          ]
        }}"#
        ))
        .expect("model")
    }

    /// task-187: SetDestination wins over the default first-available strategy.
    #[test]
    fn set_destination_overrides_routing_strategy() {
        let report = crate::run_model(&set_destination_model(1)).expect("run");
        assert_eq!(report.objects["K1"].input_count.mean, 0.0);
        assert_eq!(report.objects["K2"].input_count.mean, 1.0);
    }

    /// task-187: an out-of-range destination is a hard error naming the object
    /// and its output-port count.
    #[test]
    fn set_destination_rejects_out_of_range_port() {
        let error = crate::run_model(&set_destination_model(9)).expect_err("port error");
        let text = error.to_string();
        assert!(text.contains("set_destination"), "{text}");
        assert!(text.contains("'S'"), "{text}");
        assert!(text.contains("2 output ports"), "{text}");
    }

    /// task-187: the override is consumed by the first delivery and does not
    /// leak to the next object's own routing strategy.
    #[test]
    fn set_destination_does_not_leak_downstream() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "set-destination-leak",
          "run": { "stop_time": 20.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {
                "interarrival": { "distribution": "constant", "value": 1000.0 },
                "max_arrivals": 1, "first_arrival": 0.0,
                "triggers": { "on_create": [
                    { "action": "set_destination", "port": 1 } ] } } },
            { "name": "Q", "kind": "queue", "params": {} },
            { "name": "K1", "kind": "sink", "params": {} },
            { "name": "K2", "kind": "sink", "params": {} }
          ],
          "connections": [
            { "from": "S", "to": "K1" }, { "from": "S", "to": "Q" },
            { "from": "Q", "to": "K1" }, { "from": "Q", "to": "K2" }
          ]
        }"#,
        )
        .expect("model");
        let report = crate::run_model(&model).expect("run");
        assert_eq!(report.objects["Q"].input_count.mean, 1.0);
        assert_eq!(report.objects["K1"].input_count.mean, 1.0);
        assert_eq!(
            report.objects["K2"].input_count.mean, 0.0,
            "queued item must not inherit the source's destination override"
        );
    }

    /// task-187: routing with no SetDestination remains deterministic and the
    /// FlowItem default carries no destination field in report JSON.
    #[test]
    fn set_destination_is_optional_and_deterministic() {
        let model = set_destination_model(1);
        let a = crate::run_model(&model).expect("first");
        let b = crate::run_model(&model).expect("second");
        assert_eq!(
            serde_json::to_string(&a).expect("json"),
            serde_json::to_string(&b).expect("json")
        );
        let plain = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "plain-route",
              "run": { "stop_time": 20.0, "seed": 1 },
              "objects": [
                { "name": "S", "kind": "source", "params": {
                    "interarrival": { "distribution": "constant", "value": 1000.0 },
                    "max_arrivals": 1 } },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [{ "from": "S", "to": "K" }]
            }"#,
        )
        .expect("model");
        let plain_report = crate::run_model(&plain).expect("plain");
        assert!(
            !serde_json::to_string(&plain_report)
                .expect("json")
                .contains("destination")
        );
    }

    fn decision_destination_model(points: &str) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "decision-destination",
          "run": {{ "stop_time": 30.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "C", "kind": "conveyor", "params": {{
                "speed": 1.0, "length": 10.0, "item_spacing": 1.0,
                "accumulating": true, "decision_points": [{points}] }} }},
            {{ "name": "K1", "kind": "sink", "params": {{}} }},
            {{ "name": "K2", "kind": "sink", "params": {{}} }}
          ],
          "connections": [
            {{ "from": "S", "to": "C" }},
            {{ "from": "C", "to": "K1" }}, {{ "from": "C", "to": "K2" }}
          ]
        }}"#
        ))
        .expect("decision destination model")
    }

    /// task-187: a decision point using SetDestination sends the item to that
    /// port; a false condition leaves the default routing untouched.
    #[test]
    fn decision_point_set_destination_routes_and_defaults() {
        let matched = decision_destination_model(
            r#"{ "position": 5.0, "range": 0.5, "condition": "item.type == 0",
                 "actions": [{ "action": "set_destination", "port": 1 }] }"#,
        );
        let report = crate::run_model(&matched).expect("matched");
        assert_eq!(report.objects["K1"].input_count.mean, 0.0);
        assert_eq!(report.objects["K2"].input_count.mean, 1.0);

        let unmatched = decision_destination_model(
            r#"{ "position": 5.0, "range": 0.5, "condition": "item.type == 99",
                 "actions": [{ "action": "set_destination", "port": 1 }] }"#,
        );
        let report = crate::run_model(&unmatched).expect("unmatched");
        assert_eq!(report.objects["K1"].input_count.mean, 1.0);
        assert_eq!(report.objects["K2"].input_count.mean, 0.0);
    }

    /// task-187: two decision points on the same object each override the
    /// destination; the last one (in belt order) wins.
    #[test]
    fn later_decision_point_destination_wins() {
        let model = decision_destination_model(
            r#"{ "position": 3.0, "range": 0.5, "condition": "item.type == 0",
                 "actions": [{ "action": "set_destination", "port": 0 }] },
               { "position": 7.0, "range": 0.5, "condition": "item.type == 0",
                 "actions": [{ "action": "set_destination", "port": 1 }] }"#,
        );
        let report = crate::run_model(&model).expect("run");
        assert_eq!(report.objects["K1"].input_count.mean, 0.0);
        assert_eq!(report.objects["K2"].input_count.mean, 1.0);
    }

    /// A congested non-accumulating belt with a decision point must not spin in
    /// tiny `BeltTick`s (the "Zeno" loop): the run has to finish quickly and
    /// every item still reaches a lane.
    #[test]
    fn belt_decision_point_congestion_does_not_zeno() {
        let model = Model::from_json(
            r#"{"format":"openflex-model","version":1,"name":"zeno","run":{"stop_time":150,"seed":1,"replications":1},
             "objects":[
              {"name":"InfeedA","kind":"source","params":{"interarrival":{"distribution":"exponential","mean":6.0},"item_type":0,"first_arrival":0.0}},
              {"name":"InfeedB","kind":"source","params":{"interarrival":{"distribution":"exponential","mean":6.0},"item_type":1,"first_arrival":0.0}},
              {"name":"Sorter","kind":"conveyor","params":{"speed":1.0,"length":12.0,"item_spacing":1.0,
                "decision_points":[{"position":6.0,"range":0.5,"once_per_item":true,"actions":[]}]}},
              {"name":"LaneA","kind":"sink","params":{}},{"name":"LaneB","kind":"sink","params":{}}],
             "connections":[{"from":"InfeedA","to":"Sorter"},{"from":"InfeedB","to":"Sorter"},
               {"from":"Sorter","to":"LaneA"},{"from":"Sorter","to":"LaneB"}]}"#,
        )
        .expect("model");
        let config = SimConfig {
            stop_time: 150.0,
            warmup: 0.0,
            seed: 1,
        };
        let started = std::time::Instant::now();
        // `run_model` derives the replication seed (this is the CLI path that
        // reproduces the loop).
        let report = crate::run_model(&model).expect("run");
        let result = &report.replications[0];
        let elapsed = started.elapsed();
        assert!(elapsed.as_secs() < 10, "the run must not loop: {elapsed:?}");
        assert!(
            result.items_completed > 0,
            "items must still flow through the belt"
        );
        assert_eq!(report.replications.len(), 1);
        let _ = config;

        // Count the events up to the stop time: the run must stay bounded.
        let config =
            SimConfig::from_run_settings(&model.run, crate::engine::derive_seed(model.run.seed, 0));
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let mut events = 0usize;
        while let Some(Reverse(event)) = sim.events.pop() {
            if event.time > 150.0 - 1e-9 {
                break;
            }
            events += 1;
            sim.now = event.time;
            sim.handle(event.kind).expect("handle");
        }
        eprintln!("zeno events={events}");
        assert!(
            events < 50_000,
            "the run must stay bounded: {events} events"
        );
    }

    /// The same congested belt without a decision point must keep flowing (the
    /// fix must not stall belts that never had decision points).
    #[test]
    fn congested_belt_without_decision_points_still_flows() {
        let model = Model::from_json(
            r#"{"format":"openflex-model","version":1,"name":"zeno-plain","run":{"stop_time":150,"seed":1,"replications":1},
             "objects":[
              {"name":"InfeedA","kind":"source","params":{"interarrival":{"distribution":"exponential","mean":6.0},"item_type":0,"first_arrival":0.0}},
              {"name":"InfeedB","kind":"source","params":{"interarrival":{"distribution":"exponential","mean":6.0},"item_type":1,"first_arrival":0.0}},
              {"name":"Sorter","kind":"conveyor","params":{"speed":1.0,"length":12.0,"item_spacing":1.0}},
              {"name":"LaneA","kind":"sink","params":{}},{"name":"LaneB","kind":"sink","params":{}}],
             "connections":[{"from":"InfeedA","to":"Sorter"},{"from":"InfeedB","to":"Sorter"},
               {"from":"Sorter","to":"LaneA"},{"from":"Sorter","to":"LaneB"}]}"#,
        )
        .expect("model");
        let config =
            SimConfig::from_run_settings(&model.run, crate::engine::derive_seed(model.run.seed, 0));
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let mut events = 0usize;
        while let Some(Reverse(event)) = sim.events.pop() {
            if event.time > 150.0 - 1e-9 {
                break;
            }
            events += 1;
            sim.now = event.time;
            sim.handle(event.kind).expect("handle");
        }
        assert!(
            events < 50_000,
            "the plain belt must stay bounded: {events}"
        );
        assert!(events > 0, "the plain belt must keep delivering items");
    }

    fn recycle_model(recycle: &str) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "sink-recycle",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 10.0 }},
                "max_arrivals": 6, "first_arrival": 0.0 }} }},
            {{ "name": "K", "kind": "sink", "params": {{ "recycle": "{recycle}" }} }}
          ],
          "connections": [ {{ "from": "S", "to": "K" }} ]
        }}"#
        ))
        .expect("model")
    }

    /// task-188: the recycle strategy must not change any sink statistic.
    #[test]
    fn sink_recycle_keeps_the_statistics() {
        let none = crate::run_model(&recycle_model("none")).expect("none");
        let entry = crate::run_model(&recycle_model("on_entry")).expect("on_entry");
        let none_sink = &none.replications[0].objects["K"];
        let entry_sink = &entry.replications[0].objects["K"];
        assert_eq!(none_sink.input_count, entry_sink.input_count);
        assert_eq!(none_sink.output_count, entry_sink.output_count);
        assert_eq!(none_sink.avg_stay_time, entry_sink.avg_stay_time);
        assert_eq!(
            none.replications[0].items_completed,
            entry.replications[0].items_completed
        );
    }

    /// task-188: the same seed reproduces the identical report with recycling.
    #[test]
    fn sink_recycle_is_deterministic() {
        let first = crate::run_model(&recycle_model("by_batch")).expect("first");
        let second = crate::run_model(&recycle_model("by_batch")).expect("second");
        assert_eq!(first.replications, second.replications);
    }

    /// task-188: `by_batch` with a zero batch size is rejected by validation.
    #[test]
    fn sink_by_batch_rejects_zero_batch() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "bad-recycle",
          "run": { "stop_time": 10.0, "seed": 1, "replications": 1 },
          "objects": [
            { "name": "S", "kind": "source", "params": {} },
            { "name": "K", "kind": "sink", "params": { "recycle": "by_batch", "recycle_batch": 0 } }
          ],
          "connections": [ { "from": "S", "to": "K" } ]
        }"#,
        )
        .expect_err("by_batch with a zero batch must be rejected");
        let text = model.to_string();
        assert!(
            text.contains("recycle_batch"),
            "the error must mention recycle_batch, got: {text}"
        );
    }

    fn load_trigger_model(on_load: &str, on_unload: &str) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "load-triggers",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S", "kind": "source", "pos": [0, 0], "params": {{
                "interarrival": {{ "distribution": "constant", "value": 100.0 }},
                "first_arrival": 0.0 }} }},
            {{ "name": "P", "kind": "processor", "pos": [0, 0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }},
            {{ "name": "D", "kind": "dispatcher", "pos": [10, 0], "params": {{}} }},
            {{ "name": "T", "kind": "transporter", "pos": [10, 0], "params": {{
                "speed": 10.0, "load_time": {{ "distribution": "constant", "value": 3.0 }},
                "home": "D",
                "triggers": {{ "on_load": [ {on_load} ], "on_unload": [ {on_unload} ] }} }} }}
          ],
          "connections": [
            {{ "from": "S", "to": "P" }}, {{ "from": "P", "to": "K" }},
            {{ "from": "D", "to": "T" }}
          ]
        }}"#
        ))
        .expect("model")
    }

    /// task-188: models without the new triggers keep their JSON untouched.
    #[test]
    fn load_triggers_are_not_serialized_when_empty() {
        let model = load_trigger_model("", "");
        let text = serde_json::to_string(&model).expect("serialize");
        assert!(!text.contains("on_load"), "empty on_load must be omitted");
        assert!(
            !text.contains("on_unload"),
            "empty on_unload must be omitted"
        );
        // And the run is deterministic.
        let first = crate::run_model(&model).expect("first");
        let second = crate::run_model(&model).expect("second");
        assert_eq!(first.replications, second.replications);
    }

    /// task-188: `on_load` fires exactly when the loaded delivery leg starts
    /// (`pickup + dwell + load`, dwell is 0 here).
    /// A `script` action in `on_load` must be precompiled like every other
    /// trigger list: it is collected by a hand-written list, and missing an
    /// entry made the run fail with "not precompiled" even though validation
    /// accepted the model.
    #[test]
    fn on_load_script_is_precompiled() {
        let model = Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "load-script",
              "run": { "stop_time": 60.0, "seed": 1, "replications": 1 },
              "objects": [
                { "name": "S", "kind": "source", "params": {
                    "interarrival": { "distribution": "constant", "value": 20.0 },
                    "max_arrivals": 2, "first_arrival": 0.0 } },
                { "name": "P", "kind": "processor", "params": {
                    "process_time": { "distribution": "constant", "value": 2.0 },
                    "process_operator": "D" } },
                { "name": "T", "kind": "transporter", "params": {
                    "speed": 2.0, "capacity": 1,
                    "triggers": {
                      "on_load": [ { "action": "script", "source": "log(\"loaded\");" } ],
                      "on_unload": [ { "action": "script", "source": "log(\"unloaded\");" } ] } } },
                { "name": "D", "kind": "dispatcher", "params": {} },
                { "name": "K", "kind": "sink", "params": {} }
              ],
              "connections": [
                { "from": "S", "to": "P" }, { "from": "P", "to": "K" },
                { "from": "D", "to": "T" }
              ]
            }"#,
        )
        .expect("model");
        let config = SimConfig::from_run_settings(&model.run, model.run.seed);
        let (_, trace) =
            crate::simulate_traced(&model, &config, 0).expect("the hooks must not fail");
        let logs = super::hook_logs(&trace);
        assert!(
            logs.iter().any(|line| line.contains("loaded")),
            "on_load script ran: {logs:?}"
        );
        assert!(
            logs.iter().any(|line| line.contains("unloaded")),
            "on_unload script ran: {logs:?}"
        );
    }

    #[test]
    fn on_load_fires_at_dwell_plus_load() {
        let model = load_trigger_model(
            r#"{ "action": "set_label", "name": "loaded", "value": "1" }"#,
            "",
        );
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let mut pickup_at = None;
        let mut depart_at = None;
        while let Some(Reverse(event)) = sim.events.pop() {
            match event.kind {
                EventKind::TransportPickup(..) if pickup_at.is_none() => {
                    pickup_at = Some(event.time);
                }
                EventKind::TransportDepart(..) if depart_at.is_none() => {
                    depart_at = Some(event.time);
                }
                _ => {}
            }
            sim.now = event.time;
            sim.handle(event.kind).expect("handle");
            if pickup_at.is_some() && depart_at.is_some() {
                break;
            }
        }
        let pickup = pickup_at.expect("pickup event");
        let depart = depart_at.expect("depart event");
        assert!(
            (depart - (pickup + 3.0)).abs() < 1e-9,
            "on_load must fire at pickup + dwell + load: {pickup} -> {depart}"
        );
    }

    /// task-188: the `on_load` hook mutates the requester's item and the
    /// `on_unload` hook sees that same item.
    #[test]
    fn on_load_label_is_visible_in_the_unload_hook() {
        let model = load_trigger_model(
            r#"{ "action": "set_label", "name": "loaded", "value": "1" }"#,
            r#"{ "action": "increment_label", "name": "unloaded", "by": "1" }"#,
        );
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let processor = sim.by_name["P"];
        let mut seen_load = false;
        let mut seen_unload = false;
        while let Some(Reverse(event)) = sim.events.pop() {
            let is_depart = matches!(event.kind, EventKind::TransportDepart(..));
            let is_loaded = matches!(event.kind, EventKind::OperatorLoaded(..));
            sim.now = event.time;
            sim.handle(event.kind).expect("handle");
            if is_depart {
                let item = sim.objects[processor]
                    .content
                    .first()
                    .expect("the requester still holds the item");
                assert_eq!(item.labels.get("loaded"), Some(&1.0), "on_load label");
                seen_load = true;
            }
            if is_loaded {
                let item = sim.objects[processor]
                    .content
                    .first()
                    .expect("the requester still holds the item");
                assert_eq!(item.labels.get("loaded"), Some(&1.0), "label survived");
                assert_eq!(
                    item.labels.get("unloaded"),
                    Some(&1.0),
                    "on_unload sees the same item"
                );
                seen_unload = true;
                break;
            }
        }
        assert!(seen_load && seen_unload, "both hooks must fire");
    }

    fn add_fluid_trigger(model: &mut Model, object: &str, trigger: SimTrigger, source: &str) {
        let index = model
            .objects
            .iter()
            .position(|candidate| candidate.name == object)
            .expect("object");
        let triggers = match &mut model.objects[index].params {
            ObjectParams::FluidTank(params) => &mut params.triggers,
            ObjectParams::FluidGenerator(params) => &mut params.triggers,
            ObjectParams::FluidSink(params) => &mut params.triggers,
            _ => panic!("not a fluid object: {object}"),
        };
        let action = Action::Script {
            source: source.to_string(),
        };
        match trigger {
            SimTrigger::OnEmpty => triggers.on_empty.push(action),
            SimTrigger::OnFull => triggers.on_full.push(action),
            other => panic!("unsupported trigger in test: {other:?}"),
        }
    }

    /// task-189: `on_full` fires once when the tank reaches capacity and does
    /// not repeat while the level stays there.
    #[test]
    fn fluid_on_full_fires_once() {
        let mut model = Model::from_json(include_str!("../../../examples/objects/fluid-line.ofm"))
            .expect("model");
        add_fluid_trigger(
            &mut model,
            "Tank1",
            SimTrigger::OnFull,
            "set_label(\"full\", 1.0);",
        );
        let config = SimConfig {
            stop_time: 60.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let tank = result
            .fluid
            .as_ref()
            .and_then(|fluid| fluid.objects.iter().find(|object| object.name == "Tank1"))
            .expect("tank");
        eprintln!(
            "tank level_max={} events_with_on={:?}",
            tank.level_max,
            trace
                .events
                .iter()
                .filter(|e| e.event.starts_with("on_"))
                .map(|e| e.event.clone())
                .collect::<Vec<_>>()
        );
        let fired = trace
            .events
            .iter()
            .filter(|event| event.event == "on_full")
            .count();
        assert_eq!(fired, 1, "on_full must fire once, got {fired}");
    }

    /// task-189: `on_empty` fires once on the crossing to zero and not again
    /// while the tank stays empty.
    #[test]
    fn fluid_on_empty_fires_once() {
        let mut model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "drain",
          "run": { "stop_time": 10.0, "warmup": 0.0, "seed": 1, "replications": 1 },
          "fluid_step": 0.1,
          "objects": [
            { "name": "Tank", "kind": "fluid_tank", "params": {
                "capacity": 100.0, "initial_level": 5.0, "max_outflow": 50.0 } },
            { "name": "Sink", "kind": "fluid_sink", "params": {} }
          ],
          "connections": [ { "from": "Tank", "to": "Sink" } ]
        }"#,
        )
        .expect("model");
        add_fluid_trigger(
            &mut model,
            "Tank",
            SimTrigger::OnEmpty,
            "set_label(\"empty\", 1.0);",
        );
        let config = SimConfig {
            stop_time: 10.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(&model, &config, 0).expect("run");
        let fired = trace
            .events
            .iter()
            .filter(|event| event.event == "on_empty")
            .count();
        assert_eq!(fired, 1, "on_empty must fire once, got {fired}");
    }

    /// task-189: models without the new hooks are unchanged bit for bit.
    #[test]
    fn fluid_boundary_hooks_do_not_change_reports() {
        let model = Model::from_json(include_str!("../../../examples/objects/fluid-line.ofm"))
            .expect("model");
        let first = crate::run_model(&model).expect("first");
        let second = crate::run_model(&model).expect("second");
        assert_eq!(first.replications, second.replications);
        let text = serde_json::to_string(&model).expect("serialize");
        assert!(!text.contains("on_empty"), "empty on_empty must be omitted");
        assert!(!text.contains("on_full"), "empty on_full must be omitted");
    }

    fn traffic_model_with_request_hook(source: &str) -> Model {
        let mut model = traffic_model("mutual_exclusion", 1, 1.0);
        let index = model
            .objects
            .iter()
            .position(|object| object.name == "T1")
            .expect("T1");
        if let ObjectParams::Transporter(params) = &mut model.objects[index].params {
            params.triggers.on_entry_request = vec![Action::Script {
                source: source.to_string(),
            }];
        }
        model
    }

    /// task-189: `on_entry_request` can refuse an entry (`admit == 0`); the
    /// request waits and is retried, so the run still completes.
    #[test]
    fn on_entry_request_admit_zero_is_retried_later() {
        let hook = traffic_model_with_request_hook(
            "if time < 2.0 { set_label(\"admit\", 0.0); } else { set_label(\"admit\", 1.0); }",
        );
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (result, _) = crate::simulate_traced(&hook, &config, 0).expect("hook run");
        assert_eq!(
            result.items_completed, 2,
            "a refused entry must be retried, not deadlock"
        );
        // The refusal pushes T1's first operator_loaded later than in the plain
        // model (it retries instead of acquiring at t = 0).
        let plain = traffic_model("mutual_exclusion", 1, 1.0);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, hook_trace) = crate::simulate_traced(&hook, &config, 0).expect("hook trace");
        let (_, plain_trace) = crate::simulate_traced(&plain, &config, 0).expect("plain trace");
        let first_loaded = |trace: &TraceLog| {
            trace
                .object("T1")
                .filter(|event| event.event == "operator_loaded")
                .map(|event| event.time)
                .fold(f64::INFINITY, f64::min)
        };
        assert!(
            first_loaded(&hook_trace) > first_loaded(&plain_trace) + 0.5,
            "a refusal must delay the accept path ({} vs {})",
            first_loaded(&hook_trace),
            first_loaded(&plain_trace)
        );
    }

    /// task-189: a missing `admit` label keeps the normal acquire path, so the
    /// report stays bit-identical to a model without the hook.
    #[test]
    fn on_entry_request_without_admit_keeps_the_default_path() {
        let plain = traffic_model("mutual_exclusion", 1, 1.0);
        let hooked = traffic_model_with_request_hook("set_label(\"other\", 1.0);");
        let first = crate::run_model(&plain).expect("plain");
        let second = crate::run_model(&hooked).expect("hooked");
        assert_eq!(
            first.replications, second.replications,
            "a hook that leaves `admit` unset must not change the run"
        );
    }

    /// task-189: the `on_entry_request` script is precompiled (task-188's gap).
    #[test]
    fn on_entry_request_script_is_precompiled() {
        let model = traffic_model_with_request_hook(
            "if label(\"admit\") > 0.5 { set_label(\"admit\", 1.0); }",
        );
        // Compiling the model precompiles the script; a missing entry would
        // fail at run time with "script source was not precompiled".
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let result = crate::simulate(&model, &config, 0).expect("script hook runs");
        assert_eq!(result.items_completed, 2);
    }

    fn max_tasks_model(max_tasks: u32) -> Model {
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "max-tasks",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S1", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "S2", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 1000.0 }},
                "max_arrivals": 1, "first_arrival": 0.0 }} }},
            {{ "name": "P1", "kind": "processor", "pos": [10, 0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "P2", "kind": "processor", "pos": [10, 0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "process_operator": "D" }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }},
            {{ "name": "D", "kind": "dispatcher", "pos": [0, 0], "params": {{
                "max_tasks": {max_tasks} }} }},
            {{ "name": "T1", "kind": "transporter", "pos": [0, 0], "params": {{ "speed": 1.0, "home": "D" }} }},
            {{ "name": "T2", "kind": "transporter", "pos": [0, 0], "params": {{ "speed": 1.0, "home": "D" }} }}
          ],
          "connections": [
            {{ "from": "S1", "to": "P1" }}, {{ "from": "P1", "to": "K" }},
            {{ "from": "S2", "to": "P2" }}, {{ "from": "P2", "to": "K" }},
            {{ "from": "D", "to": "T1" }}, {{ "from": "D", "to": "T2" }}
          ]
        }}"#
        ))
        .expect("model")
    }

    fn first_operators_loaded(model: &Model) -> Vec<f64> {
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(model, &config, 0).expect("trace run");
        let mut times: Vec<f64> = trace
            .events
            .iter()
            .filter(|event| event.event == "operator_loaded")
            .map(|event| event.time)
            .collect();
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        times
    }

    /// task-190: `max_tasks == 0` (the default) keeps the unlimited behaviour.
    #[test]
    fn max_tasks_zero_is_unlimited() {
        let model = max_tasks_model(0);
        let times = first_operators_loaded(&model);
        assert_eq!(times.len(), 2, "both tasks are served");
        assert!(
            (times[1] - times[0]).abs() < 1e-9,
            "without a cap both operators load at once: {times:?}"
        );
        let first = crate::run_model(&model).expect("first");
        let second = crate::run_model(&model).expect("second");
        assert_eq!(first.replications, second.replications);
    }

    /// task-190: `max_tasks == 1` queues the second request and serves it after
    /// the first task finished (the request is not dropped).
    #[test]
    fn max_tasks_one_delays_the_second_request() {
        let model = max_tasks_model(1);
        let times = first_operators_loaded(&model);
        assert_eq!(times.len(), 2, "both tasks must still be served");
        assert!(
            times[1] > times[0] + 0.5,
            "the second task must wait for the first to finish: {times:?}"
        );
        let report = crate::run_model(&model).expect("run");
        assert_eq!(report.replications[0].items_completed, 2);
    }

    /// task-190: changing only `max_tasks` must not change the statistics of a
    /// model that never reaches the cap.
    #[test]
    fn max_tasks_does_not_change_a_single_task_model() {
        let plain = max_tasks_model(0);
        let mut capped = max_tasks_model(0);
        // Same model, cap large enough to never bind.
        if let ObjectParams::Dispatcher(params) = &mut capped.objects[5].params {
            params.max_tasks = 5;
        }
        let first = crate::run_model(&plain).expect("plain");
        let second = crate::run_model(&capped).expect("capped");
        assert_eq!(first.replications, second.replications);
    }

    fn changeover_model(changeover: f64, alternating: bool) -> Model {
        let (s0_max, s1_max, s1_first) = if alternating {
            (3, 2, 1.0)
        } else {
            (5, 1, 1000.0)
        };
        Model::from_json(&format!(
            r#"{{
          "format": "openflex-model", "version": 1, "name": "changeover",
          "run": {{ "stop_time": 100.0, "seed": 1, "replications": 1 }},
          "objects": [
            {{ "name": "S0", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 2.0 }},
                "max_arrivals": {s0_max}, "first_arrival": 0.0, "item_type": 0 }} }},
            {{ "name": "S1", "kind": "source", "params": {{
                "interarrival": {{ "distribution": "constant", "value": 2.0 }},
                "max_arrivals": {s1_max}, "first_arrival": {s1_first}, "item_type": 1 }} }},
            {{ "name": "P", "kind": "processor", "pos": [0, 0], "params": {{
                "process_time": {{ "distribution": "constant", "value": 1.0 }},
                "changeover_time": {{ "distribution": "constant", "value": {changeover} }} }} }},
            {{ "name": "K", "kind": "sink", "params": {{}} }}
          ],
          "connections": [
            {{ "from": "S0", "to": "P" }}, {{ "from": "S1", "to": "P" }},
            {{ "from": "P", "to": "K" }}
          ]
        }}"#
        ))
        .expect("model")
    }

    fn processor_busy(model: &Model) -> f64 {
        let report = crate::run_model(model).expect("run");
        report.replications[0].objects["P"].state_times.busy
    }

    fn changeover_count(model: &Model) -> usize {
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = crate::simulate_traced(model, &config, 0).expect("trace");
        let count = trace
            .events
            .iter()
            .filter(|event| event.event == "changeover")
            .count();
        eprintln!("changeover events={count} busy={}", processor_busy(model));
        count
    }

    /// task-190: alternating item types insert a changeover between every pair.
    #[test]
    fn changeover_is_inserted_between_different_types() {
        let plain = processor_busy(&changeover_model(0.0, true));
        let swapped = processor_busy(&changeover_model(3.0, true));
        let count = changeover_count(&changeover_model(3.0, true)) as f64;
        assert!(count > 0.0, "alternating types must insert changeovers");
        assert!(
            (swapped - (plain + count * 3.0)).abs() < 1e-9,
            "every changeover of 3 must add 3 to busy ({swapped} vs {plain}, {count} changeovers)"
        );
    }

    /// task-190: consecutive items of the same type never insert a changeover.
    #[test]
    fn changeover_is_not_inserted_for_the_same_type() {
        let plain = processor_busy(&changeover_model(0.0, false));
        let swapped = processor_busy(&changeover_model(3.0, false));
        assert!(
            (swapped - plain).abs() < 1e-9,
            "same-type runs must not change busy ({swapped} vs {plain})"
        );
    }

    /// task-190: changeover time counts as busy, so utilization moves with it.
    #[test]
    fn changeover_enters_the_utilisation() {
        let plain = crate::run_model(&changeover_model(0.0, true)).expect("plain");
        let swapped = crate::run_model(&changeover_model(3.0, true)).expect("swapped");
        let plain_busy = plain.replications[0].objects["P"].state_times.busy;
        let swapped_busy = swapped.replications[0].objects["P"].state_times.busy;
        assert!(
            swapped_busy > plain_busy,
            "changeover time must be counted as busy ({swapped_busy} vs {plain_busy})"
        );
        assert!(
            (swapped_busy - plain_busy) % 3.0 < 1e-9,
            "the busy delta must be a whole number of changeovers"
        );
    }

    /// task-190: changeover_delay in isolation (first item, same type, switch).
    #[test]
    fn changeover_delay_requires_a_type_switch() {
        let model = changeover_model(3.0, true);
        let config = SimConfig {
            stop_time: 100.0,
            warmup: 0.0,
            seed: 1,
        };
        let mut sim = Simulation::new(&model, config, false).expect("simulation");
        let processor = sim.by_name["P"];
        let params = sim.processor_params(processor).expect("processor params");
        // First item: no previous type -> no changeover.
        assert_eq!(sim.changeover_delay(processor, &params, Some(0)), 0.0);
        sim.objects[processor].last_item_type = Some(0);
        // Same type -> no changeover.
        assert_eq!(sim.changeover_delay(processor, &params, Some(0)), 0.0);
        // A different type inserts the configured 3.
        assert_eq!(sim.changeover_delay(processor, &params, Some(1)), 3.0);
    }

    /// task-190: `changeover_time == 0` keeps the report bit-identical.
    #[test]
    fn changeover_zero_keeps_the_report() {
        let model = changeover_model(0.0, true);
        let first = crate::run_model(&model).expect("first");
        let second = crate::run_model(&model).expect("second");
        assert_eq!(first.replications, second.replications);
    }
}
