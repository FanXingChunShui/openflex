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
//! | `source` | first item at `first_arrival` (0 = at time zero), then one inter-arrival time after the previous batch *left* the source |
//! | `queue`/`rack` | immediate release, optional batching with `batch_size` and `batch_max_wait` |
//! | `processor` | setup + process, breakdown/repair with preemption, optional operator (dispatcher or resource pool) |
//! | `conveyor` | item positions, accumulating or rigid transport, spacing and exit blocking |
//! | `combiner` | port 1 container + `components_per_container` components, `pack` or `join` |
//! | `separator` | unpack into `split_count + 1` items (0 = every component inside the container) |
//! | `merge` | one item per input port, optional `wait_for_all` gating |
//! | `split` | round-robin or by-item-type distribution over the output ports |
//! | `transporter`/`dispatcher` | task assignment with euclidean travel time, load/unload delays, capacity |
//! | `resource` | counted resource pool with downtime windows |
//! | any | optional `shifts` plan: off-shift pauses work and accumulates `off_shift` time |
//!
//! # Simplifications (documented, not FlexSim behaviour)
//!
//! * Conveyors transfer an item to the downstream object at the exact instant
//!   it reaches the end of the belt; there is no separate transfer delay.
//! * Task executers travel in a straight line at constant speed; there is no
//!   acceleration, network or collision model.
//! * A transporter's `capacity` is interpreted as the number of operator tasks
//!   it can serve concurrently (openflex v1 does not carry flow items).
//! * Operators stay with a processor through a breakdown instead of being
//!   released; the processor simply pauses.
//! * A released queue batch is committed at once: the items count as leaving
//!   the queue at the batch release instant, even though a downstream object
//!   may accept them one at a time afterwards.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, VecDeque};

use openflex_model::{
    Action, CombinerMode, Distribution, DowntimeWindow, EvalContext, Model, Object, ObjectKind,
    ObjectParams, PortStrategy, QueueOrder, SimTrigger, Value,
};
use rand::prelude::*;
use rand_distr::{Distribution as _, Exp, Gamma, LogNormal, Normal};

use crate::conveyor::Belt;
use crate::error::SimError;
use crate::shifts;
use crate::stats::{ObjectStats, ReplicationResult, SimConfig, SimReport, StateTimes};

/// Safety valve: a model that never drains its events is reported instead of
/// hanging forever.
const MAX_EVENTS: u64 = 50_000_000;

/// Numerical tolerance for time comparisons.
const EPS: f64 = 1e-9;

/// Label used to carry the number of components inside a container item.
pub const COMPONENTS_LABEL: &str = "components";

/// One flow item travelling through the model.
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
enum EventKind {
    /// A source creates its next batch.
    Arrive(usize),
    /// An object tries to push its pending items downstream.
    Flow(usize),
    SetupDone(usize, u64),
    ProcessDone(usize, u64),
    Breakdown(usize),
    RepairDone(usize),
    /// A user message reaches an object.
    Message(usize),
    /// The next conveyor movement boundary is reached.
    BeltTick(usize),
    /// A queue batch waited `batch_max_wait` and is released early.
    BatchTimeout(usize),
    /// A shift window starts or ends.
    ShiftChange(usize),
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
    /// Warm-up boundary: reset the statistics accumulators.
    ResetStats,
}

#[derive(Debug, Clone)]
struct Event {
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

#[derive(Debug, Default, Clone)]
struct Accum {
    input: u64,
    output: u64,
    content_integral: f64,
    stay_sum: f64,
    stay_count: u64,
    max_content: u32,
    states: StateTimes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Setup,
    Processing,
    WaitingForOutput,
}

/// Which phase of a processor an operator is requested for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OperatorPhase {
    Setup,
    Process,
}

/// Where a processor's operator came from; needed to release it correctly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

