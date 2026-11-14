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
//! | `transporter`/`dispatcher` | two-leg operator tasks (travel to the home pickup point → load → travel to the requester → unload), euclidean or road-network travel, capacity (B4) |
//! | `rack` | queue storage (B2); `gravity`/`push_back` release LIFO, `drive_in` is FIFO; `channels`/`depth` narrow the usable capacity |
//! | `resource` | counted resource pool with downtime windows |
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
//!   it can serve concurrently (openflex v1 does not carry flow items).
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
//!   and `sink` destroys them. The graph must be acyclic (validated), blocks
//!   have no capacity limit, and `transfer` blocks are not implemented yet
//!   (validation rejects them). Per-block statistics are reported in
//!   `SimReport.process_flow` and token events are written to the trace log
//!   (`token_create`/`token_delay`/`token_assign`/`token_decision`/`token_sink`).
//!   The object-free equivalent of the same graph is shown in
//!   `examples/objects/process-flow-equivalent.ofm`.
//! * An item leaving an object runs that object's `on_exit` trigger before the
//!   receiver's `on_entry`/`on_receive` logic (Processor included); the sink
//!   still runs its own `on_exit` when it destroys the item.
//! * Repeating arrival schedules treat later passes' row times as offsets from
//!   the previous pass's last row (the first row's time only applies once).
//! * A released queue batch is committed at once: the items count as leaving
//!   the queue at the batch release instant, even though a downstream object
//!   may accept them one at a time afterwards.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, VecDeque};

use openflex_model::{
    Action, ArrivalStyle, CombinerMode, ConditionalPort, Distribution, DowntimeWindow, EvalContext,
    FlowBlockKind, Model, NetworkEdge, Object, ObjectKind, ObjectParams, PortStrategy, PullParams,
    QueueOrder, RackType, SeparateMode, SimTrigger, Value,
};
use rand::prelude::*;
use rand_chacha::ChaCha12Rng;
use rand_distr::{Distribution as _, Exp, Gamma, LogNormal, Normal};
use serde::{Deserialize, Serialize};

use crate::checkpoint::{CHECKPOINT_FORMAT_VERSION, Checkpoint};
use crate::conveyor::Belt;
use crate::error::SimError;
use crate::shifts;
use crate::stats::{
    ObjectStats, ReplicationFlow, ReplicationFlowBlock, ReplicationResult, SimConfig, SimReport,
    StateTimes,
};
use crate::trace::{TraceEvent, TraceLog};

/// Safety valve: a model that never drains its events is reported instead of
/// hanging forever.
const MAX_EVENTS: u64 = 50_000_000;