struct ObjectRt {
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
    last_state_change: f64,
    last_content_change: f64,
    acc: Accum,
    /// Round-robin cursor for routing and for splitters.
    rr: usize,
    split_cursor: usize,
    /// Source bookkeeping.
    arrivals: u32,
    next_arrival_pending: bool,
    /// Processor bookkeeping.
    processing_item: Option<FlowItem>,
    phase_token: u64,
    phase_end: f64,
    paused_remaining: f64,
    ready_for_process: bool,
    /// Operator bookkeeping.
    operator_held: bool,
    requesting_operator: bool,
    operator_source: Option<OperatorSource>,
    /// Conveyor runtime.
    belt: Option<Belt>,
    belt_items: Vec<FlowItem>,
    belt_last_update: f64,
    belt_tick_at: f64,
    /// Queue batching.
    batch_anchor: Option<f64>,
    force_release: bool,
    /// Combiner runtime.
    pack_mode: bool,
    container: Option<FlowItem>,
    components: Vec<FlowItem>,
    join_slots: Vec<Option<FlowItem>>,
    /// Separator runtime.
    pending_parts: Vec<(FlowItem, u32)>,
    /// Merge runtime.
    merge_slots: Vec<Option<FlowItem>>,
    /// Task executer runtime.
    position: [f64; 2],
    capacity: u32,
    tasks: usize,
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
            last_state_change: now,
            last_content_change: now,
            acc: Accum::default(),
            rr: 0,
            split_cursor: 0,
            arrivals: 0,
            next_arrival_pending: false,
            processing_item: None,
            phase_token: 0,
            phase_end: f64::INFINITY,
            paused_remaining: 0.0,
            ready_for_process: false,
            operator_held: false,
            requesting_operator: false,
            operator_source: None,
            belt: None,
            belt_items: Vec::new(),
            belt_last_update: now,
            belt_tick_at: f64::INFINITY,
            batch_anchor: None,
            force_release: false,
            pack_mode: true,
            container: None,
            components: Vec::new(),
            join_slots: Vec::new(),
            pending_parts: Vec::new(),
            merge_slots: Vec::new(),
            position: object.pos,
            capacity: 1,
            tasks: 0,
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
            }
            ObjectParams::Transporter(params) => runtime.capacity = params.capacity.max(1),
            ObjectParams::Combiner(params) => runtime.pack_mode = params.mode == CombinerMode::Pack,
            _ => {}
        }
        runtime
    }

    /// Size the per-port staging buffers once the connections are known.
    fn configure_ports(&mut self, input_count: usize) {
        match self.kind {
            ObjectKind::Merge => self.merge_slots = vec![None; input_count.max(1)],
            ObjectKind::Combiner if !self.pack_mode => {
                self.join_slots = vec![None; input_count.max(1)];
            }
            _ => {}
        }
    }

    /// Items the object currently holds. Container items count once, their
    /// components are inside the container and are not counted separately.
    fn held(&self) -> usize {
        match self.kind {
            ObjectKind::Combiner => {
                if self.pack_mode {
                    usize::from(self.container.is_some())
                } else {
                    self.join_slots.iter().flatten().count()
                }
            }
            ObjectKind::Separator => {
                self.content.len() + usize::from(!self.pending_parts.is_empty())
            }
            ObjectKind::Conveyor => self.belt_items.len(),
            ObjectKind::Merge => self.merge_slots.iter().flatten().count(),
            _ => self.content.len() + usize::from(self.processing_item.is_some()),
        }
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
        self.state = state;
        self.last_state_change = now;
    }

    fn accumulate_state(&mut self, now: f64) {
        let dt = (now - self.last_state_change).max(0.0);
        match self.state {
            State::Idle => self.acc.states.idle += dt,
            State::Busy => self.acc.states.busy += dt,
            State::Blocked => self.acc.states.blocked += dt,
            State::Down => self.acc.states.down += dt,
            State::OffShift => self.acc.states.off_shift += dt,
        }
        self.last_state_change = now;
    }

    fn finalize(&mut self, now: f64) {
        self.note_content(now);
        self.accumulate_state(now);
    }

    fn stats(&self, elapsed: f64) -> ObjectStats {
        let denom = elapsed.max(1e-12);
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
            state_times: self.acc.states,
            utilization: self.acc.states.busy / denom,
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
    model.validate()?;
    if config.stop_time <= 0.0 {
        return Err(SimError::Config("stop_time must be positive".into()));
    }
    let mut sim = Simulation::new(model, config.clone())?;
    sim.run()?;
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
    Ok(ReplicationResult {
        index,
        seed: config.seed,
        final_time: config.stop_time,
        warmup: config.warmup,
        items_in_system,
        items_created: sim.stats_created,
        items_completed,
        objects,
    })
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
    rng: StdRng,
    /// Separate stream for source inter-arrival times: scenarios that only
    /// change processing times then see the same arrival sequence (common
    /// random numbers), which makes `openflex compare` meaningful.
    arrival_rng: StdRng,
    items_created: u64,
    /// Items created since the last warm-up reset (reported).
    stats_created: u64,
    /// Items created after the warm-up that reached a sink (reported). Items
    /// created before the warm-up belong to the transient phase and are not
    /// counted, so `created - completed` equals the in-system items of the
    /// statistical window.
    stats_completed: u64,
    event_count: u64,
}

impl<'m> Simulation<'m> {
    fn new(model: &'m Model, config: SimConfig) -> Result<Self, SimError> {
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
            rng: StdRng::seed_from_u64(seed),
            arrival_rng: StdRng::seed_from_u64(derive_seed(seed ^ 0xA5A5_5A5A_1234_5678, 7)),
            items_created: 0,
            stats_created: 0,
            stats_completed: 0,
            event_count: 0,
        };
        sim.initialize()?;
        Ok(sim)
    }

    fn initialize(&mut self) -> Result<(), SimError> {
        if self.config.warmup > 0.0 {
            self.schedule(self.config.warmup, EventKind::ResetStats);
        }
        // Shift plans first: they decide which objects are available at t = 0.
        for index in 0..self.model.objects.len() {
            let shifts = self.model.objects[index].params.shifts().to_vec();
            if shifts.is_empty() {
                continue;
            }
            if !shifts::is_active(&shifts, 0.0) {
                self.objects[index].on_shift = false;
                self.objects[index].set_state(0.0, State::OffShift);
            }
            if let Some(next) = shifts::next_transition(&shifts, 0.0)
                && next < self.config.stop_time
            {
                self.schedule(next, EventKind::ShiftChange(index));
            }
        }
        for index in 0..self.model.objects.len() {
            let params = self.model.objects[index].params.clone();
            match params {
                ObjectParams::Source(params) => {
                    let first = params.first_arrival.max(0.0);
                    if first < self.config.stop_time {
                        self.schedule(first, EventKind::Arrive(index));
                    }
                }
                ObjectParams::Processor(params) => {
                    if let Some(failure) = &params.failure {
                        let mtbf = self.sample(&failure.mtbf).max(EPS);
                        self.schedule(self.now + mtbf, EventKind::Breakdown(index));
                    }
                }
                _ => {}
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

fn sample_distribution(rng: &mut StdRng, distribution: &Distribution) -> f64 {
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
    fn run(&mut self) -> Result<(), SimError> {
        while let Some(Reverse(event)) = self.events.pop() {
            if event.time > self.config.stop_time {
                break;
            }
            if event.time > self.now {
                self.now = event.time;
            }
            self.event_count += 1;
            if self.event_count > MAX_EVENTS {
                return Err(SimError::EventLimit(MAX_EVENTS));
            }
            self.handle(event.kind)?;
        }
        // Statistics run until the stop time even if no event lands exactly there.
        if self.now < self.config.stop_time {
            self.now = self.config.stop_time;
        }
        let now = self.now;
        for object in &mut self.objects {
            object.finalize(now);
        }
        Ok(())
    }

    fn handle(&mut self, kind: EventKind) -> Result<(), SimError> {
        match kind {
            EventKind::Arrive(index) => self.handle_arrival(index),
            EventKind::Flow(index) => self.flow(index),
            EventKind::SetupDone(index, token) => self.handle_setup_done(index, token),
            EventKind::ProcessDone(index, token) => self.handle_process_done(index, token),
            EventKind::Breakdown(index) => self.handle_breakdown(index),
            EventKind::RepairDone(index) => self.handle_repair(index),
            EventKind::Message(index) => self.handle_message(index),
            EventKind::BeltTick(index) => self.handle_belt_tick(index),
            EventKind::BatchTimeout(index) => self.handle_batch_timeout(index),
            EventKind::ShiftChange(index) => self.handle_shift_change(index),
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
            EventKind::ResetStats => {
                self.reset_stats();
                Ok(())
            }
        }
    }

    fn kind(&self, index: usize) -> ObjectKind {
        self.objects[index].kind
    }

    fn set_state(&mut self, index: usize, state: State) {
        self.objects[index].set_state(self.now, state);
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
        if !self.objects[index].on_shift {
            // No production while off shift; keep the arrival rhythm going.
            self.schedule_next_arrival(index)?;
            return Ok(());
        }
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

    /// Schedule the next arrival one inter-arrival time from now.
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
        let interval = self.sample_arrival(&params.interarrival).max(0.0);
        let next = self.now + interval;
        let horizon = match params.stop_time {
            Some(stop) => stop.min(self.config.stop_time),
            None => self.config.stop_time,
        };
        if next < horizon - EPS {
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
            ObjectParams::Queue(params) => (object.content.len() as u32) < params.max_content,
            ObjectParams::Rack(params) => (object.content.len() as u32) < params.capacity,
            ObjectParams::Processor(params) => (object.held() as u32) < params.max_content,
            ObjectParams::Conveyor(params) => object
                .belt
                .as_ref()
                .is_some_and(|belt| belt.entry_free(params.max_content)),
            ObjectParams::Combiner(params) => {
                if object.pack_mode {
                    if port == 0 {
                        object.container.is_none() && object.components.is_empty()
                    } else {
                        object.container.is_some()
                            && (object.components.len() as u32) < params.components_per_container
                    }
                } else {
                    object
                        .join_slots
                        .get(port as usize)
                        .is_some_and(Option::is_none)
                }
            }
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
            ObjectParams::BasicTe(params) => (object.content.len() as u32) < params.capacity,
        }
    }

    /// Try to push everything an object can currently send.
    fn flow(&mut self, index: usize) -> Result<(), SimError> {
        let kind = self.objects[index].kind;
        if !self.objects[index].on_shift {
            return Ok(());
        }
        if !self.drain_out_pending(index)? {
            return Ok(());
        }
        match kind {
            ObjectKind::Queue | ObjectKind::Rack => self.flow_queue(index)?,
            ObjectKind::Processor => self.flow_processor(index)?,
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
        Ok(())
    }

    /// Deliver `out_pending` items. Returns `false` when the object blocked.
    fn drain_out_pending(&mut self, index: usize) -> Result<bool, SimError> {
        while let Some(item) = self.objects[index].out_pending.first().cloned() {
            let Some(conn) = self.route(index, &item)? else {
                self.objects[index].set_state(self.now, State::Blocked);
                self.objects[index].phase = Phase::WaitingForOutput;
                return Ok(false);
            };
            if !self.deliver(index, conn, item.clone())? {
                self.objects[index].set_state(self.now, State::Blocked);
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
        let outputs = self.outputs[from].clone();
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
        };
        Ok(chosen)
    }

    /// Deliver an item over one connection. Returns `false` when the target
    /// rejects it; the caller keeps the item and retries later.
    fn deliver(&mut self, _from: usize, conn: usize, item: FlowItem) -> Result<bool, SimError> {
        let (target, to_port) = {
            let connection = &self.conns[conn];
            (connection.to, connection.to_port)
        };
        if !self.can_accept(target, to_port) {
            return Ok(false);
        }
        let mut item = item;
        let kind = self.objects[target].kind;
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
                self.objects[target].set_state(now, State::Busy);
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
                self.objects[target].note_content(now);
                if let Some(belt) = self.objects[target].belt.as_mut() {
                    belt.admit();
                }
                self.objects[target].belt_items.push(item);
                self.objects[target].belt_last_update = now;
                self.objects[target].touch();
                self.objects[target].set_state(now, State::Busy);
                self.schedule(now, EventKind::Flow(target));
            }
            ObjectKind::Combiner => {
                self.objects[target].note_content(now);
                if self.objects[target].pack_mode {
                    if to_port == 0 {
                        self.objects[target].container = Some(item);
                    } else {
                        self.objects[target].components.push(item);
                    }
                } else if let Some(slot) = self.objects[target].join_slots.get_mut(to_port as usize)
                {
                    *slot = Some(item);
                }
                self.objects[target].touch();
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
                self.objects[target].set_state(now, State::Busy);
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
        Ok(())
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
                self.objects[index].set_state(self.now, State::Idle);
                return Ok(());
            }
            if let Some(batch_size) = params.batch_size {
                let enough = self.objects[index].content.len() >= batch_size as usize;
                if !enough && !self.objects[index].force_release {
                    self.objects[index].set_state(self.now, State::Blocked);
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
        self.objects[index].set_state(self.now, State::Blocked);
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
        if due < self.config.stop_time - EPS {
            self.schedule(due, EventKind::BatchTimeout(index));
        }
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
            self.objects[index].phase = Phase::Processing;
            self.objects[index].phase_token += 1;
            let token = self.objects[index].phase_token;
            self.objects[index].phase_end = self.now + process;
            self.objects[index].set_state(self.now, State::Busy);
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
            self.objects[index].set_state(self.now, State::Idle);
            self.objects[index].phase = Phase::Idle;
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
        self.objects[index].set_state(self.now, State::Busy);
        self.objects[index].phase_token += 1;
        let token = self.objects[index].phase_token;
        if setup > 0.0 {
            self.objects[index].phase = Phase::Setup;
            self.objects[index].phase_end = self.now + setup;
            self.schedule(self.now + setup, EventKind::SetupDone(index, token));
        } else {
            let process = self.sample(&params.process_time).max(0.0);
            self.objects[index].phase = Phase::Processing;
            self.objects[index].phase_end = self.now + process;
            self.schedule(self.now + process, EventKind::ProcessDone(index, token));
        }
        Ok(())
    }

    fn handle_setup_done(&mut self, index: usize, token: u64) -> Result<(), SimError> {
        if self.objects[index].phase_token != token {
            return Ok(());
        }
        let Some(params) = self.processor_params(index) else {
            return Ok(());
        };
        let same_operator =
            params.setup_operator.is_some() && params.setup_operator == params.process_operator;
        if !same_operator {
            self.release_operator(index)?;
        }
        self.objects[index].phase = Phase::Idle;
        self.objects[index].ready_for_process = true;
        self.flow_processor(index)
    }

    fn handle_process_done(&mut self, index: usize, token: u64) -> Result<(), SimError> {
        if self.objects[index].phase_token != token {
            return Ok(());
        }
        self.objects[index].note_content(self.now);
        let Some(item) = self.objects[index].processing_item.take() else {
            self.objects[index].phase = Phase::Idle;
            self.objects[index].set_state(self.now, State::Idle);
            return Ok(());
        };
        self.objects[index].out_pending.push(item);
        self.objects[index].touch();
        self.objects[index].phase = Phase::WaitingForOutput;
        self.release_operator(index)?;
        // `flow` delivers the finished item and, when there is another item
        // waiting, starts the next setup/process step.
        self.flow(index)?;
        if self.objects[index].out_pending.is_empty()
            && self.objects[index].content.is_empty()
            && !matches!(self.objects[index].phase, Phase::Setup | Phase::Processing)
        {
            self.objects[index].phase = Phase::Idle;
            self.objects[index].set_state(self.now, State::Idle);
        }
        // A finished item frees a slot: upstream objects may now flow.
        self.notify_upstreams(index)?;
        Ok(())
    }

    /// Pause the current setup/process step, remembering the remaining time.
    fn pause_phase(&mut self, index: usize) {
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
        self.objects[index].set_state(self.now, State::Busy);
        self.schedule(self.now + remaining, event);
        Ok(true)
    }

    // -----------------------------------------------------------------------
    // Operators, dispatchers, transporters and resource pools
    // -----------------------------------------------------------------------

    /// Make sure the operator required for `phase` is held. Returns `false`
    /// when the processor has to wait (its state is then `Blocked`).
    fn ensure_operator(
        &mut self,
        index: usize,
        phase: OperatorPhase,
        params: &openflex_model::ProcessorParams,
    ) -> Result<bool, SimError> {
        if self.objects[index].operator_held {
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
        if self.objects[index].requesting_operator {
            return Ok(false);
        }
        self.objects[index].requesting_operator = true;
        self.set_state(index, State::Blocked);
        match self.objects[target].kind {
            ObjectKind::Dispatcher => {
                self.objects[target].requests.push_back(index);
                self.dispatch(target)?;
            }
            ObjectKind::Resource => {
                self.resource_request(target, index)?;
            }
            _ => {
                // Not a task executer or pool: treat as always available.
                self.objects[index].requesting_operator = false;
                return Ok(true);
            }
        }
        Ok(self.objects[index].operator_held)
    }

    /// Hand an operator to a waiting processor and let it continue.
    fn grant_operator(&mut self, index: usize, source: OperatorSource) {
        self.objects[index].operator_held = true;
        self.objects[index].requesting_operator = false;
        self.objects[index].operator_source = Some(source);
        self.schedule(self.now, EventKind::Flow(index));
    }

    /// Release the operator currently held by a processor.
    fn release_operator(&mut self, index: usize) -> Result<(), SimError> {
        if !self.objects[index].operator_held {
            return Ok(());
        }
        self.objects[index].operator_held = false;
        let source = self.objects[index].operator_source.take();
        match source {
            Some(OperatorSource::Resource(resource)) => {
                if self.objects[resource].resource_in_use > 0 {
                    self.objects[resource].resource_in_use -= 1;
                }
                if self.objects[resource].resource_in_use == 0 {
                    self.set_state(resource, State::Idle);
                }
                self.grant_waiting_resource(resource)?;
            }
            Some(OperatorSource::Transporter(transporter)) => {
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
            None => {}
        }
        Ok(())
    }

    /// Assign queued requests to idle transporters of a dispatcher.
    fn dispatch(&mut self, dispatcher: usize) -> Result<(), SimError> {
        if !self.objects[dispatcher].on_shift {
            return Ok(());
        }
        while let Some(&requester) = self.objects[dispatcher].requests.front() {
            if !self.objects[requester].requesting_operator {
                self.objects[dispatcher].requests.pop_front();
                continue;
            }
            let Some((transporter, travel)) = self.pick_transporter(dispatcher, requester) else {
                break;
            };
            self.objects[dispatcher].requests.pop_front();
            self.objects[transporter].tasks += 1;
            self.objects[transporter].acc.input += 1;
            self.set_state(transporter, State::Busy);
            self.schedule(
                self.now + travel,
                EventKind::TransportArrive(transporter, requester),
            );
        }
        let busy = !self.objects[dispatcher].requests.is_empty();
        self.set_state(dispatcher, if busy { State::Blocked } else { State::Idle });
        Ok(())
    }

    /// Pick an idle transporter for a request, with the travel time to the
    /// requesting object (euclidean distance / speed).
    fn pick_transporter(&self, dispatcher: usize, requester: usize) -> Option<(usize, f64)> {
        let target = self.objects[requester].position;
        let mut best: Option<(usize, f64)> = None;
        for &transporter in &self.transporters[dispatcher] {
            if !self.objects[transporter].on_shift {
                continue;
            }
            if self.objects[transporter].tasks >= self.objects[transporter].capacity as usize {
                continue;
            }
            let speed = match &self.model.objects[transporter].params {
                ObjectParams::Transporter(params) => params.speed.max(EPS),
                _ => 1.0,
            };
            let travel = distance(self.objects[transporter].position, target) / speed;
            if best.is_none_or(|(_, current)| travel < current) {
                best = Some((transporter, travel));
            }
        }
        best
    }

    fn handle_transport_arrive(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
        self.objects[transporter].position = self.objects[requester].position;
        let load_distribution = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params) => params.load_time.clone(),
            _ => Distribution::Constant { value: 0.0 },
        };
        let load = self.sample(&load_distribution).max(0.0);
        self.schedule(
            self.now + load,
            EventKind::OperatorLoaded(transporter, requester),
        );
        Ok(())
    }

    fn handle_operator_loaded(
        &mut self,
        transporter: usize,
        requester: usize,
    ) -> Result<(), SimError> {
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
        _requester: usize,
    ) -> Result<(), SimError> {
        let speed = match &self.model.objects[transporter].params {
            ObjectParams::Transporter(params) => params.speed.max(EPS),
            _ => 1.0,
        };
        let home_travel = self.objects[transporter].home.map(|home| {
            distance(
                self.objects[transporter].position,
                self.objects[home].position,
            ) / speed
        });
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
        if at < self.config.stop_time - EPS {
            self.objects[resource].retry_pending = true;
            self.schedule(at, EventKind::ResourceRetry(resource));
        }
    }

    fn handle_resource_retry(&mut self, resource: usize) -> Result<(), SimError> {
        self.objects[resource].retry_pending = false;
        self.grant_waiting_resource(resource)
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

    /// Schedule the next movement boundary, if the belt is moving.
    fn schedule_belt_tick(&mut self, index: usize) {
        let Some(dt) = self.objects[index]
            .belt
            .as_ref()
            .and_then(|belt| belt.time_to_next_event())
        else {
            return;
        };
        if dt <= EPS {
            return;
        }
        let at = self.now + dt;
        if at >= self.config.stop_time - EPS {
            return;
        }
        if at + EPS < self.objects[index].belt_tick_at {
            self.objects[index].belt_tick_at = at;
            self.schedule(at, EventKind::BeltTick(index));
        }
    }

    fn handle_belt_tick(&mut self, index: usize) -> Result<(), SimError> {
        self.objects[index].belt_tick_at = f64::INFINITY;
        self.sync_belt(index);
        self.flow(index)?;
        self.notify_upstreams(index)
    }

    fn flow_conveyor(&mut self, index: usize) -> Result<(), SimError> {
        self.sync_belt(index);
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
                self.objects[index].set_state(self.now, State::Blocked);
                self.schedule_belt_tick(index);
                return Ok(());
            };
            if !self.deliver(index, conn, item.clone())? {
                self.objects[index].set_state(self.now, State::Blocked);
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
            self.objects[index].touch();
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
            self.notify_upstreams(index)?;
        }
        if self.objects[index].belt_items.is_empty() {
            self.objects[index].set_state(self.now, State::Idle);
            self.objects[index].belt_tick_at = f64::INFINITY;
        } else {
            self.objects[index].set_state(self.now, State::Busy);
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
        if self.objects[index].pack_mode {
            if self.objects[index].container.is_none() {
                self.objects[index].set_state(self.now, State::Idle);
                return Ok(());
            }
            if (self.objects[index].components.len() as u32) < params.components_per_container {
                // Waiting for the remaining components.
                self.objects[index].set_state(self.now, State::Blocked);
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
            self.objects[index].set_state(now, State::Idle);
            Ok(())
        } else {
            let all = self.objects[index].join_slots.iter().all(Option::is_some);
            let any = self.objects[index].join_slots.iter().any(Option::is_some);
            if !all {
                self.objects[index]
                    .set_state(self.now, if any { State::Blocked } else { State::Idle });
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
            self.objects[index].set_state(now, State::Idle);
            Ok(())
        }
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
                self.objects[index].set_state(now, State::Blocked);
                return Ok(());
            }
            let components = container
                .labels
                .get(COMPONENTS_LABEL)
                .copied()
                .unwrap_or(0.0)
                .max(0.0) as usize;
            // unpack semantics: the container is released as well, so `qty`
            // parts plus the container itself leave the separator.
            let parts = if params.split_count > 0 {
                params.split_count as usize + 1
            } else {
                components + 1
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
                self.objects[index].set_state(now, State::Blocked);
                return Ok(());
            };
            if !self.deliver(index, conn, item.clone())? {
                self.objects[index].set_state(now, State::Blocked);
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
            self.objects[index].set_state(now, State::Idle);
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
            self.objects[index].set_state(
                self.now,
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
        self.objects[index].set_state(now, State::Idle);
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
                self.objects[index].set_state(self.now, State::Blocked);
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
        self.objects[index].set_state(self.now, State::Idle);
        Ok(())
    }

    /// Objects without a dedicated behaviour pass items through, which keeps
    /// `basic_fr` / `basic_te` usable while their logic comes from triggers.
    fn flow_generic(&mut self, index: usize) -> Result<(), SimError> {
        while let Some(item) = self.objects[index].content.first().cloned() {
            let Some(conn) = self.route(index, &item)? else {
                self.objects[index].set_state(self.now, State::Blocked);
                return Ok(());
            };
            if !self.deliver(index, conn, item.clone())? {
                self.objects[index].set_state(self.now, State::Blocked);
                return Ok(());
            }
            let now = self.now;
            self.objects[index].take_content(now, 0);
            self.objects[index].acc.output += 1;
            self.objects[index].acc.stay_sum += now - item.entry_time;
            self.objects[index].acc.stay_count += 1;
        }
        self.objects[index].set_state(self.now, State::Idle);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Failures
    // -----------------------------------------------------------------------

    fn handle_breakdown(&mut self, index: usize) -> Result<(), SimError> {
        let Some(params) = self.processor_params(index) else {
            return Ok(());
        };
        let Some(failure) = params.failure.clone() else {
            return Ok(());
        };
        if self.objects[index].state == State::Down {
            return Ok(());
        }
        self.fire_trigger(index, SimTrigger::OnBreakdown)?;
        self.pause_phase(index);
        self.set_state(index, State::Down);
        let repair = self.sample(&failure.mttr).max(0.0);
        self.schedule(self.now + repair, EventKind::RepairDone(index));
        Ok(())
    }

    fn handle_repair(&mut self, index: usize) -> Result<(), SimError> {
        let Some(params) = self.processor_params(index) else {
            return Ok(());
        };
        self.fire_trigger(index, SimTrigger::OnRepair)?;
        if self.objects[index].on_shift {
            if !self.resume_phase(index)? {
                self.set_state(index, State::Idle);
            }
        } else {
            self.set_state(index, State::OffShift);
        }
        if let Some(failure) = &params.failure {
            let mtbf = self.sample(&failure.mtbf).max(EPS);
            self.schedule(self.now + mtbf, EventKind::Breakdown(index));
        }
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
        let active = shifts::is_active(&shifts, self.now);
        self.set_availability(index, active)?;
        if let Some(next) = shifts::next_transition(&shifts, self.now)
            && next < self.config.stop_time - EPS
        {
            self.schedule(next, EventKind::ShiftChange(index));
        }
        Ok(())
    }

    fn set_availability(&mut self, index: usize, active: bool) -> Result<(), SimError> {
        if self.objects[index].on_shift == active {
            return Ok(());
        }
        self.objects[index].on_shift = active;
        if !active {
            self.pause_phase(index);
            if self.objects[index].state != State::Down {
                self.set_state(index, State::OffShift);
            }
            return Ok(());
        }
        // Back on shift.
        if self.objects[index].state == State::Down {
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
        for object in &mut self.objects {
            object.acc = Accum::default();
            object.last_state_change = self.now;
            object.last_content_change = self.now;
        }
    }
}

/// Euclidean distance between two object positions.
fn distance(from: [f64; 2], to: [f64; 2]) -> f64 {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    (dx * dx + dy * dy).sqrt()
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

/// Queue parameters for both queues and racks.
fn queue_params_of(object: &Object) -> openflex_model::QueueParams {
    match &object.params {
        ObjectParams::Queue(params) => params.clone(),
        ObjectParams::Rack(params) => openflex_model::QueueParams {
            max_content: params.capacity,
            ..Default::default()
        },
        _ => openflex_model::QueueParams::default(),
    }
}