/// Numerical tolerance for time comparisons.
const EPS: f64 = 1e-9;

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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum State {
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
    /// Warm-up boundary: reset the statistics accumulators.
    ResetStats,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum Phase {
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
    /// The ordinal of this connection among the target's incoming connections.
    to_port: u32,
    /// The explicit `port` from the model, if any.
    declared_port: Option<u32>,
    strategy: Option<PortStrategy>,
}

/// One fixed point on a conveyor belt (B3): a photo eye or a station.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct BeltStop {
    position: f64,
    kind: BeltStopKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum BeltStopKind {
    PhotoEye,
    Station(usize),
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
    belt_items: Vec<FlowItem>,
    belt_last_update: f64,
    #[serde(with = "infinite_time")]
    belt_tick_at: f64,
    /// B3: photo eyes and stations, sorted by position.
    belt_stops: Vec<BeltStop>,
    /// B3: per-item flags of the stops already handled, aligned with
    /// `belt_items` (front first).
    belt_handled: Vec<Vec<bool>>,
    /// B3: number of upstream belts merged onto this belt (0/1 = disabled).
    join_inputs: u32,
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
            belt_items: Vec::new(),
            belt_last_update: now,
            belt_tick_at: f64::INFINITY,
            belt_stops: Vec::new(),
            belt_handled: Vec::new(),
            join_inputs: 0,
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
            ObjectParams::Conveyor(params) => {
                runtime.belt = Some(Belt::new(
                    params.length,
                    params.speed,
                    params.item_spacing,
                    params.accumulating,
                ));
                let mut stops: Vec<BeltStop> = Vec::new();
                for photo_eye in &params.photo_eyes {
                    stops.push(BeltStop {
                        position: photo_eye.position,
                        kind: BeltStopKind::PhotoEye,
                    });
                }
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
            }
            ObjectParams::Transporter(params) => runtime.capacity = params.capacity.max(1),
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
            ObjectKind::Conveyor => self.belt_items.len(),
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
        let counted = self
            .failure
            .as_ref()
            .is_some_and(|failure| Self::counts_up_time(failure, self.state, self.phase));
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
    fn counts_up_time(failure: &FailureRt, state: State, phase: Phase) -> bool {
        if failure.count_states.is_empty() {
            return false;
        }
        failure.count_states.iter().any(|name| match name.as_str() {
            "idle" => state == State::Idle,
            "busy" => state == State::Busy,
            "setup" => state == State::Busy && phase == Phase::Setup,
            "processing" => state == State::Busy && phase == Phase::Processing,
            "blocked" => state == State::Blocked,
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
        for lane in &mut self.lanes {
            if lane.busy_since > 0.0 || lane.item.is_some() {
                if lane.item.is_some() && lane.busy_since > 0.0 {
                    self.slot_busy += (now - lane.busy_since).max(0.0);
                }
                lane.busy_since = 0.0;
            }
        }
    }

    fn stats(&self, elapsed: f64) -> ObjectStats {
        let denom = elapsed.max(1e-12);
        let mut state_times = self.acc.states;
        state_times.setup = self.acc.detail.setup;
        state_times.processing = self.acc.detail.processing;
        state_times.waiting_for_operator = self.acc.detail.waiting_operator;
        state_times.waiting_for_transport = self.acc.detail.waiting_transport;
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
    }
    sim.run_events()?;
    sim.advance_to_stop();
    let checkpoint = if save_checkpoint {
        Some(sim.snapshot())
    } else {
        None
    };
    sim.finalize_stats();
    let mut objects = BTreeMap::new();
    let elapsed = config.stop_time - config.warmup;
    for object in &sim.objects {
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
    /// B7: the optional process-flow token runtime.
    process_flow: Option<FlowRuntime>,
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
        for connection in &model.connections {
            let Some(&from) = by_name.get(&connection.from) else {
                continue;
            };
            let Some(&to) = by_name.get(&connection.to) else {
                continue;
            };
            let to_port = input_ports[to];
            input_ports[to] += 1;
            let index = conns.len();
            conns.push(SimConn {
                from,
                to,
                to_port,
                declared_port: connection.port,
                strategy: connection.strategy.clone(),
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
            if let ObjectParams::Transporter(params) = &object.params
                && let Some(home) = &params.home
                && let Some(&target) = by_name.get(home)
            {
                objects[index].home = Some(target);
            }
            for &conn in &outputs[index] {
                let to = conns[conn].to;
                match (object.kind, model.objects[to].kind) {
                    (ObjectKind::Dispatcher, ObjectKind::Transporter) => {
                        transporters[index].push(to);
                        objects[to].assigned_dispatcher = Some(index);
                    }
                    (ObjectKind::Transporter, ObjectKind::Dispatcher) => {
                        transporters[to].push(index);
                        objects[index].assigned_dispatcher = Some(to);
                    }
                    _ => {}
                }
            }
        }

        // B7: resolve the process-flow graph once.
        let process_flow = model.process_flow.as_ref().map(|flow| {
            let index: HashMap<&str, usize> = flow
                .blocks
                .iter()
                .enumerate()
                .map(|(position, block)| (block.id.as_str(), position))
                .collect();
            let mut connections = Vec::new();
            let mut outgoing = vec![Vec::new(); flow.blocks.len()];
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
            FlowRuntime {
                blocks: vec![FlowBlockRt::default(); flow.blocks.len()],
                connections,
                outgoing,
                next_token_id: 0,
                tokens_created: 0,
                tokens_completed: 0,
            }
        });

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
        // B7: create blocks start producing tokens.
        if self.process_flow.is_some() {
            for block in 0..self.process_flow_block_count() {
                if matches!(self.flow_block_kind(block), Some(FlowBlockKind::Create(_))) {
                    self.schedule_flow_create(block)?;
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
            if next_time >= self.config.stop_time - EPS {
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
        }
        Ok(())
    }

    /// Statistics run until the stop time even if no event lands exactly there.
    fn advance_to_stop(&mut self) {
        if self.now < self.config.stop_time {
            self.now = self.config.stop_time;
        }
    }

    fn finalize_stats(&mut self) {
        let now = self.now;
        for object in &mut self.objects {
            object.finalize(now);
        }
        self.finalize_flow();
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
            config: self.config.clone(),
        }
    }

    /// Restore a snapshot produced by [`Simulation::snapshot`].
    ///
    /// The object model must be unchanged (same name, same object list) and
    /// the seed/warm-up must match the saved run; the stop time may be later
    /// than the checkpoint time (that is the point of resuming).
    fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), SimError> {
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
        self.rng = checkpoint.rng.clone();
        self.arrival_rng = checkpoint.arrival_rng.clone();
        self.items_created = checkpoint.items_created;
        self.stats_created = checkpoint.stats_created;
        self.stats_completed = checkpoint.stats_completed;
        self.event_count = checkpoint.event_count;
        if let (Some(saved), Some(current)) = (
            checkpoint.process_flow.as_ref(),
            self.model.process_flow.as_ref(),
        ) && saved.blocks.len() != current.blocks.len()
        {
            return Err(SimError::Config(
                "checkpoint process-flow block count does not match the model".into(),
            ));
        }
        self.process_flow = checkpoint.process_flow.clone();
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
            EventKind::ShiftChange(index) => self.handle_shift_change(index),
            EventKind::TimetableChange(index) => self.handle_timetable_change(index),
            EventKind::FlowCreate(block) => self.handle_flow_create(block),
            EventKind::FlowAdvance(block) => self.handle_flow_advance(block),
            EventKind::FlowDelayDone(block, token) => self.handle_flow_delay_done(block, token),
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
            )
        {
            return;
        }
        let (index, name, detail) = describe_event(kind);
        let object = match index {
            Some(index) => self.objects[index].name.clone(),
            None => "sim".to_string(),
        };
        if let Some(log) = self.trace.as_mut() {
            let mut event = TraceEvent::new(time, seq, name, object);
            if let Some((key, value)) = detail {
                event = event.with(key, value);
            }
            log.push(event);
        }
    }

    fn kind(&self, index: usize) -> ObjectKind {
        self.objects[index].kind
    }

    fn set_state(&mut self, index: usize, state: State) {
        self.objects[index].set_state(self.now, state);
        self.reschedule_failure(index);
    }

    fn set_phase(&mut self, index: usize, phase: Phase) {
        let now = self.now;
        self.objects[index].set_phase(now, phase);
        self.reschedule_failure(index);
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
        if !ObjectRt::counts_up_time(&clock, self.objects[index].state, self.objects[index].phase) {
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
        };
        let actions = self.trigger_actions(source, SimTrigger::OnCreate);
        if !actions.is_empty() {
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
    fn can_accept(&self, index: usize, port: u32) -> bool {
        if !self.objects[index].on_shift {
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
            ObjectParams::Conveyor(params) => object
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
            | ObjectParams::Dispatcher(_)
            | ObjectParams::Resource(_) => false,
            ObjectParams::BasicFr(_) => object.content.is_empty(),
            ObjectParams::BasicTe(params) => (object.effective_content() as u32) < params.capacity,
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
            ObjectKind::Conveyor => self.flow_conveyor(index)?,
            ObjectKind::Combiner => self.flow_combiner(index)?,
            ObjectKind::Separator => self.flow_separator(index)?,
            ObjectKind::Merge => self.flow_merge(index)?,
            ObjectKind::Split => self.flow_split(index)?,
            ObjectKind::Source
            | ObjectKind::Sink
            | ObjectKind::Transporter
            | ObjectKind::Dispatcher
            | ObjectKind::Resource => {}
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
        // otherwise the item would be delivered twice.
        let outputs: Vec<usize> = self.outputs[from]
            .iter()
            .copied()
            .filter(|&conn| !self.pull_enabled(self.conns[conn].to))
            .collect();
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
                    let matched = {
                        let context = ExprContext::new(self, Some(item));
                        condition
                            .condition
                            .eval(&context)
                            .map_err(|err| SimError::Expression {
                                location: format!("{name} routing condition"),
                                detail: err.to_string(),
                            })?
                    };
                    if matched.as_bool() {
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
        let kind = self.objects[target].kind;
        // The item now leaves the upstream object: its OnExit trigger runs
        // before the receiving object's OnEntry/OnReceive logic (A10-on_exit).
        let exit_actions = self.trigger_actions(from, SimTrigger::OnExit);
        if !exit_actions.is_empty() {
            self.apply_actions(from, &mut item, exit_actions)?;
        }
        // Trigger the receiving object's OnEntry/OnReceive logic.
        let actions = self.trigger_actions(target, SimTrigger::OnEntry);
        if !actions.is_empty() {
            self.apply_actions(target, &mut item, actions)?;
        }
        let receive_actions = self.trigger_actions(target, SimTrigger::OnReceive);
        if !receive_actions.is_empty() {
            self.apply_actions(target, &mut item, receive_actions)?;
        }
        item.entry_time = self.now;
        self.objects[target].acc.input += 1;
        let now = self.now;
        match kind {
            ObjectKind::Sink => {
                self.objects[target].acc.output += 1;
                if item.created + EPS >= self.config.warmup {
                    self.stats_completed += 1;
                }
                self.objects[target].acc.stay_sum += now - item.entry_time;
                self.objects[target].acc.stay_count += 1;
                self.trigger_actions_run(target, SimTrigger::OnExit, &mut item)?;
            }
            ObjectKind::Queue | ObjectKind::Rack => {
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
            ObjectKind::Conveyor => {
                self.sync_belt(target);
                self.process_belt_stops(target)?;
                self.objects[target].note_content(now);
                if let Some(belt) = self.objects[target].belt.as_mut() {
                    belt.admit();
                }
                self.objects[target].belt_items.push(item);
                let stop_count = self.objects[target].belt_stops.len();
                self.objects[target]
                    .belt_handled
                    .push(vec![false; stop_count]);
                self.objects[target].belt_last_update = now;
                self.objects[target].touch();
                if self.objects[target].join_inputs >= 2 {
                    let inputs = self.objects[target].join_inputs as usize;
                    self.objects[target].join_cursor = (to_port as usize + 1) % inputs;
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

    /// B3: the input port a join conveyor should serve next.
    ///
    /// The cursor rotates over the input ports; ports whose upstream object has
    /// nothing waiting are skipped. `None` means "no preference" (no upstream
    /// is ready, so any port may be accepted).
    fn join_preferred_port(&self, target: usize) -> Option<u32> {
        let inputs = self.objects[target].join_inputs;
        if inputs < 2 {
            return None;
        }
        let ports = inputs as usize;
        let start = self.objects[target].join_cursor % ports;
        let connections = &self.inputs[target];
        for offset in 0..ports {
            let port = (start + offset) % ports;
            let Some(&conn) = connections.get(port) else {
                continue;
            };
            let from = self.conns[conn].from;
            if self.has_held_item(from) {
                return Some(port as u32);
            }
        }
        None
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
                        self.objects[from].take_content(now, at);
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
                    let matched = {
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
                for at in 0..self.objects[from].content.len() {
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
            let position = self.pick_queue_position(index, &params.order);
            let Some(item) = self.objects[index].take_content(self.now, position) else {
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
        }
    }

    /// Put an item back at the position it was taken from and block.
    fn undo_queue_take(&mut self, index: usize, position: usize, item: FlowItem) {
        let at = position.min(self.objects[index].content.len());
        self.objects[index].note_content(self.now);
        self.objects[index].content.insert(at, item);
        self.set_state(index, State::Blocked);
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
        let setup = self.sample(&params.setup_time).max(0.0);
        let phase = if setup > 0.0 {
            OperatorPhase::Setup
        } else {
            OperatorPhase::Process
        };
        if !self.ensure_operator(index, phase, &params)? {
            return Ok(());
        }
        let Some(item) = self.objects[index].take_content(self.now, 0) else {
            return Ok(());
        };
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
        self.model
            .process_flow
            .as_ref()?
            .blocks
            .get(block)
            .map(|definition| definition.kind.clone())
    }

    fn flow_block_name(&self, block: usize) -> String {
        self.model
            .process_flow
            .as_ref()
            .and_then(|flow| flow.blocks.get(block))
            .map(|definition| definition.id.clone())
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
        let item_type = params.item_type;
        let labels = params.labels.clone();
        for _ in 0..quantity {
            let token_id = {
                let Some(flow) = self.process_flow.as_mut() else {
                    return Ok(());
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
            };
            self.flow_note(block, self.now);
            self.flow_add_token(block, token, None);
            self.record_token_trace("token_create", block, token_id);
        }
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
                FlowBlockKind::Transfer(_) => break,
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
            let context = ExprContext::new(self, Some(token));
            let matched =
                condition
                    .condition
                    .eval(&context)
                    .map_err(|err| SimError::Expression {
                        location: "process flow decision".into(),
                        detail: err.to_string(),
                    })?;
            if matched.as_bool() {
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
        let definitions = self.model.process_flow.as_ref()?;
        let denom = elapsed.max(1e-12);
        let blocks = flow
            .blocks
            .iter()
            .enumerate()
            .map(|(position, state)| {
                let definition = definitions.blocks.get(position);
                ReplicationFlowBlock {
                    id: definition.map(|block| block.id.clone()).unwrap_or_default(),
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
        while let Some(position) = self.pick_request(dispatcher) {
            let requester = self.objects[dispatcher].requests[position];
            let Some(transporter) = self.pick_transporter(dispatcher, requester) else {
                // A10: a preemptive request may interrupt a lower-priority task.
                if self.try_preempt(dispatcher, requester)? {
                    continue;
                }
                break;
            };
            let pickup_travel = self.transport_pickup_travel(transporter);
            self.objects[dispatcher].requests.remove(position);
            self.objects[transporter].tasks += 1;
            self.objects[transporter].serving_requester = Some(requester);
            self.objects[transporter].acc.input += 1;
            // A5: the operator is on its way; the destination counts it as an
            // in-transit item until it arrives.
            self.objects[requester].in_transit += 1;
            self.set_state(transporter, State::Busy);
            // B4: the task has two legs — travel to the pickup point (the
            // transporter's home) and then to the requester.
            self.schedule(
                self.now + pickup_travel,
                EventKind::TransportPickup(transporter, requester),
            );
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
            let Some(low) = self.objects[transporter].serving_requester else {
                continue;
            };
            if low == requester {
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
        self.preempt_operator(low, transporter)?;
        Ok(true)
    }

    /// Take the operator away from `index` and re-queue its request.
    fn preempt_operator(&mut self, index: usize, transporter: usize) -> Result<(), SimError> {
        let removed = self.objects[index].operator_sources.iter().position(
            |source| matches!(source, OperatorSource::Transporter(t) if *t == transporter),
        );
        let Some(position) = removed else {
            return Ok(());
        };
        self.objects[index].operator_sources.remove(position);
        self.objects[index].operator_held = self.objects[index].operator_held.saturating_sub(1);
        // Abort the transporter task (it is not counted as completed).
        self.objects[transporter].serving_requester = None;
        self.objects[transporter].tasks = self.objects[transporter].tasks.saturating_sub(1);
        self.set_state(transporter, State::Idle);
        // Pause the interrupted setup/process step and re-queue the request.
        self.pause_phase(index);
        self.objects[index].operator_preempted = true;
        self.objects[index].requesting_operator = true;
        self.objects[index].operators_pending += 1;
        self.block(index, BlockReason::Transport);
        if let Some(dispatcher) = self.objects[transporter].assigned_dispatcher {
            // `pick_request` always serves the highest priority first, so the
            // queue order itself does not matter.
            self.objects[dispatcher].requests.push_back(index);
        }
        Ok(())
    }

    /// Pick an idle transporter for a request, with the travel time to the
    /// requesting object (euclidean distance / speed).
    fn pick_transporter(&self, dispatcher: usize, requester: usize) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for &transporter in &self.transporters[dispatcher] {
            if !self.objects[transporter].on_shift {
                continue;
            }
            if self.objects[transporter].tasks >= self.objects[transporter].capacity as usize {
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
    fn transport_pickup_travel(&self, transporter: usize) -> f64 {
        let speed = self.transport_speed(transporter);
        match self.objects[transporter].home {
            Some(home) => {
                distance(
                    self.objects[transporter].position,
                    self.objects[home].position,
                ) / speed
            }
            None => 0.0,
        }
    }

    /// B4: first task leg — arrive at the pickup point (the transporter's
    /// `home`), load the operator there and travel to the requester.
    fn handle_transport_pickup(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        if let Some(home) = self.objects[transporter].home {
            self.objects[transporter].position = self.objects[home].position;
        }
        let load_distribution = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params) => params.load_time.clone(),
            _ => Distribution::Constant { value: 0.0 },
        };
        let load = self.sample(&load_distribution).max(0.0);
        let travel = self.transport_leg_travel(transporter, requester);
        self.schedule(
            self.now + load + travel,
            EventKind::TransportArrive(transporter, requester),
        );
        Ok(())
    }

    fn handle_transport_arrive(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        self.objects[transporter].position = self.objects[requester].position;
        let unload_distribution = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params) => params.unload_time.clone(),
            _ => Distribution::Constant { value: 0.0 },
        };
        let unload = self.sample(&unload_distribution).max(0.0);
        self.schedule(
            self.now + unload,
            EventKind::OperatorLoaded(transporter, requester),
        );
        Ok(())
    }

    /// B4: shortest distance for one operational leg of a transport task.
    ///
    /// With a `network` the leg uses the shortest path between the two objects;
    /// without one it falls back to the straight-line distance.
    fn transport_object_distance(&self, transporter: usize, from: usize, to: usize) -> f64 {
        let edges: Vec<NetworkEdge> = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params) => params.network.clone(),
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

    fn transport_speed(&self, transporter: usize) -> f64 {
        match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params) => params.speed.max(EPS),
            _ => 1.0,
        }
    }

    /// Travel time from the pickup point to the requester.
    fn transport_leg_travel(&self, transporter: usize, requester: usize) -> f64 {
        let speed = self.transport_speed(transporter);
        match self.objects[transporter].home {
            Some(home) => self.transport_object_distance(transporter, home, requester) / speed,
            None => {
                distance(
                    self.objects[transporter].position,
                    self.objects[requester].position,
                ) / speed
            }
        }
    }

    /// Full task estimate used to pick the closest idle transporter (B4).
    fn transport_estimate_travel(&self, transporter: usize, requester: usize) -> f64 {
        let speed = self.transport_speed(transporter);
        let pickup = match self.objects[transporter].home {
            Some(home) => {
                distance(
                    self.objects[transporter].position,
                    self.objects[home].position,
                ) / speed
            }
            None => 0.0,
        };
        pickup + self.transport_leg_travel(transporter, requester)
    }

    fn handle_operator_loaded(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
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
        let speed = self.transport_speed(transporter);
        let home_travel = self.objects[transporter]
            .home
            .map(|home| self.transport_object_distance(transporter, requester, home) / speed);
        if let Some(travel) = home_travel
            && travel > EPS
        {
            self.set_state(transporter, State::Busy);
            self.schedule(self.now + travel, EventKind::TransportHome(transporter));
            return Ok(());
        }
        self.finish_task(transporter)
    }

    fn handle_transport_home(&mut self, transporter: usize) -> Result<(), SimError> {
        if let Some(home) = self.objects[transporter].home {
            self.objects[transporter].position = self.objects[home].position;
        }
        self.finish_task(transporter)
    }

    /// Mark one transporter task as complete and look for queued work.
    fn finish_task(&mut self, transporter: usize) -> Result<(), SimError> {
        self.objects[transporter].tasks = self.objects[transporter].tasks.saturating_sub(1);
        self.objects[transporter].serving_requester = None;
        self.objects[transporter].acc.output += 1;
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
            let Some(item) = self.objects[index].take_content(self.now, 0) else {
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
        let dt = self.now - self.objects[index].belt_last_update;
        if dt <= 0.0 {
            return;
        }
        if let Some(belt) = self.objects[index].belt.as_mut() {
            belt.advance(dt);
        }
        self.objects[index].belt_last_update = self.now;
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
                    BeltStopKind::PhotoEye => self.fire_photo_eye(index, item_index)?,
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

    /// Run the conveyor's `on_photo_eye` trigger against one item.
    fn fire_photo_eye(&mut self, index: usize, item_index: usize) -> Result<(), SimError> {
        let actions = self.trigger_actions(index, SimTrigger::OnPhotoEye);
        if actions.is_empty() {
            return Ok(());
        }
        let Some(mut item) = self.objects[index].belt_items.get(item_index).cloned() else {
            return Ok(());
        };
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
    fn schedule_belt_tick(&mut self, index: usize) {
        let stops = self.belt_stop_positions(index);
        let Some(dt) = self.objects[index]
            .belt
            .as_ref()
            .and_then(|belt| belt.time_to_next_event_with_stops(&stops))
        else {
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
        self.objects[index].belt_tick_at = f64::INFINITY;
        self.sync_belt(index);
        self.process_belt_stops(index)?;
        self.flow(index)?;
        self.notify_upstreams(index)
    }

    fn flow_conveyor(&mut self, index: usize) -> Result<(), SimError> {
        self.sync_belt(index);
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
            self.objects[index].touch();
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
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
            let Some(container) = self.objects[index].take_content(now, 0) else {
                return Ok(());
            };
            let outputs = self.outputs[index].len();
            if outputs == 0 {
                self.objects[index].push_content(now, container);
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
            self.objects[index].take_content(now, 0);
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
        }
        self.set_state(index, State::Idle);
        Ok(())
    }

    /// Objects without a dedicated behaviour pass items through, which keeps
    /// `basic_fr` / `basic_te` usable while their logic comes from triggers.
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
            self.objects[index].take_content(now, 0);
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

    fn handle_breakdown(&mut self, index: usize, generation: u64) -> Result<(), SimError> {
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
        for object in &mut self.objects {
            object.slot_busy = 0.0;
            object.acc = Accum::default();
            object.last_state_change = self.now;
            object.last_content_change = self.now;
            object.phase_start = self.now;
        }
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
        EventKind::ResetStats => (None, "reset_stats", None),
    }
}

/// Shortest distance between two named network nodes (B4).
///
/// Edges are bidirectional; the graph is small, so Dijkstra uses a linear scan
/// for the next node instead of a priority queue.
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
        adjacency
            .entry(edge.to.as_str())
            .or_default()
            .push((edge.from.as_str(), edge.distance));
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
    fn lookup(&self, path: &str) -> Option<Value> {
        match path {
            "time" | "sim.time" | "now" => return Some(Value::Number(self.sim.now)),
            "sim.items_created" => {
                return Some(Value::Number(self.sim.stats_created as f64));
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
}
